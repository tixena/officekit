use core::str;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use officekit_xml::ready::ready_now;
use os_shim::System as _;
use os_shim::bucket::Condition;
use os_shim::real::RealSystem;
use tempfile::TempDir;

use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;

fn run(args: &[&str]) -> Output {
    Command::cargo_bin("officekit")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

fn write_sample(dir: &Path) -> PathBuf {
    let mut package = Package::new();
    package
        .set_part_bytes(
            &PartName::new("/word/document.xml").unwrap(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            b"<w:document/>",
        )
        .unwrap();
    let path = dir.join("sample.docx");
    ready_now(package.save(&RealSystem::new(), &path)).unwrap();
    path
}

#[test]
fn a_package_that_opens_validates_with_status_0_and_no_output() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["validate", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn json_prints_an_empty_array_when_there_are_no_errors() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&[
        "validate",
        path.to_str().unwrap(),
        "--json",
        "--target",
        "2013",
        "--max-errors",
        "0",
    ]);
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    assert_eq!(output.stdout, b"[]\n");
}

#[test]
fn a_file_that_does_not_exist_exits_2_with_the_reason() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("missing.docx");

    let output = run(&["validate", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        stderr.starts_with("officekit validate: "),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("missing.docx"), "stderr: {stderr}");
}

#[test]
fn a_file_that_is_not_a_zip_archive_exits_2() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("not-a-package.docx");
    ready_now(RealSystem::new().write(&path, b"plain text", Condition::None)).unwrap();

    let output = run(&["validate", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
}

#[test]
fn a_target_outside_the_six_is_a_usage_error() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["validate", path.to_str().unwrap(), "--target", "2007"]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(
        stderr.contains("'2007' is not a target"),
        "stderr: {stderr}"
    );
}
