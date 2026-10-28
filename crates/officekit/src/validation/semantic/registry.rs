//! The checks of the semantic constraints.
//!
//! They run on each element the schema walk visits. A run (`Run`) keeps what
//! the checks read across the parts of one validation: the package, the
//! target, the document's application, and the trees of the parts index and
//! reference constraints point into, each loaded once per run with what those
//! constraints read from it. A part's checks (`PartChecks`) see each element
//! through the walk's `Observer`, and keep the values each unique-value
//! constraint meets until the walk of the part ends, when a value held twice
//! is reported once, on the element SDK 3.5.1 reports: its traversal goes
//! through siblings last to first, so it meets first the element the walk
//! shows last.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;

use officekit_xml::element::{Document, Element};
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, OfficeVersion};
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelatedPart, Target};
use regex::Regex;

use super::constraint::{Application, AttributeName, Constraint, ElementName, Registration, Slot};
use crate::validation::error::{ErrorCode, Errors, Step, ValidationError};
use crate::validation::generated::{PART_RULES, SCHEMA_TYPES, SEMANTIC_CONSTRAINTS};
use crate::validation::package::{DATA_PART_REFERENCES, HYPERLINK};
use crate::validation::schema::simple_types::has_value;
use crate::validation::schema::types::{
    AttributeConstraint, ChildKind, PartRule, SchemaType, TypeId, ValueClass,
};
use crate::validation::schema::walk::{Observer, traverse};
use crate::validation::validator::{document_rule, transitional_type};

/// The elements counted in a part, by folded part name and the parent they need.
type CountKey = (String, Option<ElementName>);

/// A part path from a part, by the folded name of the part it starts from.
type PathKey = (String, &'static str);

/// The values an attribute takes on the elements of a name in a part, by
/// folded part name.
type ValueKey = (String, ElementName, AttributeName);

/// Counts the elements a traversal yields whose parent is `parent`, or all
/// of them.
struct Counter {
    count: usize,
    parent: Option<ElementName>,
}

/// Gathers the non-empty values of an attribute on the elements of one name.
struct Gatherer {
    attribute: AttributeName,
    element: ElementName,
    target: OfficeVersion,
    values: BTreeSet<String>,
}

/// One element a unique-value constraint met, with its value.
struct Occurrence {
    /// The attribute's name, for the message.
    attribute: String,
    /// The value as the constraint compares it.
    key: String,
    path: Vec<Step>,
    /// Its place in the order the walk shows elements.
    seen: usize,
    value: String,
}

/// The semantic checks of one part, fed by the schema walk.
pub struct PartChecks<'checks, 'package> {
    errors: Vec<ValidationError>,
    part: &'checks PartName,
    /// The part's relationships, read on first use.
    relationships: Option<Vec<RelatedPart>>,
    rule: &'static PartRule,
    run: &'checks mut Run<'package>,
    /// How many elements the walk has shown.
    seen: usize,
    /// What each unique-value constraint met, by type and the registration's
    /// index among the type's.
    unique: BTreeMap<(TypeId, usize), Vec<Occurrence>>,
}

/// An error a check found: its code and the message's arguments.
struct Problem {
    arguments: Vec<(&'static str, String)>,
    code: ErrorCode,
}

/// What the semantic checks read across the parts of one validation.
pub struct Run<'package> {
    application: Option<Application>,
    counts: BTreeMap<CountKey, usize>,
    /// The rule of the package's own document type.
    document: Option<&'static PartRule>,
    package: &'package Package,
    /// The parts validated, in the order the relationships reach them.
    parts: &'package [(PartName, &'static PartRule)],
    /// The part each part path names from a part, by the folded name of the
    /// part it starts from.
    referenced: BTreeMap<PathKey, Option<(PartName, &'static PartRule)>>,
    /// Each referenced part's tree, by folded name; `None` when it does not load.
    trees: BTreeMap<String, Option<Document>>,
    values: BTreeMap<ValueKey, BTreeSet<String>>,
    version: OfficeVersion,
}

/// An element's attributes as its schema type declares them.
struct Slots<'element> {
    element: &'element Element,
    schema_type: &'static SchemaType,
    target: OfficeVersion,
}

impl Observer for Counter {
    fn element(
        &mut self,
        _element: &Element,
        _type_id: Option<TypeId>,
        parent: Option<&Element>,
        _path: &[Step],
    ) {
        let counted = self
            .parent
            .is_none_or(|wanted| parent.is_some_and(|holder| named(holder, wanted)));
        if counted {
            self.count += 1;
        }
    }
}

