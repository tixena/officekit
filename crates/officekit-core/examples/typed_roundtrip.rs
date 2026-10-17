//! Reads the main document, header, and footer parts of every `.docx` file in
//! a folder, and the workbook, worksheet, presentation, and slide parts of
//! every `.xlsx` and `.pptx` file, into the typed tree with markup
//! compatibility kept; writes each back and compares it with the part's own
//! tree, attributes and namespace declarations as sets, both as the compact
//! layout writes them.
//!
//! ```text
//! cargo run -p officekit-core --example typed_roundtrip -- notes/samples
//! ```

use core::error::Error;
use std::io::{Write, stdout};
use std::path::{Path, PathBuf};

use clap::Parser;
use officekit_core::markup::{presentation, spreadsheet, wordprocessing};
use officekit_core::packaging::package::{OpenOptions, Package};
use officekit_xml::element::{Document, Element, Node};
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, Scope, WriteOptions};
use os_shim::System as _;
use os_shim::real::RealSystem;

/// The parts read, by content type, each with its typed round trip.
const KINDS: [(&str, RoundTrip); 7] = [
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        |root| {
            let node = wordprocessing::read::document(root);
            wordprocessing::write::document(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        |root| {
            let node = wordprocessing::read::header(root);
            wordprocessing::write::header(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        |root| {
            let node = wordprocessing::read::footer(root);
            wordprocessing::write::footer(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        |root| {
            let node = spreadsheet::read::workbook(root);
            spreadsheet::write::workbook(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        |root| {
            let node = spreadsheet::read::worksheet(root);
            spreadsheet::write::worksheet(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        |root| {
            let node = presentation::read::presentation(root);
            presentation::write::presentation(&node, &mut Scope::default())
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        |root| {
            let node = presentation::read::presentation_slide_slide(root);
            presentation::write::presentation_slide_slide(&node, &mut Scope::default())
        },
    ),
];

/// A part's root read into the typed tree and written back.
type RoundTrip = fn(&Element) -> Element;

/// Round-trip the typed tree over a folder of Office files.
#[derive(Parser)]
struct Arguments {
    /// The folder whose `.docx`, `.xlsx`, and `.pptx` files are read.
    folder: PathBuf,
}

/// The first difference between two elements, at `path`; `None` when they are
/// equal with attributes and namespace declarations compared as sets.
fn difference(left: &Element, right: &Element, path: &str) -> Option<String> {
    let here = format!("{path}/{}:{}", left.name.prefix, left.name.local);
    if left.name != right.name {
        return Some(format!(
            "{here}: written as {}:{}",
            right.name.prefix, right.name.local
        ));
    }
    if !same_set(&left.attributes, &right.attributes) {
        return Some(format!(
            "{here}: attributes {:?} written as {:?}",
            left.attributes, right.attributes
        ));
    }
    if !same_set(&left.namespace_declarations, &right.namespace_declarations) {
        return Some(format!(
            "{here}: declarations {:?} written as {:?}",
            left.namespace_declarations, right.namespace_declarations
        ));
    }
    if left.children.len() != right.children.len() {
        return Some(format!(
            "{here}: children {} written as {}",
            child_names(left),
            child_names(right)
        ));
    }
    for (index, (read, written)) in left.children.iter().zip(&right.children).enumerate() {
        let found = match (read, written) {
            (Node::Element(read_element), Node::Element(written_element)) => {
                difference(read_element, written_element, &here)
            }
            _ if read == written => None,
            _ => Some(format!(
                "{here}: child {index} {read:?} written as {written:?}"
            )),
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// The tags of an element's children, `#text` for any other node.
fn child_names(element: &Element) -> String {
    element
        .children
        .iter()
        .map(|child| {
            if let Node::Element(inner) = child {
                format!("{}:{}", inner.name.prefix, inner.name.local)
            } else {
                "#text".to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether a part's typed round trip gives its own tree back; the first
/// difference when it does not.
fn check_file(path: &Path, out: &mut dyn Write) -> Result<(usize, usize), Box<dyn Error>> {
    let system = RealSystem::new();
    let package = Package::open(&system, path)?;
    let mut checked = 0;
    let mut equal = 0;
    for name in package.part_names()? {
        let Some(content_type) = package.content_type(&name) else {
            continue;
        };
        let Some((_, round_trip)) = KINDS.iter().find(|(kind, _)| *kind == content_type) else {
            continue;
        };
        let document = package.load_tree(&name, OpenOptions::default())?;
        let mut typed = document.clone();
        typed.root = round_trip(&document.root);
        let own = compact(&document)?;
        let written = compact(&typed)?;
        checked += 1;
        match difference(&own, &written, "") {
            None => equal += 1,
            Some(found) => writeln!(out, "  {}: {found}", name.as_str())?,
        }
    }
    Ok((checked, equal))
}

/// A part's root as written compact without the cleanup and read again:
/// whitespace between elements dropped as both layouts drop it.
fn compact(document: &Document) -> Result<Element, Box<dyn Error>> {
    let mut options = WriteOptions::default();
    options.layout = Layout::Compact;
    options.namespace_cleanup = false;
    Ok(parse(&document.to_xml(options))?.root)
}

/// Whether a part names a `.docx`, `.xlsx`, or `.pptx` file.
fn is_office_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "docx" || extension == "xlsx" || extension == "pptx")
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
    let mut checked = 0;
    let mut equal = 0;
    for path in &files {
        let (file_checked, file_equal) = check_file(path, &mut out)?;
        let label = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        writeln!(out, "{label}: {file_equal} of {file_checked} parts equal")?;
        checked += file_checked;
        equal += file_equal;
    }
    writeln!(
        out,
        "{} files, {equal} of {checked} parts equal",
        files.len()
    )?;
    if equal == checked {
        Ok(())
    } else {
        Err(format!("{} parts differ", checked - equal).into())
    }
}

/// Whether two lists hold the same items, in any order.
fn same_set<T>(left: &[T], right: &[T]) -> bool
where
    T: PartialEq,
{
    left.len() == right.len()
        && left.iter().all(|item| right.contains(item))
        && right.iter().all(|item| left.contains(item))
}
