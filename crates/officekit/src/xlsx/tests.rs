#[cfg(feature = "transitional")]
use std::path::Path;

use crate::parts::kinds::SpreadsheetKind;
#[cfg(feature = "strict")]
use crate::parts::part_root::{PartRoot, store};
#[cfg(feature = "strict")]
use crate::parts::pivot_table_cache_definition;
use crate::parts::worksheet;
#[cfg(feature = "strict")]
use crate::parts::{pivot_table, table_definition};
#[cfg(all(feature = "strict", feature = "transitional"))]
use crate::transitional::dates::SaveWarning;
#[cfg(feature = "strict")]
use officekit_markup::spreadsheet::{
    AutoFilterColumn, AutoFilterColumnContent, AutoFilterSettings, DynamicFilter, FileSharing,
    Filters, NumberFormatId, PivotCacheDefinition, PivotCacheField,
    PivotCacheSourceDescriptionContent, PivotCacheType, PivotFilterTypes, PivotTableAdvancedFilter,
    PivotTableDefinition, StDynamicFilterType, Table, TableColumn, UnsignedShortHex,
    WorkbookProperties, WorksheetPivotCacheSource,
};
#[cfg(all(feature = "strict", feature = "transitional"))]
use officekit_markup::spreadsheet::{Cell, CellType, CellValue, Row};
#[cfg(feature = "strict")]
use officekit_strict::iso_date_time;
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "transitional")]
use officekit_xml::ready::ready_now;
#[cfg(feature = "strict")]
use officekit_xml::write::WriteOptions;
#[cfg(feature = "transitional")]
use os_shim::mock::MemorySystem;

use super::Xlsx;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// Turn `workbookPr/@date1904` of `xlsx`'s workbook to `date1904` when it names a value.
#[cfg(feature = "strict")]
fn date_system(xlsx: &mut Xlsx, date1904: Option<&str>) {
    if let Some(value) = date1904 {
        let mut workbook = xlsx.workbook().unwrap();
        let mut properties = WorkbookProperties::default();
        properties.date1904 = Some(value.to_owned());
        workbook.properties = Some(Box::new(properties));
        xlsx.set_workbook(&workbook).unwrap();
    }
}

/// Store `root` as the part `part` of `xlsx`, in the file's class.
#[cfg(feature = "strict")]
fn put<R>(xlsx: &mut Xlsx, part: &PartName, content_type: &str, root: &R)
where
    R: PartRoot,
{
    let class = xlsx.conformance_class();
    store(
        xlsx.package_mut(),
        part,
        content_type,
        root,
        class,
        WriteOptions::default(),
    )
    .unwrap();
}

/// A filter of the cells A1 to A3 whose first column keeps the dates up to the serial date-time
/// 46309.
#[cfg(feature = "strict")]
fn filtered() -> AutoFilterSettings {
    let mut dynamic = DynamicFilter::default();
    dynamic.r#type = Some(StDynamicFilterType::from("aboveAverage".to_owned()));
    dynamic.val = Some("46000".to_owned());
    dynamic.max_val = Some("46309".to_owned());
    let mut column = AutoFilterColumn::default();
    column.col_id = Some("0".to_owned());
    column.content = Some(Box::new(AutoFilterColumnContent::DynamicFilter(dynamic)));
    let mut settings = AutoFilterSettings::default();
    settings.r#ref = Some("A1:A3".to_owned());
    settings.autofilter_column = vec![column];
    settings
}

/// The dynamic filter of the first column `settings` filters.
#[cfg(feature = "strict")]
fn dynamic_filter(settings: &AutoFilterSettings) -> Option<&DynamicFilter> {
    let content = settings.autofilter_column.first()?.content.as_deref()?;
    if let AutoFilterColumnContent::DynamicFilter(found) = content {
        Some(found)
    } else {
        None
    }
}

/// The maximum `filter` keeps, as an ISO date-time and as a serial date-time.
#[cfg(feature = "strict")]
fn maximum(filter: &DynamicFilter) -> (Option<&str>, Option<&str>) {
    (filter.max_val_iso.as_deref(), filter.max_val.as_deref())
}

/// A table of one column over A1 to A3, filtered.
#[cfg(feature = "strict")]
fn filtered_table() -> Table {
    let mut column = TableColumn::default();
    column.id = Some("1".to_owned());
    column.name = Some("Day".to_owned());
    let mut table = Table::default();
    table.id = Some("1".to_owned());
    table.name = Some("T".to_owned());
    table.display_name = Some("T".to_owned());
    table.r#ref = Some("A1:A3".to_owned());
    table.autofilter_settings = Some(Box::new(filtered()));
    table.columns.count = Some("1".to_owned());
    table.columns.table_column = vec![column];
    table
}

