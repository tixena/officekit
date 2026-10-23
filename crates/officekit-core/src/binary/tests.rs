#[cfg(feature = "markdown")]
use base64::Engine as _;
#[cfg(feature = "markdown")]
use base64::engine::general_purpose::STANDARD;
#[cfg(feature = "transitional")]
use officekit_packaging::package::OpenOptions;
#[cfg(feature = "transitional")]
use officekit_parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
#[cfg(feature = "markdown")]
use os_shim::mock::MemorySystem;
#[cfg(feature = "markdown")]
use png::{BitDepth, ColorType, Encoder};

#[cfg(feature = "transitional")]
use super::encode;
use super::{BinaryError, MAGIC, decode};
#[cfg(feature = "markdown")]
use super::{VERSION, package_of};
#[cfg(feature = "transitional")]
use crate::docx::Docx;
#[cfg(feature = "markdown")]
use crate::markdown::MarkdownOptions;
#[cfg(feature = "transitional")]
use crate::office_file::OfficeFile;
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;

/// A Transitional main document whose run holds a block Office 2016 understands, with a fallback.
#[cfg(feature = "transitional")]
const ALTERNATE: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\"><w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx/></mc:Choice><mc:Fallback><w:t>x</w:t></mc:Fallback></mc:AlternateContent></w:r></w:p></w:body></w:document>";

/// The Word file the markdown `text` reads as.
#[cfg(feature = "markdown")]
fn word(text: &str) -> OfficeFile {
    OfficeFile::from_input(
        &MemorySystem::new(),
        text.as_bytes().to_vec(),
        None,
        OpenOptions::default(),
        MarkdownOptions::default(),
    )
    .unwrap()
}

/// A Word file whose main document is [`ALTERNATE`], opened processing markup compatibility for
/// Office 2016.
#[cfg(feature = "transitional")]
fn processed_for_office_2016() -> OfficeFile {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    docx.package_mut()
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            ALTERNATE.as_bytes(),
        )
        .unwrap();
    let mut options = OpenOptions::default();
    options.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2016,
    };
    OfficeFile::from_bytes(docx.package().to_bytes().unwrap(), options).unwrap()
}

/// The Word file `file` is, if it is one.
#[cfg(feature = "transitional")]
fn word_of(file: &OfficeFile) -> Option<&Docx> {
    if let OfficeFile::Word(docx) = file {
        Some(docx)
    } else {
        None
    }
}

/// `file`'s Transitional bytes, and those of the file its binary form decodes to.
#[cfg(feature = "transitional")]
fn before_and_after(file: &OfficeFile) -> (Vec<u8>, Vec<u8>) {
    let back = decode(&encode(file).unwrap()).unwrap();
    (
        file.to_bytes_transitional().unwrap().0,
        back.to_bytes_transitional().unwrap().0,
    )
}

/// An opaque grey PNG of two pixels by one.
#[cfg(feature = "markdown")]
fn logo() -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = Encoder::new(&mut bytes, 2, 1);
    encoder.set_color(ColorType::Rgb);
    encoder.set_depth(BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[128; 6]).unwrap();
    writer.finish().unwrap();
    bytes
}

#[cfg(feature = "markdown")]
#[test]
fn the_end_result_in_use() {
    let word = word("# Report\n\n1. one\n");
    let bytes = encode(&word).unwrap();
    assert_eq!(&bytes[..10], b"officekit\0");
    assert_eq!(bytes[10], VERSION);
    let back = decode(&bytes).unwrap();
    assert_eq!(
        back.to_bytes_transitional().unwrap().0,
        word.to_bytes_transitional().unwrap().0
    );
    assert!(matches!(
        decode(b"archie\0\x03"),
        Err(BinaryError::NotOfficekit)
    ));
}

#[cfg(feature = "markdown")]
#[test]
fn a_word_file_from_markdown_gives_the_same_bytes_after_its_binary_form() {
    let file = word(
        "# Report\n\n1. one\n2. two\n\n| Region | Revenue |\n| --- | --- |\n| North | 1,200 |\n",
    );
    let (before, after) = before_and_after(&file);
    assert_eq!(after, before);
}

#[cfg(feature = "transitional")]
#[test]
fn an_excel_and_a_powerpoint_file_give_the_same_bytes_after_their_binary_form() {
    for file in [
        OfficeFile::Excel(Xlsx::new(SpreadsheetKind::Workbook).unwrap()),
        OfficeFile::PowerPoint(Pptx::new(PresentationKind::Presentation).unwrap()),
    ] {
        let (before, after) = before_and_after(&file);
        assert_eq!(after, before);
    }
}

#[cfg(feature = "transitional")]
#[test]
fn a_file_opened_with_markup_compatibility_processed_is_opened_so_again_after_its_binary_form() {
    let word = processed_for_office_2016();
    let back = decode(&encode(&word).unwrap()).unwrap();
    assert_eq!(back.options(), word.options());
    let before = word_of(&word).unwrap().main_document().unwrap();
    let after = word_of(&back).unwrap().main_document().unwrap();
    assert_eq!(after, before);
}

#[cfg(feature = "transitional")]
#[test]
fn the_binary_form_keeps_the_alternate_content_the_options_process() {
    let bytes = encode(&processed_for_office_2016()).unwrap();
    assert!(
        bytes
            .windows(16)
            .any(|window| window == b"AlternateContent")
    );
}

#[cfg(feature = "markdown")]
#[test]
fn a_word_file_holding_a_picture_keeps_the_picture_and_its_content_types_in_order() {
    let picture = logo();
    let file = word(&format!(
        "![Logo](data:image/png;base64,{})\n",
        STANDARD.encode(&picture)
    ));
    let back = decode(&encode(&file).unwrap()).unwrap();
    let package = package_of(&back);
    let pictures: Vec<Vec<u8>> = package
        .part_names()
        .unwrap()
        .iter()
        .filter(|name| name.extension() == "png")
        .map(|name| package.part_bytes(name).unwrap())
        .collect();
    assert_eq!(pictures, [picture]);
    assert_eq!(package.content_types(), package_of(&file).content_types());
    assert_eq!(
        back.to_bytes_transitional().unwrap().0,
        file.to_bytes_transitional().unwrap().0
    );
}

#[cfg(feature = "markdown")]
#[test]
fn the_binary_form_of_a_word_file_holds_no_xml_of_its_main_document() {
    let bytes = encode(&word("# Report\n\nSome text.\n")).unwrap();
    assert!(!bytes.windows(11).any(|window| window == b"<w:document"));
}

#[test]
fn bytes_that_do_not_open_with_officekit_and_a_zero_byte_are_not_officekits() {
    for bytes in [&b"archie\0\x03 a document"[..], b"officekit", MAGIC] {
        assert!(
            matches!(decode(bytes), Err(BinaryError::NotOfficekit)),
            "{bytes:?}"
        );
    }
}

#[test]
fn another_version_byte_is_a_version_error() {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&[99, 0, 0, 0, 0]);
    assert!(matches!(
        decode(&bytes),
        Err(BinaryError::Version { found: 99 })
    ));
}

#[test]
fn the_binary_form_of_version_1_is_a_version_error() {
    let mut bytes = MAGIC.to_vec();
    bytes.extend_from_slice(&[1, 0, 0, 0, 0]);
    assert!(matches!(
        decode(&bytes),
        Err(BinaryError::Version { found: 1 })
    ));
}
