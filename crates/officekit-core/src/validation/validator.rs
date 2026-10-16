//! The validator: the target version, the cap on errors it reports, and the
//! pipeline that runs the package pass, then the schema pass over every part
//! it reaches.

use alloc::borrow::Cow;
use alloc::collections::BTreeSet;
use alloc::string::String;
use alloc::vec::Vec;
use std::path::Path;

use super::error::{ErrorCode, Errors, ValidationError};
use super::generated::PART_RULES;
use super::package;
use super::schema::types::{ChildKind, PartRule};
use super::schema::walk::walk;
use crate::packaging::package::{OpenOptions, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{RelationshipType, Target};
use crate::xml::conformance::ConformanceClass;
use crate::xml::name::OfficeVersion;

#[cfg(test)]
mod tests;

/// The part types SDK 3.5.1 leaves out of each document's vocabulary,
/// although the part rules reach them, so that each relationship type names
/// one part type in a document.
const LEFT_OUT: &[(&str, &[&str])] = &[
    (
        "PresentationDocument",
        &["SpreadsheetPrinterSettingsPart", "WorksheetCommentsPart"],
    ),
    ("SpreadsheetDocument", &["SlideCommentsPart"]),
    (
        "WordprocessingDocument",
        &[
            "SlideCommentsPart",
            "SpreadsheetPrinterSettingsPart",
            "WorksheetCommentsPart",
        ],
    ),
];

/// The Strict relationship types whose Transitional spelling is not the
/// family's prefix swapped.
const STRICT_RENAMED: &[(&str, &str)] = &[
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships/customProperties",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
    ),
    (
        "http://purl.oclc.org/ooxml/officeDocument/relationships/metadata/thumbnail",
        "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail",
    ),
];

/// The `officeDocument` relationship family in each class's spelling.
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const TRANSITIONAL_RELATIONSHIPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";

/// Checks a package against one target version, stopping at a number of errors.
#[derive(Debug, Clone, Copy)]
pub struct Validator {
    max_errors: usize,
    version: OfficeVersion,
}

impl Validator {
    pub const DEFAULT_MAX_ERRORS: usize = 1000;

    /// Keeps the errors up to the cap; the one that reaches it is kept.
    #[must_use]
    pub fn limit(&self, mut errors: Vec<ValidationError>) -> Vec<ValidationError> {
        if self.max_errors != 0 {
            errors.truncate(self.max_errors);
        }
        errors
    }

    #[must_use]
    pub const fn max_errors(&self) -> usize {
        self.max_errors
    }

    #[must_use]
    pub const fn new(version: OfficeVersion) -> Self {
        Self {
            max_errors: Self::DEFAULT_MAX_ERRORS,
            version,
        }
    }

    /// Every error of the package pass, then of the schema pass, up to the
    /// cap.
    ///
    /// The schema pass, for each part reachable from the package and known at
    /// the target: `MissingRootElement` when the part is an empty XML file,
    /// else its tree walked from the root type its part rule gives. A part
    /// that does not parse is the package pass's to report.
    #[must_use]
    pub fn validate(&self, package: &Package) -> Vec<ValidationError> {
        let mut errors = Errors::new(self.max_errors);
        package::check(package, self.version, &mut errors);
        if !errors.full() {
            let strict = package_class(package) == Some(ConformanceClass::Strict);
            for (part, part_rule) in reachable_parts(package, self.version) {
                let Ok(bytes) = package.part_bytes(&part) else {
                    continue;
                };
                if bytes.is_empty() && xml_part(&part) {
                    errors.push(ValidationError::new(
                        ErrorCode::MissingRootElement,
                        Some(part.clone()),
                        None,
                        &[("part", part.as_str())],
                    ));
                } else if let Some(root) = part_rule.root
                    && let Ok(tree) = package.load_tree(&part, OpenOptions::default())
                {
                    walk(&tree.root, root, self.version, strict, &part, &mut errors);
                } else {
                    // A part with no root type, or one the package pass reports.
                }
                if errors.full() {
                    break;
                }
            }
        }
        errors.into_vec()
    }

    #[must_use]
    pub const fn version(&self) -> OfficeVersion {
        self.version
    }

    /// `0` means no limit.
    #[must_use]
    pub const fn with_max_errors(mut self, max_errors: usize) -> Self {
        self.max_errors = max_errors;
        self
    }
}

