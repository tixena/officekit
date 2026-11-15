use core::array::from_fn;

use crate::parts::kinds::SpreadsheetKind;
use crate::parts::part_root::{PartRoot, store};
use crate::parts::{
    chartsheet, dialogsheet, international_macro_sheet, macro_sheet, shared_string_table, styles,
    theme, worksheet,
};
use officekit_markup::drawing::Theme;
use officekit_markup::spreadsheet::{
    ChartSheet, DialogSheet, Fonts, SharedStringTable, SheetInformation, SheetVisibilityTypes,
    SheetVisibilityTypesMember, StyleSheet, Worksheet,
};
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "strict")]
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::write::WriteOptions;

use super::{SheetEntry, SheetKind};
use crate::xlsx::Xlsx;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// Store `root` at `part` in the Transitional class with `content_type`, relate it from the
/// workbook part by `relationship_type`, and give the relationship's id.
fn add<R>(
    xlsx: &mut Xlsx,
    part: &PartName,
    content_type: &str,
    root: &R,
    relationship_type: &RelationshipType,
) -> String
where
    R: PartRoot,
{
    store(
        xlsx.package_mut(),
        part,
        content_type,
        root,
        ConformanceClass::Transitional,
        WriteOptions::default(),
    )
    .unwrap();
    relate(xlsx, part, relationship_type)
}

/// Relate `part` from the workbook part by `relationship_type`; the relationship's id.
fn relate(xlsx: &mut Xlsx, part: &PartName, relationship_type: &RelationshipType) -> String {
    let main = xlsx.workbook_part().clone();
    xlsx.package_mut()
        .relate(Some(&main), part, relationship_type.transitional)
        .unwrap()
}

/// A new workbook with `N` worksheets, its own `sheet1.xml` first, and each one's name and
/// relationship id in the order of their part names.
fn with_worksheets<const N: usize>() -> (Xlsx, [(PartName, String); N]) {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let first = xlsx
        .workbook()
        .unwrap()
        .sheets
        .sheet_information
        .first()
        .and_then(|sheet| sheet.id.clone())
        .unwrap();
    let added = from_fn(|index| {
        let part = name(&format!("/xl/worksheets/sheet{}.xml", index + 1));
        if index == 0 {
            (part, first.clone())
        } else {
            let id = add(
                &mut xlsx,
                &part,
                worksheet::CONTENT_TYPE,
                &Worksheet::default(),
                &worksheet::RELATIONSHIP_TYPE,
            );
            (part, id)
        }
    });
    (xlsx, added)
}

/// A `sheet` element with the relationship id `id`, the name `sheet_name` and the state `state`.
fn sheet(id: Option<&str>, sheet_name: &str, state: Option<&str>) -> SheetInformation {
    let mut element = SheetInformation::default();
    element.id = id.map(str::to_owned);
    element.name = Some(sheet_name.to_owned());
    element.state = state.map(|text| SheetVisibilityTypes::from(text.to_owned()));
    element
}

/// Set the workbook's sheet list to `list`, in order.
fn set_list(xlsx: &mut Xlsx, list: Vec<SheetInformation>) {
    let mut workbook = xlsx.workbook().unwrap();
    workbook.sheets.sheet_information = list;
    xlsx.set_workbook(&workbook).unwrap();
}

fn entry(kind: SheetKind, sheet_name: &str, part: &PartName, position: u32) -> SheetEntry {
    SheetEntry {
        kind,
        name: sheet_name.to_owned(),
        part: part.clone(),
        position,
        state: SheetVisibilityTypesMember::Visible,
    }
}

