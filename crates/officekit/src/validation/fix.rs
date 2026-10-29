//! The fix pass: the errors validation repairs itself, repaired in the package before it is
//! checked. Today that is a later copy of an id that nothing in ECMA-376 refers to.

use alloc::collections::BTreeSet;
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;
use core::mem;

use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document, Element, Node};
use officekit_xml::name::{Namespace, OfficeVersion, QName};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::{Layout, WriteOptions};

use super::error::{Step, ValidationError, steps_text};
use super::validator::reachable_parts;

#[cfg(test)]
mod tests;

/// How a repaired part is written back: compact, every namespace declaration kept.
const STORE: WriteOptions = WriteOptions::new(Layout::Compact, false);

const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const VML: &str = "urn:schemas-microsoft-com:vml";
const OFFICE: &str = "urn:schemas-microsoft-com:office:office";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The ids the pass renames: element namespace, element, unprefixed attribute, and how a fresh
/// value is made.
const RENAMEABLE: &[(&str, &str, &str, Fresh)] = &[
    (WORDPROCESSING_DRAWING, "docPr", "id", Fresh::Number),
    (VML, "group", "id", Fresh::Suffix),
    (VML, "rect", "id", Fresh::Suffix),
    (VML, "shape", "id", Fresh::Suffix),
];

/// The attributes that refer to an id: element namespace, element, attribute namespace (empty for
/// none), attribute. A value one of them holds is never renamed.
const REFERENCES: &[(&str, &str, &str, &str)] = &[
    (OFFICE, "OLEObject", "", "ShapeID"),
    (WORDPROCESSING, "control", WORDPROCESSING, "shapeid"),
    (DRAWING, "stCxn", "", "id"),
    (DRAWING, "endCxn", "", "id"),
];

/// What validation repairs before it checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Fix {
    /// Nothing: the package does not change.
    #[default]
    Off,
    /// Each later copy of an id in `RENAMEABLE` takes a value that nothing in the package holds.
    Trivial,
}

/// One value the fix pass changed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Fixed {
    /// The local name of the attribute.
    pub attribute: String,
    /// The value before.
    pub from: String,
    /// The part the element is in.
    pub part: PartName,
    /// The path to the element, as an error's path.
    pub path: Vec<Step>,
    /// The value after.
    pub to: String,
}

impl Fixed {
    /// `part path: renamed id "from" to "to"`, with the path written as
    /// [`ValidationError::line`] writes it.
    #[must_use]
    pub fn line(&self) -> String {
        format!(
            "{} {}: renamed {} \"{}\" to \"{}\"",
            self.part,
            steps_text(&self.path),
            self.attribute,
            self.from,
            self.to
        )
    }
}

/// What a validation that may fix found and changed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Validation {
    /// The errors left after the fix pass.
    pub errors: Vec<ValidationError>,
    /// The values the fix pass changed, in the order it changed them.
    pub fixed: Vec<Fixed>,
}

/// How a fresh value is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fresh {
    /// One more than the largest number any id of this kind holds in the package, then up by one.
    Number,
    /// The old value with `_2`, `_3`, … appended: the first that no id of this kind holds in the
    /// package.
    Suffix,
}

/// An element name with its namespace resolved, so that both classes' URIs match.
struct Target {
    local: &'static str,
    namespace: Namespace,
}

/// A `RENAMEABLE` row with its namespace resolved.
struct Renameable {
    attribute: &'static str,
    fresh: Fresh,
    target: Target,
}

/// A `REFERENCES` row with its namespaces resolved.
struct Reference {
    attribute: &'static str,
    attribute_namespace: Namespace,
    target: Target,
}

/// The two lists with their namespaces resolved once.
struct Tables {
    references: Vec<Reference>,
    renameable: Vec<Renameable>,
}

/// The values the package holds, and the next number to hand out.
#[derive(Default)]
struct Pool {
    names: BTreeSet<String>,
    next_number: u64,
    numbers: BTreeSet<String>,
}

/// What the first walk found across the parts.
#[derive(Default)]
struct Scan {
    pool: Pool,
    referenced: BTreeSet<String>,
}

/// The count of the elements met so far under one parent, by namespace and local name.
#[derive(Default)]
struct Siblings(Vec<(Namespace, String, u32)>);

/// The second walk over one part: renames the later copies and records them.
struct Renamer<'run> {
    fixed: Vec<Fixed>,
    part: &'run PartName,
    path: Vec<Step>,
    pool: &'run mut Pool,
    referenced: &'run BTreeSet<String>,
    seen: BTreeSet<(usize, String)>,
    tables: &'run Tables,
}

impl Target {
    fn matches(&self, name: &QName) -> bool {
        self.local == name.local && self.namespace == name.namespace
    }

    fn new(namespace: &str, local: &'static str) -> Self {
        Self {
            local,
            namespace: Namespace::from_uri(namespace),
        }
    }
}

impl Tables {
    fn new() -> Self {
        Self {
            references: REFERENCES
                .iter()
                .map(
                    |&(namespace, element, attribute_namespace, attribute)| Reference {
                        attribute,
                        attribute_namespace: Namespace::from_uri(attribute_namespace),
                        target: Target::new(namespace, element),
                    },
                )
                .collect(),
            renameable: RENAMEABLE
                .iter()
                .map(|&(namespace, element, attribute, fresh)| Renameable {
                    attribute,
                    fresh,
                    target: Target::new(namespace, element),
                })
                .collect(),
        }
    }
}

