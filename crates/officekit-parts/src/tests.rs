#[cfg(feature = "transitional")]
use core::fmt::Debug;

use officekit_packaging::error::PackageError;
#[cfg(feature = "transitional")]
use officekit_packaging::package::DECLARATION;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "strict")]
use officekit_xml::error::WriteError;
#[cfg(feature = "transitional")]
use officekit_xml::parse::parse;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_xml::write::WriteOptions;

#[cfg(feature = "transitional")]
use super::PartModel;
use super::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
use super::{MODULES, header, main_document, presentation, workbook};
#[cfg(any(feature = "strict", feature = "transitional"))]
use super::{bytes_of, model_of};
#[cfg(feature = "transitional")]
use crate::part_root::PartRoot;

const DOCUMENT: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><w:body><w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Hello </w:t></w:r></w:p><w:sectPr><w:headerReference r:id=\"rId1\" w:type=\"default\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:body></w:document>";
#[cfg(any(feature = "strict", feature = "transitional"))]
const HEADER: &str = "<w:hdr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:r><w:t>Top</w:t></w:r></w:p></w:hdr>";
#[cfg(feature = "transitional")]
const FOOTER: &str = "<w:ftr xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr></w:p></w:ftr>";
#[cfg(feature = "transitional")]
const WORKSHEET: &str = "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\"><v>2.5</v></c></row></sheetData></worksheet>";
#[cfg(feature = "transitional")]
const SLIDE: &str = "<p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld></p:sld>";

#[cfg(feature = "transitional")]
const ADDITIONAL_CHARACTERISTICS: &str = "<ac:additionalCharacteristics xmlns:ac=\"http://schemas.openxmlformats.org/officeDocument/2006/characteristics\"><ac:characteristic name=\"x\" relation=\"eq\" val=\"1\"/></ac:additionalCharacteristics>";
#[cfg(feature = "transitional")]
const SOURCES: &str =
    "<b:Sources xmlns:b=\"http://schemas.openxmlformats.org/officeDocument/2006/bibliography\"/>";

/// A typed root's `load`, and its `store` in one class.
#[cfg(feature = "transitional")]
type Load<R> = fn(&Package, &PartName, OpenOptions) -> Result<R, PackageError>;
#[cfg(feature = "transitional")]
type Store<R> = fn(&mut Package, &PartName, &R, WriteOptions) -> Result<(), PackageError>;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// A package holding `xml` as the part `part`.
fn package_with(part: &PartName, content_type: &str, xml: &str) -> Package {
    let mut package = Package::new();
    package
        .set_part_bytes(part, content_type, xml.as_bytes())
        .unwrap();
    package
}

/// Load the part, store it back, and check that the part holds the same
/// tree and loads into the same typed root.
#[cfg(feature = "transitional")]
fn stores_back_the_same_tree<R>(xml: &str, content_type: &str, load: Load<R>, store: Store<R>)
where
    R: PartRoot + Debug + PartialEq,
{
    let part = name("/part.xml");
    let mut package = package_with(&part, content_type, xml);
    let root = load(&package, &part, OpenOptions::default()).unwrap();
    store(&mut package, &part, &root, WriteOptions::default()).unwrap();
    let stored = package.load_tree(&part, OpenOptions::default()).unwrap();
    assert_eq!(stored.root, parse(xml.as_bytes()).unwrap().root);
    assert_eq!(package.content_type(&part), Some(content_type));
    assert_eq!(load(&package, &part, OpenOptions::default()).unwrap(), root);
}

