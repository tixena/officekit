//! The validator's error record: which check found it, where, its code, and its message, with the two forms
//! `officekit validate` prints.

use alloc::borrow::ToOwned as _;
use alloc::string::String;
use alloc::vec::Vec;

use serde_json::{Value, json};

use crate::packaging::part_name::PartName;
use crate::xml::name::OfficeVersion;

#[cfg(test)]
mod tests;

macro_rules! codes {
    ($($code:ident, $kind:ident, $template:literal;)*) => {
        /// officekit's codes: one per error SDK 3.5.1 can report, and the three for a file that declares Strict that it does not.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum ErrorCode {
            $($code),*
        }

        impl ErrorCode {
            pub const ALL: &'static [Self] = &[$(Self::$code),*];

            /// The variant's name.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$code => stringify!($code)),*
                }
            }

            #[must_use]
            pub const fn kind(self) -> Finding {
                match self {
                    $(Self::$code => Finding::$kind),*
                }
            }

            #[must_use]
            pub fn parse(text: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|code| code.as_str() == text)
            }

            /// officekit's message, with `{name}` placeholders the validator fills.
            #[must_use]
            pub const fn template(self) -> &'static str {
                match self {
                    $(Self::$code => $template),*
                }
            }
        }
    };
}

/// The errors a validation has found, up to a cap (D4).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Errors {
    /// `0` means no limit.
    cap: usize,
    found: Vec<ValidationError>,
}

impl Errors {
    /// Whether the cap is reached, so no further error is kept.
    #[must_use]
    pub const fn full(&self) -> bool {
        self.cap != 0 && self.found.len() >= self.cap
    }

    #[must_use]
    pub fn into_vec(self) -> Vec<ValidationError> {
        self.found
    }

    /// Keeps up to `cap` errors; `0` means no limit.
    #[must_use]
    pub const fn new(cap: usize) -> Self {
        Self {
            cap,
            found: Vec::new(),
        }
    }

    /// Keeps `error` unless the cap is already reached; the error that
    /// reaches it is kept.
    pub fn push(&mut self, error: ValidationError) {
        if !self.full() {
            self.found.push(error);
        }
    }
}

/// Which check produced an error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Finding {
    MarkupCompatibility,
    Package,
    Schema,
    Semantic,
}

impl Finding {
    /// The word the printed line and the JSON record use.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MarkupCompatibility => "markup-compatibility",
            Self::Package => "package",
            Self::Schema => "schema",
            Self::Semantic => "semantic",
        }
    }
}

/// One step of an error's path.
///
/// The element's namespace URI and local name, and its 1-based place among the siblings before it with the same
/// namespace and local name; `prefix` is the file's, for printing only.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Step {
    pub index: u32,
    pub local: String,
    pub namespace: String,
    pub prefix: String,
}

impl Step {
    #[must_use]
    pub fn new(prefix: &str, namespace: &str, local: &str, index: u32) -> Self {
        Self {
            index,
            local: local.to_owned(),
            namespace: namespace.to_owned(),
            prefix: prefix.to_owned(),
        }
    }
}

