use core::str;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use os_shim::System as _;
use os_shim::real::RealSystem;
use serde_json::Value;
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
    package.save(&RealSystem::new(), &path).unwrap();
    path
}

#[test]
fn lists_part_names_one_per_line() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["parts", path.to_str().unwrap()]);
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    assert_eq!(stdout, "/[Content_Types].xml\n/word/document.xml\n");
}

#[test]
fn json_output_carries_file_and_parts() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["parts", path.to_str().unwrap(), "--json"]);
    assert!(output.status.success(), "command failed: {output:?}");
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["file"], path.to_str().unwrap());
    assert_eq!(
        payload["parts"],
        Value::Array(vec![
            Value::from("/[Content_Types].xml"),
            Value::from("/word/document.xml")
        ])
    );
}

#[test]
fn missing_file_exits_two_with_an_error_line() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("absent.docx");
    assert!(!RealSystem::new().exists(&path).unwrap());

    let output = run(&["parts", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(stderr.starts_with("error: opening "), "stderr: {stderr}");
    assert!(stderr.contains("absent.docx"), "stderr: {stderr}");
}
