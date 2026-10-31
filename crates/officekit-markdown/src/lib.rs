//! `officekit-markdown` turns markdown into a Word file, and a Word file into markdown, for
//! `officekit`.
//!
//! [`from_markdown`] and [`from_markdown_text`] read markdown into a `Docx`, and [`save_markdown`]
//! writes a `Docx` as markdown with its pictures. Two traits give officekit's types the same
//! work as methods: [`DocxMarkdown`] gives a `Docx` `from_arky`, `to_arky` and `save_markdown`,
//! and [`OfficeFileMarkdown`] gives `OfficeFile` `from_input`, which reads bytes that are an
//! Office file or markdown text. A TOML frontmatter block travels inside the Word file as the
//! part `/docProps/frontmatter.toml` when [`MarkdownOptions::frontmatter`] is on.
//!
//! `officekit get notes.md -f docx` turns markdown into a Word file, and
//! `officekit get report.docx -f markdown` turns a Word file into markdown. The conversion goes
//! through an arky document and carries pictures.
//!
//! ```rust
//! use std::path::Path;
//!
//! use officekit_markdown::{DocxMarkdown as _, MarkdownOptions, from_markdown_text};
//! use officekit_xml::ready::ready_now;
//! use os_shim::System as _;
//! use os_shim::mock::MemorySystem;
//!
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let system = MemorySystem::new().with_dir("/notes")?;
//! let options = MarkdownOptions::default();
//! let docx = ready_now(from_markdown_text(&system, "A | B\n--|--\n1 | 2\n", None, options))?;
//!
//! let path = Path::new("/notes/table.md");
//! ready_now(docx.save_markdown(&system, path, options))?;
//! let text = ready_now(system.read_to_string(path))?;
//! assert_eq!(text, "| A   | B   |\n| --- | --- |\n| 1   | 2   |\n");
//! # Ok(())
//! # }
//! ```
//!
//! # Features
//!
//! - `binary-format`: turns on officekit's binary form and arky's, for a Word file made from
//!   markdown kept in officekit's binary form.

extern crate alloc;

mod docx;
mod from_word;
mod office_file;
mod parts;
mod picture;
#[cfg(test)]
mod tests;
mod to_word;

pub use docx::DocxMarkdown;
pub use office_file::OfficeFileMarkdown;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use std::io;
use std::path::{Path, PathBuf};

use arky::parser::ast::{
    Document, FrontmatterFormat, InvalidFrontmatter, Mapping, MarkdownBlock, RawDocument,
};
use arky::parser::extension::{Extension, Markdown};
use arky::parser::{parse, read_toml};
use arky::writer::{self, WriteError, WriteOptions};
use officekit::parts::kinds::WordprocessingKind;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::Target;
use os_shim::System;
use os_shim::bucket::Condition;

use officekit::docx::Docx;

/// The relationship type that relates the frontmatter part, `/docProps/frontmatter.toml`, from the
/// package.
pub const FRONTMATTER_RELATIONSHIP: &str =
    "https://github.com/tixena/officekit/relationships/frontmatter";

const FRONTMATTER_CONTENT_TYPE: &str = "application/toml";

const FRONTMATTER_PART: &str = "/docProps/frontmatter.toml";

/// The line that opens and closes a TOML frontmatter.
const TOML_FENCE: &str = "+++\n";

/// A Word document converted to markdown: the arky document, and the pictures it names, each
/// as its file name and bytes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Exported {
    /// The document as an arky markdown document.
    pub document: Document<Markdown>,
    /// The pictures the document names: the file name of each, and its bytes.
    pub images: Vec<(String, Vec<u8>)>,
}

/// What a conversion carries besides the document's content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MarkdownOptions {
    /// Write each picture into the markdown as a base64 data URI, not as a file beside it. Off by
    /// default.
    pub embed_images: bool,
    /// Carry the frontmatter, as a TOML part of the Word file. Off by default.
    pub frontmatter: bool,
}

