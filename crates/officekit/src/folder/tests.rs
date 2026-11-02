use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};

use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentBody, Paragraph, ParagraphContent, Text, TextRun,
    TextRunContent,
};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::parse::parse;
use officekit_xml::ready::ready_now;
use officekit_xml::write::{Layout, WriteOptions};
use os_shim::System as _;
use os_shim::bucket::Condition;
use os_shim::mock::MemorySystem;
use zip::ZipArchive;

use super::{DEFAULT_SUFFIX, DownOptions, FolderError, UpOptions, down, format, up};
use crate::docx::Docx;
use crate::parts::kinds::WordprocessingKind;

const BINARY: &[u8] = &[0, 159, 146, 150, 10, 32, 32, 60];

/// A Word file whose one paragraph holds " kept " with its spaces preserved, and an opaque
/// media part.
fn typed_docx() -> Docx {
    let mut characters = Text::default();
    characters.space = Some("preserve".to_owned());
    characters.value = " kept ".to_owned();
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::Text(characters)];
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::TextRun(run)];
    let mut body = DocumentBody::default();
    body.content = vec![BlockContent::Paragraph(paragraph)];
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    let mut file = Docx::new(WordprocessingKind::Document).unwrap();
    file.set_main_document(&document).unwrap();
    file.package_mut()
        .set_part_bytes(
            &PartName::new("/word/media/image1.bin").unwrap(),
            "application/octet-stream",
            BINARY,
        )
        .unwrap();
    file
}

fn docx() -> Vec<u8> {
    typed_docx().package().to_bytes().unwrap()
}

fn system_with(path: &str) -> MemorySystem {
    let system = MemorySystem::new();
    let file = Path::new(path);
    ready_now(system.create_dir_all(file.parent().unwrap())).unwrap();
    ready_now(system.write(file, &docx(), Condition::None)).unwrap();
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
    ready_now(system.read(Path::new(path))).unwrap().0
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

    let written = ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    let folder = PathBuf::from("/out/solicitud_cr\u{e9}dito_(1)_docx");
    assert_eq!(written, vec![folder.clone()]);
    let file = typed_docx();
    for (entry, part) in [
        ("_rels/.rels", "/_rels/.rels"),
        ("word/document.xml", "/word/document.xml"),
    ] {
        assert_eq!(
            ready_now(system.read(&folder.join(entry))).unwrap().0,
            file.package()
                .part_bytes(&PartName::new(part).unwrap())
                .unwrap(),
            "{entry}"
        );
    }
    assert_eq!(
        ready_now(system.read_to_string(&folder.join("info.json"))).unwrap(),
        "{\n  \"name\": \"Solicitud Cr\u{e9}dito (1).DOCX\"\n}"
    );
}

#[test]
fn folder_name_drops_punctuation_and_lowercases() {
    let system = system_with("/in/A&B, Ltd. #5.docx");

    let written = ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    assert_eq!(written, vec![PathBuf::from("/out/ab_ltd._5_docx")]);
}

#[test]
fn down_keeps_the_directory_of_a_nested_document() {
    let system = system_with("/in/a/b/Deep.docx");

    let written = ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    assert_eq!(written, vec![PathBuf::from("/out/a/b/deep_docx")]);
    let rebuilt = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap();
    assert_eq!(rebuilt, vec![PathBuf::from("/back/a/b/Deep.docx")]);
}

#[test]
fn down_replaces_an_existing_folder() {
    let system = system_with("/in/d.docx");
    ready_now(system.create_dir_all(Path::new("/out/d_docx"))).unwrap();
    ready_now(system.write(Path::new("/out/d_docx/stale.txt"), b"x", Condition::None)).unwrap();

    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    assert!(!ready_now(system.exists(Path::new("/out/d_docx/stale.txt"))).unwrap());
}

#[test]
fn pretty_indents_xml_and_keeps_preserved_space() {
    let system = system_with("/in/d.docx");

    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(true),
    ))
    .unwrap();

    let document =
        ready_now(system.read_to_string(Path::new("/out/d_docx/word/document.xml"))).unwrap();
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
    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    let written = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
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
    ready_now(system.create_dir_all(Path::new("/out/x_docx"))).unwrap();
    ready_now(system.write(
        Path::new("/out/x_docx/info.json"),
        br#"{"name":"plain"}"#,
        Condition::None,
    ))
    .unwrap();
    ready_now(system.write(Path::new("/out/x_docx/a.txt"), b"a", Condition::None)).unwrap();

    let written = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap();

    assert_eq!(written, vec![PathBuf::from("/back/plain.docx")]);
}

#[test]
fn up_ignores_a_folder_without_info() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/out/x_docx"))).unwrap();
    ready_now(system.write(Path::new("/out/x_docx/a.txt"), b"a", Condition::None)).unwrap();

    let written = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap();

    assert_eq!(written, Vec::<PathBuf>::new());
}

