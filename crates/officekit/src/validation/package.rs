//! The package pass: the relationship graph walked from the package's own
//! relationships, each source's relationships checked against its part rule,
//! before any part's XML is read.

use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::Target;

use super::error::{ErrorCode, Errors, ValidationError};
use super::generated::PART_RULES;
use super::schema::types::{ChildKind, ChildPart, PartRule};
use super::validator::{document_parts, document_rule, part_rule, transitional_type};

#[cfg(test)]
mod tests;

/// The data part references a part rule can list, by the data's name, with
/// their relationship types.
pub(crate) const DATA_PART_REFERENCES: &[(&str, &str)] = &[
    (
        "AudioReferenceRelationship",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio",
    ),
    (
        "MediaReferenceRelationship",
        "http://schemas.microsoft.com/office/2007/relationships/media",
    ),
    (
        "VideoReferenceRelationship",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video",
    ),
];

/// A hyperlink relates to no part, internal or external.
pub(crate) const HYPERLINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";

/// The standard's relationship type families, in each class's spelling.
const STANDARD_FAMILIES: &[&str] = &[
    "http://schemas.openxmlformats.org/",
    "http://purl.oclc.org/ooxml/",
];

/// A part one relationship of a source reaches.
struct Reached {
    /// The source, `None` for the package.
    from: Option<PartName>,
    part: PartName,
    /// `None` when no part type of the document matches the part.
    rule: Option<&'static PartRule>,
    /// In its Transitional spelling.
    type_uri: String,
}

/// The state of one walk of the relationship graph.
struct Walk<'walk> {
    document: &'static PartRule,
    errors: &'walk mut Errors,
    /// The part types the document can hold.
    known: Vec<&'static PartRule>,
    package: &'walk Package,
    version: OfficeVersion,
}

impl Walk<'_> {
    /// `DuplicatePart` for a child allowed once and related more than once,
    /// `MissingRequiredPart` for a required one not related, each by a child
    /// rule that applies at the target.
    fn check_counts(&mut self, source: Option<&PartName>, rule: &PartRule, children: &[Reached]) {
        for child in rule.children {
            let Some(child_rule) = ordinary_rule(child)
                .filter(|child_rule| child_rule.versions.contains(self.version))
            else {
                continue;
            };
            let count = children
                .iter()
                .filter(|reached| child_rule.relationship_type == Some(reached.type_uri.as_str()))
                .count();
            if child.required && count == 0 {
                self.report(
                    ErrorCode::MissingRequiredPart,
                    source,
                    None,
                    &[("part", child_rule.name)],
                );
            }
            if !child.many && count > 1 {
                let name = self.name(source);
                self.report(
                    ErrorCode::DuplicatePart,
                    source,
                    None,
                    &[("part", name), ("target", child_rule.name)],
                );
            }
        }
    }

    /// `PartNotAllowed` for each relationship to a part of a type the target
    /// knows that no child rule of the source allows.
    fn check_relationships(
        &mut self,
        source: Option<&PartName>,
        rule: &PartRule,
        children: &[Reached],
    ) {
        for reached in children {
            let allowed = rule.children.iter().any(|child| {
                ordinary_rule(child).is_some_and(|child_rule| {
                    child_rule.relationship_type == Some(reached.type_uri.as_str())
                })
            });
            if !allowed
                && reached
                    .rule
                    .is_some_and(|part_rule| part_rule.versions.contains(self.version))
            {
                let name = self.name(source);
                self.report(
                    ErrorCode::PartNotAllowed,
                    source,
                    Some(&reached.part),
                    &[("part", name), ("target", reached.part.as_str())],
                );
            }
        }
    }

    /// Check the relationships of `source` (`None`: the package) against its
    /// rule, `None` for a part no rule matches, which allows no data part and
    /// constrains no other; returns the parts they reach, in order.
    fn check_source(
        &mut self,
        source: Option<&PartName>,
        rule: Option<&'static PartRule>,
    ) -> Vec<Reached> {
        let mut children = Vec::new();
        for outcome in self.package.relationships_of(source) {
            let Ok(relationship) = outcome else {
                continue;
            };
            let Target::Part(part) = relationship.target else {
                continue;
            };
            let type_uri = transitional_type(&relationship.type_uri).into_owned();
            if type_uri == HYPERLINK || !self.package.contains(&part) {
                continue;
            }
            if let Some(&(data_part, _)) = DATA_PART_REFERENCES
                .iter()
                .find(|(_, data_type)| *data_type == type_uri)
            {
                let listed = rule.is_some_and(|part_rule| {
                    part_rule
                        .children
                        .iter()
                        .any(|child| child.kind == ChildKind::DataPart && child.name == data_part)
                });
                if !listed {
                    let name = self.name(source);
                    self.report(
                        ErrorCode::DataPartNotAllowed,
                        source,
                        Some(&part),
                        &[("part", name), ("target", part.as_str())],
                    );
                }
                continue;
            }
            let part_rule = part_rule(&self.known, &type_uri, relationship.content_type.as_deref());
            children.push(Reached {
                from: source.cloned(),
                part,
                rule: part_rule,
                type_uri,
            });
        }
        if let Some(part_rule) = rule {
            self.check_relationships(source, part_rule, &children);
            self.check_counts(source, part_rule, &children);
        }
        children
    }

    /// How a message names `source`: its part name, or the document's rule
    /// for the package.
    fn name<'name>(&self, source: Option<&'name PartName>) -> &'name str {
        source.map_or(self.document.name, PartName::as_str)
    }

    fn report(
        &mut self,
        code: ErrorCode,
        source: Option<&PartName>,
        related: Option<&PartName>,
        arguments: &[(&str, &str)],
    ) {
        let mut error = ValidationError::new(code, source.cloned(), None, arguments);
        error.related_part = related.cloned();
        self.errors.push(error);
    }
}

/// Every package error, up to the cap: the graph walked from the package's
/// own relationships, each part reached checked once, typed by the first
/// relationship that reaches it. External relationships, hyperlinks, and
/// relationships to parts the archive lacks reach no part.
pub(crate) fn check(package: &Package, version: OfficeVersion, errors: &mut Errors) {
    let Some(document) = document_rule(package) else {
        return;
    };
    let mut walk = Walk {
        document,
        errors,
        known: document_parts(document),
        package,
        version,
    };
    let mut seen = BTreeSet::new();
    let mut queue: Vec<Reached> = walk.check_source(None, Some(document));
    queue.reverse();
    while let Some(reached) = queue.pop() {
        if walk.errors.full() {
            return;
        }
        if !seen.insert(reached.part.folded()) {
            continue;
        }
        if reached.rule.is_none() && standard(&reached.type_uri) {
            walk.report(
                ErrorCode::UnknownPartOfStandardType,
                reached.from.as_ref(),
                Some(&reached.part),
                &[("part", reached.part.as_str())],
            );
        }
        let children = walk.check_source(Some(&reached.part), reached.rule);
        queue.extend(children.into_iter().rev());
    }
}

/// The rule of a child that is a part type, not a data part reference.
fn ordinary_rule(child: &ChildPart) -> Option<&'static PartRule> {
    let id = child.part.filter(|_| child.kind != ChildKind::DataPart)?;
    PART_RULES.get(usize::from(id))
}

/// Whether a relationship type is of one of the standard's families,
/// compared without ASCII case.
fn standard(type_uri: &str) -> bool {
    STANDARD_FAMILIES.iter().any(|family| {
        type_uri
            .get(..family.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(family))
    })
}
