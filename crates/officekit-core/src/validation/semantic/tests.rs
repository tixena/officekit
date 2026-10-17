use alloc::collections::BTreeSet;

use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_xml::name::OfficeVersion;
use regex::Regex;

use super::constraint::Constraint;
use super::registry::registrations;
use crate::validation::error::{ErrorCode, Finding};
use crate::validation::generated::{SCHEMA_TYPES, SEMANTIC_CONSTRAINTS, type_id};
use crate::validation::validator::Validator;

const FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const FOOTNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
const HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const NAMESPACES: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
     xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
     xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
     xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14\"";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const SETTINGS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
const SHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
const SPREADSHEET: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const SPREADSHEET_STYLES: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml";
const TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
const WORKBOOK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";

/// One part of a package: its name, content type, the part it is related
/// from (`None`: the package), the last segment of its relationship type, and
/// its XML.
type Part<'part> = (
    &'part str,
    &'part str,
    Option<&'part str>,
    &'part str,
    String,
);

/// The path of `steps` under `/w:document[1]/w:body[1]` in the main part.
fn in_body(steps: &str) -> String {
    format!("/word/document.xml /w:document[1]/w:body[1]{steps}")
}

/// A package of `parts`, each related from its source in order.
fn package(parts: &[Part<'_>]) -> Package {
    let mut package = Package::new();
    for (name, content_type, source, relationship, xml) in parts {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, xml.as_bytes())
            .unwrap();
        let from = source.map(|found| PartName::new(found).unwrap());
        package
            .relate(
                from.as_ref(),
                &part,
                &format!("{RELATIONSHIPS}{relationship}"),
            )
            .unwrap();
    }
    package
}

/// Each semantic error at `target`: its code, and its part and path.
fn semantic(package: &Package, target: OfficeVersion) -> Vec<(ErrorCode, String)> {
    Validator::new(target)
        .with_max_errors(0)
        .validate(package)
        .into_iter()
        .filter(|error| {
            error.code.kind() == Finding::Semantic
                || matches!(
                    error.code,
                    ErrorCode::AttributeCannotBeOmitted
                        | ErrorCode::AttributeValueOutsideConstraint
                )
        })
        .map(|error| {
            let part = error
                .part
                .as_ref()
                .map_or_else(String::new, |name| name.as_str().to_owned());
            (error.code, format!("{part} {}", error.path_text()))
        })
        .collect()
}

/// A spreadsheet whose one worksheet holds `sheet`, with a styles part of one
/// cell format, and a table part of `columns` when given.
fn spreadsheet(sheet: &str, columns: Option<&str>) -> Package {
    let mut parts: Vec<Part<'_>> = vec![
        (
            "/xl/workbook.xml",
            WORKBOOK,
            None,
            "officeDocument",
            format!(
                "<workbook xmlns=\"{SPREADSHEET}\" xmlns:r=\"{RELATIONSHIPS}\">\
                 <sheets><sheet name=\"S\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>"
            ),
        ),
        (
            "/xl/worksheets/sheet1.xml",
            SHEET,
            Some("/xl/workbook.xml"),
            "worksheet",
            format!("<worksheet xmlns=\"{SPREADSHEET}\">{sheet}</worksheet>"),
        ),
        (
            "/xl/styles.xml",
            SPREADSHEET_STYLES,
            Some("/xl/workbook.xml"),
            "styles",
            format!(
                "<styleSheet xmlns=\"{SPREADSHEET}\"><cellXfs count=\"1\">\
                 <xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/></cellXfs></styleSheet>"
            ),
        ),
    ];
    if let Some(table) = columns {
        parts.push((
            "/xl/tables/table1.xml",
            TABLE,
            Some("/xl/worksheets/sheet1.xml"),
            "table",
            format!(
                "<table xmlns=\"{SPREADSHEET}\" id=\"1\" name=\"T\" displayName=\"T\" ref=\"A1:A2\">\
                 <tableColumns count=\"1\">{table}</tableColumns></table>"
            ),
        ));
    }
    package(&parts)
}

