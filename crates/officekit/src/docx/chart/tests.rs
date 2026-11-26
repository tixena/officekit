use crate::parts::chart;
#[cfg(feature = "transitional")]
use crate::parts::kinds::SpreadsheetKind;
use crate::parts::kinds::WordprocessingKind;
#[cfg(feature = "transitional")]
use crate::parts::{shared_string_table, worksheet};
#[cfg(feature = "transitional")]
use crate::xlsx::Xlsx;
use officekit_markup::chart::write as chart_write;
use officekit_markup::chart::{
    AxisDataSourceContent, BarChartSeries, BarCharts, CategoryAxisDataCat, ChartSpace, FormatCode,
    Formula, NumberCache, NumberDataSourceContent, NumberReference, NumericValue,
    PlotAreaChartContent, PointCount, SeriesText, SeriesTextContent, StringCache, StringPoint,
    StringReference, Values,
};
#[cfg(feature = "transitional")]
use officekit_markup::chart::{
    ChartExtensibility, Extension, NumericPoint, ScatterChartSeries, ScatterCharts, XValues,
};
#[cfg(feature = "transitional")]
use officekit_markup::drawing::AnyElementContent;
#[cfg(feature = "transitional")]
use officekit_markup::spreadsheet::{
    Cell, CellType, CellValue, RichTextInline, RichTextRun, Row, SharedStringTable,
    SheetInformation, StringItem, Text as SheetText, Workbook, Worksheet,
};
use officekit_xml::element::Document as XmlDocument;
#[cfg(feature = "transitional")]
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
#[cfg(feature = "transitional")]
use officekit_xml::name::{OfficeVersion, QName};
use officekit_xml::packaging::package::DECLARATION;
#[cfg(feature = "transitional")]
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
#[cfg(feature = "transitional")]
use officekit_xml::write::WriteOptions as PartWriteOptions;
use officekit_xml::write::{Scope, WriteOptions};

use super::{ChartRanges, Docx};
#[cfg(feature = "transitional")]
use crate::validation::error::ValidationError;
#[cfg(feature = "transitional")]
use crate::validation::validator::Validator;

const SPREADSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// What a data reference holds once read back: its formula, the kind of its cache, the number
/// cache's format code, the cache's point count, and its points as index and value.
#[cfg(feature = "transitional")]
#[derive(Clone, Debug, PartialEq)]
struct Cached {
    cache: &'static str,
    count: Option<String>,
    format: Option<String>,
    formula: String,
    points: Vec<(String, String)>,
}

