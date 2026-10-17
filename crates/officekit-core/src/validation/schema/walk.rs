//! One walk over a part's tree that runs the schema checks.
//!
//! The walk sees the part as the file holds it, markup compatibility kept,
//! and checks every element SDK 3.5.1 validates: its markup-compatibility
//! attributes, its attributes, its text, its content model, and, when the
//! file declares Strict, what only Transitional allows.
//!
//! An element's type comes from the element it sits in. An element no
//! vocabulary types is not validated, nor is what it holds unless
//! `mc:ProcessContent` names it. Inside an `mc:AlternateContent` the walk
//! follows the one branch SDK 3.5.1 validates at the target.
//!
//! In a file that declares Strict, an element in a namespace only Transitional
//! has is reported, and what it holds in that namespace is not reported again.
//!
//! An `Observer` sees each element SDK 3.5.1's validating traversal yields,
//! after the element's content, which is how the semantic checks run in the
//! same walk; `traverse` walks a part for an observer alone.

use alloc::borrow::ToOwned as _;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::mc::is_mc;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, NamespaceId, OfficeVersion, QName};

use super::attributes::{self, Failure, qualified, transitional_only_namespace};
use super::particle::{self, Child};
use super::types::{Particle, SchemaType, TypeId, ValueClass};
use crate::packaging::part_name::PartName;
use crate::validation::error::{ErrorCode, Errors, Step, ValidationError};
use crate::validation::generated::SCHEMA_TYPES;

#[cfg(test)]
mod tests;

/// The namespace the `xml` prefix is bound to.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// The markup-compatibility attributes one element puts in force: the
/// namespaces its `mc:Ignorable` names and the elements its
/// `mc:ProcessContent` names (`*` for every element of a namespace).
#[derive(Clone, Debug, Default)]
struct Compatibility {
    ignorable: Vec<Namespace>,
    process_content: Vec<(Namespace, String)>,
}

/// Whose vocabulary types an element's children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Factory {
    /// Inside an `mc:AlternateContent`: its branches.
    AlternateContent,
    /// A typed element: its type's declared children.
    Typed(TypeId),
    /// An element SDK 3.5.1 does not type, whose children it does not type either.
    Unknown,
}

/// What an element is to SDK 3.5.1, by the vocabulary it sits in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    AlternateContent,
    Choice,
    Fallback,
    /// A typed element, and whether only Transitional allows it where it sits.
    Typed(TypeId, bool),
    Unknown,
}

/// What sees each element SDK 3.5.1's validating traversal yields: a typed
/// element that exists at the target, or an `mc:AlternateContent`.
pub trait Observer {
    /// `element` after its content was walked: `type_id` is `None` for an
    /// `mc:AlternateContent`, `parent` is the element holding it in the file
    /// (an `mc:Choice` or `mc:Fallback` inside a block), and `path` its path.
    fn element(
        &mut self,
        element: &Element,
        type_id: Option<TypeId>,
        parent: Option<&Element>,
        path: &[Step],
    );
}

/// The walk's state: where it is, what is in scope, and what it found.
struct Walker<'tree, 'run> {
    /// Every element on the way down, with the branch inside a block.
    ancestors: Vec<&'tree Element>,
    errors: &'run mut Errors,
    /// One frame per element on the way down.
    mc: Vec<Compatibility>,
    observer: &'run mut dyn Observer,
    part: &'run PartName,
    path: Vec<Step>,
    /// The namespaces only Transitional has of the reported elements on the
    /// way down, whose descendants in them are not reported again.
    reported_namespaces: Vec<NamespaceId>,
    /// Whether the schema checks run; without them only the observer sees the part.
    schema: bool,
    /// The namespace declarations of every element on the way down.
    scope: Vec<&'tree [NamespaceDeclaration]>,
    strict: bool,
    target: OfficeVersion,
}

