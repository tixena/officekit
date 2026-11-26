#[cfg(feature = "transitional")]
use officekit_markup::wordprocessing::Footer;
use officekit_markup::wordprocessing::{BlockContent, Document};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Element, NamespaceDeclaration, Node, XmlError};
use officekit_xml::error::WriteError;
use officekit_xml::mc::MarkupCompatibility;
use officekit_xml::name::{OfficeVersion, QName};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::{Layout, WriteOptions};

use super::{PartRoot, load, store};
#[cfg(feature = "transitional")]
use crate::parts::footer;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

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

/// A typed root for the tests that declares a namespace it does not use: `<w:spare>`.
struct Spare;

impl PartRoot for Spare {
    const LOCAL_NAME: &'static str = "spare";

    fn from_tree(_root: &Element) -> Result<Self, String> {
        Ok(Self)
    }

    fn to_tree(&self, _class: ConformanceClass) -> Result<Element, WriteError> {
        let mut root = Element::new(QName::prefixed("w", "spare", WORDPROCESSING));
        for (prefix, uri) in [("w", WORDPROCESSING), ("w14", W14)] {
            root.namespace_declarations
                .push(NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned()));
        }
        Ok(root)
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
fn storing_a_typed_root_keeps_the_parts_epilog() {
    let mut package = Package::new();
    let part = name("/word/note.xml");
    package
        .set_part_bytes(&part, "application/xml", b"<?xml version=\"1.0\"?><?mso-application progid=\"Word.Document\"?><w:note xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">old</w:note>\n<!-- after -->")
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
        "<?xml version=\"1.0\"?><?mso-application progid=\"Word.Document\"?><w:note xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">new</w:note>\n<!-- after -->"
    );
}

#[test]
fn storing_over_a_part_that_does_not_parse_writes_the_declaration() {
    let mut package = Package::new();
    let part = name("/word/note.xml");
    package
        .set_part_bytes(&part, "application/xml", b"<!-- kept? --><w:note>")
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
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:note xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">new</w:note>"
    );
}

/// The bytes `Spare` stores with `options`.
fn spare_stored(options: WriteOptions) -> String {
    let mut package = Package::new();
    let part = name("/word/spare.xml");
    store(
        &mut package,
        &part,
        "application/xml",
        &Spare,
        ConformanceClass::Transitional,
        options,
    )
    .unwrap();
    String::from_utf8(package.part_bytes(&part).unwrap()).unwrap()
}

#[test]
fn a_store_with_the_cleanup_on_removes_an_unused_declaration() {
    assert_eq!(
        spare_stored(WriteOptions::default()),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:spare xmlns:w=\"{WORDPROCESSING}\"/>"
        )
    );
}

