//! Markup compatibility (ECMA-376 Part 3): `mc:AlternateContent` and
//! ignorable content resolved for an Office version.

use alloc::borrow::Cow;
use alloc::collections::{BTreeSet, VecDeque};
use core::mem;

use super::element::{Document, Element, NamespaceDeclaration, Node, XmlError};
use super::name::table::NAMESPACES;
use super::name::{Namespace, OfficeVersion};
use super::parse::{Scope, split_qualified};

#[cfg(test)]
mod tests;

const MC_NAMESPACE: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// The `mc` attributes in force on an element: the namespaces `mc:Ignorable`
/// names, and the elements `mc:ProcessContent` names.
#[derive(Clone, Debug, Default)]
struct Context {
    ignorable: BTreeSet<Namespace>,
    process_content: BTreeSet<(Namespace, String)>,
}

/// What a read does with markup compatibility.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum MarkupCompatibility {
    /// Every `mc:AlternateContent` block and `mc:*` attribute stays as the
    /// file has them.
    #[default]
    Keep,
    /// Each `mc:AlternateContent` becomes its first `mc:Choice` `target` understands, or its
    /// `mc:Fallback`; ignorable content `target` lacks goes, or is unwrapped by `mc:ProcessContent`.
    Process { target: OfficeVersion },
}

/// What becomes of an element.
enum Outcome {
    /// It goes, with its subtree.
    Drop,
    Keep,
    /// Its children take its place.
    Unwrap,
}

/// The nodes that take the place of an `mc:AlternateContent` block: the
/// children of its first `mc:Choice` whose every `Requires` prefix `target`
/// understands, else of its `mc:Fallback`, else none. The block's and the
/// branch's declarations move onto each element among them.
fn alternate_content(block: Element, target: OfficeVersion, scope: &mut Scope) -> Vec<Node> {
    scope.push(&block.namespace_declarations);
    let mut chosen = None;
    let mut fallback = None;
    for node in block.children {
        if let Node::Element(branch) = node {
            let is_choice = is_mc(&branch.name.namespace) && branch.name.local == "Choice";
            let is_fallback = is_mc(&branch.name.namespace) && branch.name.local == "Fallback";
            if is_choice && understands_requires(&branch, target, scope) {
                chosen = Some(branch);
                break;
            } else if is_fallback && fallback.is_none() {
                fallback = Some(branch);
            } else {
                // Another branch, or a Choice the target does not understand.
            }
        }
    }
    scope.pop();
    chosen.or(fallback).map_or_else(Vec::new, |branch| {
        branch_content(block.namespace_declarations, branch)
    })
}

/// The nodes that take the place of an `mc:AlternateContent` block declaring `declarations`
/// when `branch` is the one chosen: its children, each element given the block's and the
/// branch's declarations it does not make itself.
pub(crate) fn branch_content(
    declarations: Vec<NamespaceDeclaration>,
    branch: Element,
) -> Vec<Node> {
    let mut moved: Vec<NamespaceDeclaration> = Vec::new();
    for declaration in declarations
        .into_iter()
        .chain(branch.namespace_declarations)
    {
        moved.retain(|earlier| earlier.prefix != declaration.prefix);
        moved.push(declaration);
    }
    branch
        .children
        .into_iter()
        .map(|node| with_declarations(node, &moved))
        .collect()
}

/// Apply `mode` to `document`.
pub(crate) fn apply(document: &mut Document, mode: MarkupCompatibility) -> Result<(), XmlError> {
    let MarkupCompatibility::Process { target } = mode else {
        return Ok(());
    };
    let mut scope = Scope::default();
    let root = &mut document.root;
    scope.push(&root.namespace_declarations);
    let outermost = Context::default();
    let inside = context(root, &scope, &outermost);
    process_kept(root, target, &mut scope, &inside)
}

/// The context inside `element`: the one around it, with the namespaces and
/// names of its own `mc:Ignorable` and `mc:ProcessContent`.
fn context<'context>(
    element: &Element,
    scope: &Scope,
    around: &'context Context,
) -> Cow<'context, Context> {
    let ignorable = mc_attribute(element, "Ignorable");
    let process_content = mc_attribute(element, "ProcessContent");
    if ignorable.is_none() && process_content.is_none() {
        return Cow::Borrowed(around);
    }
    let mut inside = around.clone();
    for prefix in ignorable.unwrap_or_default().split_whitespace() {
        if let Some(uri) = scope.uri(prefix) {
            inside.ignorable.insert(Namespace::from_uri(uri));
        }
    }
    for item in process_content.unwrap_or_default().split_whitespace() {
        let (prefix, local) = split_qualified(item);
        let namespace = match scope.uri(prefix) {
            Some(uri) => Namespace::from_uri(uri),
            None if prefix.is_empty() => Namespace::None,
            None => continue,
        };
        inside.process_content.insert((namespace, local.to_owned()));
    }
    Cow::Owned(inside)
}

/// Whether `namespace` is the markup-compatibility namespace.
pub(crate) fn is_mc(namespace: &Namespace) -> bool {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES
            .get(usize::from(id))
            .is_some_and(|row| row.uri == MC_NAMESPACE)
    } else {
        false
    }
}