#[cfg(feature = "transitional")]
#[test]
fn main_document_header_footer_worksheet_and_slide_store_back_the_same_tree() {
    stores_back_the_same_tree(
        DOCUMENT,
        main_document::CONTENT_TYPES[0],
        main_document::load,
        main_document::store_transitional,
    );
    stores_back_the_same_tree(
        HEADER,
        header::CONTENT_TYPE,
        header::load,
        header::store_transitional,
    );
    stores_back_the_same_tree(
        FOOTER,
        super::footer::CONTENT_TYPE,
        super::footer::load,
        super::footer::store_transitional,
    );
    stores_back_the_same_tree(
        WORKSHEET,
        super::worksheet::CONTENT_TYPE,
        super::worksheet::load,
        super::worksheet::store_transitional,
    );
    stores_back_the_same_tree(
        SLIDE,
        super::slide::CONTENT_TYPE,
        super::slide::load,
        super::slide::store_transitional,
    );
}

#[cfg(feature = "transitional")]
#[test]
fn additional_characteristics_and_bibliography_store_back_the_same_tree() {
    stores_back_the_same_tree(
        ADDITIONAL_CHARACTERISTICS,
        super::additional_characteristics::CONTENT_TYPE,
        super::additional_characteristics::load,
        super::additional_characteristics::store_transitional,
    );
    stores_back_the_same_tree(
        SOURCES,
        super::bibliography::CONTENT_TYPE,
        super::bibliography::load,
        super::bibliography::store_transitional,
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_new_part_takes_the_modules_first_content_type() {
    let source = name("/word/document.xml");
    let package = package_with(&source, main_document::CONTENT_TYPES[1], DOCUMENT);
    let root = main_document::load(&package, &source, OpenOptions::default()).unwrap();
    let mut target = Package::new();
    main_document::store_transitional(&mut target, &source, &root, WriteOptions::default())
        .unwrap();
    assert_eq!(
        target.content_type(&source),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
    );
    let mut kept = package_with(&source, main_document::CONTENT_TYPES[1], DOCUMENT);
    main_document::store_transitional(&mut kept, &source, &root, WriteOptions::default()).unwrap();
    assert_eq!(
        kept.content_type(&source),
        Some(main_document::CONTENT_TYPES[1])
    );
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_store_writes_the_strict_namespace_and_reads_the_same_content_back() {
    let part = name("/word/header1.xml");
    let mut package = package_with(&part, header::CONTENT_TYPE, HEADER);
    let root = header::load(&package, &part, OpenOptions::default()).unwrap();
    header::store_strict(&mut package, &part, &root, WriteOptions::default()).unwrap();
    let text = String::from_utf8(package.part_bytes(&part).unwrap()).unwrap();
    assert!(
        text.contains("xmlns:w=\"http://purl.oclc.org/ooxml/wordprocessingml/main\""),
        "{text}"
    );
    let reloaded = header::load(&package, &part, OpenOptions::default()).unwrap();
    assert_eq!(reloaded.content, root.content);
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_store_of_the_main_document_converts_the_content_and_marks_it_strict() {
    let part = name("/word/document.xml");
    let mut package = package_with(&part, main_document::CONTENT_TYPES[0], DOCUMENT);
    let root = main_document::load(&package, &part, OpenOptions::default()).unwrap();
    main_document::store_strict(&mut package, &part, &root, WriteOptions::default()).unwrap();
    let text = String::from_utf8(package.part_bytes(&part).unwrap()).unwrap();
    assert!(text.contains("w:val=\"start\""), "{text}");
    assert!(!text.contains("w:val=\"left\""), "{text}");
    assert!(text.contains("conformance=\"strict\""), "{text}");
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_store_of_content_strict_has_no_form_for_fails_and_stores_nothing() {
    const VML: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:pict><v:rect xmlns:v=\"urn:schemas-microsoft-com:vml\"/></w:pict></w:r></w:p></w:body></w:document>";
    let source = name("/word/document.xml");
    let loaded = package_with(&source, main_document::CONTENT_TYPES[0], VML);
    let root = main_document::load(&loaded, &source, OpenOptions::default()).unwrap();
    let mut package = Package::new();
    let error = main_document::store_strict(&mut package, &source, &root, WriteOptions::default())
        .unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::Write {
                error: WriteError::NotInClass {
                    class: ConformanceClass::Strict,
                    tag,
                },
                part,
            } if !tag.is_empty() && *part == source
        ),
        "{error:?}"
    );
    package.part_bytes(&source).unwrap_err();
}

#[cfg(feature = "transitional")]
#[test]
fn model_of_reads_a_part_through_the_module_its_content_type_names() {
    let part = name("/word/document.xml");
    let package = package_with(&part, main_document::CONTENT_TYPES[0], DOCUMENT);
    let root = main_document::load(&package, &part, OpenOptions::default()).unwrap();
    assert_eq!(
        model_of(&package, &part, OpenOptions::default()).unwrap(),
        Some(PartModel::MainDocument(root))
    );
    let item = name("/customXml/item1.xml");
    let sources = package_with(&item, "application/xml", SOURCES);
    assert!(matches!(
        model_of(&sources, &item, OpenOptions::default()).unwrap(),
        Some(PartModel::Bibliography(_))
    ));
    let unknown = package_with(&item, "application/xml", "<data/>");
    assert_eq!(
        model_of(&unknown, &item, OpenOptions::default()).unwrap(),
        None
    );
    let picture = name("/word/media/image1.png");
    let image = package_with(&picture, "image/png", "png");
    assert_eq!(
        model_of(&image, &picture, OpenOptions::default()).unwrap(),
        None
    );
}

#[cfg(feature = "transitional")]
#[test]
fn bytes_of_writes_a_typed_root_as_the_store_of_its_module_does() {
    let part = name("/word/document.xml");
    let xml = format!("<?{DECLARATION}?>\r\n{DOCUMENT}");
    let mut package = package_with(&part, main_document::CONTENT_TYPES[0], &xml);
    let model = model_of(&package, &part, OpenOptions::default())
        .unwrap()
        .unwrap();
    let bytes = bytes_of(
        &part,
        &model,
        ConformanceClass::Transitional,
        WriteOptions::default(),
    )
    .unwrap();
    let root = main_document::load(&package, &part, OpenOptions::default()).unwrap();
    main_document::store_transitional(&mut package, &part, &root, WriteOptions::default()).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        String::from_utf8(package.part_bytes(&part).unwrap()).unwrap()
    );
}

#[cfg(feature = "strict")]
#[test]
fn bytes_of_content_strict_has_no_form_for_is_a_write_error_naming_the_part() {
    const VML: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:pict><v:rect xmlns:v=\"urn:schemas-microsoft-com:vml\"/></w:pict></w:r></w:p></w:body></w:document>";
    let part = name("/word/document.xml");
    let package = package_with(&part, main_document::CONTENT_TYPES[0], VML);
    let model = model_of(&package, &part, OpenOptions::default())
        .unwrap()
        .unwrap();
    let error = bytes_of(
        &part,
        &model,
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
                    tag,
                },
                part: named,
            } if !tag.is_empty() && *named == part
        ),
        "{error:?}"
    );
}

