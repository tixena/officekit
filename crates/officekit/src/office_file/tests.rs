use super::OfficeFile;
#[cfg(feature = "transitional")]
use crate::docx::Docx;
#[cfg(feature = "transitional")]
use crate::parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use crate::pptx::Pptx;
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;
#[cfg(feature = "transitional")]
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
#[cfg(feature = "transitional")]
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::RelationshipType;

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
fn options_are_those_a_word_excel_or_powerpoint_file_was_opened_with() {
    let mut options = OpenOptions::default();
    options.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2016,
    };
    for bytes in [
        Docx::new(WordprocessingKind::Document)
            .unwrap()
            .to_bytes_transitional(),
        Xlsx::new(SpreadsheetKind::Workbook)
            .unwrap()
            .to_bytes_transitional(),
        Pptx::new(PresentationKind::Presentation)
            .unwrap()
            .to_bytes_transitional(),
    ] {
        let file = OfficeFile::from_bytes(bytes.unwrap().0, options).unwrap();
        assert_eq!(file.options(), options);
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
