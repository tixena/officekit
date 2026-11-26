use officekit_markup::common_simple_types::{
    VerticalPositioningLocation, VerticalPositioningLocationMember,
};
use officekit_markup::spreadsheet::{
    Alignment, Bold, Border, BorderId, Borders, Cell, CellFormats as CellFormatList, CellStyle,
    CellStyleFormatId, CellStyles, CharacterSet, ColumnInformation, ColumnWidthFormatting,
    Condense, DataBarColor, Extend, Fill, FillContent, FillId, Fills, Font, FontContent,
    FontFamily, FontFamilyIntProperty, FontId, FontName, FontRFont, FontSchemeStyles,
    FontSchemeStylesMember, FontSize, Fonts, Format, FormattingRecords, HorizontalAlignmentType,
    Italic, NumberFormat, NumberFormatId, NumberFormats, Outline, Pattern, PatternType, Row,
    RunProperties, RunPropertiesContent, Scheme, Shadow, StFontFamily, StrikeThrough, StyleSheet,
    Underline, UnderlineTypes, UnderlineTypesMember, VerticalAlignment,
};

use super::{CellFormat, CellFormats, FontProperties};
use crate::parts::kinds::SpreadsheetKind;
use crate::xlsx::Xlsx;

/// A font that states only its name.
fn font_named(name: &str) -> Font {
    let mut node = FontName::default();
    node.val = Some(name.to_owned());
    let mut font = Font::default();
    font.content.push(FontContent::from(node));
    font
}

/// A pattern fill of `pattern`.
fn fill(pattern: &str) -> Fill {
    let mut node = Pattern::default();
    node.pattern_type = Some(PatternType::from(pattern.to_owned()));
    let mut fill = Fill::default();
    fill.content = Some(Box::new(FillContent::from(node)));
    fill
}

/// A border told apart from the others by its `outline`.
fn border(marker: &str) -> Border {
    let mut border = Border::default();
    border.outline = Some(marker.to_owned());
    border
}

/// An `xf` naming `font`, `fill`, `border` and number format `format`.
fn entry(font: &str, fill: &str, border: &str, format: &str) -> Format {
    let mut entry = Format::default();
    entry.font_id = Some(FontId::from(font.to_owned()));
    entry.fill_id = Some(FillId::from(fill.to_owned()));
    entry.border_id = Some(BorderId::from(border.to_owned()));
    entry.num_fmt_id = Some(NumberFormatId::from(format.to_owned()));
    entry
}

/// A styles part of two fonts, three fills and two borders, whose `cellXfs` are `entries`.
fn styles(entries: Vec<Format>) -> StyleSheet {
    let mut fonts = Fonts::default();
    fonts.font = vec![font_named("Calibri"), font_named("Arial")];
    let mut fills = Fills::default();
    fills.fill = vec![fill("none"), fill("gray125"), fill("solid")];
    let mut borders = Borders::default();
    borders.border = vec![border("0"), border("1")];
    let mut list = CellFormatList::default();
    list.format = entries;
    let mut sheet = StyleSheet::default();
    sheet.fonts = Some(Box::new(fonts));
    sheet.fills = Some(Box::new(fills));
    sheet.borders = Some(Box::new(borders));
    sheet.cell_formats = Some(Box::new(list));
    sheet
}

fn number_format(id: &str, code: &str) -> NumberFormat {
    let mut format = NumberFormat::default();
    format.num_fmt_id = Some(NumberFormatId::from(id.to_owned()));
    format.format_code = Some(code.to_owned());
    format
}

fn nothing(format: &CellFormat<'_>) -> bool {
    format.alignment.is_none()
        && format.border.is_none()
        && format.fill.is_none()
        && format.font.is_none()
}

#[test]
fn an_entry_gives_the_font_fill_and_border_its_ids_name_and_its_alignment() {
    let mut alignment = Alignment::default();
    alignment.horizontal = Some(HorizontalAlignmentType::from("center".to_owned()));
    let mut first = entry("1", "2", "1", "0");
    first.alignment = Some(Box::new(alignment.clone()));
    let formats = CellFormats::new(Some(&styles(vec![first])));
    let format = formats.format(0);
    assert_eq!(format.font, Some(&font_named("Arial")));
    assert_eq!(format.fill, Some(&fill("solid")));
    assert_eq!(format.border, Some(&border("1")));
    assert_eq!(format.alignment, Some(&alignment));
    assert_eq!(format.number_format, "General");
}