/// A Word document whose body holds `body`, its main part related to each of
/// `parts`: name, content type, the last segment of the relationship type,
/// and XML.
fn word(body: &str, parts: &[(&str, &str, &str, String)]) -> Package {
    let mut all: Vec<Part<'_>> = vec![(
        "/word/document.xml",
        MAIN,
        None,
        "officeDocument",
        format!("<w:document {NAMESPACES}><w:body>{body}</w:body></w:document>"),
    )];
    for (name, content_type, relationship, xml) in parts {
        all.push((
            name,
            content_type,
            Some("/word/document.xml"),
            relationship,
            xml.clone(),
        ));
    }
    package(&all)
}

/// The XML of a Word part whose root, `w:root`, holds `content`.
fn word_part(root: &str, content: &str) -> String {
    format!("<w:{root} {NAMESPACES}>{content}</w:{root}>")
}

#[test]
fn a_value_one_attribute_must_not_take_is_outside_the_constraint() {
    let package = word("<w:p><w:pPr><w:divId w:val=\"0\"/></w:pPr></w:p>", &[]);
    assert_eq!(
        semantic(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            in_body("/w:p[1]/w:pPr[1]/w:divId[1]")
        )]
    );
    let allowed = word("<w:p><w:pPr><w:divId w:val=\"7\"/></w:pPr></w:p>", &[]);
    assert_eq!(semantic(&allowed, OfficeVersion::Microsoft365), []);
}

#[test]
fn a_hexadecimal_value_is_compared_by_its_number() {
    let settings = word_part("settings", "<w:stylePaneFormatFilter w:val=\"0040\"/>");
    let package = word(
        "<w:p/>",
        &[("/word/settings.xml", SETTINGS, "settings", settings)],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2016),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            "/word/settings.xml /w:settings[1]/w:stylePaneFormatFilter[1]".to_owned()
        )]
    );
}

#[test]
fn a_value_out_of_range_is_outside_the_constraint() {
    let package = word(
        "<w:p><w:pPr><w:outlineLvl w:val=\"10\"/></w:pPr></w:p>\
         <w:p w14:paraId=\"80000001\" w14:textId=\"7FFFFFFF\"/>",
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2010),
        [
            (
                ErrorCode::AttributeValueOutsideConstraint,
                in_body("/w:p[1]/w:pPr[1]/w:outlineLvl[1]")
            ),
            (
                ErrorCode::AttributeValueOutsideConstraint,
                in_body("/w:p[2]")
            ),
        ]
    );
}

#[test]
fn a_value_too_long_is_outside_the_constraint() {
    let longest = word(
        &format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{}\"/></w:pPr></w:p>",
            "s".repeat(253)
        ),
        &[],
    );
    assert_eq!(semantic(&longest, OfficeVersion::Office2013), []);
    let package = word(
        &format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{}\"/></w:pPr></w:p>",
            "s".repeat(254)
        ),
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            in_body("/w:p[1]/w:pPr[1]/w:pStyle[1]")
        )]
    );
}

#[test]
fn a_value_off_its_pattern_is_outside_the_constraint() {
    let settings = word_part("settings", "<w:decimalSymbol w:val=\"ab\"/>");
    let package = word(
        "<w:p/>",
        &[("/word/settings.xml", SETTINGS, "settings", settings)],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2019),
        [(
            ErrorCode::AttributeValueOutsideConstraint,
            "/word/settings.xml /w:settings[1]/w:decimalSymbol[1]".to_owned()
        )]
    );
}

#[test]
fn a_bookmark_id_held_twice_is_reported_once_on_the_last() {
    let package = word(
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"a\"/></w:p>\
         <w:p><w:bookmarkStart w:id=\"1\" w:name=\"b\"/></w:p>\
         <w:p><w:bookmarkStart w:id=\"2\" w:name=\"c\"/></w:p>",
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::DuplicateAttributeValue,
            in_body("/w:p[2]/w:bookmarkStart[1]")
        )]
    );
}