/// The part types a document can hold: every rule the document's rule
/// reaches through child parts, but the ones SDK 3.5.1 leaves out of it.
#[must_use]
pub fn document_parts(document: &PartRule) -> Vec<&'static PartRule> {
    let left_out = LEFT_OUT
        .iter()
        .find(|(name, _)| *name == document.name)
        .map_or(&[][..], |(_, names)| *names);
    let mut found: Vec<&'static PartRule> = Vec::new();
    let mut queue: Vec<&PartRule> = vec![document];
    let mut at = 0;
    while let Some(current) = queue.get(at).copied() {
        at += 1;
        for child in current.children {
            let Some(child_rule) = child
                .part
                .filter(|_| child.kind != ChildKind::DataPart)
                .and_then(|id| PART_RULES.get(usize::from(id)))
            else {
                continue;
            };
            if left_out.contains(&child_rule.name)
                || found.iter().any(|known| known.name == child_rule.name)
            {
                continue;
            }
            found.push(child_rule);
            queue.push(child_rule);
        }
    }
    found
}

/// The rule of the document the package holds, by its main part's content
/// type: a word-processing document, a spreadsheet, or a presentation.
pub(crate) fn document_rule(package: &Package) -> Option<&'static PartRule> {
    let main = package
        .related(None)
        .ok()?
        .into_iter()
        .find(|related| RelationshipType::OFFICE_DOCUMENT.matches(&related.type_uri))?;
    let content_type = main.content_type?;
    let name = if content_type.contains("wordprocessingml") || content_type.contains("ms-word") {
        "WordprocessingDocument"
    } else if content_type.contains("spreadsheetml") || content_type.contains("ms-excel") {
        "SpreadsheetDocument"
    } else if content_type.contains("presentationml") || content_type.contains("ms-powerpoint") {
        "PresentationDocument"
    } else {
        return None;
    };
    rule(name)
}

/// The class the package declares through its `officeDocument`
/// relationship type (D21).
fn package_class(package: &Package) -> Option<ConformanceClass> {
    package.related(None).ok()?.into_iter().find_map(|related| {
        if related.type_uri == RelationshipType::OFFICE_DOCUMENT.strict {
            Some(ConformanceClass::Strict)
        } else if related.type_uri == RelationshipType::OFFICE_DOCUMENT.transitional {
            Some(ConformanceClass::Transitional)
        } else {
            None
        }
    })
}

/// Walk the relationships of `source` (`None`: the package) depth first.
fn reach(
    package: &Package,
    source: Option<&PartName>,
    known: &[&'static PartRule],
    seen: &mut BTreeSet<String>,
    found: &mut Vec<(PartName, &'static PartRule)>,
) {
    let Ok(related) = package.related(source) else {
        return;
    };
    for relationship in related {
        let Target::Part(part) = relationship.target else {
            continue;
        };
        if !seen.insert(part.folded()) {
            continue;
        }
        let type_uri = transitional_type(&relationship.type_uri);
        if let Some(part_rule) = known
            .iter()
            .find(|candidate| candidate.relationship_type == Some(type_uri.as_ref()))
        {
            found.push((part.clone(), part_rule));
        }
        reach(package, Some(&part), known, seen, found);
    }
}

/// Every part reachable from the package that the target knows, with its rule.
///
/// A part is known when its relationship type names a part type of the
/// document and that type exists at `version`. The parts come in the order a
/// depth-first walk of the relationships meets them, each once, typed by the
/// first relationship that reaches it.
#[must_use]
pub fn reachable_parts(
    package: &Package,
    version: OfficeVersion,
) -> Vec<(PartName, &'static PartRule)> {
    let Some(document) = document_rule(package) else {
        return Vec::new();
    };
    let known = document_parts(document);
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    reach(package, None, &known, &mut seen, &mut found);
    found.retain(|(_, part_rule)| part_rule.versions.contains(version));
    found
}

/// The part rule called `name`, `MainDocumentPart`.
fn rule(name: &str) -> Option<&'static PartRule> {
    let index = PART_RULES
        .binary_search_by(|candidate| candidate.name.cmp(name))
        .ok()?;
    PART_RULES.get(index)
}

/// A relationship type in its Transitional spelling, as the part rules name it.
pub(crate) fn transitional_type(type_uri: &str) -> Cow<'_, str> {
    if let Some((_, transitional)) = STRICT_RENAMED
        .iter()
        .find(|(strict, _)| *strict == type_uri)
    {
        return Cow::Borrowed(transitional);
    }
    type_uri
        .strip_prefix(STRICT_RELATIONSHIPS)
        .map_or(Cow::Borrowed(type_uri), |rest| {
            Cow::Owned(format!("{TRANSITIONAL_RELATIONSHIPS}{rest}"))
        })
}

/// Whether the part's name ends in `.xml`, in any case.
fn xml_part(part: &PartName) -> bool {
    Path::new(part.as_str())
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}
