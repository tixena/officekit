use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    Bold, BorderStyles, BorderStylesMember, ColorValue, ConditionalTableStyleFormattingTypes,
    ConditionalTableStyleFormattingTypesMember, DecimalNumberValue, DontWrapCellContent,
    FourDigitHexadecimalValue, LeftParagraphBorderTblWidth, MeasurementInEighthsOfAPoint,
    MeasurementOrPercentageValue, NumberOfRowsInRowBand, ParagraphBorderAboveIdenticalParagraphs,
    ParagraphBorderAboveIdenticalParagraphsTblWidth, ParagraphBorderBelowIdenticalParagraphs,
    ParagraphBorderBelowIdenticalParagraphsTblWidth, ParagraphShading, ParentStyleID,
    PreferredTableWidth, PreviousTableProperties, ReferencedTableStyle, RepeatTableRow,
    RunContentColor, RunProperties, RunPropertiesContent, ShadingPatterns, ShadingPatternsMember,
    SingleTableCellMargins, StyleConditionalTableFormattingProperties, StyleDefinition,
    StyleDefinitions, StyleTypes, StyleTypesMember, TableBorders, TableCellLeadingEdgeBorder,
    TableCellLeadingEdgeBorderTblWidth, TableCellMarginExceptions, TableCellProperties,
    TableCellTrailingMarginException, TableCellTrailingMarginExceptionTblWidth,
    TableInsideHorizontalEdgesBorder, TableInsideVerticalEdgesBorder, TableLevelPropertyExceptions,
    TableProperties, TableRowCannotBreakAcrossPages, TableRowCellSpacing, TableRowProperties,
    TableRowPropertiesContent, TableStyleConditionalFormattingSettingsException, TableWidthUnits,
    TableWidthUnitsMember, VisuallyRightToLeftTable,
};

use super::CellPosition;
use crate::docx::effective::StyleSheet;
use crate::docx::table_style::TableFormat;

use ConditionalTableStyleFormattingTypesMember::{
    Band1Horz, Band1Vert, Band2Horz, Band2Vert, FirstRow, LastCol, LastRow, NwCell, WholeTable,
};

macro_rules! border {
    ($kind:ident, $style:expr, $size:literal, $color:literal) => {{
        let mut edge = $kind::default();
        edge.val = Some(BorderStyles::from($style));
        edge.sz = Some(MeasurementInEighthsOfAPoint::from($size.to_owned()));
        edge.color = Some(ColorValue::from($color.to_owned()));
        Some(Box::new(edge))
    }};
}

macro_rules! width {
    ($kind:ident, $twips:literal) => {{
        let mut edge = $kind::default();
        edge.w = Some(MeasurementOrPercentageValue::from($twips.to_owned()));
        edge.r#type = Some(TableWidthUnits::from(TableWidthUnitsMember::Dxa));
        Some(Box::new(edge))
    }};
}

fn position(row: usize, rows: usize, column: usize, columns: usize) -> CellPosition {
    CellPosition::new(row, rows, column, columns, false)
}

fn shaded_cell(fill: &str) -> TableCellProperties {
    let mut shading = ParagraphShading::default();
    shading.val = Some(ShadingPatterns::from(ShadingPatternsMember::Clear));
    shading.fill = Some(ColorValue::from(fill.to_owned()));
    let mut cell = TableCellProperties::default();
    cell.paragraph_shading = Some(Box::new(shading));
    cell
}

fn run_of(content: Vec<RunPropertiesContent>) -> RunProperties {
    let mut run = RunProperties::default();
    run.content = content;
    run
}

fn bold_run() -> RunProperties {
    run_of(alloc::vec![RunPropertiesContent::Bold(Bold::default())])
}

fn colored_run(color: &str) -> RunProperties {
    let mut found = RunContentColor::default();
    found.val = Some(ColorValue::from(color.to_owned()));
    run_of(alloc::vec![RunPropertiesContent::RunContentColor(found)])
}

/// A conditional format of type `kind` with the cell properties `cell` and the run properties
/// `run`.
fn conditional(
    kind: ConditionalTableStyleFormattingTypesMember,
    cell: Option<TableCellProperties>,
    run: Option<RunProperties>,
) -> StyleConditionalTableFormattingProperties {
    let mut format = StyleConditionalTableFormattingProperties::default();
    format.r#type = Some(ConditionalTableStyleFormattingTypes::from(kind));
    format.table_cell_properties = cell.map(Box::new);
    format.run_properties = run.map(Box::new);
    format
}

fn shaded(
    kind: ConditionalTableStyleFormattingTypesMember,
    fill: &str,
) -> StyleConditionalTableFormattingProperties {
    conditional(kind, Some(shaded_cell(fill)), None)
}

/// A table style `id` with the whole-table properties `table` and the conditional formats
/// `formats`.
fn table_style(
    id: &str,
    table: Option<PreviousTableProperties>,
    formats: Vec<StyleConditionalTableFormattingProperties>,
) -> StyleDefinition {
    let mut style = StyleDefinition::default();
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(StyleTypesMember::Table));
    style.previous_table_properties = table.map(Box::new);
    style.style_conditional_table_formatting_properties = formats;
    style
}

fn sheet_of(styles: Vec<StyleDefinition>) -> StyleSheet {
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = styles;
    StyleSheet::new(&definitions)
}