/// The workbook of three worksheets, listed third, first and second, and the entries it gives.
fn listed_out_of_order() -> (Xlsx, Vec<SheetEntry>) {
    let (mut xlsx, [first, second, third]) = with_worksheets();
    set_list(
        &mut xlsx,
        vec![
            sheet(Some(&third.1), "Third", None),
            sheet(Some(&first.1), "First", None),
            sheet(Some(&second.1), "Second", None),
        ],
    );
    let expected = vec![
        entry(SheetKind::Worksheet, "Third", &third.0, 0),
        entry(SheetKind::Worksheet, "First", &first.0, 1),
        entry(SheetKind::Worksheet, "Second", &second.0, 2),
    ];
    (xlsx, expected)
}

#[test]
fn a_new_workbook_has_one_visible_worksheet_named_sheet1() {
    let xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    assert_eq!(
        xlsx.sheets().unwrap(),
        [entry(
            SheetKind::Worksheet,
            "Sheet1",
            &name("/xl/worksheets/sheet1.xml"),
            0
        )]
    );
}

#[test]
fn the_entries_follow_the_list_s_order_not_the_parts() {
    let (xlsx, expected) = listed_out_of_order();
    assert_eq!(xlsx.sheets().unwrap(), expected);
}

#[test]
fn the_state_is_visible_when_absent_or_no_value_of_st_sheet_state() {
    let (mut xlsx, [first, second, third, fourth]) = with_worksheets();
    set_list(
        &mut xlsx,
        vec![
            sheet(Some(&first.1), "A", Some("hidden")),
            sheet(Some(&second.1), "B", Some("veryHidden")),
            sheet(Some(&third.1), "C", None),
            sheet(Some(&fourth.1), "D", Some("nonsense")),
        ],
    );
    let states: Vec<SheetVisibilityTypesMember> = xlsx
        .sheets()
        .unwrap()
        .into_iter()
        .map(|found| found.state)
        .collect();
    assert_eq!(
        states,
        [
            SheetVisibilityTypesMember::Hidden,
            SheetVisibilityTypesMember::VeryHidden,
            SheetVisibilityTypesMember::Visible,
            SheetVisibilityTypesMember::Visible,
        ]
    );
}

#[test]
fn the_kind_comes_from_the_relationship_s_type() {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let chart = name("/xl/chartsheets/sheet1.xml");
    let dialog = name("/xl/dialogsheets/sheet1.xml");
    let macros = name("/xl/macrosheets/sheet1.xml");
    let international = name("/xl/macrosheets/intlsheet1.xml");
    let linked = name("/xl/worksheets/sheet1.xml");
    let chart_id = add(
        &mut xlsx,
        &chart,
        chartsheet::CONTENT_TYPE,
        &ChartSheet::default(),
        &chartsheet::RELATIONSHIP_TYPE,
    );
    let dialog_id = add(
        &mut xlsx,
        &dialog,
        dialogsheet::CONTENT_TYPE,
        &DialogSheet::default(),
        &dialogsheet::RELATIONSHIP_TYPE,
    );
    xlsx.package_mut()
        .set_part_bytes(&macros, macro_sheet::CONTENT_TYPE, &[])
        .unwrap();
    let macros_id = relate(&mut xlsx, &macros, &macro_sheet::RELATIONSHIP_TYPE);
    xlsx.package_mut()
        .set_part_bytes(&international, international_macro_sheet::CONTENT_TYPE, &[])
        .unwrap();
    let international_id = relate(
        &mut xlsx,
        &international,
        &international_macro_sheet::RELATIONSHIP_TYPE,
    );
    let linked_id = relate(&mut xlsx, &linked, &RelationshipType::HYPERLINK);
    set_list(
        &mut xlsx,
        vec![
            sheet(Some(&chart_id), "Chart", None),
            sheet(Some(&dialog_id), "Dialog", None),
            sheet(Some(&macros_id), "Macros", None),
            sheet(Some(&international_id), "International", None),
            sheet(Some(&linked_id), "Linked", None),
        ],
    );
    assert_eq!(
        xlsx.sheets().unwrap(),
        [
            entry(SheetKind::Chartsheet, "Chart", &chart, 0),
            entry(SheetKind::Dialogsheet, "Dialog", &dialog, 1),
            entry(SheetKind::Macrosheet, "Macros", &macros, 2),
            entry(SheetKind::Macrosheet, "International", &international, 3),
            entry(SheetKind::Other, "Linked", &linked, 4),
        ]
    );
}

