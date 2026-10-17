use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;
use officekit_parts::kinds::SpreadsheetKind;
use officekit_parts::worksheet::CONTENT_TYPE as WORKSHEET;

#[cfg(feature = "transitional")]
use super::Embedded;
use super::open;
#[cfg(all(feature = "strict", feature = "transitional"))]
use super::store_strict;
#[cfg(feature = "transitional")]
use super::store_transitional;
use crate::xlsx::Xlsx;

const WORKBOOK_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const SHEET: &str = "/xl/worksheets/sheet1.xml";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

#[cfg(feature = "transitional")]
fn workbook_of(embedded: Embedded) -> Xlsx {
    match embedded {
        Embedded::Xlsx(workbook) => Some(workbook),
        Embedded::Docx(_) | Embedded::Pptx(_) => None,
    }
    .unwrap()
}

fn sheet_with(b2: u32, c2: u32) -> Vec<u8> {
    format!(
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="2"><c r="B2"><v>{b2}</v></c><c r="C2"><v>{c2}</v></c></row></sheetData></worksheet>"#
    )
    .into_bytes()
}

/// A document holding a workbook at `/word/embeddings/Microsoft_Excel_Worksheet.xlsx`.
fn document_with_workbook() -> (Package, PartName) {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    package
        .set_part_bytes(&main, MAIN, b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>")
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    let mut workbook = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    workbook
        .package_mut()
        .set_part_bytes(&name(SHEET), WORKSHEET, &sheet_with(1, 2))
        .unwrap();
    let embedded = name("/word/embeddings/Microsoft_Excel_Worksheet.xlsx");
    package
        .set_part_bytes(
            &embedded,
            WORKBOOK_TYPE,
            &workbook.package().to_bytes().unwrap(),
        )
        .unwrap();
    (package, embedded)
}

#[cfg(feature = "transitional")]
#[test]
fn the_workbook_of_a_document_opens_as_xlsx() {
    let (package, part) = document_with_workbook();
    let workbook = workbook_of(open(&package, &part, OpenOptions::default()).unwrap());
    assert_eq!(
        workbook.package().part_bytes(&name(SHEET)).unwrap(),
        sheet_with(1, 2)
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_workbook_edit_writes_back_with_its_edit() {
    let (mut package, part) = document_with_workbook();
    let mut workbook = workbook_of(open(&package, &part, OpenOptions::default()).unwrap());
    workbook
        .package_mut()
        .set_part_bytes(&name(SHEET), WORKSHEET, &sheet_with(42, 17))
        .unwrap();
    let warnings = store_transitional(&mut package, &part, &Embedded::Xlsx(workbook)).unwrap();
    assert_eq!(warnings, []);
    assert_eq!(package.content_type(&part), Some(WORKBOOK_TYPE));
    let reopened = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    let again = workbook_of(open(&reopened, &part, OpenOptions::default()).unwrap());
    assert_eq!(
        again.package().part_bytes(&name(SHEET)).unwrap(),
        sheet_with(42, 17)
    );
}

#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_workbook_written_back_strict_reopens_as_xlsx() {
    let (mut package, part) = document_with_workbook();
    let embedded = open(&package, &part, OpenOptions::default()).unwrap();
    store_strict(&mut package, &part, &embedded).unwrap();
    assert_eq!(package.content_type(&part), Some(WORKBOOK_TYPE));
    assert!(matches!(
        open(&package, &part, OpenOptions::default()).unwrap(),
        Embedded::Xlsx(_)
    ));
}

#[test]
fn bytes_that_are_not_a_package_are_not_a_package() {
    let (mut package, _) = document_with_workbook();
    let ole = name("/word/embeddings/oleObject1.bin");
    package
        .set_part_bytes(
            &ole,
            "application/vnd.openxmlformats-officedocument.oleObject",
            b"\xD0\xCF\x11\xE0 not a zip",
        )
        .unwrap();
    let error = open(&package, &ole, OpenOptions::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("the part /word/embeddings/oleObject1.bin does not hold a package")
    );
}

#[test]
fn a_zip_without_a_main_part_is_not_a_package() {
    let (mut package, _) = document_with_workbook();
    let empty = name("/word/embeddings/empty.xlsx");
    package
        .set_part_bytes(&empty, WORKBOOK_TYPE, &Package::new().to_bytes().unwrap())
        .unwrap();
    let error = open(&package, &empty, OpenOptions::default()).unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("the part /word/embeddings/empty.xlsx does not hold a package")
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_package_of_another_main_part_is_not_a_package() {
    let (mut package, _) = document_with_workbook();
    let other = name("/word/embeddings/other.xlsx");
    let mut inner = Package::new();
    let main = name("/word/other.xml");
    inner
        .set_part_bytes(&main, "application/xml", b"<a/>")
        .unwrap();
    inner
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    package
        .set_part_bytes(&other, WORKBOOK_TYPE, &inner.to_bytes().unwrap())
        .unwrap();
    let error = open(&package, &other, OpenOptions::default()).unwrap_err();
    assert_eq!(
        error.to_string(),
        "the part /word/embeddings/other.xlsx does not hold a package: the main part's content type is Some(\"application/xml\")"
    );
}