/// A table's own properties naming `style`, with the `w:tblLook` `look`.
fn own_table(
    style: Option<&str>,
    look: Option<TableStyleConditionalFormattingSettingsException>,
) -> TableProperties {
    let mut own = TableProperties::default();
    own.referenced_table_style = style.map(|id| {
        let mut reference = ReferencedTableStyle::default();
        reference.val = Some(id.to_owned());
        Box::new(reference)
    });
    own.table_style_conditional_formatting_settings_exception = look.map(Box::new);
    own
}

/// A `w:tblLook` with `w:firstRow`, and `w:firstColumn` when `first_column`.
fn first_row_look(
    first_row: bool,
    first_column: bool,
) -> TableStyleConditionalFormattingSettingsException {
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.first_row = Some(first_row.into());
    look.first_column = first_column.then(|| true.into());
    look
}

fn fill(cell: &TableCellProperties) -> Option<&str> {
    cell.paragraph_shading
        .as_deref()
        .and_then(|shading| shading.fill.as_ref())
        .map(|value| value.0.as_str())
}

fn bold(run: &RunProperties) -> Option<bool> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::Bold(found) = property {
            Some(
                found
                    .val
                    .as_ref()
                    .is_none_or(|value| value.value().unwrap_or(true)),
            )
        } else {
            None
        }
    })
}

fn color(run: &RunProperties) -> Option<&str> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::RunContentColor(found) = property {
            found.val.as_ref().map(|value| value.0.as_str())
        } else {
            None
        }
    })
}

/// The effective run properties of a run with no properties of its own, in a paragraph with no
/// properties of its own, in the cell at `at`.
fn run_at(sheet: &StyleSheet, format: &TableFormat, at: CellPosition) -> RunProperties {
    let (paragraph, _) = sheet.paragraph_in_cell(format, at, None, None);
    sheet.run(&paragraph, None)
}

