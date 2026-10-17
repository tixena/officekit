use core::slice;
#[cfg(feature = "transitional")]
use std::path::Path;

#[cfg(feature = "strict")]
use officekit_markup::wordprocessing;
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_parts::kinds::SpreadsheetKind;
use officekit_parts::kinds::WordprocessingKind;
#[cfg(feature = "transitional")]
use officekit_parts::{document_settings, font_table};
use officekit_xml::conformance::ConformanceClass;
#[cfg(all(feature = "strict", feature = "transitional"))]
use officekit_xml::error::WriteError;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
#[cfg(feature = "strict")]
use officekit_xml::parse::parse;
#[cfg(feature = "strict")]
use officekit_xml::write::{Layout, WriteOptions};
#[cfg(all(feature = "strict", feature = "transitional"))]
use os_shim::System as _;
#[cfg(feature = "transitional")]
use os_shim::mock::MemorySystem;

use super::Docx;
use crate::compact::Comments;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
#[cfg(feature = "strict")]
const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

/// A Transitional main document with a left-justified paragraph.
#[cfg(feature = "strict")]
const JUSTIFIED: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr></w:p></w:body></w:document>";

/// A Transitional main document whose run holds a block only Office 2015 understands, with a
/// fallback.
#[cfg(feature = "transitional")]
const ALTERNATE: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\"><w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx/></mc:Choice><mc:Fallback><w:t>x</w:t></mc:Fallback></mc:AlternateContent></w:r></w:p></w:body></w:document>";

/// A bibliography in a custom XML part, whose namespace Strict spells otherwise.
#[cfg(feature = "strict")]
const SOURCES: &str = "<b:Sources xmlns:b=\"http://schemas.openxmlformats.org/officeDocument/2006/bibliography\" SelectedStyle=\"\"/>";

/// A custom XML part no module knows.
#[cfg(any(feature = "strict", feature = "transitional"))]
const FOREIGN: &str = "<DocumentGeneration xmlns=\"http://example.org/generation\"><Field key=\"a\"/></DocumentGeneration>";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn text(package: &Package, part: &PartName) -> String {
    String::from_utf8(package.part_bytes(part).unwrap()).unwrap()
}

/// A package whose `officeDocument` relationship, in `class`'s spelling, names a main part of
/// `content_type`.
fn package_of(class: ConformanceClass, content_type: &str) -> Package {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    package
        .set_part_bytes(&main, content_type, b"<w:document/>")
        .unwrap();
    package
        .relate(
            None,
            &main,
            RelationshipType::OFFICE_DOCUMENT.in_class(class),
        )
        .unwrap();
    package
}

/// A Word template as the demo templates are made: a main document, a font table whose font
/// names a character set by a code Strict has no name for, settings whose shape defaults are VML,
/// a custom XML part no module knows, and an image.
#[cfg(feature = "transitional")]
fn template() -> Package {
    let mut package = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .package()
        .duplicate()
        .unwrap();
    let main = name("/word/document.xml");
    let parts = [
        (
            "/word/fontTable.xml",
            font_table::CONTENT_TYPE,
            RelationshipType::FONT_TABLE,
            format!(
                "<w:fonts xmlns:w=\"{W}\"><w:font w:name=\"Symbol\"><w:charset w:val=\"01\"/></w:font></w:fonts>"
            ),
        ),
        (
            "/word/settings.xml",
            document_settings::CONTENT_TYPE,
            RelationshipType::SETTINGS,
            format!(
                "<w:settings xmlns:w=\"{W}\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:v=\"urn:schemas-microsoft-com:vml\"><w:shapeDefaults><o:shapedefaults v:ext=\"edit\" spidmax=\"1026\"/></w:shapeDefaults></w:settings>"
            ),
        ),
        (
            "/word/media/image1.png",
            "image/png",
            RelationshipType::IMAGE,
            "\u{89}PNG not really".to_owned(),
        ),
    ];
    for (path, content_type, relationship_type, content) in parts {
        let part = name(path);
        package
            .set_part_bytes(&part, content_type, content.as_bytes())
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type.transitional)
            .unwrap();
    }
    package
        .add_custom_xml(
            &main,
            FOREIGN.as_bytes(),
            "{6C9D1F4A-0000-4000-8000-000000000001}",
            &[],
        )
        .unwrap();
    package
}