#[test]
fn an_index_or_an_id_past_its_list_gives_none_for_that_piece() {
    let formats = CellFormats::new(Some(&styles(vec![entry("7", "1", "9", "0")])));
    let past = formats.format(99);
    assert!(nothing(&past));
    assert_eq!(past.number_format, "General");
    let ids_past = formats.format(0);
    assert_eq!(ids_past.font, None);
    assert_eq!(ids_past.fill, Some(&fill("gray125")));
    assert_eq!(ids_past.border, None);
}

#[test]
fn a_workbook_with_no_styles_part_has_no_piece_and_the_general_format() {
    for formats in [CellFormats::new(None), CellFormats::default()] {
        let format = formats.format(0);
        assert!(nothing(&format));
        assert_eq!(format.number_format, "General");
        assert_eq!(formats.normal_font(), None);
    }
}

#[test]
fn the_workbook_s_own_number_format_comes_before_a_built_in_one() {
    let mut sheet = styles(vec![entry("0", "0", "0", "164"), entry("0", "0", "0", "4")]);
    let mut own = NumberFormats::default();
    own.number_format = vec![
        number_format("164", "yyyy-mm-dd"),
        number_format("4", "0.0"),
    ];
    sheet.number_formats = Some(Box::new(own));
    let formats = CellFormats::new(Some(&sheet));
    assert_eq!(formats.format(0).number_format, "yyyy-mm-dd");
    assert_eq!(formats.format(1).number_format, "0.0");
}

#[test]
fn a_built_in_id_gives_the_code_of_the_list_for_all_languages() {
    let ids = ["0", "4", "9", "14", "49"];
    let formats = CellFormats::new(Some(&styles(
        ids.iter().map(|id| entry("0", "0", "0", id)).collect(),
    )));
    let codes: Vec<&str> = (0..5)
        .map(|index| formats.format(index).number_format)
        .collect();
    assert_eq!(codes, ["General", "#,##0.00", "0%", "mm-dd-yy", "@"]);
}

#[test]
fn an_id_nothing_gives_is_the_general_format() {
    let mut no_id = entry("0", "0", "0", "0");
    no_id.num_fmt_id = None;
    let formats = CellFormats::new(Some(&styles(vec![entry("0", "0", "0", "200"), no_id])));
    assert_eq!(formats.format(0).number_format, "General");
    assert_eq!(formats.format(1).number_format, "General");
}

#[test]
fn the_apply_attributes_and_the_style_s_entry_change_nothing() {
    let mut applied = entry("1", "2", "1", "4");
    for apply in [
        &mut applied.apply_font,
        &mut applied.apply_fill,
        &mut applied.apply_border,
        &mut applied.apply_number_format,
        &mut applied.apply_alignment,
    ] {
        *apply = Some("0".to_owned());
    }
    applied.xf_id = Some(CellStyleFormatId::from("0".to_owned()));
    let mut sheet = styles(vec![applied]);
    let mut records = FormattingRecords::default();
    records.format = vec![entry("0", "0", "0", "0")];
    sheet.formatting_records = Some(Box::new(records));
    let formats = CellFormats::new(Some(&sheet));
    let format = formats.format(0);
    assert_eq!(format.font, Some(&font_named("Arial")));
    assert_eq!(format.fill, Some(&fill("solid")));
    assert_eq!(format.border, Some(&border("1")));
    assert_eq!(format.number_format, "#,##0.00");
}

fn row(s: Option<&str>, custom_format: Option<&str>) -> Row {
    let mut row = Row::default();
    row.s = s.map(str::to_owned);
    row.custom_format = custom_format.map(str::to_owned);
    row
}

/// One `cols` holding a `col` of columns 2 to 4 with style 5.
fn columns() -> Vec<ColumnInformation> {
    let mut column = ColumnWidthFormatting::default();
    column.min = Some("2".to_owned());
    column.max = Some("4".to_owned());
    column.style = Some("5".to_owned());
    let mut information = ColumnInformation::default();
    information.column_width_formatting.push(column);
    vec![information]
}

