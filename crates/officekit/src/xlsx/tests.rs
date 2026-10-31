#[cfg(feature = "transitional")]
use std::path::Path;

use crate::parts::kinds::SpreadsheetKind;
#[cfg(feature = "strict")]
use crate::parts::pivot_table_cache_definition;
use crate::parts::worksheet;
#[cfg(feature = "strict")]
use crate::parts::{pivot_table, table_definition};
#[cfg(all(feature = "strict", feature = "transitional"))]
use crate::transitional::dates::SaveWarning;
#[cfg(feature = "strict")]
use officekit_markup::spreadsheet::WorkbookProperties;
#[cfg(feature = "strict")]
use officekit_strict::iso_date_time;
use officekit_xml::conformance::ConformanceClass;
#[cfg(feature = "strict")]
use officekit_xml::element::Element;
#[cfg(feature = "transitional")]
use officekit_xml::mc::MarkupCompatibility;
#[cfg(feature = "strict")]
use officekit_xml::name::Namespace;
#[cfg(feature = "transitional")]
use officekit_xml::name::OfficeVersion;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_xml::packaging::package::OpenOptions;
#[cfg(feature = "strict")]
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};
#[cfg(feature = "strict")]
use officekit_xml::parse::parse;
#[cfg(feature = "transitional")]
use officekit_xml::ready::ready_now;
#[cfg(feature = "transitional")]
use os_shim::mock::MemorySystem;

use super::Xlsx;

/// A Transitional worksheet whose filter keeps the dates up to a serial date-time.
#[cfg(feature = "strict")]
const FILTERED_SHEET: &str = "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData/><autoFilter ref=\"A1:A3\"><filterColumn colId=\"0\"><dynamicFilter type=\"aboveAverage\" val=\"46000\" maxVal=\"46309\"/></filterColumn></autoFilter></worksheet>";

/// A Transitional table definition whose filter keeps the dates up to a serial date-time.
#[cfg(feature = "strict")]
const FILTERED_TABLE: &str = "<table xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" id=\"1\" name=\"T\" displayName=\"T\" ref=\"A1:A3\"><autoFilter ref=\"A1:A3\"><filterColumn colId=\"0\"><dynamicFilter type=\"aboveAverage\" val=\"46000\" maxVal=\"46309\"/></filterColumn></autoFilter><tableColumns count=\"1\"><tableColumn id=\"1\" name=\"Day\"/></tableColumns></table>";

/// A Transitional pivot table whose advanced filter keeps the dates up to a serial date-time.
#[cfg(feature = "strict")]
const FILTERED_PIVOT_TABLE: &str = "<pivotTableDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"P\" cacheId=\"1\" dataCaption=\"Values\"><location ref=\"A1:B3\" firstHeaderRow=\"1\" firstDataRow=\"1\" firstDataCol=\"1\"/><filters count=\"1\"><filter fld=\"0\" type=\"dateBetween\" id=\"1\"><autoFilter ref=\"A1:A3\"><filterColumn colId=\"0\"><dynamicFilter type=\"aboveAverage\" val=\"46000\" maxVal=\"46309\"/></filterColumn></autoFilter></filter></filters></pivotTableDefinition>";

/// A Transitional pivot cache definition refreshed at a serial date-time.
#[cfg(feature = "strict")]
const PIVOT_CACHE: &str = "<pivotCacheDefinition xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" refreshedDate=\"46309\" createdVersion=\"8\" recordCount=\"0\"><cacheSource type=\"worksheet\"><worksheetSource ref=\"A1:A3\" sheet=\"Sheet1\"/></cacheSource><cacheFields count=\"1\"><cacheField name=\"Day\" numFmtId=\"0\"><sharedItems/></cacheField></cacheFields></pivotCacheDefinition>";

/// A Strict worksheet holding one date cell.
#[cfg(all(feature = "strict", feature = "transitional"))]
const DATED_SHEET: &str = "<worksheet xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\"><sheetData><row r=\"1\"><c r=\"A1\" s=\"1\" t=\"d\"><v>2024-03-01T12:00:00</v></c></row></sheetData></worksheet>";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// The first element below `element`, itself included, named `local`.
#[cfg(feature = "strict")]
fn find<'tree>(element: &'tree Element, local: &str) -> Option<&'tree Element> {
    if element.name.local == local {
        return Some(element);
    }
    element
        .child_elements()
        .find_map(|child| find(child, local))
}

