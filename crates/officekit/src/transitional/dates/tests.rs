use officekit_markup::spreadsheet::{Cell, CellType, CellValue, Row, Worksheet};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::error::WriteError;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::values::DateTime;

use super::{SaveWarning, cell_date, convert_date_cells, serial_date_time};
use crate::transitional::Transitional as _;

/// The serial date-time of a date cell's text, as Rust writes the `f64`.
fn serial(text: &str, date1904: bool) -> Option<String> {
    let date = cell_date(text, date1904)?;
    serial_date_time(&date, date1904).map(|value| value.to_string())
}

/// The serial date-time of a date cell's text to `places` decimal places.
fn rounded(text: &str, date1904: bool, places: usize) -> Option<String> {
    let date = cell_date(text, date1904)?;
    serial_date_time(&date, date1904).map(|value| format!("{value:.places$}"))
}

/// A cell of `reference`, of the style `style` and the type `kind` when given, holding `value`.
fn cell(reference: &str, style: Option<&str>, kind: Option<&str>, value: &str) -> Cell {
    let mut held = CellValue::default();
    held.value = value.to_owned();
    let mut cell = Cell::default();
    cell.r = Some(reference.to_owned());
    cell.s = style.map(str::to_owned);
    cell.t = kind.map(|found| CellType::from(found.to_owned()));
    cell.value = Some(Box::new(held));
    cell
}

/// A worksheet whose row 1 holds `cells`.
fn worksheet(cells: Vec<Cell>) -> Worksheet {
    let mut row = Row::default();
    row.cell = cells;
    row.r = Some("1".to_owned());
    let mut sheet = Worksheet::default();
    sheet.sheet_data.row = vec![row];
    sheet
}

/// The reference, style, type and value of each cell of `sheet`'s first row.
fn cells(sheet: &Worksheet) -> Vec<(String, Option<String>, Option<String>, String)> {
    sheet.sheet_data.row.first().map_or_default(|row| {
        row.cell
            .iter()
            .map(|found| {
                (
                    found.r.clone().unwrap_or_default(),
                    found.s.clone(),
                    found.t.as_ref().map(|kind| kind.0.clone()),
                    found
                        .value
                        .as_ref()
                        .map_or_else(String::new, |held| held.value.clone()),
                )
            })
            .collect()
    })
}

#[test]
fn the_date_of_a_save_gives_its_serial_in_both_systems() {
    assert_eq!(
        serial("2026-10-14T00:00:00", false).as_deref(),
        Some("46309")
    );
    assert_eq!(
        serial("2026-10-14T00:00:00", true).as_deref(),
        Some("44847")
    );
}

#[test]
fn the_standards_dates_give_their_serial_date_times() {
    // ECMA-376 Part 1, 18.17.4.1: the examples and the limits of both systems.
    assert_eq!(serial("1899-12-25T18:00", false).as_deref(), Some("-4.25"));
    assert_eq!(serial("0001-01-01", false).as_deref(), Some("-693593"));
    assert_eq!(serial("0001-01-01", true).as_deref(), Some("-695055"));
    assert_eq!(
        rounded("9999-12-31T23:59:59.999", false, 8).as_deref(),
        Some("2958465.99999999")
    );
    assert_eq!(
        rounded("9999-12-31T23:59:59.999", true, 8).as_deref(),
        Some("2957003.99999999")
    );
    // 18.17.4.2: times of day, on the system's base date.
    assert_eq!(serial("00:00:00", false).as_deref(), Some("0"));
    assert_eq!(rounded("00:00:01", false, 7).as_deref(), Some("0.0000116"));
    assert_eq!(rounded("10:05:54", true, 7).as_deref(), Some("0.4207639"));
    assert_eq!(serial("12:00", false).as_deref(), Some("0.5"));
    assert_eq!(rounded("23:59:59", false, 7).as_deref(), Some("0.9999884"));
    // 18.17.4.3: dates with times, and the note's -1.25.
    assert_eq!(serial("1899-12-28T18:00", false).as_deref(), Some("-1.25"));
    assert_eq!(
        rounded("1893-08-05T00:00:01", false, 5).as_deref(),
        Some("-2337.99999")
    );
    assert_eq!(
        rounded("1910-02-03T10:05:54", false, 7).as_deref(),
        Some("3687.4207639")
    );
    assert_eq!(
        rounded("9999-12-31T23:59:59", false, 7).as_deref(),
        Some("2958465.9999884")
    );
    assert_eq!(
        rounded("1893-08-05T00:00:01", true, 5).as_deref(),
        Some("-3799.99999")
    );
    assert_eq!(
        rounded("1910-02-03T10:05:54", true, 7).as_deref(),
        Some("2225.4207639")
    );
    assert_eq!(serial("1904-01-01T12:00:00", true).as_deref(), Some("0.5"));
    assert_eq!(
        rounded("9999-12-31T23:59:59", true, 7).as_deref(),
        Some("2957003.9999884")
    );
}

#[test]
fn a_transitional_workbook_counts_1900_01_01_as_one() {
    assert_eq!(serial("1899-12-30", false).as_deref(), Some("0"));
    assert_eq!(serial("1899-12-31T12:00", false).as_deref(), Some("0.5"));
    assert_eq!(serial("1900-01-01", false).as_deref(), Some("1"));
    assert_eq!(serial("1900-01-01T12:00:00", false).as_deref(), Some("1.5"));
    assert_eq!(serial("1900-02-28", false).as_deref(), Some("59"));
    assert_eq!(serial("1900-03-01", false).as_deref(), Some("61"));
    assert_eq!(serial("12:00", false).as_deref(), Some("0.5"));
    assert_eq!(serial("1904-01-01", true).as_deref(), Some("0"));
}

