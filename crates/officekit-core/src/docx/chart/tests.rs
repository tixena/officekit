use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::RelationshipType;

use super::{ChartRanges, Docx};
use crate::parts::kinds::WordprocessingKind;
use crate::parts::{chart, shared_string_table, workbook, worksheet};

const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const EMPTY_CACHE: &str =
    "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"1\"/></c:numCache>";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const S: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const SPREADSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
/// Rows 2 and 3 of the workbook rewritten with inline-string categories and values in column B.
const TWO_ROWS: &str = "<row r=\"2\"><c r=\"A2\" t=\"inlineStr\"><is><t>Week 1</t></is></c><c r=\"B2\"><v>12</v></c></row><row r=\"3\"><c r=\"A3\" t=\"inlineStr\"><is><t>Week 2</t></is></c><c r=\"B3\"><v>18</v></c></row>";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// The workbook of chart 1 of the monthly threat report: one sheet, "Sheet1", with the series
/// names as shared strings in row 1, then `rows` as the cells of `sheetData` after row 1.
fn workbook_bytes(rows: &str) -> Vec<u8> {
    let mut package = Package::new();
    let main = name("/xl/workbook.xml");
    let book = format!(
        "<workbook xmlns=\"{S}\" xmlns:r=\"{R}\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/><sheet name=\"Q1 data\" sheetId=\"2\" r:id=\"rId2\"/></sheets></workbook>"
    );
    package
        .set_part_bytes(&main, workbook::CONTENT_TYPES[0], book.as_bytes())
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    let first = format!(
        "<worksheet xmlns=\"{S}\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c><c r=\"C1\" t=\"s\"><v>2</v></c></row>{rows}</sheetData></worksheet>"
    );
    let second = format!(
        "<worksheet xmlns=\"{S}\"><sheetData><row r=\"2\"><c r=\"B2\"><v>7</v></c><c r=\"B3\" t=\"s\"><v>3</v></c></row></sheetData></worksheet>"
    );
    for (path, text) in [
        ("/xl/worksheets/sheet1.xml", first),
        ("/xl/worksheets/sheet2.xml", second),
    ] {
        let sheet = name(path);
        package
            .set_part_bytes(&sheet, worksheet::CONTENT_TYPE, text.as_bytes())
            .unwrap();
        package
            .relate(
                Some(&main),
                &sheet,
                RelationshipType::WORKSHEET.transitional,
            )
            .unwrap();
    }
    let strings = format!(
        "<sst xmlns=\"{S}\"><si><t xml:space=\"preserve\"> </t></si><si><t>Alertas Recibidas</t></si><si><r><t>Anomal</t></r><r><t>ias</t></r></si><si><t>Quarter</t></si></sst>"
    );
    let part = name("/xl/sharedStrings.xml");
    package
        .set_part_bytes(&part, shared_string_table::CONTENT_TYPE, strings.as_bytes())
        .unwrap();
    package
        .relate(
            Some(&main),
            &part,
            RelationshipType::SHARED_STRINGS.transitional,
        )
        .unwrap();
    package.to_bytes().unwrap()
}

fn series_name(cell: &str, cache: &str) -> String {
    format!("<c:tx><c:strRef><c:f>{cell}</c:f>{cache}</c:strRef></c:tx>")
}

fn reference(local: &str, formula: &str, cache: &str) -> String {
    format!("<c:{local}><c:numRef><c:f>{formula}</c:f>{cache}</c:numRef></c:{local}>")
}

/// A document whose chart part holds `series` inside one `c:ser` and relates to `workbook`.
fn document(series: &str, workbook: Option<Vec<u8>>) -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let part = name("/word/charts/chart1.xml");
    let xml = format!(
        "<c:chartSpace xmlns:c=\"{C}\"><c:chart><c:plotArea><c:barChart><c:ser>{series}</c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"
    );
    docx.package_mut()
        .set_part_bytes(&part, chart::CONTENT_TYPE, xml.as_bytes())
        .unwrap();
    if let Some(bytes) = workbook {
        let embedded = name("/word/embeddings/Microsoft_Excel_Worksheet.xlsx");
        docx.package_mut()
            .set_part_bytes(&embedded, SPREADSHEET, &bytes)
            .unwrap();
        docx.package_mut()
            .relate(
                Some(&part),
                &embedded,
                RelationshipType::PACKAGE.transitional,
            )
            .unwrap();
    }
    docx
}

