use core::str;
use std::io::Read as _;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use os_shim::System as _;
use os_shim::WriteMode;
use os_shim::real::RealSystem;
use tempfile::TempDir;

use officekit_core::docx::Docx;
use officekit_core::xlsx::Xlsx;
use officekit_packaging::package::OpenOptions;
use officekit_parts::kinds::SpreadsheetKind;

const REPORT: &str = "# Report\n\n1. one\n2. two\n";

fn run(args: &[&str], stdin: &[u8]) -> Output {
    Command::cargo_bin("officekit")
        .unwrap()
        .args(args)
        .write_stdin(stdin)
        .output()
        .unwrap()
}

fn report_docx(dir: &Path) -> Vec<u8> {
    let markdown = dir.join("it.md");
    RealSystem::new()
        .write(&markdown, REPORT.as_bytes(), WriteMode::Replace)
        .unwrap();
    let word = dir.join("it.docx");
    let output = run(
        &[
            "get",
            markdown.to_str().unwrap(),
            "-f",
            "docx",
            "-o",
            word.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    let mut bytes = Vec::new();
    RealSystem::new()
        .open(&word)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn a_markdown_file_becomes_a_word_file_that_reads_back_as_the_same_markdown() {
    let dir = TempDir::new().unwrap();
    report_docx(dir.path());
    let word = dir.path().join("it.docx");

    Docx::open(&RealSystem::new(), &word, OpenOptions::default()).unwrap();
    let output = run(&["get", word.to_str().unwrap(), "-f", "md"], b"");
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    assert_eq!(str::from_utf8(&output.stdout).unwrap(), REPORT);
}

#[test]
fn md_and_markdown_give_the_same_output() {
    let dir = TempDir::new().unwrap();
    report_docx(dir.path());
    let word = dir.path().join("it.docx");

    let short = run(&["get", word.to_str().unwrap(), "-f", "md"], b"");
    let long = run(&["get", word.to_str().unwrap(), "-f", "markdown"], b"");
    assert_eq!(short.stdout, long.stdout);
    assert_eq!(RealSystem::new().read_dir(dir.path()).unwrap().len(), 2);
}

#[test]
fn standard_input_is_read_for_the_file_dash() {
    let dir = TempDir::new().unwrap();
    let docx = report_docx(dir.path());

    let output = run(&["get", "-", "-f", "markdown"], &docx);
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    assert_eq!(str::from_utf8(&output.stdout).unwrap(), REPORT);
}

#[test]
fn a_relative_picture_on_standard_input_exits_2_naming_the_file() {
    let output = run(&["get", "-", "-f", "docx"], b"![Logo](logo.png)\n");
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    assert!(str::from_utf8(&output.stderr).unwrap().contains("logo.png"));
}

#[test]
fn markdown_of_an_excel_workbook_exits_2() {
    let dir = TempDir::new().unwrap();
    let (bytes, _) = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    let book = dir.path().join("book.xlsx");
    RealSystem::new()
        .write(&book, &bytes, WriteMode::Replace)
        .unwrap();

    let output = run(&["get", book.to_str().unwrap(), "-f", "md"], b"");
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    assert_eq!(
        str::from_utf8(&output.stderr).unwrap(),
        "error: markdown is not available for an Excel workbook yet\n"
    );
}

#[test]
fn the_form_is_required() {
    let output = run(&["get", "it.docx"], b"");
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert!(str::from_utf8(&output.stderr).unwrap().contains("--format"));
}
