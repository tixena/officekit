use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use officekit::binary::{BinaryError, VERSION, decode, encode};
use officekit::office_file::OfficeFile;
use officekit_markdown::{MarkdownOptions, OfficeFileMarkdown as _};
use officekit_xml::packaging::package::{OpenOptions, Package};
use os_shim::mock::MemorySystem;
use png::{BitDepth, ColorType, Encoder};

/// `file`'s Transitional bytes, and those of the file its binary form decodes to.
fn before_and_after(file: &OfficeFile) -> (Vec<u8>, Vec<u8>) {
    let back = decode(&encode(file).unwrap()).unwrap();
    (
        file.to_bytes_transitional().unwrap().0,
        back.to_bytes_transitional().unwrap().0,
    )
}

/// The package of `file`, when it is a Word file.
const fn package_of(file: &OfficeFile) -> Option<&Package> {
    if let OfficeFile::Word(docx) = file {
        Some(docx.package())
    } else {
        None
    }
}

/// The Word file the markdown `text` reads as.
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

/// An opaque grey PNG of two pixels by one.
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
        decode(b"arky\0\x03"),
        Err(BinaryError::NotOfficekit)
    ));
}

#[test]
fn a_word_file_from_markdown_gives_the_same_bytes_after_its_binary_form() {
    let file = word(
        "# Report\n\n1. one\n2. two\n\n| Region | Revenue |\n| --- | --- |\n| North | 1,200 |\n",
    );
    let (before, after) = before_and_after(&file);
    assert_eq!(after, before);
}

#[test]
fn a_word_file_holding_a_picture_keeps_the_picture_and_its_content_types_in_order() {
    let picture = logo();
    let file = word(&format!(
        "![Logo](data:image/png;base64,{})\n",
        STANDARD.encode(&picture)
    ));
    let back = decode(&encode(&file).unwrap()).unwrap();
    let package = package_of(&back).unwrap();
    let pictures: Vec<Vec<u8>> = package
        .part_names()
        .unwrap()
        .iter()
        .filter(|name| name.extension() == "png")
        .map(|name| package.part_bytes(name).unwrap())
        .collect();
    assert_eq!(pictures, [picture]);
    assert_eq!(
        package.content_types(),
        package_of(&file).unwrap().content_types()
    );
    assert_eq!(
        back.to_bytes_transitional().unwrap().0,
        file.to_bytes_transitional().unwrap().0
    );
}

#[test]
fn the_binary_form_of_a_word_file_holds_no_xml_of_its_main_document() {
    let bytes = encode(&word("# Report\n\nSome text.\n")).unwrap();
    assert!(!bytes.windows(11).any(|window| window == b"<w:document"));
}
