#[cfg(feature = "transitional")]
mod whole_file {
    use officekit::docx::Docx;
    #[cfg(feature = "binary-format")]
    use officekit::office_file::OfficeFile;
    use officekit::parts::kinds::WordprocessingKind;
    use officekit_markup::drawing::{
        AnyElementContent, DrawingElementID, GraphicObject, GraphicObjectData, PositiveCoordinate,
    };
    use officekit_markup::wordprocessing::{
        BlockContent, Document, DocumentBody, DrawingMLObject, DrawingObjectContent, Paragraph,
        ParagraphContent, TextRun, TextRunContent,
    };
    use officekit_markup::wordprocessing_drawing::{
        DrawingObjectNonVisualProperties, DrawingObjectSize, InlineDrawingMLObject,
        WordprocessingMLShape,
    };
    use officekit_xml::name::QName;
    use officekit_xml::packaging::package::OpenOptions;

    use super::{Unknown, W, WPS, word_shape};

    /// A main document whose one paragraph shows `shape` in the line of text.
    fn document_with(shape: WordprocessingMLShape) -> Document {
        let mut data = GraphicObjectData::default();
        data.content = vec![AnyElementContent::from(shape)];
        data.uri = Some(Box::from(WPS));
        let mut graphic = GraphicObject::default();
        graphic.data = Box::new(data);
        let mut properties = DrawingObjectNonVisualProperties::default();
        properties.id = Some(DrawingElementID::from("1".to_owned()));
        properties.name = Some(Box::from("Text Box 1"));
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
        let mut paragraph = Paragraph::default();
        paragraph.content = vec![ParagraphContent::from(run)];
        let mut body = DocumentBody::default();
        body.content = vec![BlockContent::from(paragraph)];
        let mut document = Document::default();
        document.body = Some(Box::new(body));
        document
    }

    /// The shape the first drawing of `document` shows.
    fn shape_in(document: &Document) -> Option<&WordprocessingMLShape> {
        let BlockContent::Paragraph(paragraph) = document.body.as_ref()?.content.first()? else {
            return None;
        };
        let ParagraphContent::TextRun(run) = paragraph.content.first()? else {
            return None;
        };
        let TextRunContent::DrawingMLObject(drawing) = run.content.first()? else {
            return None;
        };
        let DrawingObjectContent::InlineDrawingMLObject(inline) = drawing.content.first()? else {
            return None;
        };
        let AnyElementContent::WordprocessingMLShape(shape) =
            inline.graphic_object.data.content.first()?
        else {
            return None;
        };
        Some(shape)
    }

    /// A new document holding the shape with Word's names, saved in Transitional and opened.
    fn saved() -> Docx {
        let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
        docx.set_main_document(&document_with(word_shape()))
            .unwrap();
        let (bytes, warnings) = docx.to_bytes_transitional().unwrap();
        assert_eq!(warnings, []);
        Docx::from_bytes(bytes, OpenOptions::default()).unwrap()
    }

    #[test]
    fn a_shape_keeps_words_names_through_a_saved_file() {
        let docx = saved();
        let document = docx.main_document().unwrap();
        let shape = shape_in(&document).unwrap();
        assert_eq!(
            shape.unknown().unwrap_or_else(Unknown::empty).name,
            Some(Box::new(QName::prefixed("wps", "wsp", WPS)))
        );
        assert_eq!(
            super::text_box(shape)
                .unwrap()
                .rich_text_box_content_container
                .unknown()
                .unwrap_or_else(Unknown::empty)
                .name,
            Some(Box::new(QName::prefixed("w", "txbxContent", W)))
        );
        let mut built = word_shape();
        let mut read = shape.clone();
        built.unknown_mut().namespace_declarations.clear();
        read.unknown_mut().namespace_declarations.clear();
        assert_eq!(read, built);
        let part = docx
            .package()
            .part_bytes(docx.main_document_part())
            .unwrap();
        let text = String::from_utf8(part).unwrap();
        assert!(text.contains("<wps:wsp"), "{text}");
        assert!(text.contains("<w:txbxContent"), "{text}");
        assert!(!text.contains("<wp:wsp"), "{text}");
    }

    /// The Word file `file` is.
    #[cfg(feature = "binary-format")]
    fn word_of(file: OfficeFile) -> Option<Docx> {
        let OfficeFile::Word(docx) = file else {
            return None;
        };
        Some(docx)
    }

