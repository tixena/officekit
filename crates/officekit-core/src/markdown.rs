//! A Word document from an archie document and back, with the feature `markdown`.

mod parts;
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
use archie_core::parser::extension::Extension;
use archie_core::parser::parse;
use archie_core::writer::{self, WriteError, WriteOptions};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
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
/// [`MarkdownError::Write`] when `options.frontmatter` is on and the frontmatter holds a null, and
/// [`MarkdownError::Package`] when the new package cannot be built or its parts stored.
pub(crate) fn from_archie<S, E>(
    _system: &S,
    _base: &Path,
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
        to_word::blocks(&document.body, docx.package_mut(), &main)?;
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