#[test]
fn a_footnote_reference_to_a_missing_footnote_is_reported() {
    let footnotes = word_part(
        "footnotes",
        "<w:footnote w:id=\"-1\"><w:p/></w:footnote><w:footnote w:id=\"0\"><w:p/></w:footnote>",
    );
    let package = word(
        "<w:p><w:r><w:footnoteReference w:id=\"9\"/></w:r>\
         <w:r><w:footnoteReference w:id=\"0\"/></w:r></w:p>",
        &[("/word/footnotes.xml", FOOTNOTES, "footnotes", footnotes)],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::ReferencedElementNotFound,
            in_body("/w:p[1]/w:r[1]/w:footnoteReference[1]")
        )]
    );
}

#[test]
fn an_id_naming_no_relationship_is_reported() {
    let package = word(
        "<w:p><w:hyperlink r:id=\"rId99\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>",
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2016),
        [(
            ErrorCode::RelationshipNotFound,
            in_body("/w:p[1]/w:hyperlink[1]")
        )]
    );
}

#[test]
fn a_relationship_whose_target_names_no_part_hides_no_other_id() {
    let body = "<w:p><w:hyperlink r:id=\"rId1\"><w:r><w:t>x</w:t></w:r></w:hyperlink></w:p>";
    let mut written = word(body, &[]);
    let main = PartName::new("/word/document.xml").unwrap();
    written
        .set_part_bytes(
            &PartName::new(&main.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!(
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
                 <Relationship Id=\"rId1\" Type=\"{RELATIONSHIPS}hyperlink\" Target=\"https://example.org/\" TargetMode=\"External\"/>\
                 <Relationship Id=\"rId2\" Type=\"{RELATIONSHIPS}image\" Target=\"media/\"/>\
                 </Relationships>"
            )
            .as_bytes(),
        )
        .unwrap();
    let package = Package::from_bytes(written.to_bytes().unwrap()).unwrap();
    assert_eq!(semantic(&package, OfficeVersion::Microsoft365), []);
}

#[test]
fn a_header_reference_naming_a_footer_relationship_is_reported() {
    let parts = [
        (
            "/word/footer1.xml",
            FOOTER,
            "footer",
            word_part("ftr", "<w:p/>"),
        ),
        (
            "/word/header1.xml",
            HEADER,
            "header",
            word_part("hdr", "<w:p/>"),
        ),
    ];
    let package = word(
        "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
         <w:headerReference w:type=\"first\" r:id=\"rId2\"/></w:sectPr>",
        &parts,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2021),
        [(
            ErrorCode::WrongRelationshipType,
            in_body("/w:sectPr[1]/w:headerReference[1]")
        )]
    );
}

#[test]
fn a_required_attribute_left_out_cannot_be_omitted() {
    let package = word(
        "<w:tbl><w:tblPr><w:tblCellMar><w:bottom w:type=\"dxa\"/></w:tblCellMar></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"10\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
        &[],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2010),
        [(
            ErrorCode::AttributeCannotBeOmitted,
            in_body("/w:tbl[1]/w:tblPr[1]/w:tblCellMar[1]/w:bottom[1]")
        )]
    );
}

#[test]
fn a_value_the_other_attribute_does_not_allow_conflicts_with_it() {
    let settings = word_part(
        "settings",
        "<w:compat><w:compatSetting w:name=\"compatibilityMode\" \
         w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"13\"/></w:compat>",
    );
    let package = word(
        "<w:p/>",
        &[("/word/settings.xml", SETTINGS, "settings", settings)],
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::AttributeValueConflictsWithAnother,
            "/word/settings.xml /w:settings[1]/w:compat[1]/w:compatSetting[1]".to_owned()
        )]
    );
}

