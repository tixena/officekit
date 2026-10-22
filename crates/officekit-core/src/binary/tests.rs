#[cfg(feature = "markdown")]
use base64::Engine as _;
#[cfg(feature = "markdown")]
use base64::engine::general_purpose::STANDARD;
#[cfg(feature = "markdown")]
use officekit_packaging::package::OpenOptions;
#[cfg(feature = "transitional")]
use officekit_parts::kinds::{PresentationKind, SpreadsheetKind};
#[cfg(feature = "markdown")]
use os_shim::mock::MemorySystem;
#[cfg(feature = "markdown")]
use png::{BitDepth, ColorType, Encoder};

#[cfg(feature = "transitional")]
use super::encode;
use super::{BinaryError, MAGIC, decode};
#[cfg(feature = "markdown")]
use super::{VERSION, package_of};
#[cfg(feature = "markdown")]
use crate::markdown::MarkdownOptions;
#[cfg(feature = "transitional")]
use crate::office_file::OfficeFile;
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;

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
