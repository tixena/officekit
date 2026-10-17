//! The run properties that count: for each run kind, the children of its
//! properties element, and for `a:rPr` the attributes, whose difference
//! changes how the text renders.
//!
//! A property the validation tables declare counts unless ECMA-376 (or, for
//! an extension, \[MS-DOCX\] / \[MS-ODRAWXML\]) describes it as affecting only
//! proofing, revision tracking, editing state, or a cached value; one they do
//! not declare at all counts too, compared as written, so a merge never drops
//! formatting officekit does not know.

use officekit_xml::element::{Attribute, Element, Node};
use officekit_xml::name::Namespace;
use officekit_xml::write::is_whitespace_text;

use super::prefix;
use crate::validation::generated::{SCHEMA_TYPES, type_id};
use crate::validation::schema::types::SchemaType;

#[cfg(test)]
mod tests;

/// `a:rPr` (21.1.2.3.9).
pub(super) static DRAWING: Counted = Counted {
    attributes: &[
        ("", "b"),
        ("", "baseline"),
        ("", "bmk"),
        ("", "cap"),
        ("", "i"),
        ("", "kern"),
        ("", "kumimoji"),
        // The font slot of the quote characters follows it (21.1.2.3).
        ("", "lang"),
        ("", "normalizeH"),
        ("", "smtId"),
        ("", "spc"),
        ("", "strike"),
        ("", "sz"),
        ("", "u"),
    ],
    children: &[
        ("a", "blipFill"),
        ("a", "cs"),
        ("a", "ea"),
        ("a", "effectDag"),
        ("a", "effectLst"),
        ("a", "extLst"),
        ("a", "gradFill"),
        ("a", "grpFill"),
        ("a", "highlight"),
        ("a", "hlinkClick"),
        ("a", "hlinkMouseOver"),
        ("a", "latin"),
        ("a", "ln"),
        ("a", "noFill"),
        ("a", "pattFill"),
        ("a", "rtl"),
        ("a", "solidFill"),
        ("a", "sym"),
        ("a", "uFill"),
        ("a", "uFillTx"),
        ("a", "uLn"),
        ("a", "uLnTx"),
    ],
    schema_type: "a:CT_TextCharacterProperties/a:rPr",
};

/// `x:rPr` in rich text (18.4.7).
pub(super) static SPREADSHEET: Counted = Counted {
    attributes: &[],
    children: &[
        ("x", "b"),
        ("x", "charset"),
        ("x", "color"),
        ("x", "condense"),
        ("x", "extend"),
        ("x", "family"),
        ("x", "i"),
        ("x", "outline"),
        ("x", "rFont"),
        ("x", "scheme"),
        ("x", "shadow"),
        ("x", "strike"),
        ("x", "sz"),
        ("x", "u"),
        ("x", "vertAlign"),
    ],
    schema_type: "x:CT_RPrElt/x:rPr",
};

/// `w:rPr` of a run (17.3.2.28).
pub(super) static WORDPROCESSING: Counted = Counted {
    attributes: &[],
    children: &[
        ("w", "b"),
        ("w", "bCs"),
        ("w", "bdr"),
        ("w", "caps"),
        ("w", "color"),
        ("w", "cs"),
        ("w", "dstrike"),
        ("w", "eastAsianLayout"),
        ("w", "effect"),
        ("w", "em"),
        ("w", "emboss"),
        ("w", "fitText"),
        ("w", "highlight"),
        ("w", "i"),
        ("w", "iCs"),
        ("w", "imprint"),
        ("w", "kern"),
        // The East Asian font slot of some characters follows it (17.3.2.26).
        ("w", "lang"),
        ("w", "outline"),
        ("w", "position"),
        ("w", "rFonts"),
        ("w", "rStyle"),
        ("w", "rtl"),
        ("w", "shadow"),
        ("w", "shd"),
        ("w", "smallCaps"),
        ("w", "snapToGrid"),
        ("w", "spacing"),
        ("w", "specVanish"),
        ("w", "strike"),
        ("w", "sz"),
        ("w", "szCs"),
        ("w", "u"),
        ("w", "vanish"),
        ("w", "vertAlign"),
        ("w", "w"),
        ("w", "webHidden"),
        ("w14", "cntxtAlts"),
        ("w14", "glow"),
        ("w14", "ligatures"),
        ("w14", "numForm"),
        ("w14", "numSpacing"),
        ("w14", "props3d"),
        ("w14", "reflection"),
        ("w14", "scene3d"),
        ("w14", "shadow"),
        ("w14", "stylisticSets"),
        ("w14", "textFill"),
        ("w14", "textOutline"),
    ],
    schema_type: "w:CT_RPr/w:rPr",
};

/// The properties of one run kind that count, each by the namespace table's
/// prefix (empty for no namespace) and its local name.
pub(super) struct Counted {
    attributes: &'static [(&'static str, &'static str)],
    children: &'static [(&'static str, &'static str)],
    /// The row of the properties element in the validation tables.
    schema_type: &'static str,
}