impl<'tree> Walker<'tree, '_> {
    /// The structure of an `mc:AlternateContent`: one or more `mc:Choice`,
    /// then at most one `mc:Fallback`, none of them prefixless attributes or
    /// `xml:lang` and `xml:space`, every `mc:Choice` with a `Requires` whose
    /// prefixes are declared.
    fn alternate_content(&mut self, block: &'tree Element) {
        self.branch_attributes(block, None);
        self.compatibility_checks(block, None);
        let holds_nothing = block.children.iter().all(|node| {
            if let Node::Text(text) = node {
                text.chars()
                    .all(|character| matches!(character, ' ' | '\t' | '\n' | '\r'))
            } else {
                false
            }
        });
        if holds_nothing {
            self.error(ErrorCode::AlternateContentWithoutChoice, &[]);
        }
        let mut seen_choice = false;
        let mut seen_fallback = false;
        let mut counts = Counts::default();
        for node in &block.children {
            let Node::Element(child) = node else {
                continue;
            };
            let index = counts.next(&child.name);
            let kind = classify(child, Factory::AlternateContent);
            match kind {
                Kind::AlternateContent => {
                    self.error(
                        ErrorCode::NestedAlternateContent,
                        &[("child", qualified(&child.name))],
                    );
                }
                Kind::Choice if !seen_fallback => {
                    seen_choice = true;
                    self.branch_checks(block, child, index, true);
                }
                Kind::Fallback if seen_choice && !seen_fallback => {
                    seen_fallback = true;
                    self.branch_checks(block, child, index, false);
                }
                Kind::Fallback if !seen_choice => {
                    self.error(ErrorCode::AlternateContentWithoutChoice, &[]);
                    self.branch_checks(block, child, index, false);
                }
                Kind::Choice | Kind::Fallback | Kind::Typed(..) | Kind::Unknown => {
                    let code = if seen_choice {
                        ErrorCode::UnexpectedChildInAlternateContent
                    } else {
                        ErrorCode::AlternateContentWithoutChoice
                    };
                    self.error(code, &[]);
                }
            }
        }
    }

    /// The branch of an `mc:AlternateContent` SDK 3.5.1 validates at the
    /// target, with its place among the siblings of its name: the first
    /// `mc:Choice` whose every `Requires` namespace was introduced at exactly
    /// the target (SDK 3.5.1 compares for equality), else the first `mc:Fallback`.
    fn branch(&mut self, block: &'tree Element) -> Option<(&'tree Element, u32)> {
        let mut choices = 0;
        let mut fallback = None;
        for node in &block.children {
            let Node::Element(child) = node else {
                continue;
            };
            match classify(child, Factory::AlternateContent) {
                Kind::Choice => {
                    choices += 1;
                    let requires = child
                        .attribute(&Namespace::None, "Requires")
                        .unwrap_or_default();
                    let mut prefixes = requires
                        .split(' ')
                        .filter(|prefix| !prefix.is_empty())
                        .peekable();
                    if prefixes.peek().is_none() {
                        continue;
                    }
                    self.scope.push(&child.namespace_declarations);
                    let understood = prefixes.all(|prefix| {
                        self.declared(prefix).is_some_and(|uri| {
                            introduced_at(&Namespace::from_uri(uri), self.target)
                        })
                    });
                    self.scope.pop();
                    if understood {
                        return Some((child, choices));
                    }
                }
                Kind::Fallback if fallback.is_none() => fallback = Some(child),
                Kind::AlternateContent | Kind::Fallback | Kind::Typed(..) | Kind::Unknown => {}
            }
        }
        fallback.map(|found| (found, 1))
    }

    /// The prefixless attributes and `xml:lang` or `xml:space` on an
    /// `mc:AlternateContent`, `mc:Choice`, or `mc:Fallback`, at its path.
    /// `declared` is the one attribute the element declares, `Requires` on
    /// `mc:Choice`.
    fn branch_attributes(&mut self, element: &Element, declared: Option<&str>) {
        for attribute in &element.attributes {
            if compatibility_attribute(attribute)
                || (attribute.name.namespace == Namespace::None
                    && declared == Some(attribute.name.local.as_str()))
            {
                continue;
            }
            if attribute.name.prefix.is_empty() {
                self.error(
                    ErrorCode::UnprefixedAttributeOnAlternateContent,
                    &[("attribute", qualified(&attribute.name))],
                );
            }
            if is_xml_lang_or_space(attribute) {
                self.error(
                    ErrorCode::XmlAttributeOnCompatibilityElement,
                    &[("element", qualified(&element.name))],
                );
            }
        }
    }

    /// The checks of one `mc:Choice` or `mc:Fallback` of `block`, as SDK
    /// 3.5.1 runs them: its attributes, the block's markup-compatibility
    /// attributes once more, and a choice's `Requires`.
    fn branch_checks(
        &mut self,
        block: &'tree Element,
        branch: &'tree Element,
        index: u32,
        choice: bool,
    ) {
        self.path.push(step(&branch.name, index));
        self.scope.push(&branch.namespace_declarations);
        self.branch_attributes(branch, choice.then_some("Requires"));
        self.scope.pop();
        self.path.pop();
        self.compatibility_checks(block, None);
        if !choice {
            return;
        }
        self.path.push(step(&branch.name, index));
        self.scope.push(&branch.namespace_declarations);
        match branch.attribute(&Namespace::None, "Requires") {
            None => self.error(ErrorCode::ChoiceWithoutRequires, &[]),
            Some(requires) => {
                for prefix in requires.split_whitespace() {
                    if self.declared(prefix).is_none() {
                        self.error(
                            ErrorCode::UndeclaredPrefixInRequires,
                            &[("value", requires.to_owned())],
                        );
                    }
                }
            }
        }
        self.scope.pop();
        self.path.pop();
    }

    /// Walk the element children of `element`, each typed by `factory`.
    fn children(&mut self, element: &'tree Element, factory: Factory) {
        let mut counts = Counts::default();
        for node in &element.children {
            if self.errors.full() {
                return;
            }
            let Node::Element(child) = node else {
                continue;
            };
            let index = counts.next(&child.name);
            self.path.push(step(&child.name, index));
            self.visit(child, classify(child, factory), factory);
            self.path.pop();
        }
    }

    /// The markup-compatibility attributes of `element`, checked: every
    /// `mc:Ignorable` and `mc:MustUnderstand` prefix declared, every name
    /// `mc:PreserveAttributes`, `mc:PreserveElements`, and `mc:ProcessContent`
    /// list in a namespace its own `mc:Ignorable` names, and no `xml:lang` or
    /// `xml:space` beside `mc:ProcessContent` that the type does not declare.
    fn compatibility_checks(&mut self, element: &Element, schema_type: Option<&SchemaType>) {
        let ignorable: Option<Vec<Namespace>> =
            compatibility_value(element, "Ignorable").map(|value| {
                let mut namespaces = Vec::new();
                for prefix in value.split_whitespace() {
                    match self.declared(prefix) {
                        Some(uri) => namespaces.push(Namespace::from_uri(uri)),
                        None => self.error(
                            ErrorCode::UndeclaredPrefixInIgnorable,
                            &[("value", value.to_owned())],
                        ),
                    }
                }
                namespaces
            });
        for (local, code) in [
            ("PreserveAttributes", ErrorCode::InvalidPreserveAttributes),
            ("PreserveElements", ErrorCode::InvalidPreserveElements),
            ("ProcessContent", ErrorCode::InvalidProcessContent),
        ] {
            let Some(value) = compatibility_value(element, local) else {
                continue;
            };
            let valid = ignorable
                .as_ref()
                .is_some_and(|namespaces| self.names_in(value, namespaces));
            if !valid {
                self.error(code, &[("value", value.to_owned())]);
            }
            if local == "ProcessContent" {
                for attribute in &element.attributes {
                    let declared_here = schema_type.is_some_and(|found| {
                        found.attributes.iter().any(|constraint| {
                            attribute.name.local == constraint.name
                                && constraint.namespace.is_some_and(|id| {
                                    attribute.name.namespace == Namespace::Known(id)
                                })
                        })
                    });
                    if is_xml_lang_or_space(attribute) && !declared_here {
                        self.error(ErrorCode::XmlAttributeOnProcessedElement, &[]);
                    }
                }
            }
        }
        if let Some(value) = compatibility_value(element, "MustUnderstand") {
            for prefix in value.split_whitespace() {
                if self.declared(prefix).is_none() {
                    self.error(
                        ErrorCode::UndeclaredPrefixInMustUnderstand,
                        &[("value", value.to_owned())],
                    );
                }
            }
        }
    }

    /// The frame `element`'s own `mc:Ignorable` and `mc:ProcessContent` put
    /// in force; prefixes that name nothing are left out. The element's
    /// declarations are in scope.
    fn compatibility_frame(&self, element: &Element) -> Compatibility {
        let mut frame = Compatibility::default();
        if let Some(value) = compatibility_value(element, "Ignorable") {
            for prefix in value.split(' ').filter(|prefix| !prefix.is_empty()) {
                if let Some(uri) = self.declared(prefix) {
                    frame.ignorable.push(Namespace::from_uri(uri));
                }
            }
        }
        if let Some(value) = compatibility_value(element, "ProcessContent") {
            for item in value.split(' ').filter(|item| !item.is_empty()) {
                let mut parts = item.split(':');
                if let (Some(prefix), Some(local), None) =
                    (parts.next(), parts.next(), parts.next())
                    && let Some(uri) = self.declared(prefix)
                {
                    frame
                        .process_content
                        .push((Namespace::from_uri(uri), local.to_owned()));
                }
            }
        }
        frame
    }

    /// The children the content model of `element` meets: each
    /// `mc:AlternateContent` replaced by its branch, and each untyped element
    /// or one outside the target left out when its namespace is ignorable, or
    /// replaced by its children when `mc:ProcessContent` names it too.
    fn content(&mut self, element: &'tree Element, parent: TypeId) -> Vec<Child<'tree>> {
        let mut found = Vec::new();
        self.content_of(element, Factory::Typed(parent), &mut found);
        found
    }

    fn content_of(
        &mut self,
        holder: &'tree Element,
        factory: Factory,
        found: &mut Vec<Child<'tree>>,
    ) {
        for node in &holder.children {
            let Node::Element(child) = node else {
                continue;
            };
            let kind = classify(child, factory);
            self.scope.push(&child.namespace_declarations);
            match kind {
                Kind::AlternateContent => {
                    if let Some((branch, _)) = self.branch(child) {
                        self.scope.push(&branch.namespace_declarations);
                        self.content_of(branch, factory, found);
                        self.scope.pop();
                    }
                }
                Kind::Typed(type_id, _) if exists_at(type_id, self.target) => {
                    found.push(Child::new(
                        &child.name,
                        &holder.name.namespace,
                        Some(type_id),
                    ));
                }
                Kind::Choice | Kind::Fallback | Kind::Typed(..) | Kind::Unknown => {
                    let frame = self.compatibility_frame(child);
                    self.mc.push(frame);
                    let ignorable = self.ignorable(&child.name.namespace);
                    let processed = ignorable && self.process_content(&child.name);
                    self.mc.pop();
                    let type_id = if let Kind::Typed(type_id, _) = kind {
                        Some(type_id)
                    } else {
                        None
                    };
                    if processed {
                        let inner = type_id.map_or(Factory::Unknown, Factory::Typed);
                        self.content_of(child, inner, found);
                    } else if !ignorable {
                        found.push(Child::new(&child.name, &holder.name.namespace, type_id));
                    } else {
                        // An ignorable element the content model does not see.
                    }
                }
            }
            self.scope.pop();
        }
    }

    /// The URI a prefix is declared for in scope, innermost first; the
    /// default namespace and the bare `xml` prefix are no declaration here.
    fn declared(&self, prefix: &str) -> Option<&'tree str> {
        self.scope
            .iter()
            .rev()
            .flat_map(|frame| frame.iter().rev())
            .find(|declaration| declaration.prefix == prefix && !declaration.uri.is_empty())
            .map(|declaration| declaration.uri.as_str())
    }

    /// An error at the current path.
    fn error(&mut self, code: ErrorCode, arguments: &[(&str, String)]) {
        let filled: Vec<(&str, &str)> = arguments
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        self.errors.push(ValidationError::new(
            code,
            Some(self.part.clone()),
            Some(self.path.clone()),
            &filled,
        ));
    }

    /// Whether `namespace` is ignorable where the walk is.
    fn ignorable(&self, namespace: &Namespace) -> bool {
        *namespace != Namespace::None
            && self
                .mc
                .iter()
                .any(|frame| frame.ignorable.contains(namespace))
    }

    /// Whether every name of a `prefix:local` list is in one of `namespaces`.
    fn names_in(&self, list: &str, namespaces: &[Namespace]) -> bool {
        list.split_whitespace().all(|item| {
            let mut parts = item.split(':');
            match (parts.next(), parts.next(), parts.next()) {
                (Some(prefix), Some(_), None) => self
                    .declared(prefix)
                    .is_some_and(|uri| namespaces.contains(&Namespace::from_uri(uri))),
                _ => false,
            }
        })
    }

    /// Whether `mc:ProcessContent` names `name` where the walk is.
    fn process_content(&self, name: &QName) -> bool {
        self.mc.iter().any(|frame| {
            frame.process_content.iter().any(|(namespace, local)| {
                *namespace == name.namespace && (*local == name.local || local == "*")
            })
        })
    }

    /// Walk from `root`, typed `root_type`, when it is the element its type
    /// names.
    fn root(&mut self, root: &'tree Element, root_type: TypeId) {
        let Some(schema_type) = SCHEMA_TYPES.get(usize::from(root_type)) else {
            return;
        };
        if !is_element_of(root, schema_type) {
            return;
        }
        self.path.push(step(&root.name, 1));
        self.visit(root, Kind::Typed(root_type, false), Factory::Unknown);
        self.path.pop();
    }

    /// The checks of one typed element that exists at the target.
    fn validate(&mut self, element: &'tree Element, type_id: TypeId) {
        let Some(schema_type) = SCHEMA_TYPES.get(usize::from(type_id)) else {
            return;
        };
        self.compatibility_checks(element, Some(schema_type));
        let problems = attributes::check(
            element,
            schema_type,
            self.target,
            self.strict,
            |namespace| self.ignorable(namespace),
        );
        for problem in problems {
            self.error(problem.code, &problem.arguments);
        }
        if composite(schema_type) {
            let Some(model) = particle_at(schema_type, self.target) else {
                return;
            };
            let children = self.content(element, type_id);
            let Some(mismatch) = particle::check(model, &children, type_id, self.target) else {
                return;
            };
            let child = mismatch.child.and_then(|index| children.get(index));
            let child_name = child.map(|found| qualified(found.name)).unwrap_or_default();
            let child_type = child
                .and_then(|found| found.type_id)
                .and_then(|id| SCHEMA_TYPES.get(usize::from(id)))
                .map_or("", |found| found.name);
            self.error(
                mismatch.code,
                &[
                    ("child", child_name),
                    ("expected", mismatch.expected),
                    ("type", child_type.to_owned()),
                ],
            );
            return;
        }
        let element_name = qualified(&element.name);
        if element
            .children
            .iter()
            .any(|node| matches!(node, Node::Element(_)))
        {
            self.error(
                ErrorCode::ChildInLeafElement,
                &[("element", element_name.clone())],
            );
        }
        if schema_type.text_checks.is_empty() {
            return;
        }
        let text = element
            .children
            .iter()
            .find_map(|node| {
                if let Node::CData(text) | Node::Text(text) = node {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .unwrap_or_default();
        for failure in attributes::run(
            schema_type.text_checks,
            &ValueClass::String,
            Some(text),
            self.target,
            false,
        ) {
            let (code, detail) = match failure {
                Failure::Invalid(detail) => (ErrorCode::InvalidElementValue, detail),
                Failure::Missing => (ErrorCode::MissingRequiredAttribute, String::new()),
                Failure::NotInUnion => (ErrorCode::ValueNotInUnion, String::new()),
                Failure::Undeclared => (ErrorCode::UndeclaredAttribute, String::new()),
            };
            self.error(
                code,
                &[
                    ("attribute", element_name.clone()),
                    ("detail", detail),
                    ("element", element_name.clone()),
                    ("value", text.to_owned()),
                ],
            );
        }
    }

    /// Walk one element of kind `kind`; `factory` is the vocabulary it sits
    /// in, which also types the content of an `mc:AlternateContent` branch.
    fn visit(&mut self, element: &'tree Element, kind: Kind, factory: Factory) {
        let parent = self.ancestors.last().copied();
        self.ancestors.push(element);
        self.scope.push(&element.namespace_declarations);
        let frame = self.compatibility_frame(element);
        self.mc.push(frame);
        let mut reported_in = None;
        match kind {
            Kind::Typed(type_id, transitional_only) => {
                let namespace = transitional_only_namespace(&element.name.namespace);
                let reported_above =
                    namespace.is_some_and(|id| self.reported_namespaces.contains(&id));
                if self.schema
                    && self.strict
                    && !reported_above
                    && (transitional_only || namespace.is_some())
                {
                    self.error(
                        ErrorCode::TransitionalOnlyElement,
                        &[("element", qualified(&element.name))],
                    );
                    if let Some(id) = namespace {
                        self.reported_namespaces.push(id);
                        reported_in = namespace;
                    }
                }
                let exists = exists_at(type_id, self.target);
                if exists && self.schema {
                    self.validate(element, type_id);
                }
                if SCHEMA_TYPES
                    .get(usize::from(type_id))
                    .is_some_and(composite)
                {
                    self.children(element, Factory::Typed(type_id));
                }
                if exists {
                    self.observer
                        .element(element, Some(type_id), parent, &self.path);
                }
            }
            Kind::Unknown => {
                if self.process_content(&element.name) {
                    self.children(element, Factory::Unknown);
                }
            }
            Kind::AlternateContent => {
                if self.schema {
                    self.alternate_content(element);
                }
                if let Some((branch, index)) = self.branch(element) {
                    self.path.push(step(&branch.name, index));
                    self.scope.push(&branch.namespace_declarations);
                    self.ancestors.push(branch);
                    self.children(branch, factory);
                    self.ancestors.pop();
                    self.scope.pop();
                    self.path.pop();
                }
                self.observer.element(element, None, parent, &self.path);
            }
            Kind::Choice | Kind::Fallback => {}
        }
        if reported_in.is_some() {
            self.reported_namespaces.pop();
        }
        self.mc.pop();
        self.scope.pop();
        self.ancestors.pop();
    }
}

/// The running count of each name among an element's children.
#[derive(Debug, Default)]
struct Counts<'tree> {
    seen: Vec<(&'tree Namespace, &'tree str, u32)>,
}

impl<'tree> Counts<'tree> {
    /// The 1-based place of the next child called `name` among the children
    /// of its name so far.
    fn next(&mut self, name: &'tree QName) -> u32 {
        if let Some(entry) = self
            .seen
            .iter_mut()
            .find(|(namespace, local, _)| **namespace == name.namespace && *local == name.local)
        {
            entry.2 += 1;
            entry.2
        } else {
            self.seen.push((&name.namespace, &name.local, 1));
            1
        }
    }
}

/// What SDK 3.5.1 makes of `child` in `factory`'s vocabulary.
fn classify(child: &Element, factory: Factory) -> Kind {
    let alternate_content = is_mc(&child.name.namespace) && child.name.local == "AlternateContent";
    match factory {
        Factory::AlternateContent if is_mc(&child.name.namespace) => {
            match child.name.local.as_str() {
                "AlternateContent" => Kind::AlternateContent,
                "Choice" => Kind::Choice,
                "Fallback" => Kind::Fallback,
                _ => Kind::Unknown,
            }
        }
        Factory::Typed(parent) => {
            let declared = SCHEMA_TYPES
                .get(usize::from(parent))
                .and_then(|parent_type| {
                    let Namespace::Known(namespace) = child.name.namespace else {
                        return None;
                    };
                    parent_type
                        .children
                        .iter()
                        .find(|(own, local, _, _)| *own == namespace && *local == child.name.local)
                });
            match declared {
                Some((_, _, type_id, transitional_only)) => {
                    Kind::Typed(*type_id, *transitional_only)
                }
                None if alternate_content => Kind::AlternateContent,
                None => Kind::Unknown,
            }
        }
        Factory::Unknown if alternate_content => Kind::AlternateContent,
        Factory::AlternateContent | Factory::Unknown => Kind::Unknown,
    }
}

/// Whether `attribute` steers markup compatibility.
fn compatibility_attribute(attribute: &Attribute) -> bool {
    is_mc(&attribute.name.namespace)
        && matches!(
            attribute.name.local.as_str(),
            "Ignorable"
                | "MustUnderstand"
                | "PreserveAttributes"
                | "PreserveElements"
                | "ProcessContent"
        )
}

/// The value of an `mc:` attribute of markup compatibility, when not empty.
fn compatibility_value<'element>(element: &'element Element, local: &str) -> Option<&'element str> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == local && is_mc(&attribute.name.namespace))
        .map(|attribute| attribute.value.as_str())
        .filter(|value| !value.is_empty())
}

