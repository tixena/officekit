#[cfg(all(feature = "strict", feature = "transitional"))]
use officekit_markup::spreadsheet::Worksheet;
#[cfg(feature = "transitional")]
use officekit_markup::wordprocessing::Footer;
use officekit_markup::wordprocessing::{BlockContent, Document};
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "strict")]
use officekit_xml::element::Document as XmlDocument;
use officekit_xml::element::{Element, NamespaceDeclaration, Node, XmlError};
use officekit_xml::error::WriteError;
use officekit_xml::mc::MarkupCompatibility;
use officekit_xml::name::{OfficeVersion, QName};
use officekit_xml::packaging::error::PackageError;
#[cfg(feature = "strict")]
use officekit_xml::packaging::package::DECLARATION;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
#[cfg(feature = "strict")]
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{PartRoot, load, store};
#[cfg(feature = "transitional")]
use crate::parts::footer;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

/// Word documents, in either class, holding what the classes' writes change: spellings, values
/// only one class has, Word's shapes and VML, unknown content, and unused declarations.
#[cfg(feature = "strict")]
const DOCUMENTS: [&str; 7] = [
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" mc:Ignorable="w14 w15"><w:body><w:p w14:paraId="1"><w:pPr><w:jc w:val="left"/><w:ind w:left="720" w:right="1"/></w:pPr><w:r><w:rPr><w:b/><w:w w:val="150"/></w:rPr><w:t xml:space="preserve"> Hello &amp; &lt;you&gt; </w:t></w:r><w:hyperlink r:id="rId1"><w:r><w:t>link</w:t></w:r></w:hyperlink></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblBorders><w:left w:val="single"/><w:right w:val="single"/></w:tblBorders></w:tblPr><w:tblGrid><w:gridCol w:w="100"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="100" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" mc:Ignorable="wp14"><w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx="10" cy="10"/><wp:wrapNone/><wp:docPr id="1" name="Box"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:spPr/><wps:txbx><w:txbxContent><w:p><w:r><w:t>box</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic><wp14:sizeRelH relativeFrom="margin"><wp14:pctWidth>50000</wp14:pctWidth></wp14:sizeRelH></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict><v:rect o:spid="_x0000_s1" style="width:1pt"/></w:pict></mc:Fallback></mc:AlternateContent></w:r><w:r><w:pict><v:rect o:spid="_x0000_s2"/></w:pict></w:r></w:p><mc:AlternateContent><mc:Choice Requires="wps"><w:p/></mc:Choice></mc:AlternateContent><w:sectPr/></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:x="urn:x" x:root="1"><!-- a comment --><w:body><x:thing x:a="1"><w:p/></x:thing><w:p xmlns:y="urn:y" y:a="2"><w:r><w:t>a</w:t></w:r><w:r><w:t></w:t></w:r><w:r><w:t xml:space="preserve">   </w:t></w:r><w:fldSimple w:instr="PAGE"><w:r><w:t>1</w:t></w:r></w:fldSimple></w:p><w:p><w:pPr><w:rPr><w:w w:val="90"/></w:rPr></w:pPr></w:p></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main" xmlns:wp="http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing" xmlns:a="http://purl.oclc.org/ooxml/drawingml/main" xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" w:conformance="strict"><w:body><w:p><w:pPr><w:jc w:val="start"/></w:pPr><w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><wp:docPr id="2" name="Shape"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wp:wsp><wp:spPr/><wp:txbx><wne:txbxContent><w:p><w:r><w:t>strict</w:t></w:r></w:p></wne:txbxContent></wp:txbx><wp:bodyPr/></wp:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r><w:r><mc:AlternateContent><mc:Choice Requires="v"><w:pict/></mc:Choice></mc:AlternateContent></w:r></w:p><w:sectPr/></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:tcPr><w:hMerge w:val="restart"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:o="urn:schemas-microsoft-com:office:office"><w:body><w:p o:gfxdata="x"><w:r><w:t>a</w:t></w:r></w:p></w:body></w:document>"#,
    r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr>stray<w:jc w:val="left"/><![CDATA[kept]]></w:pPr><w:r><w:t></w:t><w:t>a</w:t></w:r></w:p></w:body></w:document>"#,
];

/// A store of a typed root `R`: [`store`], or the store through its tree.
#[cfg(feature = "strict")]
type Store<R> = fn(
    &mut Package,
    &PartName,
    &str,
    &R,
    ConformanceClass,
    WriteOptions,
) -> Result<(), PackageError>;

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
    assert_eq!(read.unknown().unwrap().children.len(), 2);
    assert_eq!(
        read.body
            .as_deref()
            .unwrap()
            .unknown()
            .unwrap()
            .attributes
            .len(),
        1
    );
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