    #[cfg(feature = "binary-format")]
    #[test]
    fn the_binary_form_keeps_the_names_a_shape_was_read_with() {
        use officekit::binary;
        use rkyv::rancor::Error;

        let docx = saved();
        let document = docx.main_document().unwrap();
        let archive = binary::encode(&OfficeFile::Word(docx)).unwrap();
        let back = word_of(binary::decode(&archive).unwrap()).unwrap();
        let restored = back.main_document().unwrap();
        assert_eq!(restored, document);
        assert_eq!(
            shape_in(&restored)
                .unwrap()
                .unknown()
                .unwrap_or_else(Unknown::empty)
                .name,
            Some(Box::new(QName::prefixed("wps", "wsp", WPS)))
        );

        let shape = word_shape();
        let bytes = rkyv::to_bytes::<Error>(&shape).unwrap();
        let unarchived = rkyv::from_bytes::<WordprocessingMLShape, Error>(&bytes).unwrap();
        assert_eq!(unarchived, shape);
    }
}

use officekit_markup::Read as _;
use officekit_markup::Spelling as _;
use officekit_markup::Unknown;
use officekit_markup::drawing::{AnyElementContent, GeometryContent, GraphicObjectData};
use officekit_markup::vml::{self, TextBox, TextBoxContent};
use officekit_markup::wordprocessing::{BlockContent, Paragraph, RichTextBoxContentContainer};
use officekit_markup::wordprocessing_drawing::{
    self, DrawingCanvasContent, GroupShape, GroupShapeContent, NonVisualDrawingPropertiesForAShape,
    ShapeNonVisualPropertiesContent, ShapeTextContent, TextualContentsOfShapeTxbx,
    WordprocessingMLDrawingCanvas, WordprocessingMLShape, WordprocessingMLShapeGroup,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::name::QName;
use officekit_xml::write::Scope;

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const V: &str = "urn:schemas-microsoft-com:vml";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WNE: &str = "http://schemas.microsoft.com/office/word/2006/wordml";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const WPC: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas";
const WPG: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

/// Word's names for a shape and its children.
const WORD: (&str, &str) = ("wps", WPS);

/// ECMA-376's names for a shape and its children (Part 1, 20.4.2.37 to 20.4.2.42).
const STANDARD: (&str, &str) = ("wp", WP);

/// An element `prefix:local` of `uri` holding `children`.
fn element(prefix: &str, local: &str, uri: &str, children: Vec<Element>) -> Element {
    let mut built = Element::new(QName::prefixed(prefix, local, uri));
    built.children = children.into_iter().map(Node::Element).collect();
    built
}

/// `element` with the unprefixed attribute `local` set to `value`.
fn with_attribute(mut element: Element, local: &str, value: &str) -> Element {
    element.attributes.push(Attribute::new(
        QName::unprefixed(local, ""),
        value.to_owned(),
    ));
    element
}

/// A shape with a preset geometry and a text box holding one paragraph: the shape and its
/// children named in `shape`'s prefix and namespace, the text box's content in `content`'s.
fn shape_tree(shape: (&str, &str), content: (&str, &str)) -> Element {
    let (prefix, uri) = shape;
    let geometry = with_attribute(element("a", "prstGeom", A, Vec::new()), "prst", "rect");
    let paragraph = element("w", "p", W, Vec::new());
    element(
        prefix,
        "wsp",
        uri,
        vec![
            with_attribute(element(prefix, "cNvSpPr", uri, Vec::new()), "txBox", "1"),
            element(prefix, "spPr", uri, vec![geometry]),
            element(
                prefix,
                "txbx",
                uri,
                vec![element(
                    content.0,
                    "txbxContent",
                    content.1,
                    vec![paragraph],
                )],
            ),
            element(prefix, "bodyPr", uri, Vec::new()),
        ],
    )
}

/// A shape with no geometry and no text box, named as Word names it.
fn bare_word_shape() -> Element {
    element(
        "wps",
        "wsp",
        WPS,
        vec![
            element("wps", "spPr", WPS, Vec::new()),
            element("wps", "bodyPr", WPS, Vec::new()),
        ],
    )
}

/// The items an `a:graphicData` of `uri` holding `payload` reads as.
fn read_payload(uri: &str, payload: Element) -> Vec<AnyElementContent> {
    let data = with_attribute(element("a", "graphicData", A, vec![payload]), "uri", uri);
    GraphicObjectData::read(&data).content
}

/// The name of each element of `element`'s tree, in document order.
fn names(element: &Element) -> Vec<QName> {
    let mut found = vec![element.name.clone()];
    for child in element.child_elements() {
        found.extend(names(child));
    }
    found
}

/// The shape a list of one item holds.
fn shape_of(items: Vec<AnyElementContent>) -> Option<WordprocessingMLShape> {
    let Ok([AnyElementContent::WordprocessingMLShape(shape)]) = <[_; 1]>::try_from(items) else {
        return None;
    };
    Some(*shape)
}

/// The group a list of one item holds.
fn group_of(items: Vec<AnyElementContent>) -> Option<WordprocessingMLShapeGroup> {
    let Ok([AnyElementContent::WordprocessingMLShapeGroup(group)]) = <[_; 1]>::try_from(items)
    else {
        return None;
    };
    Some(*group)
}

/// The canvas a list of one item holds.
fn canvas_of(items: Vec<AnyElementContent>) -> Option<WordprocessingMLDrawingCanvas> {
    let Ok([AnyElementContent::WordprocessingMLDrawingCanvas(canvas)]) = <[_; 1]>::try_from(items)
    else {
        return None;
    };
    Some(*canvas)
}

/// The inner group of a group that holds a shape, then that group.
fn inner_group_of(group: &WordprocessingMLShapeGroup) -> Option<&GroupShape> {
    let [
        GroupShapeContent::WordprocessingMLShape(_),
        GroupShapeContent::GroupShape(inner),
    ] = group.content.as_slice()
    else {
        return None;
    };
    Some(inner)
}

/// The content of a VML text box.
fn vml_content(text_box: &TextBox) -> Option<&RichTextBoxContentContainer> {
    let TextBoxContent::RichTextBoxContentContainer(content) = text_box.content.as_deref()? else {
        return None;
    };
    Some(content)
}

/// The text box of a shape.
fn text_box(shape: &WordprocessingMLShape) -> Option<&TextualContentsOfShapeTxbx> {
    let ShapeTextContent::TextualContentsOfShapeTxbx(found) = shape.text_content.as_deref()? else {
        return None;
    };
    Some(found)
}

/// The preset of a shape's geometry.
fn preset(shape: &WordprocessingMLShape) -> Option<String> {
    let GeometryContent::PresetGeometry(geometry) = shape.properties.geometry_content.as_deref()?
    else {
        return None;
    };
    geometry
        .prst
        .as_ref()
        .map(|found| found.spelling(ConformanceClass::Transitional).into_owned())
}

/// `shape` with every name it was read with taken off.
fn without_names(mut shape: WordprocessingMLShape) -> WordprocessingMLShape {
    shape.unknown = unnamed(shape.unknown.take());
    shape.properties.unknown = unnamed(shape.properties.unknown.take());
    shape.body_properties.unknown = unnamed(shape.body_properties.unknown.take());
    if let ShapeNonVisualPropertiesContent::NonVisualDrawingPropertiesForAShape(properties) =
        &mut *shape.non_visual_properties_content
    {
        properties.unknown = unnamed(properties.unknown.take());
    }
    if let Some(ShapeTextContent::TextualContentsOfShapeTxbx(found)) =
        shape.text_content.as_deref_mut()
    {
        found.unknown = unnamed(found.unknown.take());
        found.rich_text_box_content_container.unknown =
            unnamed(found.rich_text_box_content_container.unknown.take());
    }
    shape
}

/// `unknown` without the name it was read with; `None` when nothing else is left.
fn unnamed(unknown: Option<Box<Unknown>>) -> Option<Box<Unknown>> {
    let mut found = unknown?;
    found.name = None;
    (!found.is_empty()).then_some(found)
}

/// A shape built in code with a text box holding one paragraph, under the standard's names.
fn standard_shape() -> WordprocessingMLShape {
    let mut found = TextualContentsOfShapeTxbx::default();
    found.rich_text_box_content_container.content = vec![BlockContent::from(Paragraph::default())];
    let mut shape = WordprocessingMLShape::default();
    shape.non_visual_properties_content = Box::new(ShapeNonVisualPropertiesContent::from(
        NonVisualDrawingPropertiesForAShape::default(),
    ));
    shape.text_content = Some(Box::new(ShapeTextContent::from(found)));
    shape
}

/// `standard_shape` with Word's names set on it and on each child, and the namespaces of those
/// names declared on the shape.
fn word_shape() -> WordprocessingMLShape {
    let mut shape = standard_shape();
    shape.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "wsp", WPS)));
    shape.unknown_mut().namespace_declarations = vec![
        NamespaceDeclaration::new("wps".to_owned(), WPS.to_owned()),
        NamespaceDeclaration::new("w".to_owned(), W.to_owned()),
    ];
    shape.properties.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "spPr", WPS)));
    shape.body_properties.unknown_mut().name =
        Some(Box::new(QName::prefixed("wps", "bodyPr", WPS)));
    if let ShapeNonVisualPropertiesContent::NonVisualDrawingPropertiesForAShape(properties) =
        &mut *shape.non_visual_properties_content
    {
        properties.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "cNvSpPr", WPS)));
    }
    if let Some(ShapeTextContent::TextualContentsOfShapeTxbx(found)) =
        shape.text_content.as_deref_mut()
    {
        found.unknown_mut().name = Some(Box::new(QName::prefixed("wps", "txbx", WPS)));
        found.rich_text_box_content_container.unknown_mut().name =
            Some(Box::new(QName::prefixed("w", "txbxContent", W)));
    }
    shape
}

