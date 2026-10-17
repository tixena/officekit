//! Times the typed tree over every Office file in a folder. Each part a typed reader knows (the
//! main document, headers, and footers of a `.docx`, the workbook and worksheets of an `.xlsx`, the
//! presentation and slides of a `.pptx`) is parsed, read into the typed tree, transformed and
//! visited with no hooks, written through to bytes, and written back to the generic tree. Prints a
//! Markdown table of the median of 20 runs of each step, in microseconds, per file and in total,
//! with each file's SHA-1 so that a later run shows it timed the same files.
//!
//! ```text
//! cargo run --release -p officekit-core --example traversal_speed -- notes/samples
//! ```

use core::error::Error;
use core::fmt::{self, Write as _};
use core::hint::black_box;
use core::time::Duration;
use std::io::{Read as _, Write, stdout};
use std::path::{Path, PathBuf};
use std::time::Instant;

use clap::Parser;
use officekit_markup::write_through::{Unchanged, WriteThrough};
use officekit_markup::{Transform, Visit, presentation, spreadsheet, wordprocessing};
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_xml::element::Element;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{Layout, Scope};
use os_shim::System as _;
use os_shim::real::RealSystem;
use sha1::{Digest as _, Sha1};

/// The parts timed, by content type, each with its steps after the parse.
const KINDS: [(&str, Steps); 7] = [
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        |root, times| {
            time_steps(
                root,
                wordprocessing::read::document,
                wordprocessing::write::document,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        |root, times| {
            time_steps(
                root,
                wordprocessing::read::header,
                wordprocessing::write::header,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml",
        |root, times| {
            time_steps(
                root,
                wordprocessing::read::footer,
                wordprocessing::write::footer,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
        |root, times| {
            time_steps(
                root,
                spreadsheet::read::workbook,
                spreadsheet::write::workbook,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
        |root, times| {
            time_steps(
                root,
                spreadsheet::read::worksheet,
                spreadsheet::write::worksheet,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
        |root, times| {
            time_steps(
                root,
                presentation::read::presentation,
                presentation::write::presentation,
                times,
            )
        },
    ),
    (
        "application/vnd.openxmlformats-officedocument.presentationml.slide+xml",
        |root, times| {
            time_steps(
                root,
                presentation::read::presentation_slide_slide,
                presentation::write::presentation_slide_slide,
                times,
            )
        },
    ),
];

/// The runs timed per file, after one untimed run.
const RUNS: usize = 20;

/// A part kind's steps after the parse, run on the part's root and added to the times.
type Steps = fn(&Element, &mut Times) -> Result<(), Box<dyn Error>>;

/// Time the typed tree over a folder of Office files.
#[derive(Parser)]
struct Arguments {
    /// The folder whose `.docx`, `.xlsx`, and `.pptx` files are read.
    folder: PathBuf,
}

/// A visitor with no hooks: visiting through it walks every node.
struct Everything;

/// The time each step took, over every timed part of one file.
#[derive(Clone, Copy, Default)]
struct Times {
    parse: Duration,
    read: Duration,
    transform: Duration,
    visit: Duration,
    write: Duration,
    write_through: Duration,
}

impl officekit_markup::Visitor for Everything {}

impl Times {
    fn add(&mut self, other: &Self) {
        self.parse += other.parse;
        self.read += other.read;
        self.transform += other.transform;
        self.visit += other.visit;
        self.write_through += other.write_through;
        self.write += other.write;
    }

    /// The median of each step over `runs`.
    fn median(runs: &[Self]) -> Self {
        Self {
            parse: median(runs, |times| times.parse),
            read: median(runs, |times| times.read),
            transform: median(runs, |times| times.transform),
            visit: median(runs, |times| times.visit),
            write_through: median(runs, |times| times.write_through),
            write: median(runs, |times| times.write),
        }
    }
}

/// Bytes as lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    let mut text = String::new();
    for byte in bytes {
        let _: fmt::Result = write!(text, "{byte:02x}");
    }
    text
}

/// Whether a path names a `.docx`, `.xlsx`, or `.pptx` file.
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
    writeln!(
        out,
        "| File | SHA-1 | Parts | Parse | Read | Transform | Visit | Write-through | Write |"
    )?;
    writeln!(out, "|---|---|--:|--:|--:|--:|--:|--:|--:|")?;
    let mut total = Times::default();
    let mut total_parts = 0_usize;
    for path in &files {
        let mut bytes = Vec::new();
        system.open(path)?.read_to_end(&mut bytes)?;
        let hash = hex(&Sha1::digest(&bytes));
        let package = Package::from_bytes(bytes)?;
        let parts = typed_parts(&package)?;
        time_file(&package, &parts)?;
        let mut runs = Vec::with_capacity(RUNS);
        for _ in 0..RUNS {
            runs.push(time_file(&package, &parts)?);
        }
        let medians = Times::median(&runs);
        total.add(&medians);
        total_parts += parts.len();
        let label = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        row(&mut out, &label, &hash, parts.len(), &medians)?;
    }
    row(&mut out, "**Total**", "", total_parts, &total)?;
    Ok(())
}

/// The median of one step's times over `runs`.
fn median(runs: &[Times], step: fn(&Times) -> Duration) -> Duration {
    let mut times: Vec<Duration> = runs.iter().map(step).collect();
    times.sort_unstable();
    times.get(times.len() / 2).copied().unwrap_or_default()
}

/// One table row: a file's label, its hash, its timed parts, and its step times in microseconds.
fn row(
    out: &mut dyn Write,
    label: &str,
    hash: &str,
    parts: usize,
    times: &Times,
) -> Result<(), Box<dyn Error>> {
    writeln!(
        out,
        "| {label} | {hash} | {parts} | {} | {} | {} | {} | {} | {} |",
        times.parse.as_micros(),
        times.read.as_micros(),
        times.transform.as_micros(),
        times.visit.as_micros(),
        times.write_through.as_micros(),
        times.write.as_micros(),
    )?;
    Ok(())
}

/// One run over every timed part of a file.
fn time_file(package: &Package, parts: &[(PartName, Steps)]) -> Result<Times, Box<dyn Error>> {
    let mut times = Times::default();
    for (name, steps) in parts {
        let document = timed(&mut times.parse, || {
            package.load_tree(name, OpenOptions::default())
        })?;
        steps(&document.root, &mut times)?;
    }
    Ok(times)
}

/// Read a part's root into the typed tree, transform it, visit it, write it through to bytes, and
/// write it back to the generic tree, adding each step's time to `times`.
fn time_steps<T>(
    root: &Element,
    read: fn(&Element) -> T,
    write: fn(&T, &mut Scope) -> Element,
    times: &mut Times,
) -> Result<(), Box<dyn Error>>
where
    T: Transform + Visit + WriteThrough,
{
    let node = timed(&mut times.read, || read(root));
    let _: T = timed(&mut times.transform, || {
        node.transform(&mut Unchanged::default())
    });
    timed(&mut times.visit, || node.visit(&mut Everything));
    let mut bytes = Vec::new();
    timed(&mut times.write_through, || {
        node.write_through(
            &mut Unchanged::default(),
            &mut PartWriter::new(&mut bytes, Layout::Compact),
        )
    })?;
    let _: Element = timed(&mut times.write, || write(&node, &mut Scope::default()));
    Ok(())
}

/// Run `work`, adding the time it took to `time`.
fn timed<R, F>(time: &mut Duration, work: F) -> R
where
    F: FnOnce() -> R,
{
    let start = Instant::now();
    let result = black_box(work());
    *time += start.elapsed();
    result
}

/// The parts of `package` whose content type a typed reader knows, each with its steps.
fn typed_parts(package: &Package) -> Result<Vec<(PartName, Steps)>, Box<dyn Error>> {
    let mut parts = Vec::new();
    for name in package.part_names()? {
        let Some(content_type) = package.content_type(&name) else {
            continue;
        };
        if let Some((_, steps)) = KINDS.iter().find(|(kind, _)| *kind == content_type) {
            parts.push((name, *steps));
        }
    }
    Ok(parts)
}