impl Observer for Gatherer {
    fn element(
        &mut self,
        element: &Element,
        type_id: Option<TypeId>,
        _parent: Option<&Element>,
        _path: &[Step],
    ) {
        let Some(schema_type) = type_id
            .filter(|_| named(element, self.element))
            .and_then(|found| SCHEMA_TYPES.get(usize::from(found)))
        else {
            return;
        };
        let slots = Slots {
            element,
            schema_type,
            target: self.target,
        };
        if let Some((_, text)) = slots.value(find_slot(schema_type, self.attribute))
            && !text.is_empty()
        {
            self.values.insert(text.to_owned());
        }
    }
}

impl Observer for PartChecks<'_, '_> {
    fn element(
        &mut self,
        element: &Element,
        type_id: Option<TypeId>,
        _parent: Option<&Element>,
        path: &[Step],
    ) {
        self.seen += 1;
        if let Some(found) = type_id {
            self.check(element, found, path);
        }
    }
}

impl<'checks, 'package> PartChecks<'checks, 'package> {
    /// Whether a registration applies at the run's target and to its document.
    fn applies(&self, registration: &Registration) -> bool {
        registration.versions.contains(self.run.version)
            && self
                .run
                .application
                .is_some_and(|application| registration.applications.contains(application))
    }

    /// Every constraint of the element's type that applies, in SDK 3.5.1's order.
    fn check(&mut self, element: &Element, type_id: TypeId, path: &[Step]) {
        let Some(schema_type) = SCHEMA_TYPES.get(usize::from(type_id)) else {
            return;
        };
        let slots = Slots {
            element,
            schema_type,
            target: self.run.version,
        };
        for (index, registration) in registrations(type_id).iter().enumerate() {
            if !self.applies(registration) {
                continue;
            }
            let found = self.problem(&slots, registration.constraint, (type_id, index), path);
            if let Some(problem) = found {
                self.errors.push(problem.into_error(self.part, path));
            }
        }
    }

    /// Keep what the walk found in `errors`: each check's errors, then one
    /// `DuplicateAttributeValue` per value two elements of a type hold.
    pub fn finish(self, errors: &mut Errors) {
        for error in self.errors {
            errors.push(error);
        }
        for occurrences in self.unique.values() {
            let mut by_value: BTreeMap<&str, (usize, &Occurrence)> = BTreeMap::new();
            for occurrence in occurrences {
                let entry = by_value
                    .entry(occurrence.key.as_str())
                    .or_insert((0, occurrence));
                entry.0 += 1;
                if occurrence.seen > entry.1.seen {
                    entry.1 = occurrence;
                }
            }
            for (count, occurrence) in by_value.values() {
                if *count > 1 && !occurrence.value.is_empty() {
                    errors.push(ValidationError::new(
                        ErrorCode::DuplicateAttributeValue,
                        Some(self.part.clone()),
                        Some(occurrence.path.clone()),
                        &[
                            ("attribute", occurrence.attribute.as_str()),
                            ("value", occurrence.value.as_str()),
                        ],
                    ));
                }
            }
        }
    }

