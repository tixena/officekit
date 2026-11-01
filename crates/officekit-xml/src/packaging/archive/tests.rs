use std::io::Cursor;

use zip::{CompressionMethod, DateTime, HasZipMetadata as _, System, ZipArchive};

use crate::packaging::package::Package;
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::RelationshipType;

const OFFICE_MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

fn sample() -> Vec<u8> {
    let mut package = Package::new();
    for (name, bytes) in [
        ("/customXml/item1.xml", b"<a/>".as_slice()),
        ("/docProps/app.xml", b"<b/>".as_slice()),
        ("/word/media/image1.png", b"png".as_slice()),
        ("/word/document.xml", b"<c/>".as_slice()),
    ] {
        package
            .set_part_bytes(&PartName::new(name).unwrap(), OFFICE_MAIN, bytes)
            .unwrap();
    }
    package
        .relate(
            None,
            &PartName::new("/word/document.xml").unwrap(),
            RelationshipType::OFFICE_DOCUMENT.transitional,
        )
        .unwrap();
    package.to_bytes().unwrap()
}

fn names(bytes: &[u8]) -> Vec<String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .unwrap()
                .name()
                .unwrap()
                .into_owned()
        })
        .collect()
}

#[test]
fn a_saved_package_holds_no_folder_item() {
    let found = names(&sample());
    assert!(found.iter().all(|name| !name.ends_with('/')), "{found:?}");
}

#[test]
fn a_saved_package_starts_with_the_content_types_then_the_relationships_then_the_main_part() {
    let found = names(&sample());
    assert_eq!(
        found[..3],
        ["[Content_Types].xml", "_rels/.rels", "word/document.xml"]
    );
}

#[test]
fn a_saved_entry_takes_no_zip64_and_needs_only_version_two_of_zip() {
    let bytes = sample();
    assert!(!bytes.windows(4).any(|window| window == [0x50, 0x4b, 6, 6]));
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    for index in 0..archive.len() {
        let entry = archive.by_index(index).unwrap();
        assert_eq!(entry.compression(), CompressionMethod::Deflated);
        assert_eq!(entry.version_needed(), 20);
        assert_eq!(entry.get_metadata().system, System::Dos);
        assert_eq!(entry.last_modified(), Some(DateTime::default()));
        assert!(entry.extra_data().as_deref().is_none_or(<[u8]>::is_empty));
    }
}

#[test]
fn a_package_saved_twice_gives_the_same_bytes() {
    let package = Package::from_bytes(sample()).unwrap();
    assert_eq!(package.to_bytes().unwrap(), package.to_bytes().unwrap());
}

#[test]
fn a_saved_package_reopens_with_its_parts() {
    let package = Package::from_bytes(sample()).unwrap();
    assert_eq!(package.part_names().unwrap().len(), 4);
    assert_eq!(
        package
            .part_bytes(&PartName::new("/word/document.xml").unwrap())
            .unwrap(),
        b"<c/>"
    );
}
