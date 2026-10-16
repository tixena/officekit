//! Run compaction: under every element of a part's generic tree, each stretch
//! of neighbouring runs that render the same becomes one run.
//!
//! Two runs merge when both hold only their properties element and text
//! content, and the run properties that count are equal. Between them may sit
//! white space and `w:proofErr`, and under [`Comments::Remove`] the nodes of a
//! Word comment; every other node ends the stretch. The merged run is the
//! first run followed by the content of the others, adjacent text elements
//! joined into one.

mod properties;

use alloc::collections::BTreeSet;
use core::mem;

use properties::{Counted, DRAWING, SPREADSHEET, WORDPROCESSING};

use crate::xml::element::{Attribute, Element, Node};
use crate::xml::mc::with_declarations;
use crate::xml::name::table::NAMESPACES;
use crate::xml::name::{Namespace, QName};
use crate::xml::write::{is_whitespace_text, preserves_space};

#[cfg(test)]
mod tests;

/// The namespace the `xml` prefix is bound to.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// What a run compaction does with a Word comment that sits between two runs
/// it would merge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Comments {
    /// The comment stays, and its nodes keep the runs on either side apart.
    Keep,
    /// The comment goes: every node of it in the tree, and its id is reported.
    Remove,
}

/// A run kind, by the namespace of its `r`, `rPr`, and text elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `a:r`.
    Drawing,
    /// `x:r` of rich text.
    Spreadsheet,
    /// `w:r`.
    Wordprocessing,
}

/// What compacting one part's tree did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartCompaction {
    /// Runs merged into the run before them.
    pub merged: usize,
    /// The `w:id` of every comment removed, for the comments parts.
    pub removed_comments: BTreeSet<String>,
}

/// What a node is to the stretch of runs it sits in.
enum Role {
    /// It ends the stretch.
    Barrier(Node),
    /// It goes when the runs on both sides merge; a comment's node carries
    /// the comment's `w:id`.
    Between(Node, Option<String>),
    /// A run that can merge.
    Run(Element, Kind),
}

impl Kind {
    /// The text content a mergeable run may hold besides its properties.
    const fn content(self) -> &'static [&'static str] {
        match self {
            Self::Drawing | Self::Spreadsheet => &["t"],
            Self::Wordprocessing => &[
                "br",
                "cr",
                "delText",
                "lastRenderedPageBreak",
                "noBreakHyphen",
                "softHyphen",
                "t",
                "tab",
            ],
        }
    }

    const fn counted(self) -> &'static Counted {
        match self {
            Self::Drawing => &DRAWING,
            Self::Spreadsheet => &SPREADSHEET,
            Self::Wordprocessing => &WORDPROCESSING,
        }
    }

    /// The kind of `element` when it is a run.
    fn of(element: &Element) -> Option<Self> {
        if element.name.local != "r" {
            return None;
        }
        let table_prefix = prefix(&element.name.namespace)?;
        if table_prefix == "a" {
            Some(Self::Drawing)
        } else if table_prefix == "x" {
            Some(Self::Spreadsheet)
        } else if table_prefix == "w" {
            Some(Self::Wordprocessing)
        } else {
            None
        }
    }

    /// The namespace table's prefix of the kind's namespace.
    const fn prefix(self) -> &'static str {
        match self {
            Self::Drawing => "a",
            Self::Spreadsheet => "x",
            Self::Wordprocessing => "w",
        }
    }

    /// Whether `xml:space` decides the white space of the kind's text.
    const fn spaces(self) -> bool {
        match self {
            Self::Drawing => false,
            Self::Spreadsheet | Self::Wordprocessing => true,
        }
    }

    /// The text elements, whose neighbours of the same name join.
    const fn text(self) -> &'static [&'static str] {
        match self {
            Self::Drawing | Self::Spreadsheet => &["t"],
            Self::Wordprocessing => &["delText", "t"],
        }
    }
}

/// Append the content of `next` to `first`, a run of the same kind whose
/// properties count the same; `around` is whether `xml:space="preserve"` is
/// in force around both.
fn absorb(first: &mut Element, next: Element, kind: Kind, around: bool) {
    let first_preserves = preserves_space(first, around);
    let next_preserves = preserves_space(&next, around);
    for node in next.children {
        // The run holds white space besides its elements, which goes.
        let Node::Element(mut content) = node else {
            continue;
        };
        if is(&content, kind.prefix(), "rPr") {
            continue;
        }
        if kind.spaces() && first_preserves != next_preserves {
            keep_space(&mut content, kind, next_preserves);
        }
        if let Some(Node::Element(last)) = first
            .children
            .iter_mut()
            .rev()
            .find(|child| !is_whitespace_text(child))
            && join(last, &content, kind, first_preserves, next_preserves)
        {
            continue;
        }
        first.children.push(with_declarations(
            Node::Element(content),
            &next.namespace_declarations,
        ));
    }
}