    /// `IndexedElementNotFound` when the attribute, a whole number, is not
    /// below the count of elements in the referenced part plus the index base.
    fn index_reference(
        &mut self,
        slots: &Slots<'_>,
        attribute: Option<Slot>,
        element: ElementName,
        parent: Option<ElementName>,
        (part, index_base): (&'static str, u8),
    ) -> Option<Problem> {
        let (declared, text) = slots.value(attribute)?;
        if text.is_empty() {
            return None;
        }
        let index = whole_number(text)?;
        let referenced = self.run.referenced(self.part, self.rule, part);
        let count = referenced
            .as_ref()
            .map_or(0, |(target, rule)| self.run.count(target, rule, parent));
        let limit = i64::try_from(count)
            .unwrap_or(i64::MAX)
            .saturating_add(i64::from(index_base));
        if i64::from(index) < limit {
            return None;
        }
        Some(Problem {
            arguments: vec![
                ("attribute", attribute_name(declared)),
                ("element", element_name(element)),
                ("index", text.to_owned()),
                ("part", part_shown(referenced, part)),
            ],
            code: ErrorCode::IndexedElementNotFound,
        })
    }

    /// The checks of the part `part`, whose part rule is `rule`, reading across parts through `run`.
    #[must_use]
    pub const fn new(
        run: &'checks mut Run<'package>,
        part: &'checks PartName,
        rule: &'static PartRule,
    ) -> Self {
        Self {
            errors: Vec::new(),
            part,
            relationships: None,
            rule,
            run,
            seen: 0,
            unique: BTreeMap::new(),
        }
    }

    /// The error one constraint finds on the element, if any; a unique-value
    /// constraint keeps the value for `finish` and finds none here.
    fn problem(
        &mut self,
        slots: &Slots<'_>,
        constraint: Constraint,
        key: (TypeId, usize),
        path: &[Step],
    ) -> Option<Problem> {
        match constraint {
            Constraint::AbsentUnlessValue {
                attribute,
                other,
                values,
            } => absent_unless_value(slots, attribute, other, values),
            Constraint::AbsentWhenValue {
                attribute,
                other,
                values,
            } => absent_when_value(slots, attribute, other, values),
            Constraint::CannotBeOmitted { attribute } => cannot_be_omitted(slots, attribute),
            Constraint::IndexReference {
                attribute,
                element: name,
                index_base,
                parent,
                part,
            } => self.index_reference(slots, attribute, name, parent, (part, index_base)),
            Constraint::LessOrEqual {
                attribute,
                can_equal,
                other,
            } => less_or_equal(slots, attribute, other, can_equal),
            Constraint::MutuallyExclusive { attributes } => mutually_exclusive(slots, attributes),
            Constraint::ReferenceExists {
                attribute,
                element: name,
                element_attribute,
                part,
            } => self.reference_exists(slots, attribute, (name, element_attribute), part),
            Constraint::RelationshipExists { attribute } => {
                self.relationship_exists(slots, attribute)
            }
            Constraint::RelationshipType {
                attribute,
                relationship_type,
            } => self.relationship_type(slots, attribute, relationship_type),
            Constraint::RequiredWhenValue {
                attribute,
                other,
                value,
            } => required_when_value(slots, attribute, other, value),
            Constraint::UniqueValue {
                attribute,
                case_sensitive,
                parent,
            } => {
                if parent.is_none() {
                    self.record(key, slots, attribute, case_sensitive, path);
                }
                None
            }
            Constraint::ValueConditioned {
                attribute,
                other,
                other_values,
                values,
            } => value_conditioned(slots, (attribute, values), (other, other_values)),
            Constraint::ValueLength {
                attribute,
                max,
                min,
            } => value_length(slots, attribute, min, max),
            Constraint::ValuePattern { attribute, pattern } => {
                value_pattern(slots, attribute, pattern)
            }
            Constraint::ValueRange {
                attribute,
                max,
                max_inclusive,
                min,
                min_inclusive,
            } => value_range(slots, attribute, (min, min_inclusive), (max, max_inclusive)),
            Constraint::ValueSet {
                attribute,
                valid,
                values,
            } => value_set(slots, attribute, valid, values),
        }
    }

    /// Keep the value a unique-value constraint with no parent scope meets;
    /// SDK 3.5.1 checks none with a parent scope.
    fn record(
        &mut self,
        key: (TypeId, usize),
        slots: &Slots<'_>,
        attribute: Option<Slot>,
        case_sensitive: bool,
        path: &[Step],
    ) {
        let Some((declared, text)) = slots.value(attribute) else {
            return;
        };
        self.unique.entry(key).or_default().push(Occurrence {
            attribute: attribute_name(declared),
            key: if case_sensitive {
                text.to_owned()
            } else {
                folded(text)
            },
            path: path.to_vec(),
            seen: self.seen,
            value: text.to_owned(),
        });
    }

    /// `ReferencedElementNotFound` when no element of the name in the
    /// referenced part holds the attribute's value.
    fn reference_exists(
        &mut self,
        slots: &Slots<'_>,
        attribute: Option<Slot>,
        (element, element_attribute): (ElementName, AttributeName),
        part: &'static str,
    ) -> Option<Problem> {
        let (declared, text) = slots.value(attribute)?;
        if text.is_empty() {
            return None;
        }
        let referenced = self.run.referenced(self.part, self.rule, part);
        let held = referenced.as_ref().is_some_and(|(target, rule)| {
            self.run
                .holds(target, rule, element, element_attribute, text)
        });
        if held {
            return None;
        }
        Some(Problem {
            arguments: vec![
                ("attribute", attribute_name(declared)),
                ("element", element_name(element)),
                ("part", part_shown(referenced, part)),
                ("value", text.to_owned()),
            ],
            code: ErrorCode::ReferencedElementNotFound,
        })
    }

    /// `RelationshipNotFound` when the part has no relationship of the id.
    fn relationship_exists(
        &mut self,
        slots: &Slots<'_>,
        attribute: Option<Slot>,
    ) -> Option<Problem> {
        let (declared, id) = slots.value(attribute)?;
        if id.is_empty() || self.relationships().iter().any(|related| related.id == id) {
            return None;
        }
        Some(Problem {
            arguments: vec![
                ("attribute", attribute_name(declared)),
                ("id", id.to_owned()),
            ],
            code: ErrorCode::RelationshipNotFound,
        })
    }

    /// `WrongRelationshipType` when the id names an external relationship or
    /// a part of another type. An id that names neither (a hyperlink, a data
    /// part reference, a part the archive lacks, or nothing) makes SDK 3.5.1
    /// throw, and gets no error here.
    fn relationship_type(
        &mut self,
        slots: &Slots<'_>,
        attribute: Option<Slot>,
        relationship_type: &str,
    ) -> Option<Problem> {
        let (declared, id) = slots.value(attribute)?;
        if id.is_empty() {
            return None;
        }
        let package = self.run.package;
        let related = self
            .relationships()
            .iter()
            .find(|related| related.id == id)?
            .clone();
        let type_uri = transitional_type(&related.type_uri).into_owned();
        if type_uri == HYPERLINK || type_uri == relationship_type {
            return None;
        }
        if let Target::Part(part) = &related.target
            && (DATA_PART_REFERENCES
                .iter()
                .any(|(_, data_type)| *data_type == type_uri)
                || !package.contains(part))
        {
            return None;
        }
        Some(Problem {
            arguments: vec![
                ("attribute", attribute_name(declared)),
                ("id", id.to_owned()),
                ("type", relationship_type.to_owned()),
            ],
            code: ErrorCode::WrongRelationshipType,
        })
    }

    /// The part's relationships, read once.
    fn relationships(&mut self) -> &[RelatedPart] {
        let package = self.run.package;
        let part = self.part;
        self.relationships.get_or_insert_with(|| {
            package
                .relationships_of(Some(part))
                .into_iter()
                .filter_map(Result::ok)
                .collect()
        })
    }
}

impl Problem {
    fn into_error(self, part: &PartName, path: &[Step]) -> ValidationError {
        let filled: Vec<(&str, &str)> = self
            .arguments
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        ValidationError::new(self.code, Some(part.clone()), Some(path.to_vec()), &filled)
    }
}

impl<'package> Run<'package> {
    /// How many elements SDK 3.5.1's traversal yields in `part` whose parent
    /// is `parent`, or in all, counted once per run.
    fn count(
        &mut self,
        part: &PartName,
        rule: &'static PartRule,
        parent: Option<ElementName>,
    ) -> usize {
        let key = (part.folded(), parent);
        if let Some(found) = self.counts.get(&key) {
            return *found;
        }
        let mut counter = Counter { count: 0, parent };
        self.walk_part(part, rule, &mut counter);
        self.counts.insert(key, counter.count);
        counter.count
    }