#[test]
fn two_mutually_exclusive_attributes_are_reported() {
    let package = spreadsheet(
        "<sheetPr><tabColor rgb=\"FF0000FF\" theme=\"1\"/></sheetPr><sheetData/>",
        None,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::MutuallyExclusiveAttributes,
            "/xl/worksheets/sheet1.xml /worksheet[1]/sheetPr[1]/tabColor[1]".to_owned()
        )]
    );
}

#[test]
fn a_minimum_above_its_maximum_exceeds_it() {
    let package = spreadsheet(
        "<cols><col min=\"5\" max=\"2\" width=\"9\"/></cols><sheetData/>",
        None,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2016),
        [(
            ErrorCode::AttributeValueExceedsAnother,
            "/xl/worksheets/sheet1.xml /worksheet[1]/cols[1]/col[1]".to_owned()
        )]
    );
}

#[test]
fn a_shared_formula_requires_its_index() {
    let package = spreadsheet(
        "<sheetData><row r=\"1\"><c r=\"B1\"><f t=\"shared\" ref=\"B1:B2\">A1</f></c></row></sheetData>",
        None,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2019),
        [(
            ErrorCode::AttributeRequiredForValue,
            "/xl/worksheets/sheet1.xml /worksheet[1]/sheetData[1]/row[1]/c[1]/f[1]".to_owned()
        )]
    );
}

#[test]
fn a_source_range_must_be_absent_unless_the_source_is_a_range() {
    let package = spreadsheet(
        "<sheetData/><webPublishItems count=\"1\"><webPublishItem id=\"1\" divId=\"d\" \
         sourceType=\"sheet\" sourceRef=\"A1\" destinationFile=\"f.htm\"/></webPublishItems>",
        None,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2021),
        [(
            ErrorCode::AttributeMustBeAbsentUnlessValue,
            "/xl/worksheets/sheet1.xml /worksheet[1]/webPublishItems[1]/webPublishItem[1]"
                .to_owned()
        )]
    );
}

#[test]
fn a_totals_label_must_be_absent_for_a_custom_function() {
    let package = spreadsheet(
        "<sheetData/>",
        Some(
            "<tableColumn id=\"1\" name=\"C\" totalsRowFunction=\"custom\" totalsRowLabel=\"x\"/>",
        ),
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Office2010),
        [(
            ErrorCode::AttributeMustBeAbsentForValue,
            "/xl/tables/table1.xml /table[1]/tableColumns[1]/tableColumn[1]".to_owned()
        )]
    );
}

#[test]
fn a_style_index_past_the_cell_formats_is_reported() {
    let package = spreadsheet(
        "<sheetData><row r=\"1\"><c r=\"A1\" s=\"5\"><v>1</v></c>\
         <c r=\"B1\" s=\"0\"><v>1</v></c></row></sheetData>",
        None,
    );
    assert_eq!(
        semantic(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::IndexedElementNotFound,
            "/xl/worksheets/sheet1.xml /worksheet[1]/sheetData[1]/row[1]/c[1]".to_owned()
        )]
    );
}

#[test]
fn the_table_holds_every_registration_sorted_by_type() {
    assert_eq!(SEMANTIC_CONSTRAINTS.len(), 988);
    let types: BTreeSet<u16> = SEMANTIC_CONSTRAINTS.iter().map(|row| row.type_id).collect();
    assert_eq!(types.len(), 466);
    assert!(
        SEMANTIC_CONSTRAINTS
            .windows(2)
            .all(|pair| matches!(pair, [left, right] if left.type_id <= right.type_id))
    );
    for row in SEMANTIC_CONSTRAINTS {
        assert!(SCHEMA_TYPES.get(usize::from(row.type_id)).is_some());
        if let Constraint::ValuePattern {
            attribute: _attribute,
            pattern,
        } = row.constraint
        {
            assert!(Regex::new(pattern).is_ok(), "{pattern}");
        }
    }
    let bookmark = type_id("w:CT_Bookmark/w:bookmarkStart").unwrap();
    assert_eq!(registrations(bookmark).len(), 1);
}