/// The whole-table properties of the example of section 17.7.6: single borders all around and
/// inside, cell margins of 108 at the start and end and of 0 at the top and bottom.
fn example_table_properties() -> PreviousTableProperties {
    let mut borders = TableBorders::default();
    borders.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    borders.table_cell_leading_edge_border = border!(
        TableCellLeadingEdgeBorder,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    borders.paragraph_border_below_identical_paragraphs = border!(
        ParagraphBorderBelowIdenticalParagraphs,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    borders.table_cell_trailing_margin_exception = border!(
        TableCellTrailingMarginException,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    borders.table_inside_horizontal_edges_border = border!(
        TableInsideHorizontalEdgesBorder,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    borders.table_inside_vertical_edges_border = border!(
        TableInsideVerticalEdgesBorder,
        BorderStylesMember::Single,
        "4",
        "auto"
    );
    let mut margins = TableCellMarginExceptions::default();
    margins.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "0");
    margins.table_cell_leading_edge_border_tbl_width =
        width!(TableCellLeadingEdgeBorderTblWidth, "108");
    margins.paragraph_border_below_identical_paragraphs_tbl_width =
        width!(ParagraphBorderBelowIdenticalParagraphsTblWidth, "0");
    margins.table_cell_trailing_margin_exception_tbl_width =
        width!(TableCellTrailingMarginExceptionTblWidth, "108");
    let mut table = PreviousTableProperties::default();
    table.table_borders = Some(Box::new(borders));
    table.table_cell_margin_exceptions = Some(Box::new(margins));
    table
}

/// The style of the example of section 17.7.6: its whole-table properties and a first-row
/// format with bold runs and a red shading.
fn example_sheet() -> StyleSheet {
    let first_row = conditional(FirstRow, Some(shaded_cell("ED1C24")), Some(bold_run()));
    sheet_of(alloc::vec![table_style(
        "TestTableStyle",
        Some(example_table_properties()),
        alloc::vec![first_row],
    )])
}

#[test]
fn the_example_of_the_standard_shades_and_bolds_the_first_row() {
    let sheet = example_sheet();
    let format = sheet.table(&own_table(
        Some("TestTableStyle"),
        Some(first_row_look(true, false)),
    ));
    for column in 0..2 {
        let top = position(0, 3, column, 2);
        assert_eq!(fill(&format.cell(top, None)), Some("ED1C24"));
        assert_eq!(bold(&run_at(&sheet, &format, top)), Some(true));
        let next = position(1, 3, column, 2);
        assert_eq!(fill(&format.cell(next, None)), None);
        assert_eq!(bold(&run_at(&sheet, &format, next)), None);
    }
    let properties = format.properties();
    assert_eq!(
        properties.table_borders.as_deref(),
        example_table_properties().table_borders.as_deref()
    );
    assert_eq!(
        properties.table_cell_margin_exceptions.as_deref(),
        example_table_properties()
            .table_cell_margin_exceptions
            .as_deref()
    );
}

#[test]
fn first_row_off_or_no_look_takes_the_first_row_format_away() {
    let sheet = example_sheet();
    let off = sheet.table(&own_table(
        Some("TestTableStyle"),
        Some(first_row_look(false, false)),
    ));
    let unlooked = sheet.table(&own_table(Some("TestTableStyle"), None));
    for format in [off, unlooked] {
        let top = position(0, 3, 0, 2);
        assert_eq!(fill(&format.cell(top, None)), None);
        assert_eq!(bold(&run_at(&sheet, &format, top)), None);
    }
}

#[test]
fn a_cell_s_own_margin_wins_over_the_style_s() {
    let mut margins = SingleTableCellMargins::default();
    margins.left_paragraph_border_tbl_width = width!(LeftParagraphBorderTblWidth, "108");
    margins.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "0");
    let mut style_cell = TableCellProperties::default();
    style_cell.single_table_cell_margins = Some(Box::new(margins));
    let mut style = table_style("Margins", None, Vec::new());
    style.table_cell_properties = Some(Box::new(style_cell));
    let sheet = sheet_of(alloc::vec![style]);

    let mut own_margins = SingleTableCellMargins::default();
    own_margins.left_paragraph_border_tbl_width = width!(LeftParagraphBorderTblWidth, "200");
    let mut own = TableCellProperties::default();
    own.single_table_cell_margins = Some(Box::new(own_margins));

    let format = sheet.table(&own_table(Some("Margins"), None));
    let cell = format.cell(position(0, 1, 0, 1), Some(&own));
    let found = cell.single_table_cell_margins.unwrap();
    let left = found.left_paragraph_border_tbl_width.unwrap();
    let top = found
        .paragraph_border_above_identical_paragraphs_tbl_width
        .unwrap();
    assert_eq!(left.w.map(|value| value.0), Some("200".to_owned()));
    assert_eq!(top.w.map(|value| value.0), Some("0".to_owned()));
    assert_eq!(cell.unknown.children.len(), 0);
}

fn banded_sheet(rows_in_band: Option<i64>) -> StyleSheet {
    let table = rows_in_band.map(|size| {
        let mut band = NumberOfRowsInRowBand::default();
        band.val = Some(DecimalNumberValue::from(size.to_string()));
        let mut properties = PreviousTableProperties::default();
        properties.number_of_rows_in_row_band = Some(Box::new(band));
        properties
    });
    sheet_of(alloc::vec![table_style(
        "Banded",
        table,
        alloc::vec![shaded(Band1Horz, "AAAAAA"), shaded(Band2Horz, "BBBBBB")],
    )])
}

fn row_fills(format: &TableFormat) -> Vec<Option<String>> {
    (0..4)
        .map(|row| fill(&format.cell(position(row, 4, 0, 1), None)).map(str::to_owned))
        .collect()
}

#[test]
fn row_bands_alternate_by_the_band_size() {
    let first = Some("AAAAAA".to_owned());
    let second = Some("BBBBBB".to_owned());
    let single = banded_sheet(None).table(&own_table(Some("Banded"), None));
    assert_eq!(
        row_fills(&single),
        [first.clone(), second.clone(), first.clone(), second.clone()]
    );
    let double = banded_sheet(Some(2)).table(&own_table(Some("Banded"), None));
    assert_eq!(
        row_fills(&double),
        [first.clone(), first, second.clone(), second]
    );
}

#[test]
fn no_h_band_takes_the_row_bands_away() {
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.no_h_band = Some(true.into());
    let format = banded_sheet(None).table(&own_table(Some("Banded"), Some(look)));
    assert_eq!(row_fills(&format), [None, None, None, None]);
}

#[test]
fn the_val_bitmask_states_the_settings_its_attributes_leave_out() {
    let mut masked = TableStyleConditionalFormattingSettingsException::default();
    masked.val = Some(FourDigitHexadecimalValue::from("0200".to_owned()));
    let format = banded_sheet(None).table(&own_table(Some("Banded"), Some(masked.clone())));
    assert_eq!(row_fills(&format), [None, None, None, None]);

    masked.no_h_band = Some(false.into());
    let stated = banded_sheet(None).table(&own_table(Some("Banded"), Some(masked)));
    assert_eq!(row_fills(&stated)[0].as_deref(), Some("AAAAAA"));

    let mut first_row = TableStyleConditionalFormattingSettingsException::default();
    first_row.val = Some(FourDigitHexadecimalValue::from("0020".to_owned()));
    let sheet = example_sheet();
    let looked = sheet.table(&own_table(Some("TestTableStyle"), Some(first_row)));
    assert_eq!(
        fill(&looked.cell(position(0, 3, 0, 2), None)),
        Some("ED1C24")
    );
}

#[test]
fn later_formats_win_over_earlier_ones() {
    let sheet = sheet_of(alloc::vec![table_style(
        "Ordered",
        None,
        alloc::vec![
            shaded(NwCell, "CCCCCC"),
            shaded(FirstRow, "BBBBBB"),
            shaded(Band1Horz, "AAAAAA"),
        ],
    )]);
    let format = sheet.table(&own_table(
        Some("Ordered"),
        Some(first_row_look(true, true)),
    ));
    let corner = position(0, 3, 0, 2);
    assert_eq!(
        format.formats(corner),
        [WholeTable, Band1Horz, FirstRow, NwCell]
    );
    assert_eq!(fill(&format.cell(corner, None)), Some("CCCCCC"));
    assert_eq!(
        fill(&format.cell(position(0, 3, 1, 2), None)),
        Some("BBBBBB")
    );
}

#[test]
fn a_header_row_takes_the_first_row_format() {
    let sheet = example_sheet();
    let format = sheet.table(&own_table(
        Some("TestTableStyle"),
        Some(first_row_look(true, false)),
    ));
    let header = CellPosition::new(1, 3, 0, 2, true);
    assert_eq!(fill(&format.cell(header, None)), Some("ED1C24"));
    assert_eq!(bold(&run_at(&sheet, &format, header)), Some(true));
    assert_eq!(fill(&format.cell(position(2, 3, 0, 2), None)), None);
}

#[test]
fn the_last_column_wins_over_the_last_row() {
    let sheet = sheet_of(alloc::vec![table_style(
        "Last",
        None,
        alloc::vec![shaded(LastCol, "DDDDDD"), shaded(LastRow, "EEEEEE")],
    )]);
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.last_row = Some(true.into());
    look.last_column = Some(true.into());
    let format = sheet.table(&own_table(Some("Last"), Some(look)));
    let corner = position(2, 3, 1, 2);
    assert_eq!(format.formats(corner), [WholeTable, LastRow, LastCol]);
    assert_eq!(fill(&format.cell(corner, None)), Some("DDDDDD"));
    assert_eq!(
        fill(&format.cell(position(2, 3, 0, 2), None)),
        Some("EEEEEE")
    );
}

#[test]
fn a_style_takes_what_its_parent_states() {
    let mut parent_table = PreviousTableProperties::default();
    parent_table.table_borders = example_table_properties().table_borders;
    let parent = table_style(
        "Parent",
        Some(parent_table),
        alloc::vec![conditional(FirstRow, None, Some(bold_run()))],
    );
    let mut child = table_style("Child", None, Vec::new());
    let mut based_on = ParentStyleID::default();
    based_on.val = Some("Parent".to_owned());
    child.parent_style_id = Some(Box::new(based_on));
    child.run_properties = Some(Box::new(colored_run("00FF00")));
    let sheet = sheet_of(alloc::vec![parent, child]);

    let format = sheet.table(&own_table(Some("Child"), Some(first_row_look(true, false))));
    let run = run_at(&sheet, &format, position(0, 2, 0, 1));
    assert_eq!(bold(&run), Some(true));
    assert_eq!(color(&run), Some("00FF00"));
    assert_eq!(
        format.properties().table_borders.as_deref(),
        example_table_properties().table_borders.as_deref()
    );
}

#[test]
fn the_table_s_own_border_wins_over_the_style_s() {
    let sheet = sheet_of(alloc::vec![table_style(
        "Bordered",
        Some(example_table_properties()),
        Vec::new(),
    )]);
    let mut own_borders = TableBorders::default();
    own_borders.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::Double,
        "12",
        "FF0000"
    );
    let mut own = own_table(Some("Bordered"), None);
    own.table_borders = Some(Box::new(own_borders.clone()));

    let borders = sheet.table(&own).properties().table_borders.unwrap();
    assert_eq!(
        borders.paragraph_border_above_identical_paragraphs,
        own_borders.paragraph_border_above_identical_paragraphs
    );
    let style_borders = example_table_properties().table_borders.unwrap();
    assert_eq!(
        borders.table_cell_leading_edge_border,
        style_borders.table_cell_leading_edge_border
    );
    assert_eq!(
        borders.table_inside_horizontal_edges_border,
        style_borders.table_inside_horizontal_edges_border
    );
    assert_eq!(borders.unknown.children.len(), 0);
}

#[test]
fn a_table_with_no_style_keeps_its_own_properties() {
    let mut own = own_table(None, None);
    own.table_borders = example_table_properties().table_borders;
    let mut own_row = TableRowProperties::default();
    own_row
        .content
        .push(TableRowPropertiesContent::TableRowCannotBreakAcrossPages(
            TableRowCannotBreakAcrossPages::default(),
        ));
    let own_cell = shaded_cell("123456");
    let at = position(0, 3, 0, 2);

    let unstyled = sheet_of(Vec::new()).table(&own);
    assert_eq!(unstyled.properties(), own);
    assert_eq!(unstyled.row(at, Some(&own_row)), own_row);
    assert_eq!(unstyled.cell(at, Some(&own_cell)), own_cell);
    assert_eq!(unstyled.formats(at), [WholeTable]);

    let mut default = table_style("Default", Some(example_table_properties()), Vec::new());
    default.default = Some(true.into());
    let mut missing = own;
    missing.referenced_table_style = own_table(Some("Gone"), None).referenced_table_style;
    let named = sheet_of(alloc::vec![default]).table(&missing);
    assert_eq!(named.properties(), missing);
    assert_eq!(named.row(at, Some(&own_row)), own_row);
    assert_eq!(named.cell(at, Some(&own_cell)), own_cell);
    assert_eq!(named.formats(at), [WholeTable]);
}

#[test]
fn a_table_that_names_no_style_takes_the_default_table_style() {
    let mut default = table_style("Default", Some(example_table_properties()), Vec::new());
    default.default = Some(true.into());
    let sheet = sheet_of(alloc::vec![default]);
    let properties = sheet.table(&own_table(None, None)).properties();
    assert_eq!(
        properties.table_cell_margin_exceptions.as_deref(),
        example_table_properties()
            .table_cell_margin_exceptions
            .as_deref()
    );
}

#[test]
fn a_row_takes_the_formats_of_its_row_only() {
    let mut first_row = conditional(FirstRow, None, None);
    let mut row = TableRowProperties::default();
    row.content
        .push(TableRowPropertiesContent::TableRowCannotBreakAcrossPages(
            TableRowCannotBreakAcrossPages::default(),
        ));
    first_row.table_row_properties = Some(Box::new(row.clone()));
    let mut first_column = conditional(
        ConditionalTableStyleFormattingTypesMember::FirstCol,
        None,
        None,
    );
    first_column.table_row_properties = Some(Box::new(row.clone()));
    let sheet = sheet_of(alloc::vec![table_style(
        "Rows",
        None,
        alloc::vec![first_row, first_column],
    )]);
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.first_column = Some(true.into());
    let format = sheet.table(&own_table(Some("Rows"), Some(look)));
    assert_eq!(
        format.row(position(0, 2, 0, 1), None),
        TableRowProperties::default()
    );
    let both = sheet.table(&own_table(Some("Rows"), Some(first_row_look(true, true))));
    assert_eq!(both.row(position(0, 2, 0, 1), None), row);
}

#[test]
fn column_bands_alternate_unless_no_v_band() {
    let sheet = sheet_of(alloc::vec![table_style(
        "Columns",
        None,
        alloc::vec![shaded(Band1Vert, "AAAAAA"), shaded(Band2Vert, "BBBBBB")],
    )]);
    let banded = sheet.table(&own_table(Some("Columns"), None));
    let found: Vec<_> = (0..3)
        .map(|column| banded.formats(position(0, 1, column, 3)))
        .collect();
    assert_eq!(
        found,
        [
            [WholeTable, Band1Vert],
            [WholeTable, Band2Vert],
            [WholeTable, Band1Vert]
        ]
    );
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.no_v_band = Some(true.into());
    let unbanded = sheet.table(&own_table(Some("Columns"), Some(look)));
    assert_eq!(unbanded.formats(position(0, 1, 1, 3)), [WholeTable]);
}

#[test]
fn the_last_default_table_style_is_the_one() {
    let mut first = table_style("First", None, Vec::new());
    first.default = Some(true.into());
    let mut last = table_style("Last", Some(example_table_properties()), Vec::new());
    last.default = Some(true.into());
    let sheet = sheet_of(alloc::vec![first, last]);
    assert_eq!(
        sheet
            .table(&own_table(None, None))
            .properties()
            .table_borders,
        example_table_properties().table_borders
    );
}

/// The `w:sz` of a table's top border, when stated.
fn top_size(table: &TableProperties) -> Option<String> {
    table
        .table_borders
        .as_deref()
        .and_then(|borders| {
            borders
                .paragraph_border_above_identical_paragraphs
                .as_deref()
        })
        .and_then(|top| top.sz.as_ref())
        .map(|size| size.0.clone())
}

#[test]
fn a_row_with_no_exceptions_sees_the_table_s_properties() {
    let sheet = example_sheet();
    let format = sheet.table(&own_table(
        Some("TestTableStyle"),
        Some(first_row_look(true, false)),
    ));
    let empty = TableLevelPropertyExceptions::default();
    assert_eq!(format.properties_for_row(None), format.properties());
    assert_eq!(format.properties_for_row(Some(&empty)), format.properties());
    let top = position(0, 3, 0, 2);
    for row in [format.for_row(None), format.for_row(Some(&empty))] {
        assert_eq!(row.formats(top), format.formats(top));
        assert_eq!(row.cell(top, None), format.cell(top, None));
        assert_eq!(row.row(top, None), format.row(top, None));
    }
}

#[test]
fn a_row_s_exception_borders_replace_the_table_s() {
    let mut borders = TableBorders::default();
    borders.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::Single,
        "4",
        "000000"
    );
    borders.table_inside_horizontal_edges_border = border!(
        TableInsideHorizontalEdgesBorder,
        BorderStylesMember::Single,
        "4",
        "000000"
    );
    let mut own = own_table(None, None);
    own.table_borders = Some(Box::new(borders));
    let format = sheet_of(Vec::new()).table(&own);

    let mut none = TableBorders::default();
    none.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::None,
        "0",
        "auto"
    );
    none.table_inside_horizontal_edges_border = border!(
        TableInsideHorizontalEdgesBorder,
        BorderStylesMember::None,
        "0",
        "auto"
    );
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_borders = Some(Box::new(none.clone()));
    let row = format.properties_for_row(Some(&exceptions));
    assert_eq!(row.table_borders.as_deref(), Some(&none));
}