    /// Whether an element named `element` in `part` holds `value` in its
    /// `attribute`; each part's values are gathered once per run.
    fn holds(
        &mut self,
        part: &PartName,
        rule: &'static PartRule,
        element: ElementName,
        attribute: AttributeName,
        value: &str,
    ) -> bool {
        let key = (part.folded(), element, attribute);
        if !self.values.contains_key(&key) {
            let mut gatherer = Gatherer {
                attribute,
                element,
                target: self.version,
                values: BTreeSet::new(),
            };
            self.walk_part(part, rule, &mut gatherer);
            self.values.insert(key.clone(), gatherer.values);
        }
        self.values
            .get(&key)
            .is_some_and(|values| values.contains(value))
    }

    /// The checks' state for one validation of `package` at `version`;
    /// `parts` are the parts it validates, in the order it reaches them.
    #[must_use]
    pub fn new(
        package: &'package Package,
        version: OfficeVersion,
        parts: &'package [(PartName, &'static PartRule)],
    ) -> Self {
        let document = document_rule(package);
        let application = document.and_then(|rule| match rule.name {
            "PresentationDocument" => Some(Application::PowerPoint),
            "SpreadsheetDocument" => Some(Application::Excel),
            "WordprocessingDocument" => Some(Application::Word),
            _ => None,
        });
        Self {
            application,
            counts: BTreeMap::new(),
            document,
            package,
            parts,
            referenced: BTreeMap::new(),
            trees: BTreeMap::new(),
            values: BTreeMap::new(),
            version,
        }
    }

    /// The part a constraint's path names from `current`, resolved once per
    /// run: `.` is `current`, `..` the first part validated that relates to
    /// it, `/A/B` the chain of part types from the package, and `A/B` the
    /// chain from `current`.
    fn referenced(
        &mut self,
        current: &PartName,
        rule: &'static PartRule,
        path: &'static str,
    ) -> Option<(PartName, &'static PartRule)> {
        let key = (current.folded(), path);
        if let Some(found) = self.referenced.get(&key) {
            return found.clone();
        }
        let found = match path {
            "." => Some((current.clone(), rule)),
            ".." => self
                .parts
                .iter()
                .find(|(candidate, _)| relates(self.package, candidate, current))
                .cloned(),
            _ => match path.strip_prefix('/') {
                Some(absolute) => self
                    .document
                    .and_then(|document| descend(self.package, None, document, absolute)),
                None => descend(self.package, Some(current), rule, path),
            },
        };
        self.referenced.insert(key, found.clone());
        found
    }

    /// Walk `part`'s tree, loaded once per run, for `observer`.
    fn walk_part(&mut self, part: &PartName, rule: &'static PartRule, observer: &mut dyn Observer) {
        let Some(root_type) = rule.root else {
            return;
        };
        let package = self.package;
        let version = self.version;
        let tree = self
            .trees
            .entry(part.folded())
            .or_insert_with(|| package.load_tree(part, OpenOptions::default()).ok());
        if let Some(document) = tree {
            traverse(&document.root, root_type, version, part, observer);
        }
    }
}

