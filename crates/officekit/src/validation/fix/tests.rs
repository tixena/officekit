use officekit_markup::Descend;
use officekit_markup::drawing::DrawingElementID;
use officekit_markup::vml::{self, Rectangle, ShapeDefinition};
use officekit_markup::vml_office_drawing::EmbeddedOLEObject;
use officekit_markup::wordprocessing::{
    BlockContent, BookmarkStart, DecimalNumberValue, Document, DocumentBody, DrawingMLObject,
    DrawingObjectContent, EmbeddedObject, Header, Paragraph, ParagraphContent, TextRun,
    TextRunContent, VMLObject, VmlElementContent, read, write,
};
use officekit_markup::wordprocessing_drawing::{
    self, DrawingObjectNonVisualProperties, InlineDrawingMLObject,
};
use officekit_strict::traversal::Visit as _;
use officekit_xml::element::{Document as XmlDocument, Element};
use officekit_xml::name::OfficeVersion;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::package::{DECLARATION, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::parse::parse;
use officekit_xml::write::{Scope, WriteOptions};

use super::{Fix, fix_package};
#[cfg(feature = "transitional")]
use crate::docx::Docx;
use crate::validation::error::{ErrorCode, ValidationError};
use crate::validation::validator::{Validator, reachable_parts};

const HEADER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
const HEADER_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const NAMESPACES: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const VERSION: OfficeVersion = OfficeVersion::Microsoft365;

/// The `wp:docPr`, `v:rect` and `v:shape` ids it visits, in document order.
struct Ids(Vec<String>);

impl officekit_markup::Visitor for Ids {
    fn vml(&mut self) -> Option<&mut dyn vml::Visitor> {
        Some(self)
    }

    fn wordprocessing_drawing(&mut self) -> Option<&mut dyn wordprocessing_drawing::Visitor> {
        Some(self)
    }
}

impl vml::Visitor for Ids {
    fn rectangle(&mut self, node: &Rectangle) -> Descend {
        self.0.extend(node.id.clone());
        Descend::Yes
    }

    fn shape_definition(&mut self, node: &ShapeDefinition) -> Descend {
        self.0.extend(node.id.clone());
        Descend::Yes
    }
}

impl wordprocessing_drawing::Visitor for Ids {
    fn drawing_object_non_visual_properties(
        &mut self,
        node: &DrawingObjectNonVisualProperties,
    ) -> Descend {
        self.0.extend(node.id.as_ref().map(|id| id.0.clone()));
        Descend::Yes
    }
}

fn document_xml(root: &str, body: &str) -> String {
    format!("<w:{root} {NAMESPACES}>{body}</w:{root}>")
}

fn doc_pr_xml(id: &str) -> String {
    format!(
        r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><wp:docPr id="{id}" name="Picture"/></wp:inline></w:drawing></w:r></w:p>"#
    )
}

fn rect_xml(id: &str) -> String {
    format!(r#"<w:p><w:r><w:pict><v:rect id="{id}"/></w:pict></w:r></w:p>"#)
}

fn package_xml(body: &str, header: Option<&str>) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(
            &main,
            MAIN,
            document_xml("document", &format!("<w:body>{body}</w:body>")).as_bytes(),
        )
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    if let Some(content) = header {
        let part = PartName::new("/word/header2.xml").unwrap();
        package
            .set_part_bytes(
                &part,
                HEADER_CONTENT,
                document_xml("hdr", content).as_bytes(),
            )
            .unwrap();
        package.relate(Some(&main), &part, HEADER).unwrap();
    }
    package
}

fn text(package: &Package, name: &str) -> String {
    String::from_utf8(package.part_bytes(&PartName::new(name).unwrap()).unwrap()).unwrap()
}

fn duplicates(errors: &[ValidationError]) -> usize {
    errors
        .iter()
        .filter(|error| error.code == ErrorCode::DuplicateAttributeValue)
        .count()
}

fn values(xml: &str, attribute: &str) -> Vec<String> {
    let marker = format!(" {attribute}=\"");
    xml.split(&marker)
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
        .collect()
}

/// A paragraph holding a run of `content`.
fn in_run(content: TextRunContent) -> BlockContent {
    let mut run = TextRun::default();
    run.content = vec![content];
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::TextRun(run)];
    BlockContent::Paragraph(paragraph)
}

/// A paragraph holding an inline drawing whose `wp:docPr` has `id`.
fn doc_pr(id: &str) -> BlockContent {
    let mut properties = DrawingObjectNonVisualProperties::default();
    properties.id = Some(DrawingElementID::from(id.to_owned()));
    properties.name = Some("Picture".to_owned());
    let mut inline = InlineDrawingMLObject::default();
    inline.drawing_object_non_visual_properties = Box::new(properties);
    let mut drawing = DrawingMLObject::default();
    drawing.content = vec![DrawingObjectContent::InlineDrawingMLObject(inline)];
    in_run(TextRunContent::DrawingMLObject(drawing))
}

