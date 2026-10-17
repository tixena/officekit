//! A removed comment leaves the parts that record it: the comments part
//! (ECMA-376 Part 1, 17.13.4) and the commentsExtended, commentsIds, and
//! commentsExtensible parts ([MS-DOCX] 2.3).

use alloc::collections::{BTreeMap, BTreeSet};

use officekit_xml::element::{Element, Node};

use super::{STORE, is, prefix};
use crate::error::PackageError;
use crate::packaging::package::{OpenOptions, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{RelationshipType, Target};

#[cfg(test)]
mod tests;

/// The comments part: `w:comment` by `w:id`.
const COMMENTS: Entries = Entries {
    entry: ("w", "comment"),
    key: ("w", "id"),
    relationship: RelationshipType {
        strict: "http://purl.oclc.org/ooxml/officeDocument/relationships/comments",
        transitional: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
    },
};

/// The commentsExtensible part: `w16cex:commentExtensible` by the durable id.
const COMMENTS_EXTENSIBLE: Entries = Entries {
    entry: ("w16cex", "commentExtensible"),
    key: ("w16cex", "durableId"),
    relationship: RelationshipType {
        strict: "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible",
        transitional: "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible",
    },
};

/// The commentsExtended part: `w15:commentEx` by the paragraph id.
const COMMENTS_EXTENDED: Entries = Entries {
    entry: ("w15", "commentEx"),
    key: ("w15", "paraId"),
    relationship: RelationshipType {
        strict: "http://schemas.microsoft.com/office/2011/relationships/commentsExtended",
        transitional: "http://schemas.microsoft.com/office/2011/relationships/commentsExtended",
    },
};

/// The commentsIds part: `w16cid:commentId` by the paragraph id.
const COMMENTS_IDS: Entries = Entries {
    entry: ("w16cid", "commentId"),
    key: ("w16cid", "paraId"),
    relationship: RelationshipType {
        strict: "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds",
        transitional: "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds",
    },
};

/// A part that records each comment in a child of its root.
struct Entries {
    /// The child, by the namespace table's prefix and its local name.
    entry: (&'static str, &'static str),
    /// The attribute of the child that names the comment.
    key: (&'static str, &'static str),
    /// How the part that owns the comments relates this part.
    relationship: RelationshipType,
}

/// The value of an attribute named `local` in the namespace the namespace
/// table names `table_prefix`.
fn attribute<'element>(
    element: &'element Element,
    table_prefix: &str,
    local: &str,
) -> Option<&'element str> {
    element
        .attributes
        .iter()
        .find(|attribute| {
            attribute.name.local == local && prefix(&attribute.name.namespace) == Some(table_prefix)
        })
        .map(|attribute| attribute.value.as_str())
}

/// The `w14:paraId` of the last paragraph of a `w:comment`.
fn last_paragraph(comment: &Element) -> Option<String> {
    comment
        .child_elements()
        .filter(|child| is(child, "w", "p"))
        .last()
        .and_then(|paragraph| attribute(paragraph, "w14", "paraId"))
        .map(ToOwned::to_owned)
}

/// The part whose comments part records the comments of `part`: `part`
/// itself when it relates one, else a part relating `part` that does, as
/// the main document relates a header.
fn owner(package: &Package, part: &PartName) -> Option<PartName> {
    if related(package, part, &COMMENTS.relationship).is_some() {
        return Some(part.clone());
    }
    let folded = part.folded();
    package
        .relationship_sets()
        .into_iter()
        .filter_map(|(source, _)| source)
        .find(|source| {
            related(package, source, &COMMENTS.relationship).is_some()
                && package
                    .relationships_of(Some(source))
                    .into_iter()
                    .flatten()
                    .any(|relation| {
                        matches!(&relation.target, Target::Part(target) if target.folded() == folded)
                    })
        })
}

/// The first part `source` relates by `relationship`; a target that is no
/// valid part name names no part.
fn related(
    package: &Package,
    source: &PartName,
    relationship: &RelationshipType,
) -> Option<PartName> {
    package
        .relationships_of(Some(source))
        .into_iter()
        .flatten()
        .find_map(|relation| {
            if let Target::Part(target) = relation.target
                && relationship.matches(&relation.type_uri)
            {
                Some(target)
            } else {
                None
            }
        })
}

/// Remove the comments `found` names, each part's `w:id`s, from the comments
/// parts of the part that owns them; the parts stored back.
pub(super) fn remove(
    package: &mut Package,
    found: Vec<(PartName, BTreeSet<String>)>,
) -> Result<Vec<PartName>, PackageError> {
    let mut by_owner: BTreeMap<String, (PartName, BTreeSet<String>)> = BTreeMap::new();
    for (part, ids) in found {
        if let Some(owner_part) = owner(package, &part) {
            by_owner
                .entry(owner_part.folded())
                .or_insert_with(|| (owner_part, BTreeSet::new()))
                .1
                .extend(ids);
        }
    }
    let mut stored = Vec::new();
    for (owner_part, ids) in by_owner.into_values() {
        let mut remove_from = |entries: &Entries, keys: &BTreeSet<String>| {
            remove_entries(package, &owner_part, entries, keys, &mut stored)
        };
        let paragraphs: BTreeSet<String> = remove_from(&COMMENTS, &ids)?
            .iter()
            .filter_map(last_paragraph)
            .collect();
        remove_from(&COMMENTS_EXTENDED, &paragraphs)?;
        let durable: BTreeSet<String> = remove_from(&COMMENTS_IDS, &paragraphs)?
            .iter()
            .filter_map(|entry| attribute(entry, "w16cid", "durableId"))
            .map(ToOwned::to_owned)
            .collect();
        remove_from(&COMMENTS_EXTENSIBLE, &durable)?;
    }
    Ok(stored)
}

/// Remove every entry whose key is in `keys` from the part `owner_part`
/// relates as `entries`, when it has one, and store the part back when one
/// went; the entries removed.
fn remove_entries(
    package: &mut Package,
    owner_part: &PartName,
    entries: &Entries,
    keys: &BTreeSet<String>,
    stored: &mut Vec<PartName>,
) -> Result<Vec<Element>, PackageError> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let Some(part) = related(package, owner_part, &entries.relationship) else {
        return Ok(Vec::new());
    };
    let Some(content_type) = package.content_type(&part).map(str::to_owned) else {
        return Ok(Vec::new());
    };
    if !package.contains(&part) {
        return Ok(Vec::new());
    }
    let mut document = package.load_tree(&part, OpenOptions::default())?;
    let (entry_prefix, entry_local) = entries.entry;
    let (key_prefix, key_local) = entries.key;
    let removed: Vec<Element> = document
        .root
        .children
        .extract_if(.., |node| {
            matches!(node, Node::Element(entry)
                if is(entry, entry_prefix, entry_local)
                    && attribute(entry, key_prefix, key_local).is_some_and(|key| keys.contains(key)))
        })
        .filter_map(|node| {
            if let Node::Element(entry) = node {
                Some(entry)
            } else {
                None
            }
        })
        .collect();
    if !removed.is_empty() {
        package.store_tree(&part, &content_type, &document, STORE)?;
        stored.push(part);
    }
    Ok(removed)
}
