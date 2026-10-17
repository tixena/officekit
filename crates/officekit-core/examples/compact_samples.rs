//! Compacts the runs of every XML part of every Office file in a folder,
//! under both comment modes, and checks that every container renders the
//! same characters with the same counted run properties before and after.
//! Prints the counts, and exits non-zero when a part renders differently.
//!
//! The rendering is computed here, apart from the compaction: a run's text
//! with `xml:space` applied (trimmed unless preserved), a marker for every
//! other node, and its properties as written but for the ones the standard
//! says do not change rendering. Proofing and comment anchors render nothing.
//!
//! ```text
//! cargo run -p officekit-core --example compact_samples -- notes/samples
//! ```

use core::error::Error;
use core::fmt::{self, Write as _};
use std::io::{Cursor, Read as _, Write, stdout};
use std::path::{Path, PathBuf};

use clap::Parser;
use officekit_core::compact::{Comments, compact_tree};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Element, Node};
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::name::{Namespace, QName};
use officekit_xml::parse::parse;
use os_shim::System;
use os_shim::mock::MemorySystem;
use os_shim::real::RealSystem;

/// Check run compaction over a folder of Office files.
#[derive(Parser)]
struct Arguments {
    /// The folder whose `.docx`, `.xlsx`, and `.pptx` files are read.
    folder: PathBuf,
}

/// What one element renders, piece by piece.
#[derive(PartialEq, Eq)]
struct Container {
    name: String,
    pieces: Vec<Piece>,
}

/// What the check found under one comment mode.
#[derive(Default)]
struct Counts {
    /// Parts that render differently once compacted.
    differing: usize,
    merged: usize,
    removed_comments: usize,
}

/// A run kind.
#[derive(Clone, Copy)]
enum Kind {
    /// `a:r`.
    Drawing,
    /// `x:r`.
    Spreadsheet,
    /// `w:r`.
    Wordprocessing,
}

/// A rendered piece of a container, with the properties of the run holding it.
#[derive(PartialEq, Eq)]
enum Piece {
    /// A node that is not text.
    Marker { name: String, properties: String },
    /// The characters of neighbouring text with the same properties.
    Text { properties: String, text: String },
}

impl Kind {
    /// The unprefixed attributes of the properties element that do not change
    /// rendering: ECMA-376 Part 1, 21.1.2.3.9.
    const fn ignored_attributes(self) -> &'static [&'static str] {
        match self {
            Self::Drawing => &["altLang", "dirty", "err", "noProof", "smtClean"],
            Self::Spreadsheet | Self::Wordprocessing => &[],
        }
    }

    /// The children of the properties element that do not change rendering:
    /// ECMA-376 Part 1, 17.3.2.21 and 17.13.5.31.
    const fn ignored_children(self) -> &'static [&'static str] {
        match self {
            Self::Drawing | Self::Spreadsheet => &[],
            Self::Wordprocessing => &["noProof", "rPrChange"],
        }
    }

    /// The kind of `element` when it is a run.
    fn of(element: &Element) -> Option<Self> {
        if element.name.local != "r" {
            return None;
        }
        let prefix = table_prefix(&element.name.namespace)?;
        if prefix == "a" {
            Some(Self::Drawing)
        } else if prefix == "x" {
            Some(Self::Spreadsheet)
        } else if prefix == "w" {
            Some(Self::Wordprocessing)
        } else {
            None
        }
    }

    /// The namespace table's prefix of the kind's namespace.
    const fn prefix(self) -> &'static str {
        match self {
            Self::Drawing => "a",
            Self::Spreadsheet => "x",
            Self::Wordprocessing => "w",
        }
    }

    /// Whether `xml:space` decides the white space of the kind's text.
    const fn spaces(self) -> bool {
        match self {
            Self::Drawing => false,
            Self::Spreadsheet | Self::Wordprocessing => true,
        }
    }

    /// The elements whose content is text.
    const fn text(self) -> &'static [&'static str] {
        match self {
            Self::Drawing | Self::Spreadsheet => &["t"],
            Self::Wordprocessing => &["delText", "t"],
        }
    }
}

/// The properties of a run that change rendering, written so that equal ones
/// give the same string: attributes and children sorted by how they are
/// written, white space aside.
fn canonical(kind: Kind, run: &Element) -> String {
    let Some(element) = run
        .child_elements()
        .find(|child| is(child, kind.prefix(), "rPr"))
    else {
        return String::new();
    };
    let mut attributes: Vec<String> = element
        .attributes
        .iter()
        .filter(|attribute| {
            attribute.name.namespace != Namespace::None
                || !kind
                    .ignored_attributes()
                    .contains(&attribute.name.local.as_str())
        })
        .map(|attribute| {
            format!(
                "{{{}}}{}=\"{}\"",
                uri(&attribute.name.namespace),
                attribute.name.local,
                attribute.value
            )
        })
        .collect();
    attributes.sort();
    let mut children: Vec<String> = element
        .child_elements()
        .filter(|child| {
            !kind
                .ignored_children()
                .iter()
                .any(|local| is(child, kind.prefix(), local))
        })
        .map(written)
        .collect();
    children.sort();
    let others: Vec<String> = element
        .children
        .iter()
        .filter(|node| !matches!(node, Node::Element(_)) && !is_whitespace_text(node))
        .map(describe)
        .collect();
    format!(
        "{} | {} | {}",
        attributes.join(" "),
        children.concat(),
        others.concat()
    )
}

