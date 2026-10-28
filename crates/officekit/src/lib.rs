//! `officekit` reads, edits, writes and validates `.docx`, `.xlsx` and `.pptx` files. It handles
//! both conformance classes of Office Open XML, Transitional and Strict.
//!
//! # The problem
//!
//! An Office file is a zip archive of XML parts. Relationships tie the parts together, and a
//! content type says what each part holds. Reading one by hand means unzipping it, following the
//! relationships, and knowing the schema of every part. Writing one means keeping the content
//! types and relationships in step with the parts. `officekit` does this work. It opens the
//! archive, gives each part as a typed tree, keeps the content it does not model in its place, and
//! writes the file back.
//!
//! Transitional is the class Word, Excel and `PowerPoint` write by default. Strict is the stricter
//! class of the same standard: it uses other namespace URIs and other spellings, and it has no
//! form for some legacy content. A file is in one class, and `officekit` converts between them.
//!
//! Use it from a Rust program that generates documents, edits many documents, converts documents
//! between Transitional and Strict, checks files before they reach Office, or moves text between
//! markdown and Word. No Office installation is needed.
//!
//! # Why use it
//!
//! - Content it does not model is kept in place, so a file survives a read and a write.
//! - Every part is a typed tree of structs and enums that follow the schema, so a misspelled
//!   field is a compile error.
//! - It reads and writes Transitional and Strict, and converts one to the other.
//! - It validates a file against a version of Office before Office sees it.
//!
//! A document is a tree. The text of a Word file is reached like this:
//!
//! ```text
//! Docx::main_document()          -> Document
//!   .body                        -> DocumentBody
//!     .content[i]                -> BlockContent::Paragraph(Paragraph)
//!       .content[j]              -> ParagraphContent::TextRun(TextRun)
//!         .content[k]            -> TextRunContent::Text(Text)
//!           .value               -> the text
//! ```
//!
//! The types are in `officekit_markup::wordprocessing`.
//!
//! # What it does
//!
//! | You want to | Use | Feature |
//! | --- | --- | --- |
//! | Open a file without knowing its type | [`office_file::OfficeFile`] | `transitional` or `strict`, for the class of the file |
//! | Create, open and edit a Word document | [`docx::Docx`] | `transitional` to create and save Transitional, `strict` to save Strict |
//! | Edit a table, header, footer, picture or chart in a Word document | [`docx::table::TableRows`], `Docx::add_header`, `Docx::set_picture`, `Docx::refresh_chart_caches` | none |
//! | Read and write workbook cells | [`xlsx::Xlsx`] with `officekit_parts::worksheet` | `transitional` to save Transitional, `strict` to save Strict |
//! | Read and write slides | [`pptx::Pptx`] with `officekit_parts::slide` | `transitional` to save Transitional, `strict` to save Strict |
//! | Validate against an Office version | [`Validator`](officekit_validation::validator::Validator) | none |
//! | Convert between Transitional and Strict | `to_bytes_strict` and `to_bytes_transitional` of `Docx`, `Xlsx` and `Pptx` | `strict` or `transitional` |
//! | Merge neighbouring runs that render the same | [`compact`] | none |
//! | Open a document stored inside another | [`embedded`] | none |
//! | Work on a `.docx` as a folder of files | [`folder`] | none |
//! | Turn markdown into Word and back | `markdown`, `Docx::from_arky`, `Docx::to_arky`, `OfficeFile::from_input` | `markdown` |
//! | Keep a parsed file in a form that opens without parsing XML | `binary` | `binary-format` |
//!
//! # Getting started
//!
//! Add `officekit` and `os-shim` to `Cargo.toml`. Every function that opens, creates or saves a
//! file takes an `os_shim::System`, so the same code runs against the real file system
//! (`os_shim::real::RealSystem`) and against an in-memory one (`os_shim::mock::MemorySystem`).
//!
//! ```toml
//! [dependencies]
//! officekit = "0.1"
//! os-shim = "0.1"
//! ```
//!
//! Types you build or pass come from the crate that owns them, and `officekit` re-exports none of
//! them. `OpenOptions`, `Package` and `PartName` are in `officekit-packaging`, the document kinds
//! are in `officekit-parts`, and the typed tree is in `officekit-markup`. Add the ones your code
//! names.
//!
//! ```toml
//! officekit-packaging = "0.1"
//! officekit-parts = "0.1"
//! officekit-markup = "0.1"
//! ```
//!
//! This creates a Word document, appends a paragraph that says `Hello`, saves it to bytes, opens
//! the bytes again and reads the text back.
//!
#![cfg_attr(
    feature = "transitional",
    doc = r#"
