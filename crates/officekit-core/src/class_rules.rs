//! What the two conformance classes' reads and writes share: matching a tag
//! of their tables against the tree, reading a node of the expected tag, and
//! the base dates of a workbook's two date systems (ECMA-376 Part 1,
//! 18.17.4.1).

use core::cmp::Ordering;

use officekit_xml::element::Element;
use officekit_xml::error::ReadError;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{NamespaceId, QName, tag_of};

use crate::markup::{self, Tag};

/// Days from 1970-01-01 to the 1900 date system's base date, 1899-12-30.
pub const BASE_1900: i64 = -25_569;

/// Days from 1970-01-01 to the 1904 date system's base date, 1904-01-01.
pub const BASE_1904: i64 = -24_107;

/// Whether `name` is the tag `tag`.
pub fn is(tag: Tag, name: &QName) -> bool {
    tag_of(name).is_some_and(|found| order(tag, found) == Ordering::Equal)
}

/// How a table's tag sorts against a tag of the tree.
pub fn order(table: Tag, found: (Option<NamespaceId>, &str)) -> Ordering {
    table.0.cmp(&found.0).then_with(|| table.1.cmp(found.1))
}

/// The node of `T` `element` holds, when it is the tag `tag`.
pub fn read<T>(element: &Element, tag: Tag, reader: fn(&Element) -> T) -> Result<T, ReadError> {
    if markup::name(element).is_some_and(|found| order(tag, found) == Ordering::Equal) {
        return Ok(reader(element));
    }
    let expected = tag.0.map_or_else(
        || tag.1.to_owned(),
        |id| {
            let prefix = NAMESPACES.get(usize::from(id)).map_or("", |row| row.prefix);
            format!("{prefix}:{}", tag.1)
        },
    );
    let found = if element.name.prefix.is_empty() {
        element.name.local.clone()
    } else {
        format!("{}:{}", element.name.prefix, element.name.local)
    };
    Err(ReadError::UnexpectedTag { expected, found })
}