#[test]
fn a_part_whose_root_has_another_tag_is_a_read_error() {
    let part = name("/word/document.xml");
    let package = package_with(&part, main_document::CONTENT_TYPES[0], DOCUMENT);
    let error = header::load(&package, &part, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part: read, reason } if read.as_str() == "/word/document.xml" && reason == "the root is <document>, not <hdr>"),
        "got {error:?}"
    );
}

#[test]
fn a_root_of_the_same_name_in_another_namespace_is_a_read_error() {
    let part = name("/word/header1.xml");
    let package = package_with(
        &part,
        header::CONTENT_TYPE,
        "<x:hdr xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"/>",
    );
    let error = header::load(&package, &part, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part: read, reason } if read.as_str() == "/word/header1.xml" && reason == "the root is {http://schemas.openxmlformats.org/spreadsheetml/2006/main}hdr, not {http://schemas.openxmlformats.org/wordprocessingml/2006/main}hdr"),
        "got {error:?}"
    );
}

#[test]
fn the_office_document_type_has_both_spellings() {
    let office_document = RelationshipType::OFFICE_DOCUMENT;
    assert_eq!(
        office_document.strict,
        "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"
    );
    assert_eq!(
        office_document.transitional,
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument"
    );
    assert!(office_document.matches(office_document.strict));
    assert!(office_document.matches(office_document.transitional));
    assert_eq!(
        office_document.in_class(ConformanceClass::Strict),
        office_document.strict
    );
    for module in [
        main_document::RELATIONSHIP_TYPE,
        workbook::RELATIONSHIP_TYPE,
        presentation::RELATIONSHIP_TYPE,
    ] {
        assert_eq!(module, office_document);
    }
    assert_eq!(
        header::RELATIONSHIP_TYPE.in_class(ConformanceClass::Strict),
        "http://purl.oclc.org/ooxml/officeDocument/relationships/header"
    );
    assert_eq!(
        RelationshipType::CORE_PROPERTIES.strict,
        RelationshipType::CORE_PROPERTIES.transitional
    );
}