impl<'element> Slots<'element> {
    fn declared(&self, slot: Option<Slot>) -> Option<&'static AttributeConstraint> {
        self.schema_type.attributes.get(usize::from(slot?))
    }

    /// The attribute declared in `slot`, and the element's value of it when
    /// present.
    fn value(&self, slot: Option<Slot>) -> Option<(&'static AttributeConstraint, &'element str)> {
        let declared = self.declared(slot)?;
        let namespace = declared.namespace.map_or(Namespace::None, Namespace::Known);
        Some((declared, self.element.attribute(&namespace, declared.name)?))
    }
}

/// `AttributeMustBeAbsentUnlessValue` when both attributes are present and
/// the other holds none of the values.
fn absent_unless_value(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    other: Option<Slot>,
    values: &[&str],
) -> Option<Problem> {
    let (declared, _) = slots.value(attribute)?;
    let (other_declared, other_text) = slots.value(other)?;
    if values
        .iter()
        .any(|value| equals(&other_declared.class, other_text, value))
    {
        return None;
    }
    Some(Problem {
        arguments: vec![
            ("attribute", attribute_name(declared)),
            ("other", attribute_name(other_declared)),
            ("value", values.join(" or ")),
        ],
        code: ErrorCode::AttributeMustBeAbsentUnlessValue,
    })
}

/// `AttributeMustBeAbsentForValue` when both attributes are present and the
/// other holds one of the values.
fn absent_when_value(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    other: Option<Slot>,
    values: &[&str],
) -> Option<Problem> {
    let (declared, _) = slots.value(attribute)?;
    let (other_declared, other_text) = slots.value(other)?;
    if !values
        .iter()
        .any(|value| equals(&other_declared.class, other_text, value))
    {
        return None;
    }
    Some(Problem {
        arguments: vec![
            ("attribute", attribute_name(declared)),
            ("other", attribute_name(other_declared)),
            ("value", other_text.to_owned()),
        ],
        code: ErrorCode::AttributeMustBeAbsentForValue,
    })
}

/// A declared attribute's name with the table's prefix, `w:id`.
fn attribute_name(declared: &AttributeConstraint) -> String {
    declared
        .namespace
        .and_then(|id| NAMESPACES.get(usize::from(id)))
        .map_or_else(
            || declared.name.to_owned(),
            |row| format!("{}:{}", row.prefix, declared.name),
        )
}

/// A boolean class's value of `text`; `None` when the text is not one.
fn boolean(class: &ValueClass, text: &str) -> Option<bool> {
    let trimmed = text.trim_matches([' ', '\t', '\n', '\r']);
    match (class, trimmed, text) {
        (ValueClass::Boolean, "true" | "1", _)
        | (ValueClass::OnOff, _, "true" | "on" | "1")
        | (ValueClass::TrueFalse | ValueClass::TrueFalseBlank, _, "t" | "true") => Some(true),
        (ValueClass::Boolean, "false" | "0", _)
        | (ValueClass::OnOff, _, "false" | "off" | "0")
        | (ValueClass::TrueFalse, _, "f" | "false")
        | (ValueClass::TrueFalseBlank, _, "f" | "false" | "") => Some(false),
        _ => None,
    }
}