codes! {
    // Schema: the content model, attributes, and values.
    RepeatedChildInAllGroup, Schema, "the child {child} appears more than once in a group that allows each child once";
    ValueNotInUnion, Schema, "the attribute {attribute} has the value \"{value}\", which none of its type's member types accepts";
    InvalidAttributeValue, Schema, "the attribute {attribute} has the value \"{value}\": {detail}";
    InvalidElementValue, Schema, "the element {element} holds the value \"{value}\": {detail}";
    MissingChild, Schema, "the element ends before a child it requires{expected}";
    ChildInLeafElement, Schema, "the element {element} cannot hold child elements";
    InvalidChild, Schema, "the element holds {child}, which it does not allow anywhere{expected}";
    ChildOfWrongType, Schema, "the element holds {child} of type {type}, which it does not allow";
    MissingRequiredAttribute, Schema, "the attribute {attribute} is required and missing";
    MissingRootElement, Schema, "the part {part} has no root element";
    UndeclaredAttribute, Schema, "the attribute {attribute} is not declared for this element in the target version";
    UnexpectedChild, Schema, "the element holds {child} where it does not expect it{expected}";
    // Semantic: rules between values, across the document, and through relationships.
    AttributeMustBeAbsentUnlessValue, Semantic, "the attribute {attribute} must be absent unless {other} is {value}";
    AttributeMustBeAbsentForValue, Semantic, "the attribute {attribute} must be absent when {other} is {value}";
    AttributeCannotBeOmitted, Schema, "the attribute {attribute} is missing and cannot be omitted";
    MutuallyExclusiveAttributes, Semantic, "the attributes {attributes} cannot appear together";
    AttributeRequiredForValue, Semantic, "the attribute {attribute} is required when {other} is {value}";
    AttributeValueConflictsWithAnother, Semantic, "the attribute {attribute} must be {allowed} when {other} is {value}; it is \"{actual}\"";
    AttributeValueOutsideConstraint, Schema, "the attribute {attribute} has the value \"{value}\": {detail}";
    AttributeValueExceedsAnother, Semantic, "the attribute {attribute} is {value}, more than {other}, which is {limit}";
    IndexedElementNotFound, Semantic, "{attribute} refers to {element} number {index} of the part {part}, which does not exist";
    ReferencedElementNotFound, Semantic, "{attribute} refers to the {element} \"{value}\" in the part {part}, which does not exist";
    RelationshipNotFound, Semantic, "{attribute} refers to the relationship {id}, which does not exist";
    WrongRelationshipType, Semantic, "{attribute} refers to the relationship {id}, whose type is not {type}";
    DuplicateAttributeValue, Semantic, "the attribute {attribute} has the value \"{value}\", which another element already has";
    // Markup compatibility.
    UnprefixedAttributeOnAlternateContent, MarkupCompatibility, "the attribute {attribute} on mc:AlternateContent has no prefix";
    UndeclaredPrefixInIgnorable, MarkupCompatibility, "mc:Ignorable \"{value}\" names a prefix that is not declared";
    UndeclaredPrefixInMustUnderstand, MarkupCompatibility, "mc:MustUnderstand \"{value}\" names a prefix that is not declared";
    InvalidPreserveAttributes, MarkupCompatibility, "mc:PreserveAttributes \"{value}\" names an attribute outside the ignorable namespaces";
    InvalidPreserveElements, MarkupCompatibility, "mc:PreserveElements \"{value}\" names an element outside the ignorable namespaces";
    InvalidProcessContent, MarkupCompatibility, "mc:ProcessContent \"{value}\" names an element outside the ignorable namespaces";
    UndeclaredPrefixInRequires, MarkupCompatibility, "mc:Choice Requires \"{value}\" names a prefix that is not declared";
    XmlAttributeOnCompatibilityElement, MarkupCompatibility, "the element {element} cannot carry xml:lang or xml:space";
    XmlAttributeOnProcessedElement, MarkupCompatibility, "an element that mc:ProcessContent names cannot carry xml:lang or xml:space";
    ChoiceWithoutRequires, MarkupCompatibility, "an mc:Choice has no Requires attribute";
    AlternateContentWithoutChoice, MarkupCompatibility, "an mc:AlternateContent holds no mc:Choice, or its first child is not an mc:Choice";
    UnexpectedChildInAlternateContent, MarkupCompatibility, "an mc:AlternateContent holds a child after its mc:Fallback, or one that is neither mc:Choice nor mc:Fallback";
    NestedAlternateContent, MarkupCompatibility, "an mc:AlternateContent sits directly inside another";
    // Package: which parts relate to which.
    DataPartNotAllowed, Package, "the part {part} cannot relate to the data part {target}";
    UnknownPartOfStandardType, Package, "the part {part} has a relationship type of the standard, which officekit expects a part module for";
    DuplicatePart, Package, "the part {part} can relate to only one {target}";
    PartNotAllowed, Package, "the part {part} cannot relate to the part {target}";
    MissingRequiredPart, Package, "the required part {part} is missing";
    // A file that declares Strict, checked against Strict (D35); SDK 3.5.1 has none of these.
    TransitionalOnlyElement, Schema, "the element {element} exists only in Transitional, and the file declares Strict";
    TransitionalOnlyAttribute, Schema, "the attribute {attribute} exists only in Transitional, and the file declares Strict";
    TransitionalOnlyValue, Schema, "the value \"{value}\" of {attribute} exists only in Transitional, and the file declares Strict";
}