#[test]
fn a_new_document_holds_one_empty_paragraph_in_its_main_part() {
    let docx = Docx::new(WordprocessingKind::Template).unwrap();
    let main = name("/word/document.xml");
    assert_eq!(docx.package().part_names().unwrap(), slice::from_ref(&main));
    assert_eq!(docx.main_document_part(), &main);
    assert_eq!(docx.kind(), WordprocessingKind::Template);
    assert_eq!(docx.conformance_class(), ConformanceClass::Transitional);
    assert_eq!(
        docx.package().content_type(&main),
        Some(WordprocessingKind::Template.main_content_type())
    );
    assert!(text(docx.package(), &main).ends_with(&format!(
        "<w:document xmlns:w=\"{W}\"><w:body><w:p/></w:body></w:document>"
    )));
    let related = docx.package().related(None).unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].id, "rId1");
    assert_eq!(related[0].target, Target::Part(main));
    assert_eq!(
        related[0].type_uri,
        RelationshipType::OFFICE_DOCUMENT.transitional
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_main_part_of_another_file_type_is_a_content_type_error() {
    let package = package_of(
        ConformanceClass::Transitional,
        SpreadsheetKind::Workbook.main_content_type(),
    );
    let error = Docx::from_package(package, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::ContentType { expected, found: Some(found), part }
            if *expected == "a WordprocessingML main part"
                && found == SpreadsheetKind::Workbook.main_content_type()
                && part.as_str() == "/word/document.xml"),
        "{error:?}"
    );
}