/// `AttributeCannotBeOmitted` when a declared attribute is absent.
fn cannot_be_omitted(slots: &Slots<'_>, attribute: Option<Slot>) -> Option<Problem> {
    let declared = slots.declared(attribute)?;
    if slots.value(attribute).is_some() {
        return None;
    }
    Some(Problem {
        arguments: vec![("attribute", attribute_name(declared))],
        code: ErrorCode::AttributeCannotBeOmitted,
    })
}

/// The part reached from `container` (`None`: the package), typed `rule`,
/// through the part types `path` names one after another, each by the first
/// relationship of its type.
fn descend(
    package: &Package,
    container: Option<&PartName>,
    rule: &'static PartRule,
    path: &str,
) -> Option<(PartName, &'static PartRule)> {
    let mut source = container.cloned();
    let mut source_rule = rule;
    for name in path.split('/') {
        let child_rule = source_rule
            .children
            .iter()
            .filter(|child| child.kind != ChildKind::DataPart && child.name == name)
            .find_map(|child| PART_RULES.get(usize::from(child.part?)))?;
        let relationship_type = child_rule.relationship_type?;
        let found = package
            .relationships_of(source.as_ref())
            .into_iter()
            .filter_map(Result::ok)
            .find_map(|related| {
                if let Target::Part(part) = related.target
                    && transitional_type(&related.type_uri) == relationship_type
                {
                    Some(part)
                } else {
                    None
                }
            })?;
        source = Some(found);
        source_rule = child_rule;
    }
    source.map(|part| (part, source_rule))
}

/// An element's name with the table's prefix, `w:footnote`.
fn element_name(name: ElementName) -> String {
    NAMESPACES.get(usize::from(name.namespace)).map_or_else(
        || name.local.to_owned(),
        |row| format!("{}:{}", row.prefix, name.local),
    )
}

/// Whether the attribute's `text` equals a constraint's `value`, as SDK
/// 3.5.1 compares them: hexadecimal numbers by value, a boolean class's value
/// by its reading or by its text, any other by its text.
fn equals(class: &ValueClass, text: &str, value: &str) -> bool {
    match class {
        ValueClass::HexBinary => hex_bits(text)
            .zip(hex_bits(value))
            .is_some_and(|(left, right)| left == right),
        ValueClass::Boolean
        | ValueClass::OnOff
        | ValueClass::TrueFalse
        | ValueClass::TrueFalseBlank => boolean(class, text).is_some_and(|reading| {
            text == value || parse_boolean(value).is_some_and(|constant| constant == reading)
        }),
        ValueClass::Base64Binary
        | ValueClass::Byte
        | ValueClass::DateTime
        | ValueClass::Decimal
        | ValueClass::Double
        | ValueClass::Enum(_)
        | ValueClass::Int16
        | ValueClass::Int32
        | ValueClass::Int64
        | ValueClass::Integer
        | ValueClass::List(_)
        | ValueClass::SByte
        | ValueClass::Single
        | ValueClass::String
        | ValueClass::UInt16
        | ValueClass::UInt32
        | ValueClass::UInt64 => text == value,
    }
}

/// The slot SDK 3.5.1 finds for `attribute` among a type's attributes: the
/// one of its namespace and local name, else the last of its local name.
fn find_slot(schema_type: &SchemaType, attribute: AttributeName) -> Option<Slot> {
    let mut found = None;
    for (index, declared) in schema_type.attributes.iter().enumerate() {
        if declared.name == attribute.local {
            found = Some(index);
            if declared.namespace == attribute.namespace {
                break;
            }
        }
    }
    Slot::try_from(found?).ok()
}

/// A text folded as .NET's ordinal comparison without case folds it: each
/// character to its single upper-case form, when it has one.
fn folded(text: &str) -> String {
    text.chars()
        .map(|character| {
            let mut upper = character.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => character,
            }
        })
        .collect()
}

/// The 64 bits of a hexadecimal number as .NET's `Convert.ToInt64(text, 16)`
/// reads it: an optional `0x`, then one to sixteen digits.
fn hex_bits(text: &str) -> Option<u64> {
    let digits = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    if digits.is_empty() || digits.len() > 16 {
        return None;
    }
    u64::from_str_radix(digits, 16).ok()
}