/// A new workbook, given `workbookPr/@date1904` when `date1904` names a value, its sheet
/// filtered by `maxVal` and a pivot cache definition refreshed at `refreshedDate`, saved Strict:
/// the dynamic filter and the pivot cache definition as Strict writes them.
#[cfg(feature = "strict")]
fn saved_strict(date1904: Option<&str>) -> (Element, Element) {
    let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
    if let Some(value) = date1904 {
        let mut workbook = xlsx.workbook().unwrap();
        let mut properties = WorkbookProperties::default();
        properties.date1904 = Some(value.to_owned());
        workbook.properties = Some(Box::new(properties));
        xlsx.set_workbook(&workbook).unwrap();
    }
    let sheet = name("/xl/worksheets/sheet1.xml");
    let main = xlsx.workbook_part().clone();
    xlsx.package_mut()
        .set_part_bytes(&sheet, worksheet::CONTENT_TYPE, FILTERED_SHEET.as_bytes())
        .unwrap();
    let (cache, _) = xlsx
        .package_mut()
        .add_part(
            Some(&main),
            RelationshipType::PIVOT_CACHE_DEFINITION.transitional,
            pivot_table_cache_definition::CONTENT_TYPE,
            "/xl/pivotCache/pivotCacheDefinition{}.xml",
            PIVOT_CACHE.as_bytes(),
        )
        .unwrap();
    let strict = Package::from_bytes(xlsx.to_bytes_strict().unwrap()).unwrap();
    let filter = parse(&strict.part_bytes(&sheet).unwrap()).unwrap().root;
    let definition = parse(&strict.part_bytes(&cache).unwrap()).unwrap().root;
    (find(&filter, "dynamicFilter").unwrap().clone(), definition)
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
    if date1904 {
        let mut workbook = xlsx.workbook().unwrap();
        let mut properties = WorkbookProperties::default();
        properties.date1904 = Some("1".to_owned());
        workbook.properties = Some(Box::new(properties));
        xlsx.set_workbook(&workbook).unwrap();
    }
    xlsx.package_mut()
        .set_part_bytes(
            &name("/xl/worksheets/sheet1.xml"),
            worksheet::CONTENT_TYPE,
            DATED_SHEET.as_bytes(),
        )
        .unwrap();
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
        let written =
            String::from_utf8(transitional.package().part_bytes(&sheet).unwrap()).unwrap();
        assert!(
            written.contains(&format!("<c r=\"A1\" s=\"1\"><v>{serial}</v></c>")),
            "{written}"
        );
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
            definition.attribute(&Namespace::None, "refreshedDateIso"),
            Some(iso.as_str())
        );
        assert_eq!(
            definition.attribute(&Namespace::None, "refreshedDate"),
            None
        );
        assert_eq!(
            filter.attribute(&Namespace::None, "maxValIso"),
            Some(iso.as_str())
        );
        assert_eq!(filter.attribute(&Namespace::None, "maxVal"), None);
    }
    assert_ne!(iso_date_time("46309", true), iso_date_time("46309", false));
}

#[cfg(feature = "strict")]
#[test]
fn a_strict_save_writes_table_and_pivot_table_filter_dates_in_the_workbook_date_system() {
    for (date1904, system_1904) in [(Some("1"), true), (None, false)] {
        let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook).unwrap();
        if let Some(value) = date1904 {
            let mut workbook = xlsx.workbook().unwrap();
            let mut properties = WorkbookProperties::default();
            properties.date1904 = Some(value.to_owned());
            workbook.properties = Some(Box::new(properties));
            xlsx.set_workbook(&workbook).unwrap();
        }
        let sheet = name("/xl/worksheets/sheet1.xml");
        let (table, _) = xlsx
            .package_mut()
            .add_part(
                Some(&sheet),
                RelationshipType::TABLE.transitional,
                table_definition::CONTENT_TYPE,
                "/xl/tables/table{}.xml",
                FILTERED_TABLE.as_bytes(),
            )
            .unwrap();
        let (pivot, _) = xlsx
            .package_mut()
            .add_part(
                Some(&sheet),
                RelationshipType::PIVOT_TABLE.transitional,
                pivot_table::CONTENT_TYPE,
                "/xl/pivotTables/pivotTable{}.xml",
                FILTERED_PIVOT_TABLE.as_bytes(),
            )
            .unwrap();
        let strict = Package::from_bytes(xlsx.to_bytes_strict().unwrap()).unwrap();
        let iso = iso_date_time("46309", system_1904).unwrap();
        for part in [table, pivot] {
            let root = parse(&strict.part_bytes(&part).unwrap()).unwrap().root;
            let filter = find(&root, "dynamicFilter").unwrap();
            assert_eq!(
                filter.attribute(&Namespace::None, "maxValIso"),
                Some(iso.as_str()),
                "{part:?}"
            );
            assert_eq!(filter.attribute(&Namespace::None, "maxVal"), None);
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
