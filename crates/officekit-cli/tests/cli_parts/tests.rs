use core::str;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use officekit::docx::Docx;
use officekit::parts::kinds::WordprocessingKind;
use officekit_xml::ready::ready_now;
use os_shim::System as _;
use os_shim::real::RealSystem;
use serde_json::Value;
use tempfile::TempDir;

fn run(args: &[&str]) -> Output {
    Command::cargo_bin("officekit")
        .unwrap()
        .args(args)
        .output()
        .unwrap()
}

fn write_sample(dir: &Path) -> PathBuf {
    let path = dir.join("sample.docx");
    let docx = Docx::new(WordprocessingKind::Document).unwrap();
    ready_now(docx.save_transitional(&RealSystem::new(), &path)).unwrap();
    path
}

#[test]
fn lists_part_names_one_per_line() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["parts", path.to_str().unwrap()]);
    assert!(output.status.success(), "command failed: {output:?}");
    let stdout = str::from_utf8(&output.stdout).unwrap();
    assert_eq!(
        stdout,
        "/[Content_Types].xml\n/_rels/.rels\n/word/document.xml\n"
    );
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
            Value::from("/_rels/.rels"),
            Value::from("/word/document.xml")
        ])
    );
}

#[test]
fn missing_file_exits_two_with_an_error_line() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("absent.docx");
    assert!(!ready_now(RealSystem::new().exists(&path)).unwrap());

    let output = run(&["parts", path.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert!(stderr.starts_with("error: opening "), "stderr: {stderr}");
    assert!(stderr.contains("absent.docx"), "stderr: {stderr}");
}
