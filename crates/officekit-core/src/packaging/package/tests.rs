use std::io::{Cursor, Write as _};
use std::path::Path;

use os_shim::System as _;
use os_shim::mock::MemorySystem;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::Package;
use crate::error::PackageError;
use crate::packaging::part_name::PartName;

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn sample() -> Package {
    let mut package = Package::default();
    package.set_part(name("/[Content_Types].xml"), b"<Types/>".to_vec());
    package.set_part(name("/_rels/.rels"), b"<Relationships/>".to_vec());
    package.set_part(name("/word/document.xml"), b"<w:document/>".to_vec());
    package
}

fn names(package: &Package) -> Vec<&str> {
    package.part_names().map(PartName::as_str).collect()
}

#[test]
fn round_trips_through_bytes() {
    let bytes = sample().to_bytes().unwrap();
    let package = Package::from_bytes(bytes).unwrap();
    assert_eq!(
        names(&package),
        ["/[Content_Types].xml", "/_rels/.rels", "/word/document.xml"]
    );
    assert_eq!(
        package.part(&name("/word/document.xml")),
        Some(b"<w:document/>".as_slice())
    );
}

#[test]
fn opens_through_the_system_gate() {
    let system = MemorySystem::new()
        .with_file("/docs/sample.docx", &sample().to_bytes().unwrap())
        .unwrap();
    let package = Package::open(&system, Path::new("/docs/sample.docx")).unwrap();
    assert_eq!(names(&package).len(), 3);
}

#[test]
fn saves_through_the_system_gate() {
    let system = MemorySystem::new().with_dir("/out").unwrap();
    sample()
        .save(&system, Path::new("/out/sample.docx"))
        .unwrap();
    assert!(system.exists(Path::new("/out/sample.docx")).unwrap());
    let reopened = Package::open(&system, Path::new("/out/sample.docx")).unwrap();
    assert_eq!(names(&reopened), names(&sample()));
}

#[test]
fn part_lookup_folds_case() {
    let package = sample();
    assert!(package.part(&name("/WORD/DOCUMENT.XML")).is_some());
}

#[test]
fn set_part_replaces_a_case_variant_and_keeps_the_new_spelling() {
    let mut package = sample();
    package.set_part(
        name("/Word/Document.xml"),
        b"<w:document>2</w:document>".to_vec(),
    );
    assert_eq!(
        names(&package),
        ["/[Content_Types].xml", "/_rels/.rels", "/Word/Document.xml"]
    );
    assert_eq!(
        package.part(&name("/word/document.xml")),
        Some(b"<w:document>2</w:document>".as_slice())
    );
}

#[test]
fn directory_entries_are_not_parts() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    writer.add_directory("word/", options).unwrap();
    writer.start_file("word/document.xml", options).unwrap();
    writer.write_all(b"<w:document/>").unwrap();
    let bytes = writer.finish().unwrap().into_inner();

    let package = Package::from_bytes(bytes).unwrap();
    assert_eq!(names(&package), ["/word/document.xml"]);
}

#[test]
fn rejects_bytes_that_are_not_a_zip() {
    let err = Package::from_bytes(b"not a zip".to_vec()).unwrap_err();
    assert!(matches!(err, PackageError::Container(_)), "got {err:?}");
}

#[test]
fn missing_file_surfaces_as_io() {
    let system = MemorySystem::new();
    let err = Package::open(&system, Path::new("/missing.docx")).unwrap_err();
    assert!(matches!(err, PackageError::Io(_)), "got {err:?}");
}