#[test]
fn a_row_s_exception_margin_replaces_that_margin_only() {
    let mut margins = TableCellMarginExceptions::default();
    margins.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "0");
    margins.table_cell_leading_edge_border_tbl_width =
        width!(TableCellLeadingEdgeBorderTblWidth, "108");
    let mut own = own_table(None, None);
    own.table_cell_margin_exceptions = Some(Box::new(margins));
    own.preferred_table_width = width!(PreferredTableWidth, "5000");
    let format = sheet_of(Vec::new()).table(&own);

    let mut over = TableCellMarginExceptions::default();
    over.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "288");
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_cell_margin_exceptions = Some(Box::new(over));
    let row = format.properties_for_row(Some(&exceptions));

    let found = row.table_cell_margin_exceptions.unwrap();
    let top = found
        .paragraph_border_above_identical_paragraphs_tbl_width
        .unwrap();
    let start = found.table_cell_leading_edge_border_tbl_width.unwrap();
    assert_eq!(top.w.map(|value| value.0), Some("288".to_owned()));
    assert_eq!(start.w.map(|value| value.0), Some("108".to_owned()));
    assert_eq!(row.preferred_table_width, own.preferred_table_width);
    assert_eq!(row.table_borders, format.properties().table_borders);
}

#[test]
fn a_row_s_exceptions_lie_over_the_table_s_own_properties_over_the_style_s() {
    let sheet = example_sheet();
    let mut own_borders = TableBorders::default();
    own_borders.paragraph_border_below_identical_paragraphs = border!(
        ParagraphBorderBelowIdenticalParagraphs,
        BorderStylesMember::Double,
        "8",
        "auto"
    );
    let mut own = own_table(Some("TestTableStyle"), None);
    own.table_borders = Some(Box::new(own_borders));
    let format = sheet.table(&own);

    let mut over = TableBorders::default();
    over.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::ThinThickThinMediumGap,
        "24",
        "auto"
    );
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_borders = Some(Box::new(over));
    let row = format.properties_for_row(Some(&exceptions));
    let borders = row.table_borders.as_deref().unwrap();

    assert_eq!(top_size(&row), Some("24".to_owned()));
    assert_eq!(top_size(&format.properties()), Some("4".to_owned()));
    let bottom = borders
        .paragraph_border_below_identical_paragraphs
        .as_deref()
        .unwrap();
    assert_eq!(bottom.sz.as_ref().map(|size| size.0.as_str()), Some("8"));
    let inside = borders
        .table_inside_horizontal_edges_border
        .as_deref()
        .unwrap();
    assert_eq!(inside.sz.as_ref().map(|size| size.0.as_str()), Some("4"));
}

