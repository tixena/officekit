use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;

use super::{Fix, fix_package};
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

fn document_xml(root: &str, body: &str) -> String {
    format!("<w:{root} {NAMESPACES}>{body}</w:{root}>")
}

fn doc_pr(id: &str) -> String {
    format!(
        r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><wp:docPr id="{id}" name="Picture"/></wp:inline></w:drawing></w:r></w:p>"#
    )
}

fn rect(id: &str) -> String {
    format!(r#"<w:p><w:r><w:pict><v:rect id="{id}"/></w:pict></w:r></w:p>"#)
}

fn shape(id: &str) -> String {
    format!(r#"<w:p><w:r><w:pict><v:shape id="{id}"/></w:pict></w:r></w:p>"#)
}

fn package(body: &str, header: Option<&str>) -> Package {
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

#[test]
fn numbered_ids_take_the_next_numbers() {
    let body = format!(
        "{}{}{}{}",
        doc_pr("1"),
        doc_pr("1"),
        doc_pr("7"),
        doc_pr("1")
    );
    let mut package = package(&body, None);
    let validator = Validator::new(VERSION);
    assert_eq!(duplicates(&validator.validate(&package)), 1);
    let validation = validator
        .validate_fixing(&mut package, Fix::Trivial)
        .unwrap();
    assert_eq!(
        values(&text(&package, "/word/document.xml"), "id"),
        ["1", "8", "7", "9"]
    );
    assert_eq!(validation.fixed.len(), 2);
    assert_eq!(duplicates(&validation.errors), 0);
    assert_eq!(validation.fixed[0].from, "1");
    assert_eq!(validation.fixed[0].to, "8");
}

#[test]
fn suffixed_ids_avoid_the_values_of_other_parts() {
    let mut package = package(
        &shape("Rectangle 13_2"),
        Some(&format!("{}{}", rect("Rectangle 13"), rect("Rectangle 13"))),
    );
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed.len(), 1);
    assert_eq!(fixed[0].to, "Rectangle 13_3");
    assert_eq!(
        values(&text(&package, "/word/header2.xml"), "id"),
        ["Rectangle 13", "Rectangle 13_3"]
    );
}

#[test]
fn a_referenced_id_is_not_renamed() {
    let body = format!(
        r#"{}{}<w:p><w:r><w:object><o:OLEObject ShapeID="_x0000_i1025"/></w:object></w:r></w:p>"#,
        shape("_x0000_i1025"),
        shape("_x0000_i1025")
    );
    let mut package = package(&body, None);
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
    let body = r#"<w:p><w:bookmarkStart w:id="0" w:name="a"/><w:bookmarkStart w:id="0" w:name="b"/></w:p>"#;
    let mut package = package(body, None);
    let validation = Validator::new(VERSION)
        .validate_fixing(&mut package, Fix::Trivial)
        .unwrap();
    assert_eq!(validation.fixed, Vec::new());
    assert_eq!(duplicates(&validation.errors), 1);
}

#[test]
fn the_same_id_in_two_parts_is_not_renamed() {
    let mut package = package(&doc_pr("1"), Some(&doc_pr("1")));
    let fixed = fix_package(&mut package, VERSION, Fix::Trivial).unwrap();
    assert_eq!(fixed, Vec::new());
}

#[test]
fn off_changes_nothing() {
    let body = format!("{}{}", doc_pr("1"), doc_pr("1"));
    let mut docx = Docx::from_package(package(&body, None), OpenOptions::default()).unwrap();
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
    let mut package = package(&doc_pr("1"), Some(&format!("{}{}", rect("a"), rect("a"))));
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
    let mut package = package(
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
    let mut source = package(&doc_pr("1"), None);
    let main = PartName::new("/word/document.xml").unwrap();
    let custom = PartName::new("/customXml/item1.dat").unwrap();
    source
        .set_part_bytes(
            &custom,
            CUSTOM_XML_CONTENT,
            document_xml("item", &format!("{}{}", rect("a"), rect("a"))).as_bytes(),
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
