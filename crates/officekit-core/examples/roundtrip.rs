//! Reads every XML part of every Office file in a folder through the generic
//! element tree and checks the round trip of D29:
//!
//! - every part parses;
//! - written compact without the cleanup, every part reads back to a tree that
//!   writes the same bytes;
//! - the compact form of every part's pretty form is its compact form;
//! - a part either comes back byte for byte, or differs from what was read
//!   only in the six spellings the tree does not keep, counted by kind;
//! - how many parts the namespace cleanup changes, and how many markup
//!   compatibility processed for Office 2010 changes.
//!
//! For each Word file it also prints what the cleanup leaves on the root of
//! `word/document.xml`.
//!
//! ```text
//! cargo run -p officekit-core --example roundtrip -- notes/samples
//! ```

use core::error::Error;
use core::mem;
use std::io::{Cursor, Read as _, Write, stdout};
use std::path::{Path, PathBuf};

use clap::Parser;
use officekit_core::xml::element::Document;
use officekit_core::xml::mc::MarkupCompatibility;
use officekit_core::xml::name::OfficeVersion;
use officekit_core::xml::parse::parse;
use officekit_core::xml::write::{Layout, WriteOptions};
use os_shim::System;
use os_shim::mock::MemorySystem;
use os_shim::real::RealSystem;
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// The spellings the tree does not keep, in the order section 6 lists them.
const KINDS: [&str; KIND_COUNT] = [
    "empty element written as a start and an end tag",
    "namespace declared after an attribute",
    "whitespace between elements",
    "apostrophe in text written as &apos;",
    "more than one space between attributes",
    "line break \\r\\n inside text",
];
const KIND_COUNT: usize = 6;
const EMPTY_ELEMENT: usize = 0;
const DECLARATION_AFTER_ATTRIBUTE: usize = 1;
const WHITESPACE: usize = 2;
const APOSTROPHE: usize = 3;
const ATTRIBUTE_SPACING: usize = 4;
const LINE_BREAK: usize = 5;

/// Check the generic tree's round trip over a folder of Office files.
#[derive(Parser)]
struct Arguments {
    /// The folder whose `.docx`, `.xlsx`, and `.pptx` files are read.
    folder: PathBuf,
}

/// What the run found over one file, or over all of them.
#[derive(Default)]
struct Counts {
    /// Parts the cleanup changed.
    cleaned: usize,
    /// Parts that differ only in the spellings of `KINDS`.
    explained: usize,
    /// The differing parts by the kind of their first difference.
    first_kinds: [usize; KIND_COUNT],
    /// Parts whose compact form reads back to a tree that writes it again.
    fixed: usize,
    /// Parts written compact exactly as read.
    identical: usize,
    /// Parts holding each kind of `KINDS`.
    kinds: [usize; KIND_COUNT],
    /// Parts processing markup compatibility for Office 2010 changed.
    mc_changed: usize,
    /// Parts that parsed.
    parsed: usize,
    /// XML parts read.
    parts: usize,
    /// Parts whose pretty form compacts to their compact form.
    pretty: usize,
}

/// The compact form a part is expected to take, predicted from its bytes by
/// undoing only the spellings of `KINDS`, and which of them it holds.
#[derive(Default)]
struct Prediction {
    /// Whether a difference outside `KINDS` was found in a start tag.
    elsewhere: bool,
    /// The kind of the first difference, in document order.
    first: Option<usize>,
    kinds: [bool; KIND_COUNT],
    out: String,
    /// The start tag waiting for its first child: an end tag next makes it an
    /// empty element. The flag tells whether whitespace was dropped since.
    pending: Option<(String, bool)>,
    preserve: Vec<bool>,
    text: String,
}

impl Counts {
    fn add(&mut self, other: &Self) {
        self.cleaned += other.cleaned;
        self.explained += other.explained;
        for (total, count) in self.first_kinds.iter_mut().zip(other.first_kinds) {
            *total += count;
        }
        self.fixed += other.fixed;
        self.identical += other.identical;
        for (total, count) in self.kinds.iter_mut().zip(other.kinds) {
            *total += count;
        }
        self.mc_changed += other.mc_changed;
        self.parsed += other.parsed;
        self.parts += other.parts;
        self.pretty += other.pretty;
    }