#[test]
fn a_row_s_exception_look_turns_the_first_row_format_off_for_its_cells() {
    let sheet = example_sheet();
    let format = sheet.table(&own_table(
        Some("TestTableStyle"),
        Some(first_row_look(true, false)),
    ));
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_style_conditional_formatting_settings_exception =
        Some(Box::new(first_row_look(false, false)));
    let row = format.for_row(Some(&exceptions));
    for column in 0..2 {
        let top = position(0, 3, column, 2);
        assert!(!row.formats(top).contains(&FirstRow));
        assert_eq!(fill(&row.cell(top, None)), None);
        assert_eq!(bold(&run_at(&sheet, &row, top)), None);
        assert_eq!(fill(&format.cell(top, None)), Some("ED1C24"));
    }
    assert_eq!(
        row.properties()
            .table_style_conditional_formatting_settings_exception
            .as_deref(),
        Some(&first_row_look(false, false))
    );
}

#[test]
fn a_row_s_exception_look_turns_the_first_row_format_on_where_the_table_has_it_off() {
    let sheet = example_sheet();
    let format = sheet.table(&own_table(Some("TestTableStyle"), None));
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_style_conditional_formatting_settings_exception =
        Some(Box::new(first_row_look(true, false)));
    let row = format.for_row(Some(&exceptions));
    let top = position(0, 3, 0, 2);
    assert!(row.formats(top).contains(&FirstRow));
    assert_eq!(fill(&row.cell(top, None)), Some("ED1C24"));
    assert_eq!(bold(&run_at(&sheet, &row, top)), Some(true));
    assert_eq!(fill(&format.cell(top, None)), None);
}

