use super::{Area, CellReference, LAST_COLUMN, LAST_ROW, area, cell, column_name};

#[test]
fn a_cell_gives_its_column_and_row() {
    let cells: Vec<Option<CellReference>> = ["A1", "$B$12", "ad58", "XFD1048576", "Z9", "$AA10"]
        .into_iter()
        .map(cell)
        .collect();
    assert_eq!(
        cells,
        [
            Some(CellReference::new(1, 1)),
            Some(CellReference::new(2, 12)),
            Some(CellReference::new(30, 58)),
            Some(CellReference::new(LAST_COLUMN, LAST_ROW)),
            Some(CellReference::new(26, 9)),
            Some(CellReference::new(27, 10)),
        ]
    );
}

#[test]
fn what_is_no_cell_gives_nothing() {
    for text in [
        "A0", "1A", "XFE1", "A1048577", "", "A1:B2", "1", "A", "$", "A$", "A1$", "$$A1",
    ] {
        assert_eq!(cell(text), None, "{text}");
    }
}

#[test]
fn an_area_of_cells_is_put_in_order() {
    assert_eq!(area("A1:C3"), Some(Area::new(1, 1, 3, 3)));
    assert_eq!(area("$A$1:$T$100"), Some(Area::new(1, 1, 20, 100)));
    assert_eq!(area("C3:A1"), area("A1:C3"));
    assert_eq!(area("B2"), Some(Area::new(2, 2, 2, 2)));
    assert_eq!(area("A3:C1"), Some(Area::new(1, 1, 3, 3)));
}

#[test]
fn whole_rows_and_whole_columns_span_the_sheet() {
    let rows = area("1:3").unwrap();
    assert_eq!(rows, Area::new(1, 1, LAST_COLUMN, 3));
    assert!(rows.whole_rows() && !rows.whole_columns());
    let columns = area("$A:$B").unwrap();
    assert_eq!(columns, Area::new(1, 1, 2, LAST_ROW));
    assert!(columns.whole_columns() && !columns.whole_rows());
    assert_eq!(area("5:5"), Some(Area::new(1, 5, LAST_COLUMN, 5)));
    assert_eq!(area("$3:1"), Some(Area::new(1, 1, LAST_COLUMN, 3)));
}

#[test]
fn what_is_no_area_gives_nothing() {
    for text in [
        "A1:3",
        "A:3",
        "0:1",
        "A1:B2:C3",
        ":",
        "A:",
        "Sheet1!A1",
        "XFE:XFE",
    ] {
        assert_eq!(area(text), None, "{text}");
    }
}

#[test]
fn a_column_s_letters() {
    let names: Vec<Option<String>> = [1, 26, 27, 30, 702, 703, LAST_COLUMN, 0, LAST_COLUMN + 1]
        .into_iter()
        .map(column_name)
        .collect();
    let expected = [
        Some("A"),
        Some("Z"),
        Some("AA"),
        Some("AD"),
        Some("ZZ"),
        Some("AAA"),
        Some("XFD"),
        None,
        None,
    ];
    assert_eq!(names, expected.map(|name| name.map(str::to_owned)));
}

#[test]
fn every_column_s_letters_give_it_back() {
    for column in 1..=LAST_COLUMN {
        let letters = column_name(column).unwrap();
        assert_eq!(
            cell(&format!("{letters}1")),
            Some(CellReference::new(column, 1)),
            "{letters}"
        );
    }
}