#[test]
fn an_entry_with_no_part_is_passed_over_and_the_next_keeps_its_position() {
    let (mut xlsx, [only]) = with_worksheets();
    let main = xlsx.workbook_part().clone();
    let absent = relate(
        &mut xlsx,
        &name("/xl/worksheets/sheet9.xml"),
        &worksheet::RELATIONSHIP_TYPE,
    );
    let external = xlsx
        .package_mut()
        .relate_external(
            Some(&main),
            "https://example.com/sheet.xml",
            worksheet::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    set_list(
        &mut xlsx,
        vec![
            sheet(None, "No id", None),
            sheet(Some("rId99"), "Unknown id", None),
            sheet(Some(&external), "External", None),
            sheet(Some(&absent), "Absent", None),
            sheet(Some(&only.1), "Kept", None),
        ],
    );
    assert_eq!(
        xlsx.sheets().unwrap(),
        [entry(SheetKind::Worksheet, "Kept", &only.0, 4)]
    );
}

#[test]
fn a_new_workbook_relates_to_no_styles_strings_or_theme() {
    let xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    assert_eq!(xlsx.styles().unwrap(), None);
    assert_eq!(xlsx.shared_strings().unwrap(), None);
    assert_eq!(xlsx.theme().unwrap(), None);
}

#[test]
fn each_related_part_gives_the_root_that_was_stored() {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let styles_part = name("/xl/styles.xml");
    let strings_part = name("/xl/sharedStrings.xml");
    let theme_part = name("/xl/theme/theme1.xml");
    let mut fonts = Fonts::default();
    fonts.count = Some("0".to_owned());
    let mut style_sheet = StyleSheet::default();
    style_sheet.fonts = Some(Box::new(fonts));
    let mut strings = SharedStringTable::default();
    strings.unique_count = Some("0".to_owned());
    let mut theme_root = Theme::default();
    theme_root.name = Some("Office Theme".to_owned());
    add(
        &mut xlsx,
        &styles_part,
        styles::CONTENT_TYPE,
        &style_sheet,
        &styles::RELATIONSHIP_TYPE,
    );
    add(
        &mut xlsx,
        &strings_part,
        shared_string_table::CONTENT_TYPE,
        &strings,
        &shared_string_table::RELATIONSHIP_TYPE,
    );
    add(
        &mut xlsx,
        &theme_part,
        theme::CONTENT_TYPE,
        &theme_root,
        &theme::RELATIONSHIP_TYPE,
    );
    let options = xlsx.options();
    let found_styles = xlsx.styles().unwrap().unwrap();
    let found_strings = xlsx.shared_strings().unwrap().unwrap();
    let found_theme = xlsx.theme().unwrap().unwrap();
    assert_eq!(
        found_styles,
        styles::load(xlsx.package(), &styles_part, options).unwrap()
    );
    assert_eq!(
        found_strings,
        shared_string_table::load(xlsx.package(), &strings_part, options).unwrap()
    );
    assert_eq!(
        found_theme,
        theme::load(xlsx.package(), &theme_part, options).unwrap()
    );
    assert_eq!(found_styles.fonts, style_sheet.fonts);
    assert_eq!(found_strings.unique_count, strings.unique_count);
    assert_eq!(found_theme.name, theme_root.name);
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_workbook_gives_the_same_entries() {
    let (xlsx, expected) = listed_out_of_order();
    let strict = Xlsx::from_bytes(xlsx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    assert_eq!(strict.conformance_class(), ConformanceClass::Strict);
    assert_eq!(strict.sheets().unwrap(), expected);
}
