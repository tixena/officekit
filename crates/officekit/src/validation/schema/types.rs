//! The types of the validation tables: what each schema type, attribute,
//! value, content model, and part allows, and at which Office versions.

use officekit_xml::name::{NamespaceId, OfficeVersion};

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
    /// Only the namespace with this URI.
    Uri(&'static str),
}

/// An attribute a schema type declares, and the checks of its value.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct AttributeConstraint {
    /// Run in order; the first that fails stops the rest.
    pub checks: &'static [ValueCheck],
    /// The class the value reads as.
    pub class: ValueClass,
    /// The local name.
    pub name: &'static str,
    /// `None` for an unprefixed attribute.
    pub namespace: Option<NamespaceId>,
    /// The checks of its Strict type, when that is not its Transitional type:
    /// a value they refuse exists only in Transitional.
    pub strict_checks: Option<&'static [ValueCheck]>,
    /// Whether the attribute exists only in Transitional.
    pub transitional_only: bool,
}

/// A number check's bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Bounds {
    /// The value must be below this.
    pub max_exclusive: Option<f64>,
    /// The value must not be above this.
    pub max_inclusive: Option<f64>,
    /// The value must be above this.
    pub min_exclusive: Option<f64>,
    /// The value must not be below this.
    pub min_inclusive: Option<f64>,
    /// The value must not be negative.
    pub non_negative: bool,
    /// The value must be above zero.
    pub positive: bool,
}

/// One check of a value, as one of SDK 3.5.1's validators makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Check {
    /// A member of the enumeration at the target.
    Enum { class: ValueClass },
    /// Each item of a white-space separated list passes the item check.
    List { item: &'static Self },
    /// The value (each item, for a list) reads as `class` and keeps within the bounds.
    Number {
        bounds: Bounds,
        class: ValueClass,
        list: bool,
    },
    /// The attribute exists only at the check's versions; elsewhere it is undeclared.
    OfficeVersion,
    /// The attribute must be present.
    Required,
    /// The value's lengths, its lexical kind, and a pattern it matches whole.
    String {
        /// What the lengths count: bytes of hexadecimal digits for `HexBinary`, characters for
        /// `String`; `None` takes the attribute's class, as SDK 3.5.1 does.
        class: Option<ValueClass>,
        /// The lexical kind the text must have.
        kind: StringKind,
        /// The exact length, when the type fixes one.
        length: Option<usize>,
        /// The longest length allowed.
        max_length: Option<usize>,
        /// The shortest length allowed.
        min_length: Option<usize>,
        /// A pattern the whole text must match.
        pattern: Option<&'static str>,
    },
}

/// How a part reaches a child part.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChildKind {
    /// A data part (media) shared by reference.
    DataPart,
    /// A part of its own, reached by a relationship.
    Ordinary,
    /// A part embedded in the package in a form of its own, such as an embedded document.
    SpecialEmbedded,
}

/// A child part a part type may relate to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildPart {
    /// Whether the child's content type is fixed by its part type.
    pub fixed_content: bool,
    /// How the part reaches the child.
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
    /// Each item at most once, in any order.
    All,
    /// One of the items.
    Choice,
    /// A named group of items.
    Group,
    /// The items in order.
    Sequence,
}

/// An enumeration of the data, by its simple type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Enumeration {
    /// Each value, the targets it is a member at, and whether only Transitional allows it.
    pub members: &'static [(&'static str, Versions, bool)],
    /// `w:ST_TblWidth`.
    pub simple_type: &'static str,
}

/// A part type's rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PartRule {
    /// The child parts the part type may relate to.
    pub children: &'static [ChildPart],
    /// The content type the part must have; `None` for any.
    pub content_type: Option<&'static str>,
    /// The data's name, `MainDocumentPart`.
    pub name: &'static str,
    /// The relationship type, in its Transitional spelling, that reaches the part type.
    pub relationship_type: Option<&'static str>,
    /// The type of the part's root element.
    pub root: Option<TypeId>,
    /// The targets the part type exists at; at another target its part is not validated.
    pub versions: Versions,
}