/// Whether an on/off property with `val` is on: an absent `w:val` is on (section 17.17.4).
fn on(val: Option<&OnOffValue>) -> bool {
    val.is_none_or(|value| value.value().unwrap_or(true))
}

fn row_with(property: TableRowPropertiesContent) -> TableRowProperties {
    let mut row = TableRowProperties::default();
    row.content.push(property);
    row
}

/// A sheet with the table style `OnOff`, whose own row properties are `row`.
fn row_style_sheet(row: TableRowProperties) -> StyleSheet {
    let mut style = table_style("OnOff", None, Vec::new());
    style.table_row_properties = Some(Box::new(row));
    sheet_of(alloc::vec![style])
}

#[test]
fn a_row_s_bare_cant_split_turns_on_the_style_s_cant_split_that_is_off() {
    let mut off = TableRowCannotBreakAcrossPages::default();
    off.val = Some(false.into());
    let sheet = row_style_sheet(row_with(
        TableRowPropertiesContent::TableRowCannotBreakAcrossPages(off),
    ));
    let own = row_with(TableRowPropertiesContent::TableRowCannotBreakAcrossPages(
        TableRowCannotBreakAcrossPages::default(),
    ));
    let row = sheet
        .table(&own_table(Some("OnOff"), None))
        .row(position(0, 1, 0, 1), Some(&own));
    let found = row.content.iter().find_map(|property| {
        if let TableRowPropertiesContent::TableRowCannotBreakAcrossPages(found) = property {
            Some(on(found.val.as_ref()))
        } else {
            None
        }
    });
    assert_eq!(found, Some(true));
}

#[test]
fn a_row_s_header_that_is_off_turns_off_the_style_s_bare_header() {
    let sheet = row_style_sheet(row_with(TableRowPropertiesContent::RepeatTableRow(
        RepeatTableRow::default(),
    )));
    let mut off = RepeatTableRow::default();
    off.val = Some(false.into());
    let own = row_with(TableRowPropertiesContent::RepeatTableRow(off));
    let row = sheet
        .table(&own_table(Some("OnOff"), None))
        .row(position(0, 1, 0, 1), Some(&own));
    let found = row.content.iter().find_map(|property| {
        if let TableRowPropertiesContent::RepeatTableRow(found) = property {
            Some(on(found.val.as_ref()))
        } else {
            None
        }
    });
    assert_eq!(found, Some(false));
}

