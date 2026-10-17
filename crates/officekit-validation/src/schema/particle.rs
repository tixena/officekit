//! A content model matched against an element's children.
//!
//! The model at one target meets the children in order, as SDK 3.5.1's
//! particle validators match them, and a failed match gives the one error SDK
//! 3.5.1 reports for it.

use alloc::borrow::ToOwned as _;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, OfficeVersion, QName};

use super::types::{AnyNamespace, CompositeKind, Particle, SchemaType, TypeId};
use crate::error::ErrorCode;
use crate::generated::SCHEMA_TYPES;

#[cfg(test)]
mod tests;

/// One child the content model meets.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Child<'element> {
    pub name: &'element QName,
    /// The namespace of the element the child sits in, as the file holds it:
    /// an `mc:Choice` when the child sits in one.
    pub parent_namespace: &'element Namespace,
    /// `None` for an element the parent cannot hold as a typed child.
    pub type_id: Option<TypeId>,
}

impl<'element> Child<'element> {
    #[must_use]
    pub const fn new(
        name: &'element QName,
        parent_namespace: &'element Namespace,
        type_id: Option<TypeId>,
    ) -> Self {
        Self {
            name,
            parent_namespace,
            type_id,
        }
    }
}

/// A child the content model expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expected {
    Any(AnyNamespace),
    Element(TypeId),
}

/// What a match of children against a particle gives.
#[derive(Clone, Debug, Default)]
struct Info {
    expected: Vec<Expected>,
    /// The last child matched.
    last: Option<usize>,
    outcome: Outcome,
}

/// Why an element's children do not follow its content model.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Mismatch {
    /// The child the error is about, by index; `None` for a child missing.
    pub child: Option<usize>,
    pub code: ErrorCode,
    /// The children the model allows there, `", expected w:pPr or w:r"`, or
    /// nothing.
    pub expected: String,
}

/// How far a particle matches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Outcome {
    Matched,
    #[default]
    NoMatch,
    /// Some children matched, and the particle wants more.
    Partial,
}

/// The element children of an `xsd:all`, with their minimum.
fn all_items(items: &[Particle]) -> Vec<(TypeId, u32)> {
    items
        .iter()
        .filter_map(|item| {
            if let Particle::Element {
                max: _max,
                min,
                type_id,
            } = *item
            {
                Some((type_id, min))
            } else {
                None
            }
        })
        .collect()
}

