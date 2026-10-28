use crate::parse::parse;
use crate::write::WriteOptions;

use super::ContentTypes;
use crate::packaging::part_name::PartName;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn parsed(xml: &str) -> Result<ContentTypes, String> {
    ContentTypes::parse(&parse(xml.as_bytes()).map_err(|error| error.to_string())?)
}

#[test]
fn a_part_takes_its_override_else_its_extensions_default() {
    let types = parsed("<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"PNG\" ContentType=\"image/png\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>").unwrap();
    assert_eq!(types.of(&name("/WORD/Document.xml")), Some(MAIN));
    assert_eq!(types.of(&name("/word/styles.xml")), Some("application/xml"));
    assert_eq!(types.of(&name("/word/media/image1.png")), Some("image/png"));
    assert_eq!(types.of(&name("/word/media/image1.gif")), None);
}

#[test]
fn an_extension_without_a_default_gets_one() {
    let mut types = ContentTypes::default();
    types.register(&name("/word/media/image1.png"), "image/png");
    types.register(&name("/word/media/image2.png"), "image/png");
    assert_eq!(
        types.defaults(),
        [("png".to_owned(), "image/png".to_owned())]
    );
    assert_eq!(types.overrides(), []);
}

#[test]
fn an_xml_part_adds_the_application_xml_default_and_an_override_when_its_type_differs() {
    let mut types = ContentTypes::default();
    types.register(&name("/word/document.xml"), MAIN);
    types.register(&name("/customXml/item1.xml"), "application/xml");
    assert_eq!(
        types.defaults(),
        [("xml".to_owned(), "application/xml".to_owned())]
    );
    assert_eq!(
        types.overrides(),
        [(name("/word/document.xml"), MAIN.to_owned())]
    );
}

#[test]
fn a_type_other_than_the_default_takes_an_override_and_registering_again_replaces_it() {
    let mut types = ContentTypes::default();
    types.register(&name("/a/one.bin"), "application/one");
    types.register(&name("/a/two.bin"), "application/two");
    types.register(&name("/a/Two.bin"), "application/three");
    assert_eq!(
        types.defaults(),
        [("bin".to_owned(), "application/one".to_owned())]
    );
    assert_eq!(
        types.overrides(),
        [(name("/a/Two.bin"), "application/three".to_owned())]
    );
    types.register(&name("/a/two.bin"), "application/one");
    assert_eq!(types.overrides(), []);
}

#[test]
fn a_part_without_an_extension_takes_an_override() {
    let mut types = ContentTypes::default();
    types.register(&name("/folder/data"), "application/octet-stream");
    assert_eq!(types.defaults(), []);
    assert_eq!(
        types.of(&name("/folder/data")),
        Some("application/octet-stream")
    );
}

#[test]
fn removing_drops_the_override_only() {
    let mut types = ContentTypes::default();
    types.register(&name("/word/document.xml"), MAIN);
    assert!(types.remove(&name("/Word/document.xml")));
    assert!(!types.remove(&name("/word/document.xml")));
    assert_eq!(
        types.of(&name("/word/document.xml")),
        Some("application/xml")
    );
}

#[test]
fn ensure_default_adds_a_missing_default_once() {
    let mut types = ContentTypes::default();
    assert!(types.ensure_default(
        "rels",
        "application/vnd.openxmlformats-package.relationships+xml"
    ));
    assert!(!types.ensure_default("RELS", "text/plain"));
    assert_eq!(types.defaults().len(), 1);
}

#[test]
fn entries_write_back_in_order() {
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
    let written = parsed(xml)
        .unwrap()
        .to_document()
        .to_xml(WriteOptions::default());
    assert_eq!(String::from_utf8(written).unwrap(), xml);
}

#[test]
fn unexpected_content_is_refused() {
    assert_eq!(
        parsed("<Other/>"),
        Err("the root is <Other>, not <Types>".to_owned())
    );
    assert_eq!(
        parsed("<Types><Default Extension=\"png\"/></Types>"),
        Err("a <Default> without ContentType".to_owned())
    );
    assert_eq!(
        parsed("<Types><Something/></Types>"),
        Err("an unexpected <Something>".to_owned())
    );
    assert_eq!(
        parsed("<Types><Override PartName=\"word/a.xml\" ContentType=\"a/b\"/></Types>"),
        Err("invalid part name 'word/a.xml': must start with '/'".to_owned())
    );
}
