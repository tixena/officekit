use core::future::poll_fn;
use core::pin::pin;
use core::slice;
use core::task::{Context, Poll, Waker};
use std::io::{Cursor, Write as _};
use std::path::Path;

use crate::conformance::ConformanceClass;
use crate::mc::MarkupCompatibility;
use crate::name::OfficeVersion;
use crate::ready::ready_now;
use crate::stream::writer::PartWriter;
use crate::write::Layout;
use os_shim::bucket::{Bucket, Condition, Depth, Guarantees, MemoryBucket, RevisionToken};
use os_shim::error::Result as StorageResult;
use os_shim::mock::MemorySystem;
use os_shim::{BoxFuture, FileMetadata, System as _, WalkEntry};
use zip::write::{SimpleFileOptions, StreamWriter, ZipWriter};

use super::{OpenOptions, Package};
use crate::packaging::error::PackageError;
use crate::packaging::image::{Image, ImageFormat, PixelSize};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{RelationshipType, Target, TargetMode};

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const HEADER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const WORKBOOK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
const HYPERLINK: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const PACKAGE_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";
const IMAGE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
const PNG_PREFIX: [u8; 16] = *b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const PNG_SUFFIX: [u8; 5] = [8, 6, 0, 0, 0];
const JPEG_PREFIX: [u8; 7] = [0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8];
const JPEG_SUFFIX: [u8; 10] = [3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1];
/// The width then the height of a PNG header, four bytes each.
const WIDE: [u8; 8] = [0, 0, 1, 0x2C, 0, 0, 0, 100];
const PORTRAIT: [u8; 8] = [0, 0, 6, 0x1B, 0, 0, 8, 2];
/// The height then the width of a JPEG start-of-frame segment, two bytes each.
const SQUARE: [u8; 4] = [1, 0xC8, 1, 0xC8];
const SMALL: [u8; 4] = [0, 8, 0, 8];
const TINY: [u8; 8] = [0, 0, 0, 1, 0, 0, 0, 1];

/// A memory bucket whose futures wait once before they are ready, as a network bucket's do.
struct WaitingBucket(MemoryBucket);