/// An `xsd:all` once: each of its element children at most once, in any order.
fn all_once(items: &[Particle], children: &[Child<'_>], start: usize) -> Info {
    let declared = all_items(items);
    let mut seen: Vec<TypeId> = Vec::new();
    let mut last = None;
    for (index, child) in children.iter().enumerate().skip(start) {
        let Some(type_id) = child
            .type_id
            .filter(|id| declared.iter().any(|(own, _)| own == id))
        else {
            break;
        };
        if seen.contains(&type_id) {
            break;
        }
        seen.push(type_id);
        last = Some(index);
    }
    if last.is_none() {
        return Info {
            expected: declared
                .iter()
                .map(|(type_id, _)| Expected::Element(*type_id))
                .collect(),
            last,
            outcome: Outcome::NoMatch,
        };
    }
    let missing = declared
        .iter()
        .any(|(type_id, min)| *min == 1 && !seen.contains(type_id));
    Info {
        expected: declared
            .iter()
            .filter(|(type_id, _)| !seen.contains(type_id))
            .map(|(type_id, _)| Expected::Element(*type_id))
            .collect(),
        last,
        outcome: if missing {
            Outcome::Partial
        } else {
            Outcome::Matched
        },
    }
}

/// The error of an `xsd:all` content model, at the child after the last
/// matched; none when every child matched.
fn all_mismatch(
    particle: &Particle,
    info: Info,
    child: Option<usize>,
    children: &[Child<'_>],
    parent: &SchemaType,
) -> Option<Mismatch> {
    let index = child?;
    let in_group = children
        .get(index)
        .and_then(|found| found.type_id)
        .is_some_and(|type_id| {
            if let Particle::Composite {
                items,
                kind: _kind,
                max: _max,
                min: _min,
            } = *particle
            {
                all_items(items).iter().any(|(own, _)| *own == type_id)
            } else {
                false
            }
        });
    if info.outcome != Outcome::NoMatch && in_group {
        return Some(Mismatch {
            child: Some(index),
            code: ErrorCode::RepeatedChildInAllGroup,
            expected: String::new(),
        });
    }
    let expected = if info.outcome == Outcome::NoMatch {
        expected_children(particle)
    } else {
        info.expected
    };
    Some(Mismatch {
        child: Some(index),
        code: ErrorCode::InvalidChild,
        expected: expected_text(parent, &expected),
    })
}

/// Whether a wildcard takes `child`.
fn any_takes(namespace: AnyNamespace, child: &Child<'_>) -> bool {
    match namespace {
        AnyNamespace::Any => true,
        AnyNamespace::Local => child.name.namespace == Namespace::None,
        AnyNamespace::Other => {
            child.name.namespace == Namespace::None
                || child.name.namespace != *child.parent_namespace
        }
        AnyNamespace::TargetNamespace => child.name.namespace == *child.parent_namespace,
        // SDK 3.5.1's wildcards name no namespace URI, so such a wildcard takes nothing.
        AnyNamespace::Uri(_) => false,
    }
}

/// A wildcard as many times as it may occur, from the child at `start`.
fn any_match(
    max: u32,
    min: u32,
    namespace: AnyNamespace,
    children: &[Child<'_>],
    start: usize,
) -> Info {
    let takes = |index: usize| {
        children
            .get(index)
            .is_some_and(|child| any_takes(namespace, child))
    };
    let mut count = 0;
    let mut index = start;
    while (max == 0 || max > count) && takes(index) {
        count += 1;
        index += 1;
    }
    let outcome = if count == 0 {
        Outcome::NoMatch
    } else if count >= min || max == 1 {
        Outcome::Matched
    } else {
        Outcome::Partial
    };
    Info {
        expected: Vec::new(),
        last: (count > 0).then(|| index - 1),
        outcome,
    }
}

/// Whether the parent's own children include `child`'s type: the child is in
/// the parent's vocabulary, only not where the model wants it.
fn can_contain(parent: &SchemaType, child: &Child<'_>) -> bool {
    child
        .type_id
        .is_some_and(|type_id| parent.children.iter().any(|(_, _, id, _)| *id == type_id))
}

/// Check `children` against `particle`, the content model of `parent` at `target`.
///
/// `None` when they follow it, else the one error SDK 3.5.1 reports: a
/// required child missing, a child the parent does not allow there, or a
/// child of an `xsd:all` repeated.
#[must_use]
pub fn check(
    particle: &Particle,
    children: &[Child<'_>],
    parent: TypeId,
    target: OfficeVersion,
) -> Option<Mismatch> {
    let parent_type = SCHEMA_TYPES.get(usize::from(parent))?;
    let Particle::Composite {
        items: _items,
        kind,
        max,
        min,
    } = *particle
    else {
        return None;
    };
    if children.is_empty() {
        if min == 0 {
            return None;
        }
        let mut required = Vec::new();
        required_children(particle, Some(&mut required));
        return (!required.is_empty()).then(|| Mismatch {
            child: None,
            code: ErrorCode::MissingChild,
            expected: expected_text(parent_type, &required),
        });
    }
    let info = try_match(particle, children, 0);
    if info.outcome == Outcome::Matched && next(info.last, children).is_none() {
        return None;
    }
    let child = info.last.map_or(Some(0), |last| next(Some(last), children));
    if kind == CompositeKind::All {
        return all_mismatch(particle, info, child, children, parent_type);
    }
    if info.outcome == Outcome::Partial && child.is_none() {
        return Some(Mismatch {
            child: None,
            code: ErrorCode::MissingChild,
            expected: expected_text(parent_type, &info.expected),
        });
    }
    let repeats = max == 0 || max > 1;
    let expected =
        if info.outcome == Outcome::NoMatch || (info.outcome == Outcome::Matched && repeats) {
            expected_children(particle)
        } else {
            info.expected
        };
    let index = child?;
    let found = children.get(index)?;
    let code = if can_contain(parent_type, found) {
        ErrorCode::UnexpectedChild
    } else if holds_name_of_another_type(parent_type, found, target) {
        ErrorCode::ChildOfWrongType
    } else {
        ErrorCode::InvalidChild
    };
    Some(Mismatch {
        child: Some(index),
        code,
        expected: expected_text(parent_type, &expected),
    })
}

/// A choice once: the first alternative that matches.
fn choice_once(items: &[Particle], children: &[Child<'_>], start: usize) -> Info {
    for item in items {
        let found = try_match(item, children, start);
        match found.outcome {
            Outcome::Matched => {
                return Info {
                    expected: Vec::new(),
                    ..found
                };
            }
            Outcome::NoMatch => {}
            Outcome::Partial => return found,
        }
    }
    Info::default()
}

/// A composite particle once.
fn composite_once(
    kind: CompositeKind,
    items: &[Particle],
    children: &[Child<'_>],
    start: usize,
) -> Info {
    match kind {
        CompositeKind::All => all_once(items, children, start),
        CompositeKind::Choice => choice_once(items, children, start),
        CompositeKind::Group => items
            .first()
            .map_or_else(Info::default, |item| try_match(item, children, start)),
        CompositeKind::Sequence => sequence_once(items, children, start),
    }
}

/// A composite particle that may occur more than once, from the child at `start`.
fn composite_repeated(particle: &Particle, children: &[Child<'_>], start: usize) -> Info {
    let Particle::Composite {
        items,
        kind,
        max,
        min,
    } = *particle
    else {
        return Info::default();
    };
    let mut info = Info::default();
    let mut count = 0;
    let mut position = Some(start);
    while let Some(at) = position
        && (max == 0 || max > count)
    {
        let found = composite_once(kind, items, children, at);
        match found.outcome {
            Outcome::NoMatch => break,
            Outcome::Matched => {
                count += 1;
                info.last = found.last;
                position = next(found.last, children);
            }
            Outcome::Partial => {
                return Info {
                    outcome: Outcome::Partial,
                    ..found
                };
            }
        }
    }
    if count == 0 {
        info.expected = expected_children(particle);
        info.outcome = Outcome::NoMatch;
    } else if count >= min {
        info.outcome = Outcome::Matched;
    } else if required_children(particle, Some(&mut info.expected)) {
        info.outcome = Outcome::Partial;
    } else {
        info.outcome = Outcome::Matched;
    }
    info
}

/// An element as many times as it may occur, from the child at `start`.
fn element_match(
    max: u32,
    min: u32,
    type_id: TypeId,
    children: &[Child<'_>],
    start: usize,
) -> Info {
    let matches = |index: usize| {
        children
            .get(index)
            .is_some_and(|child| child.type_id == Some(type_id))
    };
    if !matches(start) {
        return Info::default();
    }
    if max == 1 {
        return Info {
            expected: Vec::new(),
            last: Some(start),
            outcome: Outcome::Matched,
        };
    }
    let mut count = 0;
    let mut index = start;
    while (max == 0 || max > count) && matches(index) {
        count += 1;
        index += 1;
    }
    let last = Some(index - 1);
    if count >= min {
        Info {
            expected: Vec::new(),
            last,
            outcome: Outcome::Matched,
        }
    } else {
        Info {
            expected: vec![Expected::Element(type_id)],
            last,
            outcome: Outcome::Partial,
        }
    }
}

/// The children a particle allows first: every alternative's, a sequence's
/// first item's.
fn expected_children(particle: &Particle) -> Vec<Expected> {
    let mut found = Vec::new();
    expected_into(particle, &mut found);
    found
}

fn expected_into(particle: &Particle, found: &mut Vec<Expected>) {
    match *particle {
        Particle::Any {
            max: _max,
            min: _min,
            namespace,
        } => found.push(Expected::Any(namespace)),
        Particle::Composite {
            items,
            kind: CompositeKind::Sequence,
            max: _max,
            min: _min,
        } => {
            if let Some(first) = items.first() {
                expected_into(first, found);
            }
        }
        Particle::Composite {
            items,
            kind: CompositeKind::All | CompositeKind::Choice | CompositeKind::Group,
            max: _max,
            min: _min,
        } => {
            for item in items {
                expected_into(item, found);
            }
        }
        Particle::Element {
            max: _max,
            min: _min,
            type_id,
        } => found.push(Expected::Element(type_id)),
    }
}

/// `", expected w:pPr or w:r"`: the parent's own children among `expected`,
/// in the parent's order, then the wildcards' namespaces.
fn expected_text(parent: &SchemaType, expected: &[Expected]) -> String {
    let mut names: Vec<String> = parent
        .children
        .iter()
        .filter(|(_, _, type_id, _)| expected.contains(&Expected::Element(*type_id)))
        .map(|(namespace, local, _, _)| {
            NAMESPACES.get(usize::from(*namespace)).map_or_else(
                || (*local).to_owned(),
                |row| format!("{}:{local}", row.prefix),
            )
        })
        .collect();
    for item in expected {
        if let Expected::Any(namespace) = *item {
            names.push(
                match namespace {
                    AnyNamespace::Any => "##any",
                    AnyNamespace::Local => "##local",
                    AnyNamespace::Other => "##other",
                    AnyNamespace::TargetNamespace => "##targetNamespace",
                    AnyNamespace::Uri(uri) => uri,
                }
                .to_owned(),
            );
        }
    }
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => format!(", expected {last}"),
        Some((last, rest)) => format!(", expected {} or {last}", rest.join(", ")),
    }
}

/// Whether the parent holds an element of `child`'s name as another type
/// that exists at `target`: the child was typed by another element's
/// vocabulary.
fn holds_name_of_another_type(
    parent: &SchemaType,
    child: &Child<'_>,
    target: OfficeVersion,
) -> bool {
    let Namespace::Known(namespace) = child.name.namespace else {
        return false;
    };
    parent
        .children
        .iter()
        .find(|(own, local, _, _)| *own == namespace && *local == child.name.local)
        .and_then(|(_, _, type_id, _)| SCHEMA_TYPES.get(usize::from(*type_id)))
        .is_some_and(|found| found.versions.contains(target))
}

/// The child after `last`, if there is one.
fn next(last: Option<usize>, children: &[Child<'_>]) -> Option<usize> {
    last.map(|index| index + 1)
        .filter(|index| *index < children.len())
}

/// Whether a particle requires a child, and which.
///
/// An element or wildcard with a minimum does; a sequence when an item does
/// (the first such item's); a choice when every alternative does (all of
/// theirs); a group or `xsd:all` when any item does (all of theirs).
fn required_children(particle: &Particle, mut found: Option<&mut Vec<Expected>>) -> bool {
    match *particle {
        Particle::Any {
            max: _max,
            min,
            namespace,
        } => {
            if min > 0
                && let Some(list) = found
            {
                list.push(Expected::Any(namespace));
            }
            min > 0
        }
        Particle::Element {
            max: _max,
            min,
            type_id,
        } => {
            if min > 0
                && let Some(list) = found
            {
                list.push(Expected::Element(type_id));
            }
            min > 0
        }
        Particle::Composite {
            items: _items,
            kind: _kind,
            max: _max,
            min: 0,
        } => false,
        Particle::Composite {
            items,
            kind: CompositeKind::Sequence,
            max: _max,
            min: _min,
        } => items
            .iter()
            .any(|item| required_children(item, found.as_deref_mut())),
        Particle::Composite {
            items,
            kind: CompositeKind::Choice,
            max: _max,
            min: _min,
        } => {
            let mut alternatives = Vec::new();
            let every = items
                .iter()
                .all(|item| required_children(item, Some(&mut alternatives)));
            if every && let Some(list) = found {
                list.extend(alternatives);
            }
            every
        }
        Particle::Composite {
            items,
            kind: CompositeKind::All | CompositeKind::Group,
            max: _max,
            min: _min,
        } => {
            let mut any = false;
            for item in items {
                if required_children(item, found.as_deref_mut()) {
                    any = true;
                }
            }
            any
        }
    }
}

/// A sequence once: its items in order, each matched or skipped when optional.
fn sequence_once(items: &[Particle], children: &[Child<'_>], start: usize) -> Info {
    let mut info = Info::default();
    let mut position = Some(start);
    let mut item_index = 0;
    while let (Some(item), Some(at)) = (items.get(item_index), position) {
        let found = try_match(item, children, at);
        match found.outcome {
            Outcome::NoMatch => {
                if required_children(item, None) {
                    let mut expected = Vec::new();
                    required_children(item, Some(&mut expected));
                    info.expected = expected;
                    if at == start {
                        info.last = None;
                        info.outcome = Outcome::NoMatch;
                    } else {
                        info.outcome = Outcome::Partial;
                    }
                    return info;
                }
                item_index += 1;
            }
            Outcome::Matched => {
                info.last = found.last;
                position = next(found.last, children);
                item_index += 1;
            }
            Outcome::Partial => {
                return Info {
                    outcome: Outcome::Partial,
                    ..found
                };
            }
        }
    }
    if item_index == items.len() {
        info.outcome = if info.last.is_some() {
            Outcome::Matched
        } else {
            Outcome::NoMatch
        };
        return info;
    }
    for item in &items[item_index..] {
        if required_children(item, None) {
            required_children(item, Some(&mut info.expected));
            info.outcome = Outcome::Partial;
            return info;
        }
    }
    info.outcome = Outcome::Matched;
    info
}

/// A particle as many times as it may occur, from the child at `start`.
fn try_match(particle: &Particle, children: &[Child<'_>], start: usize) -> Info {
    match *particle {
        Particle::Element { max, min, type_id } => {
            element_match(max, min, type_id, children, start)
        }
        Particle::Any {
            max,
            min,
            namespace,
        } => any_match(max, min, namespace, children, start),
        Particle::Composite {
            items,
            kind: CompositeKind::All,
            max: _max,
            min: _min,
        } => all_once(items, children, start),
        Particle::Composite {
            items,
            kind,
            max: 1,
            min: _min,
        } => composite_once(kind, items, children, start),
        Particle::Composite {
            items: _items,
            kind: _kind,
            max: _max,
            min: _min,
        } => composite_repeated(particle, children, start),
    }
}