```rust
use officekit::docx::Docx;
use officekit_markup::wordprocessing::{
    BlockContent, Paragraph, ParagraphContent, Text, TextRun, TextRunContent,
};
use officekit_packaging::package::OpenOptions;
use officekit_parts::kinds::WordprocessingKind;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let mut docx = Docx::new(WordprocessingKind::Document)?;
let mut document = docx.main_document()?;

let mut text = Text::default();
text.value = "Hello".to_owned();
let mut run = TextRun::default();
run.content.push(TextRunContent::Text(text));
let mut paragraph = Paragraph::default();
paragraph.content.push(ParagraphContent::TextRun(run));
document
    .body
    .as_mut()
    .ok_or("the document has no body")?
    .content
    .push(BlockContent::Paragraph(paragraph));
docx.set_main_document(&document)?;

let (bytes, warnings) = docx.to_bytes_transitional()?;
assert!(warnings.is_empty());

let reopened = Docx::from_bytes(bytes, OpenOptions::default())?;
let document = reopened.main_document()?;
let body = document.body.as_ref().ok_or("the document has no body")?;
let mut found = Vec::new();
for block in &body.content {
    if let BlockContent::Paragraph(paragraph) = block {
        for item in &paragraph.content {
            if let ParagraphContent::TextRun(run) = item {
                for part in &run.content {
                    if let TextRunContent::Text(text) = part {
                        found.push(text.value.as_str());
                    }
                }
            }
        }
    }
}
assert!(found.contains(&"Hello"));
# Ok(())
# }
```
"#
)]
//!
//! Where to read next:
//!
//! - [`docx`], [`xlsx`] and [`pptx`]: the file types, with their saves and operations.
//! - [`officekit-markup`](https://docs.rs/officekit-markup): the typed tree, with a page per
//!   namespace.
//! - [`officekit-parts`](https://docs.rs/officekit-parts): how to load and store one part.
//! - [`officekit-validation`](https://docs.rs/officekit-validation): the validator.
//!
//! # Use cases
//!
//! | Who has the problem | What `officekit` does | Where |
//! | --- | --- | --- |
//! | A program that fills a letter or an invoice template | Replaces placeholder text in the runs of a document | Fill a template, below |
//! | A team that must archive in ISO 29500 Strict | Writes a file in the other class | Open a file and write it in the other class |
//! | A build that generates documents | Checks each file against a version of Office | Check a file against an Office version |
//! | A repository that tracks documents | Expands a `.docx` into a folder of parts | Work on a Word file as a folder |
//!
//! Each use case is the library code behind a command of the `officekit` command line, which the
//! crate `officekit-cli` installs. The commands are `cat`, `down`, `format`, `get`, `parts`,
//! `save`, `up` and `validate`.
//!
//! ## Fill a template
//!
//! Load the main document, change the text of the runs that hold a placeholder, and store it
//! again. A placeholder that Word split over two runs needs [`Docx::compact_runs`](docx::Docx::compact_runs)
//! first.
//!
#![cfg_attr(
    feature = "transitional",
    doc = r#"