fn written(shape: &WordprocessingMLShape) -> Element {
    wordprocessing_drawing::write::wordprocessing_ml_shape(shape, &mut Scope::default())
}

#[test]
fn a_word_shape_reads_as_the_typed_shape_with_its_geometry_and_text_box() {
    let shape = shape_of(read_payload(WPS, shape_tree(WORD, ("w", W)))).unwrap();
    assert!(matches!(
        *shape.non_visual_properties_content,
        ShapeNonVisualPropertiesContent::NonVisualDrawingPropertiesForAShape(_)
    ));
    assert_eq!(preset(&shape).as_deref(), Some("rect"));
    let content = &text_box(&shape).unwrap().rich_text_box_content_container;
    assert!(matches!(
        content.content.as_slice(),
        [BlockContent::Paragraph(_)]
    ));
    assert_eq!(shape.unknown().unwrap_or_else(Unknown::empty).children, []);
    assert_eq!(
        shape.unknown().unwrap_or_else(Unknown::empty).absent,
        Vec::<usize>::new()
    );
    assert_eq!(
        shape
            .properties
            .unknown()
            .unwrap_or_else(Unknown::empty)
            .children,
        []
    );
    assert_eq!(
        text_box(&shape)
            .unwrap()
            .unknown()
            .unwrap_or_else(Unknown::empty)
            .children,
        []
    );
    assert_eq!(
        shape.unknown().unwrap_or_else(Unknown::empty).name,
        Some(Box::new(QName::prefixed("wps", "wsp", WPS)))
    );
    assert_eq!(
        content.unknown().unwrap_or_else(Unknown::empty).name,
        Some(Box::new(QName::prefixed("w", "txbxContent", W)))
    );
}