/// The `w:id` of a comment's range start or end, or of the comment whose
/// reference is a run's only content.
fn comment_id(element: &Element) -> Option<String> {
    if is(element, "w", "commentRangeStart") || is(element, "w", "commentRangeEnd") {
        return word_id(element).map(ToOwned::to_owned);
    }
    if !is(element, "w", "r") {
        return None;
    }
    let mut id = None;
    for node in &element.children {
        if let Node::Element(child) = node {
            if is(child, "w", "rPr") {
                continue;
            }
            if id.is_none() && is(child, "w", "commentReference") {
                id = Some(word_id(child)?);
                continue;
            }
            return None;
        }
        if !is_whitespace_text(node) {
            return None;
        }
    }
    id.map(ToOwned::to_owned)
}

/// Compact the children of `element`, then the subtree of each child;
/// `around` is whether `xml:space="preserve"` is in force around `element`.
fn compact(element: &mut Element, around: bool, comments: Comments, done: &mut PartCompaction) {
    let preserve = preserves_space(element, around);
    merge_children(element, preserve, comments, done);
    for child in &mut element.children {
        if let Node::Element(child_element) = child {
            compact(child_element, preserve, comments, done);
        }
    }
}

/// Merge, under every element of `root`, each stretch of neighbouring runs
/// that render the same.
pub fn compact_tree(root: &mut Element, comments: Comments) -> PartCompaction {
    let mut done = PartCompaction::default();
    compact(root, false, comments, &mut done);
    if !done.removed_comments.is_empty() {
        remove_comments(root, &done.removed_comments);
    }
    done
}

/// Whether `element` is named `local` in the namespace the namespace table
/// names `table_prefix`.
fn is(element: &Element, table_prefix: &str, local: &str) -> bool {
    element.name.local == local && prefix(&element.name.namespace) == Some(table_prefix)
}

/// Join `next` onto `last` when both are text elements of the same name
/// whose white space renders the same either way; whether it did. Each side
/// is read with `xml:space` as in force in its own run.
fn join(
    last: &mut Element,
    next: &Element,
    kind: Kind,
    last_preserves: bool,
    next_preserves: bool,
) -> bool {
    if !kind.text().contains(&last.name.local.as_str())
        || prefix(&last.name.namespace) != Some(kind.prefix())
        || last.name.local != next.name.local
        || last.name.namespace != next.name.namespace
    {
        return false;
    }
    let (Some(mut joined), Some(more)) = (
        text_of(last, kind, last_preserves),
        text_of(next, kind, next_preserves),
    ) else {
        return false;
    };
    joined.push_str(&more);
    if kind.spaces() {
        last.attributes
            .retain(|attribute| !is_space(&attribute.name));
        if joined.starts_with(is_xml_whitespace) || joined.ends_with(is_xml_whitespace) {
            last.attributes.push(space_attribute("preserve"));
        }
    }
    last.children = if joined.is_empty() {
        Vec::new()
    } else {
        vec![Node::Text(joined)]
    };
    true
}

/// Give a text element moved out of a run the `xml:space` it had there,
/// `preserved`, when the run it moves into would read its white space
/// otherwise.
fn keep_space(content: &mut Element, kind: Kind, preserved: bool) {
    let edged = content.children.iter().any(|node| {
        if let Node::Text(text) = node {
            text.starts_with(is_xml_whitespace) || text.ends_with(is_xml_whitespace)
        } else {
            false
        }
    });
    let own = content
        .attributes
        .iter()
        .any(|attribute| is_space(&attribute.name));
    if edged && !own && kind.text().contains(&content.name.local.as_str()) {
        content.attributes.push(space_attribute(if preserved {
            "preserve"
        } else {
            "default"
        }));
    }
}

/// Whether a run of `kind` holds its properties element, at most one, and
/// only text content besides; an `x:r` also needs its `x:t`, which sets a
/// rich text run apart from a pivot cache record, also `x:r`.
fn mergeable(run: &Element, kind: Kind) -> bool {
    let mut properties = 0_usize;
    let mut holds_text = false;
    for node in &run.children {
        if let Node::Element(child) = node {
            if is(child, kind.prefix(), "rPr") {
                properties += 1;
                continue;
            }
            if prefix(&child.name.namespace) == Some(kind.prefix())
                && kind.content().contains(&child.name.local.as_str())
            {
                holds_text = holds_text || child.name.local == "t";
                continue;
            }
            return false;
        }
        if !is_whitespace_text(node) {
            return false;
        }
    }
    properties <= 1 && (holds_text || kind != Kind::Spreadsheet)
}