fn chart_text(docx: &Docx) -> String {
    String::from_utf8(
        docx.package()
            .part_bytes(&name("/word/charts/chart1.xml"))
            .unwrap(),
    )
    .unwrap()
}

fn refreshed(series: &str, rows: &str, ranges: ChartRanges) -> String {
    let mut docx = document(series, Some(workbook_bytes(rows)));
    docx.refresh_chart_caches(&name("/word/charts/chart1.xml"), ranges)
        .unwrap();
    chart_text(&docx)
}

fn report_series() -> String {
    format!(
        "{}{}{}{}{}{}",
        series_name(
            "Sheet1!$B$1",
            "<c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>old</c:v></c:pt></c:strCache>"
        ),
        reference("cat", "Sheet1!$A$2", EMPTY_CACHE),
        reference("val", "Sheet1!$B$2", EMPTY_CACHE),
        series_name(
            "Sheet1!$C$1",
            "<c:strCache><c:ptCount val=\"1\"/></c:strCache>"
        ),
        reference("cat", "Sheet1!$A$2", EMPTY_CACHE),
        reference("val", "Sheet1!$C$2", EMPTY_CACHE),
    )
}

#[test]
fn two_rows_written_give_the_ranges_and_caches_of_the_last_row_holding_a_value() {
    let text = refreshed(&report_series(), TWO_ROWS, ChartRanges::ToLastRow);
    assert!(text.contains("<c:f>Sheet1!$A$2:$A$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>Week 1</c:v></c:pt><c:pt idx=\"1\"><c:v>Week 2</c:v></c:pt></c:numCache>"), "{text}");
    assert!(text.contains("<c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt><c:pt idx=\"1\"><c:v>18</c:v></c:pt></c:numCache>"), "{text}");
    assert!(text.contains("<c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Alertas Recibidas</c:v></c:pt></c:strCache>"), "{text}");
    assert!(text.contains("<c:f>Sheet1!$C$2:$C$2</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"1\"/></c:numCache>"), "{text}");
}

#[test]
fn a_series_name_made_of_runs_is_the_runs_text() {
    let text = refreshed(&report_series(), TWO_ROWS, ChartRanges::ToLastRow);
    assert!(text.contains("<c:f>Sheet1!$C$1</c:f><c:strCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>Anomalias</c:v></c:pt></c:strCache>"), "{text}");
}

#[test]
fn as_written_reads_the_cells_the_formula_names_and_keeps_it() {
    let series = reference("val", "Sheet1!$B$2:$B$3", EMPTY_CACHE);
    let text = refreshed(&series, TWO_ROWS, ChartRanges::AsWritten);
    assert!(text.contains("<c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt><c:pt idx=\"1\"><c:v>18</c:v></c:pt></c:numCache>"), "{text}");
    let single = reference("val", "Sheet1!$B$2", EMPTY_CACHE);
    let single_text = refreshed(&single, TWO_ROWS, ChartRanges::AsWritten);
    assert!(single_text.contains("<c:f>Sheet1!$B$2</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt>"), "{single_text}");
}

#[test]
fn a_range_never_ends_above_its_first_row() {
    let series = reference("val", "Sheet1!$D$5", EMPTY_CACHE);
    let text = refreshed(&series, TWO_ROWS, ChartRanges::ToLastRow);
    assert!(text.contains("<c:f>Sheet1!$D$5:$D$5</c:f>"), "{text}");
}

#[test]
fn a_range_of_several_columns_keeps_its_formula_and_counts_row_by_row() {
    let series = reference("val", "Sheet1!$A$2:$B$3", EMPTY_CACHE);
    let text = refreshed(&series, TWO_ROWS, ChartRanges::ToLastRow);
    assert!(text.contains("<c:f>Sheet1!$A$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"4\"/><c:pt idx=\"0\"><c:v>Week 1</c:v></c:pt><c:pt idx=\"1\"><c:v>12</c:v></c:pt><c:pt idx=\"2\"><c:v>Week 2</c:v></c:pt><c:pt idx=\"3\"><c:v>18</c:v></c:pt></c:numCache>"), "{text}");
}

#[test]
fn a_quoted_sheet_name_finds_its_sheet_and_is_written_back_as_quoted() {
    let series = format!(
        "{}{}",
        reference("val", "'Q1 data'!$B$2", EMPTY_CACHE),
        reference("cat", "'Q1 data'!B2", EMPTY_CACHE)
    );
    let text = refreshed(&series, TWO_ROWS, ChartRanges::ToLastRow);
    assert!(text.contains("<c:f>'Q1 data'!$B$2:$B$3</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>7</c:v></c:pt><c:pt idx=\"1\"><c:v>Quarter</c:v></c:pt>"), "{text}");
}