/// Whether a type has a content model at some target: an element of it holds
/// children rather than text or nothing.
fn composite(schema_type: &SchemaType) -> bool {
    schema_type
        .particles
        .iter()
        .any(|(_, model)| model.is_some())
}

/// Whether the element type exists at `target`.
fn exists_at(type_id: TypeId, target: OfficeVersion) -> bool {
    SCHEMA_TYPES
        .get(usize::from(type_id))
        .is_some_and(|found| found.versions.contains(target))
}

/// Whether `attribute` is `xml:lang` or `xml:space`.
fn is_xml_lang_or_space(attribute: &Attribute) -> bool {
    matches!(attribute.name.local.as_str(), "lang" | "space")
        && namespace_uri(&attribute.name.namespace) == XML_NAMESPACE
}

/// The URI a step names: a known namespace's Transitional URI, as SDK 3.5.1
/// reads Strict files.
fn namespace_uri(namespace: &Namespace) -> &str {
    if let Namespace::Known(id) = namespace {
        NAMESPACES.get(usize::from(*id)).map_or("", |row| row.uri)
    } else if let Namespace::Other(uri) = namespace {
        uri
    } else {
        ""
    }
}

/// The content model of a type at `target`.
fn particle_at(schema_type: &SchemaType, target: OfficeVersion) -> Option<&'static Particle> {
    schema_type
        .particles
        .iter()
        .find(|(versions, _)| versions.contains(target))
        .and_then(|(_, model)| model.as_ref())
}