/// One validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValidationError {
    pub code: ErrorCode,
    pub message: String,
    pub part: Option<PartName>,
    /// The element the error is on; `None` for a package error.
    pub path: Option<Vec<Step>>,
    /// The second part a package error names.
    pub related_part: Option<PartName>,
}

impl ValidationError {
    /// What `officekit validate` prints: `part path: kind code: message`.
    #[must_use]
    pub fn line(&self) -> String {
        let path = self.path_text();
        let location = match &self.part {
            Some(part) if path.is_empty() => format!("{part}: "),
            Some(part) => format!("{part} {path}: "),
            None => String::new(),
        };
        format!(
            "{location}{} {}: {}",
            self.code.kind().as_str(),
            self.code.as_str(),
            self.message
        )
    }

    /// The message from the code's template, each `{name}` filled from `arguments`; an unfilled one is left out.
    #[must_use]
    pub fn new(
        code: ErrorCode,
        part: Option<PartName>,
        path: Option<Vec<Step>>,
        arguments: &[(&str, &str)],
    ) -> Self {
        Self {
            code,
            message: fill(code.template(), arguments),
            part,
            path,
            related_part: None,
        }
    }

    /// `/w:document[1]/w:body[1]`, with the file's prefixes.
    #[must_use]
    pub fn path_text(&self) -> String {
        self.path.as_ref().map_or_else(String::new, |steps| {
            steps
                .iter()
                .map(|step| {
                    if step.prefix.is_empty() {
                        format!("/{}[{}]", step.local, step.index)
                    } else {
                        format!("/{}:{}[{}]", step.prefix, step.local, step.index)
                    }
                })
                .collect()
        })
    }

    /// What `officekit validate --json` prints for one error.
    #[must_use]
    pub fn record(&self, file: &str, target: OfficeVersion) -> Value {
        let path = self.path.as_ref().map(|steps| {
            steps
                .iter()
                .map(|step| {
                    json!({ "namespace": step.namespace, "local": step.local, "index": step.index })
                })
                .collect::<Vec<_>>()
        });
        json!({
            "file": file,
            "target": target_name(target),
            "kind": self.code.kind().as_str(),
            "code": self.code.as_str(),
            "part": self.part.as_ref().map(PartName::as_str),
            "related_part": self.related_part.as_ref().map(PartName::as_str),
            "path": path,
            "message": self.message,
        })
    }
}

/// The target as the JSON record and the oracle name it.
#[must_use]
pub const fn target_name(target: OfficeVersion) -> &'static str {
    match target {
        OfficeVersion::Office2010 => "Office2010",
        OfficeVersion::Office2013 => "Office2013",
        OfficeVersion::Office2016 => "Office2016",
        OfficeVersion::Office2019 => "Office2019",
        OfficeVersion::Office2021 => "Office2021",
        OfficeVersion::Microsoft365 => "Microsoft365",
    }
}

fn fill(template: &str, arguments: &[(&str, &str)]) -> String {
    let mut message = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        message.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            message.push_str(&rest[open..]);
            return message;
        };
        let name = &after[..close];
        if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
            message.push_str(value);
        }
        rest = &after[close + 1..];
    }
    message.push_str(rest);
    message
}
