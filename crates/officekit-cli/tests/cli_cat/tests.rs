use core::str;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use officekit::docx::Docx;
use officekit::parts::kinds::WordprocessingKind;
use officekit_xml::ready::ready_now;
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
    let path = dir.join("sample.docx");
    let docx = Docx::new(WordprocessingKind::Document).unwrap();
    ready_now(docx.save_transitional(&RealSystem::new(), &path)).unwrap();
    path
}

/// The bytes of the part `name` of the package at `path`.
fn part_bytes(path: &Path, name: &str) -> Vec<u8> {
    let package = ready_now(Package::open(&RealSystem::new(), path)).unwrap();
    package.part_bytes(&PartName::new(name).unwrap()).unwrap()
}

#[test]
fn writes_the_bytes_of_the_part() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["cat", path.to_str().unwrap(), "/word/document.xml"]);
    assert_eq!(output.status.code(), Some(0_i32), "got {output:?}");
    assert_eq!(output.stdout, part_bytes(&path, "/word/document.xml"));
}

#[test]
fn a_missing_part_exits_2() {
    let dir = TempDir::new().unwrap();
    let path = write_sample(dir.path());

    let output = run(&["cat", path.to_str().unwrap(), "/word/absent.xml"]);
    assert_eq!(output.status.code(), Some(2_i32), "got {output:?}");
    assert_eq!(output.stdout, b"");
    let stderr = str::from_utf8(&output.stderr).unwrap();
    assert_eq!(stderr, "error: the package has no part /word/absent.xml\n");
}