/// A paragraph holding a VML picture of `shape`.
fn picture(shape: VmlElementContent) -> BlockContent {
    let mut picture = VMLObject::default();
    picture.content = vec![shape];
    in_run(TextRunContent::VMLObject(picture))
}

/// A paragraph holding a VML rectangle of `id`.
fn rect(id: &str) -> BlockContent {
    let mut rectangle = Rectangle::default();
    rectangle.id = Some(id.to_owned());
    picture(VmlElementContent::Rectangle(rectangle))
}

/// A paragraph holding a VML shape of `id`.
fn shape(id: &str) -> BlockContent {
    let mut definition = ShapeDefinition::default();
    definition.id = Some(id.to_owned());
    picture(VmlElementContent::ShapeDefinition(definition))
}

/// The part text of `root`, after the declaration Office writes.
fn xml_of(root: Element) -> Vec<u8> {
    XmlDocument::new(DECLARATION, root).to_xml(WriteOptions::default())
}

/// A Word document whose body holds `body`, with a header part holding `header` when given.
fn typed_package(body: Vec<BlockContent>, header: Option<Vec<BlockContent>>) -> Package {
    let mut document_body = DocumentBody::default();
    document_body.content = body;
    let mut document = Document::default();
    document.body = Some(Box::new(document_body));
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(
            &main,
            MAIN,
            &xml_of(write::document(&document, &mut Scope::default())),
        )
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    if let Some(content) = header {
        let mut node = Header::default();
        node.content = content;
        let part = PartName::new("/word/header2.xml").unwrap();
        package
            .set_part_bytes(
                &part,
                HEADER_CONTENT,
                &xml_of(write::header(&node, &mut Scope::default())),
            )
            .unwrap();
        package.relate(Some(&main), &part, HEADER).unwrap();
    }
    package
}

/// The drawing and shape ids of the main document, read back through the typed tree.
fn document_ids(package: &Package) -> Vec<String> {
    let bytes = package
        .part_bytes(&PartName::new("/word/document.xml").unwrap())
        .unwrap();
    let mut ids = Ids(Vec::new());
    read::document(&parse(&bytes).unwrap().root).visit(&mut ids);
    ids.0
}

/// The drawing and shape ids of the header, read back through the typed tree.
fn header_ids(package: &Package) -> Vec<String> {
    let bytes = package
        .part_bytes(&PartName::new("/word/header2.xml").unwrap())
        .unwrap();
    let mut ids = Ids(Vec::new());
    read::header(&parse(&bytes).unwrap().root).visit(&mut ids);
    ids.0
}

#[test]
fn numbered_ids_take_the_next_numbers() {
    let mut package = typed_package(
        vec![doc_pr("1"), doc_pr("1"), doc_pr("7"), doc_pr("1")],
        None,
    );
    let validator = Validator::new(VERSION);
    assert_eq!(duplicates(&validator.validate(&package)), 1);
    let validation = validator
        .validate_fixing(&mut package, Fix::Trivial)
        .unwrap();
    assert_eq!(document_ids(&package), ["1", "8", "7", "9"]);
    assert_eq!(validation.fixed.len(), 2);
    assert_eq!(duplicates(&validation.errors), 0);
    assert_eq!(validation.fixed[0].from, "1");
    assert_eq!(validation.fixed[0].to, "8");
}

#[test]
fn suffixed_ids_avoid_the_values_of_other_parts() {
    let mut package = typed_package(
        vec![shape("Rectangle 13_2")],
        Some(vec![rect("Rectangle 13"), rect("Rectangle 13")]),
    );
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed.len(), 1);
    assert_eq!(fixed[0].to, "Rectangle 13_3");
    assert_eq!(header_ids(&package), ["Rectangle 13", "Rectangle 13_3"]);
}

#[test]
fn a_referenced_id_is_not_renamed() {
    let mut ole = EmbeddedOLEObject::default();
    ole.shape_id = Some("_x0000_i1025".to_owned());
    let mut object = EmbeddedObject::default();
    object.vml_element_content = vec![VmlElementContent::EmbeddedOLEObject(ole)];
    let mut package = typed_package(
        vec![
            shape("_x0000_i1025"),
            shape("_x0000_i1025"),
            in_run(TextRunContent::EmbeddedObject(object)),
        ],
        None,
    );
    let before = text(&package, "/word/document.xml");
    let validation = Validator::new(VERSION)
        .validate_fixing(&mut package, Fix::Trivial)
        .unwrap();
    assert_eq!(validation.fixed, Vec::new());
    assert_eq!(text(&package, "/word/document.xml"), before);
    assert_eq!(duplicates(&validation.errors), 1);
}