#[test]
fn up_ignores_a_folder_without_the_suffix() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/out/x"))).unwrap();
    ready_now(system.write(
        Path::new("/out/x/info.json"),
        br#"{"name":"p"}"#,
        Condition::None,
    ))
    .unwrap();

    let written = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap();

    assert_eq!(written, Vec::<PathBuf>::new());
}

#[test]
fn up_reports_a_malformed_info() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/out/x_docx"))).unwrap();
    ready_now(system.write(
        Path::new("/out/x_docx/info.json"),
        b"{\"name\": 3}",
        Condition::None,
    ))
    .unwrap();

    let error = ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap_err();

    assert!(
        matches!(&error, FolderError::Json { error: _error, path } if path == Path::new("/out/x_docx/info.json")),
        "{error}"
    );
}

#[test]
fn round_trip_keeps_trees_and_other_bytes() {
    let system = system_with("/in/d.docx");
    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(true),
    ))
    .unwrap();

    ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(true),
    ))
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
    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();

    let first = ready_now(format(&system, Path::new("/out"))).unwrap();
    let second = ready_now(format(&system, Path::new("/out"))).unwrap();

    assert_eq!((first.inspected, first.rewritten), (3, 3));
    assert_eq!((second.inspected, second.rewritten), (3, 0));
    let document =
        ready_now(system.read_to_string(Path::new("/out/d_docx/word/document.xml"))).unwrap();
    assert!(
        document.contains("<w:t xml:space=\"preserve\"> kept </w:t>"),
        "{document}"
    );
}

#[test]
fn format_reports_malformed_xml() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/d"))).unwrap();
    ready_now(system.write(Path::new("/d/a.xml"), b"<a><b></a>", Condition::None)).unwrap();

    let error = ready_now(format(&system, Path::new("/d"))).unwrap_err();

    assert!(
        matches!(&error, FolderError::Xml { error: _error, path } if path == Path::new("/d/a.xml")),
        "{error}"
    );
}

#[test]
fn down_rejects_a_missing_source() {
    let system = MemorySystem::new();

    let error = ready_now(down(
        &system,
        Path::new("/nope"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap_err();

    assert!(
        matches!(&error, FolderError::Io { error: _error, path } if path == Path::new("/nope")),
        "{error}"
    );
}

#[test]
fn down_rejects_a_file_that_is_not_a_zip() {
    let system = MemorySystem::new();
    ready_now(system.create_dir_all(Path::new("/in"))).unwrap();
    ready_now(system.write(Path::new("/in/bad.docx"), b"not a zip", Condition::None)).unwrap();

    let error = ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap_err();

    assert!(
        matches!(&error, FolderError::Io { error: _error, path } if path == Path::new("/in/bad.docx")),
        "{error}"
    );
}

#[test]
fn a_relative_folder_gives_paths_spelled_from_it() {
    let system = MemorySystem::new()
        .with_current_dir("/work")
        .unwrap()
        .with_file("/work/in/a/Report.docx", &docx())
        .unwrap();

    let folders = ready_now(down(
        &system,
        Path::new("in"),
        Path::new("out"),
        options(false),
    ));
    assert_eq!(folders.unwrap(), [PathBuf::from("out/a/report_docx")]);
    assert!(
        ready_now(system.is_file(Path::new("/work/out/a/report_docx/word/document.xml"))).unwrap()
    );

    assert!(
        ready_now(format(&system, Path::new("out")))
            .unwrap()
            .inspected
            > 0
    );

    let files = ready_now(up(
        &system,
        Path::new("out"),
        Path::new("back"),
        up_options(false),
    ));
    assert_eq!(files.unwrap(), [PathBuf::from("back/a/Report.docx")]);
    let rebuilt = entries(bytes_of(&system, "/work/back/a/Report.docx"));
    assert!(rebuilt.iter().any(|(name, _)| name == "word/document.xml"));
}

#[test]
fn up_writes_a_package_with_no_folder_items_and_the_content_types_first() {
    let system = system_with("/in/d.docx");
    ready_now(down(
        &system,
        Path::new("/in"),
        Path::new("/out"),
        options(false),
    ))
    .unwrap();
    ready_now(up(
        &system,
        Path::new("/out"),
        Path::new("/back"),
        up_options(false),
    ))
    .unwrap();

    let mut archive = ZipArchive::new(Cursor::new(bytes_of(&system, "/back/d.docx"))).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .unwrap()
                .name()
                .unwrap()
                .into_owned()
        })
        .collect();
    assert_eq!(
        names.first().map(String::as_str),
        Some("[Content_Types].xml")
    );
    assert!(names.iter().all(|name| !name.ends_with('/')), "{names:?}");
}
