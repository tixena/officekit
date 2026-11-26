//! Markup compatibility applied to a part read forward: the events of the tree
//! [`Document::process_markup_compatibility`](crate::element::Document::process_markup_compatibility)
//! gives.

use alloc::borrow::Cow;
use alloc::collections::VecDeque;
use alloc::rc::Rc;

use super::{
    Context, MarkupCompatibility, add_declarations, context, is_alternate_content, keep_attributes,
    process_node, understands, with_declarations,
};
use crate::element::{Element, NamespaceDeclaration, Node, XmlError};
use crate::name::OfficeVersion;
use crate::parse::Scope;
use crate::stream::reader::{PartEvent, ReadEvents};

#[cfg(test)]
mod tests;

/// An element the source started and has not ended.
struct Open {
    /// The `mc` context inside it.
    context: Rc<Context>,
    /// The declarations of an element whose children take its place; `None` for one that stays.
    unwrapped: Option<Vec<NamespaceDeclaration>>,
}

/// A part's events with markup compatibility applied as a read's option says.
///
/// Each element gets the rules of ECMA-376 Part 3 as it starts: an ignorable one the target
/// does not understand is skipped, or unwrapped when `mc:ProcessContent` names it, and one that
/// stays loses the ignorable attributes the target does not understand. An
/// `mc:AlternateContent` block is read whole and comes as the nodes of its chosen branch.
pub struct Processed<'source> {
    open: Vec<Open>,
    /// Events ready to be given, in order.
    ready: VecDeque<PartEvent>,
    /// The declarations in scope in the source.
    scope: Scope,
    source: &'source mut dyn ReadEvents,
    /// The target while the root is read; `None` with `Keep`, and past the root's end.
    target: Option<OfficeVersion>,
}

impl<'source> Processed<'source> {
    fn end(&mut self) {
        let Some(open) = self.open.pop() else {
            self.ready.push_back(PartEvent::End);
            return;
        };
        self.scope.pop();
        if open.unwrapped.is_none() {
            self.ready.push_back(PartEvent::End);
        }
        if self.open.is_empty() {
            self.target = None;
        }
    }

    /// The events of `source` with markup compatibility applied as `mode` says.
    #[must_use = "this only builds the value"]
    pub fn new(source: &'source mut dyn ReadEvents, mode: MarkupCompatibility) -> Self {
        let target = match mode {
            MarkupCompatibility::Keep => None,
            MarkupCompatibility::Process { target } => Some(target),
        };
        Self {
            open: Vec::new(),
            ready: VecDeque::new(),
            scope: Scope::default(),
            source,
            target,
        }
    }

    /// A node of the source: outside the root it stays, inside it takes the nodes the tree
    /// processing puts in its place.
    fn node(&mut self, node: Node, target: OfficeVersion) -> Result<(), XmlError> {
        let Node::Element(element) = node else {
            self.ready.push_back(PartEvent::Node(node));
            return Ok(());
        };
        let Some(parent) = self.open.last() else {
            let mut root = element;
            self.root(&mut root, target)?;
            self.scope.pop();
            self.target = None;
            self.ready.push_back(PartEvent::Node(Node::Element(root)));
            return Ok(());
        };
        let around = Rc::clone(&parent.context);
        let mut nodes = Vec::new();
        process_node(
            Node::Element(element),
            target,
            &mut self.scope,
            &around,
            &mut nodes,
        )?;
        for processed in nodes {
            let placed = self.placed(processed);
            self.ready.push_back(PartEvent::Node(placed));
        }
        Ok(())
    }

    /// `node` given the declarations of each unwrapped element it stands in for, innermost first.
    fn placed(&self, node: Node) -> Node {
        let mut placed = node;
        for open in self.open.iter().rev() {
            let Some(declarations) = &open.unwrapped else {
                break;
            };
            placed = with_declarations(placed, declarations);
        }
        placed
    }

    /// The root's attribute rules and the context inside it, its declarations pushed.
    fn root(&mut self, root: &mut Element, target: OfficeVersion) -> Result<Rc<Context>, XmlError> {
        self.scope.push(&root.namespace_declarations);
        let inside = Rc::new(context(root, &self.scope, &Context::default()).into_owned());
        keep_attributes(root, target, &self.scope, &inside)?;
        Ok(inside)
    }

    /// The source's events up to the end of the element just started.
    fn skip(&mut self) -> Result<(), XmlError> {
        let mut depth = 1_usize;
        while depth > 0 {
            match self.source.event()? {
                Some(PartEvent::Start(_)) => depth += 1,
                Some(PartEvent::End) => depth -= 1,
                Some(PartEvent::Declaration(_) | PartEvent::Node(_)) => {}
                None => {
                    return Err(XmlError::Malformed(
                        "the part ends inside an element".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }

    fn start(&mut self, start: Element, target: OfficeVersion) -> Result<(), XmlError> {
        let mut element = start;
        let Some(parent) = self.open.last() else {
            let context = self.root(&mut element, target)?;
            self.open.push(Open {
                context,
                unwrapped: None,
            });
            self.ready.push_back(PartEvent::Start(element));
            return Ok(());
        };
        if is_alternate_content(&element) {
            let block = self.source.element(element)?;
            return self.node(Node::Element(block), target);
        }
        let around = Rc::clone(&parent.context);
        self.scope.push(&element.namespace_declarations);
        let own = match context(&element, &self.scope, &around) {
            Cow::Borrowed(_) => None,
            Cow::Owned(own) => Some(own),
        };
        let inside = own.map_or(around, Rc::new);
        let namespace = &element.name.namespace;
        if inside.ignorable.contains(namespace) && !understands(namespace, target) {
            let processed = inside
                .process_content
                .iter()
                .any(|(named, local)| named == namespace && *local == element.name.local);
            if processed {
                self.open.push(Open {
                    context: inside,
                    unwrapped: Some(element.namespace_declarations),
                });
                return Ok(());
            }
            self.scope.pop();
            return self.skip();
        }
        keep_attributes(&mut element, target, &self.scope, &inside)?;
        for open in self.open.iter().rev() {
            let Some(declarations) = &open.unwrapped else {
                break;
            };
            add_declarations(&mut element, declarations);
        }
        self.open.push(Open {
            context: inside,
            unwrapped: None,
        });
        self.ready.push_back(PartEvent::Start(element));
        Ok(())
    }
}

impl ReadEvents for Processed<'_> {
    fn event(&mut self) -> Result<Option<PartEvent>, XmlError> {
        loop {
            if let Some(event) = self.ready.pop_front() {
                return Ok(Some(event));
            }
            let Some(target) = self.target else {
                return self.source.event();
            };
            let Some(event) = self.source.event()? else {
                return Ok(None);
            };
            match event {
                PartEvent::Declaration(_) => return Ok(Some(event)),
                PartEvent::End => self.end(),
                PartEvent::Node(node) => self.node(node, target)?,
                PartEvent::Start(element) => self.start(element, target)?,
            }
        }
    }
}