#[test]
fn a_word_shape_is_written_back_with_the_names_it_was_read_with() {
    let tree = shape_tree(WORD, ("w", W));
    let shape = shape_of(read_payload(WPS, tree.clone())).unwrap();
    let back = written(&shape);
    assert_eq!(names(&back), names(&tree));
    assert_eq!(
        names(&back)
            .iter()
            .map(|found| format!("{}:{}", found.prefix, found.local))
            .collect::<Vec<_>>(),
        [
            "wps:wsp",
            "wps:cNvSpPr",
            "wps:spPr",
            "a:prstGeom",
            "wps:txbx",
            "w:txbxContent",
            "w:p",
            "wps:bodyPr"
        ]
    );
}

#[test]
fn a_word_group_reads_its_shape_and_inner_group_typed_and_writes_their_names_back() {
    let inner = element(
        "wpg",
        "grpSp",
        WPG,
        vec![
            element("wpg", "cNvGrpSpPr", WPG, Vec::new()),
            element("wpg", "grpSpPr", WPG, Vec::new()),
            bare_word_shape(),
        ],
    );
    let tree = element(
        "wpg",
        "wgp",
        WPG,
        vec![
            element("wpg", "cNvGrpSpPr", WPG, Vec::new()),
            element("wpg", "grpSpPr", WPG, Vec::new()),
            bare_word_shape(),
            inner,
        ],
    );
    let group = group_of(read_payload(WPG, tree.clone())).unwrap();
    assert_eq!(group.unknown().unwrap_or_else(Unknown::empty).children, []);
    assert_eq!(
        group.unknown().unwrap_or_else(Unknown::empty).absent,
        Vec::<usize>::new()
    );
    let inner_group = inner_group_of(&group).unwrap();
    assert!(matches!(
        inner_group.content.as_slice(),
        [GroupShapeContent::WordprocessingMLShape(_)]
    ));
    assert_eq!(
        inner_group.unknown().unwrap_or_else(Unknown::empty).name,
        Some(Box::new(QName::prefixed("wpg", "grpSp", WPG)))
    );
    let back =
        wordprocessing_drawing::write::wordprocessing_ml_shape_group(&group, &mut Scope::default());
    assert_eq!(names(&back), names(&tree));
}