/// A pivot table whose one advanced filter is filtered.
#[cfg(feature = "strict")]
fn filtered_pivot_table() -> PivotTableDefinition {
    let mut filter = PivotTableAdvancedFilter::default();
    filter.fld = Some("0".to_owned());
    filter.r#type = Some(PivotFilterTypes::from("dateBetween".to_owned()));
    filter.id = Some("1".to_owned());
    *filter.autofilter_settings = filtered();
    let mut filters = Filters::default();
    filters.count = Some("1".to_owned());
    filters.pivottable_advanced_filter = vec![filter];
    let mut definition = PivotTableDefinition::default();
    definition.name = Some("P".to_owned());
    definition.cache_id = Some("1".to_owned());
    definition.data_caption = Some("Values".to_owned());
    let location = &mut definition.pivottable_location;
    location.r#ref = Some("A1:B3".to_owned());
    location.first_header_row = Some("1".to_owned());
    location.first_data_row = Some("1".to_owned());
    location.first_data_col = Some("1".to_owned());
    definition.filters = Some(Box::new(filters));
    definition
}

/// A pivot cache of one field over Sheet1's A1 to A3, refreshed at the serial date-time 46309.
#[cfg(feature = "strict")]
fn pivot_cache() -> PivotCacheDefinition {
    let mut source = WorksheetPivotCacheSource::default();
    source.r#ref = Some("A1:A3".to_owned());
    source.sheet = Some("Sheet1".to_owned());
    let mut field = PivotCacheField::default();
    field.name = Some("Day".to_owned());
    field.num_fmt_id = Some(NumberFormatId::from("0".to_owned()));
    field.shared_items = Some(Box::default());
    let mut definition = PivotCacheDefinition::default();
    definition.refreshed_date = Some("46309".to_owned());
    definition.created_version = Some("8".to_owned());
    definition.record_count = Some("0".to_owned());
    let description = &mut definition.pivotcache_source_description;
    description.r#type = Some(PivotCacheType::from("worksheet".to_owned()));
    description.content = Some(Box::new(
        PivotCacheSourceDescriptionContent::WorksheetPivotCacheSource(source),
    ));
    definition.pivotcache_fields.count = Some("1".to_owned());
    definition.pivotcache_fields.pivotcache_field = vec![field];
    definition
}