    fn report(&self, out: &mut dyn Write) -> Result<(), Box<dyn Error>> {
        writeln!(
            out,
            "  parts {}, parsed {}, compact fixed point {}, pretty compacts back {}, byte for \
             byte {}, differing only in the six ways {}, cleanup changed {}, mc for Office 2010 \
             changed {}",
            self.parts,
            self.parsed,
            self.fixed,
            self.pretty,
            self.identical,
            self.explained,
            self.cleaned,
            self.mc_changed,
        )?;
        for ((kind, first), holding) in KINDS.iter().zip(self.first_kinds).zip(self.kinds) {
            if holding > 0 {
                writeln!(
                    out,
                    "    {kind}: first difference in {first}, held by {holding}"
                )?;
            }
        }
        Ok(())
    }
}

impl Prediction {
    /// The part holds a difference of `kind`.
    fn flag(&mut self, kind: usize) {
        self.kinds[kind] = true;
        self.first.get_or_insert(kind);
    }

    /// The text read since the last markup, inside the root.
    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let raw = mem::take(&mut self.text);
        let preserved = self.preserve.last().copied().unwrap_or(false);
        if !preserved
            && raw
                .chars()
                .all(|character| matches!(character, ' ' | '\t' | '\n' | '\r'))
        {
            self.flag(WHITESPACE);
            if let Some((_, dropped)) = &mut self.pending {
                *dropped = true;
            }
            return;
        }
        self.open_pending();
        let mut spelled = raw;
        if spelled.contains("&apos;") {
            self.flag(APOSTROPHE);
            spelled = spelled.replace("&apos;", "'");
        }
        if spelled.contains("\r\n") {
            self.flag(LINE_BREAK);
            spelled = spelled.replace("\r\n", "\n");
        }
        self.out.push_str(&spelled);
    }

    /// Write the waiting start tag: a child follows it.
    fn open_pending(&mut self) {
        if let Some((tag, _)) = self.pending.take() {
            self.out.push('<');
            self.out.push_str(&tag);
            self.out.push('>');
        }
    }

    /// Read the original bytes event by event.
    fn read(mut self, original: &[u8]) -> Result<Self, Box<dyn Error>> {
        let mut reader = Reader::from_reader(original);
        reader.config_mut().trim_text(false);
        loop {
            let event = reader.read_event()?;
            if !matches!(event, Event::Text(_) | Event::GeneralRef(_)) {
                self.flush_text();
            }
            match event {
                Event::CData(content) => {
                    self.open_pending();
                    self.out.push_str("<![CDATA[");
                    self.out.push_str(&content);
                    self.out.push_str("]]>");
                }
                Event::Comment(content) => {
                    self.open_pending();
                    self.out.push_str("<!--");
                    self.out.push_str(&content);
                    self.out.push_str("-->");
                }
                Event::Decl(content) => {
                    self.out.push_str("<?");
                    self.out.push_str(&content);
                    self.out.push_str("?>");
                }
                Event::DocType(_) => return Err("a document type declaration".into()),
                Event::Empty(start) => {
                    self.open_pending();
                    let tag = self.tag(&start)?;
                    self.out.push('<');
                    self.out.push_str(&tag);
                    self.out.push_str("/>");
                }
                Event::End(end) => {
                    if let Some((tag, dropped)) = self.pending.take() {
                        if !dropped {
                            self.flag(EMPTY_ELEMENT);
                        }
                        self.out.push('<');
                        self.out.push_str(&tag);
                        self.out.push_str("/>");
                    } else {
                        self.out.push_str("</");
                        self.out.push_str(end.name().into_inner());
                        self.out.push('>');
                    }
                    self.preserve.pop();
                }
                Event::Eof => break,
                Event::GeneralRef(reference) => {
                    self.text.push('&');
                    self.text.push_str(&reference);
                    self.text.push(';');
                }
                Event::PI(instruction) => {
                    self.open_pending();
                    self.out.push_str("<?");
                    self.out.push_str(instruction.target());
                    let content = instruction.content().trim_start();
                    if !content.is_empty() {
                        self.out.push(' ');
                        self.out.push_str(content);
                    }
                    self.out.push_str("?>");
                }
                Event::Start(start) => {
                    self.open_pending();
                    let tag = self.tag(&start)?;
                    let around = self.preserve.last().copied().unwrap_or(false);
                    let space = start
                        .try_get_attribute("xml:space")?
                        .map_or(around, |attribute| attribute.value == "preserve");
                    self.preserve.push(space);
                    self.pending = Some((tag, false));
                }
                Event::Text(content) if self.preserve.is_empty() => self.out.push_str(&content),
                Event::Text(content) => self.text.push_str(&content),
            }
        }
        Ok(self)
    }

    /// A start tag's content with the declarations first and one space
    /// between attributes, the values as read.
    fn tag(&mut self, start: &BytesStart<'_>) -> Result<String, Box<dyn Error>> {
        let name = start.name().into_inner();
        let mut declarations = Vec::new();
        let mut attributes = Vec::new();
        let mut in_order = name.to_owned();
        for read in start.attributes() {
            let attribute = read?;
            let key = attribute.key.into_inner();
            let written = format!("{key}=\"{}\"", attribute.value);
            in_order.push(' ');
            in_order.push_str(&written);
            if key == "xmlns" || key.starts_with("xmlns:") {
                if !attributes.is_empty() {
                    self.flag(DECLARATION_AFTER_ATTRIBUTE);
                }
                declarations.push(written);
            } else {
                attributes.push(written);
            }
        }
        let raw: &str = start;
        if raw != in_order {
            if collapse_spaces(raw) == in_order {
                self.flag(ATTRIBUTE_SPACING);
            } else {
                self.elsewhere = true;
            }
        }
        let mut tag = name.to_owned();
        for written in declarations.iter().chain(&attributes) {
            tag.push(' ');
            tag.push_str(written);
        }
        Ok(tag)
    }
}