#[test]
fn a_table_s_bare_bidi_visual_turns_on_the_style_s_bidi_visual_that_is_off() {
    let mut off = VisuallyRightToLeftTable::default();
    off.val = Some(false.into());
    let mut style_table = PreviousTableProperties::default();
    style_table.visually_right_to_left_table = Some(Box::new(off));
    let sheet = sheet_of(alloc::vec![table_style(
        "OnOff",
        Some(style_table),
        Vec::new()
    )]);
    let mut own = own_table(Some("OnOff"), None);
    own.visually_right_to_left_table = Some(Box::default());
    let table = sheet.table(&own).properties();
    let found = table
        .visually_right_to_left_table
        .as_deref()
        .map(|found| on(found.val.as_ref()));
    assert_eq!(found, Some(true));
}

#[test]
fn a_cell_s_bare_no_wrap_turns_on_the_style_s_no_wrap_that_is_off() {
    let mut off = DontWrapCellContent::default();
    off.val = Some(false.into());
    let mut style_cell = TableCellProperties::default();
    style_cell.dont_wrap_cell_content = Some(Box::new(off));
    let mut style = table_style("OnOff", None, Vec::new());
    style.table_cell_properties = Some(Box::new(style_cell));
    let sheet = sheet_of(alloc::vec![style]);
    let mut own = TableCellProperties::default();
    own.dont_wrap_cell_content = Some(Box::default());
    let cell = sheet
        .table(&own_table(Some("OnOff"), None))
        .cell(position(0, 1, 0, 1), Some(&own));
    let found = cell
        .dont_wrap_cell_content
        .as_deref()
        .map(|found| on(found.val.as_ref()));
    assert_eq!(found, Some(true));
}

/// A table style `Spaced` with the whole-table `w:tblPr` `whole` and a first-row format whose
/// `w:tblPr` is `first_row`.
fn spaced_sheet(
    whole: Option<PreviousTableProperties>,
    first_row: PreviousTableProperties,
) -> StyleSheet {
    let mut format = conditional(FirstRow, None, None);
    format.previous_table_properties = Some(Box::new(first_row));
    sheet_of(alloc::vec![table_style(
        "Spaced",
        whole,
        alloc::vec![format]
    )])
}

/// A `w:tblPr` with the `w:tblCellSpacing` `twips`, as in the examples of sections 17.7.6.3 and
/// 17.7.6.4.
fn spacing_of(twips: &str) -> PreviousTableProperties {
    let mut table = PreviousTableProperties::default();
    let mut spacing = TableRowCellSpacing::default();
    spacing.w = Some(MeasurementOrPercentageValue::from(twips.to_owned()));
    spacing.r#type = Some(TableWidthUnits::from(TableWidthUnitsMember::Dxa));
    table.table_row_cell_spacing = Some(Box::new(spacing));
    table
}

fn spacing(table: &TableProperties) -> Option<&str> {
    table
        .table_row_cell_spacing
        .as_deref()
        .and_then(|found| found.w.as_ref())
        .map(|value| value.0.as_str())
}

#[test]
fn the_first_row_format_s_cell_spacing_holds_in_the_first_row_only() {
    let sheet = spaced_sheet(Some(spacing_of("15")), spacing_of("29"));
    let format = sheet.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(true, false)),
    ));
    assert_eq!(
        spacing(&format.properties_at(position(0, 3, 0, 2))),
        Some("29")
    );
    assert_eq!(
        spacing(&format.properties_at(position(0, 3, 1, 2))),
        Some("29")
    );
    assert_eq!(
        spacing(&format.properties_at(position(1, 3, 0, 2))),
        Some("15")
    );
    assert_eq!(spacing(&format.properties()), Some("15"));

    let unspaced = spaced_sheet(None, spacing_of("29"));
    let without = unspaced.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(true, false)),
    ));
    assert_eq!(
        spacing(&without.properties_at(position(0, 3, 0, 2))),
        Some("29")
    );
    assert_eq!(spacing(&without.properties_at(position(1, 3, 0, 2))), None);
    let header = CellPosition::new(1, 3, 0, 2, true);
    assert_eq!(spacing(&without.properties_at(header)), Some("29"));
}

#[test]
fn first_row_off_leaves_the_whole_table_s_cell_spacing_in_the_first_row() {
    let sheet = spaced_sheet(Some(spacing_of("15")), spacing_of("29"));
    let format = sheet.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(false, false)),
    ));
    assert_eq!(
        spacing(&format.properties_at(position(0, 3, 0, 2))),
        Some("15")
    );
}

#[test]
fn the_table_s_own_cell_spacing_wins_over_the_first_row_format_s() {
    let sheet = spaced_sheet(Some(spacing_of("15")), spacing_of("29"));
    let mut own = own_table(Some("Spaced"), Some(first_row_look(true, false)));
    own.table_row_cell_spacing = spacing_of("50").table_row_cell_spacing;
    let format = sheet.table(&own);
    for row in 0..3 {
        assert_eq!(
            spacing(&format.properties_at(position(row, 3, 0, 2))),
            Some("50")
        );
    }
}

