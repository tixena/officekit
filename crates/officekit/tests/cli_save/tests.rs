use core::str;
use std::io::Read as _;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use os_shim::System as _;
use os_shim::WriteMode;
use os_shim::real::RealSystem;
use tempfile::TempDir;

use officekit_core::xlsx::Xlsx;
use officekit_packaging::package::Package;
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

fn path(dir: &Path, name: &str) -> String {
    dir.join(name).to_str().unwrap().to_owned()
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> String {
    RealSystem::new()
        .write(&dir.join(name), bytes, WriteMode::Replace)
        .unwrap();
    path(dir, name)
}

#[test]
fn the_long_form_of_a_word_file_gives_the_same_markdown() {
    let dir = TempDir::new().unwrap();
    let markdown = write(dir.path(), "it.md", REPORT.as_bytes());
    let word = path(dir.path(), "it.docx");
    let made = run(&["get", &markdown, "-f", "docx", "-o", &word], b"");
    assert_eq!(made.status.code(), Some(0_i32), "got {made:?}");

    let binary = run(&["get", &word, "-f", "binary"], b"");
    assert_eq!(binary.status.code(), Some(0_i32), "got {binary:?}");
    let saved = run(&["save", "-f", "md"], &binary.stdout);
    assert_eq!(saved.status.code(), Some(0_i32), "got {saved:?}");
    let direct = run(&["get", &word, "-f", "md"], b"");
    assert_eq!(saved.stdout, direct.stdout);
    assert_eq!(str::from_utf8(&saved.stdout).unwrap(), REPORT);
}

#[test]
fn the_long_form_of_a_markdown_file_writes_the_same_word_file() {
    let dir = TempDir::new().unwrap();
    let markdown = write(dir.path(), "notes.md", REPORT.as_bytes());
    let direct = path(dir.path(), "direct.docx");
    let through = path(dir.path(), "through.docx");
    let made = run(&["get", &markdown, "-f", "docx", "-o", &direct], b"");
    assert_eq!(made.status.code(), Some(0_i32), "got {made:?}");

    let binary = run(&["get", &markdown, "-f", "binary"], b"");
    assert_eq!(binary.status.code(), Some(0_i32), "got {binary:?}");
    let saved = run(&["save", "-f", "docx", "-o", &through], &binary.stdout);
    assert_eq!(saved.status.code(), Some(0_i32), "got {saved:?}");

    let system = RealSystem::new();
    let (mut a, mut b) = (Vec::new(), Vec::new());
    system
        .open(Path::new(&direct))
        .unwrap()
        .read_to_end(&mut a)
        .unwrap();
    system
        .open(Path::new(&through))
        .unwrap()
        .read_to_end(&mut b)
        .unwrap();
    assert_eq!(a, b);
}

#[test]
fn an_excel_workbook_goes_through_the_binary_form() {
    let dir = TempDir::new().unwrap();
    let (bytes, _) = Xlsx::new(SpreadsheetKind::Workbook)
        .unwrap()
        .to_bytes_transitional()
        .unwrap();
    let book = write(dir.path(), "book.xlsx", &bytes);
    let out = path(dir.path(), "out.xlsx");

    let binary = run(&["get", &book, "-f", "binary"], b"");
    assert_eq!(binary.status.code(), Some(0_i32), "got {binary:?}");
    let saved = run(&["save", "-f", "xlsx", "-o", &out], &binary.stdout);
    assert_eq!(saved.status.code(), Some(0_i32), "got {saved:?}");
    Package::open(&RealSystem::new(), Path::new(&out)).unwrap();
}

#[test]
fn the_binary_form_is_read_from_a_file_with_i() {
    let dir = TempDir::new().unwrap();
    let markdown = write(dir.path(), "it.md", REPORT.as_bytes());
    let binary = path(dir.path(), "it.bin");
    let made = run(&["get", &markdown, "-f", "binary", "-o", &binary], b"");
    assert_eq!(made.status.code(), Some(0_i32), "got {made:?}");

    let saved = run(&["save", "-i", &binary, "-f", "md"], b"");
    assert_eq!(saved.status.code(), Some(0_i32), "got {saved:?}");
    assert_eq!(str::from_utf8(&saved.stdout).unwrap(), REPORT);

    let piped = run(&["get", &markdown, "-f", "binary"], b"");
    let dash = run(&["save", "-i", "-", "-f", "md"], &piped.stdout);
    assert_eq!(str::from_utf8(&dash.stdout).unwrap(), REPORT);
}

#[test]
fn bytes_that_are_not_the_binary_form_exit_2() {
    let output = run(&["save", "-f", "md"], b"PK\x03\x04 a zip");
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    assert_eq!(
        str::from_utf8(&output.stderr).unwrap(),
        "error: not officekit's binary form: the bytes do not open with `officekit` and a zero byte\n"
    );
}

#[test]
fn another_version_exits_2() {
    let mut bytes = b"officekit\0".to_vec();
    bytes.push(200);
    let output = run(&["save", "-f", "md"], &bytes);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert!(
        str::from_utf8(&output.stderr)
            .unwrap()
            .starts_with("error: officekit's binary form version 200")
    );
}

#[test]
fn saving_as_binary_exits_2() {
    let output = run(&["save", "-f", "binary"], b"");
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(
        str::from_utf8(&output.stderr).unwrap(),
        "error: save writes markdown, docx, xlsx or pptx; binary is what it reads\n"
    );
}
