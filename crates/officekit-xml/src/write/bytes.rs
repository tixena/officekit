//! The namespace cleanup over a part's bytes, with no tree: what [`super::clean_namespaces`] does
//! to the tree, done to the bytes the compact layout writes of it.
//!
//! Each round reads the bytes twice as events. The first read finds the declarations nothing in
//! their scope uses, as `strip_unused` does; the second finds the start tags that lose a
//! declaration or an `mc` list entry, as `drop_list_entries` does, and those tags are written
//! again in place. Rounds repeat until one removes nothing. A part where an `mc` list names a
//! prefix that two open elements declare, whose answer depends on the inner declaration staying,
//! is parsed and cleaned as a tree instead.

use alloc::borrow::Cow;
use alloc::collections::{BTreeMap, BTreeSet};
use core::str;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::{PREFIX_LISTS, QUALIFIED_LISTS, WriteOptions, XML_NAMESPACE, escape};
use crate::mc::is_mc;
use crate::name::Namespace;
use crate::parse::{parse, split_qualified};

#[cfg(test)]
mod tests;

/// One declaration of an open element, as the first read of a round sees it.
struct Declared {
    /// Whether the element or one below it holds an item of its namespace.
    item: bool,
    /// Whether an `mc` list names its prefix where it is the declaration that counts.
    named: bool,
    /// The namespace its URI names.
    namespace: Namespace,
    prefix: String,
    /// Whether a name, an attribute or a `Requires` in its scope uses its prefix.
    used: bool,
}

/// An open element of the first read: its index in document order and its declarations.
struct Frame {
    declarations: Vec<Declared>,
    index: usize,
}

/// What the first read of a round found.
struct Found {
    /// A declaration of the element at the index, unused by any name, with its namespace and
    /// whether an `mc` list needs it.
    candidates: Vec<(usize, String, Namespace, bool)>,
    /// The namespace of every element and prefixed attribute.
    items: BTreeSet<Namespace>,
    /// Whether the answer needs the tree.
    tangled: bool,
}

/// The parts of one start tag the cleanup reads.
struct Tag<'text> {
    /// Each attribute that is not a declaration: its prefix, its local name, and its value.
    attributes: Vec<(&'text str, &'text str, Cow<'text, str>)>,
    /// Each declaration: its prefix and its URI.
    declarations: Vec<(&'text str, Cow<'text, str>)>,
    local: &'text str,
    prefix: &'text str,
}

/// Clean the namespaces of a part's bytes, as the compact layout writes them.
///
/// Every namespace declaration nothing in its scope uses goes, and its prefix leaves the `mc`
/// lists, as [`WriteOptions::namespace_cleanup`] does to the tree before it is written: the bytes
/// come out as the cleaned tree's would. Bytes that are not well-formed stay as they are.
pub fn clean_namespace_bytes(bytes: &mut Vec<u8>) {
    loop {
        let Ok(text) = str::from_utf8(bytes) else {
            return;
        };
        let Some(found) = first_read(text) else {
            return;
        };
        if found.tangled {
            clean_as_tree(bytes);
            return;
        }
        let removed = removed(found);
        if removed.is_empty() {
            return;
        }
        let Some(edits) = second_read(text, &removed) else {
            return;
        };
        apply(bytes, edits);
    }
}

/// Write each edit's bytes in place of its span, every span shorter or as long as what replaces
/// it never, edits in document order.
fn apply(bytes: &mut Vec<u8>, edits: Vec<(usize, usize, Vec<u8>)>) {
    let mut written = 0;
    let mut read = 0;
    for (start, end, replacement) in edits {
        bytes.copy_within(read..start, written);
        written += start - read;
        let at = written;
        written += replacement.len();
        if let Some(target) = bytes.get_mut(at..written) {
            target.copy_from_slice(&replacement);
        }
        read = end;
    }
    let length = bytes.len();
    bytes.copy_within(read..length, written);
    written += length - read;
    bytes.truncate(written);
}