/// `AttributeValueExceedsAnother` when both attributes read as numbers and
/// the first is above the other, or equal to it where that is refused.
fn less_or_equal(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    other: Option<Slot>,
    can_equal: bool,
) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    let value = number(&declared.class, text)?;
    let (other_declared, other_text) = slots.value(other)?;
    let limit = number(&other_declared.class, other_text)?;
    if (can_equal && value <= limit) || (!can_equal && value < limit) {
        return None;
    }
    Some(Problem {
        arguments: vec![
            ("attribute", attribute_name(declared)),
            ("limit", other_text.to_owned()),
            ("other", attribute_name(other_declared)),
            ("value", text.to_owned()),
        ],
        code: ErrorCode::AttributeValueExceedsAnother,
    })
}

/// `MutuallyExclusiveAttributes` when two or more of the attributes are present.
fn mutually_exclusive(slots: &Slots<'_>, attributes: &[Slot]) -> Option<Problem> {
    let present: Vec<String> = attributes
        .iter()
        .filter_map(|slot| slots.value(Some(*slot)))
        .map(|(declared, _)| attribute_name(declared))
        .collect();
    if present.len() < 2 {
        return None;
    }
    Some(Problem {
        arguments: vec![("attributes", present.join(", "))],
        code: ErrorCode::MutuallyExclusiveAttributes,
    })
}

/// Whether `element` is named `name`.
fn named(element: &Element, name: ElementName) -> bool {
    element.name.namespace == Namespace::Known(name.namespace) && element.name.local == name.local
}

/// An attribute's value as a number, as SDK 3.5.1 reads one for a range: a
/// hexadecimal class's text as a signed 64-bit number, any other's as digits
/// with at most one decimal point and nothing else.
fn number(class: &ValueClass, text: &str) -> Option<f64> {
    if *class == ValueClass::HexBinary {
        let trimmed = text.trim_matches(is_white);
        let digits = trimmed.trim_start_matches('0');
        if trimmed.is_empty() || digits.len() > 16 {
            return None;
        }
        let bits = if digits.is_empty() {
            0
        } else {
            u64::from_str_radix(digits, 16).ok()?
        };
        // .NET reads the bits as a signed 64-bit number, two's complement.
        let signed = if bits > i64::MAX.unsigned_abs() {
            i128::from(bits) - (1_i128 << 64_u8)
        } else {
            i128::from(bits)
        };
        return signed.to_string().parse().ok();
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits_only = whole
        .bytes()
        .chain(fraction.bytes())
        .all(|byte| byte.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits_only {
        return None;
    }
    text.parse().ok()
}

/// How a message names the part a constraint points into: its name, or the
/// constraint's path when no part is there.
fn part_shown(referenced: Option<(PartName, &'static PartRule)>, path: &str) -> String {
    referenced.map_or_else(|| path.to_owned(), |(part, _)| part.to_string())
}

/// .NET's `bool.TryParse`: `true` or `false` in any case, white space around.
fn parse_boolean(text: &str) -> Option<bool> {
    let trimmed =
        text.trim_matches(|character: char| character.is_whitespace() || character == '\0');
    if trimmed.eq_ignore_ascii_case("true") {
        Some(true)
    } else if trimmed.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
}

/// The semantic constraints registered on an element type, in SDK 3.5.1's order.
#[must_use]
pub fn registrations(type_id: TypeId) -> &'static [Registration] {
    let start = SEMANTIC_CONSTRAINTS.partition_point(|row| row.type_id < type_id);
    let end = SEMANTIC_CONSTRAINTS.partition_point(|row| row.type_id <= type_id);
    SEMANTIC_CONSTRAINTS.get(start..end).unwrap_or_default()
}

/// Whether `source` has a relationship to `target`.
fn relates(package: &Package, source: &PartName, target: &PartName) -> bool {
    package
        .relationships_of(Some(source))
        .into_iter()
        .filter_map(Result::ok)
        .any(|relationship| {
            matches!(&relationship.target, Target::Part(part) if part.folded() == target.folded())
        })
}

/// `AttributeRequiredForValue` when the attribute is absent and the other
/// holds the value.
fn required_when_value(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    other: Option<Slot>,
    value: &str,
) -> Option<Problem> {
    let declared = slots.declared(attribute)?;
    if slots.value(attribute).is_some() {
        return None;
    }
    let (other_declared, other_text) = slots.value(other)?;
    if !equals(&other_declared.class, other_text, value) {
        return None;
    }
    Some(Problem {
        arguments: vec![
            ("attribute", attribute_name(declared)),
            ("other", attribute_name(other_declared)),
            ("value", value.to_owned()),
        ],
        code: ErrorCode::AttributeRequiredForValue,
    })
}

/// `AttributeValueOutsideConstraint` on the attribute and its value.
fn outside(declared: &AttributeConstraint, text: &str, detail: String) -> Problem {
    Problem {
        arguments: vec![
            ("attribute", attribute_name(declared)),
            ("detail", detail),
            ("value", text.to_owned()),
        ],
        code: ErrorCode::AttributeValueOutsideConstraint,
    }
}

/// `AttributeValueConflictsWithAnother` when the attribute holds none of its
/// values while the other holds one of its own.
fn value_conditioned(
    slots: &Slots<'_>,
    (attribute, values): (Option<Slot>, &[&str]),
    (other, other_values): (Option<Slot>, &[&str]),
) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    if values
        .iter()
        .any(|value| equals(&declared.class, text, value))
    {
        return None;
    }
    let (other_declared, other_text) = slots.value(other)?;
    if !other_values
        .iter()
        .any(|value| equals(&other_declared.class, other_text, value))
    {
        return None;
    }
    Some(Problem {
        arguments: vec![
            ("actual", text.to_owned()),
            ("allowed", values.join(" or ")),
            ("attribute", attribute_name(declared)),
            ("other", attribute_name(other_declared)),
            ("value", other_text.to_owned()),
        ],
        code: ErrorCode::AttributeValueConflictsWithAnother,
    })
}

/// A value's length in UTF-16 code units, as .NET counts it, outside the bounds.
fn value_length(slots: &Slots<'_>, attribute: Option<Slot>, min: u32, max: u32) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    let length = text.encode_utf16().count();
    let detail = if length < usize::try_from(min).unwrap_or(usize::MAX) {
        format!("shorter than {min} characters")
    } else if length > usize::try_from(max).unwrap_or(usize::MAX) {
        format!("longer than {max} characters")
    } else {
        return None;
    };
    Some(outside(declared, text, detail))
}

/// A non-empty value that does not match the pattern; as in .NET, the end
/// anchor also matches before a final line feed.
fn value_pattern(slots: &Slots<'_>, attribute: Option<Slot>, pattern: &str) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    if text.is_empty() {
        return None;
    }
    let regex = Regex::new(pattern).ok()?;
    let matched = regex.is_match(text)
        || text
            .strip_suffix('\n')
            .is_some_and(|rest| regex.is_match(rest));
    if matched {
        return None;
    }
    Some(outside(
        declared,
        text,
        format!("does not match the pattern {pattern}"),
    ))
}