/// Check every XML part of one Office file under both comment modes; the
/// number of parts.
fn check_file<S>(
    system: &S,
    path: &Path,
    counts: &mut [(Comments, Counts); 2],
    out: &mut dyn Write,
) -> Result<usize, Box<dyn Error>>
where
    S: System + ?Sized,
{
    let package = MemorySystem::from_zip_reader(Cursor::new(read_bytes(system, path)?))?;
    let mut parts: Vec<PathBuf> = package
        .walk_dir(Path::new("/"), false, true)?
        .into_iter()
        .filter(|entry| entry.is_file)
        .map(|entry| entry.path)
        .filter(|entry| is_xml_part(entry))
        .collect();
    parts.sort();
    for part in &parts {
        let label = format!("{} {}", path.display(), part.display());
        let document = parse(&read_bytes(&package, part)?)
            .map_err(|error| format!("{label} does not parse: {error}"))?;
        let before = rendering(&document.root);
        for (comments, found) in counts.iter_mut() {
            let mut root = document.root.clone();
            let done = compact_tree(&mut root, *comments);
            found.merged += done.merged;
            found.removed_comments += done.removed_comments.len();
            let after = rendering(&root);
            if after != before {
                found.differing += 1;
                let first = before
                    .iter()
                    .zip(&after)
                    .position(|(one, other)| one != other)
                    .unwrap_or_else(|| before.len().min(after.len()));
                writeln!(
                    out,
                    "  {label} under {}: renders differently\n    before: {}\n    after:  {}",
                    mode(*comments),
                    show(before.get(first)),
                    show(after.get(first)),
                )?;
            }
        }
    }
    Ok(parts.len())
}

/// A node as written, for a marker or a comparison.
fn describe(node: &Node) -> String {
    if let Node::Element(element) = node {
        return written(element);
    }
    if let Node::Text(text) = node {
        return text.clone();
    }
    if let Node::CData(text) = node {
        return format!("<![CDATA[{text}]]>");
    }
    if let Node::Comment(text) = node {
        return format!("<!--{text}-->");
    }
    if let Node::ProcessingInstruction { content, target } = node {
        return format!("<?{target} {content}?>");
    }
    "<?>".to_owned()
}

/// Whether `element` is named `local` in the namespace the namespace table
/// names `prefix`.
fn is(element: &Element, prefix: &str, local: &str) -> bool {
    element.name.local == local && table_prefix(&element.name.namespace) == Some(prefix)
}

/// Whether a path names a `.docx`, `.xlsx`, or `.pptx` file.
fn is_office_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "docx" || extension == "xlsx" || extension == "pptx")
}

fn is_text(element: &Element, kind: Kind) -> bool {
    kind.text()
        .iter()
        .any(|local| is(element, kind.prefix(), local))
}

fn is_whitespace_text(node: &Node) -> bool {
    if let Node::Text(text) = node {
        text.chars()
            .all(|character| matches!(character, ' ' | '\t' | '\n' | '\r'))
    } else {
        false
    }
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
    let mut counts = [
        (Comments::Keep, Counts::default()),
        (Comments::Remove, Counts::default()),
    ];
    let mut parts = 0_usize;
    for path in &files {
        parts += check_file(&system, path, &mut counts, &mut out)?;
    }
    writeln!(out, "{} files, {parts} XML parts", files.len())?;
    let mut differing = 0_usize;
    for (comments, found) in &counts {
        writeln!(
            out,
            "  {}: runs merged {}, comments removed {}, parts that render differently {}",
            mode(*comments),
            found.merged,
            found.removed_comments,
            found.differing,
        )?;
        differing += found.differing;
    }
    if differing > 0 {
        return Err(format!("{differing} compacted parts render differently").into());
    }
    Ok(())
}

fn mode(comments: Comments) -> &'static str {
    if comments == Comments::Keep {
        "Keep"
    } else {
        "Remove"
    }
}

/// Whether `xml:space="preserve"` is in force inside `element`, `around`
/// telling whether it is around it.
fn preserves_space(element: &Element, around: bool) -> bool {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.prefix == "xml" && attribute.name.local == "space")
        .map_or(around, |attribute| attribute.value == "preserve")
}

/// Add rendered text to a container's pieces, joined to the piece before
/// when that is text with the same properties.
fn push_text(pieces: &mut Vec<Piece>, text: &str, properties: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Piece::Text {
        properties: last_properties,
        text: last_text,
    }) = pieces.last_mut()
        && last_properties == properties
    {
        last_text.push_str(text);
        return;
    }
    pieces.push(Piece::Text {
        properties: properties.to_owned(),
        text: text.to_owned(),
    });
}