#[test]
fn a_cell_takes_its_own_index_then_its_row_s_then_its_column_s() {
    let columns = columns();
    let mut cell = Cell::default();
    cell.s = Some("3".to_owned());
    let custom = row(Some("2"), Some("1"));
    let plain = row(Some("2"), None);
    assert_eq!(CellFormats::index(&columns, &custom, Some(&cell), 3), 3);
    assert_eq!(CellFormats::index(&columns, &custom, None, 3), 2);
    assert_eq!(CellFormats::index(&columns, &plain, None, 3), 5);
    assert_eq!(CellFormats::index(&columns, &plain, None, 5), 0);
    assert_eq!(
        CellFormats::index(&columns, &row(None, Some("true")), None, 3),
        0
    );
}

#[test]
fn a_cell_node_that_states_no_index_has_the_schema_s_default_0() {
    let columns = columns();
    let custom = row(Some("2"), Some("1"));
    assert_eq!(
        CellFormats::index(&columns, &custom, Some(&Cell::default()), 3),
        0
    );
    assert_eq!(CellFormats::index(&columns, &custom, None, 3), 2);
    assert_eq!(CellFormats::index(&[], &row(None, None), None, 3), 0);
}

#[test]
fn the_normal_font_is_the_one_of_the_style_normal_else_the_first() {
    let mut sheet = styles(Vec::new());
    let mut normal = CellStyle::default();
    normal.name = Some("Normal".to_owned());
    normal.xf_id = Some(CellStyleFormatId::from("0".to_owned()));
    normal.builtin_id = Some("0".to_owned());
    let mut cell_styles = CellStyles::default();
    cell_styles.cell_style.push(normal);
    sheet.cell_styles = Some(Box::new(cell_styles));
    let mut records = FormattingRecords::default();
    records.format = vec![entry("1", "0", "0", "0")];
    sheet.formatting_records = Some(Box::new(records));
    assert_eq!(
        CellFormats::new(Some(&sheet)).normal_font(),
        Some(&font_named("Arial"))
    );
    sheet.cell_styles = None;
    assert_eq!(
        CellFormats::new(Some(&sheet)).normal_font(),
        Some(&font_named("Calibri"))
    );
    sheet.fonts = None;
    assert_eq!(CellFormats::new(Some(&sheet)).normal_font(), None);
}

/// The font `<b/><i val="0"/><u/><sz val="11"/><name val="Arial"/>`.
fn plain_members() -> Vec<FontContent> {
    let mut italic = Italic::default();
    italic.val = Some("0".to_owned());
    let mut size = FontSize::default();
    size.val = Some("11".to_owned());
    let mut name = FontName::default();
    name.val = Some("Arial".to_owned());
    vec![
        FontContent::from(Bold::default()),
        FontContent::from(italic),
        FontContent::from(Underline::default()),
        FontContent::from(size),
        FontContent::from(name),
    ]
}

#[test]
fn a_font_states_its_members_a_boolean_and_u_with_no_val_being_true_and_single() {
    let mut font = Font::default();
    font.content = plain_members();
    let properties = FontProperties::from(&font);
    assert_eq!(properties.bold, Some(true));
    assert_eq!(properties.italic, Some(false));
    assert_eq!(properties.underline, Some(UnderlineTypesMember::Single));
    assert_eq!(properties.size, Some("11"));
    assert_eq!(properties.name, Some("Arial"));
    assert_eq!(properties.strike, None);
}

#[test]
fn a_member_written_twice_takes_the_last_and_a_val_that_is_no_value_states_nothing() {
    let mut first = Bold::default();
    first.val = Some("1".to_owned());
    let mut last = Bold::default();
    last.val = Some("0".to_owned());
    let mut nonsense = Italic::default();
    nonsense.val = Some("maybe".to_owned());
    let mut underline = Underline::default();
    underline.val = Some(UnderlineTypes::from("wavy".to_owned()));
    let mut font = Font::default();
    font.content = vec![
        FontContent::from(first),
        FontContent::from(last),
        FontContent::from(nonsense),
        FontContent::from(underline),
    ];
    let properties = FontProperties::from(&font);
    assert_eq!(properties.bold, Some(false));
    assert_eq!(properties.italic, None);
    assert_eq!(properties.underline, None);
}

