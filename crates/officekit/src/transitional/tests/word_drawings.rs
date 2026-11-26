use officekit_markup::drawing::{
    AnyElementContent, DrawingElementID, GraphicObject, GraphicObjectData, PositiveCoordinate,
};
use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentBody, DrawingMLObject, DrawingObjectContent, Paragraph,
    ParagraphContent, TextRun, TextRunContent,
};
use officekit_markup::wordprocessing_drawing::{
    DrawingCanvasContent, DrawingObjectNonVisualProperties, DrawingObjectSize,
    InlineDrawingMLObject, NonVisualDrawingPropertiesForAShape, ShapeNonVisualPropertiesContent,
    ShapeTextContent, TextualContentsOfShapeTxbx, WordprocessingMLDrawingCanvas,
    WordprocessingMLShape,
};
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::mc::MarkupCompatibility;
use officekit_xml::name::{OfficeVersion, QName};
use officekit_xml::packaging::package::OpenOptions;

use crate::docx::Docx;
use crate::parts::kinds::WordprocessingKind;

use super::{MC, Transitional as _, W, WPC, WPS};

/// A shape with no names of its own, holding a text box with one paragraph when `text_box`.
fn shape(text_box: bool) -> WordprocessingMLShape {
    let mut built = WordprocessingMLShape::default();
    built.non_visual_properties_content = Box::new(ShapeNonVisualPropertiesContent::from(
        NonVisualDrawingPropertiesForAShape::default(),
    ));
    if text_box {
        let mut found = TextualContentsOfShapeTxbx::default();
        found.rich_text_box_content_container.content =
            vec![BlockContent::from(Paragraph::default())];
        built.text_content = Some(Box::new(ShapeTextContent::from(found)));
    }
    built
}

/// A canvas with no names of its own, holding `shape`.
fn canvas(held: WordprocessingMLShape) -> WordprocessingMLDrawingCanvas {
    let mut built = WordprocessingMLDrawingCanvas::default();
    built.content = vec![DrawingCanvasContent::from(held)];
    built
}

/// `shape` under Word's names, with the namespaces of those names declared on it.
fn word_shape(text_box: bool) -> WordprocessingMLShape {
    let mut built = shape(text_box);
    built.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "wsp", WPS)));
    built.unknown_mut().namespace_declarations = vec![
        NamespaceDeclaration::new("wps".to_owned(), WPS.to_owned()),
        NamespaceDeclaration::new("w".to_owned(), W.to_owned()),
    ];
    built.properties.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "spPr", WPS)));
    built.body_properties.unknown_mut().name =
        Some(Box::new(QName::prefixed("wps", "bodyPr", WPS)));
    if let ShapeNonVisualPropertiesContent::NonVisualDrawingPropertiesForAShape(properties) =
        &mut *built.non_visual_properties_content
    {
        properties.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "cNvSpPr", WPS)));
    }
    if let Some(ShapeTextContent::TextualContentsOfShapeTxbx(found)) =
        built.text_content.as_deref_mut()
    {
        found.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "txbx", WPS)));
        found.rich_text_box_content_container.unknown_mut().name =
            Some(Box::new(QName::prefixed("w", "txbxContent", W)));
    }
    built
}

/// A paragraph showing `item`, a `graphicData` of `uri`, in the line of text.
fn paragraph(uri: &str, item: AnyElementContent) -> BlockContent {
    let mut data = GraphicObjectData::default();
    data.content = vec![item];
    data.uri = Some(uri.to_owned());
    let mut graphic = GraphicObject::default();
    graphic.data = Box::new(data);
    let mut properties = DrawingObjectNonVisualProperties::default();
    properties.id = Some(DrawingElementID::from("1".to_owned()));
    properties.name = Some("Shape 1".to_owned());
    let mut size = DrawingObjectSize::default();
    size.cx = Some(PositiveCoordinate::from("914400".to_owned()));
    size.cy = Some(PositiveCoordinate::from("457200".to_owned()));
    let mut inline = InlineDrawingMLObject::default();
    inline.drawing_object_non_visual_properties = Box::new(properties);
    inline.drawing_object_size = Box::new(size);
    inline.graphic_object = Box::new(graphic);
    let mut drawing = DrawingMLObject::default();
    drawing.content = vec![DrawingObjectContent::from(inline)];
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::from(drawing)];
    let mut built = Paragraph::default();
    built.content = vec![ParagraphContent::from(run)];
    BlockContent::from(built)
}

