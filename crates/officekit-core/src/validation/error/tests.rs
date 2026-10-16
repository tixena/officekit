use serde_json::json;

use super::{ErrorCode, Errors, Finding, Step, ValidationError, target_name};
use crate::packaging::part_name::PartName;
use crate::xml::name::OfficeVersion;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn part(name: &str) -> Option<PartName> {
    PartName::new(name).ok()
}

fn steps(prefix: &str, path: &[(&str, u32)]) -> Vec<Step> {
    path.iter()
        .map(|(local, index)| Step::new(prefix, W, local, *index))
        .collect()
}

fn left_margin(prefix: &str) -> ValidationError {
    ValidationError::new(
        ErrorCode::InvalidAttributeValue,
        part("/word/styles.xml"),
        Some(steps(
            prefix,
            &[
                ("styles", 1),
                ("style", 1),
                ("tblPr", 1),
                ("tblCellMar", 1),
                ("left", 1),
            ],
        )),
        &[
            ("attribute", "w:w"),
            ("value", "100.0"),
            ("detail", "not a whole number from 0 to 32767"),
        ],
    )
}

fn count(kind: Finding) -> usize {
    ErrorCode::ALL
        .iter()
        .filter(|code| code.kind() == kind)
        .count()
}

#[test]
fn there_are_45_codes_each_found_again_by_its_name() {
    assert_eq!(ErrorCode::ALL.len(), 45);
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::parse(code.as_str()), Some(*code));
    }
    assert_eq!(ErrorCode::parse("NotACode"), None);
}

#[test]
fn the_codes_split_into_their_kinds() {
    assert_eq!(count(Finding::Schema), 15);
    assert_eq!(count(Finding::Semantic), 13);
    assert_eq!(count(Finding::MarkupCompatibility), 12);
    assert_eq!(count(Finding::Package), 5);
}

#[test]
fn a_placeholder_left_unfilled_is_left_out_of_the_message() {
    let bare = ValidationError::new(ErrorCode::MissingChild, None, None, &[]);
    assert_eq!(bare.message, "the element ends before a child it requires");
    let listed = ValidationError::new(
        ErrorCode::InvalidChild,
        None,
        None,
        &[("child", "w:p"), ("expected", ", expected w:pPr or w:r")],
    );
    assert_eq!(
        listed.message,
        "the element holds w:p, which it does not allow anywhere, expected w:pPr or w:r"
    );
}

#[test]
fn a_filled_value_is_not_read_for_placeholders() {
    let error = ValidationError::new(
        ErrorCode::InvalidAttributeValue,
        None,
        None,
        &[("attribute", "w:w"), ("value", "{detail}"), ("detail", "x")],
    );
    assert_eq!(
        error.message,
        "the attribute w:w has the value \"{detail}\": x"
    );
}

#[test]
fn the_path_text_uses_the_files_prefixes() {
    let prefixed = ValidationError::new(
        ErrorCode::MissingRootElement,
        part("/word/document.xml"),
        Some(steps("w", &[("document", 1), ("body", 1)])),
        &[],
    );
    assert_eq!(prefixed.path_text(), "/w:document[1]/w:body[1]");
    let bare = ValidationError::new(
        ErrorCode::MissingRootElement,
        None,
        Some(steps("", &[("document", 1)])),
        &[],
    );
    assert_eq!(bare.path_text(), "/document[1]");
    assert_eq!(
        left_margin("ns0").path_text(),
        "/ns0:styles[1]/ns0:style[1]/ns0:tblPr[1]/ns0:tblCellMar[1]/ns0:left[1]"
    );
}

#[test]
fn a_line_with_a_path_prints_the_part_the_path_the_kind_the_code_and_the_message() {
    assert_eq!(
        left_margin("w").line(),
        "/word/styles.xml /w:styles[1]/w:style[1]/w:tblPr[1]/w:tblCellMar[1]/w:left[1]: schema InvalidAttributeValue: the attribute w:w has the value \"100.0\": not a whole number from 0 to 32767"
    );
}

#[test]
fn a_package_error_with_no_path_prints_the_part_alone() {
    let error = ValidationError::new(
        ErrorCode::MissingRequiredPart,
        part("/word/document.xml"),
        None,
        &[("part", "/word/document.xml")],
    );
    assert_eq!(
        error.line(),
        "/word/document.xml: package MissingRequiredPart: the required part /word/document.xml is missing"
    );
}

#[test]
fn an_error_with_no_part_starts_with_the_kind() {
    let error = ValidationError::new(ErrorCode::ChoiceWithoutRequires, None, None, &[]);
    assert_eq!(
        error.line(),
        "markup-compatibility ChoiceWithoutRequires: an mc:Choice has no Requires attribute"
    );
}

#[test]
fn the_record_holds_the_steps_by_namespace_and_not_by_prefix() {
    let record = left_margin("ns0").record("canvas.instructure.docx", OfficeVersion::Microsoft365);
    assert_eq!(record["file"], "canvas.instructure.docx");
    assert_eq!(record["target"], "Microsoft365");
    assert_eq!(record["kind"], "schema");
    assert_eq!(record["code"], "InvalidAttributeValue");
    assert_eq!(record["part"], "/word/styles.xml");
    assert_eq!(record["related_part"], json!(null));
    assert_eq!(
        record["path"][4],
        json!({ "namespace": W, "local": "left", "index": 1_u32 })
    );
    assert_eq!(
        record["message"],
        "the attribute w:w has the value \"100.0\": not a whole number from 0 to 32767"
    );
}

#[test]
fn the_record_of_a_package_error_has_no_path() {
    let mut error = ValidationError::new(ErrorCode::PartNotAllowed, part("/a.xml"), None, &[]);
    error.related_part = part("/b.xml");
    let record = error.record("f.docx", OfficeVersion::Office2010);
    assert_eq!(record["path"], json!(null));
    assert_eq!(record["related_part"], "/b.xml");
    assert_eq!(record["target"], "Office2010");
}

#[test]
fn every_target_has_its_own_name() {
    let names = [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ]
    .map(target_name);
    assert_eq!(
        names,
        [
            "Office2010",
            "Office2013",
            "Office2016",
            "Office2019",
            "Office2021",
            "Microsoft365"
        ]
    );
}

#[test]
fn errors_stop_at_the_cap_and_keep_the_one_that_reaches_it() {
    let mut errors = Errors::new(2);
    for _ in 0_u32..3 {
        errors.push(left_margin("w"));
    }
    assert!(errors.full());
    assert_eq!(errors.into_vec().len(), 2);
}

#[test]
fn errors_with_a_cap_of_zero_keep_every_error() {
    let mut errors = Errors::new(0);
    for _ in 0_u32..1500 {
        errors.push(left_margin("w"));
    }
    assert!(!errors.full());
    assert_eq!(errors.into_vec().len(), 1500);
}