/// The series of a read-back chart: what its name, categories and values hold.
#[cfg(feature = "transitional")]
#[derive(Debug, PartialEq)]
struct SeriesRead {
    categories: Option<Cached>,
    name: Option<Cached>,
    values: Option<Cached>,
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// A cell of `value` read through the shared string `index`.
#[cfg(feature = "transitional")]
fn shared(reference: &str, index: &str) -> Cell {
    let mut cell = valued(reference, index);
    cell.t = Some(CellType::from("s".to_owned()));
    cell
}

/// A cell holding the number `value`.
#[cfg(feature = "transitional")]
fn valued(reference: &str, value: &str) -> Cell {
    let mut held = CellValue::default();
    held.value = value.to_owned();
    let mut cell = Cell::default();
    cell.r = Some(reference.to_owned());
    cell.value = Some(Box::new(held));
    cell
}

/// A cell holding the inline string `value`.
#[cfg(feature = "transitional")]
fn inline(reference: &str, value: &str) -> Cell {
    let mut text = SheetText::default();
    text.value = value.to_owned();
    let mut string = RichTextInline::default();
    string.text = Some(Box::new(text));
    let mut cell = Cell::default();
    cell.r = Some(reference.to_owned());
    cell.rich_text_inline = Some(Box::new(string));
    cell.t = Some(CellType::from("inlineStr".to_owned()));
    cell
}

/// Row `number` holding `cells`.
#[cfg(feature = "transitional")]
fn row(number: &str, cells: Vec<Cell>) -> Row {
    let mut held = Row::default();
    held.cell = cells;
    held.r = Some(number.to_owned());
    held
}

/// Rows 2 and 3 of the workbook rewritten with inline-string categories and values in column B.
#[cfg(feature = "transitional")]
fn two_rows() -> Vec<Row> {
    vec![
        row("2", vec![inline("A2", "Week 1"), valued("B2", "12")]),
        row("3", vec![inline("A3", "Week 2"), valued("B3", "18")]),
    ]
}

/// The workbook of chart 1 of the monthly threat report: one sheet, "Sheet1", with the series
/// names as shared strings in row 1, then `rows`; and a second, "Q1 data".
#[cfg(feature = "transitional")]
fn workbook_bytes(rows: Vec<Row>) -> Vec<u8> {
    let sheet_named = |sheet_name: &str, number: &str| {
        let mut sheet = SheetInformation::default();
        sheet.id = Some(format!("rId{number}"));
        sheet.name = Some(sheet_name.to_owned());
        sheet.sheet_id = Some(number.to_owned());
        sheet
    };
    let mut book = Workbook::default();
    book.sheets.sheet_information = vec![sheet_named("Sheet1", "1"), sheet_named("Q1 data", "2")];
    let mut first = Worksheet::default();
    first.sheet_data.row = vec![row(
        "1",
        vec![shared("A1", "0"), shared("B1", "1"), shared("C1", "2")],
    )];
    first.sheet_data.row.extend(rows);
    let mut second = Worksheet::default();
    second.sheet_data.row = vec![row("2", vec![valued("B2", "7"), shared("B3", "3")])];
    let text_of = |value: &str| {
        let mut text = SheetText::default();
        text.value = value.to_owned();
        Box::new(text)
    };
    let plain = |value: &str| {
        let mut item = StringItem::default();
        item.text = Some(text_of(value));
        item
    };
    let mut anomalies = StringItem::default();
    anomalies.rich_text_run = ["Anomal", "ias"]
        .map(|value| {
            let mut run = RichTextRun::default();
            run.text = text_of(value);
            run
        })
        .into();
    let mut strings = SharedStringTable::default();
    strings.string_item = vec![
        plain(" "),
        plain("Alertas Recibidas"),
        anomalies,
        plain("Quarter"),
    ];
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    xlsx.set_workbook(&book).unwrap();
    let main = name("/xl/workbook.xml");
    let first_part = name("/xl/worksheets/sheet1.xml");
    let second_part = name("/xl/worksheets/sheet2.xml");
    let strings_part = name("/xl/sharedStrings.xml");
    let package = xlsx.package_mut();
    worksheet::store_transitional(package, &first_part, &first, PartWriteOptions::default())
        .unwrap();
    worksheet::store_transitional(package, &second_part, &second, PartWriteOptions::default())
        .unwrap();
    package
        .relate(
            Some(&main),
            &second_part,
            RelationshipType::WORKSHEET.transitional,
        )
        .unwrap();
    shared_string_table::store_transitional(
        package,
        &strings_part,
        &strings,
        PartWriteOptions::default(),
    )
    .unwrap();
    package
        .relate(
            Some(&main),
            &strings_part,
            RelationshipType::SHARED_STRINGS.transitional,
        )
        .unwrap();
    xlsx.to_bytes_transitional().unwrap().0
}

/// A point count of `count`.
fn point_count(count: &str) -> PointCount {
    let mut held = PointCount::default();
    held.val = Some(count.to_owned());
    held
}

/// The number cache a series holds before a refresh: General, one point, no value.
fn empty_cache() -> NumberCache {
    let mut format = FormatCode::default();
    format.value = "General".to_owned();
    let mut cache = NumberCache::default();
    cache.format_code = Some(Box::new(format));
    cache.point_count = Some(Box::new(point_count("1")));
    cache
}

/// A formula of `text`.
fn formula(text: &str) -> Formula {
    let mut held = Formula::default();
    held.value = text.to_owned();
    held
}

/// A number reference to `text` holding `cache`.
fn number_reference(text: &str, cache: Option<NumberCache>) -> NumberReference {
    let mut reference = NumberReference::default();
    reference.formula = Box::new(formula(text));
    reference.number_cache = cache.map(Box::new);
    reference
}

/// A string cache of `count` points holding `values` from index 0.
fn string_cache(count: &str, values: &[&str]) -> StringCache {
    let mut cache = StringCache::default();
    cache.point_count = Some(Box::new(point_count(count)));
    cache.string_point = values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut text = NumericValue::default();
            text.value = (*value).to_owned();
            let mut point = StringPoint::default();
            point.idx = Some(index.to_string());
            point.numeric_value = Box::new(text);
            point
        })
        .collect();
    cache
}

