//! The runtime's traversals of the generic tree, and the rebase of a rebuilt
//! node's unknown content.

use officekit_markup::{Context, Descend, Visitor};
use officekit_xml::element::{Element, Node};
use officekit_xml::name::QName;
use officekit_xml::parse::parse;

use crate::traversal::{Rebase, Transform as _, Visit as _};

/// Drops the comment children of every element it is asked about.
struct DropComments;

/// Collects the local name of each element it is shown, and does not walk
/// into an `x:c`.
#[derive(Default)]
struct Names {
    seen: Vec<String>,
}

/// Replaces every `x:b` with an `x:d` holding it.
struct ReplaceB;

impl Context for DropComments {
    fn element(&mut self, node: &Element) -> Option<Element> {
        let mut kept = node.clone();
        kept.children
            .retain(|child| !matches!(child, Node::Comment(_)));
        Some(kept)
    }
}

impl Visitor for Names {
    fn element(&mut self, node: &Element) -> Descend {
        self.seen.push(node.name.local.clone());
        if node.name.local == "c" {
            Descend::No
        } else {
            Descend::Yes
        }
    }
}

impl Context for ReplaceB {
    fn element(&mut self, node: &Element) -> Option<Element> {
        (node.name.local == "b").then(|| {
            let mut replaced = Element::new(QName::prefixed("x", "d", "urn:x"));
            replaced.children.push(Node::Element(node.clone()));
            replaced
        })
    }
}

#[test]
fn a_context_rebuilds_a_generic_element() {
    let document = parse(b"<x:a xmlns:x=\"urn:x\"><!--c--><x:b/></x:a>").unwrap();
    let rebuilt = document.root.transform(&mut DropComments);
    assert_eq!(rebuilt.children.len(), 1);
}

#[test]
fn a_generic_node_asks_the_hook_for_each_element_and_keeps_every_other_node() {
    let xml = "<x:a xmlns:x=\"urn:x\"><!--c-->t<?p d?><![CDATA[e]]><x:b/><x:c><x:b/></x:c></x:a>";
    let root = Node::Element(parse(xml.as_bytes()).unwrap().root);
    let rebuilt = "<x:a xmlns:x=\"urn:x\"><!--c-->t<?p d?><![CDATA[e]]><x:d><x:b/></x:d><x:c><x:d><x:b/></x:d></x:c></x:a>";
    assert_eq!(
        root.transform(&mut ReplaceB),
        Node::Element(parse(rebuilt.as_bytes()).unwrap().root)
    );
}

#[test]
fn a_visitor_is_shown_each_element_and_walks_in_on_yes() {
    let xml = "<x:a xmlns:x=\"urn:x\"><x:b><x:e/></x:b><!--c--><x:c><x:f/></x:c></x:a>";
    let mut names = Names::default();
    Node::Element(parse(xml.as_bytes()).unwrap().root).visit(&mut names);
    assert_eq!(names.seen, ["a", "b", "e", "c"]);
}

#[test]
fn a_rebased_place_starts_after_the_leading_items() {
    let rebase = Rebase {
        counts: vec![2, 0],
        leading: 1,
        ..Rebase::default()
    };
    let places: Vec<usize> = (0..4).map(|after| rebase.place(after)).collect();
    assert_eq!(places, [1, 3, 3, 4]);
}
