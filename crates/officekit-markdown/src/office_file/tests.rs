use std::io::ErrorKind;
use std::path::Path;

use officekit::docx::Docx;
use officekit::office_file::OfficeFile;
use officekit::parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
use officekit::pptx::Pptx;
use officekit::xlsx::Xlsx;
use officekit_markup::wordprocessing::{BlockContent, ParagraphContent, TextRunContent};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use os_shim::System as _;
use os_shim::WriteMode;
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Encoder};

use super::OfficeFileMarkdown as _;
use crate::{MarkdownError, MarkdownOptions};

/// The bytes of a package whose `officeDocument` relationship names a main part of
/// `application/xml`.
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

/// The model of `bytes`, read with no folder and the default options.
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
fn word(file: &OfficeFile) -> Option<&Docx> {
    if let OfficeFile::Word(docx) = file {
        Some(docx)
    } else {
        None
    }
}

/// An opaque grey PNG of one pixel.
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

/// Each paragraph of the main document as its style (`-` for none), ` num` when it is numbered,
/// a colon, and its text.
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

#[test]
fn a_relative_picture_is_found_from_the_folder() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/notes")).unwrap();
    system
        .write(Path::new("/notes/logo.png"), &logo(), WriteMode::Replace)
        .unwrap();
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