```rust
use officekit::docx::Docx;
use officekit_markup::wordprocessing::{
    BlockContent, Paragraph, ParagraphContent, Text, TextRun, TextRunContent,
};
use officekit_parts::kinds::WordprocessingKind;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let mut docx = Docx::new(WordprocessingKind::Document)?;
let mut document = docx.main_document()?;

let mut text = Text::default();
text.value = "Dear {{name}},".to_owned();
let mut run = TextRun::default();
run.content.push(TextRunContent::Text(text));
let mut paragraph = Paragraph::default();
paragraph.content.push(ParagraphContent::TextRun(run));
document
    .body
    .as_mut()
    .ok_or("the document has no body")?
    .content
    .push(BlockContent::Paragraph(paragraph));

let body = document.body.as_mut().ok_or("the document has no body")?;
for block in &mut body.content {
    if let BlockContent::Paragraph(paragraph) = block {
        for item in &mut paragraph.content {
            if let ParagraphContent::TextRun(run) = item {
                for part in &mut run.content {
                    if let TextRunContent::Text(text) = part {
                        text.value = text.value.replace("{{name}}", "Ada");
                    }
                }
            }
        }
    }
}
docx.set_main_document(&document)?;

let stored = String::from_utf8(docx.package().part_bytes(docx.main_document_part())?)?;
assert!(stored.contains("Dear Ada,"));
# Ok(())
# }
```
"#
)]
//!
//! ## Open a file and write it in the other class
//!
//! `officekit get FILE -f docx` loads a file and writes it again. In the library,
//! [`OfficeFile::from_bytes`](office_file::OfficeFile::from_bytes) opens a file of any of the
//! three types, and the file type saves it in either class. A save in the class the file was read
//! in writes the package as it is. A save in the other class writes every part again in that
//! class.
//!
#![cfg_attr(
    all(feature = "transitional", feature = "strict"),
    doc = r#"
```rust
use officekit::docx::Docx;
use officekit::office_file::OfficeFile;
use officekit_packaging::package::OpenOptions;
use officekit_parts::kinds::WordprocessingKind;
use officekit_xml::conformance::ConformanceClass;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let bytes = Docx::new(WordprocessingKind::Document)?.package().to_bytes()?;
let file = OfficeFile::from_bytes(bytes, OpenOptions::default())?;
let OfficeFile::Word(docx) = file else {
    return Err("the file is not a Word file".into());
};
assert_eq!(docx.conformance_class(), ConformanceClass::Transitional);

let strict = Docx::from_bytes(docx.to_bytes_strict()?, OpenOptions::default())?;
assert_eq!(strict.conformance_class(), ConformanceClass::Strict);

let (bytes, warnings) = strict.to_bytes_transitional()?;
assert!(warnings.is_empty());
let back = Docx::from_bytes(bytes, OpenOptions::default())?;
assert_eq!(back.conformance_class(), ConformanceClass::Transitional);
# Ok(())
# }
```
"#
)]
//!
//! ## Keep a parsed file in the binary form
//!
//! `officekit get FILE -f binary` writes the binary form, and `officekit save -f docx` writes a
//! file from it. The binary form holds the typed tree of every part a module models, so reading it
//! parses no XML.
//!
#![cfg_attr(
    all(feature = "transitional", feature = "binary-format"),
    doc = "
```rust
use officekit::binary;
use officekit::docx::Docx;
use officekit::office_file::OfficeFile;
use officekit_parts::kinds::WordprocessingKind;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let file = OfficeFile::Word(Docx::new(WordprocessingKind::Document)?);
let archive = binary::encode(&file)?;
assert!(archive.starts_with(binary::MAGIC));

let restored = binary::decode(&archive)?;
assert!(matches!(restored, OfficeFile::Word(_)));
# Ok(())
# }
```
"
)]
//!
//! ## Check a file against an Office version
//!
//! `officekit validate FILE --target 365` lists what a version of Office would refuse in a file.
//! It exits with 0 when there are no errors, 1 when there are errors, and 2 when the check could
//! not run. In the library, a
//! [`Validator`](officekit_validation::validator::Validator) checks a package for one
//! [`OfficeVersion`](officekit_xml::name::OfficeVersion) and stops at a cap of errors.
//!
#![cfg_attr(
    feature = "markdown",
    doc = r#"
```rust
use officekit::markdown::{self, MarkdownOptions};
use officekit_validation::validator::Validator;
use officekit_xml::name::OfficeVersion;
use os_shim::mock::MemorySystem;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let system = MemorySystem::new();
let text = "A | B\n--|--\n1 | 2\n";
let docx = markdown::from_markdown_text(&system, text, None, MarkdownOptions::default())?;

let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(50);
let errors = validator.validate(docx.package());
assert!(errors.is_empty(), "{errors:?}");
# Ok(())
# }
```
"#
)]
//!
//! ## Work on a Word file as a folder
//!
//! `officekit down` expands every `.docx` under a folder into a folder of its parts, so a
//! version-control system can show a change to one part. `officekit format` indents the XML parts
//! of those folders where no whitespace lays them out yet, and `officekit up` zips them back.
//! Each folder holds an `info.json` with the name of the file it came from. The folder functions
//! work on `.docx` files only.
//!
#![cfg_attr(
    feature = "transitional",
    doc = r#"