/// A new workbook, given `workbookPr/@date1904` when `date1904` names a value, its sheet
/// filtered and a pivot cache definition refreshed at a serial date-time, saved Strict: the
/// dynamic filter and the pivot cache definition read back from the Strict file.
#[cfg(feature = "strict")]
fn saved_strict(date1904: Option<&str>) -> (DynamicFilter, PivotCacheDefinition) {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    date_system(&mut xlsx, date1904);
    let sheet = name("/xl/worksheets/sheet1.xml");
    let mut filtered_sheet =
        worksheet::load(xlsx.package(), &sheet, OpenOptions::default()).unwrap();
    filtered_sheet.autofilter_settings = Some(Box::new(filtered()));
    put(&mut xlsx, &sheet, worksheet::CONTENT_TYPE, &filtered_sheet);
    let cache = name("/xl/pivotCache/pivotCacheDefinition1.xml");
    put(
        &mut xlsx,
        &cache,
        pivot_table_cache_definition::CONTENT_TYPE,
        &pivot_cache(),
    );
    let main = xlsx.workbook_part().clone();
    xlsx.package_mut()
        .relate(
            Some(&main),
            &cache,
            RelationshipType::PIVOT_CACHE_DEFINITION.transitional,
        )
        .unwrap();
    let strict = Xlsx::from_bytes(xlsx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    let saved = worksheet::load(strict.package(), &sheet, OpenOptions::default()).unwrap();
    let filter = dynamic_filter(saved.autofilter_settings.as_deref().unwrap())
        .unwrap()
        .clone();
    let definition =
        pivot_table_cache_definition::load(strict.package(), &cache, OpenOptions::default())
            .unwrap();
    (filter, definition)
}

/// A Strict workbook whose sheet holds one date cell, its `workbookPr/@date1904` on when
/// `date1904`.
#[cfg(all(feature = "strict", feature = "transitional"))]
fn strict_dated(date1904: bool) -> Xlsx {
    let strict = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_strict()
        .unwrap();
    let mut xlsx = Xlsx::from_bytes(strict, OpenOptions::default()).unwrap();
    assert_eq!(xlsx.conformance_class(), ConformanceClass::Strict);
    date_system(&mut xlsx, date1904.then_some("1"));
    let mut value = CellValue::default();
    value.value = "2024-03-01T12:00:00".to_owned();
    let mut cell = Cell::default();
    cell.r = Some("A1".to_owned());
    cell.s = Some("1".to_owned());
    cell.t = Some(CellType::from("d".to_owned()));
    cell.value = Some(Box::new(value));
    let mut row = Row::default();
    row.r = Some("1".to_owned());
    row.cell = vec![cell];
    let sheet = name("/xl/worksheets/sheet1.xml");
    let mut dated = worksheet::load(xlsx.package(), &sheet, OpenOptions::default()).unwrap();
    dated.sheet_data.row = vec![row];
    put(&mut xlsx, &sheet, worksheet::CONTENT_TYPE, &dated);
    xlsx
}

#[test]
fn a_new_workbook_holds_one_empty_sheet_related_as_rid1() {
    let xlsx = Xlsx::new(SpreadsheetKind::MacroEnabledWorkbook).unwrap();
    let main = name("/xl/workbook.xml");
    let sheet = name("/xl/worksheets/sheet1.xml");
    let package = xlsx.package();
    assert_eq!(package.part_names().unwrap(), [main.clone(), sheet.clone()]);
    assert_eq!(xlsx.workbook_part(), &main);
    assert_eq!(xlsx.kind(), SpreadsheetKind::MacroEnabledWorkbook);
    assert_eq!(xlsx.conformance_class(), ConformanceClass::Transitional);
    assert_eq!(
        package.content_type(&main),
        Some(SpreadsheetKind::MacroEnabledWorkbook.main_content_type())
    );
    assert_eq!(package.content_type(&sheet), Some(worksheet::CONTENT_TYPE));
    let related = package.related(Some(&main)).unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0].id, "rId1");
    assert_eq!(related[0].target, Target::Part(sheet));
    assert_eq!(
        related[0].type_uri,
        RelationshipType::WORKSHEET.transitional
    );
    let sheets = xlsx.workbook().unwrap().sheets.sheet_information;
    assert_eq!(sheets.len(), 1);
    assert_eq!(sheets[0].name.as_deref(), Some("Sheet1"));
    assert_eq!(sheets[0].sheet_id.as_deref(), Some("1"));
    assert_eq!(sheets[0].id.as_deref(), Some("rId1"));
}

#[cfg(feature = "transitional")]
#[test]
fn the_five_kinds_reopen_with_their_kind() {
    for kind in SpreadsheetKind::ALL {
        let bytes = Xlsx::new(kind).unwrap().package().to_bytes().unwrap();
        let reopened = Xlsx::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert_eq!(reopened.kind(), kind);
    }
}

#[cfg(feature = "transitional")]
#[test]
fn options_are_those_the_file_was_opened_with() {
    let bytes = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .package()
        .to_bytes()
        .unwrap();
    let mut options = OpenOptions::default();
    options.markup_compatibility = MarkupCompatibility::Process {
        target: OfficeVersion::Office2016,
    };
    assert_eq!(Xlsx::from_bytes(bytes, options).unwrap().options(), options);
}

#[cfg(feature = "transitional")]
#[test]
fn create_writes_the_new_workbook_through_the_system() {
    let system = MemorySystem::new().with_dir("/out").unwrap();
    let path = Path::new("/out/new.xltx");
    ready_now(Xlsx::create(&system, path, SpreadsheetKind::Template)).unwrap();
    let opened = ready_now(Xlsx::open(&system, path, OpenOptions::default())).unwrap();
    assert_eq!(opened.kind(), SpreadsheetKind::Template);
}