/// Merge the stretches of runs among the children of `element`, with
/// `preserve` telling whether `xml:space="preserve"` is in force there.
fn merge_children(
    element: &mut Element,
    preserve: bool,
    comments: Comments,
    done: &mut PartCompaction,
) {
    let mut kept: Vec<Node> = Vec::with_capacity(element.children.len());
    // The run the stretch merges into, by its index in `kept`.
    let mut anchor: Option<(usize, Kind)> = None;
    let mut between: Vec<Node> = Vec::new();
    let mut between_comments: Vec<String> = Vec::new();
    for node in mem::take(&mut element.children) {
        match role(node, comments) {
            Role::Run(run, kind) => {
                let first =
                    anchor
                        .filter(|&(_, anchored)| anchored == kind)
                        .and_then(|(index, _)| {
                            if let Some(Node::Element(first_run)) = kept.get_mut(index) {
                                Some(first_run)
                            } else {
                                None
                            }
                        });
                if let Some(first_run) = first
                    && kind
                        .counted()
                        .equal(properties(first_run, kind), properties(&run, kind))
                {
                    absorb(first_run, run, kind, preserve);
                    between.clear();
                    done.removed_comments
                        .extend(mem::take(&mut between_comments));
                    done.merged += 1;
                } else {
                    kept.append(&mut between);
                    between_comments.clear();
                    anchor = Some((kept.len(), kind));
                    kept.push(Node::Element(run));
                }
            }
            Role::Between(node_between, id) => {
                if anchor.is_some() {
                    between.push(node_between);
                    between_comments.extend(id);
                } else {
                    kept.push(node_between);
                }
            }
            Role::Barrier(barrier) => {
                kept.append(&mut between);
                between_comments.clear();
                anchor = None;
                kept.push(barrier);
            }
        }
    }
    kept.append(&mut between);
    element.children = kept;
}

/// The prefix the namespace table gives `namespace`, when it knows it.
fn prefix(namespace: &Namespace) -> Option<&'static str> {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES.get(usize::from(id)).map(|row| row.prefix)
    } else {
        None
    }
}

/// The properties element of a run.
fn properties(run: &Element, kind: Kind) -> Option<&Element> {
    run.child_elements()
        .find(|child| is(child, kind.prefix(), "rPr"))
}

/// Remove every node of the comments in `ids`: their range starts and ends,
/// and their references, with the run holding one when nothing else is left
/// in it but its properties.
fn remove_comments(element: &mut Element, ids: &BTreeSet<String>) {
    element.children.retain_mut(|node| {
        let Node::Element(child) = node else {
            return true;
        };
        if (is(child, "w", "commentRangeStart") || is(child, "w", "commentRangeEnd"))
            && word_id(child).is_some_and(|id| ids.contains(id))
        {
            return false;
        }
        if is(child, "w", "r") {
            let held = child.children.len();
            child.children.retain(|content| {
                !matches!(content, Node::Element(reference)
                    if is(reference, "w", "commentReference")
                        && word_id(reference).is_some_and(|id| ids.contains(id)))
            });
            let emptied = child.children.iter().all(|content| {
                is_whitespace_text(content)
                    || matches!(content, Node::Element(properties) if is(properties, "w", "rPr"))
            });
            if child.children.len() != held && emptied {
                return false;
            }
        }
        remove_comments(child, ids);
        true
    });
}

/// What `node` is to the stretch it sits in, with `comments` deciding a
/// comment's nodes.
fn role(node: Node, comments: Comments) -> Role {
    let Node::Element(element) = node else {
        return if is_whitespace_text(&node) {
            Role::Between(node, None)
        } else {
            Role::Barrier(node)
        };
    };
    if let Some(kind) = Kind::of(&element)
        && mergeable(&element, kind)
    {
        return Role::Run(element, kind);
    }
    if is(&element, "w", "proofErr") {
        return Role::Between(Node::Element(element), None);
    }
    if comments == Comments::Remove
        && let Some(id) = comment_id(&element)
    {
        return Role::Between(Node::Element(element), Some(id));
    }
    Role::Barrier(Node::Element(element))
}

fn is_space(name: &QName) -> bool {
    name.prefix == "xml" && name.local == "space"
}

const fn is_xml_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

/// An `xml:space` attribute.
fn space_attribute(value: &str) -> Attribute {
    Attribute {
        name: QName {
            local: "space".to_owned(),
            namespace: Namespace::from_uri(XML_NAMESPACE),
            prefix: "xml".to_owned(),
        },
        value: value.to_owned(),
    }
}

/// The text of a text element as it renders, when it holds only text and no
/// attribute but `xml:space`, and its white space renders the same whether
/// or not the consumer trims it; `around` is whether `xml:space="preserve"`
/// is in force around it.
fn text_of(element: &Element, kind: Kind, around: bool) -> Option<String> {
    if element
        .attributes
        .iter()
        .any(|attribute| !kind.spaces() || !is_space(&attribute.name))
    {
        return None;
    }
    let mut text = String::new();
    for node in &element.children {
        if let Node::Text(piece) = node {
            text.push_str(piece);
        } else {
            return None;
        }
    }
    let edged = text.starts_with(is_xml_whitespace) || text.ends_with(is_xml_whitespace);
    if kind.spaces() && edged && !preserves_space(element, around) {
        return None;
    }
    Some(text)
}

/// The `w:id` of an element.
fn word_id(element: &Element) -> Option<&str> {
    element
        .attributes
        .iter()
        .find(|attribute| {
            attribute.name.local == "id" && prefix(&attribute.name.namespace) == Some("w")
        })
        .map(|attribute| attribute.value.as_str())
}