impl Pool {
    fn hold(&mut self, fresh: Fresh, value: &str) {
        let held = match fresh {
            Fresh::Number => &mut self.numbers,
            Fresh::Suffix => &mut self.names,
        };
        held.insert(value.to_owned());
    }

    /// Starts the numbers one above the largest the package holds.
    fn start_numbers(&mut self) {
        let largest = self
            .numbers
            .iter()
            .filter_map(|value| value.parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        self.next_number = largest.saturating_add(1);
    }

    /// A value no id of its kind holds, now held.
    fn take(&mut self, fresh: Fresh, old: &str) -> String {
        match fresh {
            Fresh::Number => loop {
                let candidate = self.next_number.to_string();
                self.next_number = self.next_number.saturating_add(1);
                if self.numbers.insert(candidate.clone()) {
                    break candidate;
                }
            },
            Fresh::Suffix => {
                let mut suffix = 2_u64;
                loop {
                    let candidate = format!("{old}_{suffix}");
                    suffix = suffix.saturating_add(1);
                    if self.names.insert(candidate.clone()) {
                        break candidate;
                    }
                }
            }
        }
    }
}

impl Scan {
    fn element(&mut self, tables: &Tables, element: &Element) {
        for row in &tables.renameable {
            if row.target.matches(&element.name)
                && let Some(value) = element.attribute(&Namespace::None, row.attribute)
            {
                self.pool.hold(row.fresh, value);
            }
        }
        for row in &tables.references {
            if row.target.matches(&element.name)
                && let Some(value) = element.attribute(&row.attribute_namespace, row.attribute)
            {
                self.referenced.insert(value.to_owned());
            }
        }
        for child in element.child_elements() {
            self.element(tables, child);
        }
    }
}

impl Siblings {
    fn next(&mut self, name: &QName) -> u32 {
        if let Some((_, _, count)) = self
            .0
            .iter_mut()
            .find(|(namespace, local, _)| *namespace == name.namespace && *local == name.local)
        {
            *count = count.saturating_add(1);
            *count
        } else {
            self.0.push((name.namespace.clone(), name.local.clone(), 1));
            1
        }
    }
}

impl Renamer<'_> {
    fn rename(&mut self, element: &mut Element) {
        for (row, entry) in self.tables.renameable.iter().enumerate() {
            if !entry.target.matches(&element.name) {
                continue;
            }
            let Some(attribute) = element.attributes.iter_mut().find(|attribute| {
                attribute.name.local == entry.attribute
                    && attribute.name.namespace == Namespace::None
            }) else {
                continue;
            };
            if attribute.value.is_empty()
                || self.referenced.contains(&attribute.value)
                || self.seen.insert((row, attribute.value.clone()))
            {
                continue;
            }
            let to = self.pool.take(entry.fresh, &attribute.value);
            let from = mem::replace(&mut attribute.value, to.clone());
            self.fixed.push(Fixed {
                attribute: entry.attribute.to_owned(),
                from,
                part: self.part.clone(),
                path: self.path.clone(),
                to,
            });
        }
    }

    fn visit(&mut self, element: &mut Element, index: u32) {
        self.path.push(Step::new(
            &element.name.prefix,
            element.name.namespace.uri(ConformanceClass::Transitional),
            &element.name.local,
            index,
        ));
        self.rename(element);
        let mut siblings = Siblings::default();
        for node in &mut element.children {
            if let Node::Element(child) = node {
                let child_index = siblings.next(&child.name);
                self.visit(child, child_index);
            }
        }
        self.path.pop();
    }
}

/// The tree of an XML part the validator walks, or `None` for an empty part.
fn load(package: &Package, part: &PartName) -> Result<Option<Document>, PackageError> {
    if package.part_bytes(part)?.is_empty() {
        return Ok(None);
    }
    package.load_tree(part, OpenOptions::default()).map(Some)
}

/// The repairs `fix` names, made in the parts `version` reaches.
///
/// The first walk reads every part for the values the package holds; the second renames, part by
/// part, and stores a part only when it changed. A part with no content type is skipped and its
/// renames are not recorded: the package pass reports it, and it cannot be stored.
///
/// # Errors
///
/// A part that cannot be read, parsed, or stored.
pub fn fix_package(
    package: &mut Package,
    version: OfficeVersion,
    fix: Fix,
) -> Result<Vec<Fixed>, PackageError> {
    if fix == Fix::Off {
        return Ok(Vec::new());
    }
    let tables = Tables::new();
    let parts: Vec<PartName> = reachable_parts(package, version)
        .into_iter()
        .filter(|(_, rule)| rule.root.is_some())
        .map(|(part, _)| part)
        .collect();
    let mut scan = Scan::default();
    for part in &parts {
        if let Some(document) = load(package, part)? {
            scan.element(&tables, &document.root);
        }
    }
    scan.pool.start_numbers();
    let mut fixed = Vec::new();
    for part in &parts {
        let Some(content_type) = package.content_type(part).map(str::to_owned) else {
            continue;
        };
        let Some(mut document) = load(package, part)? else {
            continue;
        };
        let mut renamer = Renamer {
            fixed: Vec::new(),
            part,
            path: Vec::new(),
            pool: &mut scan.pool,
            referenced: &scan.referenced,
            seen: BTreeSet::new(),
            tables: &tables,
        };
        renamer.visit(&mut document.root, 1);
        if !renamer.fixed.is_empty() {
            package.store_tree(part, &content_type, &document, STORE)?;
            fixed.append(&mut renamer.fixed);
        }
    }
    Ok(fixed)
}