/// A series name read from the cell `cell`, holding `cache`.
fn series_name(cell: &str, cache: StringCache) -> SeriesText {
    let mut reference = StringReference::default();
    reference.formula = Box::new(formula(cell));
    reference.string_cache = Some(Box::new(cache));
    let mut text = SeriesText::default();
    text.content = Box::new(SeriesTextContent::from(reference));
    text
}

/// Values read through `reference`.
fn values_of(reference: NumberReference) -> Values {
    let mut values = Values::default();
    values.content = Box::new(NumberDataSourceContent::from(reference));
    values
}

/// Categories read through the number reference `reference`.
fn categories_of(reference: NumberReference) -> CategoryAxisDataCat {
    let mut categories = CategoryAxisDataCat::default();
    categories.content = Box::new(AxisDataSourceContent::from(reference));
    categories
}

/// A bar chart series of `values`.
fn series_of_values(values: NumberReference) -> BarChartSeries {
    let mut series = BarChartSeries::default();
    series.values = Some(Box::new(values_of(values)));
    series
}

/// The two series of chart 1 of the monthly threat report, each named by a cell of row 1 and
/// reading row 2.
fn report_series() -> Vec<BarChartSeries> {
    let mut first = series_of_values(number_reference("Sheet1!$B$2", Some(empty_cache())));
    first.category_axis_data_cat = Some(Box::new(categories_of(number_reference(
        "Sheet1!$A$2",
        Some(empty_cache()),
    ))));
    first.text = Some(Box::new(series_name(
        "Sheet1!$B$1",
        string_cache("1", &["old"]),
    )));
    let mut second = series_of_values(number_reference("Sheet1!$C$2", Some(empty_cache())));
    second.category_axis_data_cat = Some(Box::new(categories_of(number_reference(
        "Sheet1!$A$2",
        Some(empty_cache()),
    ))));
    second.text = Some(Box::new(series_name("Sheet1!$C$1", string_cache("1", &[]))));
    vec![first, second]
}

/// A chart of one bar chart holding `series`.
fn chart_of(series: Vec<BarChartSeries>) -> ChartSpace {
    let mut bars = BarCharts::default();
    bars.bar_chart_series = series;
    let mut space = ChartSpace::default();
    space.chart.plot_area.chart_content = vec![PlotAreaChartContent::from(bars)];
    space
}

/// A document whose chart part holds `chart` and relates to `workbook`.
fn document(chart: &ChartSpace, workbook: Option<Vec<u8>>) -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let part = name("/word/charts/chart1.xml");
    let xml = XmlDocument::new(
        DECLARATION,
        chart_write::chart_space(chart, &mut Scope::default()),
    )
    .to_xml(WriteOptions::default());
    docx.package_mut()
        .set_part_bytes(&part, chart::CONTENT_TYPE, &xml)
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

/// The chart part's text.
#[cfg(feature = "transitional")]
fn chart_text(docx: &Docx) -> String {
    String::from_utf8(
        docx.package()
            .part_bytes(&name("/word/charts/chart1.xml"))
            .unwrap(),
    )
    .unwrap()
}

/// `chart` refreshed with `ranges` from the workbook holding `rows`, and read back.
#[cfg(feature = "transitional")]
fn refreshed(chart: &ChartSpace, ranges: ChartRanges) -> ChartSpace {
    let mut docx = document(chart, Some(workbook_bytes(two_rows())));
    let part = name("/word/charts/chart1.xml");
    docx.refresh_chart_caches(&part, ranges).unwrap();
    chart::load(docx.package(), &part, OpenOptions::default()).unwrap()
}