/// A non-empty value that reads as its class and as a number outside the bounds.
fn value_range(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    (min, min_inclusive): (f64, bool),
    (max, max_inclusive): (f64, bool),
) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    if text.is_empty() || !has_value(&declared.class, text, slots.target) {
        return None;
    }
    let value = number(&declared.class, text)?;
    let below = if min_inclusive {
        value < min
    } else {
        value <= min
    };
    let above = if max_inclusive {
        value > max
    } else {
        value >= max
    };
    let detail = match (above, below, max_inclusive, min_inclusive) {
        (true, _, true, _) => format!("above the maximum, {max}"),
        (true, _, false, _) => format!("not below {max}"),
        (false, true, _, true) => format!("below the minimum, {min}"),
        (false, true, _, false) => format!("not above {min}"),
        (false, false, _, _) => return None,
    };
    Some(outside(declared, text, detail))
}

/// A non-empty value outside the values allowed, or among those refused.
fn value_set(
    slots: &Slots<'_>,
    attribute: Option<Slot>,
    valid: bool,
    values: &[&str],
) -> Option<Problem> {
    let (declared, text) = slots.value(attribute)?;
    if text.is_empty() {
        return None;
    }
    let listed = values
        .iter()
        .any(|value| equals(&declared.class, text, value));
    if listed == valid {
        return None;
    }
    let detail = if valid {
        format!("not one of {}", values.join(", "))
    } else {
        format!("one of {}, which it cannot take", values.join(", "))
    };
    Some(outside(declared, text, detail))
}

/// .NET's white space around a number: tab to carriage return, and space.
const fn is_white(character: char) -> bool {
    matches!(character, '\t'..='\r' | ' ')
}

/// A whole number as .NET's `int.TryParse` reads it: white space around, an
/// optional sign, digits, within 32 bits.
fn whole_number(text: &str) -> Option<i32> {
    text.trim_matches(is_white).parse().ok()
}
