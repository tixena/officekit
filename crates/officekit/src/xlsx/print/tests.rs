use officekit_markup::spreadsheet::{DefinedName, DefinedNames, Workbook};

use super::{PrintTitles, print_areas, print_titles};
use crate::parts::kinds::SpreadsheetKind;
use crate::xlsx::Xlsx;
use crate::xlsx::reference::{Area, LAST_COLUMN, LAST_ROW};

/// A workbook whose defined names are `names`, each a name, a `localSheetId` and a text.
fn workbook(names: &[(&str, Option<&str>, &str)]) -> Workbook {
    let mut defined = DefinedNames::default();
    for &(name, sheet, text) in names {
        let mut one = DefinedName::default();
        one.name = Some(Box::from(name));
        one.local_sheet_id = sheet.map(Box::from);
        one.value = Box::from(text);
        defined.defined_name.push(one);
    }
    let mut workbook = Workbook::default();
    workbook.defined_names = Some(Box::new(defined));
    workbook
}

#[test]
fn a_print_area_is_its_sheet_s() {
    let found = workbook(&[("_xlnm.Print_Area", Some("2"), "Fit!$A$1:$T$100")]);
    assert_eq!(print_areas(&found, 2), [Area::new(1, 1, 20, 100)]);
    assert_eq!(print_areas(&found, 1), []);
}

#[test]
fn several_areas_keep_their_order() {
    let found = workbook(&[(
        "_xlnm.Print_Area",
        Some("0"),
        "Sheet1!$E$1:$F$3,Sheet1!$A$1:$C$3",
    )]);
    assert_eq!(
        print_areas(&found, 0),
        [Area::new(5, 1, 6, 3), Area::new(1, 1, 3, 3)]
    );
}

#[test]
fn repeated_rows_and_columns() {
    let both = workbook(&[("_xlnm.Print_Titles", Some("1"), "Big!$A:$A,Big!$1:$1")]);
    assert_eq!(
        print_titles(&both, 1),
        PrintTitles::new(Some((1, 1)), Some((1, 1)))
    );
    let rows = workbook(&[("_xlnm.Print_Titles", Some("1"), "Big!$1:$2")]);
    assert_eq!(print_titles(&rows, 1), PrintTitles::new(None, Some((1, 2))));
}

#[test]
fn the_first_rows_and_the_first_columns_are_taken() {
    let found = workbook(&[(
        "_xlnm.Print_Titles",
        Some("0"),
        "S!$3:$4,S!$B:$C,S!$A$1:$B$2,S!$1:$1,S!$D:$D",
    )]);
    assert_eq!(
        print_titles(&found, 0),
        PrintTitles::new(Some((2, 3)), Some((3, 4)))
    );
}

#[test]
fn a_quoted_sheet_name_is_read_past() {
    let rows = workbook(&[("_xlnm.Print_Titles", Some("0"), "'My sheet'!$1:$2")]);
    assert_eq!(print_titles(&rows, 0), PrintTitles::new(None, Some((1, 2))));
    let quoted = workbook(&[(
        "_xlnm.Print_Area",
        Some("0"),
        "'It''s, really!'!$A$1:$B$2,'A,B'!$C$3",
    )]);
    assert_eq!(
        print_areas(&quoted, 0),
        [Area::new(1, 1, 2, 2), Area::new(3, 3, 3, 3)]
    );
}

#[test]
fn a_part_that_is_no_area_is_passed_over() {
    let deleted = workbook(&[("_xlnm.Print_Area", Some("0"), "#REF!")]);
    assert_eq!(print_areas(&deleted, 0), []);
    let some = workbook(&[("_xlnm.Print_Area", Some("0"), "Sheet1!$A$1:$C$3,#REF!")]);
    assert_eq!(print_areas(&some, 0), [Area::new(1, 1, 3, 3)]);
    let function = workbook(&[("_xlnm.Print_Area", Some("0"), "OFFSET(Sheet1!$A$1,0,0)")]);
    assert_eq!(print_areas(&function, 0), []);
    let others = workbook(&[(
        "_xlnm.Print_Area",
        Some("0"),
        "Sheet1:Sheet2!$A$1,'One:Two'!$A$1,[1]Sheet1!$A$1,Total,Sheet1!Total",
    )]);
    assert_eq!(print_areas(&others, 0), []);
}

#[test]
fn the_name_is_taken_in_any_case_but_only_for_a_sheet() {
    let upper = workbook(&[("_XLNM.PRINT_AREA", Some("0"), "S!$A$1")]);
    assert_eq!(print_areas(&upper, 0), [Area::new(1, 1, 1, 1)]);
    let global = workbook(&[("_xlnm.Print_Area", None, "S!$A$1")]);
    assert_eq!(print_areas(&global, 0), []);
    let titles = workbook(&[("_xlnm.print_titles", Some("0"), "S!$A:$B")]);
    assert_eq!(
        print_titles(&titles, 0),
        PrintTitles::new(Some((1, 2)), None)
    );
}

#[test]
fn whole_rows_in_a_print_area_span_every_column() {
    let found = workbook(&[("_xlnm.Print_Area", Some("0"), "S!$2:$5,S!$B:$B")]);
    assert_eq!(
        print_areas(&found, 0),
        [
            Area::new(1, 2, LAST_COLUMN, 5),
            Area::new(2, 1, 2, LAST_ROW)
        ]
    );
}

#[test]
fn a_new_workbook_names_nothing() {
    let xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let found = xlsx.workbook().unwrap();
    assert_eq!(print_areas(&found, 0), []);
    assert_eq!(print_titles(&found, 0), PrintTitles::default());
    assert_eq!(
        print_titles(&Workbook::default(), 0),
        PrintTitles::default()
    );
}
