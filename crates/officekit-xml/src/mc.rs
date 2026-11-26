//! Markup compatibility (ECMA-376 Part 3): `mc:AlternateContent` and
//! ignorable content resolved for an Office version.

#[cfg(feature = "binary-format")]
mod archive;
mod events;

pub use events::Processed;

use alloc::borrow::Cow;
use alloc::collections::BTreeSet;
use core::mem;

use super::element::{Document, Element, NamespaceDeclaration, Node, XmlError};
use super::name::table::NAMESPACES;
use super::name::{Namespace, OfficeVersion};
use super::parse::{Scope, split_qualified};
use super::write::{PREFIX_LISTS, QUALIFIED_LISTS};

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

/// What a read does with markup compatibility. With `binary-format`, rkyv
/// archives it through its mirror in `archive`.
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

/// The branch that takes the place of an `mc:AlternateContent` block, with the declarations in
/// force inside it: its first `mc:Choice` whose every `Requires` prefix `target` understands,
/// else its `mc:Fallback`, else none.
fn selected_branch(
    block: Element,
    target: OfficeVersion,
    scope: &mut Scope,
) -> Option<(Vec<NamespaceDeclaration>, Element)> {
    scope.push(&block.namespace_declarations);
    let mut chosen = None;
    let mut fallback = None;
    for node in block.children {
        if let Node::Element(branch) = node {
            let is_choice = is_mc(&branch.name.namespace) && &*branch.name.local == "Choice";
            let is_fallback = is_mc(&branch.name.namespace) && &*branch.name.local == "Fallback";
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
    chosen.or(fallback).map(|mut branch| {
        let declarations = merged(
            block.namespace_declarations,
            mem::take(&mut branch.namespace_declarations),
        );
        (declarations, branch)
    })
}

/// The nodes that take the place of an `mc:AlternateContent` block when `branch` is the one
/// chosen.
///
/// They are the branch's children, each element given those of the block's `declarations` and
/// the branch's that its subtree uses and does not make itself.
#[must_use = "this only returns the value"]
pub fn branch_content(declarations: Vec<NamespaceDeclaration>, branch: Element) -> Vec<Node> {
    let moved = merged(declarations, branch.namespace_declarations);
    branch
        .children
        .into_iter()
        .map(|node| with_used_declarations(node, &moved))
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

/// Whether `element` is an `mc:AlternateContent` block.
fn is_alternate_content(element: &Element) -> bool {
    is_mc(&element.name.namespace) && &*element.name.local == "AlternateContent"
}

/// Whether `namespace` is the markup-compatibility namespace.
#[must_use = "this only answers the question"]
pub fn is_mc(namespace: &Namespace) -> bool {
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
        .find(|attribute| &*attribute.name.local == local && is_mc(&attribute.name.namespace))
        .map(|attribute| attribute.value.as_str())
}

/// `outer` then `inner`, a prefix `inner` declares again given only its declaration.
fn merged(
    outer: Vec<NamespaceDeclaration>,
    inner: Vec<NamespaceDeclaration>,
) -> Vec<NamespaceDeclaration> {
    let mut all: Vec<NamespaceDeclaration> = Vec::new();
    for declaration in outer.into_iter().chain(inner) {
        all.retain(|earlier| earlier.prefix != declaration.prefix);
        all.push(declaration);
    }
    all
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
            .any(|(named, local)| named == namespace && *local == *element.name.local);
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
    let nodes = mem::take(&mut element.children);
    let mut children = Vec::with_capacity(nodes.len());
    for node in nodes {
        process_node(node, target, scope, context, &mut children)?;
    }
    element.children = children;
    Ok(())
}

/// Process one child of an element in `context`, the element's declarations already in
/// `scope`: what takes its place goes onto `children`.
fn process_node(
    node: Node,
    target: OfficeVersion,
    scope: &mut Scope,
    context: &Context,
    children: &mut Vec<Node>,
) -> Result<(), XmlError> {
    let Node::Element(mut child) = node else {
        children.push(node);
        return Ok(());
    };
    if is_alternate_content(&child) {
        if let Some((declarations, mut branch)) = selected_branch(child, target, scope) {
            scope.push(&declarations);
            let processed = process_children(&mut branch, target, scope, context);
            scope.pop();
            processed?;
            children.extend(
                branch
                    .children
                    .into_iter()
                    .map(|content| with_used_declarations(content, &declarations)),
            );
        }
        return Ok(());
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
    keep_attributes(element, target, scope, context)?;
    process_children(element, target, scope, context)
}

/// The attribute rules of an element that stays, its declarations already in `scope`: refuse a
/// `mc:MustUnderstand` namespace `target` lacks, and drop the ignorable attributes `target` does
/// not understand.
fn keep_attributes(
    element: &mut Element,
    target: OfficeVersion,
    scope: &Scope,
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
    Ok(())
}

/// The `Requires` list of an `mc:Choice`, an unqualified attribute; empty when
/// it has none.
pub(crate) fn requires(choice: &Element) -> &str {
    choice
        .attribute(&Namespace::None, "Requires")
        .unwrap_or_default()
}

/// The prefixes `element` and its subtree use, in a name, a `Requires` or an `mc` list, that no
/// declaration among them binds.
fn used_prefixes(element: &Element) -> BTreeSet<&str> {
    let mut used = BTreeSet::from([&*element.name.prefix]);
    for attribute in &element.attributes {
        if !attribute.name.prefix.is_empty() {
            used.insert(&*attribute.name.prefix);
        }
        let local = &*attribute.name.local;
        if is_mc(&attribute.name.namespace) {
            if PREFIX_LISTS.contains(&local) {
                used.extend(attribute.value.split_whitespace());
            } else if QUALIFIED_LISTS.contains(&local) {
                used.extend(
                    attribute
                        .value
                        .split_whitespace()
                        .map(|item| split_qualified(item).0),
                );
            } else {
                // Not a list of prefixes.
            }
        }
    }
    if is_mc(&element.name.namespace) && &*element.name.local == "Choice" {
        used.extend(requires(element).split_whitespace());
    }
    for child in element.child_elements() {
        used.extend(used_prefixes(child));
    }
    for declaration in &element.namespace_declarations {
        used.remove(declaration.prefix.as_str());
    }
    used
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
#[must_use = "this only returns the value"]
pub fn with_declarations(node: Node, declarations: &[NamespaceDeclaration]) -> Node {
    let Node::Element(mut element) = node else {
        return node;
    };
    add_declarations(&mut element, declarations);
    Node::Element(element)
}

/// Give `element` the `declarations` it does not make itself, before its own.
fn add_declarations(element: &mut Element, declarations: &[NamespaceDeclaration]) {
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
}

/// [`with_declarations`] with only those of `declarations` the node's subtree uses: a
/// declaration no name in it needs is not carried onto it.
fn with_used_declarations(node: Node, declarations: &[NamespaceDeclaration]) -> Node {
    let Node::Element(element) = &node else {
        return node;
    };
    let used = used_prefixes(element);
    let needed: Vec<NamespaceDeclaration> = declarations
        .iter()
        .filter(|declaration| used.contains(declaration.prefix.as_str()))
        .cloned()
        .collect();
    with_declarations(node, &needed)
}
