#[cfg(feature = "markdown")]
use std::io::ErrorKind;
#[cfg(feature = "markdown")]
use std::path::Path;

#[cfg(feature = "markdown")]
use officekit_markup::wordprocessing::{BlockContent, ParagraphContent, TextRunContent};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
#[cfg(feature = "transitional")]
use officekit_packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_packaging::relationship::RelationshipType;
#[cfg(feature = "transitional")]
use officekit_parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "markdown")]
use os_shim::System as _;
#[cfg(feature = "markdown")]
use os_shim::mock::MemorySystem;
#[cfg(feature = "markdown")]
use png::{BitDepth, ColorType, Encoder};

use super::OfficeFile;
#[cfg(feature = "transitional")]
use crate::docx::Docx;
#[cfg(feature = "markdown")]
use crate::markdown::{MarkdownError, MarkdownOptions};
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;

/// The model of `bytes`, read with no folder and the default options.
#[cfg(feature = "markdown")]
fn input(system: &MemorySystem, bytes: &[u8]) -> Result<OfficeFile, MarkdownError> {
    OfficeFile::from_input(
        system,
        bytes.to_vec(),
        None,
        OpenOptions::default(),
        MarkdownOptions::default(),
    )
}

/// The Word file `file` is, if it is one.
#[cfg(feature = "markdown")]
fn word(file: &OfficeFile) -> Option<&Docx> {
    if let OfficeFile::Word(docx) = file {
        Some(docx)
    } else {
        None
    }
}

/// An opaque grey PNG of one pixel.
#[cfg(feature = "markdown")]
fn logo() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, 1, 1);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[128, 128, 128]).unwrap();
    writer.finish().unwrap();
    bytes
}

/// The bytes of a package whose `officeDocument` relationship names a main part of
/// `application/xml`.
#[cfg(feature = "transitional")]
fn plain_xml() -> Vec<u8> {
    let mut package = Package::new();
    let main = PartName::new("/data.xml").unwrap();
    package
        .set_part_bytes(&main, "application/xml", b"<data/>")
        .unwrap();
    package
        .relate(
            None,
            &main,
            RelationshipType::OFFICE_DOCUMENT.in_class(ConformanceClass::Transitional),
        )
        .unwrap();
    package.to_bytes().unwrap()
}

