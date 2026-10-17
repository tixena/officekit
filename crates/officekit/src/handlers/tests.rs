use std::path::{Path, PathBuf};

use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_validation::error::{ErrorCode, Step, ValidationError};
use officekit_xml::name::OfficeVersion;
use os_shim::mock::MemorySystem;
use serde_json::{Value, json};

use super::{PackageValidator, Validate, cmd_cat, cmd_validate};
use crate::cli::{CatArgs, ValidateArgs};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

struct Fixed(Result<Vec<ValidationError>, String>);

impl Validate for Fixed {
    fn validate(
        &self,
        _file: &Path,
        _target: OfficeVersion,
        _max_errors: usize,
    ) -> Result<Vec<ValidationError>, String> {
        self.0.clone()
    }
}

fn left_margin() -> ValidationError {
    let path = ["styles", "style", "tblPr", "tblCellMar", "left"]
        .iter()
        .map(|local| Step::new("w", W, local, 1))
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

fn args(json: bool) -> ValidateArgs {
    ValidateArgs {
        file: PathBuf::from("samples/canvas.instructure.docx"),
        json,
        max_errors: 1000,
        target: OfficeVersion::Microsoft365,
    }
}

fn run(result: Result<Vec<ValidationError>, String>, json: bool) -> (u8, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = cmd_validate(&Fixed(result), &args(json), &mut out, &mut err).unwrap();
    (
        status,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

fn sample(system: &MemorySystem) -> PathBuf {
    let mut package = Package::new();
    package
        .set_part_bytes(
            &PartName::new("/word/document.xml").unwrap(),
            MAIN,
            b"<w:document/>",
        )
        .unwrap();
    let path = PathBuf::from("/sample.docx");
    package.save(system, &path).unwrap();
    path
}

#[test]
fn validate_prints_one_line_per_error_and_exits_1() {
    let (status, out, err) = run(Ok(vec![left_margin()]), false);
    assert_eq!(status, 1);
    assert_eq!(err, "");
    assert_eq!(
        out,
        "/word/styles.xml /w:styles[1]/w:style[1]/w:tblPr[1]/w:tblCellMar[1]/w:left[1]: schema InvalidAttributeValue: the attribute w:w has the value \"100.0\": not a whole number from 0 to 32767\n"
    );
}

#[test]
fn validate_prints_the_json_records_and_exits_1_with_errors() {
    let (status, out, _) = run(Ok(vec![left_margin()]), true);
    assert_eq!(status, 1);
    let records: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(records[0]["code"], "InvalidAttributeValue");
    assert_eq!(records[0]["file"], "canvas.instructure.docx");
    assert_eq!(records[0]["target"], "Microsoft365");
    assert_eq!(
        records[0]["path"][4],
        json!({ "namespace": W, "local": "left", "index": 1_u32 })
    );
}

#[test]
fn validate_prints_an_empty_array_or_nothing_and_exits_0_with_no_errors() {
    assert_eq!(run(Ok(vec![]), true), (0, "[]\n".to_owned(), String::new()));
    assert_eq!(run(Ok(vec![]), false), (0, String::new(), String::new()));
}

#[test]
fn validate_exits_2_with_the_reason_on_standard_error_when_the_check_cannot_run() {
    let (status, out, err) = run(Err("not a zip archive".to_owned()), false);
    assert_eq!((status, out.as_str()), (2, ""));
    assert_eq!(
        err,
        "officekit validate: samples/canvas.instructure.docx: not a zip archive\n"
    );
}

#[test]
fn a_package_validator_reports_no_error_for_a_package_that_opens() {
    let system = MemorySystem::new();
    let path = sample(&system);
    let validator = PackageValidator::new(&system);
    let errors = validator
        .validate(&path, OfficeVersion::Microsoft365, 1000)
        .unwrap();
    assert_eq!(errors.len(), 0);
}

#[test]
fn a_package_validator_gives_the_reason_when_the_file_does_not_open() {
    let system = MemorySystem::new();
    let validator = PackageValidator::new(&system);
    let reason = validator
        .validate(Path::new("/absent.docx"), OfficeVersion::Microsoft365, 1000)
        .unwrap_err();
    assert_eq!(reason, "File not found: /absent.docx");
}

#[test]
fn cat_writes_the_bytes_of_the_part() {
    let system = MemorySystem::new();
    let path = sample(&system);
    let args = CatArgs {
        file: path,
        part: "/word/document.xml".to_owned(),
    };
    let mut out = Vec::new();
    cmd_cat(&system, &args, &mut out).unwrap();
    assert_eq!(out, b"<w:document/>");
}

#[test]
fn cat_of_a_missing_part_fails() {
    let system = MemorySystem::new();
    let path = sample(&system);
    let args = CatArgs {
        file: path,
        part: "/word/absent.xml".to_owned(),
    };
    let mut out = Vec::new();
    let err = cmd_cat(&system, &args, &mut out).unwrap_err();
    assert_eq!(err.to_string(), "the package has no part /word/absent.xml");
    assert_eq!(out, b"");
}
