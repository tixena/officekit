#[cfg(feature = "transitional")]
use super::encode;
use super::{BinaryError, MAGIC, decode};
#[cfg(feature = "transitional")]
use crate::docx::Docx;
#[cfg(feature = "transitional")]
use crate::office_file::OfficeFile;
#[cfg(feature = "transitional")]
use crate::parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::package::OpenOptions;

/// A Transitional main document whose run holds a block Office 2016 understands, with a fallback.
#[cfg(feature = "transitional")]
const ALTERNATE: &str = "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\"><w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx/></mc:Choice><mc:Fallback><w:t>x</w:t></mc:Fallback></mc:AlternateContent></w:r></w:p></w:body></w:document>";

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

#[test]
fn bytes_that_do_not_open_with_officekit_and_a_zero_byte_are_not_officekits() {
    for bytes in [&b"arky\0\x03 a document"[..], b"officekit", MAGIC] {
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