/// Clean the part as a tree and write it back compact, for the case the reads cannot answer.
fn clean_as_tree(bytes: &mut Vec<u8>) {
    if let Ok(mut document) = parse(bytes) {
        document.root.clean_namespaces();
        *bytes = document.to_xml(WriteOptions::new(super::Layout::Compact, false));
    }
}

/// The first read of a round: the declarations no name uses, and the namespaces of the part's
/// items; `None` for bytes that are not well-formed.
fn first_read(text: &str) -> Option<Found> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = false;
    let mut frames: Vec<Frame> = Vec::new();
    let mut found = Found {
        candidates: Vec::new(),
        items: BTreeSet::new(),
        tangled: false,
    };
    let mut index = 0;
    loop {
        match reader.read_event().ok()? {
            Event::Eof => return Some(found),
            Event::End(_) => close(&mut frames, &mut found),
            Event::Empty(start) => {
                open(&start, index, &mut frames, &mut found)?;
                index += 1;
                close(&mut frames, &mut found);
            }
            Event::Start(start) => {
                open(&start, index, &mut frames, &mut found)?;
                index += 1;
            }
            Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::PI(_)
            | Event::Text(_) => {}
        }
    }
}

/// Close the innermost element of the first read: each of its declarations no name used is a
/// candidate.
fn close(frames: &mut Vec<Frame>, found: &mut Found) {
    let Some(frame) = frames.pop() else {
        return;
    };
    for declared in frame.declarations {
        if !declared.used {
            found.candidates.push((
                frame.index,
                declared.prefix,
                declared.namespace,
                declared.named && declared.item,
            ));
        }
    }
}

/// Open an element of the first read: its declarations pushed, and what its name, its
/// attributes and its `mc` lists tell the declarations in scope.
fn open(
    start: &BytesStart<'_>,
    index: usize,
    frames: &mut Vec<Frame>,
    found: &mut Found,
) -> Option<()> {
    let tag = read_tag(start)?;
    let declarations = tag
        .declarations
        .iter()
        .map(|(prefix, uri)| Declared {
            item: false,
            named: false,
            namespace: Namespace::from_uri(uri),
            prefix: (*prefix).to_owned(),
            used: false,
        })
        .collect();
    frames.push(Frame {
        declarations,
        index,
    });
    let name = resolve(frames, tag.prefix);
    let attributes: Vec<(&str, Namespace)> = tag
        .attributes
        .iter()
        .map(|(prefix, _, _)| {
            let namespace = if prefix.is_empty() {
                Namespace::None
            } else {
                resolve(frames, prefix)
            };
            (*prefix, namespace)
        })
        .collect();
    found.items.insert(name.clone());
    for (prefix, namespace) in &attributes {
        if !prefix.is_empty() {
            found.items.insert(namespace.clone());
        }
    }
    let mut uses = vec![tag.prefix];
    uses.extend(
        attributes
            .iter()
            .map(|(prefix, _)| *prefix)
            .filter(|prefix| !prefix.is_empty()),
    );
    if is_mc(&name) && tag.local == "Choice" {
        let requires = tag
            .attributes
            .iter()
            .find(|(prefix, local, _)| prefix.is_empty() && *local == "Requires");
        if let Some((_, _, value)) = requires {
            uses.extend(value.split_whitespace());
        }
    }
    for prefix in uses {
        mark_used(frames, prefix);
    }
    let listed = listed_prefixes(&tag, frames);
    let holds = |namespace: &Namespace| {
        name == *namespace || attributes.iter().any(|(_, held)| held == namespace)
    };
    let own = frames.len() - 1;
    for (depth, frame) in frames.iter_mut().enumerate() {
        for declared in &mut frame.declarations {
            let lists = listed.contains(declared.prefix.as_str());
            if depth == own {
                declared.named = lists;
                declared.item = holds(&declared.namespace);
                continue;
            }
            declared.named |= lists;
            if !declared.item {
                declared.item = holds(&declared.namespace);
            }
        }
    }
    if listed
        .iter()
        .any(|prefix| declaring_frames(frames, prefix) > 1)
    {
        found.tangled = true;
    }
    Some(())
}

