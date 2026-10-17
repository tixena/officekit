use std::io::{Cursor, Read as _, Write as _};
use std::path::{Path, PathBuf};

use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};
use os_shim::System as _;
use os_shim::mock::MemorySystem;
use zip::ZipArchive;
use zip::write::{SimpleFileOptions, ZipWriter};

use super::{DEFAULT_SUFFIX, DownOptions, FolderError, UpOptions, down, format, up};

const RELS: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
const DOCUMENT: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t xml:space=\"preserve\"> kept </w:t></w:r></w:p></w:body></w:document>";
const TYPES: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/></Types>";
const BINARY: &[u8] = &[0, 159, 146, 150, 10, 32, 32, 60];

fn docx() -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    for (name, bytes) in [
        ("[Content_Types].xml", TYPES),
        ("_rels/.rels", RELS),
        ("word/document.xml", DOCUMENT),
        ("word/media/image1.bin", BINARY),
    ] {
        writer.start_file(name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn system_with(path: &str) -> MemorySystem {
    let system = MemorySystem::new();
    let file = Path::new(path);
    system.create_dir_all(file.parent().unwrap()).unwrap();
    system.write(file, &docx()).unwrap();
    system
}

const fn options(pretty: bool) -> DownOptions<'static> {
    DownOptions {
        pretty,
        suffix: DEFAULT_SUFFIX,
    }
}

const fn up_options(minify: bool) -> UpOptions<'static> {
    UpOptions {
        minify,
        suffix: DEFAULT_SUFFIX,
    }
}

fn bytes_of(system: &MemorySystem, path: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    system
        .open(Path::new(path))
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

fn entries(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut found = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if !entry.is_file() {
            continue;
        }
        let mut contents = Vec::new();
        entry.read_to_end(&mut contents).unwrap();
        found.push((entry.name().unwrap().into_owned(), contents));
    }
    found.sort();
    found
}

fn is_xml(name: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|extension| extension == "xml" || extension == "rels")
}

fn tree(bytes: &[u8], layout: Layout) -> Vec<u8> {
    parse(bytes)
        .unwrap()
        .to_xml(WriteOptions::new(layout, false))
}

#[test]
fn down_writes_every_entry_and_info() {
    let system = system_with("/in/Solicitud Cr\u{e9}dito (1).DOCX");

    let written = down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    let folder = PathBuf::from("/out/solicitud_cr\u{e9}dito_(1)_docx");
    assert_eq!(written, vec![folder.clone()]);
    assert_eq!(
        system
            .read_to_string(&folder.join("_rels/.rels"))
            .unwrap()
            .as_bytes(),
        RELS
    );
    assert_eq!(
        system
            .read_to_string(&folder.join("word/document.xml"))
            .unwrap()
            .as_bytes(),
        DOCUMENT
    );
    assert_eq!(
        system.read_to_string(&folder.join("info.json")).unwrap(),
        "{\n  \"name\": \"Solicitud Cr\u{e9}dito (1).DOCX\"\n}"
    );
}

#[test]
fn folder_name_drops_punctuation_and_lowercases() {
    let system = system_with("/in/A&B, Ltd. #5.docx");

    let written = down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    assert_eq!(written, vec![PathBuf::from("/out/ab_ltd._5_docx")]);
}

#[test]
fn down_keeps_the_directory_of_a_nested_document() {
    let system = system_with("/in/a/b/Deep.docx");

    let written = down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    assert_eq!(written, vec![PathBuf::from("/out/a/b/deep_docx")]);
    let rebuilt = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap();
    assert_eq!(rebuilt, vec![PathBuf::from("/back/a/b/Deep.docx")]);
}

#[test]
fn down_replaces_an_existing_folder() {
    let system = system_with("/in/d.docx");
    system.create_dir_all(Path::new("/out/d_docx")).unwrap();
    system
        .write(Path::new("/out/d_docx/stale.txt"), b"x")
        .unwrap();

    down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    assert!(!system.exists(Path::new("/out/d_docx/stale.txt")).unwrap());
}

#[test]
fn pretty_indents_xml_and_keeps_preserved_space() {
    let system = system_with("/in/d.docx");

    down(&system, Path::new("/in"), Path::new("/out"), options(true)).unwrap();

    let document = system
        .read_to_string(Path::new("/out/d_docx/word/document.xml"))
        .unwrap();
    assert!(document.contains("\n  <w:body>"), "{document}");
    assert!(
        document.contains("<w:t xml:space=\"preserve\"> kept </w:t>"),
        "{document}"
    );
    let binary = bytes_of(&system, "/out/d_docx/word/media/image1.bin");
    assert_eq!(binary, BINARY);
}