/// A name as written: `prefix:local`, or `local` with no prefix.
fn qualified(name: &QName) -> String {
    if name.prefix.is_empty() {
        name.local.clone()
    } else {
        format!("{}:{}", name.prefix, name.local)
    }
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

/// Record what `element` renders and what each element under it renders;
/// `around` is whether `xml:space="preserve"` is in force around it. A run's
/// text belongs to the element holding the run.
fn render(element: &Element, around: bool, containers: &mut Vec<Container>) {
    let preserve = preserves_space(element, around);
    let mut pieces = Vec::new();
    for node in &element.children {
        if let Node::Element(child) = node {
            if let Some(kind) = Kind::of(child) {
                render_run(child, kind, preserve, &mut pieces);
            } else if !is(child, "w", "proofErr")
                && !is(child, "w", "commentRangeStart")
                && !is(child, "w", "commentRangeEnd")
            {
                pieces.push(Piece::Marker {
                    name: qualified(&child.name),
                    properties: String::new(),
                });
            } else {
                // A proofing or comment anchor renders nothing.
            }
        } else if let Node::Text(text) = node {
            if !is_whitespace_text(node) {
                push_text(&mut pieces, text, "");
            }
        } else {
            pieces.push(Piece::Marker {
                name: describe(node),
                properties: String::new(),
            });
        }
    }
    if !pieces.is_empty() {
        containers.push(Container {
            name: qualified(&element.name),
            pieces,
        });
    }
    for child in element.child_elements() {
        if let Some(kind) = Kind::of(child) {
            let run_preserve = preserves_space(child, preserve);
            for content in child.child_elements() {
                if !is(content, kind.prefix(), "rPr") && !is_text(content, kind) {
                    render(content, run_preserve, containers);
                }
            }
        } else {
            render(child, preserve, containers);
        }
    }
}

/// Add what a run renders to its container's pieces.
fn render_run(run: &Element, kind: Kind, around: bool, pieces: &mut Vec<Piece>) {
    let preserve = preserves_space(run, around);
    let properties = canonical(kind, run);
    for node in &run.children {
        if let Node::Element(content) = node {
            if is(content, kind.prefix(), "rPr") || is(content, "w", "commentReference") {
                continue;
            }
            if is_text(content, kind) {
                let mut text = String::new();
                for piece in &content.children {
                    if let Node::Text(characters) | Node::CData(characters) = piece {
                        text.push_str(characters);
                    }
                }
                let rendered = if kind.spaces() && !preserves_space(content, preserve) {
                    text.trim_matches(|character| matches!(character, ' ' | '\t' | '\n' | '\r'))
                } else {
                    text.as_str()
                };
                push_text(pieces, rendered, &properties);
                continue;
            }
            pieces.push(Piece::Marker {
                name: qualified(&content.name),
                properties: properties.clone(),
            });
        } else if !is_whitespace_text(node) {
            pieces.push(Piece::Marker {
                name: describe(node),
                properties: properties.clone(),
            });
        } else {
            // White space between a run's elements renders nothing.
        }
    }
}

/// Every element of `root` that renders something, in document order.
fn rendering(root: &Element) -> Vec<Container> {
    let mut containers = Vec::new();
    render(root, false, &mut containers);
    containers
}

/// A container and its pieces, for the report of a difference.
fn show(container: Option<&Container>) -> String {
    let Some(shown) = container else {
        return "nothing".to_owned();
    };
    let mut text = format!("{}:", shown.name);
    for piece in &shown.pieces {
        let _: fmt::Result = match piece {
            Piece::Marker { name, properties } => write!(text, " {name} [{properties}]"),
            Piece::Text {
                properties,
                text: characters,
            } => write!(text, " \"{characters}\" [{properties}]"),
        };
    }
    text
}

/// The prefix the namespace table gives `namespace`, when it knows it.
fn table_prefix(namespace: &Namespace) -> Option<&'static str> {
    if let Namespace::Known(id) = *namespace {
        NAMESPACES.get(usize::from(id)).map(|row| row.prefix)
    } else {
        None
    }
}

/// A namespace's URI, the Transitional one for a namespace the table knows.
fn uri(namespace: &Namespace) -> &str {
    namespace.uri(ConformanceClass::Transitional)
}

/// An element as written, its attributes in order, white space between its
/// children aside.
fn written(element: &Element) -> String {
    let mut attributes: Vec<String> = element
        .attributes
        .iter()
        .map(|attribute| {
            format!(
                "{{{}}}{}=\"{}\"",
                uri(&attribute.name.namespace),
                attribute.name.local,
                attribute.value
            )
        })
        .collect();
    attributes.sort();
    let children: Vec<String> = element
        .children
        .iter()
        .filter(|node| !is_whitespace_text(node))
        .map(describe)
        .collect();
    format!(
        "<{{{}}}{} {}>{}</>",
        uri(&element.name.namespace),
        element.name.local,
        attributes.join(" "),
        children.concat()
    )
}