/// The value of an `mc:*` attribute of `element`.
fn mc_attribute<'element>(element: &'element Element, local: &str) -> Option<&'element str> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == local && is_mc(&attribute.name.namespace))
        .map(|attribute| attribute.value.as_str())
}

/// Process an element that is not an `mc:AlternateContent` block, its
/// declarations already in `scope`.
fn process_child(
    element: &mut Element,
    target: OfficeVersion,
    scope: &mut Scope,
    around: &Context,
) -> Result<Outcome, XmlError> {
    let inside = context(element, scope, around);
    let namespace = &element.name.namespace;
    if inside.ignorable.contains(namespace) && !understands(namespace, target) {
        let processed = inside
            .process_content
            .iter()
            .any(|(named, local)| named == namespace && *local == element.name.local);
        if !processed {
            return Ok(Outcome::Drop);
        }
        process_children(element, target, scope, &inside)?;
        return Ok(Outcome::Unwrap);
    }
    process_kept(element, target, scope, &inside)?;
    Ok(Outcome::Keep)
}

/// Process the children of `element` in `context`, its declarations already
/// in `scope`.
fn process_children(
    element: &mut Element,
    target: OfficeVersion,
    scope: &mut Scope,
    context: &Context,
) -> Result<(), XmlError> {
    let mut queue: VecDeque<Node> = mem::take(&mut element.children).into();
    let mut children = Vec::with_capacity(queue.len());
    while let Some(node) = queue.pop_front() {
        let Node::Element(mut child) = node else {
            children.push(node);
            continue;
        };
        if is_mc(&child.name.namespace) && child.name.local == "AlternateContent" {
            for replacement in alternate_content(child, target, scope).into_iter().rev() {
                queue.push_front(replacement);
            }
            continue;
        }
        scope.push(&child.namespace_declarations);
        let outcome = process_child(&mut child, target, scope, context);
        scope.pop();
        match outcome? {
            Outcome::Drop => {}
            Outcome::Keep => children.push(Node::Element(child)),
            Outcome::Unwrap => {
                let declarations = child.namespace_declarations;
                children.extend(
                    child
                        .children
                        .into_iter()
                        .map(|grandchild| with_declarations(grandchild, &declarations)),
                );
            }
        }
    }
    element.children = children;
    Ok(())
}

/// Process an element that stays, its declarations already in `scope`:
/// refuse a `mc:MustUnderstand` namespace `target` lacks, drop the ignorable
/// attributes `target` does not understand, and process the children.
fn process_kept(
    element: &mut Element,
    target: OfficeVersion,
    scope: &mut Scope,
    context: &Context,
) -> Result<(), XmlError> {
    for prefix in mc_attribute(element, "MustUnderstand")
        .unwrap_or_default()
        .split_whitespace()
    {
        let understood = scope
            .uri(prefix)
            .is_some_and(|uri| understands(&Namespace::from_uri(uri), target));
        if !understood {
            return Err(XmlError::MustUnderstand {
                prefix: prefix.to_owned(),
            });
        }
    }
    element.attributes.retain(|attribute| {
        let namespace = &attribute.name.namespace;
        !context.ignorable.contains(namespace) || understands(namespace, target)
    });
    process_children(element, target, scope, context)
}

/// The `Requires` list of an `mc:Choice`, an unqualified attribute; empty when
/// it has none.
pub(crate) fn requires(choice: &Element) -> &str {
    choice
        .attribute(&Namespace::None, "Requires")
        .unwrap_or_default()
}

/// Whether `target` understands a namespace: ECMA-376's own, `mc`, and `xml`
/// always, a Microsoft extension from the version that brought it; a
/// namespace outside the table never.
fn understands(namespace: &Namespace, target: OfficeVersion) -> bool {
    match *namespace {
        Namespace::Known(id) => NAMESPACES.get(usize::from(id)).is_some_and(|row| {
            row.introduced
                .is_none_or(|introduced| introduced.rank() <= target.rank())
        }),
        Namespace::None | Namespace::Other(_) => false,
    }
}

/// Whether `target` understands every prefix the `Requires` of `choice` names.
fn understands_requires(choice: &Element, target: OfficeVersion, scope: &mut Scope) -> bool {
    scope.push(&choice.namespace_declarations);
    let understood = requires(choice).split_whitespace().all(|prefix| {
        scope
            .uri(prefix)
            .is_some_and(|uri| understands(&Namespace::from_uri(uri), target))
    });
    scope.pop();
    understood
}

/// A node taking the place of an element that held `declarations`: an
/// element gets the ones it does not declare itself, before its own.
pub(crate) fn with_declarations(node: Node, declarations: &[NamespaceDeclaration]) -> Node {
    let Node::Element(mut element) = node else {
        return node;
    };
    let mut merged: Vec<NamespaceDeclaration> = declarations
        .iter()
        .filter(|declaration| {
            !element
                .namespace_declarations
                .iter()
                .any(|own| own.prefix == declaration.prefix)
        })
        .cloned()
        .collect();
    merged.append(&mut element.namespace_declarations);
    element.namespace_declarations = merged;
    Node::Element(element)
}