/// Store a typed root through its generic tree: the tree in `class`, its namespaces cleaned when
/// `options` say so, written in `options`' layout inside the part's frame.
#[cfg(feature = "strict")]
fn store_tree<R>(
    package: &mut Package,
    name: &PartName,
    content_type: &str,
    root: &R,
    class: ConformanceClass,
    options: WriteOptions,
) -> Result<(), PackageError>
where
    R: PartRoot,
{
    let mut tree = root.to_tree(class).map_err(|error| PackageError::Write {
        error,
        part: name.clone(),
    })?;
    tree.set_class(class);
    if options.namespace_cleanup {
        tree.clean_namespaces();
    }
    let document = match package.part_frame(name)? {
        Some(frame) => frame.with_root(tree),
        None => XmlDocument::new(DECLARATION, tree),
    };
    package.store_tree(
        name,
        content_type,
        &document,
        WriteOptions::new(options.layout, false),
    )
}

/// What a store of `root` gives in `class` with `options`: the part's bytes, or the error.
#[cfg(feature = "strict")]
fn stored(
    root: &Document,
    class: ConformanceClass,
    options: WriteOptions,
    store_with: Store<Document>,
) -> Result<String, String> {
    let mut package = document();
    let main = name("/word/document.xml");
    store_with(&mut package, &main, MAIN, root, class, options)
        .map_err(|error| format!("{error:?}"))?;
    let bytes = package
        .part_bytes(&main)
        .map_err(|error| format!("{error:?}"))?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

#[cfg(feature = "strict")]
#[test]
fn a_store_written_through_gives_the_bytes_and_the_refusals_of_the_tree() {
    let mut refused = 0;
    for xml in DOCUMENTS {
        let root = Document::from_tree(&parse(xml.as_bytes()).unwrap().root).unwrap();
        for class in [ConformanceClass::Strict, ConformanceClass::Transitional] {
            for layout in [Layout::Compact, Layout::Pretty] {
                for cleanup in [true, false] {
                    let options = WriteOptions::new(layout, cleanup);
                    let through = stored(&root, class, options, store);
                    let tree = stored(&root, class, options, store_tree);
                    assert_eq!(
                        through, tree,
                        "{class:?}, {layout:?}, cleanup {cleanup}: {xml}"
                    );
                    refused += usize::from(through.is_err());
                }
            }
        }
    }
    assert_eq!(
        refused, 8,
        "the hMerge and the o: attribute are refused in Strict"
    );
}

#[cfg(feature = "strict")]
#[test]
fn a_store_in_the_pretty_layout_puts_each_child_of_an_element_on_a_line_of_its_own() {
    let root = Document::from_tree(&parse(DOCUMENTS[0].as_bytes()).unwrap().root).unwrap();
    let options = WriteOptions::new(Layout::Pretty, true);
    let pretty = stored(&root, ConformanceClass::Strict, options, store).unwrap();
    assert!(pretty.contains("\n  <w:body>"), "{pretty}");
    assert_eq!(
        Ok(pretty),
        stored(&root, ConformanceClass::Strict, options, store_tree)
    );
}

#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_date_cell_is_refused_in_transitional_as_the_tree_refuses_it() {
    let sheet = Worksheet::from_tree(
        &parse(b"<x:worksheet xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><x:sheetData><x:row r=\"1\"><x:c r=\"A1\"><x:v>1</x:v></x:c><x:c r=\"B1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c></x:row></x:sheetData></x:worksheet>")
            .unwrap()
            .root,
    )
    .unwrap();
    let part = name("/xl/worksheets/sheet1.xml");
    let outcome = |write: Store<Worksheet>| {
        let mut package = Package::new();
        let error = write(
            &mut package,
            &part,
            "application/xml",
            &sheet,
            ConformanceClass::Transitional,
            WriteOptions::default(),
        )
        .unwrap_err();
        (format!("{error:?}"), package.contains(&part))
    };
    let through = outcome(store);
    assert_eq!(through, outcome(store_tree));
    assert!(through.0.contains("c/@t"), "{}", through.0);
    assert!(!through.1);
}

