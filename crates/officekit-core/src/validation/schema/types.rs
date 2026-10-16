//! The types of the validation tables: what each schema type, attribute,
//! value, content model, and part allows, and at which Office versions.

use crate::xml::name::NamespaceId;

/// Index into `PART_RULES`.
pub type PartRuleId = u8;

/// Index into `SCHEMA_TYPES`.
pub type TypeId = u16;

/// The namespaces an `xsd:any` particle accepts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AnyNamespace {
    /// `##any`.
    Any,
    /// `##local`: no namespace.
    Local,
    /// `##other`: any namespace but the type's own.
    Other,
    /// `##targetNamespace`: the type's own namespace.
    TargetNamespace,
    /// A namespace URI.
    Uri(&'static str),
}

/// An attribute a schema type declares, and the checks of its value.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AttributeConstraint {
    /// Run in order; the first that fails stops the rest.
    pub checks: &'static [ValueCheck],
    pub class: ValueClass,
    /// The local name.
    pub name: &'static str,
    /// `None` for an unprefixed attribute.
    pub namespace: Option<NamespaceId>,
    /// Only Transitional declares it (D35).
    pub transitional_only: bool,
}

/// A number check's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Bounds {
    pub max_exclusive: Option<f64>,
    pub max_inclusive: Option<f64>,
    pub min_exclusive: Option<f64>,
    pub min_inclusive: Option<f64>,
    pub non_negative: bool,
    pub positive: bool,
}

/// One check of a value, as one of SDK 3.5.1's validators makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Check {
    /// A member of the enumeration at the target.
    Enum {
        class: ValueClass,
    },
    /// Each item of a white-space separated list passes the item check.
    List {
        item: &'static Self,
    },
    /// The value (each item, for a list) reads as `class` and keeps within the bounds.
    Number {
        bounds: Bounds,
        class: ValueClass,
        list: bool,
    },
    /// The attribute exists only at the check's versions; elsewhere it is undeclared.
    OfficeVersion,
    Required,
    /// The value's lengths in characters, its lexical kind, and a pattern it matches whole.
    String {
        kind: StringKind,
        length: Option<usize>,
        max_length: Option<usize>,
        min_length: Option<usize>,
        pattern: Option<&'static str>,
    },
}

/// How a part reaches a child part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChildKind {
    /// A data part (media) shared by reference.
    DataPart,
    Ordinary,
    SpecialEmbedded,
}

/// A child part a part type may relate to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildPart {
    pub fixed_content: bool,
    pub kind: ChildKind,
    /// More than one may be related.
    pub many: bool,
    /// The data's name: a part type's, or a data part reference's, `VideoReferenceRelationship`.
    pub name: &'static str,
    /// The child's rule; `None` for a data part reference, which has none.
    pub part: Option<PartRuleId>,
    /// At least one must be related.
    pub required: bool,
}

/// How a composite particle combines its items.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CompositeKind {
    All,
    Choice,
    Group,
    Sequence,
}

/// An enumeration of the data, by its simple type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Enumeration {
    /// Each value, the targets it is a member at, and whether only Transitional allows it (D35).
    pub members: &'static [(&'static str, Versions, bool)],
    /// `w:ST_TblWidth`.
    pub simple_type: &'static str,
}

/// A part type's rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PartRule {
    pub children: &'static [ChildPart],
    pub content_type: Option<&'static str>,
    /// The data's name, `MainDocumentPart`.
    pub name: &'static str,
    pub relationship_type: Option<&'static str>,
    /// The type of the part's root element.
    pub root: Option<TypeId>,
}

/// A content model at one target; `max` 0 is unbounded, as in SDK 3.5.1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Particle {
    Any {
        max: u32,
        min: u32,
        namespace: AnyNamespace,
    },
    Composite {
        items: &'static [Self],
        kind: CompositeKind,
        max: u32,
        min: u32,
    },
    Element {
        max: u32,
        min: u32,
        type_id: TypeId,
    },
}

/// A schema type of the data: an element type, `w:CT_TblCellMar/w:tblCellMar`,
/// or a schema type alone, `w:CT_TblWidth/`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SchemaType {
    pub attributes: &'static [AttributeConstraint],
    /// Each child by namespace and local name, its type, and whether only
    /// Transitional allows it here (D35); a child's type is resolved from its
    /// parent, because one tag is a different type under another parent (D2).
    pub children: &'static [(NamespaceId, &'static str, TypeId, bool)],
    pub name: &'static str,
    /// The content model at each target, equal ones merged.
    pub particles: &'static [(Versions, Option<Particle>)],
    /// The checks of a leaf element's text.
    pub text_checks: &'static [ValueCheck],
}

/// The lexical kind a string check requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StringKind {
    Id,
    NcName,
    Plain,
    QName,
    Token,
    Uri,
}

/// A check, the targets it runs at, and the union it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ValueCheck {
    pub check: Check,
    /// A value passes a union when one member running at the target passes, or none runs there.
    pub union: Option<u8>,
    pub versions: Versions,
}

/// The class a value reads as, by an attribute's `Type` in the data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueClass {
    Base64Binary,
    Boolean,
    Byte,
    DateTime,
    Decimal,
    Double,
    /// An enumeration, by its simple type: `w:ST_TblWidth`.
    Enum(&'static str),
    HexBinary,
    Int16,
    Int32,
    Int64,
    Integer,
    /// `ListValue<T>`: white-space separated items of the class.
    List(&'static Self),
    OnOff,
    SByte,
    Single,
    String,
    TrueFalse,
    TrueFalseBlank,
    UInt16,
    UInt32,
    UInt64,
}

/// The targets a row applies at: bit 0 is Office 2010, bit 5 Microsoft 365.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Versions(pub u8);
