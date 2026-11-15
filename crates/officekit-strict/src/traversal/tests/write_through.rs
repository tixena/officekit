use std::io;

use officekit_markup::Read as _;
use officekit_markup::wordprocessing_drawing::{self, WordprocessingMLShape};
use officekit_markup::{Context, Extra, Unknown, Written};
use officekit_xml::element::{Element, Node};
use officekit_xml::name::QName;
use officekit_xml::parse::parse;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{Layout, Scope};

use crate::traversal::{Place, Transform as _, Unchanged, WriteThrough as _};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
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

/// An element `prefix:local` of `uri` holding `children`.
fn element_of(prefix: &str, local: &str, uri: &str, children: Vec<Element>) -> Element {
    let mut built = Element::new(QName::prefixed(prefix, local, uri));
    built.children = children.into_iter().map(Node::Element).collect();
    built
}

/// A shape holding a text box of one paragraph, named as Word names it: `wps:wsp`, `wps:txbx`
/// and `w:txbxContent`.
fn word_shape_tree() -> Element {
    let paragraph = element_of("w", "p", W, Vec::new());
    let content = element_of("w", "txbxContent", W, vec![paragraph]);
    let text_box = element_of("wps", "txbx", WPS, vec![content]);
    element_of("wps", "wsp", WPS, vec![text_box])
}

/// The bytes the typed tree's writer gives for `shape`, through a compact `PartWriter`.
fn written_by_writer(shape: &WordprocessingMLShape) -> String {
    let element =
        wordprocessing_drawing::write::wordprocessing_ml_shape(shape, &mut Scope::default());
    written(|out| out.node(&Node::Element(element)))
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

#[test]
fn a_shape_read_under_words_names_is_written_through_with_them() {
    let shape = WordprocessingMLShape::read(&word_shape_tree());
    let bytes = written(|out| shape.write_through(&mut Unchanged, out));
    assert_eq!(
        bytes,
        format!(
            "<wps:wsp xmlns:wps=\"{WPS}\"><wps:txbx><w:txbxContent xmlns:w=\"{W}\"><w:p/>\
             </w:txbxContent></wps:txbx></wps:wsp>"
        )
    );
    assert_eq!(bytes, written_by_writer(&shape));
}

#[test]
fn a_shape_built_in_code_is_written_through_with_the_standards_name() {
    let shape = WordprocessingMLShape::default();
    let bytes = written(|out| shape.write_through(&mut Unchanged, out));
    assert!(
        bytes.starts_with(&format!("<wp:wsp xmlns:wp=\"{WP}\">")),
        "{bytes}"
    );
    assert_eq!(bytes, written_by_writer(&shape));
}

#[test]
fn a_shape_read_under_words_names_keeps_them_through_a_transform() {
    let shape = WordprocessingMLShape::read(&word_shape_tree());
    let changed = shape.transform(&mut Unchanged);
    assert_eq!(changed, shape);
    assert_eq!(
        written(|out| changed.write_through(&mut Unchanged, out)),
        written_by_writer(&shape)
    );
}