#[test]
fn a_run_s_properties_give_the_value_of_the_same_font() {
    let mut italic = Italic::default();
    italic.val = Some("0".to_owned());
    let mut size = FontSize::default();
    size.val = Some("11".to_owned());
    let mut name = FontRFont::default();
    name.val = Some("Arial".to_owned());
    let mut run = RunProperties::default();
    run.content = vec![
        RunPropertiesContent::from(Bold::default()),
        RunPropertiesContent::from(italic),
        RunPropertiesContent::from(Underline::default()),
        RunPropertiesContent::from(size),
        RunPropertiesContent::from(name),
    ];
    let mut font = Font::default();
    font.content = plain_members();
    assert_eq!(FontProperties::from(&run), FontProperties::from(&font));
}

#[test]
fn every_member_of_a_font_and_of_a_run_is_read_alike() {
    let mut charset = CharacterSet::default();
    charset.val = Some("1".to_owned());
    let mut color = DataBarColor::default();
    color.theme = Some("1".to_owned());
    let mut family = FontFamily::default();
    family.val = Some(StFontFamily::from("2".to_owned()));
    let mut run_family = FontFamilyIntProperty::default();
    run_family.val = Some("2".to_owned());
    let mut scheme = Scheme::default();
    scheme.val = Some(FontSchemeStyles::from("minor".to_owned()));
    let mut vertical = VerticalAlignment::default();
    vertical.val = Some(VerticalPositioningLocation::from("superscript".to_owned()));
    let mut font = Font::default();
    font.content = vec![
        FontContent::from(charset.clone()),
        FontContent::from(Condense::default()),
        FontContent::from(color.clone()),
        FontContent::from(Extend::default()),
        FontContent::from(family),
        FontContent::from(Outline::default()),
        FontContent::from(scheme.clone()),
        FontContent::from(Shadow::default()),
        FontContent::from(StrikeThrough::default()),
        FontContent::from(vertical.clone()),
    ];
    let mut run = RunProperties::default();
    run.content = vec![
        RunPropertiesContent::from(charset),
        RunPropertiesContent::from(Condense::default()),
        RunPropertiesContent::from(color.clone()),
        RunPropertiesContent::from(Extend::default()),
        RunPropertiesContent::from(run_family),
        RunPropertiesContent::from(Outline::default()),
        RunPropertiesContent::from(scheme),
        RunPropertiesContent::from(Shadow::default()),
        RunPropertiesContent::from(StrikeThrough::default()),
        RunPropertiesContent::from(vertical),
    ];
    let from_font = FontProperties::from(&font);
    assert_eq!(from_font, FontProperties::from(&run));
    assert_eq!(from_font.character_set, Some("1"));
    assert_eq!(from_font.color, Some(&color));
    assert_eq!(from_font.family, Some("2"));
    assert_eq!(from_font.scheme, Some(FontSchemeStylesMember::Minor));
    assert_eq!(
        from_font.vertical_alignment,
        Some(VerticalPositioningLocationMember::Superscript)
    );
    for stated in [
        from_font.condense,
        from_font.extend,
        from_font.outline,
        from_font.shadow,
        from_font.strike,
    ] {
        assert_eq!(stated, Some(true));
    }
}

#[test]
fn a_run_over_a_font_keeps_what_the_run_states_and_takes_the_rest_from_the_font() {
    let mut font = Font::default();
    font.content = plain_members();
    font.content.push(FontContent::from(Italic::default()));
    let lower = FontProperties::from(&font);
    let mut run = RunProperties::default();
    run.content
        .push(RunPropertiesContent::from(Bold::default()));
    let merged = FontProperties::from(&run).over(&lower);
    assert_eq!(merged.bold, Some(true));
    assert_eq!(merged.italic, Some(true));
    assert_eq!(merged.size, Some("11"));
    assert_eq!(merged.name, Some("Arial"));
    assert_eq!(FontProperties::default().over(&lower), lower);
}

#[test]
fn a_new_workbook_has_empty_cell_formats() {
    let xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    let formats = xlsx.cell_formats().unwrap();
    assert!(nothing(&formats.format(0)));
    assert_eq!(formats.normal_font(), None);
}
