use alloc::borrow::Cow;

use officekit_markup::spreadsheet::{
    Bold, Cell, CellType, CellValue, FormulaF, PhoneticProperties, PhoneticRun, RichTextInline,
    RichTextRun, RunProperties, RunPropertiesContent, SharedStringTable, StringItem, Text,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::WriteOptions;

use super::{CellContent, SharedStrings, TextRun, content, unescape};
use crate::parts::kinds::SpreadsheetKind;
use crate::parts::part_root::store;
use crate::parts::shared_string_table;
use crate::xlsx::Xlsx;

fn bold() -> RunProperties {
    let mut properties = RunProperties::default();
    properties
        .content
        .push(RunPropertiesContent::Bold(Bold::default()));
    properties
}

/// A cell whose `t` is `kind` and whose `v` is `value`, each when given.
fn cell(kind: Option<&str>, value: Option<&str>) -> Cell {
    let mut node = Cell::default();
    node.t = kind.map(|text| CellType::from(text.to_owned()));
    node.value = value.map(|text| {
        let mut stored = CellValue::default();
        stored.value = text.to_owned();
        Box::new(stored)
    });
    node
}

/// A string item holding the plain text `plain`, when given, then `runs`.
fn item(plain: Option<&str>, runs: Vec<RichTextRun>) -> StringItem {
    let mut node = StringItem::default();
    node.text = plain.map(|value| Box::new(text(value)));
    node.rich_text_run = runs;
    node
}

fn rich(value: &str, properties: Option<RunProperties>) -> RichTextRun {
    let mut node = RichTextRun::default();
    node.text = Box::new(text(value));
    node.properties = properties.map(Box::new);
    node
}

fn table(items: Vec<StringItem>) -> SharedStringTable {
    let mut root = SharedStringTable::default();
    root.string_item = items;
    root
}

fn text(value: &str) -> Text {
    let mut node = Text::default();
    node.value = value.to_owned();
    node
}

fn three() -> SharedStrings {
    SharedStrings::new(Some(&table(vec![
        item(Some("first"), Vec::new()),
        item(Some("second"), Vec::new()),
        item(Some("third"), Vec::new()),
    ])))
}

#[test]
fn a_number_is_its_text_with_no_white_space_at_its_ends() {
    let strings = SharedStrings::default();
    let untyped = cell(None, Some(" 42.5 "));
    let typed = cell(Some("n"), Some("42.5"));
    assert_eq!(content(&untyped, &strings), CellContent::Number("42.5"));
    assert_eq!(content(&typed, &strings), CellContent::Number("42.5"));
}

#[test]
fn a_formula_gives_its_stored_value() {
    let strings = SharedStrings::default();
    let mut sum = cell(None, Some("1229.5"));
    let mut formula = FormulaF::default();
    formula.value = "SUM(C3:C4)".to_owned();
    sum.formula_f = Some(Box::new(formula.clone()));
    let mut string = cell(Some("str"), Some("mono"));
    string.formula_f = Some(Box::new(formula));
    assert_eq!(content(&sum, &strings), CellContent::Number("1229.5"));
    assert_eq!(
        content(&string, &strings),
        CellContent::Text(vec![TextRun::new("mono", None)])
    );
}

#[test]
fn a_logical_value_an_error_and_a_date() {
    let strings = SharedStrings::default();
    let logical = ["1", "0", "true", "false", "2"].map(|value| cell(Some("b"), Some(value)));
    let found = logical.each_ref().map(|node| content(node, &strings));
    assert_eq!(
        found,
        [
            CellContent::Boolean(true),
            CellContent::Boolean(false),
            CellContent::Boolean(true),
            CellContent::Boolean(false),
            CellContent::Blank,
        ]
    );
    let error = cell(Some("e"), Some("#DIV/0!"));
    let date = cell(Some("d"), Some("2025-12-09T00:00:00"));
    assert_eq!(content(&error, &strings), CellContent::Error("#DIV/0!"));
    assert_eq!(
        content(&date, &strings),
        CellContent::Date("2025-12-09T00:00:00")
    );
}

#[test]
fn a_cell_with_no_value_is_blank_and_an_unknown_type_is_a_number() {
    let strings = three();
    for kind in [
        None,
        Some("b"),
        Some("d"),
        Some("e"),
        Some("n"),
        Some("s"),
        Some("str"),
    ] {
        assert_eq!(content(&cell(kind, None), &strings), CellContent::Blank);
    }
    let unknown = cell(Some("number"), Some("7"));
    assert_eq!(content(&unknown, &strings), CellContent::Number("7"));
}

#[test]
fn a_shared_string_is_the_runs_of_the_item_its_index_names() {
    let strings = three();
    assert_eq!(strings.len(), 3);
    assert!(!strings.is_empty());
    assert_eq!(
        content(&cell(Some("s"), Some("1")), &strings),
        CellContent::Text(vec![TextRun::new("second", None)])
    );
}

#[test]
fn a_string_of_runs_gives_each_run_with_its_properties() {
    let properties = bold();
    let strings = SharedStrings::new(Some(&table(vec![item(
        None,
        vec![rich("Total ", None), rich("due", Some(properties.clone()))],
    )])));
    assert_eq!(
        strings.get(0),
        Some(vec![
            TextRun::new("Total ", None),
            TextRun::new("due", Some(&properties)),
        ])
    );
}

#[test]
fn a_string_gives_its_text_then_its_runs_and_nothing_of_a_phonetic_run() {
    let mut string = item(
        Some("plain "),
        vec![rich("one ", None), rich("two", Some(bold()))],
    );
    let mut phonetic = PhoneticRun::default();
    phonetic.text = Box::new(text("reading"));
    string.phonetic_run.push(phonetic);
    string.phonetic_properties = Some(Box::new(PhoneticProperties::default()));
    let strings = SharedStrings::new(Some(&table(vec![string])));
    let properties = bold();
    assert_eq!(
        strings.get(0),
        Some(vec![
            TextRun::new("plain ", None),
            TextRun::new("one ", None),
            TextRun::new("two", Some(&properties)),
        ])
    );
}

#[test]
fn a_string_of_no_characters_is_text_with_no_run() {
    let strings = SharedStrings::new(Some(&table(vec![item(Some(""), vec![rich("", None)])])));
    assert_eq!(
        content(&cell(Some("s"), Some("0")), &strings),
        CellContent::Text(Vec::new())
    );
    assert_eq!(
        content(&cell(Some("str"), Some("")), &strings),
        CellContent::Text(Vec::new())
    );
}

#[test]
fn an_index_nothing_has_is_blank() {
    let strings = three();
    assert_eq!(
        content(&cell(Some("s"), Some("7")), &strings),
        CellContent::Blank
    );
    assert_eq!(
        content(&cell(Some("s"), Some("x")), &strings),
        CellContent::Blank
    );
    let none = SharedStrings::default();
    assert!(none.is_empty());
    assert_eq!(
        content(&cell(Some("s"), Some("0")), &none),
        CellContent::Blank
    );
}

#[test]
fn an_escaped_character_is_decoded() {
    assert_eq!(unescape("a_x000D_b"), "a\rb");
    assert_eq!(unescape("_x0041_"), "A");
    assert_eq!(unescape("_x004a__x004A_"), "JJ");
}

#[test]
fn an_escaped_underscore_leaves_what_follows_as_text() {
    assert_eq!(unescape("_x005F_x000D_"), "_x000D_");
}

#[test]
fn what_is_no_escape_stays_and_a_text_with_none_is_borrowed() {
    for written in ["_x12_", "_xZZZZ_", "_xD800_", "a_x00", "_x+041_"] {
        assert_eq!(unescape(written), written);
    }
    assert_eq!(unescape("_xD800__x0041_"), "_xD800_A");
    assert!(matches!(
        unescape("plain text"),
        Cow::Borrowed("plain text")
    ));
    assert!(matches!(unescape("_x12_"), Cow::Borrowed("_x12_")));
}

#[test]
fn a_run_of_a_string_is_decoded() {
    let strings = SharedStrings::new(Some(&table(vec![item(Some("a_x0009_b"), Vec::new())])));
    assert_eq!(strings.get(0), Some(vec![TextRun::new("a\tb", None)]));
    assert_eq!(
        content(&cell(Some("str"), Some("c_x000A_d")), &strings),
        CellContent::Text(vec![TextRun::new("c\nd", None)])
    );
}

#[test]
fn a_string_in_the_cell_is_read_from_its_own_string() {
    let strings = SharedStrings::default();
    let mut inline = cell(Some("inlineStr"), None);
    let mut own = RichTextInline::default();
    own.text = Some(Box::new(text("inline")));
    inline.rich_text_inline = Some(Box::new(own));
    assert_eq!(
        content(&inline, &strings),
        CellContent::Text(vec![TextRun::new("inline", None)])
    );

    let properties = bold();
    let mut two = cell(Some("inlineStr"), None);
    let mut runs = RichTextInline::default();
    runs.rich_text_run = vec![rich("one", None), rich("two", Some(properties.clone()))];
    two.rich_text_inline = Some(Box::new(runs));
    assert_eq!(
        content(&two, &strings),
        CellContent::Text(vec![
            TextRun::new("one", None),
            TextRun::new("two", Some(&properties)),
        ])
    );

    let empty = cell(Some("inlineStr"), Some("ignored"));
    assert_eq!(content(&empty, &strings), CellContent::Blank);
}

#[test]
fn a_workbook_gives_the_string_items_of_its_shared_string_part() {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    assert!(xlsx.string_table().unwrap().is_empty());
    let part = PartName::new("/xl/sharedStrings.xml").unwrap();
    store(
        xlsx.package_mut(),
        &part,
        shared_string_table::CONTENT_TYPE,
        &table(vec![
            item(Some("one"), Vec::new()),
            item(Some("two"), Vec::new()),
        ]),
        ConformanceClass::Transitional,
        WriteOptions::default(),
    )
    .unwrap();
    let main = xlsx.workbook_part().clone();
    xlsx.package_mut()
        .relate(
            Some(&main),
            &part,
            shared_string_table::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    let strings = xlsx.string_table().unwrap();
    assert_eq!(strings.len(), 2);
    assert_eq!(strings.get(1), Some(vec![TextRun::new("two", None)]));
    assert_eq!(strings.get(2), None);
}