/// What a number reference holds.
#[cfg(feature = "transitional")]
fn number_cached(reference: &NumberReference) -> Cached {
    let cache = reference.number_cache.as_deref();
    Cached {
        cache: "num",
        count: cache.and_then(|held| held.point_count.as_ref()?.val.clone()),
        format: cache.and_then(|held| Some(held.format_code.as_ref()?.value.clone())),
        formula: reference.formula.value.clone(),
        points: cache.map_or_default(|held| {
            held.numeric_point
                .iter()
                .map(|point| {
                    (
                        point.idx.clone().unwrap_or_default(),
                        point.numeric_value.value.clone(),
                    )
                })
                .collect()
        }),
    }
}

/// What a string reference holds.
#[cfg(feature = "transitional")]
fn string_cached(reference: &StringReference) -> Cached {
    let cache = reference.string_cache.as_deref();
    Cached {
        cache: "str",
        count: cache.and_then(|held| held.point_count.as_ref()?.val.clone()),
        format: None,
        formula: reference.formula.value.clone(),
        points: cache.map_or_default(|held| {
            held.string_point
                .iter()
                .map(|point: &StringPoint| {
                    (
                        point.idx.clone().unwrap_or_default(),
                        point.numeric_value.value.clone(),
                    )
                })
                .collect()
        }),
    }
}

/// What axis data holds, when it is a number or a string reference.
#[cfg(feature = "transitional")]
fn axis_cached(content: &AxisDataSourceContent) -> Option<Cached> {
    if let AxisDataSourceContent::NumberReference(reference) = content {
        Some(number_cached(reference))
    } else if let AxisDataSourceContent::StringReference(reference) = content {
        Some(string_cached(reference))
    } else {
        None
    }
}

/// The bar chart series of `space`, as read back.
#[cfg(feature = "transitional")]
fn series_read(space: &ChartSpace) -> Vec<SeriesRead> {
    space
        .chart
        .plot_area
        .chart_content
        .iter()
        .filter_map(|content| {
            if let PlotAreaChartContent::BarCharts(bars) = content {
                Some(&bars.bar_chart_series)
            } else {
                None
            }
        })
        .flatten()
        .map(|series| SeriesRead {
            categories: series
                .category_axis_data_cat
                .as_ref()
                .and_then(|held| axis_cached(&held.content)),
            name: series.text.as_ref().and_then(|held| {
                if let SeriesTextContent::StringReference(reference) = held.content.as_ref() {
                    Some(string_cached(reference))
                } else {
                    None
                }
            }),
            values: series.values.as_ref().and_then(|held| {
                if let NumberDataSourceContent::NumberReference(reference) = held.content.as_ref() {
                    Some(number_cached(reference))
                } else {
                    None
                }
            }),
        })
        .collect()
}

