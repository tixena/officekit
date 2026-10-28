//! What the Strict and Transitional classes share when they read and write a node.
//!
//! The class crates (`officekit-strict` and `officekit-transitional`) match a tag of their
//! generated tables against a name in the tree with [`is`] and [`order`], and read a node only
//! when its element has the tag the node expects with [`read`]. [`BASE_1900`] and [`BASE_1904`]
//! are the base dates of the two date systems of a workbook (ECMA-376 Part 1, 18.17.4.1).

use core::cmp::Ordering;

use officekit_xml::element::Element;
use officekit_xml::error::ReadError;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{NamespaceId, QName, tag_of};

use crate::Tag;

/// Days from 1970-01-01 to the 1900 date system's base date, 1899-12-30.
pub const BASE_1900: i64 = -25_569;

/// Days from 1970-01-01 to the 1904 date system's base date, 1904-01-01.
pub const BASE_1904: i64 = -24_107;

/// Whether `name` is the tag `tag`: the same namespace and the same local name, in either
/// class's spelling of the namespace.
#[must_use]
pub fn is(tag: Tag, name: &QName) -> bool {
    tag_of(name).is_some_and(|found| order(tag, found) == Ordering::Equal)
}

/// How a table's tag sorts against a tag of the tree: by namespace id, then by local name.
///
/// The generated tables are sorted this way, so a binary search finds a tag with it.
#[must_use]
pub fn order(table: Tag, found: (Option<NamespaceId>, &str)) -> Ordering {
    table.0.cmp(&found.0).then_with(|| table.1.cmp(found.1))
}

/// The node of `T` that `element` holds, when it has the tag `tag`.
///
/// `reader` is the generated `read::<node>` function of the node.
///
/// # Errors
///
/// `ReadError::UnexpectedTag` when `element` is another tag.
pub fn read<T>(element: &Element, tag: Tag, reader: fn(&Element) -> T) -> Result<T, ReadError> {
    if crate::name(element).is_some_and(|found| order(tag, found) == Ordering::Equal) {
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