/// A root of type `R` read from `xml`, stored in Strict written through and through its tree in
/// every layout, with and without the cleanup: the part's text, or the error's, both stores give.
#[cfg(feature = "strict")]
fn strict_outcome<R>(xml: &str) -> String
where
    R: PartRoot,
{
    let root = R::from_tree(&parse(xml.as_bytes()).unwrap().root).unwrap();
    let part = name("/part.xml");
    let mut outcomes = Vec::new();
    for layout in [Layout::Compact, Layout::Pretty] {
        for cleanup in [true, false] {
            let options = WriteOptions::new(layout, cleanup);
            let outcome = |write: Store<R>| {
                let mut package = Package::new();
                write(
                    &mut package,
                    &part,
                    "application/xml",
                    &root,
                    ConformanceClass::Strict,
                    options,
                )
                .map_err(|error| format!("{error:?}"))?;
                let bytes = package
                    .part_bytes(&part)
                    .map_err(|error| format!("{error:?}"))?;
                String::from_utf8(bytes).map_err(|error| error.to_string())
            };
            let through = outcome(store);
            assert_eq!(
                through,
                outcome(store_tree),
                "{layout:?}, cleanup {cleanup}: {xml}"
            );
            outcomes.push(through.unwrap_or_else(|error| error));
        }
    }
    outcomes.swap_remove(0)
}

#[cfg(feature = "strict")]
#[test]
fn a_root_naming_children_strict_refuses_gives_the_bytes_and_the_refusals_of_the_tree() {
    use officekit_markup::presentation::PresentationWideProperties;
    use officekit_markup::spreadsheet::{ChartSheet, DialogSheet, Worksheet};
    use officekit_markup::wordprocessing::{DocumentSettings, WebPageSettings};

    const X: &str = "xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
    const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:v=\"urn:schemas-microsoft-com:vml\"";
    const P: &str = "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
    let refused = |outcome: String| {
        assert!(outcome.contains("NotInClass"), "{outcome}");
        outcome
    };
    let rows = "<x:sheetData><x:row r=\"1\"><x:c r=\"A1\"><x:v>1</x:v></x:c></x:row></x:sheetData>";
    let sheet = strict_outcome::<Worksheet>(&format!("<x:worksheet {X}>{rows}</x:worksheet>"));
    assert!(sheet.contains("<x:v>1</x:v>"), "{sheet}");
    for refused_child in ["legacyDrawing", "legacyDrawingHF"] {
        let xml =
            format!("<x:worksheet {X}>{rows}<x:{refused_child} r:id=\"rId1\"/></x:worksheet>");
        let error = refused(strict_outcome::<Worksheet>(&xml));
        assert!(
            error.contains(&format!("tag: \"{refused_child}\"")),
            "{error}"
        );
    }
    let chartsheet = format!(
        "<x:chartsheet {X}><x:sheetViews><x:sheetView workbookViewId=\"0\"/></x:sheetViews><x:drawing r:id=\"rId1\"/><x:legacyDrawing r:id=\"rId2\"/></x:chartsheet>"
    );
    assert!(refused(strict_outcome::<ChartSheet>(&chartsheet)).contains("tag: \"legacyDrawing\""));
    let dialogsheet =
        format!("<x:dialogsheet {X}><x:legacyDrawingHF r:id=\"rId1\"/></x:dialogsheet>");
    assert!(
        refused(strict_outcome::<DialogSheet>(&dialogsheet)).contains("tag: \"legacyDrawingHF\"")
    );
    let settings = strict_outcome::<DocumentSettings>(&format!(
        "<w:settings {W}><w:zoom w:percent=\"100\"/><w:hdrShapeDefaults><o:shapedefaults v:ext=\"edit\" spidmax=\"2049\"/></w:hdrShapeDefaults><w:defaultTabStop w:val=\"720\"/><w:shapeDefaults><o:shapedefaults v:ext=\"edit\" spidmax=\"2049\"/></w:shapeDefaults></w:settings>"
    ));
    assert!(
        settings.contains("<w:defaultTabStop w:val=\"720\"/>"),
        "{settings}"
    );
    assert!(!settings.contains("shapedefaults"), "{settings}");
    let compat = format!(
        "<w:settings {W}><w:compat><w:doNotUseHTMLParagraphAutoSpacing/></w:compat></w:settings>"
    );
    assert!(
        refused(strict_outcome::<DocumentSettings>(&compat))
            .contains("tag: \"doNotUseHTMLParagraphAutoSpacing\"")
    );
    let web = strict_outcome::<WebPageSettings>(&format!(
        "<w:webSettings {W}><w:relyOnVML/><w:optimizeForBrowser/></w:webSettings>"
    ));
    assert!(
        web.contains("<w:optimizeForBrowser/>") && !web.contains("relyOnVML"),
        "{web}"
    );
    for refused_child in ["htmlPubPr", "webPr"] {
        let xml = format!("<p:presentationPr {P}><p:{refused_child}/></p:presentationPr>");
        let error = refused(strict_outcome::<PresentationWideProperties>(&xml));
        assert!(
            error.contains(&format!("tag: \"{refused_child}\"")),
            "{error}"
        );
    }
}