#[test]
fn a_first_row_format_s_bottom_border_replaces_that_edge_only() {
    let mut bottom = TableBorders::default();
    bottom.paragraph_border_below_identical_paragraphs = border!(
        ParagraphBorderBelowIdenticalParagraphs,
        BorderStylesMember::Double,
        "12",
        "FF0000"
    );
    let mut first_row = PreviousTableProperties::default();
    first_row.table_borders = Some(Box::new(bottom.clone()));
    let sheet = spaced_sheet(Some(example_table_properties()), first_row);
    let format = sheet.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(true, false)),
    ));

    let whole = example_table_properties().table_borders.unwrap();
    let mut expected = (*whole).clone();
    expected.paragraph_border_below_identical_paragraphs =
        bottom.paragraph_border_below_identical_paragraphs;
    let top = format.properties_at(position(0, 3, 0, 2));
    assert_eq!(top.table_borders.as_deref(), Some(&expected));
    let next = format.properties_at(position(1, 3, 0, 2));
    assert_eq!(next.table_borders.as_deref(), Some(&*whole));
}

#[test]
fn the_table_s_own_border_and_margin_win_over_the_format_s_edge_by_edge() {
    let mut format_borders = TableBorders::default();
    format_borders.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::Double,
        "12",
        "FF0000"
    );
    format_borders.paragraph_border_below_identical_paragraphs = border!(
        ParagraphBorderBelowIdenticalParagraphs,
        BorderStylesMember::Double,
        "12",
        "FF0000"
    );
    let mut format_margins = TableCellMarginExceptions::default();
    format_margins.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "72");
    format_margins.table_cell_leading_edge_border_tbl_width =
        width!(TableCellLeadingEdgeBorderTblWidth, "144");
    let mut first_row = PreviousTableProperties::default();
    first_row.table_borders = Some(Box::new(format_borders.clone()));
    first_row.table_cell_margin_exceptions = Some(Box::new(format_margins.clone()));
    let sheet = spaced_sheet(Some(example_table_properties()), first_row);

    let mut own_borders = TableBorders::default();
    own_borders.paragraph_border_above_identical_paragraphs = border!(
        ParagraphBorderAboveIdenticalParagraphs,
        BorderStylesMember::Single,
        "24",
        "0000FF"
    );
    let mut own_margins = TableCellMarginExceptions::default();
    own_margins.paragraph_border_above_identical_paragraphs_tbl_width =
        width!(ParagraphBorderAboveIdenticalParagraphsTblWidth, "288");
    let mut own = own_table(Some("Spaced"), Some(first_row_look(true, false)));
    own.table_borders = Some(Box::new(own_borders.clone()));
    own.table_cell_margin_exceptions = Some(Box::new(own_margins.clone()));
    let table = sheet.table(&own).properties_at(position(0, 3, 0, 2));

    let borders = table.table_borders.unwrap();
    assert_eq!(
        borders.paragraph_border_above_identical_paragraphs,
        own_borders.paragraph_border_above_identical_paragraphs
    );
    assert_eq!(
        borders.paragraph_border_below_identical_paragraphs,
        format_borders.paragraph_border_below_identical_paragraphs
    );
    let whole = example_table_properties().table_borders.unwrap();
    assert_eq!(
        borders.table_inside_horizontal_edges_border,
        whole.table_inside_horizontal_edges_border
    );
    let margins = table.table_cell_margin_exceptions.unwrap();
    assert_eq!(
        margins.paragraph_border_above_identical_paragraphs_tbl_width,
        own_margins.paragraph_border_above_identical_paragraphs_tbl_width
    );
    assert_eq!(
        margins.table_cell_leading_edge_border_tbl_width,
        format_margins.table_cell_leading_edge_border_tbl_width
    );
    let whole_margins = example_table_properties()
        .table_cell_margin_exceptions
        .unwrap();
    assert_eq!(
        margins.table_cell_trailing_margin_exception_tbl_width,
        whole_margins.table_cell_trailing_margin_exception_tbl_width
    );
}

#[test]
fn where_only_the_whole_table_applies_properties_at_is_properties() {
    let sheet = spaced_sheet(Some(example_table_properties()), spacing_of("29"));
    let format = sheet.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(true, false)),
    ));
    let inner = position(1, 3, 0, 2);
    assert_eq!(format.formats(inner), [WholeTable]);
    assert_eq!(format.properties_at(inner), format.properties());

    let unstyled = sheet_of(Vec::new()).table(&own_table(None, None));
    let at = position(0, 3, 0, 2);
    assert_eq!(unstyled.properties_at(at), unstyled.properties());
}

#[test]
fn a_row_s_exception_cell_spacing_wins_over_the_first_row_format_s() {
    let sheet = spaced_sheet(Some(spacing_of("15")), spacing_of("29"));
    let format = sheet.table(&own_table(
        Some("Spaced"),
        Some(first_row_look(true, false)),
    ));
    let mut exceptions = TableLevelPropertyExceptions::default();
    exceptions.table_row_cell_spacing = spacing_of("0").table_row_cell_spacing;
    let row = format.for_row(Some(&exceptions));
    assert_eq!(spacing(&row.properties_at(position(0, 3, 0, 2))), Some("0"));
    assert_eq!(
        spacing(&format.properties_at(position(0, 3, 0, 2))),
        Some("29")
    );
}