/// `block`, a paragraph showing a drawing, with its drawing in an `mc:AlternateContent` as Word
/// writes it: a Choice requiring `wps`, and a Fallback holding an empty `w:pict`.
fn in_words_alternate_content(block: BlockContent) -> BlockContent {
    let BlockContent::Paragraph(mut built) = block else {
        return block;
    };
    for content in &mut built.content {
        let ParagraphContent::TextRun(run) = content else {
            continue;
        };
        for item in &mut run.content {
            let TextRunContent::DrawingMLObject(drawing) = item else {
                continue;
            };
            let mut choice = Element::new(QName::prefixed("mc", "Choice", MC));
            choice.attributes.push(Attribute::new(
                QName::unprefixed("Requires", ""),
                "wps".to_owned(),
            ));
            choice
                .children
                .push(Node::Element(drawing.to_transitional().unwrap()));
            let mut fallback = Element::new(QName::prefixed("mc", "Fallback", MC));
            fallback
                .children
                .push(Node::Element(Element::new(QName::prefixed("w", "pict", W))));
            let mut alternate = Element::new(QName::prefixed("mc", "AlternateContent", MC));
            alternate.children = vec![Node::Element(choice), Node::Element(fallback)];
            *item = TextRunContent::Other(Box::new(Node::Element(alternate)));
        }
    }
    BlockContent::Paragraph(built)
}

/// A main document whose body holds `blocks`.
fn document_of(blocks: Vec<BlockContent>) -> Document {
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document
}

/// `document` written in a new Transitional file and read back with `markup_compatibility`.
fn written_and_read(document: &Document, markup_compatibility: MarkupCompatibility) -> Document {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_main_document(document).unwrap();
    let (bytes, warnings) = docx.to_bytes_transitional().unwrap();
    assert_eq!(warnings, []);
    let mut options = OpenOptions::default();
    options.markup_compatibility = markup_compatibility;
    Docx::from_bytes(bytes, options)
        .unwrap()
        .main_document()
        .unwrap()
}

/// The two markup-compatibility modes of a read: processed for Office 2016, and kept.
fn modes() -> [MarkupCompatibility; 2] {
    let processed = MarkupCompatibility::Process {
        target: OfficeVersion::Office2016,
    };
    [processed, MarkupCompatibility::Keep]
}

/// The tree a first write and read with `mode` gives equals the one a second gives.
fn assert_stable_in(document: &Document, mode: MarkupCompatibility) {
    let first = written_and_read(document, mode);
    let second = written_and_read(&first, mode);
    assert_eq!(first, second);
}

/// [`assert_stable_in`] in both modes.
fn assert_stable(document: &Document) {
    for mode in modes() {
        assert_stable_in(document, mode);
    }
}

#[test]
fn a_shape_a_text_box_and_a_canvas_built_with_no_names_read_the_same_after_a_second_write() {
    assert_stable(&document_of(vec![
        paragraph(WPS, AnyElementContent::from(shape(false))),
        paragraph(WPS, AnyElementContent::from(shape(true))),
        paragraph(WPC, AnyElementContent::from(canvas(shape(false)))),
    ]));
}

#[test]
fn a_shape_built_with_no_names_reads_the_same_after_a_second_write() {
    assert_stable(&document_of(vec![paragraph(
        WPS,
        AnyElementContent::from(shape(false)),
    )]));
}

#[test]
fn a_text_box_built_with_no_names_reads_the_same_after_a_second_write() {
    assert_stable(&document_of(vec![paragraph(
        WPS,
        AnyElementContent::from(shape(true)),
    )]));
}

#[test]
fn a_canvas_with_a_shape_built_with_no_names_reads_the_same_after_a_second_write() {
    assert_stable(&document_of(vec![paragraph(
        WPC,
        AnyElementContent::from(canvas(shape(false))),
    )]));
}

#[test]
fn a_drawing_in_words_own_names_reads_the_same_after_a_second_write() {
    assert_stable(&document_of(vec![paragraph(
        WPS,
        AnyElementContent::from(word_shape(true)),
    )]));
}

#[test]
fn a_drawing_read_from_the_alternate_content_word_writes_reads_the_same_after_a_second_write() {
    let mut word_file = document_of(vec![in_words_alternate_content(paragraph(
        WPS,
        AnyElementContent::from(word_shape(true)),
    ))]);
    word_file.unknown_mut().namespace_declarations = vec![
        NamespaceDeclaration::new("mc".to_owned(), MC.to_owned()),
        NamespaceDeclaration::new("wps".to_owned(), WPS.to_owned()),
    ];
    for mode in modes() {
        assert_stable_in(&written_and_read(&word_file, mode), mode);
    }
}