/// Check every XML part of one Office file.
fn check_file<S>(system: &S, path: &Path, out: &mut dyn Write) -> Result<Counts, Box<dyn Error>>
where
    S: System + ?Sized,
{
    let package = MemorySystem::from_zip_reader(Cursor::new(read_bytes(system, path)?))?;
    let mut entries: Vec<PathBuf> = package
        .walk_dir(Path::new("/"), false, true)?
        .into_iter()
        .filter(|entry| entry.is_file)
        .map(|entry| entry.path)
        .filter(|entry| is_xml_part(entry))
        .collect();
    entries.sort();
    let mut counts = Counts::default();
    for entry in &entries {
        let original = read_bytes(&package, entry)?;
        counts.parts += 1;
        let document = match parse(&original) {
            Ok(document) => document,
            Err(error) => {
                writeln!(out, "  {} does not parse: {error}", entry.display())?;
                continue;
            }
        };
        counts.parsed += 1;
        check_part(&document, &original, &mut counts, out, entry)?;
        if entry == Path::new("/word/document.xml") {
            root_after_cleanup(&document, out)?;
        }
    }
    Ok(counts)
}

/// Check one part, `original` being its bytes.
fn check_part(
    document: &Document,
    original: &[u8],
    counts: &mut Counts,
    out: &mut dyn Write,
    entry: &Path,
) -> Result<(), Box<dyn Error>> {
    let keep = options(Layout::Compact, false);
    let compact = document.to_xml(keep);
    counts.fixed += usize::from(parse(&compact)?.to_xml(keep) == compact);
    let pretty = document.to_xml(options(Layout::Pretty, false));
    counts.pretty += usize::from(parse(&pretty)?.to_xml(keep) == compact);
    counts.cleaned += usize::from(document.to_xml(WriteOptions::default()) != compact);
    let mut processed = document.clone();
    processed.process_markup_compatibility(MarkupCompatibility::Process {
        target: OfficeVersion::Office2010,
    })?;
    counts.mc_changed += usize::from(processed != *document);
    if compact == original {
        counts.identical += 1;
        return Ok(());
    }
    let prediction = Prediction::default().read(original)?;
    if !prediction.elsewhere && prediction.out.as_bytes() == compact {
        counts.explained += 1;
        if let Some(first) = prediction.first {
            counts.first_kinds[first] += 1;
        }
        for (total, held) in counts.kinds.iter_mut().zip(prediction.kinds) {
            *total += usize::from(held);
        }
    } else {
        writeln!(out, "  {} differs in another way", entry.display())?;
    }
    Ok(())
}