#[test]
fn a_formula_that_is_not_one_area_of_a_sheet_leaves_its_cache() {
    let old = "<c:numCache><c:ptCount val=\"1\"/><c:pt idx=\"0\"><c:v>5</c:v></c:pt></c:numCache>";
    for formula in [
        "Ventas",
        "Missing!$B$2",
        "(Sheet1!$B$2,Sheet1!$B$3)",
        "Sheet1!$B$2,Sheet1!$B$3",
        "Sheet1!$B$2:$B$3:$B$4",
        "Sheet1!$B$0",
        "Sheet1!$ZZZZ$2",
        "Sheet1!",
    ] {
        let series = reference("val", formula, old);
        let text = refreshed(&series, TWO_ROWS, ChartRanges::ToLastRow);
        assert!(
            text.contains(&format!("<c:f>{formula}</c:f>{old}")),
            "{formula}: {text}"
        );
    }
}

#[test]
fn a_missing_cache_is_created_after_the_formula_and_points_go_before_the_extension_list() {
    let series = "<c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:extLst/></c:numRef></c:val><c:cat><c:strRef><c:f>Sheet1!$A$2:$A$3</c:f><c:strCache><c:ptCount val=\"9\"/><c:extLst><c:ext/></c:extLst></c:strCache></c:strRef></c:cat>";
    let text = refreshed(series, TWO_ROWS, ChartRanges::AsWritten);
    assert!(text.contains("<c:f>Sheet1!$B$2:$B$3</c:f><c:numCache><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt><c:pt idx=\"1\"><c:v>18</c:v></c:pt></c:numCache><c:extLst/>"), "{text}");
    assert!(text.contains("<c:strCache><c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>Week 1</c:v></c:pt><c:pt idx=\"1\"><c:v>Week 2</c:v></c:pt><c:extLst><c:ext/></c:extLst></c:strCache>"), "{text}");
}

#[test]
fn a_cell_without_a_value_gets_no_point_but_counts_in_the_area() {
    let series = reference("val", "Sheet1!$B$2:$B$5", EMPTY_CACHE);
    let text = refreshed(&series, TWO_ROWS, ChartRanges::AsWritten);
    assert!(text.contains("<c:ptCount val=\"4\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt><c:pt idx=\"1\"><c:v>18</c:v></c:pt></c:numCache>"), "{text}");
}

#[test]
fn the_workbook_is_not_changed() {
    let bytes = workbook_bytes(TWO_ROWS);
    let embedded = name("/word/embeddings/Microsoft_Excel_Worksheet.xlsx");
    let mut docx = document(&report_series(), Some(bytes.clone()));
    docx.refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap();
    assert_eq!(docx.package().part_bytes(&embedded).unwrap(), bytes);
}

#[test]
fn a_chart_without_a_workbook_is_a_read_error() {
    let mut docx = document(&report_series(), None);
    let error = docx
        .refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot read the part /word/charts/chart1.xml: the chart embeds no workbook"
    );
}

#[test]
fn an_embedded_document_that_is_not_a_workbook_is_a_read_error() {
    let other = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .package()
        .to_bytes()
        .unwrap();
    let mut docx = document(&report_series(), Some(other));
    let error = docx
        .refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot read the part /word/embeddings/Microsoft_Excel_Worksheet.xlsx: the document the chart embeds is not a workbook"
    );
}

#[test]
fn cell_references_convert_columns_with_or_without_dollars() {
    use super::{cell_position, column_letters};
    assert_eq!(cell_position("$A$1"), Some((1, 1)));
    assert_eq!(cell_position("Z9"), Some((26, 9)));
    assert_eq!(cell_position("AA10"), Some((27, 10)));
    assert_eq!(cell_position("XFD1048576"), Some((0x4000, 0x0010_0000)));
    assert_eq!(cell_position("XFE1"), None);
    assert_eq!(cell_position("A1048577"), None);
    assert_eq!(cell_position("a1"), None);
    assert_eq!(cell_position("1"), None);
    assert_eq!(cell_position("A"), None);
    assert_eq!(column_letters(1), "A");
    assert_eq!(column_letters(26), "Z");
    assert_eq!(column_letters(27), "AA");
    assert_eq!(column_letters(0x4000), "XFD");
}
