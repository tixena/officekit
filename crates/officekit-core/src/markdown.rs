//! A Word document from an archie document and back, with the feature `markdown`.

mod from_word;
mod parts;
mod picture;
#[cfg(test)]
mod tests;
mod to_word;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use std::io;
use std::path::{Path, PathBuf};

use archie_core::parser::ast::{
    Document, FrontmatterFormat, InvalidFrontmatter, Mapping, MarkdownBlock, RawDocument,
};
use archie_core::parser::extension::{Extension, Markdown};
use archie_core::parser::{parse, read_toml};
use archie_core::writer::{self, WriteError, WriteOptions};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::Target;
use officekit_parts::kinds::WordprocessingKind;
use os_shim::System;

use crate::docx::Docx;

/// The relationship type that relates the frontmatter part, `/docProps/frontmatter.toml`, from the
/// package.
pub const FRONTMATTER_RELATIONSHIP: &str =
    "https://github.com/tixena/officekit/relationships/frontmatter";

const FRONTMATTER_CONTENT_TYPE: &str = "application/toml";

const FRONTMATTER_PART: &str = "/docProps/frontmatter.toml";

/// The line that opens and closes a TOML frontmatter.
const TOML_FENCE: &str = "+++\n";

/// A Word document converted to markdown: the archie document, and the pictures it names, each
/// as its file name and bytes.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Exported {
    pub document: Document<Markdown>,
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
    Image { target: String, error: String },
    /// The markdown file could not be read or written.
    #[error("{}: {error}", .path.display())]
    Io { error: io::Error, path: PathBuf },
    /// The Word package could not be read or written.
    #[error(transparent)]
    Package(#[from] PackageError),
    /// archie could not write the document as markdown.
    #[error(transparent)]
    Write(#[from] WriteError),
}

/// Store `mapping` as TOML in the part `/docProps/frontmatter.toml`, related from the package with
/// [`FRONTMATTER_RELATIONSHIP`]: the lines archie writes between the `+++` lines.
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

/// A new Word document holding `document`, for [`Docx::from_archie`].
///
/// # Errors
///
/// [`MarkdownError::Image`] when a picture cannot be read or is in no format it stores,
/// [`MarkdownError::Write`] when `options.frontmatter` is on and the frontmatter holds a null, and
/// [`MarkdownError::Package`] when the new package cannot be built or its parts stored.
pub(crate) fn from_archie<S, E>(
    system: &S,
    base: &Path,
    document: &Document<E>,
    options: MarkdownOptions,
) -> Result<Docx, MarkdownError>
where
    S: System + ?Sized,
    E: Extension,
{
    let mut docx = Docx::new(WordprocessingKind::Document)?;
    let main = docx.main_document_part().clone();
    parts::add_styles(docx.package_mut(), &main)?;
    let mut root = docx.main_document()?;
    root.body.get_or_insert_with(Box::default).content =
        to_word::blocks(system, base, &document.body, docx.package_mut(), &main)?;
    docx.set_main_document(&root)?;
    if options.frontmatter
        && let Some(mapping) = &document.frontmatter
    {
        add_frontmatter(docx.package_mut(), mapping)?;
    }
    Ok(docx)
}

/// The markdown file at `path`, read through `system`, as a new Word document.
///
/// A frontmatter that does not read is passed over, and its body converted, unless
/// `options.frontmatter` asks for the frontmatter.
///
/// # Errors
///
/// [`MarkdownError::Io`] when the file cannot be read, [`MarkdownError::Frontmatter`] when its
/// frontmatter does not read and `options.frontmatter` is on, and the errors of
/// [`Docx::from_archie`].
pub fn from_markdown<S>(
    system: &S,
    path: &Path,
    options: MarkdownOptions,
) -> Result<Docx, MarkdownError>
where
    S: System + ?Sized,
{
    let text = system
        .read_to_string(path)
        .map_err(|error| MarkdownError::Io {
            error,
            path: path.to_path_buf(),
        })?;
    let document: Document = match parse(&text).document {
        RawDocument::Valid(document) => document,
        RawDocument::Invalid(invalid) if options.frontmatter => {
            return Err(MarkdownError::Frontmatter(invalid.frontmatter));
        }
        RawDocument::Invalid(invalid) => Document::new(invalid.body, None),
        _ => Document::new(MarkdownBlock::new(Vec::new()), None),
    };
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    Docx::from_archie(system, base, &document, options)
}

/// `docx` written as markdown to `path`, its pictures in the folder beside it, or inside the
/// markdown when `options.embed_images` is set.
///
/// The pictures' folder is the markdown file's name plus `_media` (`contract.md` gives
/// `contract_media`), made only when there is a picture to write. No file is removed.
///
/// # Errors
///
/// The errors of [`Docx::to_archie`], [`MarkdownError::Write`] when archie cannot write the
/// document, and [`MarkdownError::Io`] when the markdown file, the folder, or a picture cannot be
/// written.
pub fn save_markdown<S>(
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
    let exported = docx.to_archie(&media, options)?;
    let text = writer::write(
        &RawDocument::Valid(exported.document),
        &WriteOptions::default(),
    )?;
    system
        .write(path, text.as_bytes())
        .map_err(|error| MarkdownError::Io {
            error,
            path: path.to_path_buf(),
        })?;
    if exported.images.is_empty() {
        return Ok(());
    }
    let folder = path.parent().unwrap_or_else(|| Path::new("")).join(&media);
    system
        .create_dir_all(&folder)
        .map_err(|error| MarkdownError::Io {
            error,
            path: folder.clone(),
        })?;
    for (name, bytes) in &exported.images {
        let target = folder.join(name);
        system
            .write(&target, bytes)
            .map_err(|error| MarkdownError::Io {
                error,
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

/// The main document of `docx`, its parts read with `read`, as an archie document, for
/// [`Docx::to_archie`]: each picture's target in the folder `media`, or a data URI with
/// `options.embed_images`, and the frontmatter part read with `options.frontmatter`.
///
/// # Errors
///
/// [`MarkdownError::Package`] when the main document, its styles or numbering part, a picture
/// or an HTML part cannot be read, and [`MarkdownError::Frontmatter`] when the frontmatter part
/// is to be read and does not read.
pub(crate) fn to_archie(
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