#[test]
fn every_part_1_type_is_strict_in_strict_or_one_of_part_2() {
    let part_2 = ["15.2.7", "15.2.8", "15.2.12.1"];
    for module in MODULES {
        let Some(section) = module.section else {
            assert_eq!(
                module.relationship_type.strict, module.relationship_type.transitional,
                "{}",
                module.name
            );
            continue;
        };
        let strict = module.relationship_type.strict;
        if part_2.contains(&section) {
            assert!(
                strict.starts_with("http://schemas.openxmlformats.org/package/2006/relationships/"),
                "{}: {strict}",
                module.name
            );
            assert_eq!(
                strict, module.relationship_type.transitional,
                "{}",
                module.name
            );
        } else {
            assert!(
                strict.starts_with("http://purl.oclc.org/ooxml/"),
                "{}: {strict}",
                module.name
            );
        }
    }
}

#[test]
fn part_1_gives_78_modules_and_the_extensions_51() {
    let count = |prefix: &str| {
        MODULES
            .iter()
            .filter(|module| {
                module
                    .section
                    .is_some_and(|section| section.starts_with(prefix))
            })
            .count()
    };
    assert_eq!(count("11.3."), 13);
    assert_eq!(count("12.3."), 24);
    assert_eq!(count("13.3."), 13);
    assert_eq!(count("14.2."), 9);
    assert_eq!(count("15.2."), 19);
    assert_eq!(
        MODULES
            .iter()
            .filter(|module| module.section.is_some())
            .count(),
        78
    );
    assert_eq!(
        MODULES
            .iter()
            .filter(|module| module.section.is_none())
            .count(),
        51
    );
}

#[test]
fn each_file_type_has_its_kinds_and_its_main_part_lists_them() {
    assert_eq!(WordprocessingKind::ALL.len(), 4);
    assert_eq!(SpreadsheetKind::ALL.len(), 5);
    assert_eq!(PresentationKind::ALL.len(), 7);
    for kind in WordprocessingKind::ALL {
        assert!(main_document::CONTENT_TYPES.contains(&kind.main_content_type()));
    }
    for kind in SpreadsheetKind::ALL {
        assert!(workbook::CONTENT_TYPES.contains(&kind.main_content_type()));
    }
    for kind in PresentationKind::ALL {
        assert!(presentation::CONTENT_TYPES.contains(&kind.main_content_type()));
    }
    assert_eq!(
        SpreadsheetKind::MacroEnabledWorkbook.main_content_type(),
        "application/vnd.ms-excel.sheet.macroEnabled.main+xml"
    );
    assert_eq!(main_document::CONTENT_TYPES.len(), 4);
    assert_eq!(workbook::CONTENT_TYPES.len(), 5);
    assert_eq!(presentation::CONTENT_TYPES.len(), 7);
}