/// Each paragraph of the main document as its style (`-` for none), ` num` when it is numbered,
/// a colon, and its text.
#[cfg(feature = "markdown")]
fn paragraphs(docx: &Docx) -> Vec<String> {
    let content = docx.main_document().unwrap().body.unwrap().content;
    content
        .iter()
        .filter_map(|block| {
            let BlockContent::Paragraph(paragraph) = block else {
                return None;
            };
            let properties = paragraph.properties.as_deref();
            let style = properties
                .and_then(|own| own.referenced_paragraph_style.as_ref())
                .and_then(|style| style.val.clone())
                .unwrap_or_else(|| "-".to_owned());
            let numbered = properties
                .is_some_and(|own| own.numbering_definition_instance_reference_num_pr.is_some());
            let text: String = paragraph
                .content
                .iter()
                .filter_map(|item| {
                    if let ParagraphContent::TextRun(run) = item {
                        Some(&run.content)
                    } else {
                        None
                    }
                })
                .flatten()
                .filter_map(|piece| {
                    if let TextRunContent::Text(characters) = piece {
                        Some(characters.value.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            Some(format!(
                "{style}{}: {text}",
                if numbered { " num" } else { "" }
            ))
        })
        .collect()
}

#[test]
fn a_package_that_relates_no_main_part_is_a_missing_part_error() {
    let bytes = Package::new().to_bytes().unwrap();
    let error = OfficeFile::from_bytes(bytes, OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::MissingPart(part) if part.as_str() == "/_rels/.rels"),
        "{error:?}"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_new_word_excel_and_powerpoint_file_open_as_their_kind_and_save_again() {
    for kind in WordprocessingKind::ALL {
        let (bytes, _) = Docx::new(kind).unwrap().to_bytes_transitional().unwrap();
        let file = OfficeFile::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert!(matches!(&file, OfficeFile::Word(docx) if docx.kind() == kind));
        let (again, _) = file.to_bytes_transitional().unwrap();
        let reopened = OfficeFile::from_bytes(again, OpenOptions::default()).unwrap();
        assert!(matches!(reopened, OfficeFile::Word(docx) if docx.kind() == kind));
    }
    for kind in SpreadsheetKind::ALL {
        let (bytes, _) = Xlsx::new(kind).unwrap().to_bytes_transitional().unwrap();
        let file = OfficeFile::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert!(matches!(&file, OfficeFile::Excel(xlsx) if xlsx.kind() == kind));
        let (again, _) = file.to_bytes_transitional().unwrap();
        let reopened = OfficeFile::from_bytes(again, OpenOptions::default()).unwrap();
        assert!(matches!(reopened, OfficeFile::Excel(xlsx) if xlsx.kind() == kind));
    }
    for kind in PresentationKind::ALL {
        let (bytes, _) = Pptx::new(kind).unwrap().to_bytes_transitional().unwrap();
        let file = OfficeFile::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert!(matches!(&file, OfficeFile::PowerPoint(pptx) if pptx.kind() == kind));
        let (again, _) = file.to_bytes_transitional().unwrap();
        let reopened = OfficeFile::from_bytes(again, OpenOptions::default()).unwrap();
        assert!(matches!(reopened, OfficeFile::PowerPoint(pptx) if pptx.kind() == kind));
    }
}

#[cfg(feature = "transitional")]
#[test]
fn a_main_part_of_none_of_the_three_is_a_content_type_error() {
    let error = OfficeFile::from_bytes(plain_xml(), OpenOptions::default()).unwrap_err();
    assert!(
        matches!(&error, PackageError::ContentType { expected, found: Some(found), part }
            if *expected == "a WordprocessingML, SpreadsheetML or PresentationML main part"
                && found == "application/xml"
                && part.as_str() == "/data.xml"),
        "{error:?}"
    );
}

#[cfg(feature = "markdown")]
#[test]
fn markdown_text_reads_as_a_word_file_holding_its_heading_and_list() {
    let system = MemorySystem::new();
    let report = input(&system, b"# Report\n\n1. one\n2. two\n").unwrap();
    assert_eq!(
        paragraphs(word(&report).unwrap()),
        ["Heading1: Report", "- num: one", "- num: two"]
    );
    let (bytes, _) = report.to_bytes_transitional().unwrap();
    let reopened = input(&system, &bytes).unwrap();
    assert_eq!(
        paragraphs(word(&reopened).unwrap()),
        ["Heading1: Report", "- num: one", "- num: two"]
    );
}

#[cfg(feature = "markdown")]
#[test]
fn a_zip_reads_as_its_kind() {
    let system = MemorySystem::new();
    let (workbook, _) = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    assert!(matches!(
        input(&system, &workbook).unwrap(),
        OfficeFile::Excel(xlsx) if xlsx.kind() == SpreadsheetKind::Workbook
    ));
    let (slides, _) = Pptx::new(PresentationKind::Presentation)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    assert!(matches!(
        input(&system, &slides).unwrap(),
        OfficeFile::PowerPoint(pptx) if pptx.kind() == PresentationKind::Presentation
    ));
    let (document, _) = Docx::new(WordprocessingKind::Template)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    assert!(matches!(
        input(&system, &document).unwrap(),
        OfficeFile::Word(docx) if docx.kind() == WordprocessingKind::Template
    ));
}

#[cfg(feature = "markdown")]
#[test]
fn a_zip_that_is_no_office_file_is_a_package_error() {
    let error = input(&MemorySystem::new(), &plain_xml()).unwrap_err();
    assert!(
        matches!(
            &error,
            MarkdownError::Package(PackageError::ContentType { expected, found: Some(found), part })
                if *expected == "a WordprocessingML, SpreadsheetML or PresentationML main part"
                    && found == "application/xml"
                    && part.as_str() == "/data.xml"
        ),
        "{error:?}"
    );
}

#[cfg(feature = "markdown")]
#[test]
fn a_relative_picture_with_no_folder_fails_naming_it() {
    let error = input(&MemorySystem::new(), b"![Logo](logo.png)\n").unwrap_err();
    assert!(
        matches!(&error, MarkdownError::Image { target, error: cause }
            if target == "logo.png" && !cause.is_empty()),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "logo.png: a relative path, and markdown from standard input has no folder to find it from"
    );
}

#[cfg(feature = "markdown")]
#[test]
fn a_relative_picture_is_found_from_the_folder() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/notes")).unwrap();
    system.write(Path::new("/notes/logo.png"), &logo()).unwrap();
    let file = OfficeFile::from_input(
        &system,
        b"![Logo](logo.png)\n".to_vec(),
        Some(Path::new("/notes")),
        OpenOptions::default(),
        MarkdownOptions::default(),
    )
    .unwrap();
    let image = PartName::new("/word/media/image1.png").unwrap();
    assert_eq!(
        word(&file).unwrap().package().part_bytes(&image).unwrap(),
        logo()
    );
}

#[cfg(feature = "markdown")]
#[test]
fn bytes_neither_a_zip_nor_utf_8_are_an_io_error_naming_standard_input() {
    let error = input(&MemorySystem::new(), &[0xFF, 0xFE, 0x00]).unwrap_err();
    assert!(
        matches!(
            &error,
            MarkdownError::Io { error: cause, path }
                if cause.kind() == ErrorKind::InvalidData && path == Path::new("standard input")
        ),
        "{error:?}"
    );
}