#[test]
fn a_word_canvas_reads_its_shape_typed_and_writes_their_names_back() {
    let tree = element("wpc", "wpc", WPC, vec![bare_word_shape()]);
    let canvas = canvas_of(read_payload(WPC, tree.clone())).unwrap();
    assert!(matches!(
        canvas.content.as_slice(),
        [DrawingCanvasContent::WordprocessingMLShape(_)]
    ));
    assert_eq!(
        canvas.unknown().unwrap_or_else(Unknown::empty).name,
        Some(Box::new(QName::prefixed("wpc", "wpc", WPC)))
    );
    let back = wordprocessing_drawing::write::wordprocessing_ml_drawing_canvas(
        &canvas,
        &mut Scope::default(),
    );
    assert_eq!(names(&back), names(&tree));
}

#[test]
fn the_standards_names_read_as_the_same_typed_shape_with_no_name_kept() {
    let tree = shape_tree(STANDARD, STANDARD);
    let standard = shape_of(read_payload(WPS, tree.clone())).unwrap();
    let word = shape_of(read_payload(WPS, shape_tree(WORD, ("w", W)))).unwrap();
    assert_eq!(standard.unknown().unwrap_or_else(Unknown::empty).name, None);
    assert_eq!(
        text_box(&standard)
            .unwrap()
            .rich_text_box_content_container
            .unknown()
            .unwrap_or_else(Unknown::empty)
            .name,
        None
    );
    assert_eq!(without_names(word), standard);
    assert_eq!(names(&written(&standard)), names(&tree));
}

#[test]
fn a_text_boxs_content_in_wne_reads_as_its_content_and_is_written_back_in_wne() {
    let tree = shape_tree(WORD, ("wne", WNE));
    let shape = shape_of(read_payload(WPS, tree.clone())).unwrap();
    let content = &text_box(&shape).unwrap().rich_text_box_content_container;
    assert!(matches!(
        content.content.as_slice(),
        [BlockContent::Paragraph(_)]
    ));
    assert_eq!(
        content.unknown().unwrap_or_else(Unknown::empty).name,
        Some(Box::new(QName::prefixed("wne", "txbxContent", WNE)))
    );
    assert_eq!(names(&written(&shape)), names(&tree));
}

#[test]
fn a_shape_built_in_code_takes_the_standards_names() {
    let back = written(&standard_shape());
    assert_eq!(
        names(&back),
        [
            QName::prefixed("wp", "wsp", WP),
            QName::prefixed("wp", "cNvSpPr", WP),
            QName::prefixed("wp", "spPr", WP),
            QName::prefixed("wp", "txbx", WP),
            QName::prefixed("wp", "txbxContent", WP),
            QName::prefixed("w", "p", W),
            QName::prefixed("wp", "bodyPr", WP),
        ]
    );
}

#[test]
fn a_shape_built_in_code_with_words_names_is_written_with_them_and_reads_back_equal() {
    let shape = word_shape();
    let back = written(&shape);
    assert_eq!(
        names(&back),
        [
            QName::prefixed("wps", "wsp", WPS),
            QName::prefixed("wps", "cNvSpPr", WPS),
            QName::prefixed("wps", "spPr", WPS),
            QName::prefixed("wps", "txbx", WPS),
            QName::prefixed("w", "txbxContent", W),
            QName::prefixed("w", "p", W),
            QName::prefixed("wps", "bodyPr", WPS),
        ]
    );
    assert_eq!(WordprocessingMLShape::read(&back), shape);
}

#[test]
fn a_text_boxs_content_in_a_vml_text_box_reads_as_it_did() {
    let tree = element(
        "v",
        "textbox",
        V,
        vec![element(
            "w",
            "txbxContent",
            W,
            vec![element("w", "p", W, Vec::new())],
        )],
    );
    let read = TextBox::read(&tree);
    let content = vml_content(&read).unwrap();
    assert!(matches!(
        content.content.as_slice(),
        [BlockContent::Paragraph(_)]
    ));
    assert_eq!(content.unknown().unwrap_or_else(Unknown::empty).name, None);
    let back = vml::write::text_box(&read, &mut Scope::default());
    assert_eq!(names(&back), names(&tree));
}