impl Counted {
    /// The attributes of `properties` that count, by namespace and local name.
    fn attributes_of<'element>(
        &self,
        row: Option<&SchemaType>,
        properties: Option<&'element Element>,
    ) -> Vec<&'element Attribute> {
        let mut counted: Vec<&Attribute> = properties
            .map_or(&[][..], |element| element.attributes.as_slice())
            .iter()
            .filter(|attribute| self.counts_attribute(row, attribute))
            .collect();
        counted.sort_by(|left, right| {
            left.name
                .namespace
                .cmp(&right.name.namespace)
                .then_with(|| left.name.local.cmp(&right.name.local))
        });
        counted
    }

    /// The children of `properties` that count: the elements by namespace and
    /// local name, and every other node but white space in order.
    fn children_of<'element>(
        &self,
        row: Option<&SchemaType>,
        properties: Option<&'element Element>,
    ) -> (Vec<&'element Element>, Vec<&'element Node>) {
        let mut elements: Vec<&Element> = Vec::new();
        let mut others: Vec<&Node> = Vec::new();
        for node in properties.map_or(&[][..], |element| element.children.as_slice()) {
            if let Node::Element(child) = node {
                if self.counts_child(row, child) {
                    elements.push(child);
                }
            } else if !is_whitespace_text(node) {
                others.push(node);
            } else {
                // White space between properties renders nothing.
            }
        }
        elements.sort_by(|left, right| {
            left.name
                .namespace
                .cmp(&right.name.namespace)
                .then_with(|| left.name.local.cmp(&right.name.local))
        });
        (elements, others)
    }

    /// Whether an attribute of the properties element counts: listed here, or
    /// not declared by `row` at all.
    fn counts_attribute(&self, row: Option<&SchemaType>, attribute: &Attribute) -> bool {
        let declared = row.is_some_and(|declaring| {
            declaring.attributes.iter().any(|constraint| {
                constraint.name == attribute.name.local
                    && constraint
                        .namespace
                        .map_or(Namespace::None, Namespace::Known)
                        == attribute.name.namespace
            })
        });
        !declared
            || self.attributes.iter().any(|&(listed, local)| {
                local == attribute.name.local && named(&attribute.name.namespace, listed)
            })
    }

    /// Whether a child of the properties element counts: listed here, or not
    /// declared by `row` at all.
    fn counts_child(&self, row: Option<&SchemaType>, child: &Element) -> bool {
        let declared = row.is_some_and(|declaring| {
            declaring.children.iter().any(|&(namespace, local, _, _)| {
                local == child.name.local && child.name.namespace == Namespace::Known(namespace)
            })
        });
        !declared
            || self.children.iter().any(|&(listed, local)| {
                local == child.name.local && named(&child.name.namespace, listed)
            })
    }

    /// Whether two runs' properties are equal: for every property that
    /// counts, both lack it or both have it written the same.
    pub(super) fn equal(&self, left: Option<&Element>, right: Option<&Element>) -> bool {
        let row = self.row();
        let left_attributes = self.attributes_of(row, left);
        let right_attributes = self.attributes_of(row, right);
        let attributes_equal = left_attributes.len() == right_attributes.len()
            && left_attributes
                .iter()
                .zip(&right_attributes)
                .all(|(one, other)| same_attribute(one, other));
        if !attributes_equal {
            return false;
        }
        let (left_elements, left_others) = self.children_of(row, left);
        let (right_elements, right_others) = self.children_of(row, right);
        left_others == right_others
            && left_elements.len() == right_elements.len()
            && left_elements
                .iter()
                .zip(&right_elements)
                .all(|(one, other)| same(one, other))
    }

    /// The row of the properties element in the validation tables.
    fn row(&self) -> Option<&'static SchemaType> {
        let id = type_id(self.schema_type)?;
        SCHEMA_TYPES.get(usize::from(id))
    }
}

/// Whether `namespace` is the one the namespace table names `listed`, or no
/// namespace for an empty `listed`.
fn named(namespace: &Namespace, listed: &str) -> bool {
    if listed.is_empty() {
        *namespace == Namespace::None
    } else {
        prefix(namespace) == Some(listed)
    }
}

/// Whether two elements are written the same: the same name, the same
/// attributes and values in any order, and the same children in order,
/// white space between them aside.
fn same(left: &Element, right: &Element) -> bool {
    if left.name.namespace != right.name.namespace
        || left.name.local != right.name.local
        || left.attributes.len() != right.attributes.len()
        || !left.attributes.iter().all(|one| {
            right
                .attributes
                .iter()
                .any(|other| same_attribute(one, other))
        })
    {
        return false;
    }
    let mut left_children = left
        .children
        .iter()
        .filter(|node| !is_whitespace_text(node));
    let mut right_children = right
        .children
        .iter()
        .filter(|node| !is_whitespace_text(node));
    loop {
        match (left_children.next(), right_children.next()) {
            (None, None) => return true,
            (Some(one), Some(other)) if same_node(one, other) => {}
            (Some(_), _) | (None, Some(_)) => return false,
        }
    }
}

fn same_attribute(left: &Attribute, right: &Attribute) -> bool {
    left.name.namespace == right.name.namespace
        && left.name.local == right.name.local
        && left.value == right.value
}

fn same_node(left: &Node, right: &Node) -> bool {
    if let (Node::Element(one), Node::Element(other)) = (left, right) {
        same(one, other)
    } else {
        left == right
    }
}