```rust
use std::path::{Path, PathBuf};

use officekit::docx::Docx;
use officekit::folder::{self, DEFAULT_SUFFIX, DownOptions, UpOptions};
use officekit_packaging::package::OpenOptions;
use officekit_parts::kinds::WordprocessingKind;
use os_shim::System as _;
use os_shim::mock::MemorySystem;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let system = MemorySystem::new().with_dir("/in")?;
let source = Path::new("/in/Quarterly Report.docx");
Docx::create(&system, source, WordprocessingKind::Document)?;

let mut down = DownOptions::default();
down.suffix = DEFAULT_SUFFIX;
let folders = folder::down(&system, Path::new("/in"), Path::new("/parts"), down)?;
assert_eq!(folders, [PathBuf::from("/parts/quarterly_report_docx")]);

let info = system.read_to_string(Path::new("/parts/quarterly_report_docx/info.json"))?;
assert!(info.contains("Quarterly Report.docx"));

let summary = folder::format(&system, Path::new("/parts"))?;
assert!(summary.inspected > 0);

let mut up = UpOptions::default();
up.suffix = DEFAULT_SUFFIX;
let files = folder::up(&system, Path::new("/parts"), Path::new("/out"), up)?;
assert_eq!(files, [PathBuf::from("/out/Quarterly Report.docx")]);

let rebuilt = Docx::open(&system, &files[0], OpenOptions::default())?;
assert_eq!(rebuilt.kind(), WordprocessingKind::Document);
# Ok(())
# }
```
"#
)]
//!
//! ## Convert between markdown and Word
//!
//! `officekit get notes.md -f docx` turns markdown into a Word file, and
//! `officekit get report.docx -f markdown` turns a Word file into markdown. The conversion goes
//! through an arky document, carries pictures, and can carry a frontmatter block.
//!
#![cfg_attr(
    feature = "markdown",
    doc = r#"
