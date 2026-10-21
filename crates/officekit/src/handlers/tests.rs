use std::path::{Path, PathBuf};

use officekit_core::xlsx::Xlsx;
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_parts::kinds::SpreadsheetKind;
use officekit_validation::error::{ErrorCode, Step, ValidationError};
use officekit_xml::name::OfficeVersion;
use os_shim::System as _;
use os_shim::mock::MemorySystem;
use serde_json::{Value, json};

use super::{PackageValidator, Validate, cmd_cat, cmd_get, cmd_validate};
use crate::cli::{CatArgs, Form, GetArgs, ValidateArgs};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

const DOT: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAwS2OUAAAAABJRU5ErkJggg==";

const REPORT: &str = "# Report\n\n1. one\n2. two\n";

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

fn get_args(file: &str, format: Form, output: Option<&str>) -> GetArgs {
    GetArgs {
        file: PathBuf::from(file),
        format,
        output: output.map(PathBuf::from),
        frontmatter: false,
    }
}

fn get(
    system: &MemorySystem,
    args: &GetArgs,
    stdin: &[u8],
) -> (anyhow::Result<()>, Vec<u8>, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let mut input = stdin;
    let result = cmd_get(system, args, &mut input, &mut out, &mut err);
    (result, out, String::from_utf8(err).unwrap())
}

fn workbook(system: &MemorySystem) {
    let (bytes, _) = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    system.write(Path::new("/book.xlsx"), &bytes).unwrap();
}

fn report_docx(system: &MemorySystem) -> Vec<u8> {
    system
        .write(Path::new("/it.md"), REPORT.as_bytes())
        .unwrap();
    let args = get_args("/it.md", Form::Docx, None);
    let (result, bytes, _) = get(system, &args, b"");
    result.unwrap();
    bytes
}

#[test]
fn get_writes_a_markdown_file_as_a_word_file_that_reads_back_as_the_same_markdown() {
    let system = MemorySystem::new();
    system
        .write(Path::new("/it.md"), REPORT.as_bytes())
        .unwrap();
    let args = get_args("/it.md", Form::Docx, Some("/it.docx"));
    let (result, out, err) = get(&system, &args, b"");
    result.unwrap();
    assert_eq!((out.as_slice(), err.as_str()), (b"".as_slice(), ""));

    let reread = get_args("/it.docx", Form::Markdown, None);
    let (reread_result, reread_out, _) = get(&system, &reread, b"");
    reread_result.unwrap();
    assert_eq!(String::from_utf8(reread_out).unwrap(), REPORT);
}

#[test]
fn get_reads_standard_input_for_the_file_dash() {
    let system = MemorySystem::new();
    let docx = report_docx(&system);
    let args = get_args("-", Form::Markdown, Some("-"));
    let (result, out, _) = get(&system, &args, &docx);
    result.unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), REPORT);
}

#[test]
fn get_reads_markdown_from_standard_input() {
    let system = MemorySystem::new();
    let args = get_args("-", Form::Markdown, None);
    let (result, out, _) = get(&system, &args, REPORT.as_bytes());
    result.unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), REPORT);
}

#[test]
fn get_fails_naming_a_relative_picture_of_standard_input() {
    let system = MemorySystem::new();
    let args = get_args("-", Form::Docx, None);
    let (result, out, _) = get(&system, &args, b"![Logo](logo.png)\n");
    let message = format!("{:#}", result.unwrap_err());
    assert!(message.contains("logo.png"), "got {message}");
    assert_eq!(out.len(), 0);
}

#[test]
fn get_refuses_markdown_for_an_excel_workbook() {
    let system = MemorySystem::new();
    workbook(&system);
    let args = get_args("/book.xlsx", Form::Markdown, None);
    let (result, out, _) = get(&system, &args, b"");
    assert_eq!(
        result.unwrap_err().to_string(),
        "markdown is not available for an Excel workbook yet"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn get_refuses_a_form_that_is_not_the_kind_of_a_workbook() {
    let system = MemorySystem::new();
    workbook(&system);
    let args = get_args("/book.xlsx", Form::Docx, None);
    let (result, _, _) = get(&system, &args, b"");
    assert_eq!(
        result.unwrap_err().to_string(),
        "the input is an Excel workbook; it cannot be written as docx"
    );
}

#[test]
fn get_refuses_a_form_that_is_not_the_kind_of_a_word_file() {
    let system = MemorySystem::new();
    let docx = report_docx(&system);
    let args = get_args("-", Form::Pptx, None);
    let (result, _, _) = get(&system, &args, &docx);
    assert_eq!(
        result.unwrap_err().to_string(),
        "the input is a Word document; it cannot be written as pptx"
    );
}

#[test]
fn get_writes_a_workbook_as_a_workbook() {
    let system = MemorySystem::new();
    workbook(&system);
    let args = get_args("/book.xlsx", Form::Xlsx, Some("/copy.xlsx"));
    let (result, _, err) = get(&system, &args, b"");
    result.unwrap();
    assert_eq!(err, "");
    Package::open(&system, Path::new("/copy.xlsx")).unwrap();
}

#[test]
fn get_fails_for_a_file_that_is_absent() {
    let system = MemorySystem::new();
    let args = get_args("/absent.docx", Form::Markdown, None);
    let (result, _, _) = get(&system, &args, b"");
    assert!(format!("{:#}", result.unwrap_err()).starts_with("reading /absent.docx"));
}

#[test]
fn get_writes_a_picture_into_the_markdown_as_a_data_uri() {
    let system = MemorySystem::new();
    let source = format!("![Dot]({DOT})\n");
    let docx_args = get_args("-", Form::Docx, None);
    let (docx_result, docx, _) = get(&system, &docx_args, source.as_bytes());
    docx_result.unwrap();

    let args = get_args("-", Form::Markdown, None);
    let (result, out, _) = get(&system, &args, &docx);
    result.unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.starts_with("![Dot](data:image/png;base64,"),
        "got {text}"
    );
    assert_eq!(system.read_dir(Path::new("/")).unwrap().len(), 0);
}