#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_strict_date_cell_saved_transitional_becomes_a_number_cell_with_one_warning() {
    for (date1904, serial) in [(false, "45352.5"), (true, "43890.5")] {
        let (bytes, warnings) = strict_dated(date1904).to_bytes_transitional().unwrap();
        let sheet = name("/xl/worksheets/sheet1.xml");
        assert_eq!(
            warnings,
            [SaveWarning::DateCellConverted {
                cell: "A1".to_owned(),
                date: "2024-03-01T12:00:00".to_owned(),
                part: sheet.clone(),
                serial: serial.to_owned(),
            }]
        );
        let transitional = Xlsx::from_bytes(bytes, OpenOptions::default()).unwrap();
        assert_eq!(
            transitional.conformance_class(),
            ConformanceClass::Transitional
        );
        let saved =
            worksheet::load(transitional.package(), &sheet, OpenOptions::default()).unwrap();
        let cells: Vec<_> = saved
            .sheet_data
            .row
            .iter()
            .flat_map(|row| &row.cell)
            .map(|cell| {
                (
                    cell.r.as_deref(),
                    cell.s.as_deref(),
                    cell.t.as_ref().map(|kind| kind.0.as_str()),
                    cell.value.as_ref().map(|held| held.value.as_str()),
                )
            })
            .collect();
        assert_eq!(cells, [(Some("A1"), Some("1"), None, Some(serial))]);
        let written =
            String::from_utf8(transitional.package().part_bytes(&sheet).unwrap()).unwrap();
        assert!(
            written.contains("http://schemas.openxmlformats.org/spreadsheetml/2006/main"),
            "{written}"
        );
    }
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_save_writes_iso_date_times_in_the_workbook_date_system() {
    for (date1904, system_1904) in [(Some("1"), true), (None, false)] {
        let (filter, definition) = saved_strict(date1904);
        let iso = iso_date_time("46309", system_1904).unwrap();
        assert_eq!(
            (
                definition.refreshed_date_iso.as_deref(),
                definition.refreshed_date.as_deref()
            ),
            (Some(iso.as_str()), None)
        );
        assert_eq!(maximum(&filter), (Some(iso.as_str()), None));
    }
    assert_ne!(iso_date_time("46309", true), iso_date_time("46309", false));
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_save_writes_table_and_pivot_table_filter_dates_in_the_workbook_date_system() {
    for (date1904, system_1904) in [(Some("1"), true), (None, false)] {
        let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
        date_system(&mut xlsx, date1904);
        let sheet = name("/xl/worksheets/sheet1.xml");
        let table = name("/xl/tables/table1.xml");
        put(
            &mut xlsx,
            &table,
            table_definition::CONTENT_TYPE,
            &filtered_table(),
        );
        let pivot = name("/xl/pivotTables/pivotTable1.xml");
        put(
            &mut xlsx,
            &pivot,
            pivot_table::CONTENT_TYPE,
            &filtered_pivot_table(),
        );
        for (part, relationship_type) in [
            (&table, RelationshipType::TABLE),
            (&pivot, RelationshipType::PIVOT_TABLE),
        ] {
            xlsx.package_mut()
                .relate(Some(&sheet), part, relationship_type.transitional)
                .unwrap();
        }
        let strict =
            Xlsx::from_bytes(xlsx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
        let iso = iso_date_time("46309", system_1904).unwrap();
        let saved_table =
            table_definition::load(strict.package(), &table, OpenOptions::default()).unwrap();
        let saved_pivot =
            pivot_table::load(strict.package(), &pivot, OpenOptions::default()).unwrap();
        let pivot_filters = saved_pivot.filters.as_deref().unwrap();
        for (part, settings) in [
            (&table, saved_table.autofilter_settings.as_deref().unwrap()),
            (
                &pivot,
                &*pivot_filters
                    .pivottable_advanced_filter
                    .first()
                    .unwrap()
                    .autofilter_settings,
            ),
        ] {
            assert_eq!(
                maximum(dynamic_filter(settings).unwrap()),
                (Some(iso.as_str()), None),
                "{part:?}"
            );
        }
    }
}

#[cfg(feature = "strict")]
#[test]
fn a_new_workbook_saved_strict_reopens_strict() {
    let strict = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_strict()
        .unwrap();
    let xlsx = Xlsx::from_bytes(strict, OpenOptions::default()).unwrap();
    assert_eq!(xlsx.conformance_class(), ConformanceClass::Strict);
    let workbook =
        String::from_utf8(xlsx.package().part_bytes(xlsx.workbook_part()).unwrap()).unwrap();
    assert!(workbook.contains("conformance=\"strict\""), "{workbook}");
    assert!(
        workbook.contains("xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\""),
        "{workbook}"
    );
    let related = xlsx.package().related(Some(xlsx.workbook_part())).unwrap();
    assert_eq!(related[0].type_uri, RelationshipType::WORKSHEET.strict);
}

#[cfg(feature = "strict")]
#[test]
fn strict_refusals_names_a_workbook_s_legacy_password() {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    assert_eq!(xlsx.strict_refusals().unwrap(), []);
    let mut sharing = FileSharing::default();
    sharing.reservation_password = Some(UnsignedShortHex::from("ABCD".to_owned()));
    let mut workbook = xlsx.workbook().unwrap();
    workbook.file_sharing = Some(Box::new(sharing));
    xlsx.set_workbook(&workbook).unwrap();
    assert_eq!(
        xlsx.strict_refusals().unwrap(),
        [crate::StrictRefusal {
            count: 1,
            part: name("/xl/workbook.xml"),
            prefix: "x",
            tag: "fileSharing/@reservationPassword",
        }]
    );
    xlsx.to_bytes_strict().unwrap_err();
}