/// A number reference to `formula` whose General cache counts `count` points holding `points`.
#[cfg(feature = "transitional")]
fn numbers(formula_text: &str, count: &str, points: &[(&str, &str)]) -> Cached {
    Cached {
        cache: "num",
        count: Some(count.to_owned()),
        format: Some("General".to_owned()),
        formula: formula_text.to_owned(),
        points: points
            .iter()
            .map(|(index, value)| ((*index).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

/// A string reference to `formula` whose cache counts `count` points holding `values` from 0.
#[cfg(feature = "transitional")]
fn strings(formula_text: &str, count: &str, values: &[&str]) -> Cached {
    Cached {
        cache: "str",
        count: Some(count.to_owned()),
        format: None,
        formula: formula_text.to_owned(),
        points: values
            .iter()
            .enumerate()
            .map(|(index, value)| (index.to_string(), (*value).to_owned()))
            .collect(),
    }
}

/// The values of the first series of `space`, as read back.
#[cfg(feature = "transitional")]
fn first_values(space: &ChartSpace) -> Option<Cached> {
    series_read(space).into_iter().next()?.values
}

/// The `c16:uniqueId` extension Word writes into a chart's data references.
#[cfg(feature = "transitional")]
fn unique_id_extension() -> Extension {
    const C16: &str = "http://schemas.microsoft.com/office/drawing/2014/chart";
    let mut unique_id = Element::new(QName::prefixed("c16", "uniqueId", C16));
    unique_id
        .namespace_declarations
        .push(NamespaceDeclaration::new("c16".to_owned(), C16.to_owned()));
    unique_id.attributes.push(Attribute::new(
        QName::unprefixed("val", ""),
        "{00000000-0001-0000-0000-000000000000}".to_owned(),
    ));
    let mut extension = Extension::default();
    extension.uri = Some("{C3380CC4-5D6E-409C-BE32-E72D297353CC}".to_owned());
    extension.content = Box::new(AnyElementContent::Other(Box::new(Node::Element(unique_id))));
    extension
}

/// A document whose main part relates a chart holding `extension` in its values' extension list.
#[cfg(feature = "transitional")]
fn chart_with_extension(extension: Extension) -> Docx {
    let mut extended = ChartExtensibility::default();
    extended.extension = vec![extension];
    let mut values = number_reference("Sheet1!$B$2:$B$3", None);
    values.chart_extensibility = Some(Box::new(extended));
    let mut docx = document(&chart_of(vec![series_of_values(values)]), None);
    let main = docx.main_document_part().clone();
    docx.package_mut()
        .relate(
            Some(&main),
            &name("/word/charts/chart1.xml"),
            RelationshipType::CHART.transitional,
        )
        .unwrap();
    docx
}

/// What the validator at `target` reports on the `c:ext` of `docx`'s chart, one line each.
#[cfg(feature = "transitional")]
fn reported_on_ext(docx: &Docx, target: OfficeVersion) -> Vec<String> {
    Validator::new(target)
        .validate(docx.package())
        .iter()
        .map(ValidationError::line)
        .filter(|line| line.contains("/c:ext[1]:"))
        .collect()
}

#[cfg(feature = "transitional")]
#[test]
fn a_default_extension_writes_an_empty_ext_and_no_drawing() {
    let text = chart_text(&chart_with_extension(Extension::default()));
    assert!(
        text.contains("</c:f><c:extLst><c:ext/></c:extLst></c:numRef>"),
        "{text}"
    );
    assert!(!text.contains("wp:"), "{text}");
}

#[cfg(feature = "transitional")]
#[test]
fn the_validator_reports_the_child_a_default_extension_lacks() {
    let mut extension = Extension::default();
    extension.uri = Some("x".to_owned());
    assert_eq!(
        reported_on_ext(
            &chart_with_extension(extension),
            OfficeVersion::Microsoft365
        ),
        [
            "/word/charts/chart1.xml /c:chartSpace[1]/c:chart[1]/c:plotArea[1]/c:barChart[1]/c:ser[1]/c:val[1]/c:numRef[1]/c:extLst[1]/c:ext[1]: schema MissingChild: the element ends before a child it requires, expected c15:formulaRef, c15:fullRef, c15:levelRef or ##any"
        ]
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_validator_reports_nothing_on_a_unique_id_extension() {
    assert_eq!(
        reported_on_ext(
            &chart_with_extension(unique_id_extension()),
            OfficeVersion::Microsoft365
        ),
        Vec::<String>::new()
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_validator_reports_nothing_on_an_extension_without_its_uri_at_any_target() {
    let mut extension = unique_id_extension();
    extension.uri = None;
    let docx = chart_with_extension(extension);
    for target in [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ] {
        assert_eq!(
            reported_on_ext(&docx, target),
            Vec::<String>::new(),
            "{target:?}"
        );
    }
}

#[cfg(feature = "transitional")]
#[test]
fn an_extension_read_without_its_child_is_written_back_as_it_was() {
    let mut extension = Extension::default();
    extension.uri = Some("x".to_owned());
    let docx = chart_with_extension(extension);
    let text = chart_text(&docx);
    assert!(
        text.contains("<c:extLst><c:ext uri=\"x\"/></c:extLst>"),
        "{text}"
    );
    let part = name("/word/charts/chart1.xml");
    let read = chart::load(docx.package(), &part, OpenOptions::default()).unwrap();
    let again = XmlDocument::new(
        DECLARATION,
        chart_write::chart_space(&read, &mut Scope::default()),
    )
    .to_xml(WriteOptions::default());
    assert_eq!(String::from_utf8(again).unwrap(), text);
}

#[cfg(feature = "transitional")]
#[test]
fn two_rows_written_give_the_ranges_and_caches_of_the_last_row_holding_a_value() {
    let read = series_read(&refreshed(
        &chart_of(report_series()),
        ChartRanges::ToLastRow,
    ));
    let first = read.first().unwrap();
    assert_eq!(
        first.categories,
        Some(strings("Sheet1!$A$2:$A$3", "2", &["Week 1", "Week 2"]))
    );
    assert_eq!(
        first.values,
        Some(numbers(
            "Sheet1!$B$2:$B$3",
            "2",
            &[("0", "12"), ("1", "18")]
        ))
    );
    assert_eq!(
        first.name,
        Some(strings("Sheet1!$B$1", "1", &["Alertas Recibidas"]))
    );
    assert_eq!(
        read.get(1).and_then(|second| second.values.clone()),
        Some(numbers("Sheet1!$C$2:$C$2", "1", &[]))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_series_name_made_of_runs_is_the_runs_text() {
    let read = series_read(&refreshed(
        &chart_of(report_series()),
        ChartRanges::ToLastRow,
    ));
    assert_eq!(
        read.get(1).and_then(|second| second.name.clone()),
        Some(strings("Sheet1!$C$1", "1", &["Anomalias"]))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn as_written_reads_the_cells_the_formula_names_and_keeps_it() {
    let range = series_of_values(number_reference("Sheet1!$B$2:$B$3", Some(empty_cache())));
    assert_eq!(
        first_values(&refreshed(&chart_of(vec![range]), ChartRanges::AsWritten)),
        Some(numbers(
            "Sheet1!$B$2:$B$3",
            "2",
            &[("0", "12"), ("1", "18")]
        ))
    );
    let single = series_of_values(number_reference("Sheet1!$B$2", Some(empty_cache())));
    assert_eq!(
        first_values(&refreshed(&chart_of(vec![single]), ChartRanges::AsWritten)),
        Some(numbers("Sheet1!$B$2", "1", &[("0", "12")]))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_range_never_ends_above_its_first_row() {
    let series = series_of_values(number_reference("Sheet1!$D$5", Some(empty_cache())));
    let values = first_values(&refreshed(&chart_of(vec![series]), ChartRanges::ToLastRow));
    assert_eq!(
        values.map(|held| held.formula).as_deref(),
        Some("Sheet1!$D$5:$D$5")
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_range_of_several_columns_keeps_its_formula_and_counts_row_by_row() {
    let series = series_of_values(number_reference("Sheet1!$A$2:$B$3", Some(empty_cache())));
    assert_eq!(
        first_values(&refreshed(&chart_of(vec![series]), ChartRanges::ToLastRow)),
        Some(numbers(
            "Sheet1!$A$2:$B$3",
            "4",
            &[("1", "12"), ("3", "18")]
        ))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_quoted_sheet_name_finds_its_sheet_and_is_written_back_as_quoted() {
    let mut series = series_of_values(number_reference("'Q1 data'!$B$2", Some(empty_cache())));
    series.category_axis_data_cat = Some(Box::new(categories_of(number_reference(
        "'Q1 data'!B2",
        Some(empty_cache()),
    ))));
    let read = series_read(&refreshed(&chart_of(vec![series]), ChartRanges::ToLastRow));
    let first = read.first().unwrap();
    assert_eq!(
        first.values,
        Some(numbers("'Q1 data'!$B$2:$B$3", "2", &[("0", "7")]))
    );
    assert_eq!(
        first.categories,
        Some(strings("'Q1 data'!$B$2:$B$3", "2", &["7", "Quarter"]))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_formula_that_is_not_one_area_of_a_sheet_leaves_its_cache() {
    let old = || {
        let mut five = NumericValue::default();
        five.value = "5".to_owned();
        let mut point = NumericPoint::default();
        point.idx = Some("0".to_owned());
        point.numeric_value = Box::new(five);
        let mut cache = NumberCache::default();
        cache.numeric_point = vec![point];
        cache.point_count = Some(Box::new(point_count("1")));
        cache
    };
    for text in [
        "Ventas",
        "Missing!$B$2",
        "(Sheet1!$B$2,Sheet1!$B$3)",
        "Sheet1!$B$2,Sheet1!$B$3",
        "Sheet1!$B$2:$B$3:$B$4",
        "Sheet1!$B$0",
        "Sheet1!$ZZZZ$2",
        "Sheet1!",
    ] {
        let series = series_of_values(number_reference(text, Some(old())));
        let kept = Cached {
            format: None,
            ..numbers(text, "1", &[("0", "5")])
        };
        assert_eq!(
            first_values(&refreshed(&chart_of(vec![series]), ChartRanges::ToLastRow)),
            Some(kept),
            "{text}"
        );
    }
}

#[cfg(feature = "transitional")]
#[test]
fn a_missing_cache_is_created_after_the_formula_and_points_go_before_the_extension_list() {
    let mut values = number_reference("Sheet1!$B$2:$B$3", None);
    values.chart_extensibility = Some(Box::default());
    let mut extended = ChartExtensibility::default();
    extended.extension = vec![unique_id_extension()];
    let mut cache = string_cache("9", &[]);
    cache.chart_extensibility = Some(Box::new(extended));
    let mut categories = StringReference::default();
    categories.formula = Box::new(formula("Sheet1!$A$2:$A$3"));
    categories.string_cache = Some(Box::new(cache));
    let mut series = series_of_values(values);
    let mut category = CategoryAxisDataCat::default();
    category.content = Box::new(AxisDataSourceContent::from(categories));
    series.category_axis_data_cat = Some(Box::new(category));
    let mut docx = document(&chart_of(vec![series]), Some(workbook_bytes(two_rows())));
    docx.refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::AsWritten)
        .unwrap();
    let text = chart_text(&docx);
    assert!(
        text.contains(concat!(
            "<c:cat><c:strRef><c:f>Sheet1!$A$2:$A$3</c:f><c:strCache><c:ptCount val=\"2\"/>",
            "<c:pt idx=\"0\"><c:v>Week 1</c:v></c:pt><c:pt idx=\"1\"><c:v>Week 2</c:v></c:pt>",
            "<c:extLst><c:ext uri=\"{C3380CC4-5D6E-409C-BE32-E72D297353CC}\">",
            "<c16:uniqueId xmlns:c16=\"http://schemas.microsoft.com/office/drawing/2014/chart\" ",
            "val=\"{00000000-0001-0000-0000-000000000000}\"/></c:ext></c:extLst></c:strCache>",
            "</c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B$3</c:f><c:numCache>",
            "<c:ptCount val=\"2\"/><c:pt idx=\"0\"><c:v>12</c:v></c:pt>",
            "<c:pt idx=\"1\"><c:v>18</c:v></c:pt></c:numCache><c:extLst/></c:numRef></c:val>",
        )),
        "{text}"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_cell_without_a_value_gets_no_point_but_counts_in_the_area() {
    let series = series_of_values(number_reference("Sheet1!$B$2:$B$5", Some(empty_cache())));
    assert_eq!(
        first_values(&refreshed(&chart_of(vec![series]), ChartRanges::AsWritten)),
        Some(numbers(
            "Sheet1!$B$2:$B$5",
            "4",
            &[("0", "12"), ("1", "18")]
        ))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_workbook_is_not_changed() {
    let bytes = workbook_bytes(two_rows());
    let embedded = name("/word/embeddings/Microsoft_Excel_Worksheet.xlsx");
    let mut docx = document(&chart_of(report_series()), Some(bytes.clone()));
    docx.refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap();
    assert_eq!(docx.package().part_bytes(&embedded).unwrap(), bytes);
}

#[test]
fn a_chart_without_a_workbook_is_a_read_error() {
    let mut docx = document(&chart_of(report_series()), None);
    let error = docx
        .refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot read the part /word/charts/chart1.xml: the chart embeds no workbook"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn an_embedded_document_that_is_not_a_workbook_is_a_read_error() {
    let other = Docx::new(WordprocessingKind::Document)
        .unwrap()
        .package()
        .to_bytes()
        .unwrap();
    let mut docx = document(&chart_of(report_series()), Some(other));
    let error = docx
        .refresh_chart_caches(&name("/word/charts/chart1.xml"), ChartRanges::ToLastRow)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "cannot read the part /word/embeddings/Microsoft_Excel_Worksheet.xlsx: the document the chart embeds is not a workbook"
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_text_category_range_becomes_a_text_reference_with_a_text_cache() {
    let mut categories = number_reference("Sheet1!$A$2:$A$3", Some(empty_cache()));
    let mut extended = ChartExtensibility::default();
    extended.extension = vec![unique_id_extension()];
    categories.chart_extensibility = Some(Box::new(extended));
    let mut bar = BarChartSeries::default();
    bar.category_axis_data_cat = Some(Box::new(categories_of(categories)));
    let mut x_values = XValues::default();
    x_values.content = Box::new(AxisDataSourceContent::from(number_reference(
        "Sheet1!$A$2:$A$3",
        Some(empty_cache()),
    )));
    let mut scattered = ScatterChartSeries::default();
    scattered.x_values = Some(Box::new(x_values));
    let mut scatter = ScatterCharts::default();
    scatter.scatter_chart_series = vec![scattered];
    let mut chart = chart_of(vec![bar]);
    chart
        .chart
        .plot_area
        .chart_content
        .push(PlotAreaChartContent::from(scatter));
    let space = refreshed(&chart, ChartRanges::AsWritten);
    let weeks = strings("Sheet1!$A$2:$A$3", "2", &["Week 1", "Week 2"]);
    let category = series_read(&space)
        .into_iter()
        .next()
        .and_then(|series| series.categories);
    assert_eq!(category, Some(weeks));
    let x_read = space
        .chart
        .plot_area
        .chart_content
        .iter()
        .find_map(|content| {
            if let PlotAreaChartContent::ScatterCharts(found) = content {
                found.scatter_chart_series.first()?.x_values.as_ref()
            } else {
                None
            }
        })
        .and_then(|held| axis_cached(&held.content));
    assert_eq!(
        x_read.map(|held| (held.cache, held.formula)),
        Some(("str", "Sheet1!$A$2:$A$3".to_owned()))
    );
    let kept_extension = space
        .chart
        .plot_area
        .chart_content
        .iter()
        .find_map(|content| {
            if let PlotAreaChartContent::BarCharts(found) = content {
                found
                    .bar_chart_series
                    .first()?
                    .category_axis_data_cat
                    .as_ref()
            } else {
                None
            }
        })
        .and_then(|held| {
            if let AxisDataSourceContent::StringReference(reference) = held.content.as_ref() {
                reference.chart_extensibility.as_ref()
            } else {
                None
            }
        })
        .map(|extensions| extensions.extension.len());
    assert_eq!(kept_extension, Some(1));
}

#[cfg(feature = "transitional")]
#[test]
fn a_number_category_range_stays_a_number_reference_with_a_number_cache() {
    let mut series = BarChartSeries::default();
    series.category_axis_data_cat = Some(Box::new(categories_of(number_reference(
        "Sheet1!$B$2:$B$3",
        Some(empty_cache()),
    ))));
    let category = series_read(&refreshed(&chart_of(vec![series]), ChartRanges::AsWritten))
        .into_iter()
        .next()
        .and_then(|read| read.categories);
    assert_eq!(
        category,
        Some(numbers(
            "Sheet1!$B$2:$B$3",
            "2",
            &[("0", "12"), ("1", "18")]
        ))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn a_value_cell_that_is_text_gets_no_point_but_counts_in_the_area() {
    let series = series_of_values(number_reference("Sheet1!$A$2:$B$3", Some(empty_cache())));
    assert_eq!(
        first_values(&refreshed(&chart_of(vec![series]), ChartRanges::AsWritten)),
        Some(numbers(
            "Sheet1!$A$2:$B$3",
            "4",
            &[("1", "12"), ("3", "18")]
        ))
    );
}

#[cfg(feature = "transitional")]
#[test]
fn the_report_chart_over_text_categories_refreshes_to_text_categories_and_number_values() {
    let read = series_read(&refreshed(
        &chart_of(report_series()),
        ChartRanges::ToLastRow,
    ));
    let first = read.first().unwrap();
    assert_eq!(
        first.categories,
        Some(strings("Sheet1!$A$2:$A$3", "2", &["Week 1", "Week 2"]))
    );
    assert_eq!(
        first.values,
        Some(numbers(
            "Sheet1!$B$2:$B$3",
            "2",
            &[("0", "12"), ("1", "18")]
        ))
    );
}