#[test]
fn a_package_without_a_main_part_is_a_missing_part_error() {
    let error = Docx::from_package(Package::new(), OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::MissingPart(part) if part.as_str() == "/_rels/.rels"),
        "{error:?}"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_four_kinds_reopen_with_their_kind() {
    for kind in WordprocessingKind::ALL {
        let bytes = Docx::new(kind).unwrap().package().to_bytes().unwrap();
        let reopened = Docx::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert_eq!(reopened.kind(), kind);
        assert_eq!(reopened.conformance_class(), ConformanceClass::Transitional);
    }
}

#[cfg(feature = "transitional")]
#[test]
fn create_writes_the_new_document_through_the_system() {
    let system = MemorySystem::new().with_dir("/out").unwrap();
    let path = Path::new("/out/new.dotm");
    let created = Docx::create(&system, path, WordprocessingKind::MacroEnabledTemplate).unwrap();
    let opened = Docx::open(&system, path, OpenOptions::default()).unwrap();
    assert_eq!(opened.kind(), WordprocessingKind::MacroEnabledTemplate);
    assert_eq!(
        opened.package().part_names().unwrap(),
        created.package().part_names().unwrap()
    );
}

#[cfg(not(feature = "transitional"))]
#[test]
fn a_transitional_file_is_refused_when_its_feature_is_off() {
    let bytes = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .package()
        .to_bytes()
        .unwrap();
    let error = Docx::from_bytes(bytes, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(
            error,
            PackageError::ClassNotEnabled(ConformanceClass::Transitional)
        ),
        "{error:?}"
    );
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_file_opens_strict_when_its_feature_is_on() {
    let package = package_of(
        ConformanceClass::Strict,
        WordprocessingKind::Document.main_content_type(),
    );
    let docx = Docx::from_package(package, OpenOptions::default()).unwrap();
    assert_eq!(docx.conformance_class(), ConformanceClass::Strict);
    assert_eq!(docx.kind(), WordprocessingKind::Document);
}

#[cfg(not(feature = "strict"))]
#[test]
fn a_strict_file_is_refused_when_its_feature_is_off() {
    let package = package_of(
        ConformanceClass::Strict,
        WordprocessingKind::Document.main_content_type(),
    );
    let error = Docx::from_package(package, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(
            error,
            PackageError::ClassNotEnabled(ConformanceClass::Strict)
        ),
        "{error:?}"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn open_options_apply_to_the_parts_the_file_loads() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    docx.package_mut()
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            ALTERNATE.as_bytes(),
        )
        .unwrap();
    let bytes = docx.package().to_bytes().unwrap();
    let kept = Docx::from_bytes(bytes.clone(), OpenOptions::default())
        .unwrap()
        .main_document()
        .unwrap();
    let mut options = OpenOptions::default();
    options.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2010,
    };
    let processed = Docx::from_bytes(bytes, options)
        .unwrap()
        .main_document()
        .unwrap();
    assert_ne!(kept, processed);
    let mut written = Docx::new(WordprocessingKind::Document).unwrap();
    written.set_main_document(&processed).unwrap();
    let stored = text(written.package(), &main);
    assert!(stored.contains("<w:r><w:t>x</w:t></w:r>"), "{stored}");
    assert!(!stored.contains("AlternateContent"), "{stored}");
}

#[cfg(feature = "transitional")]
#[test]
fn a_save_in_the_class_read_writes_each_part_as_it_is() {
    let system = MemorySystem::new()
        .with_file("/in/template.docx", &template().to_bytes().unwrap())
        .unwrap()
        .with_dir("/out")
        .unwrap();
    let docx = Docx::open(
        &system,
        Path::new("/in/template.docx"),
        OpenOptions::default(),
    )
    .unwrap();
    let warnings = docx
        .save_transitional(&system, Path::new("/out/saved.docx"))
        .unwrap();
    assert_eq!(warnings, []);
    let before = Package::open(&system, Path::new("/in/template.docx")).unwrap();
    let after = Package::open(&system, Path::new("/out/saved.docx")).unwrap();
    let names = before.part_names().unwrap();
    assert_eq!(after.part_names().unwrap(), names);
    for part in &names {
        assert_eq!(
            after.part_bytes(part).unwrap(),
            before.part_bytes(part).unwrap(),
            "{part}"
        );
    }
    assert_eq!(after.relationship_sets(), before.relationship_sets());
}

#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_strict_save_of_content_strict_has_no_form_for_fails_and_writes_nothing() {
    let system = MemorySystem::new()
        .with_file("/in/template.docx", &template().to_bytes().unwrap())
        .unwrap()
        .with_dir("/out")
        .unwrap();
    let docx = Docx::open(
        &system,
        Path::new("/in/template.docx"),
        OpenOptions::default(),
    )
    .unwrap();
    let path = Path::new("/out/strict.docx");
    let error = docx.save_strict(&system, path).unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::Write {
                error: WriteError::NotInClass {
                    class: ConformanceClass::Strict,
                    tag: "charset/@val",
                },
                part,
            } if part.as_str() == "/word/fontTable.xml"
        ),
        "{error:?}"
    );
    assert!(!system.exists(path).unwrap());
    assert_eq!(docx.conformance_class(), ConformanceClass::Transitional);
}

