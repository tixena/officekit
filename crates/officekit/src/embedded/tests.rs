use crate::docx::Docx;
use crate::parts::kinds::{SpreadsheetKind, WordprocessingKind};
use crate::parts::part_root::store;
use crate::parts::worksheet;
use officekit_markup::spreadsheet::{Cell, CellValue, Row, Worksheet};
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::WriteOptions;

#[cfg(feature = "transitional")]
use super::Embedded;
use super::open;
#[cfg(all(feature = "strict", feature = "transitional"))]
use super::store_strict;
#[cfg(feature = "transitional")]
use super::store_transitional;
use crate::xlsx::Xlsx;

const WORKBOOK_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
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

fn value_cell(reference: &str, value: u32) -> Cell {
    let mut holding = CellValue::default();
    holding.value = value.to_string();
    let mut cell = Cell::default();
    cell.r = Some(reference.to_owned());
    cell.value = Some(Box::new(holding));
    cell
}

/// A worksheet whose row 2 holds `b2` in B2 and `c2` in C2.
fn sheet_with(b2: u32, c2: u32) -> Worksheet {
    let mut row = Row::default();
    row.cell = vec![value_cell("B2", b2), value_cell("C2", c2)];
    row.r = Some("2".to_owned());
    let mut sheet = Worksheet::default();
    sheet.sheet_data.row = vec![row];
    sheet
}

/// Store `sheet` as the first worksheet of `workbook`, in the workbook's class.
fn set_sheet(workbook: &mut Xlsx, sheet: &Worksheet) {
    let class = workbook.conformance_class();
    store(
        workbook.package_mut(),
        &name(SHEET),
        worksheet::CONTENT_TYPE,
        sheet,
        class,
        WriteOptions::default(),
    )
    .unwrap();
}

/// The reference and value of each cell of `workbook`'s first worksheet.
#[cfg(feature = "transitional")]
fn values(workbook: &Xlsx) -> Vec<(String, String)> {
    worksheet::load(workbook.package(), &name(SHEET), OpenOptions::default())
        .unwrap()
        .sheet_data
        .row
        .iter()
        .flat_map(|row| &row.cell)
        .map(|cell| {
            (
                cell.r.clone().unwrap_or_default(),
                cell.value
                    .as_ref()
                    .map_or_else(String::new, |held| held.value.clone()),
            )
        })
        .collect()
}

/// A Word document holding a workbook at `/word/embeddings/Microsoft_Excel_Worksheet.xlsx`.
fn document_with_workbook() -> (Package, PartName) {
    let mut package = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .package()
        .duplicate()
        .unwrap();
    let mut workbook = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    set_sheet(&mut workbook, &sheet_with(1, 2));
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
        values(&workbook),
        [
            ("B2".to_owned(), "1".to_owned()),
            ("C2".to_owned(), "2".to_owned())
        ]
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_workbook_edit_writes_back_with_its_edit() {
    let (mut package, part) = document_with_workbook();
    let mut workbook = workbook_of(open(&package, &part, OpenOptions::default()).unwrap());
    set_sheet(&mut workbook, &sheet_with(42, 17));
    let warnings = store_transitional(&mut package, &part, &Embedded::Xlsx(workbook)).unwrap();
    assert_eq!(warnings, []);
    assert_eq!(package.content_type(&part), Some(WORKBOOK_TYPE));
    let reopened = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    let again = workbook_of(open(&reopened, &part, OpenOptions::default()).unwrap());
    assert_eq!(
        values(&again),
        [
            ("B2".to_owned(), "42".to_owned()),
            ("C2".to_owned(), "17".to_owned())
        ]
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