#[test]
fn text_that_is_no_date_has_no_serial() {
    for text in [
        "2026-02-30",
        "2026-10-14T24:00",
        "2026-10-14T10:00:00Z",
        "26-10-14",
        "0000-12-31",
        "today",
        "",
    ] {
        assert_eq!(serial(text, false), None, "{text}");
    }
    let out_of_range = DateTime::new((10_000_i32, 1, 1), (0, 0, 0, 0), None);
    assert_eq!(serial_date_time(&out_of_range, false), None);
}

#[test]
fn each_date_cell_becomes_a_number_cell_with_one_warning() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    let mut sheet = worksheet(vec![
        cell("A1", Some("3"), Some("d"), "2026-10-14T00:00:00"),
        cell("B1", None, None, "7"),
        cell("C1", None, Some("d"), "1976-11-22T08:30"),
    ]);
    let warnings = convert_date_cells(&mut sheet, false, &part);
    let serial_c1 = serial("1976-11-22T08:30", false).unwrap();
    assert_eq!(
        warnings,
        vec![
            SaveWarning::DateCellConverted {
                cell: "A1".to_owned(),
                date: "2026-10-14T00:00:00".to_owned(),
                part: part.clone(),
                serial: "46309".to_owned(),
            },
            SaveWarning::DateCellConverted {
                cell: "C1".to_owned(),
                date: "1976-11-22T08:30".to_owned(),
                part: part.clone(),
                serial: serial_c1.clone(),
            },
        ]
    );
    assert_eq!(
        cells(&sheet),
        [
            (
                "A1".to_owned(),
                Some("3".to_owned()),
                None,
                "46309".to_owned()
            ),
            ("B1".to_owned(), None, None, "7".to_owned()),
            ("C1".to_owned(), None, None, serial_c1),
        ]
    );
}

#[test]
fn a_1904_workbook_counts_from_its_own_base_date() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    let mut sheet = worksheet(vec![cell("A1", None, Some("d"), "2026-10-14T00:00:00")]);
    let warnings = convert_date_cells(&mut sheet, true, &part);
    assert!(matches!(
        warnings.as_slice(),
        [SaveWarning::DateCellConverted {
            cell: _cell,
            date: _date,
            part: _part,
            serial,
        }] if serial == "44847"
    ));
    assert_eq!(
        cells(&sheet),
        [("A1".to_owned(), None, None, "44847".to_owned())]
    );
}

#[test]
fn a_date_cell_whose_text_is_not_a_date_stays_and_is_refused() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    let mut sheet = worksheet(vec![cell("A1", None, Some("d"), "soon")]);
    assert_eq!(convert_date_cells(&mut sheet, false, &part), Vec::new());
    assert_eq!(
        cells(&sheet),
        [(
            "A1".to_owned(),
            None,
            Some("d".to_owned()),
            "soon".to_owned()
        )]
    );
    assert!(matches!(
        sheet.to_transitional(),
        Err(WriteError::NotInClass {
            class: ConformanceClass::Transitional,
            tag: "c/@t",
        })
    ));
}

/// A worksheet whose one cell, A1, is a date cell holding `date`, built as a caller builds one.
fn date_sheet(date: &str) -> Worksheet {
    let mut value = CellValue::default();
    value.value = date.to_owned();
    let mut cell = Cell::default();
    cell.r = Some("A1".to_owned());
    cell.t = Some(CellType::from("d".to_owned()));
    cell.value = Some(Box::new(value));
    let mut row = Row::default();
    row.r = Some("1".to_owned());
    row.cell.push(cell);
    let mut sheet = Worksheet::default();
    sheet.sheet_data.row.push(row);
    sheet
}

/// The type and the value of the one cell of `sheet`.
fn only_cell(sheet: &Worksheet) -> (Option<&str>, &str) {
    let cell = sheet
        .sheet_data
        .row
        .first()
        .and_then(|row| row.cell.first())
        .unwrap();
    (
        cell.t.as_ref().map(|kind| kind.0.as_str()),
        cell.value.as_ref().map_or("", |value| value.value.as_str()),
    )
}

#[test]
fn a_date_before_the_1900_range_stays_and_is_refused() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    for date in ["1899-12-25", "1899-12-28T18:00"] {
        let mut sheet = date_sheet(date);
        assert_eq!(
            convert_date_cells(&mut sheet, false, &part),
            Vec::new(),
            "{date}"
        );
        assert_eq!(only_cell(&sheet), (Some("d"), date));
        assert!(matches!(
            sheet.to_transitional(),
            Err(WriteError::NotInClass {
                class: ConformanceClass::Transitional,
                tag: "c/@t",
            })
        ));
    }
    for (date, serial) in [
        ("1899-12-30", "0"),
        ("00:00", "0"),
        ("1899-12-31T12:00", "0.5"),
        ("1900-01-01", "1"),
    ] {
        let mut sheet = date_sheet(date);
        assert_eq!(
            convert_date_cells(&mut sheet, false, &part).len(),
            1,
            "{date}"
        );
        assert_eq!(only_cell(&sheet), (None, serial));
    }
    let mut system_1904 = date_sheet("1899-12-25");
    assert_eq!(convert_date_cells(&mut system_1904, true, &part).len(), 1);
}

#[test]
fn a_date_cell_warning_reads_as_one_line() {
    let warning = SaveWarning::DateCellConverted {
        cell: "A1".to_owned(),
        date: "2024-01-01T00:00:00".to_owned(),
        part: PartName::new("/xl/worksheets/sheet1.xml").unwrap(),
        serial: "45292".to_owned(),
    };
    assert_eq!(
        warning.to_string(),
        "/xl/worksheets/sheet1.xml A1: the date 2024-01-01T00:00:00 was written as the number 45292"
    );
}