/// A content model at one target; `max` 0 is unbounded, as in SDK 3.5.1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Particle {
    /// Any element of the namespaces `namespace` names, `min` to `max` times.
    Any {
        /// The most times; `0` is unbounded.
        max: u32,
        /// The fewest times.
        min: u32,
        /// The namespaces accepted.
        namespace: AnyNamespace,
    },
    /// A group of particles combined by `kind`, `min` to `max` times.
    Composite {
        /// The particles combined.
        items: &'static [Self],
        /// How they combine.
        kind: CompositeKind,
        /// The most times; `0` is unbounded.
        max: u32,
        /// The fewest times.
        min: u32,
    },
    /// An element of the type `type_id`, `min` to `max` times.
    Element {
        /// The most times; `0` is unbounded.
        max: u32,
        /// The fewest times.
        min: u32,
        /// The type of the element, an index into `SCHEMA_TYPES`.
        type_id: TypeId,
    },
}

/// A schema type of the data: an element type, `w:CT_TblCellMar/w:tblCellMar`,
/// or a schema type alone, `w:CT_TblWidth/`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SchemaType {
    /// The attributes the type declares.
    pub attributes: &'static [AttributeConstraint],
    /// Each child by namespace and local name, its type, and whether only Transitional allows it
    /// here; the type comes from the parent, since one tag is another type under another parent.
    pub children: &'static [(NamespaceId, &'static str, TypeId, bool)],
    /// The type's name in the data.
    pub name: &'static str,
    /// The content model at each target, equal ones merged.
    pub particles: &'static [(Versions, Option<Particle>)],
    /// The checks of a leaf element's text.
    pub text_checks: &'static [ValueCheck],
    /// The targets the element type exists at; at another target an element
    /// of the type is not validated.
    pub versions: Versions,
}

/// The lexical kind a string check requires.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum StringKind {
    /// An XML `ID`.
    Id,
    /// An XML `NCName`: a name without a colon.
    NcName,
    /// Any text.
    Plain,
    /// An XML qualified name.
    QName,
    /// An XML token: no leading or trailing space, no runs of spaces.
    Token,
    /// A URI.
    Uri,
}

/// A check, the targets it runs at, and the union it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ValueCheck {
    /// The check.
    pub check: Check,
    /// A value passes a union when one member running at the target passes, or none runs there.
    pub union: Option<u8>,
    /// The targets the check runs at.
    pub versions: Versions,
}

/// The class a value reads as, by an attribute's `Type` in the data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ValueClass {
    /// Base64 text.
    Base64Binary,
    /// `true`, `false`, `1` or `0`.
    Boolean,
    /// A whole number from 0 to 255.
    Byte,
    /// A date and time.
    DateTime,
    /// A decimal number.
    Decimal,
    /// A double-precision number.
    Double,
    /// An enumeration, by its simple type: `w:ST_TblWidth`.
    Enum(&'static str),
    /// An even number of hexadecimal digits.
    HexBinary,
    /// A whole number from -32768 to 32767.
    Int16,
    /// A whole number of 32 bits, with a sign.
    Int32,
    /// A whole number of 64 bits, with a sign.
    Int64,
    /// A whole number in the range of a signed 64-bit number.
    Integer,
    /// `ListValue<T>`: white-space separated items of the class.
    List(&'static Self),
    /// An on/off switch: `on`, `off`, `true`, `false`, `1` or `0`.
    OnOff,
    /// A whole number from -128 to 127.
    SByte,
    /// A single-precision number.
    Single,
    /// Any text.
    String,
    /// `t`, `f`, `true` or `false`.
    TrueFalse,
    /// `t`, `f`, `true`, `false` or empty.
    TrueFalseBlank,
    /// A whole number from 0 to 65535.
    UInt16,
    /// A whole number of 32 bits, without a sign.
    UInt32,
    /// A whole number of 64 bits, without a sign.
    UInt64,
}

/// The targets a row applies at: bit 0 is Office 2010, bit 5 Microsoft 365.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Versions(pub u8);

impl Versions {
    /// Whether the row applies at `target`.
    #[must_use]
    pub const fn contains(self, target: OfficeVersion) -> bool {
        self.0 & (1 << target.rank()) != 0
    }
}