/// Whether the element is the one a type names: `w:document` for
/// `w:CT_Document/w:document`.
fn is_element_of(element: &Element, schema_type: &SchemaType) -> bool {
    let Some((prefix, local)) = schema_type
        .name
        .split_once('/')
        .and_then(|(_, qualified_name)| qualified_name.split_once(':'))
    else {
        return false;
    };
    let namespace = NAMESPACES
        .iter()
        .position(|row| row.prefix == prefix)
        .and_then(|index| u8::try_from(index).ok());
    namespace.is_some_and(|id| element.name.namespace == Namespace::Known(id))
        && element.name.local == local
}

/// The step of an element at its place among the siblings of its name.
fn step(name: &QName, index: u32) -> Step {
    Step::new(
        &name.prefix,
        namespace_uri(&name.namespace),
        &name.local,
        index,
    )
}

/// Whether a namespace is a Microsoft extension introduced at `target`.
fn introduced_at(namespace: &Namespace, target: OfficeVersion) -> bool {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES
            .get(usize::from(id))
            .is_some_and(|row| row.introduced == Some(target))
    } else {
        false
    }
}

/// Walk a part's tree for `observer` alone, with no schema check: the
/// elements SDK 3.5.1's traversal yields at `target`, as `walk` shows them.
pub fn traverse(
    root: &Element,
    root_type: TypeId,
    target: OfficeVersion,
    part: &PartName,
    observer: &mut dyn Observer,
) {
    let mut errors = Errors::new(0);
    let mut walker = Walker {
        ancestors: Vec::new(),
        errors: &mut errors,
        mc: Vec::new(),
        observer,
        part,
        path: Vec::new(),
        reported_namespaces: Vec::new(),
        schema: false,
        scope: Vec::new(),
        strict: false,
        target,
    };
    walker.root(root, root_type);
}

/// Run the schema checks over a part's tree, keeping what they find in
/// `errors`, and show `observer` each element SDK 3.5.1's traversal yields.
///
/// `root` is the part's root element, `root_type` the type its part rule
/// gives it. A root of another element than the type's is not validated: SDK
/// 3.5.1 cannot load such a part.
pub fn walk(
    root: &Element,
    root_type: TypeId,
    target: OfficeVersion,
    strict: bool,
    part: &PartName,
    errors: &mut Errors,
    observer: &mut dyn Observer,
) {
    let mut walker = Walker {
        ancestors: Vec::new(),
        errors,
        mc: Vec::new(),
        observer,
        part,
        path: Vec::new(),
        reported_namespaces: Vec::new(),
        schema: true,
        scope: Vec::new(),
        strict,
        target,
    };
    walker.root(root, root_type);
}