#[cfg(feature = "strict")]
#[test]
fn a_new_document_saved_strict_takes_strict_names_values_and_relationship_types() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let document = wordprocessing::read::document(&parse(JUSTIFIED.as_bytes()).unwrap().root);
    docx.set_main_document(&document).unwrap();
    let strict = Docx::from_bytes(docx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    assert_eq!(strict.conformance_class(), ConformanceClass::Strict);
    let main = text(strict.package(), strict.main_document_part());
    assert!(main.contains("w:conformance=\"strict\""), "{main}");
    assert!(main.contains(&format!("xmlns:w=\"{STRICT_W}\"")), "{main}");
    assert!(main.contains("<w:jc w:val=\"start\"/>"), "{main}");
    assert!(!main.contains(W), "{main}");
    let related = strict.package().related(None).unwrap();
    assert_eq!(
        related[0].type_uri,
        RelationshipType::OFFICE_DOCUMENT.strict
    );
    assert_eq!(docx.conformance_class(), ConformanceClass::Transitional);
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_save_rewrites_known_custom_xml_and_carries_other_parts_as_they_are() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    let sources = docx
        .package_mut()
        .add_custom_xml(&main, SOURCES.as_bytes(), "{1}", &[])
        .unwrap();
    let foreign = docx
        .package_mut()
        .add_custom_xml(&main, FOREIGN.as_bytes(), "{2}", &[])
        .unwrap();
    let (image, _) = docx
        .package_mut()
        .add_part(
            Some(&main),
            RelationshipType::IMAGE.transitional,
            "image/png",
            "/word/media/image{}.png",
            b"\x89PNG not really",
        )
        .unwrap();
    let strict = Package::from_bytes(docx.to_bytes_strict().unwrap()).unwrap();
    let bibliography = text(&strict, &sources);
    assert!(
        bibliography.contains("xmlns:b=\"http://purl.oclc.org/ooxml/officeDocument/bibliography\""),
        "{bibliography}"
    );
    let properties = text(&strict, &name("/customXml/itemProps1.xml"));
    assert!(
        properties.contains("http://purl.oclc.org/ooxml/officeDocument/customXml"),
        "{properties}"
    );
    assert_eq!(
        strict.part_bytes(&foreign).unwrap(),
        docx.package().part_bytes(&foreign).unwrap()
    );
    assert_eq!(
        strict.part_bytes(&image).unwrap(),
        docx.package().part_bytes(&image).unwrap()
    );
    for related in strict.related(Some(&main)).unwrap() {
        let known = RelationshipType::find(&related.type_uri).unwrap();
        assert_eq!(related.type_uri, known.strict);
    }
}

#[cfg(feature = "strict")]
#[test]
fn the_write_options_apply_to_every_store_the_save_included() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_write_options(WriteOptions::new(Layout::Pretty, true));
    let strict = Package::from_bytes(docx.to_bytes_strict().unwrap()).unwrap();
    let main = text(&strict, &name("/word/document.xml"));
    assert!(main.contains("\n  <w:body>"), "{main}");
}

#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_strict_file_saved_transitional_takes_transitional_names_back() {
    let strict_bytes = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .to_bytes_strict()
        .unwrap();
    let strict = Docx::from_bytes(strict_bytes, OpenOptions::default()).unwrap();
    let (bytes, warnings) = strict.to_bytes_transitional().unwrap();
    assert_eq!(warnings, []);
    let transitional = Docx::from_bytes(bytes, OpenOptions::default()).unwrap();
    assert_eq!(
        transitional.conformance_class(),
        ConformanceClass::Transitional
    );
    let main = text(transitional.package(), transitional.main_document_part());
    assert!(main.contains(&format!("xmlns:w=\"{W}\"")), "{main}");
    assert!(!main.contains("conformance"), "{main}");
}

#[test]
fn compact_runs_merges_neighbouring_runs_of_the_main_part_and_keeps_the_text() {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = name("/word/document.xml");
    let xml = format!(
        "<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>The </w:t></w:r><w:r><w:t>fox</w:t></w:r></w:p></w:body></w:document>"
    );
    docx.package_mut()
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            xml.as_bytes(),
        )
        .unwrap();
    docx.compact_runs(Comments::Keep).unwrap();
    let stored = text(docx.package(), &main);
    assert_eq!(stored.matches("<w:r>").count(), 1, "{stored}");
    assert!(stored.contains("<w:t>The </w:t><w:t>fox</w:t>"), "{stored}");
}