impl Bucket for WaitingBucket {
    fn delete<'call>(
        &'call self,
        path: &'call Path,
        condition: &'call Condition,
    ) -> BoxFuture<'call, StorageResult<()>> {
        Box::pin(after_a_wait(self.0.delete(path, condition)))
    }

    fn get<'call>(
        &'call self,
        path: &'call Path,
    ) -> BoxFuture<'call, StorageResult<(Vec<u8>, RevisionToken)>> {
        Box::pin(after_a_wait(self.0.get(path)))
    }

    fn guarantees(&self) -> Guarantees {
        Guarantees::none()
    }

    fn head<'call>(
        &'call self,
        path: &'call Path,
    ) -> BoxFuture<'call, StorageResult<FileMetadata>> {
        Box::pin(after_a_wait(self.0.head(path)))
    }

    fn list<'call>(
        &'call self,
        folder: &'call Path,
        depth: Depth,
    ) -> BoxFuture<'call, StorageResult<Vec<WalkEntry>>> {
        Box::pin(after_a_wait(self.0.list(folder, depth)))
    }

    fn put<'call>(
        &'call self,
        path: &'call Path,
        bytes: &'call [u8],
        condition: &'call Condition,
    ) -> BoxFuture<'call, StorageResult<RevisionToken>> {
        Box::pin(after_a_wait(self.0.put(path, bytes, condition)))
    }
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// A package with a main document part related from the package.
fn document() -> Package {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    package
        .set_part_bytes(&main, MAIN, b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>")
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    package
}

/// `package` saved and read back with the relationship part of `source`
/// replaced by `entries`, so they parse as a file gives them.
fn with_relationship_entries(package: &Package, source: &PartName, entries: &str) -> Package {
    let mut written = package.duplicate().unwrap();
    written
        .set_part_bytes(
            &PartName::new(&source.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!("<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{entries}</Relationships>").as_bytes(),
        )
        .unwrap();
    Package::from_bytes(written.to_bytes().unwrap()).unwrap()
}

/// The text of a file inside the zip archive of `package`.
fn archived(package: &Package, path: &str) -> Option<String> {
    let fs = MemorySystem::from_zip_reader(Cursor::new(package.to_bytes().unwrap())).unwrap();
    ready_now(fs.read_to_string(Path::new(path))).ok()
}

#[test]
fn the_end_result_in_use() {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package.set_part_bytes(&main, MAIN, b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>").unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    let (header, id) = package
        .add_part(
            Some(&main),
            RelationshipType::HEADER.transitional,
            HEADER,
            "/word/header{}.xml",
            b"<w:hdr/>",
        )
        .unwrap();
    assert_eq!(
        (header.as_str(), id.as_str()),
        ("/word/header1.xml", "rId1")
    );
    let link = package
        .relate_external(Some(&main), "https://example.org/", HYPERLINK)
        .unwrap();
    assert_eq!(link, "rId2");
    assert_eq!(
        package.related(Some(&main)).unwrap()[1].target,
        Target::External("https://example.org/".to_owned())
    );
    let system = MemorySystem::new().with_dir("/out").unwrap();
    ready_now(package.save(&system, Path::new("/out/new.docx"))).unwrap();
    let reopened = ready_now(Package::open(&system, Path::new("/out/new.docx"))).unwrap();
    assert_eq!(reopened.part_names().unwrap().len(), 2);
}

#[test]
fn an_archive_whose_entries_carry_data_descriptors_opens() {
    let mut writer = ZipWriter::new_stream(Vec::<u8>::new());
    let options = SimpleFileOptions::default();
    writer.start_file("[Content_Types].xml", options).unwrap();
    writer.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>").unwrap();
    writer.start_file("_rels/.rels", options).unwrap();
    writer.write_all(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>").unwrap();
    writer.start_file("word/document.xml", options).unwrap();
    writer.write_all(b"<w:document/>").unwrap();
    let bytes = StreamWriter::into_inner(writer.finish().unwrap());
    // Byte 6 is the low byte of the first local header's flags; bit 3 defers the sizes.
    assert_eq!(
        bytes[6] & 0b1000,
        0b1000,
        "the first entry must defer its sizes"
    );

    let package = Package::from_bytes(bytes).unwrap();
    let main = name("/word/document.xml");
    assert_eq!(package.part_names().unwrap(), slice::from_ref(&main));
    assert_eq!(package.part_bytes(&main).unwrap(), b"<w:document/>");
    assert_eq!(package.content_type(&main), Some(MAIN));
    assert_eq!(package.related(None).unwrap()[0].target, Target::Part(main));
}

#[test]
fn a_new_package_holds_only_its_content_types() {
    let package = Package::new();
    assert_eq!(package.part_names().unwrap(), []);
    assert_eq!(package.relationship_sets().len(), 0);
    assert_eq!(
        archived(&package, "/[Content_Types].xml").unwrap(),
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"/>"
    );
}

#[test]
fn opens_and_saves_through_the_system_gate() {
    let system = MemorySystem::new()
        .with_file("/docs/sample.docx", &document().to_bytes().unwrap())
        .unwrap()
        .with_dir("/out")
        .unwrap();
    let package = ready_now(Package::open(&system, Path::new("/docs/sample.docx"))).unwrap();
    ready_now(package.save(&system, Path::new("/out/copy.docx"))).unwrap();
    let reopened = ready_now(Package::open(&system, Path::new("/out/copy.docx"))).unwrap();
    assert_eq!(reopened.part_names().unwrap(), [name("/word/document.xml")]);
    assert_eq!(
        reopened.content_type(&name("/word/document.xml")),
        Some(MAIN)
    );
}

#[test]
fn a_missing_file_and_bytes_that_are_not_a_zip_are_io_errors() {
    let missing = ready_now(Package::open(
        &MemorySystem::new(),
        Path::new("/missing.docx"),
    ))
    .unwrap_err();
    assert!(matches!(missing, PackageError::Io(_)), "got {missing:?}");
    let not_zip = Package::from_bytes(b"not a zip".to_vec()).unwrap_err();
    assert!(matches!(not_zip, PackageError::Io(_)), "got {not_zip:?}");
}

#[test]
fn a_content_types_part_that_is_not_xml_is_an_xml_error() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("[Content_Types].xml", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"<Types").unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let error = Package::from_bytes(bytes).unwrap_err();
    assert!(
        matches!(&error, PackageError::Xml { error: _error, part } if part.as_str() == "/[Content_Types].xml"),
        "got {error:?}"
    );
}

#[test]
fn part_names_leave_out_the_parts_the_package_keeps_parsed() {
    let package = document();
    assert_eq!(package.part_names().unwrap(), [name("/word/document.xml")]);
    package.part_bytes(&name("/_rels/.rels")).unwrap();
    package.part_bytes(&name("/[Content_Types].xml")).unwrap();
}

#[test]
fn part_lookups_fold_case_and_a_case_variant_replaces_the_part() {
    let mut package = document();
    package.part_bytes(&name("/WORD/DOCUMENT.XML")).unwrap();
    package
        .set_part_bytes(
            &name("/Word/Document.xml"),
            MAIN,
            b"<w:document>2</w:document>",
        )
        .unwrap();
    assert_eq!(package.part_names().unwrap(), [name("/word/document.xml")]);
    assert_eq!(
        package.part_bytes(&name("/word/document.xml")).unwrap(),
        b"<w:document>2</w:document>"
    );
}

#[test]
fn contains_folds_case_and_reports_only_parts_the_archive_holds() {
    let mut package = document();
    assert!(package.contains(&name("/word/document.xml")));
    assert!(package.contains(&name("/WORD/Document.XML")));
    assert!(!package.contains(&name("/word/styles.xml")));
    package.remove_part(&name("/word/document.xml")).unwrap();
    assert!(!package.contains(&name("/word/document.xml")));
}

#[test]
fn a_missing_part_is_missing_part() {
    let error = document()
        .part_bytes(&name("/word/styles.xml"))
        .unwrap_err();
    assert!(
        matches!(&error, PackageError::MissingPart(part) if part.as_str() == "/word/styles.xml"),
        "got {error:?}"
    );
}

#[test]
fn a_relationship_target_is_written_relative_to_its_source_and_resolved_with_dot_dot() {
    let mut package = Package::new();
    let chart = name("/word/charts/chart1.xml");
    let workbook = name("/word/embeddings/sheet.xlsx");
    package
        .set_part_bytes(&chart, CHART, b"<c:chartSpace/>")
        .unwrap();
    package.set_part_bytes(&workbook, WORKBOOK, b"").unwrap();
    package
        .relate(Some(&chart), &workbook, PACKAGE_TYPE)
        .unwrap();

    let reread = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    assert_eq!(
        reread.related(Some(&chart)).unwrap()[0].target,
        Target::Part(workbook)
    );
    let rels = archived(&reread, "/word/charts/_rels/chart1.xml.rels").unwrap();
    assert!(
        rels.contains("Target=\"../embeddings/sheet.xlsx\""),
        "{rels}"
    );
    assert_eq!(
        reread.relationship_sets()[0].1[0].target,
        "../embeddings/sheet.xlsx"
    );
}

#[test]
fn a_new_id_is_one_above_the_highest_so_a_removed_id_is_not_reused() {
    let mut package = document();
    let main = name("/word/document.xml");
    let first = package
        .relate_external(Some(&main), "https://a.example/", HYPERLINK)
        .unwrap();
    let second = package
        .relate_external(Some(&main), "https://b.example/", HYPERLINK)
        .unwrap();
    package.remove_relationship(Some(&main), &first).unwrap();
    let third = package
        .relate_external(Some(&main), "https://c.example/", HYPERLINK)
        .unwrap();
    assert_eq!(
        (first.as_str(), second.as_str(), third.as_str()),
        ("rId1", "rId2", "rId3")
    );
}

#[test]
fn add_relationship_keeps_the_callers_id_and_refuses_a_used_one() {
    let mut package = document();
    let main = name("/word/document.xml");
    let target = Target::External("https://example.org/".to_owned());
    package
        .add_relationship(Some(&main), "rId1", &target, HYPERLINK)
        .unwrap();
    let error = package
        .add_relationship(Some(&main), "rId1", &target, HYPERLINK)
        .unwrap_err();
    assert!(
        matches!(&error, PackageError::DuplicateRelationship { id, part } if id == "rId1" && part.as_ref() == Some(&main)),
        "got {error:?}"
    );
    assert_eq!(package.related(Some(&main)).unwrap().len(), 1);
}

#[test]
fn removing_a_part_removes_its_file_override_own_relationships_and_every_relationship_to_it() {
    let mut package = document();
    let main = name("/word/document.xml");
    let (header, id) = package
        .add_part(
            Some(&main),
            RelationshipType::HEADER.transitional,
            HEADER,
            "/word/header{}.xml",
            b"<w:hdr/>",
        )
        .unwrap();
    let (image, _) = package
        .add_part(
            Some(&header),
            RelationshipType::IMAGE.transitional,
            "image/png",
            "/word/media/image{}.png",
            b"png",
        )
        .unwrap();
    assert_eq!(package.content_type(&header), Some(HEADER));

    assert_eq!(
        package.remove_part(&header).unwrap(),
        [(Some(main.clone()), id)]
    );
    assert!(matches!(
        package.part_bytes(&header),
        Err(PackageError::MissingPart(_))
    ));
    assert_eq!(package.related(Some(&main)).unwrap().len(), 0);
    assert_eq!(
        package.content_type(&header),
        None,
        "a part the package no longer holds has no content type"
    );
    assert!(
        package
            .content_type_overrides()
            .iter()
            .all(|(part, _)| *part != header)
    );
    assert_eq!(package.related(Some(&header)).unwrap().len(), 0);
    assert!(archived(&package, "/word/_rels/header1.xml.rels").is_none());
    assert!(
        archived(&package, "/word/_rels/document.xml.rels").is_none(),
        "the emptied relationship part goes"
    );
    package.part_bytes(&image).unwrap();
}

#[test]
fn a_shared_target_stays_until_its_last_relationship_goes() {
    let mut package = document();
    let main = name("/word/document.xml");
    let (image, first) = package
        .add_part(
            Some(&main),
            RelationshipType::IMAGE.transitional,
            "image/png",
            "/word/media/image{}.png",
            b"png",
        )
        .unwrap();
    let second = package
        .relate(Some(&main), &image, RelationshipType::IMAGE.transitional)
        .unwrap();

    assert_eq!(
        package.remove_relationship(Some(&main), &first).unwrap(),
        None
    );
    package.part_bytes(&image).unwrap();
    assert_eq!(
        package.remove_relationship(Some(&main), &second).unwrap(),
        Some(image.clone())
    );
    assert!(matches!(
        package.part_bytes(&image),
        Err(PackageError::MissingPart(_))
    ));
}

#[test]
fn removing_an_external_relationship_removes_no_part() {
    let mut package = document();
    let main = name("/word/document.xml");
    let id = package
        .relate_external(Some(&main), "https://example.org/", HYPERLINK)
        .unwrap();
    assert_eq!(package.remove_relationship(Some(&main), &id).unwrap(), None);
    assert_eq!(package.part_names().unwrap(), [main]);
}

#[test]
fn removing_an_id_the_source_lacks_is_missing_relationship() {
    let mut package = document();
    let error = package.remove_relationship(None, "rId9").unwrap_err();
    assert!(
        matches!(&error, PackageError::MissingRelationship { id, part: None } if id == "rId9"),
        "got {error:?}"
    );
}

#[test]
fn content_types_take_a_default_per_extension_and_an_override_when_different() {
    let mut package = Package::new();
    package
        .set_part_bytes(&name("/word/media/image1.png"), "image/png", b"png")
        .unwrap();
    package
        .set_part_bytes(&name("/word/media/image2.PNG"), "image/png", b"png")
        .unwrap();
    package
        .set_part_bytes(&name("/word/document.xml"), MAIN, b"<w:document/>")
        .unwrap();
    package
        .set_part_bytes(&name("/customXml/item1.xml"), "application/xml", b"<a/>")
        .unwrap();
    assert_eq!(
        package.content_type_defaults(),
        [
            ("png".to_owned(), "image/png".to_owned()),
            ("xml".to_owned(), "application/xml".to_owned())
        ]
    );
    assert_eq!(
        package.content_type_overrides(),
        [(name("/word/document.xml"), MAIN.to_owned())]
    );

    package
        .relate(
            None,
            &name("/word/document.xml"),
            RelationshipType::OFFICE_DOCUMENT.transitional,
        )
        .unwrap();
    assert_eq!(
        package.content_type_defaults().last(),
        Some(&(
            "rels".to_owned(),
            "application/vnd.openxmlformats-package.relationships+xml".to_owned()
        )),
        "a new relationship part adds the rels Default"
    );
}

#[test]
fn from_parts_rebuilds_a_package_from_its_content_types_relationships_and_parts() {
    let mut original = document();
    let main = name("/word/document.xml");
    original
        .add_part(
            Some(&main),
            RelationshipType::HEADER.transitional,
            HEADER,
            "/word/header{}.xml",
            b"<w:hdr/>",
        )
        .unwrap();
    original
        .relate_external(Some(&main), "https://example.org/", HYPERLINK)
        .unwrap();
    original
        .set_part_bytes(&name("/a.bin"), "application/octet-stream", b"bin")
        .unwrap();
    original.remove_part(&name("/a.bin")).unwrap();
    let parts = original
        .part_names()
        .unwrap()
        .into_iter()
        .map(|part| {
            let bytes = original.part_bytes(&part).unwrap();
            (part, bytes)
        })
        .collect();
    let rebuilt = Package::from_parts(
        original.content_types().clone(),
        original.relationship_sets(),
        parts,
    )
    .unwrap();
    assert!(
        rebuilt
            .content_type_defaults()
            .iter()
            .any(|(extension, _)| extension == "bin"),
        "a Default no part uses stays"
    );
    assert_eq!(rebuilt.relationship_sets(), original.relationship_sets());
    assert_eq!(rebuilt.to_bytes().unwrap(), original.to_bytes().unwrap());
}

#[test]
fn set_relationship_class_respells_the_office_document_family_only() {
    let mut package = document();
    let core = name("/docProps/core.xml");
    package
        .set_part_bytes(
            &core,
            "application/vnd.openxmlformats-package.core-properties+xml",
            b"<cp:coreProperties/>",
        )
        .unwrap();
    package
        .relate(None, &core, RelationshipType::CORE_PROPERTIES.transitional)
        .unwrap();
    package
        .relate_external(
            Some(&name("/word/document.xml")),
            "https://example.org/",
            HYPERLINK,
        )
        .unwrap();

    package
        .set_relationship_class(ConformanceClass::Strict)
        .unwrap();
    let reread = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    let types: Vec<String> = reread
        .relationship_sets()
        .into_iter()
        .flat_map(|(_, items)| items.into_iter().map(|item| item.type_uri))
        .collect();
    assert_eq!(
        types,
        [
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument",
            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/hyperlink",
        ]
    );
    assert_eq!(
        reread
            .related_by_type(None, &RelationshipType::OFFICE_DOCUMENT)
            .unwrap()[0]
            .target,
        Target::Part(name("/word/document.xml"))
    );

    package
        .set_relationship_class(ConformanceClass::Transitional)
        .unwrap();
    assert_eq!(
        package.related(None).unwrap()[0].type_uri,
        RelationshipType::OFFICE_DOCUMENT.transitional
    );
}

fn types_of(package: &Package) -> Vec<String> {
    package
        .relationship_sets()
        .into_iter()
        .flat_map(|(_, items)| items.into_iter().map(|item| item.type_uri))
        .collect()
}

#[test]
fn the_three_irregular_types_respell_both_ways() {
    let mut package = document();
    let parts = [
        ("/docProps/app.xml", RelationshipType::EXTENDED_PROPERTIES),
        ("/docProps/custom.xml", RelationshipType::CUSTOM_PROPERTIES),
        ("/docProps/thumbnail.jpeg", RelationshipType::THUMBNAIL),
    ];
    for (file, known) in parts {
        let part = name(file);
        package
            .set_part_bytes(&part, "application/xml", b"<a/>")
            .unwrap();
        package.relate(None, &part, known.transitional).unwrap();
    }

    package
        .set_relationship_class(ConformanceClass::Strict)
        .unwrap();
    let strict = types_of(&Package::from_bytes(package.to_bytes().unwrap()).unwrap());
    assert_eq!(
        strict,
        [
            "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/customProperties",
            "http://purl.oclc.org/ooxml/officeDocument/relationships/metadata/thumbnail",
        ]
    );

    package
        .set_relationship_class(ConformanceClass::Transitional)
        .unwrap();
    let transitional = types_of(&Package::from_bytes(package.to_bytes().unwrap()).unwrap());
    assert_eq!(
        transitional,
        [
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties",
            "http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail",
        ]
    );
}

#[test]
fn every_known_type_respells_to_strict_and_back_to_its_own_transitional() {
    for known in RelationshipType::ALL {
        assert_eq!(RelationshipType::find(known.strict), Some(*known));
        assert_eq!(RelationshipType::find(known.transitional), Some(*known));
        assert_eq!(known.in_class(ConformanceClass::Strict), known.strict);
        assert_eq!(
            known.in_class(ConformanceClass::Transitional),
            known.transitional
        );
    }
}

#[test]
fn a_type_the_table_does_not_know_is_left_as_written() {
    const EXTENSION: &str = "http://schemas.microsoft.com/office/2011/relationships/webextension";
    let mut package = document();
    package
        .relate_external(None, "https://example.org/", EXTENSION)
        .unwrap();
    package
        .set_relationship_class(ConformanceClass::Strict)
        .unwrap();
    assert!(types_of(&package).contains(&EXTENSION.to_owned()));
    package
        .set_relationship_class(ConformanceClass::Transitional)
        .unwrap();
    assert!(types_of(&package).contains(&EXTENSION.to_owned()));
}

#[test]
fn a_custom_xml_part_comes_with_its_properties_part() {
    let mut package = document();
    let main = name("/word/document.xml");
    let item = package
        .add_custom_xml(
            &main,
            b"<DocumentGeneration xmlns=\"http://documentgeneration.prodoctivity.com/2011/\"/>",
            "{12A8E063-9BB3-4501-8935-A4492A4C1887}",
            &["http://documentgeneration.prodoctivity.com/2011/"],
        )
        .unwrap();
    assert_eq!(item.as_str(), "/customXml/item1.xml");
    assert_eq!(package.content_type(&item), Some("application/xml"));
    let from_main = package
        .related_by_type(Some(&main), &RelationshipType::CUSTOM_XML)
        .unwrap();
    assert_eq!(from_main[0].target, Target::Part(item.clone()));
    let properties = package
        .related_by_type(Some(&item), &RelationshipType::CUSTOM_XML_PROPERTIES)
        .unwrap();
    assert_eq!(
        properties[0].target,
        Target::Part(name("/customXml/itemProps1.xml"))
    );
    assert_eq!(
        properties[0].content_type.as_deref(),
        Some("application/vnd.openxmlformats-officedocument.customXmlProperties+xml")
    );
    let text = String::from_utf8(
        package
            .part_bytes(&name("/customXml/itemProps1.xml"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        text,
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\r\n<ds:datastoreItem xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\" ds:itemID=\"{12A8E063-9BB3-4501-8935-A4492A4C1887}\"><ds:schemaRefs><ds:schemaRef ds:uri=\"http://documentgeneration.prodoctivity.com/2011/\"/></ds:schemaRefs></ds:datastoreItem>"
    );
}

#[test]
fn a_custom_xml_part_takes_a_number_free_for_both_its_names() {
    let mut package = document();
    package
        .set_part_bytes(
            &name("/customXml/itemProps1.xml"),
            "application/xml",
            b"<a/>",
        )
        .unwrap();
    let item = package
        .add_custom_xml(&name("/word/document.xml"), b"<a/>", "{1}", &[])
        .unwrap();
    assert_eq!(item.as_str(), "/customXml/item2.xml");
    package
        .part_bytes(&name("/customXml/itemProps2.xml"))
        .unwrap();
}

#[test]
fn free_part_name_takes_the_first_number_no_part_has_compared_case_folded() {
    let mut package = document();
    package
        .set_part_bytes(&name("/word/Header1.xml"), HEADER, b"<w:hdr/>")
        .unwrap();
    assert_eq!(
        package.free_part_name("/word/header{}.xml").unwrap(),
        name("/word/header2.xml")
    );
    let error = package.free_part_name("/word/header.xml").unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::PartName {
                name: _name,
                reason: _reason
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn load_tree_applies_markup_compatibility_as_the_options_say() {
    let mut package = document();
    let main = name("/word/document.xml");
    package
        .set_part_bytes(&main, MAIN, b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w16se=\"http://schemas.microsoft.com/office/word/2015/wordml/symex\"><mc:AlternateContent><mc:Choice Requires=\"w16se\"><w16se:symEx/></mc:Choice><mc:Fallback><w:t/></mc:Fallback></mc:AlternateContent></w:document>")
        .unwrap();
    let kept = package.load_tree(&main, OpenOptions::default()).unwrap();
    assert_eq!(
        kept.root.child_elements().next().unwrap().name.local,
        "AlternateContent"
    );
    let options = OpenOptions {
        markup_compatibility: MarkupCompatibility::Process {
            target: OfficeVersion::Office2010,
        },
    };
    let processed = package.load_tree(&main, options).unwrap();
    assert_eq!(
        processed.root.child_elements().next().unwrap().name.local,
        "t"
    );
}

#[test]
fn duplicate_is_independent_of_the_original() {
    let original = document();
    let mut copy = original.duplicate().unwrap();
    copy.add_part(
        Some(&name("/word/document.xml")),
        RelationshipType::HEADER.transitional,
        HEADER,
        "/word/header{}.xml",
        b"<w:hdr/>",
    )
    .unwrap();
    assert_eq!(original.part_names().unwrap().len(), 1);
    assert_eq!(copy.part_names().unwrap().len(), 2);
}

#[test]
fn relationship_sets_list_each_source_with_its_entries_as_written() {
    let mut package = document();
    package
        .relate_external(
            Some(&name("/word/document.xml")),
            "https://example.org/",
            HYPERLINK,
        )
        .unwrap();
    let sets = package.relationship_sets();
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].0, None);
    assert_eq!(sets[0].1[0].target, "word/document.xml");
    assert_eq!(sets[1].0, Some(name("/word/document.xml")));
    assert_eq!(sets[1].1[0].target_mode, TargetMode::External);
    assert!(format!("{package:?}").starts_with("Package { content_types: "));
}

#[test]
fn relationships_of_gives_one_result_per_entry_and_related_fails_on_a_bad_target() {
    let main = name("/word/document.xml");
    let package = with_relationship_entries(
        &document(),
        &main,
        r#"<Relationship Id="rId1" Type="http://example.com/a" Target="styles.xml"/><Relationship Id="rId2" Type="http://example.com/b" Target="media/"/><Relationship Id="rId3" Type="http://example.com/c" Target="https://example.org/" TargetMode="External"/>"#,
    );
    let each = package.relationships_of(Some(&main));
    assert_eq!(each.len(), 3);
    assert_eq!(
        each[0].as_ref().unwrap().target,
        Target::Part(name("/word/styles.xml"))
    );
    assert!(each[1].is_err(), "got {each:?}");
    assert_eq!(
        each[2].as_ref().unwrap().target,
        Target::External("https://example.org/".to_owned())
    );
    let error = package.related(Some(&main)).unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::PartName {
                name: _name,
                reason: _reason
            }
        ),
        "got {error:?}"
    );
    assert!(
        package
            .relationships_of(Some(&name("/word/none.xml")))
            .is_empty()
    );
}

fn png_bytes(width_and_height: [u8; 8]) -> Vec<u8> {
    [&PNG_PREFIX[..], &width_and_height, &PNG_SUFFIX].concat()
}

fn jpeg_bytes(height_and_width: [u8; 4]) -> Vec<u8> {
    [&JPEG_PREFIX[..], &height_and_width, &JPEG_SUFFIX].concat()
}

/// A document whose footer relates to `/word/media/image1.png` as `rId1`, the document also
/// relating to it as `rId7`.
fn with_footer_image() -> (Package, PartName, PartName) {
    let mut package = document();
    let main = name("/word/document.xml");
    let footer = name("/word/footer1.xml");
    let picture = name("/word/media/image1.png");
    package
        .set_part_bytes(
            &footer,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
            b"<w:ftr/>",
        )
        .unwrap();
    package
        .relate(Some(&main), &footer, RelationshipType::FOOTER.transitional)
        .unwrap();
    package
        .set_part_bytes(&picture, "image/png", &png_bytes(PORTRAIT))
        .unwrap();
    package
        .add_relationship(
            Some(&footer),
            "rId1",
            &Target::Part(picture.clone()),
            IMAGE_RELATIONSHIP,
        )
        .unwrap();
    package
        .add_relationship(
            Some(&main),
            "rId7",
            &Target::Part(picture.clone()),
            IMAGE_RELATIONSHIP,
        )
        .unwrap();
    (package, footer, picture)
}

#[test]
fn an_image_part_reads_as_an_image_with_its_pixel_size() {
    let (package, _, picture) = with_footer_image();
    let image = package.image(&picture).unwrap();
    assert_eq!(image.format(), &ImageFormat::Png);
    assert_eq!(
        image.pixel_size(),
        Some(PixelSize {
            height: 2050,
            width: 1563
        })
    );
}

#[test]
fn a_part_that_is_not_an_image_is_a_content_type_error() {
    let package = document();
    let error = package.image(&name("/word/document.xml")).unwrap_err();
    assert!(error.to_string().ends_with(", not image/*"));
}

#[test]
fn set_image_of_the_same_format_replaces_the_bytes() {
    let (mut package, _, picture) = with_footer_image();
    let image = Image::from_bytes(png_bytes(WIDE)).unwrap();
    assert_eq!(package.set_image(&picture, &image).unwrap(), picture);
    assert_eq!(package.part_bytes(&picture).unwrap(), image.bytes());
    assert_eq!(package.part_names().unwrap().len(), 3);
}

#[test]
fn set_image_of_another_format_moves_it_and_retargets_every_relationship() {
    let (mut package, footer, picture) = with_footer_image();
    let image = Image::from_bytes(jpeg_bytes(SQUARE)).unwrap();
    let moved = package.set_image(&picture, &image).unwrap();
    assert_eq!(moved.as_str(), "/word/media/image1.jpeg");
    assert_eq!(package.content_type(&moved), Some("image/jpeg"));
    assert_eq!(package.part_bytes(&moved).unwrap(), image.bytes());
    assert!(!package.contains(&picture));
    for source in [&footer, &name("/word/document.xml")] {
        let image_relationships = package
            .related_by_type(Some(source), &RelationshipType::IMAGE)
            .unwrap();
        assert_eq!(image_relationships.len(), 1);
        assert_eq!(image_relationships[0].target, Target::Part(moved.clone()));
    }
    assert_eq!(
        package.related(Some(&footer)).unwrap()[0].id,
        "rId1".to_owned()
    );
    assert_eq!(
        package
            .related(Some(&name("/word/document.xml")))
            .unwrap()
            .iter()
            .filter(|related| related.id == "rId7")
            .count(),
        1
    );
}

#[test]
fn set_image_removes_the_old_parts_override() {
    let (mut package, _, picture) = with_footer_image();
    package
        .set_part_bytes(&picture, "image/x-special", &png_bytes(TINY))
        .unwrap();
    assert!(
        package
            .content_type_overrides()
            .iter()
            .any(|(part, _)| *part == picture)
    );
    let image = Image::from_bytes(jpeg_bytes(SMALL)).unwrap();
    package.set_image(&picture, &image).unwrap();
    assert!(
        package
            .content_type_overrides()
            .iter()
            .all(|(part, _)| *part != picture)
    );
}

#[test]
fn set_image_takes_the_first_free_number() {
    let (mut package, _, picture) = with_footer_image();
    package
        .set_part_bytes(&name("/word/media/image1.jpeg"), "image/jpeg", b"x")
        .unwrap();
    let image = Image::from_bytes(jpeg_bytes(SMALL)).unwrap();
    assert_eq!(
        package.set_image(&picture, &image).unwrap().as_str(),
        "/word/media/image2.jpeg"
    );
}

#[test]
fn add_image_takes_the_first_free_name_in_the_sources_media_folder() {
    let mut package = document();
    let main = name("/word/document.xml");
    let image = Image::from_bytes(png_bytes(WIDE)).unwrap();
    let (first, id) = package.add_image(&main, &image).unwrap();
    assert_eq!(
        (first.as_str(), id.as_str()),
        ("/word/media/image1.png", "rId1")
    );
    let (second, second_id) = package.add_image(&main, &image).unwrap();
    assert_eq!(
        (second.as_str(), second_id.as_str()),
        ("/word/media/image2.png", "rId2")
    );
    assert_eq!(package.content_type(&first), Some("image/png"));
    let related = package.related(Some(&main)).unwrap();
    assert_eq!(related[0].type_uri, IMAGE_RELATIONSHIP);
    assert_eq!(related[0].target, Target::Part(first));
}

#[test]
fn add_image_relates_with_the_type_of_the_packages_class() {
    let mut package = document();
    let main = name("/word/document.xml");
    package
        .set_relationship_class(ConformanceClass::Strict)
        .unwrap();
    let image = Image::from_bytes(png_bytes(WIDE)).unwrap();
    package.add_image(&main, &image).unwrap();
    assert_eq!(
        package.related(Some(&main)).unwrap()[0].type_uri,
        RelationshipType::IMAGE.strict
    );
}

#[test]
fn an_absent_part_has_no_content_type_whatever_its_extension_defaults_to() {
    let mut package = Package::new();
    package
        .set_part_bytes(&name("/customXml/item1.xml"), "application/xml", b"<a/>")
        .unwrap();
    assert_eq!(
        package.content_type(&name("/customXml/item1.xml")),
        Some("application/xml")
    );
    assert_eq!(package.content_type(&name("/word/x.xml")), None);
}

#[test]
fn a_part_read_from_its_package_streams_a_root_that_declares_only_the_prefixes_it_uses() {
    const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
    const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
    let main = PartName::new("/word/document.xml").unwrap();
    let declarations =
        format!("xmlns:w=\"{W}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" xmlns:w15=\"{W15}\"");
    let mut package = Package::new();
    package
        .set_part_bytes(
            &main,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            format!(
                "<w:document {declarations} mc:Ignorable=\"w14 w15\"><w:body>\n  <w:p w14:paraId=\"00000001\"/>\n</w:body></w:document>"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut reader = package.part_reader(&main).unwrap();
    let mut sink = Vec::new();
    let mut out = PartWriter::new(&mut sink, Layout::Compact);
    while let Some(event) = reader.read_event().unwrap() {
        out.write_event(event).unwrap();
    }
    assert_eq!(
        String::from_utf8(sink).unwrap(),
        format!(
            "<w:document xmlns:w=\"{W}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" mc:Ignorable=\"w14\">\
             <w:body><w:p w14:paraId=\"00000001\"/></w:body></w:document>"
        )
    );
}

/// What `future` gives, after one poll that finds it waiting.
async fn after_a_wait<T>(future: impl Future<Output = T> + Send) -> T {
    let mut waited = false;
    poll_fn(|_context| {
        if waited {
            Poll::Ready(())
        } else {
            waited = true;
            Poll::Pending
        }
    })
    .await;
    future.await
}

/// What `future` gives, polled until it is ready.
fn polled_to_its_end<F>(future: F) -> F::Output
where
    F: Future,
{
    let mut pinned = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(output) = pinned.as_mut().poll(&mut context) {
            return output;
        }
    }
}

#[test]
fn a_package_opens_and_saves_through_a_system_whose_futures_wait() {
    let system = MemorySystem::with_bucket(WaitingBucket(MemoryBucket::default()));
    let path = Path::new("/new.docx");
    let main = PartName::new("/word/document.xml").unwrap();
    let mut package = Package::new();
    package
        .set_part_bytes(&main, MAIN, b"<w:document/>")
        .unwrap();

    polled_to_its_end(package.save(&system, path)).unwrap();
    let reopened = polled_to_its_end(Package::open(&system, path)).unwrap();
    assert_eq!(reopened.part_names().unwrap(), [main]);

    let refused = ready_now(Package::open(&system, path)).unwrap_err();
    assert_eq!(
        refused.to_string(),
        "the future is not ready on its first poll"
    );
}
