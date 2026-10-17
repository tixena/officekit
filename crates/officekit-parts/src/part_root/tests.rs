#[cfg(feature = "transitional")]
use officekit_markup::wordprocessing::Footer;
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Element, NamespaceDeclaration, Node};
use officekit_xml::error::WriteError;
use officekit_xml::name::QName;
use officekit_xml::write::WriteOptions;

use super::{PartRoot, load, store};
#[cfg(feature = "transitional")]
use crate::footer;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// A typed root for the tests: `<w:note>` holding its text.
#[derive(Debug, PartialEq, Eq)]
struct Note {
    text: String,
}

impl PartRoot for Note {
    const LOCAL_NAME: &'static str = "note";

    fn from_tree(root: &Element) -> Result<Self, String> {
        match root.children.as_slice() {
            [] => Ok(Self {
                text: String::new(),
            }),
            [Node::Text(text)] => Ok(Self { text: text.clone() }),
            _ => Err("a note holds text only".to_owned()),
        }
    }

    fn to_tree(&self, _class: ConformanceClass) -> Result<Element, WriteError> {
        let mut root = Element::new(QName::prefixed("w", "note", WORDPROCESSING));
        root.namespace_declarations.push(NamespaceDeclaration::new(
            "w".to_owned(),
            WORDPROCESSING.to_owned(),
        ));
        root.children.push(Node::Text(self.text.clone()));
        Ok(root)
    }
}

/// A typed root for the tests that Strict has no form for: `<w:pict>`.
#[derive(Debug)]
struct Picture;

impl PartRoot for Picture {
    const LOCAL_NAME: &'static str = "pict";

    fn from_tree(_root: &Element) -> Result<Self, String> {
        Ok(Self)
    }

    fn to_tree(&self, class: ConformanceClass) -> Result<Element, WriteError> {
        Err(WriteError::NotInClass { class, tag: "pict" })
    }
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// A package with a main document part related from the package.
fn document() -> Package {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    package
        .set_part_bytes(&main, MAIN, b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>")
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    package
}

#[test]
fn a_typed_root_stores_in_a_class_and_loads_back() {
    let mut package = Package::new();
    let part = name("/word/note.xml");
    let note = Note {
        text: "hello".to_owned(),
    };
    store(
        &mut package,
        &part,
        "application/xml",
        &note,
        ConformanceClass::Strict,
        WriteOptions::default(),
    )
    .unwrap();
    let text = String::from_utf8(package.part_bytes(&part).unwrap()).unwrap();
    assert_eq!(
        text,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:note xmlns:w=\"http://purl.oclc.org/ooxml/wordprocessingml/main\">hello</w:note>"
    );
    assert_eq!(
        load::<Note>(&package, &part, OpenOptions::default()).unwrap(),
        note
    );
}

#[test]
fn storing_a_typed_root_keeps_the_parts_prolog() {
    let mut package = Package::new();
    let part = name("/word/note.xml");
    package
        .set_part_bytes(&part, "application/xml", b"<?xml version=\"1.0\"?>\n<!-- kept --><w:note xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>")
        .unwrap();
    let note = Note {
        text: "new".to_owned(),
    };
    store(
        &mut package,
        &part,
        "application/xml",
        &note,
        ConformanceClass::Transitional,
        WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(package.part_bytes(&part).unwrap()).unwrap(),
        "<?xml version=\"1.0\"?>\n<!-- kept --><w:note xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">new</w:note>"
    );
}

#[test]
fn a_root_the_class_refuses_is_a_write_error_and_writes_nothing() {
    let mut package = Package::new();
    let part = name("/word/pict.xml");
    let error = store(
        &mut package,
        &part,
        "application/xml",
        &Picture,
        ConformanceClass::Strict,
        WriteOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::Write {
                error: WriteError::NotInClass {
                    class: ConformanceClass::Strict,
                    tag: "pict",
                },
                part: written,
            } if written.as_str() == "/word/pict.xml"
        ),
        "got {error:?}"
    );
    assert!(!package.contains(&part));
    assert_eq!(package.content_type(&part), None);
}

#[test]
fn loading_a_root_of_another_tag_is_a_read_error() {
    let error = load::<Note>(
        &document(),
        &name("/word/document.xml"),
        OpenOptions::default(),
    )
    .unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part, reason } if part.as_str() == "/word/document.xml" && reason == "the root is <document>, not <note>"),
        "got {error:?}"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_generated_store_of_a_new_part_registers_the_modules_content_type() {
    let mut package = Package::new();
    package
        .set_part_bytes(&name("/customXml/item1.xml"), "application/xml", b"<a/>")
        .unwrap();
    let footer = name("/word/footer1.xml");
    footer::store_transitional(
        &mut package,
        &footer,
        &Footer::default(),
        WriteOptions::default(),
    )
    .unwrap();
    assert_eq!(package.content_type(&footer), Some(footer::CONTENT_TYPE));
    assert!(
        package
            .content_type_overrides()
            .contains(&(footer, footer::CONTENT_TYPE.to_owned()))
    );
}