#[test]
fn an_id_outside_the_list_is_not_renamed() {
    let bookmark = |name: &str| {
        let mut start = BookmarkStart::default();
        start.id = Some(DecimalNumberValue::from("0".to_owned()));
        start.name = Some(name.to_owned());
        ParagraphContent::BookmarkStart(start)
    };
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![bookmark("a"), bookmark("b")];
    let mut package = typed_package(vec![BlockContent::Paragraph(paragraph)], None);
    let validation = Validator::new(VERSION)
        .validate_fixing(&mut package, Fix::Trivial)
        .unwrap();
    assert_eq!(validation.fixed, Vec::new());
    assert_eq!(duplicates(&validation.errors), 1);
}

#[test]
fn the_same_id_in_two_parts_is_not_renamed() {
    let mut package = typed_package(vec![doc_pr("1")], Some(vec![doc_pr("1")]));
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed, Vec::new());
}

#[cfg(feature = "transitional")]
#[test]
fn off_changes_nothing() {
    let source = typed_package(vec![doc_pr("1"), doc_pr("1")], None);
    let mut docx = Docx::from_package(source, OpenOptions::default()).unwrap();
    let validator = Validator::new(VERSION);
    let before = text(docx.package(), "/word/document.xml");
    let expected = validator.validate(docx.package());
    let validation = docx.validate(&validator, Fix::Off).unwrap();
    assert_eq!(text(docx.package(), "/word/document.xml"), before);
    assert_eq!(validation.fixed, Vec::new());
    assert_eq!(validation.errors, expected);
}

#[test]
fn a_part_with_nothing_to_rename_keeps_its_bytes() {
    let mut package = typed_package(vec![doc_pr("1")], Some(vec![rect("a"), rect("a")]));
    let before = text(&package, "/word/document.xml");
    fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(text(&package, "/word/document.xml"), before);
}

#[test]
fn a_shape_in_a_fallback_keeps_its_alternate_content() {
    let alternate = |id: &str| {
        format!(
            r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wp"><w:drawing/></mc:Choice><mc:Fallback><w:pict><v:rect id="{id}"/></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>"#
        )
    };
    let mut package = package_xml(
        &format!("{}{}", alternate("shape_0"), alternate("shape_0")),
        None,
    );
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed.len(), 1);
    assert_eq!(
        fixed[0].line(),
        "/word/document.xml /w:document[1]/w:body[1]/w:p[2]/w:r[1]/mc:AlternateContent[1]/mc:Fallback[1]/w:pict[1]/v:rect[1]: renamed id \"shape_0\" to \"shape_0_2\""
    );
    let xml = text(&package, "/word/document.xml");
    assert_eq!(xml.matches("<mc:AlternateContent>").count(), 2);
    assert_eq!(xml.matches("<mc:Choice Requires=\"wp\">").count(), 2);
    assert_eq!(values(&xml, "id"), ["shape_0", "shape_0_2"]);
}

fn without_default(package: &Package, extension: &str) -> Package {
    use std::io::{Cursor, Read as _, Write as _};

    use zip::write::SimpleFileOptions;

    let mut archive = zip::ZipArchive::new(Cursor::new(package.to_bytes().unwrap())).unwrap();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        let name = entry.name().unwrap().into_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "[Content_Types].xml" {
            let types = String::from_utf8(bytes).unwrap();
            let marker = format!("<Default Extension=\"{extension}\"");
            let start = types.find(&marker).unwrap();
            let end = start + types[start..].find("/>").unwrap() + 2;
            bytes = format!("{}{}", &types[..start], &types[end..]).into_bytes();
        }
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&bytes).unwrap();
    }
    Package::from_bytes(writer.finish().unwrap().into_inner()).unwrap()
}

#[test]
fn a_part_with_no_content_type_is_skipped_and_not_recorded() {
    const CUSTOM_XML: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";
    const CUSTOM_XML_CONTENT: &str = "application/x-custom";
    let mut source = package_xml(&doc_pr_xml("1"), None);
    let main = PartName::new("/word/document.xml").unwrap();
    let custom = PartName::new("/customXml/item1.dat").unwrap();
    source
        .set_part_bytes(
            &custom,
            CUSTOM_XML_CONTENT,
            document_xml("item", &format!("{}{}", rect_xml("a"), rect_xml("a"))).as_bytes(),
        )
        .unwrap();
    source.relate(Some(&main), &custom, CUSTOM_XML).unwrap();
    let mut package = without_default(&source, "dat");
    assert_eq!(package.content_type(&custom), None);
    assert!(
        reachable_parts(&package, VERSION)
            .iter()
            .any(|(part, _)| *part == custom)
    );
    let before = text(&package, "/customXml/item1.dat");
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed, Vec::new());
    assert_eq!(text(&package, "/customXml/item1.dat"), before);
}