/// Why a conversion failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MarkdownError {
    /// The frontmatter was to be carried and does not read.
    #[error("the frontmatter does not read: {}", .0.error)]
    Frontmatter(InvalidFrontmatter),
    /// A picture's file could not be read or is in a format officekit does not read.
    #[error("{target}: {error}")]
    Image {
        /// The picture's target, as the document names it.
        target: String,
        /// Why the picture failed.
        error: String,
    },
    /// The markdown file could not be read or written.
    #[error("{}: {error}", .path.display())]
    Io {
        /// The error of the file system.
        error: io::Error,
        /// The file or folder that failed.
        path: PathBuf,
    },
    /// The Word package could not be read or written.
    #[error(transparent)]
    Package(#[from] PackageError),
    /// arky could not write the document as markdown.
    #[error(transparent)]
    Write(#[from] WriteError),
}

impl From<io::Error> for MarkdownError {
    fn from(error: io::Error) -> Self {
        Self::Package(error.into())
    }
}

/// Store `mapping` as TOML in the part `/docProps/frontmatter.toml`, related from the package with
/// [`FRONTMATTER_RELATIONSHIP`]: the lines arky writes between the `+++` lines.
///
/// # Errors
///
/// [`MarkdownError::Write`] when the mapping holds a null, which TOML cannot hold, and
/// [`MarkdownError::Package`] when the part cannot be stored.
fn add_frontmatter(package: &mut Package, mapping: &Mapping) -> Result<(), MarkdownError> {
    let mut options = WriteOptions::default();
    options.frontmatter = FrontmatterFormat::Toml;
    let frontmatter_only: Document =
        Document::new(MarkdownBlock::new(Vec::new()), Some(mapping.clone()));
    let text = writer::write(&RawDocument::Valid(frontmatter_only), &options)?;
    let inside = text.strip_prefix(TOML_FENCE).unwrap_or(&text);
    let toml: String = inside
        .split_inclusive('\n')
        .take_while(|line| *line != TOML_FENCE)
        .collect();
    let name = PartName::new(FRONTMATTER_PART)?;
    package.set_part_bytes(&name, FRONTMATTER_CONTENT_TYPE, toml.as_bytes())?;
    package.relate(None, &name, FRONTMATTER_RELATIONSHIP)?;
    Ok(())
}

/// A new Word document holding `document`, for [`DocxMarkdown::from_arky`]; a relative picture path is
/// found from `base`.
///
/// # Errors
///
/// [`MarkdownError::Image`] when a picture is a relative path and there is no `base`, cannot be
/// read or is in no format it stores,
/// [`MarkdownError::Write`] when `options.frontmatter` is on and the frontmatter holds a null, and
/// [`MarkdownError::Package`] when the new package cannot be built or its parts stored.
pub(crate) async fn from_arky<S, E>(
    system: &S,
    base: Option<&Path>,
    document: &Document<E>,
    options: MarkdownOptions,
) -> Result<Docx, MarkdownError>
where
    S: System + ?Sized,
    E: Extension,
    Document<E>: Sync,
{
    let mut files = picture::files(system, base, to_word::targets(&document.body)).await;
    let mut docx = Docx::new(WordprocessingKind::Document)?;
    let main = docx.main_document_part().clone();
    parts::add_styles(docx.package_mut(), &main)?;
    let mut root = docx.main_document()?;
    root.body.get_or_insert_with(Box::default).content =
        to_word::blocks(&mut files, &document.body, docx.package_mut(), &main)?;
    docx.set_main_document(&root)?;
    if options.frontmatter
        && let Some(mapping) = &document.frontmatter
    {
        add_frontmatter(docx.package_mut(), mapping)?;
    }
    Ok(docx)
}

/// The markdown file at `path`, read through `system`, as a new Word document, a relative picture
/// path found from the file's folder.
///
/// # Errors
///
/// [`MarkdownError::Io`] when the file cannot be read, and the errors of [`from_markdown_text`].
pub async fn from_markdown<S>(
    system: &S,
    path: &Path,
    options: MarkdownOptions,
) -> Result<Docx, MarkdownError>
where
    S: System + ?Sized,
{
    let text = system
        .read_to_string(path)
        .await
        .map_err(|error| MarkdownError::Io {
            error: error.into(),
            path: path.to_path_buf(),
        })?;
    from_markdown_text(
        system,
        &text,
        Some(path.parent().unwrap_or_else(|| Path::new(""))),
        options,
    )
    .await
}

/// The Word file of the markdown `text`. A relative picture path is found from `folder`; with no
/// folder, it fails with [`MarkdownError::Image`] naming it.
///
/// ```rust
/// use officekit_markdown::{MarkdownOptions, from_markdown_text};
/// use officekit_xml::ready::ready_now;
/// use os_shim::mock::MemorySystem;
///
/// # fn main() -> Result<(), Box<dyn core::error::Error>> {
/// let system = MemorySystem::new();
/// let options = MarkdownOptions::default();
/// let docx = ready_now(from_markdown_text(&system, "one\ntwo\n", None, options))?;
/// let body = docx.main_document()?.body;
/// assert!(body.is_some_and(|body| !body.content.is_empty()));
/// # Ok(())
/// # }
/// ```
///
/// A frontmatter that does not read is passed over, and its body converted, unless
/// `options.frontmatter` asks for the frontmatter.
///
/// # Errors
///
/// [`MarkdownError::Frontmatter`] when the frontmatter does not read and `options.frontmatter` is
/// on, and the errors of [`DocxMarkdown::from_arky`].
pub async fn from_markdown_text<S>(
    system: &S,
    text: &str,
    folder: Option<&Path>,
    options: MarkdownOptions,
) -> Result<Docx, MarkdownError>
where
    S: System + ?Sized,
{
    let document: Document = match parse(text).document {
        RawDocument::Valid(document) => document,
        RawDocument::Invalid(invalid) if options.frontmatter => {
            return Err(MarkdownError::Frontmatter(invalid.frontmatter));
        }
        RawDocument::Invalid(invalid) => Document::new(invalid.body, None),
        _ => Document::new(MarkdownBlock::new(Vec::new()), None),
    };
    from_arky(system, folder, &document, options).await
}

/// `docx` written as markdown to `path`, its pictures in the folder beside it, or inside the
/// markdown when `options.embed_images` is set.
///
/// The pictures' folder is the markdown file's name plus `_media` (`contract.md` gives
/// `contract_media`), made only when there is a picture to write. No file is removed.
///
/// # Errors
///
/// The errors of [`DocxMarkdown::to_arky`], [`MarkdownError::Write`] when arky cannot write the
/// document, and [`MarkdownError::Io`] when the markdown file, the folder, or a picture cannot be
/// written.
pub async fn save_markdown<S>(
    docx: &Docx,
    system: &S,
    path: &Path,
    options: MarkdownOptions,
) -> Result<(), MarkdownError>
where
    S: System + ?Sized,
{
    let mut media = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    media.push_str("_media");
    let exported = to_arky(docx, docx.options(), &media, options)?;
    let text = writer::write(
        &RawDocument::Valid(exported.document),
        &WriteOptions::default(),
    )?;
    system
        .write(path, text.as_bytes(), Condition::None)
        .await
        .map_err(|error| MarkdownError::Io {
            error: error.into(),
            path: path.to_path_buf(),
        })?;
    if exported.images.is_empty() {
        return Ok(());
    }
    let folder = path.parent().unwrap_or_else(|| Path::new("")).join(&media);
    system
        .create_dir_all(&folder)
        .await
        .map_err(|error| MarkdownError::Io {
            error: error.into(),
            path: folder.clone(),
        })?;
    for (name, bytes) in &exported.images {
        let target = folder.join(name);
        system
            .write(&target, bytes, Condition::None)
            .await
            .map_err(|error| MarkdownError::Io {
                error: error.into(),
                path: target.clone(),
            })?;
    }
    Ok(())
}

/// The frontmatter the part `/docProps/frontmatter.toml` holds, the part related from the
/// package with [`FRONTMATTER_RELATIONSHIP`]; `None` when the package relates no such part.
///
/// # Errors
///
/// [`MarkdownError::Frontmatter`] when the part is not TOML text, and [`MarkdownError::Package`]
/// when it cannot be read.
fn read_frontmatter(package: &Package) -> Result<Option<Mapping>, MarkdownError> {
    let part = package.related(None)?.into_iter().find_map(|related| {
        if related.type_uri == FRONTMATTER_RELATIONSHIP
            && let Target::Part(name) = related.target
        {
            Some(name)
        } else {
            None
        }
    });
    let Some(name) = part else {
        return Ok(None);
    };
    let text = String::from_utf8(package.part_bytes(&name)?).map_err(|error| {
        let written = String::from_utf8_lossy(error.as_bytes()).into_owned();
        MarkdownError::Frontmatter(InvalidFrontmatter::new(
            FrontmatterFormat::Toml,
            written,
            error.to_string(),
        ))
    })?;
    match read_toml(&text, 0..text.len()) {
        Ok(mapping) => Ok(Some(mapping)),
        Err(error) => Err(MarkdownError::Frontmatter(InvalidFrontmatter::new(
            FrontmatterFormat::Toml,
            text,
            error.message,
        ))),
    }
}

/// The main document of `docx`, its parts read with `read`, as an arky document, for
/// [`DocxMarkdown::to_arky`]: each picture's target in the folder `media`, or a data URI with
/// `options.embed_images`, and the frontmatter part read with `options.frontmatter`.
///
/// # Errors
///
/// [`MarkdownError::Package`] when the main document, its styles or numbering part, a picture
/// or an HTML part cannot be read, and [`MarkdownError::Frontmatter`] when the frontmatter part
/// is to be read and does not read.
pub(crate) fn to_arky(
    docx: &Docx,
    read: OpenOptions,
    media: &str,
    options: MarkdownOptions,
) -> Result<Exported, MarkdownError> {
    let root = docx.main_document()?;
    let package = docx.package();
    let main = docx.main_document_part();
    let styles = from_word::Styles::read(package, main, read)?;
    let numbering = from_word::Numbering::read(package, main, read)?;
    let parts = from_word::Parts::read(package, main, media, options.embed_images)?;
    let frontmatter = if options.frontmatter {
        read_frontmatter(package)?
    } else {
        None
    };
    Ok(from_word::exported(
        &root,
        &styles,
        &numbering,
        parts,
        frontmatter,
    )?)
}