```rust
use std::path::Path;

use officekit::markdown::{self, MarkdownOptions};
use os_shim::System as _;
use os_shim::mock::MemorySystem;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let system = MemorySystem::new().with_dir("/notes")?;
let options = MarkdownOptions::default();
let docx = markdown::from_markdown_text(&system, "A | B\n--|--\n1 | 2\n", None, options)?;

let path = Path::new("/notes/table.md");
markdown::save_markdown(&docx, &system, path, options)?;
let text = system.read_to_string(path)?;
assert_eq!(text, "| A   | B   |\n| --- | --- |\n| 1   | 2   |\n");
# Ok(())
# }
```
"#
)]
//!
//! ## Merge fragmented runs
//!
//! Word splits a sentence into several runs of text when it was edited in pieces. A run is a
//! stretch of text with one formatting. [`Docx::compact_runs`](docx::Docx::compact_runs) and
//! [`Pptx::compact_runs`](pptx::Pptx::compact_runs) merge neighbouring runs that render the same,
//! in every part of the file and in every Office document embedded in it.
//!
//! ```rust
//! use officekit::compact::Comments;
//! use officekit::docx::Docx;
//! use officekit_parts::kinds::WordprocessingKind;
//!
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let mut docx = Docx::new(WordprocessingKind::Document)?;
//! let main = docx.main_document_part().clone();
//! let xml = concat!(
//!     "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
//!     "<w:body><w:p><w:r><w:t>The </w:t></w:r><w:r><w:t>fox</w:t></w:r></w:p></w:body>",
//!     "</w:document>",
//! );
//! let content_type = WordprocessingKind::Document.main_content_type();
//! docx.package_mut().set_part_bytes(&main, content_type, xml.as_bytes())?;
//!
//! docx.compact_runs(Comments::Keep)?;
//!
//! let stored = String::from_utf8(docx.package().part_bytes(&main)?)?;
//! assert_eq!(stored.matches("<w:r>").count(), 1);
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! The default feature is `transitional`.
//!
//! - `transitional`: reads and writes the Transitional class. It turns on `Docx::create`,
//!   `Xlsx::create`, `Pptx::create`, `save_transitional`, `to_bytes_transitional` and
//!   `embedded::store_transitional`.
//! - `strict`: reads and writes the Strict class. It turns on `save_strict`, `to_bytes_strict`
//!   and `embedded::store_strict`.
//! - `markdown`: turns on `transitional` and the `markdown` module, `Docx::from_arky`,
//!   `Docx::to_arky` and `OfficeFile::from_input`. It adds the arky crate and the picture
//!   converters for SVG and `WebP`.
//! - `binary-format`: turns on the `binary` module and gives the typed tree an rkyv archive form.
//!
//! A build with neither `transitional` nor `strict` can make files in memory with `Docx::new`,
//! `Xlsx::new` and `Pptx::new`, but it cannot open a file: opening fails with
//! [`PackageError::ClassNotEnabled`](officekit_packaging::error::PackageError::ClassNotEnabled).
//!
//! # The crates behind it
//!
//! | Crate | Holds |
//! | --- | --- |
//! | [`officekit-packaging`](https://docs.rs/officekit-packaging) | The zip package: parts, part names, content types, relationships and images. |
//! | [`officekit-xml`](https://docs.rs/officekit-xml) | The generic element tree, names and namespaces, the conformance class, markup compatibility, and the part reader and writer. |
//! | [`officekit-parts`](https://docs.rs/officekit-parts) | One module per part type: its content type, its relationship type, and how to load and store its typed root. |
//! | [`officekit-markup`](https://docs.rs/officekit-markup) | The typed tree, one module per namespace. It gathers `officekit-markup-base`, `officekit-markup-core` and `officekit-markup-ext`. |
//! | [`officekit-transitional`](https://docs.rs/officekit-transitional) | The Transitional class: read and write of every node, and the conversion of date cells. |
//! | [`officekit-strict`](https://docs.rs/officekit-strict) | The Strict class: read and write of every node. |
//! | [`officekit-traversal`](https://docs.rs/officekit-traversal) | Transforms, visitors and write-through over the typed tree. |
//! | [`officekit-validation`](https://docs.rs/officekit-validation) | The validator and its schema tables. |
//! | `officekit-cli` | The `officekit` command line. It has no library. |
//!
//! # Limits
//!
//! - The file types are `.docx`, `.xlsx` and `.pptx` and their template and macro-enabled
//!   variants. Another zip package opens as an `officekit_packaging::package::Package` only.
//! - The folder functions and the markdown conversion work on Word files only.
//! - [`OfficeFile`](office_file::OfficeFile) saves in the Transitional class only. To save in
//!   Strict, match the variant and call the file type.
//! - A save in Strict fails with `PackageError::Write`, naming the first part that holds content
//!   Strict has no form for. A save of a workbook in Transitional turns each date cell into a
//!   number cell and returns one warning per converted cell. A date cell with no value loses its type
//!   and gets no warning. A date cell whose text is not a date stays, and the write then fails
//!   with `WriteError::NotInClass`.
//! - A file is read whole from a buffer in memory. The binary form is read as `officekit` writes
//!   it, at version 2 only.
//! - The typed tree keeps content it does not model, but it gives that content as raw elements,
//!   not as typed nodes.
//! - Most public structs and enums are `#[non_exhaustive]`. Build a value with `Default` or a
//!   constructor, assign its fields, and keep a wildcard arm in a `match`.
//! - `Docx::protect` writes a document-protection setting into the settings part. It does not
//!   encrypt the file. The caller gives the 16 bytes of salt.
//! - The validator reports what a version of Office would refuse in the parts reachable from the
//!   package relationships. It does not render a document or calculate a formula.
//! - The typed tree is large generated code, so a build that depends on it takes a while.

extern crate alloc;

#[cfg(feature = "binary-format")]
pub mod binary;
pub mod compact;
pub mod docx;
pub mod embedded;
mod file;
pub mod folder;
#[cfg(feature = "markdown")]
pub mod markdown;
mod new_parts;
pub mod office_file;
pub mod pptx;
pub mod xlsx;
