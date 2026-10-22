//! A Word, Excel or `PowerPoint` file opened from its bytes without knowing which, and, with the
//! feature `markdown`, markdown text read as a Word file.

#[cfg(test)]
mod tests;

#[cfg(feature = "markdown")]
use std::io;
#[cfg(feature = "markdown")]
use std::path::{Path, PathBuf};

use officekit_packaging::error::PackageError;
use officekit_packaging::package::{MainPart, OpenOptions, Package};
use officekit_parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use officekit_transitional::dates::SaveWarning;
#[cfg(feature = "markdown")]
use os_shim::System;

use crate::docx::Docx;
use crate::file;
#[cfg(feature = "markdown")]
use crate::markdown::{MarkdownError, MarkdownOptions, from_markdown_text};
use crate::pptx::Pptx;
use crate::xlsx::Xlsx;

/// The name an error gives input that came without one.
#[cfg(feature = "markdown")]
const STANDARD_INPUT: &str = "standard input";

/// Bytes 0 to 3 of a zip archive.
#[cfg(feature = "markdown")]
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// A Word, Excel or `PowerPoint` file, whichever its main part names.
#[derive(Debug)]
#[non_exhaustive]
pub enum OfficeFile {
    Excel(Xlsx),
    PowerPoint(Pptx),
    Word(Docx),
}

impl OfficeFile {
    /// The file `bytes` hold, by its main part's content type: one of `WordprocessingKind::ALL`,
    /// `SpreadsheetKind::ALL` or `PresentationKind::ALL`.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::from_bytes`] and of [`from_package`](Self::from_package).
    pub fn from_bytes(bytes: Vec<u8>, options: OpenOptions) -> Result<Self, PackageError> {
        Self::from_package(Package::from_bytes(bytes)?, options)
    }

    /// The model of `bytes`: a Word, Excel or `PowerPoint` file when they open with the zip
    /// signature `PK\x03\x04`, else markdown text, turned into a Word file. A relative picture
    /// path in the markdown is found from `folder`; with no folder it fails.
    ///
    /// # Errors
    ///
    /// [`MarkdownError::Package`] for a zip that is no Office file, [`MarkdownError::Io`] naming
    /// standard input for bytes that are neither a zip nor UTF-8, and the errors of
    /// [`from_markdown_text`].
    #[cfg(feature = "markdown")]
    pub fn from_input<S>(
        system: &S,
        bytes: Vec<u8>,
        folder: Option<&Path>,
        options: OpenOptions,
        markdown: MarkdownOptions,
    ) -> Result<Self, MarkdownError>
    where
        S: System + ?Sized,
    {
        if bytes.starts_with(ZIP_SIGNATURE) {
            return Ok(Self::from_bytes(bytes, options)?);
        }
        let text = String::from_utf8(bytes).map_err(|error| MarkdownError::Io {
            error: io::Error::new(io::ErrorKind::InvalidData, error),
            path: PathBuf::from(STANDARD_INPUT),
        })?;
        from_markdown_text(system, &text, folder, markdown).map(Self::Word)
    }

    /// The file `package` holds, by its main part's content type, as
    /// [`from_bytes`](Self::from_bytes) finds it.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::main_part`]; [`PackageError::ClassNotEnabled`] for a class whose
    /// feature is off; and [`PackageError::ContentType`] for a main part of none of the three.
    pub(crate) fn from_package(
        package: Package,
        options: OpenOptions,
    ) -> Result<Self, PackageError> {
        let main = file::main_part(&package)?;
        if lists(
            &main,
            &WordprocessingKind::ALL,
            WordprocessingKind::main_content_type,
        ) {
            Docx::from_package(package, options).map(Self::Word)
        } else if lists(
            &main,
            &SpreadsheetKind::ALL,
            SpreadsheetKind::main_content_type,
        ) {
            Xlsx::from_package(package, options).map(Self::Excel)
        } else if lists(
            &main,
            &PresentationKind::ALL,
            PresentationKind::main_content_type,
        ) {
            Pptx::from_package(package, options).map(Self::PowerPoint)
        } else {
            Err(PackageError::ContentType {
                expected: "a WordprocessingML, SpreadsheetML or PresentationML main part",
                found: main.content_type,
                part: main.name,
            })
        }
    }

    /// The file in the Transitional class, and the warnings of the save.
    ///
    /// # Errors
    ///
    /// The errors of [`Docx::to_bytes_transitional`], [`Xlsx::to_bytes_transitional`] and
    /// [`Pptx::to_bytes_transitional`].
    #[cfg(feature = "transitional")]
    pub fn to_bytes_transitional(&self) -> Result<(Vec<u8>, Vec<SaveWarning>), PackageError> {
        match self {
            Self::Word(docx) => docx.to_bytes_transitional(),
            Self::Excel(xlsx) => xlsx.to_bytes_transitional(),
            Self::PowerPoint(pptx) => pptx.to_bytes_transitional(),
        }
    }
}

/// Whether one of `kinds` has the main part's content type.
fn lists<K>(main: &MainPart, kinds: &[K], content_type: fn(K) -> &'static str) -> bool
where
    K: Copy,
{
    kinds
        .iter()
        .any(|kind| Some(content_type(*kind)) == main.content_type.as_deref())
}