/// How many open elements declare `prefix`.
fn declaring_frames(frames: &[Frame], prefix: &str) -> usize {
    frames
        .iter()
        .filter(|frame| {
            frame
                .declarations
                .iter()
                .any(|declared| declared.prefix == prefix)
        })
        .count()
}

/// The prefixes the `mc` lists of a start tag name.
fn listed_prefixes<'text>(tag: &'text Tag<'_>, frames: &[Frame]) -> BTreeSet<&'text str> {
    let mut listed = BTreeSet::new();
    for (prefix, local, value) in &tag.attributes {
        if prefix.is_empty() || !is_mc(&resolve(frames, prefix)) {
            continue;
        }
        let qualified = QUALIFIED_LISTS.contains(local);
        if !qualified && !PREFIX_LISTS.contains(local) {
            continue;
        }
        for item in value.split_whitespace() {
            listed.insert(if qualified {
                split_qualified(item).0
            } else {
                item
            });
        }
    }
    listed
}

/// Mark used the innermost declaration of `prefix`.
fn mark_used(frames: &mut [Frame], prefix: &str) {
    let declared = frames.iter_mut().rev().find_map(|frame| {
        frame
            .declarations
            .iter_mut()
            .rev()
            .find(|declared| declared.prefix == prefix)
    });
    if let Some(found) = declared {
        found.used = true;
    }
}

/// The prefixes the first read of a round removes, by element index: each candidate that no
/// `mc` list needs, or whose namespace no item of the part has.
fn removed(found: Found) -> BTreeMap<usize, BTreeSet<String>> {
    let mut removed: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for (index, prefix, namespace, needed) in found.candidates {
        if !(needed && found.items.contains(&namespace)) {
            removed.entry(index).or_default().insert(prefix);
        }
    }
    removed
}

/// The parts of a start tag; `None` for one that is not well-formed.
fn read_tag<'text>(start: &'text BytesStart<'_>) -> Option<Tag<'text>> {
    let (prefix, local) = split_qualified(start.name().into_inner());
    let mut tag = Tag {
        attributes: Vec::new(),
        declarations: Vec::new(),
        local,
        prefix,
    };
    let mut attributes = start.attributes();
    attributes.with_checks(false);
    for read in attributes {
        let attribute = read.ok()?;
        let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
        let key = attribute.key.into_inner();
        if key == "xmlns" {
            tag.declarations.push(("", value));
        } else if let Some(declared) = key.strip_prefix("xmlns:") {
            tag.declarations.push((declared, value));
        } else {
            let (attribute_prefix, attribute_local) = split_qualified(key);
            tag.attributes
                .push((attribute_prefix, attribute_local, value));
        }
    }
    Some(tag)
}

/// The namespace `prefix` names in scope: the innermost declaration's, `xml`'s without one, and
/// none for an empty prefix nothing declares.
fn resolve(frames: &[Frame], prefix: &str) -> Namespace {
    if prefix == "xml" {
        return Namespace::from_uri(XML_NAMESPACE);
    }
    frames
        .iter()
        .rev()
        .find_map(|frame| {
            frame
                .declarations
                .iter()
                .rev()
                .find(|declared| declared.prefix == prefix)
        })
        .map_or(Namespace::None, |declared| declared.namespace.clone())
}

