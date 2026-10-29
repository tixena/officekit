use core::slice;
use std::path::Path;

use officekit::validation::error::{ErrorCode, Step, ValidationError};
use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::part_name::PartName;
use os_shim::mock::MemorySystem;
use serde_json::{Value, json};

use super::{DIVERGENCES, SDK_KEYS, divergence, missing, run, sdk_key};

const C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The left margin of the third-party sample's table style, as officekit
/// reports it, with the file's prefix.
fn left_margin(prefix: &str) -> ValidationError {
    let path = ["styles", "style", "tblPr", "tblCellMar", "left"]
        .iter()
        .map(|local| Step::new(prefix, W, local, 1))
        .collect();
    ValidationError::new(
        ErrorCode::InvalidAttributeValue,
        PartName::new("/word/styles.xml").ok(),
        Some(path),
        &[
            ("attribute", "w:w"),
            ("value", "100.0"),
            ("detail", "not a whole number from 0 to 32767"),
        ],
    )
}

/// The same error as the oracle writes it for that file at Office 2010.
fn oracle_left_margin() -> Value {
    let step = |local: &str| json!({ "namespace": W, "local": local, "index": 1_u32 });
    json!({
        "file": "canvas.instructure.docx",
        "target": "Office2010",
        "kind": "schema",
        "id": "Sch_AttributeValueDataTypeDetailed",
        "part": "/word/styles.xml",
        "related_part": null,
        "path": [step("styles"), step("style"), step("tblPr"), step("tblCellMar"), step("left")],
        "message": "The attribute 'w:w' has invalid value '100.0'.",
    })
}

fn ours(error: &ValidationError, target: OfficeVersion) -> Value {
    error.record("canvas.instructure.docx", target)
}

#[test]
fn every_code_has_one_sdk_id_and_only_the_strict_codes_have_none() {
    assert_eq!(SDK_KEYS.len(), 43);
    for code in ErrorCode::ALL {
        let rows = SDK_KEYS.iter().filter(|(known, _)| known == code).count();
        assert_eq!(rows, usize::from(sdk_key(*code).is_some()), "{code:?}");
    }
    assert!(SDK_KEYS.iter().all(
        |(_, id)| *id != "MC_ShallContainChoice" && *id != "MC_ShallNotContainAlternateContent"
    ));
    let without: Vec<ErrorCode> = ErrorCode::ALL
        .iter()
        .copied()
        .filter(|code| sdk_key(*code).is_none())
        .collect();
    assert_eq!(
        without,
        [
            ErrorCode::TransitionalOnlyElement,
            ErrorCode::TransitionalOnlyAttribute,
            ErrorCode::TransitionalOnlyValue
        ]
    );
}

#[test]
fn an_error_matches_the_oracles_whatever_prefix_the_file_used() {
    for prefix in ["w", "ns0"] {
        let record = ours(&left_margin(prefix), OfficeVersion::Office2010);
        assert_eq!(
            missing(&[oracle_left_margin()], &[record]).len(),
            0,
            "{prefix}"
        );
    }
}

#[test]
fn a_part_matches_whatever_its_ascii_case() {
    let mut oracle = oracle_left_margin();
    oracle["part"] = json!("/word/Styles.XML");
    let record = ours(&left_margin("w"), OfficeVersion::Office2010);
    assert_eq!(missing(&[oracle], &[record]).len(), 0);
}

#[test]
fn a_second_sdk_error_at_the_same_path_needs_a_second_one() {
    let record = ours(&left_margin("w"), OfficeVersion::Office2010);
    let oracle = [oracle_left_margin(), oracle_left_margin()];
    assert_eq!(missing(&oracle, slice::from_ref(&record)).len(), 1);
    assert_eq!(missing(&oracle, &[record.clone(), record]).len(), 0);
}

#[test]
fn another_target_or_index_does_not_match() {
    let at_2013 = ours(&left_margin("w"), OfficeVersion::Office2013);
    let mut moved = left_margin("w");
    if let Some(path) = moved.path.as_mut() {
        path[1].index = 2;
    }
    let other_index = ours(&moved, OfficeVersion::Office2010);
    assert_eq!(
        missing(&[oracle_left_margin()], &[at_2013, other_index]).len(),
        1
    );
}

#[test]
fn errors_only_officekit_reports_do_not_count() {
    let strict = ValidationError::new(
        ErrorCode::TransitionalOnlyValue,
        PartName::new("/word/document.xml").ok(),
        None,
        &[("value", "left"), ("attribute", "w:val")],
    );
    let records = [
        ours(&left_margin("w"), OfficeVersion::Office2010),
        ours(&strict, OfficeVersion::Office2010),
    ];
    assert_eq!(missing(&[oracle_left_margin()], &records).len(), 0);
    assert_eq!(missing(&[oracle_left_margin()], &records[1..]).len(), 1);
}

