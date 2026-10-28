use std::io;

use officekit_markup::{Context, Extra, Unknown, Written};
use officekit_xml::element::{Element, Node};
use officekit_xml::name::QName;
use officekit_xml::parse::parse;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::Layout;

use crate::traversal::{Place, Unchanged, WriteThrough as _};

const XML: &str = "<x:a xmlns:x=\"urn:x\"><!--c--><x:b/><x:c><x:b/><x:e/></x:c>t</x:a>";

/// Writes every `x:e` itself as `<x:written/>`, and replaces every `x:b`
/// with an `x:d` holding it.
struct ReplaceB;

impl Context for ReplaceB {
    fn element(&mut self, node: &Element) -> Option<Element> {
        (node.name.local == "b").then(|| {
            let mut replaced = Element::new(QName::prefixed("x", "d", "urn:x"));
            replaced.children.push(Node::Element(node.clone()));
            replaced
        })
    }

    fn write_element(&mut self, node: &Element, out: &mut PartWriter<'_>) -> io::Result<Written> {
        if node.name.local != "e" {
            return Ok(Written::No);
        }
        out.empty("x:written", &[])?;
        Ok(Written::Yes)
    }
}

fn root() -> Node {
    Node::Element(parse(XML.as_bytes()).unwrap().root)
}

/// The bytes `write` sends through a compact `PartWriter`.
fn written<F>(write: F) -> String
where
    F: FnOnce(&mut PartWriter<'_>) -> io::Result<()>,
{
    let mut sink = Vec::new();
    write(&mut PartWriter::new(&mut sink, Layout::Compact)).unwrap();
    String::from_utf8(sink).unwrap()
}

#[test]
fn a_generic_element_is_written_through_a_context_whose_element_hook_replaces_it() {
    let root = root();
    assert_eq!(
        written(|out| root.write_through(&mut ReplaceB, out)),
        "<x:a xmlns:x=\"urn:x\"><!--c--><x:d><x:b/></x:d><x:c><x:d><x:b/></x:d><x:written/></x:c>t</x:a>"
    );
}

#[test]
fn a_generic_element_is_written_through_unchanged_as_it_is() {
    let root = root();
    assert_eq!(written(|out| root.write_through(&mut Unchanged, out)), XML);
    assert_eq!(written(|out| out.node(&root)), XML);
}

#[test]
fn place_writes_each_extra_before_the_declared_child_it_names_and_the_rest_after_the_last() {
    let comment = |text: &str| Node::Comment(text.to_owned());
    let mut unknown = Unknown::default();
    unknown.children = vec![
        Extra::new(0, comment("before the first")),
        Extra::new(1, comment("before the second")),
        Extra::new(2, comment("after the last")),
        Extra::new(5, comment("past the last")),
    ];
    let bytes = written(|out| {
        let mut place = Place::new(&unknown);
        out.start("p", &[], &[])?;
        for name in ["first", "second"] {
            place.before_known(&mut Unchanged, out)?;
            out.empty(name, &[])?;
        }
        place.finish(&mut Unchanged, out)?;
        out.end("p")
    });
    assert_eq!(
        bytes,
        "<p><!--before the first--><first/><!--before the second--><second/><!--after the last--><!--past the last--></p>"
    );
}