#[test]
fn up_zips_everything_but_info() {
    let system = system_with("/in/d.docx");
    down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    let written = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap();

    assert_eq!(written, vec![PathBuf::from("/back/d.docx")]);
    let bytes = bytes_of(&system, "/back/d.docx");
    let names: Vec<String> = entries(bytes).into_iter().map(|(name, _)| name).collect();
    assert_eq!(
        names,
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/media/image1.bin"
        ]
    );
}

#[test]
fn up_appends_docx_to_a_name_without_it() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/out/x_docx")).unwrap();
    system
        .write(Path::new("/out/x_docx/info.json"), br#"{"name":"plain"}"#)
        .unwrap();
    system.write(Path::new("/out/x_docx/a.txt"), b"a").unwrap();

    let written = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap();

    assert_eq!(written, vec![PathBuf::from("/back/plain.docx")]);
}

#[test]
fn up_ignores_a_folder_without_info() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/out/x_docx")).unwrap();
    system.write(Path::new("/out/x_docx/a.txt"), b"a").unwrap();

    let written = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap();

    assert_eq!(written, Vec::<PathBuf>::new());
}

#[test]
fn up_ignores_a_folder_without_the_suffix() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/out/x")).unwrap();
    system
        .write(Path::new("/out/x/info.json"), br#"{"name":"p"}"#)
        .unwrap();

    let written = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap();

    assert_eq!(written, Vec::<PathBuf>::new());
}

#[test]
fn up_reports_a_malformed_info() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/out/x_docx")).unwrap();
    system
        .write(Path::new("/out/x_docx/info.json"), b"{\"name\": 3}")
        .unwrap();

    let error = up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    )
    .unwrap_err();

    assert!(
        matches!(&error, FolderError::Json { error: _error, path } if path == Path::new("/out/x_docx/info.json")),
        "{error}"
    );
}

#[test]
fn round_trip_keeps_trees_and_other_bytes() {
    let system = system_with("/in/d.docx");
    down(&system, Path::new("/in"), Path::new("/out"), options(true)).unwrap();

    up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(true),
    )
    .unwrap();

    let bytes = bytes_of(&system, "/back/d.docx");
    let rebuilt = entries(bytes);
    let original = entries(docx());
    assert_eq!(rebuilt.len(), original.len());
    for ((name, left), (other, right)) in rebuilt.iter().zip(&original) {
        assert_eq!(name, other);
        if is_xml(name) {
            assert_eq!(
                tree(left, Layout::Compact),
                tree(right, Layout::Compact),
                "{name}"
            );
        } else {
            assert_eq!(left, right, "{name}");
        }
    }
}

#[test]
fn format_is_idempotent() {
    let system = system_with("/in/d.docx");
    down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap();

    let first = format(&system, Path::new("/out")).unwrap();
    let second = format(&system, Path::new("/out")).unwrap();

    assert_eq!((first.inspected, first.rewritten), (3, 3));
    assert_eq!((second.inspected, second.rewritten), (3, 0));
    let document = system
        .read_to_string(Path::new("/out/d_docx/word/document.xml"))
        .unwrap();
    assert!(
        document.contains("<w:t xml:space=\"preserve\"> kept </w:t>"),
        "{document}"
    );
}

#[test]
fn format_reports_malformed_xml() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/d")).unwrap();
    system.write(Path::new("/d/a.xml"), b"<a><b></a>").unwrap();

    let error = format(&system, Path::new("/d")).unwrap_err();

    assert!(
        matches!(&error, FolderError::Xml { error: _error, path } if path == Path::new("/d/a.xml")),
        "{error}"
    );
}

#[test]
fn down_rejects_a_missing_source() {
    let system = MemorySystem::new();

    let error = down(
        &system,
        Path::new("/nope"),
        Path::new("/out"),
        options(false),
    )
    .unwrap_err();

    assert!(
        matches!(&error, FolderError::Io { error: _error, path } if path == Path::new("/nope")),
        "{error}"
    );
}

#[test]
fn down_rejects_a_file_that_is_not_a_zip() {
    let system = MemorySystem::new();
    system.create_dir_all(Path::new("/in")).unwrap();
    system
        .write(Path::new("/in/bad.docx"), b"not a zip")
        .unwrap();

    let error = down(&system, Path::new("/in"), Path::new("/out"), options(false)).unwrap_err();

    assert!(
        matches!(&error, FolderError::Io { error: _error, path } if path == Path::new("/in/bad.docx")),
        "{error}"
    );
}