#[test]
fn a_run_reads_both_sides_through_the_system_and_counts_the_kinds_asked_for() {
    let mut markup = oracle_left_margin();
    markup["kind"] = json!("markup-compatibility");
    markup["id"] = json!("MC_InvalidIgnorableAttribute");
    let mut semantic = oracle_left_margin();
    semantic["kind"] = json!("semantic");
    semantic["id"] = json!("Sem_UniqueAttributeValue");
    let oracle = Value::Array(vec![oracle_left_margin(), markup, semantic]);
    let found = Value::Array(vec![ours(&left_margin("w"), OfficeVersion::Office2010)]);
    let system = MemorySystem::new()
        .with_file("parity/oracle.json", oracle.to_string().as_bytes())
        .unwrap()
        .with_file(
            "parity/officekit/canvas.instructure.docx.2010.json",
            found.to_string().as_bytes(),
        )
        .unwrap()
        .with_file("parity/officekit/notes.txt", b"not a result")
        .unwrap();
    let kinds = ["schema".to_owned(), "markup-compatibility".to_owned()];
    let report = run(
        &system,
        Path::new("parity/oracle.json"),
        Path::new("parity/officekit"),
        &kinds,
    )
    .unwrap();
    assert_eq!(report.missing.len(), 1);
    assert_eq!(report.missing[0]["id"], "MC_InvalidIgnorableAttribute");
    assert_eq!(
        report.summary(),
        "missing by kind: schema 0, markup-compatibility 1"
    );
    assert!(!report.complete());
}

#[test]
fn a_run_refuses_a_result_that_is_no_array() {
    let system = MemorySystem::new()
        .with_file("parity/oracle.json", b"{}")
        .unwrap()
        .with_file("parity/officekit/a.json", b"[]")
        .unwrap();
    let error = run(
        &system,
        Path::new("parity/oracle.json"),
        Path::new("parity/officekit"),
        &["schema".to_owned()],
    )
    .unwrap_err();
    assert_eq!(
        format!("{error:#}"),
        "parity/oracle.json holds no JSON array"
    );
}

fn chart_extension_error(id: &str, target: &str, locals: &[&str]) -> Value {
    let path: Vec<Value> = ["chartSpace", "chart"]
        .iter()
        .chain(locals)
        .map(|local| json!({ "namespace": C, "local": local, "index": 1_u32 }))
        .collect();
    json!({
        "file": "chart.docx",
        "target": target,
        "kind": "schema",
        "id": id,
        "part": "/word/charts/chart1.xml",
        "path": path,
    })
}

#[test]
fn the_sdk_errors_the_corrections_remove_are_intended_divergences() {
    for row in DIVERGENCES {
        for target in row.targets {
            let path: Vec<Value> = row
                .path
                .iter()
                .map(|(namespace, local)| json!({ "namespace": namespace, "local": local, "index": 1_u32 }))
                .collect();
            let record = json!({
                "file": "document.docx",
                "target": target,
                "kind": "schema",
                "id": row.id,
                "part": "/word/document.xml",
                "path": path,
                "message": row.mentioning.unwrap_or_default(),
            });
            assert_eq!(divergence(&record), Some(row.reason), "{} {target}", row.id);
        }
    }
}

#[test]
fn an_error_that_does_not_mention_the_element_is_no_divergence() {
    let record = json!({
        "file": "document.docx",
        "target": "Microsoft365",
        "kind": "schema",
        "id": "Sch_InvalidElementContentExpectingComplex",
        "part": "/word/document.xml",
        "path": [{ "namespace": W, "local": "p", "index": 1_u32 }],
        "message": "The element has invalid child element 'http://schemas.openxmlformats.org/wordprocessingml/2006/main:r'.",
    });
    assert_eq!(divergence(&record), None);
}

#[test]
fn another_error_or_target_or_path_is_no_divergence() {
    let unexpected = "Sch_UnexpectedElementContentExpectingComplex";
    let outside = chart_extension_error(unexpected, "Office2019", &["extLst"]);
    assert_eq!(divergence(&outside), None);
    let other_path = chart_extension_error(unexpected, "Office2016", &["layout"]);
    assert_eq!(divergence(&other_path), None);
    let other_id = chart_extension_error("Sch_MissRequiredAttribute", "Office2016", &["extLst"]);
    assert_eq!(divergence(&other_id), None);
}

#[test]
fn a_run_counts_a_divergence_apart_from_the_missing_errors() {
    let record = chart_extension_error(
        "Sch_UnexpectedElementContentExpectingComplex",
        "Office2016",
        &["extLst"],
    );
    let system = MemorySystem::new()
        .with_file("oracle.json", json!([record]).to_string().as_bytes())
        .unwrap()
        .with_file("ours/chart.json", b"[]")
        .unwrap();
    let report = run(
        &system,
        Path::new("oracle.json"),
        Path::new("ours"),
        &["schema".to_owned()],
    )
    .unwrap();
    assert!(report.complete());
    assert_eq!(report.divergent.len(), 1);
}