/// A start tag's content with every run of whitespace between attributes
/// made one space.
fn collapse_spaces(raw: &str) -> String {
    let mut collapsed = String::with_capacity(raw.len());
    let mut quote = None;
    let mut spacing = false;
    for character in raw.chars() {
        if quote.is_none() && character.is_ascii_whitespace() {
            spacing = true;
            continue;
        }
        if spacing {
            collapsed.push(' ');
            spacing = false;
        }
        collapsed.push(character);
        if quote == Some(character) {
            quote = None;
        } else if quote.is_none() && matches!(character, '"' | '\'') {
            quote = Some(character);
        } else {
            // Inside a value, or between attributes.
        }
    }
    collapsed
}

/// Whether a path names a `.docx`, `.xlsx`, or `.pptx` file.
fn is_office_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "docx" || extension == "xlsx" || extension == "pptx")
}

/// Whether a part holds XML: a `.xml` part, or a relationships part, whose
/// name may be `.rels` alone.
fn is_xml_part(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".rels")
        || path
            .extension()
            .is_some_and(|extension| extension == "xml" || extension == "rels")
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = Arguments::parse();
    let system = RealSystem::new();
    let mut out = stdout().lock();
    let mut files: Vec<PathBuf> = system
        .read_dir(&arguments.folder)?
        .into_iter()
        .filter(|path| is_office_file(path))
        .collect();
    files.sort();
    let mut total = Counts::default();
    for path in &files {
        let label = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        writeln!(out, "{label}")?;
        let counts = check_file(&system, path, &mut out)?;
        counts.report(&mut out)?;
        total.add(&counts);
    }
    writeln!(out, "{} files", files.len())?;
    total.report(&mut out)
}

/// Write options with this layout and cleanup.
fn options(layout: Layout, namespace_cleanup: bool) -> WriteOptions {
    let mut options = WriteOptions::default();
    options.layout = layout;
    options.namespace_cleanup = namespace_cleanup;
    options
}

/// A file's bytes, through `system`.
fn read_bytes<S>(system: &S, path: &Path) -> Result<Vec<u8>, Box<dyn Error>>
where
    S: System + ?Sized,
{
    let mut bytes = Vec::new();
    system.open(path)?.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Print the prefixes the root declares after the cleanup, and its
/// `mc:Ignorable`.
fn root_after_cleanup(document: &Document, out: &mut dyn Write) -> Result<(), Box<dyn Error>> {
    let cleaned = parse(&document.to_xml(WriteOptions::default()))?;
    let mut prefixes: Vec<&str> = cleaned
        .root
        .namespace_declarations
        .iter()
        .map(|declaration| declaration.prefix.as_str())
        .collect();
    prefixes.sort_unstable();
    let ignorable = cleaned
        .root
        .attributes
        .iter()
        .find(|attribute| attribute.name.prefix == "mc" && attribute.name.local == "Ignorable")
        .map_or_else(
            || "no mc:Ignorable".to_owned(),
            |attribute| format!("mc:Ignorable=\"{}\"", attribute.value),
        );
    writeln!(
        out,
        "  word/document.xml after the cleanup: declares {}; {ignorable}",
        prefixes.join(" "),
    )?;
    Ok(())
}
