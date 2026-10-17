//! A required child is written back as the read element held it.

use super::{GroupShapeContent, NotesSlideSize, Presentation, read, write};
use crate::markup::drawing::PositiveCoordinate;
use crate::markup::{self, Descend, Scope, Transform as _, Visit as _, presentation};
use crate::stream::transform::{Unchanged, WriteThrough};
use crate::stream::writer::PartWriter;
use crate::xml::element::Element;
use crate::xml::parse::parse;
use crate::xml::write::Layout;

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";

/// Puts two copies of each shape of a shape list in its place.
struct CopyShapes;

/// Answers a sized `p:notesSz` for each one it is asked about, and counts
/// them.
#[derive(Default)]
struct FillNotesSize {
    asked: usize,
}

/// Counts the `p:notesSz` nodes it is shown.
#[derive(Default)]
struct NotesSizes(usize);

impl markup::Context for CopyShapes {
    fn presentation(&mut self) -> Option<&mut dyn presentation::Context> {
        Some(self)
    }
}

impl presentation::Context for CopyShapes {
    fn group_shape_content_items(
        &mut self,
        node: &GroupShapeContent,
    ) -> Option<Vec<GroupShapeContent>> {
        matches!(node, GroupShapeContent::Shape(_)).then(|| vec![node.clone(), node.clone()])
    }
}

impl markup::Context for FillNotesSize {
    fn presentation(&mut self) -> Option<&mut dyn presentation::Context> {
        Some(self)
    }
}

impl presentation::Context for FillNotesSize {
    fn notes_slide_size(&mut self, _node: &NotesSlideSize) -> Option<NotesSlideSize> {
        self.asked += 1;
        Some(notes_size())
    }
}

impl markup::Visitor for NotesSizes {
    fn presentation(&mut self) -> Option<&mut dyn presentation::Visitor> {
        Some(self)
    }
}

impl presentation::Visitor for NotesSizes {
    fn notes_slide_size(&mut self, _node: &NotesSlideSize) -> Descend {
        self.0 += 1;
        Descend::Yes
    }
}

/// The XML of an element, written in the compact layout.
fn compact(element: &Element) -> String {
    let mut bytes = Vec::new();
    PartWriter::new(&mut bytes, Layout::Compact)
        .element(element)
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

/// A `p:notesSz` of 6858000 by 9144000 EMUs.
fn notes_size() -> NotesSlideSize {
    NotesSlideSize {
        cx: Some(PositiveCoordinate("6858000".to_owned())),
        cy: Some(PositiveCoordinate("9144000".to_owned())),
        ..NotesSlideSize::default()
    }
}

/// The root element of an XML text.
fn parsed(text: &str) -> Element {
    parse(text.as_bytes()).unwrap().root
}

/// How many `p:notesSz` a visitor is shown in a presentation.
fn shown(node: &Presentation) -> usize {
    let mut visitor = NotesSizes::default();
    node.visit(&mut visitor);
    visitor.0
}

/// The XML the tree writer writes for a presentation.
fn tree(node: &Presentation) -> String {
    compact(&write::presentation(node, &mut Scope::default()))
}

/// The XML a node writes through `ctx`.
fn written<N>(node: &N, ctx: &mut dyn markup::Context) -> String
where
    N: WriteThrough,
{
    let mut bytes = Vec::new();
    node.write_through(ctx, &mut PartWriter::new(&mut bytes, Layout::Compact))
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn a_required_child_the_element_lacked_is_not_written() {
    let bare = format!("<p:presentation xmlns:p=\"{P}\"/>");
    // The unknown child stays after `p:smartTags`, the declared child before it.
    let extra = format!(
        "<p:presentation xmlns:p=\"{P}\" xmlns:x=\"urn:x\"><p:smartTags/><x:u/><p:extLst/></p:presentation>"
    );
    for text in [bare, extra] {
        let node = read::presentation(&parsed(&text));
        assert_eq!(tree(&node), text);
        assert_eq!(written(&node, &mut Unchanged), text);
        assert_eq!(tree(&node.transform(&mut Unchanged)), text);
        assert_eq!(shown(&node), 0);
    }
    // The unknown child stays last when a list hook grows the list past the two absent children.
    let shapes = read::shape_tree(&parsed(&format!(
        "<p:spTree xmlns:p=\"{P}\" xmlns:x=\"urn:x\"><p:sp/><p:extLst/><x:u/></p:spTree>"
    )));
    let copied = format!(
        "<p:spTree xmlns:p=\"{P}\" xmlns:x=\"urn:x\"><p:sp/><p:sp/><p:extLst/><x:u/></p:spTree>"
    );
    let transformed = shapes.transform(&mut CopyShapes);
    assert_eq!(
        compact(&write::shape_tree(&transformed, &mut Scope::default())),
        copied
    );
    assert_eq!(written(&shapes, &mut CopyShapes), copied);
}

#[test]
fn a_required_child_the_element_held_is_written() {
    let text = format!(
        "<p:presentation xmlns:p=\"{P}\"><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    );
    let node = read::presentation(&parsed(&text));
    assert_eq!(tree(&node), text);
    assert_eq!(written(&node, &mut Unchanged), text);
    assert_eq!(shown(&node), 1);
}

#[test]
fn a_node_built_in_code_writes_its_required_children() {
    let built = format!("<p:presentation xmlns:p=\"{P}\"><p:notesSz/></p:presentation>");
    let node = Presentation::default();
    assert_eq!(tree(&node), built);
    assert_eq!(written(&node, &mut Unchanged), built);
    assert_eq!(shown(&node), 1);
}

#[test]
fn a_required_child_set_on_a_node_read_without_it_is_written() {
    let mut node = read::presentation(&parsed(&format!("<p:presentation xmlns:p=\"{P}\"/>")));
    *node.notes_slide_size = notes_size();
    let text = format!(
        "<p:presentation xmlns:p=\"{P}\"><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>"
    );
    assert_eq!(tree(&node), text);
    assert_eq!(written(&node, &mut Unchanged), text);
    assert_eq!(shown(&node), 1);
}

#[test]
fn a_hook_is_not_asked_about_the_default_node_of_a_child_the_element_lacked() {
    let text = format!("<p:presentation xmlns:p=\"{P}\"/>");
    let node = read::presentation(&parsed(&text));
    let mut transform = FillNotesSize::default();
    let transformed = tree(&node.transform(&mut transform));
    let mut through = FillNotesSize::default();
    assert_eq!(transformed, written(&node, &mut through));
    assert_eq!(transformed, text);
    assert_eq!((transform.asked, through.asked), (0, 0));
}
