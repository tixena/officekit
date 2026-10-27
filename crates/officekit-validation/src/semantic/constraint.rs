//! The types of the semantic constraint table.
//!
//! The 16 kinds of constraint SDK 3.5.1 registers on element types, and the
//! row that keys one to its element type, the Office versions, and the
//! applications it applies to.

use officekit_xml::name::NamespaceId;

use crate::schema::types::{TypeId, Versions};

/// Index into the attributes of the element type a row keys.
pub type Slot = u8;

/// A document kind a constraint can apply to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Application {
    /// A workbook.
    Excel,
    /// A presentation.
    PowerPoint,
    /// A word-processing document.
    Word,
}

/// The document kinds a row applies to: bit 0 is `Word`, bit 1 `Excel`, bit 2
/// `PowerPoint`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Applications(pub u8);

/// An attribute by namespace and local name, as a constraint names it on an
/// element of another type; `None` is no namespace.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct AttributeName {
    /// The local name.
    pub local: &'static str,
    /// The namespace; `None` for an unprefixed attribute.
    pub namespace: Option<NamespaceId>,
}

/// One semantic constraint. An attribute slot is `None` where the element
/// type declares no attribute of the constraint's local name, so the
/// constraint never applies to it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Constraint {
    /// `attribute` must be absent while `other` is present and holds none of `values`.
    AbsentUnlessValue {
        attribute: Option<Slot>,
        other: Option<Slot>,
        values: &'static [&'static str],
    },
    /// `attribute` must be absent while `other` holds one of `values`.
    AbsentWhenValue {
        attribute: Option<Slot>,
        other: Option<Slot>,
        values: &'static [&'static str],
    },
    /// A declared attribute that must be present.
    CannotBeOmitted { attribute: Option<Slot> },
    /// `attribute` is a whole number below the count of `element` items in
    /// the part `part` names, plus `index_base`; with `parent`, the elements
    /// counted are every element whose parent is `parent`.
    IndexReference {
        attribute: Option<Slot>,
        element: ElementName,
        index_base: u8,
        parent: Option<ElementName>,
        part: &'static str,
    },
    /// `attribute`, read as a number, is below `other`, or equal when `can_equal`.
    LessOrEqual {
        attribute: Option<Slot>,
        can_equal: bool,
        other: Option<Slot>,
    },
    /// At most one of the attributes is present.
    MutuallyExclusive { attributes: &'static [Slot] },
    /// `attribute`'s value is the `element_attribute` of some `element` in
    /// the part `part` names.
    ReferenceExists {
        attribute: Option<Slot>,
        element: ElementName,
        element_attribute: AttributeName,
        part: &'static str,
    },
    /// `attribute` names a relationship of the part.
    RelationshipExists { attribute: Option<Slot> },
    /// The relationship `attribute` names has the type `relationship_type`.
    RelationshipType {
        attribute: Option<Slot>,
        relationship_type: &'static str,
    },
    /// `attribute` must be present while `other` holds `value`.
    RequiredWhenValue {
        attribute: Option<Slot>,
        other: Option<Slot>,
        value: &'static str,
    },
    /// No two elements of the type in the part hold the same value; with
    /// `parent`, the scope SDK 3.5.1 names and never checks.
    UniqueValue {
        attribute: Option<Slot>,
        case_sensitive: bool,
        parent: Option<ElementName>,
    },
    /// `attribute` holds one of `values` while `other` holds one of `other_values`.
    ValueConditioned {
        attribute: Option<Slot>,
        other: Option<Slot>,
        other_values: &'static [&'static str],
        values: &'static [&'static str],
    },
    /// The value's length, in UTF-16 code units, is within the bounds.
    ValueLength {
        attribute: Option<Slot>,
        max: u32,
        min: u32,
    },
    /// The value matches `pattern`, a regular expression anchored at both ends.
    ValuePattern {
        attribute: Option<Slot>,
        pattern: &'static str,
    },
    /// The value, read as a number, is within the bounds.
    ValueRange {
        attribute: Option<Slot>,
        max: f64,
        max_inclusive: bool,
        min: f64,
        min_inclusive: bool,
    },
    /// The value is one of `values` when `valid`, none of them otherwise.
    ValueSet {
        attribute: Option<Slot>,
        valid: bool,
        values: &'static [&'static str],
    },
}

/// An element by namespace and local name.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ElementName {
    /// The local name.
    pub local: &'static str,
    /// The namespace.
    pub namespace: NamespaceId,
}

/// One constraint registered on an element type, and where it applies.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Registration {
    /// The document kinds it applies to.
    pub applications: Applications,
    /// The constraint.
    pub constraint: Constraint,
    /// The element type it is registered on, an index into `SCHEMA_TYPES`.
    pub type_id: TypeId,
    /// The targets it applies at: its version and every later one.
    pub versions: Versions,
}

impl Applications {
    /// Whether the row applies to documents of `application`.
    #[must_use]
    pub const fn contains(self, application: Application) -> bool {
        let bit = match application {
            Application::Word => 1,
            Application::Excel => 2,
            Application::PowerPoint => 4,
        };
        self.0 & bit != 0
    }
}