#[test]
fn a_store_with_the_cleanup_off_keeps_an_unused_declaration() {
    assert_eq!(
        spare_stored(WriteOptions::new(Layout::Compact, false)),
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:spare xmlns:w=\"{WORDPROCESSING}\" xmlns:w14=\"{W14}\"/>"
        )
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

/// A main document part holding `body`, its root declaring `w`, `mc`, `w14`, `w16se`, `wps`
/// and `x`.
fn document_holding(body: &str) -> Package {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    let xml = format!(
        "<?xml version=\"1.0\"?><!--before--><w:document xmlns:w=\"{WORDPROCESSING}\" \
         xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
         xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
         xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\" \
         xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\" \
         xmlns:x=\"urn:x\" mc:Ignorable=\"w14 x\" x:root=\"1\">{body}</w:document><!--after-->"
    );
    package.set_part_bytes(&main, MAIN, xml.as_bytes()).unwrap();
    package
}

/// The modes officekit's tests read with: `Keep`, and processing for each Office version.
fn modes() -> Vec<MarkupCompatibility> {
    [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ]
    .into_iter()
    .map(|target| MarkupCompatibility::Process { target })
    .chain([MarkupCompatibility::Keep])
    .collect()
}

/// The typed root read from the part's tree, as a load read it before it read the part forward.
fn from_tree(package: &Package, options: OpenOptions) -> Result<Document, PackageError> {
    let part = name("/word/document.xml");
    let tree = package.load_tree(&part, options)?;
    Document::from_tree(&tree.root).map_err(|reason| PackageError::Read { part, reason })
}

#[test]
fn a_part_read_forward_gives_the_typed_root_its_tree_gives_in_every_mode() {
    let symbol = "<mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx w16se:font=\"A\"/>\
                  </mc:Choice><mc:Fallback><w:sym w:font=\"A\"/></mc:Fallback></mc:AlternateContent>";
    let shape = "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:r><w:t>shape</w:t></w:r></mc:Choice>\
                 <mc:Choice Requires=\"x\"><w:r><w:t>unknown</w:t></w:r></mc:Choice>\
                 <mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent>";
    let package = document_holding(&format!(
        "<w:body x:b=\"2\"><!--note--><w:p w14:paraId=\"1\" x:c=\"3\"><w:r><w:t xml:space=\"preserve\"> a &amp; b </w:t>\
         {symbol}</w:r>{shape}<x:unknown><w:r/></x:unknown><w:r><w:t>c</w:t><w:t>d</w:t></w:r></w:p>\
         <x:block x:d=\"4\"><w:p/></x:block><w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>\
         <w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:body>"
    ));
    for markup_compatibility in modes() {
        let mut options = OpenOptions::default();
        options.markup_compatibility = markup_compatibility;
        let read = load::<Document>(&package, &name("/word/document.xml"), options).unwrap();
        assert_eq!(
            read,
            from_tree(&package, options).unwrap(),
            "{markup_compatibility:?}"
        );
        let body = read.body.as_deref().unwrap();
        assert!(
            body.content
                .iter()
                .any(|block| matches!(block, BlockContent::Paragraph(_))),
            "{markup_compatibility:?}: {body:?}"
        );
    }
}

#[test]
fn unknown_elements_and_attributes_are_kept_as_the_tree_keeps_them() {
    let package = document_holding("<x:before/><w:body x:b=\"2\"/><x:after>text</x:after>");
    let read = load::<Document>(
        &package,
        &name("/word/document.xml"),
        OpenOptions::default(),
    )
    .unwrap();
    assert_eq!(read, from_tree(&package, OpenOptions::default()).unwrap());
    assert_eq!(read.unknown.children.len(), 2);
    assert_eq!(read.body.as_deref().unwrap().unknown.attributes.len(), 1);
}

#[test]
fn a_generated_root_of_another_tag_is_the_same_read_error() {
    let mut package = Package::new();
    let part = name("/word/document.xml");
    package
        .set_part_bytes(
            &part,
            MAIN,
            format!("<w:ftr xmlns:w=\"{WORDPROCESSING}\"><w:p/></w:ftr>").as_bytes(),
        )
        .unwrap();
    let error = load::<Document>(&package, &part, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part: read, reason } if read.as_str() == "/word/document.xml" && reason == "the root is <ftr>, not <document>"),
        "got {error:?}"
    );
    package
        .set_part_bytes(
            &part,
            MAIN,
            b"<w:document xmlns:w=\"urn:not-word\"><w:body/></w:document>",
        )
        .unwrap();
    let read = load::<Document>(&package, &part, OpenOptions::default()).unwrap_err();
    let tree = from_tree(&package, OpenOptions::default()).unwrap_err();
    assert_eq!(read.to_string(), tree.to_string());
}

#[test]
fn a_part_that_cannot_be_read_is_an_xml_error() {
    let mut process = OpenOptions::default();
    process.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2013,
    };
    for (body, options, expected) in [
        ("<w:body><w:p></w:body>", OpenOptions::default(), None),
        (
            "<w:body mc:MustUnderstand=\"w16se\"/>",
            process,
            Some(XmlError::MustUnderstand {
                prefix: "w16se".to_owned(),
            }),
        ),
    ] {
        let package = document_holding(body);
        let error = load::<Document>(&package, &name("/word/document.xml"), options).unwrap_err();
        assert!(
            matches!(&error, PackageError::Xml { error: found, part } if part.as_str() == "/word/document.xml" && expected.as_ref().is_none_or(|wanted| found == wanted)),
            "got {error:?}"
        );
    }
    let mut package = Package::new();
    let part = name("/word/note.xml");
    for bytes in [&b"<!--no root-->"[..], b"<note/><note/>"] {
        package
            .set_part_bytes(&part, "application/xml", bytes)
            .unwrap();
        let error = load::<Note>(&package, &part, OpenOptions::default()).unwrap_err();
        assert!(
            matches!(&error, PackageError::Xml { error: XmlError::Malformed(_), part: read } if *read == part),
            "got {error:?}"
        );
    }
}
