use officekit_packaging::part_name::PartName;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::Document as Part;
use officekit_xml::error::WriteError;
use officekit_xml::parse::parse;
use officekit_xml::values::DateTime;
use officekit_xml::write::{Layout, WriteOptions};

use super::{SaveWarning, cell_date, convert_date_cells, serial_date_time};
use crate::markup::spreadsheet::Worksheet;
use crate::transitional::Transitional as _;

const X: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const KEEP: WriteOptions = WriteOptions::new(Layout::Compact, false);

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

fn worksheet(cells: &str) -> Worksheet {
    let text = format!(
        "<x:worksheet xmlns:x=\"{X}\"><x:sheetData><x:row r=\"1\">{cells}</x:row></x:sheetData></x:worksheet>"
    );
    Worksheet::from_transitional(&parse(text.as_bytes()).unwrap().root).unwrap()
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
    assert_eq!(serial("1899-12-31T12:00", false).as_deref(), Some("1.5"));
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
    assert_eq!(serial("1900-01-01T12:00:00", false).as_deref(), Some("2.5"));
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
    let mut sheet = worksheet(
        "<x:c r=\"A1\" s=\"3\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c>\
         <x:c r=\"B1\"><x:v>7</x:v></x:c>\
         <x:c r=\"C1\" t=\"d\"><x:v>1976-11-22T08:30</x:v></x:c>",
    );
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
    let root = sheet.to_transitional().unwrap();
    let mut tree = Part::new("", root);
    tree.declaration = None;
    tree.prolog.clear();
    let written = String::from_utf8(tree.to_xml(KEEP)).unwrap();
    assert!(
        written.contains(
            "<x:c r=\"A1\" s=\"3\"><x:v>46309</x:v></x:c><x:c r=\"B1\"><x:v>7</x:v></x:c>"
        ),
        "{written}"
    );
    assert!(
        written.contains(&format!("<x:c r=\"C1\"><x:v>{serial_c1}</x:v></x:c>")),
        "{written}"
    );
}

#[test]
fn a_1904_workbook_counts_from_its_own_base_date() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    let mut sheet = worksheet("<x:c r=\"A1\" t=\"d\"><x:v>2026-10-14T00:00:00</x:v></x:c>");
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
}

#[test]
fn a_date_cell_whose_text_is_not_a_date_stays_and_is_refused() {
    let part = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    let mut sheet = worksheet("<x:c r=\"A1\" t=\"d\"><x:v>soon</x:v></x:c>");
    assert_eq!(convert_date_cells(&mut sheet, false, &part), Vec::new());
    assert!(matches!(
        sheet.to_transitional(),
        Err(WriteError::NotInClass {
            class: ConformanceClass::Transitional,
            tag: "c/@t",
        })
    ));
}