/// The second read of a round: each start tag that loses a declaration or an `mc` list entry,
/// as its span and the tag written again; `None` for bytes that are not well-formed.
fn second_read(
    text: &str,
    removed: &BTreeMap<usize, BTreeSet<String>>,
) -> Option<Vec<(usize, usize, Vec<u8>)>> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = false;
    // Each open element's declarations: its prefix, its namespace, and whether it goes.
    let mut frames: Vec<Vec<(String, Namespace, bool)>> = Vec::new();
    let mut edits = Vec::new();
    let mut index = 0;
    loop {
        let before = usize::try_from(reader.buffer_position()).ok()?;
        let event = reader.read_event().ok()?;
        let after = usize::try_from(reader.buffer_position()).ok()?;
        let (start, empty) = match event {
            Event::Eof => return Some(edits),
            Event::End(_) => {
                frames.pop();
                continue;
            }
            Event::Empty(start) => (start, true),
            Event::Start(start) => (start, false),
            Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::DocType(_)
            | Event::GeneralRef(_)
            | Event::PI(_)
            | Event::Text(_) => continue,
        };
        let gone = removed.get(&index);
        index += 1;
        let mut declarations = Vec::new();
        let mut lists = false;
        let mut attributes = start.attributes();
        attributes.with_checks(false);
        for read in attributes {
            let attribute = read.ok()?;
            let key = attribute.key.into_inner();
            let declared = if key == "xmlns" {
                Some("")
            } else {
                key.strip_prefix("xmlns:")
            };
            if let Some(prefix) = declared {
                let uri = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
                declarations.push((
                    prefix.to_owned(),
                    Namespace::from_uri(&uri),
                    gone.is_some_and(|prefixes| prefixes.contains(prefix)),
                ));
            } else {
                let (prefix, local) = split_qualified(key);
                lists |= !prefix.is_empty()
                    && (QUALIFIED_LISTS.contains(&local) || PREFIX_LISTS.contains(&local));
            }
        }
        frames.push(declarations);
        if (gone.is_some() || lists)
            && let Some(rewritten) = rewritten(&start, &frames, gone, empty)
        {
            edits.push((before, after, rewritten));
        }
        if empty {
            frames.pop();
        }
    }
}

/// The start tag `start` written again without the declarations of `gone` and with the
/// removed prefixes out of its `mc` lists; `None` when neither changes it.
fn rewritten(
    start: &BytesStart<'_>,
    frames: &[Vec<(String, Namespace, bool)>],
    gone: Option<&BTreeSet<String>>,
    empty: bool,
) -> Option<Vec<u8>> {
    let removed_here = |prefix: &str| {
        frames.iter().rev().find_map(|frame| {
            frame
                .iter()
                .find(|(declared, _, _)| declared == prefix)
                .map(|(_, _, goes)| *goes)
        })
    };
    let in_scope = |prefix: &str| {
        if prefix == "xml" {
            return Namespace::from_uri(XML_NAMESPACE);
        }
        frames
            .iter()
            .rev()
            .find_map(|frame| {
                frame
                    .iter()
                    .rev()
                    .find(|(declared, _, _)| declared == prefix)
            })
            .map_or(Namespace::None, |(_, namespace, _)| namespace.clone())
    };
    let mut changed = false;
    let mut out = String::from("<");
    out.push_str(start.name().into_inner());
    let mut attributes = start.attributes();
    attributes.with_checks(false);
    for read in attributes {
        let attribute = read.ok()?;
        let key = attribute.key.into_inner();
        let declared = if key == "xmlns" {
            Some("")
        } else {
            key.strip_prefix("xmlns:")
        };
        if let Some(prefix) = declared {
            if gone.is_some_and(|prefixes| prefixes.contains(prefix)) {
                changed = true;
                continue;
            }
        } else {
            let (prefix, local) = split_qualified(key);
            let qualified = QUALIFIED_LISTS.contains(&local);
            let list = !prefix.is_empty()
                && (qualified || PREFIX_LISTS.contains(&local))
                && is_mc(&in_scope(prefix));
            if list {
                let value = attribute.normalized_value(XmlVersion::Implicit1_0).ok()?;
                let items: Vec<&str> = value.split_whitespace().collect();
                let kept: Vec<&str> = items
                    .iter()
                    .copied()
                    .filter(|item| {
                        let named = if qualified {
                            split_qualified(item).0
                        } else {
                            item
                        };
                        removed_here(named) != Some(true)
                    })
                    .collect();
                if kept.len() != items.len() {
                    changed = true;
                    let joined = kept.join(" ");
                    if !joined.is_empty() {
                        out.push(' ');
                        out.push_str(key);
                        out.push_str("=\"");
                        out.push_str(&escape(&joined, true));
                        out.push('"');
                    }
                    continue;
                }
            }
        }
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(&attribute.value);
        out.push('"');
    }
    out.push_str(if empty { "/>" } else { ">" });
    changed.then(|| out.into_bytes())
}
