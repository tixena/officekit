use core::str;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use officekit_xml::ready::ready_now;
use os_shim::System as _;
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

fn write_sample(path: &Path) {
    let mut package = Package::new();
    package
        .set_part_bytes(
            &PartName::new("/word/document.xml").unwrap(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>",
        )
        .unwrap();
    let system = RealSystem::new();
    ready_now(system.create_dir_all(path.parent().unwrap())).unwrap();
    ready_now(package.save(&system, path)).unwrap();
}

fn text(path: &Path) -> String {
    ready_now(RealSystem::new().read_to_string(path)).unwrap()
}

#[test]
fn down_format_up_round_trip() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write_sample(&root.join("in/sub/My Doc.docx"));
    let (src, expanded, rebuilt) = (root.join("in"), root.join("out"), root.join("back"));

    let expanding = run(&[
        "down",
        "--src",
        src.to_str().unwrap(),
        "--tgt",
        expanded.to_str().unwrap(),
        "--pretty",
    ]);
    assert!(expanding.status.success(), "{expanding:?}");
    let folder = expanded.join("sub/my_doc_docx");
    assert_eq!(
        str::from_utf8(&expanding.stdout).unwrap(),
        format!("expanded: {}\n", folder.display())
    );
    assert!(text(&folder.join("word/document.xml")).contains("\n  <w:body/>"));
    assert_eq!(
        text(&folder.join("info.json")),
        "{\n  \"name\": \"My Doc.docx\"\n}"
    );

    let formatting = run(&["format", "--dir", expanded.to_str().unwrap()]);
    assert!(formatting.status.success(), "{formatting:?}");
    assert_eq!(
        str::from_utf8(&formatting.stdout).unwrap(),
        "inspected 2 XML parts, rewrote 0\n"
    );

    let collapsing = run(&[
        "up",
        "--src",
        expanded.to_str().unwrap(),
        "--tgt",
        rebuilt.to_str().unwrap(),
        "--minify",
    ]);
    assert!(collapsing.status.success(), "{collapsing:?}");
    let file = rebuilt.join("sub/My Doc.docx");
    assert_eq!(
        str::from_utf8(&collapsing.stdout).unwrap(),
        format!("created: {}\n", file.display())
    );
    let package = ready_now(Package::open(&RealSystem::new(), &file)).unwrap();
    let part = package
        .part_bytes(&PartName::new("/word/document.xml").unwrap())
        .unwrap();
    assert!(!part.contains(&b'\n'));
}

#[test]
fn a_custom_suffix_names_the_folder() {
    let dir = TempDir::new().unwrap();
    write_sample(&dir.path().join("in/a.docx"));

    let output = run(&[
        "down",
        "--src",
        dir.path().join("in").to_str().unwrap(),
        "--tgt",
        dir.path().join("out").to_str().unwrap(),
        "--suffix",
        ".x",
    ]);

    assert!(output.status.success(), "{output:?}");
    assert!(ready_now(RealSystem::new().exists(&dir.path().join("out/a.x/info.json"))).unwrap());
}

#[test]
fn a_missing_source_exits_2() {
    let dir = TempDir::new().unwrap();

    let output = run(&[
        "down",
        "--src",
        dir.path().join("none").to_str().unwrap(),
        "--tgt",
        dir.path().join("out").to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2_i32));
    assert!(
        str::from_utf8(&output.stderr)
            .unwrap()
            .starts_with("error: ")
    );
}

#[test]
fn format_of_a_missing_folder_exits_2() {
    let dir = TempDir::new().unwrap();

    let output = run(&["format", "--dir", dir.path().join("none").to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2_i32));
}
