//! [`OfficeFileMarkdown`]: an [`OfficeFile`] read from bytes that may be markdown text.

#[cfg(test)]
mod tests;

use std::io;
use std::path::{Path, PathBuf};

use officekit::office_file::OfficeFile;
use officekit_xml::packaging::package::OpenOptions;
use os_shim::System;

use crate::{MarkdownError, MarkdownOptions, from_markdown_text};

/// The name an error gives input that came without one.
const STANDARD_INPUT: &str = "standard input";

/// Bytes 0 to 3 of a zip archive.
const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";

/// The markdown input of an [`OfficeFile`]. Import the trait, `use
/// officekit_markdown::OfficeFileMarkdown as _;`, to call it on `OfficeFile`.
pub trait OfficeFileMarkdown: Sized {
    /// The model of `bytes`: a Word, Excel or `PowerPoint` file when they open with the zip
    /// signature `PK\x03\x04`, else markdown text, turned into a Word file. A relative picture
    /// path in the markdown is found from `folder`; with no folder it fails.
    ///
    /// ```rust
    /// use officekit::office_file::OfficeFile;
    /// use officekit_markdown::{MarkdownOptions, OfficeFileMarkdown as _};
    /// use officekit_xml::packaging::package::OpenOptions;
    /// use os_shim::mock::MemorySystem;
    ///
    /// # fn main() -> Result<(), Box<dyn core::error::Error>> {
    /// let file = OfficeFile::from_input(
    ///     &MemorySystem::new(),
    ///     b"one\ntwo\n".to_vec(),
    ///     None,
    ///     OpenOptions::default(),
    ///     MarkdownOptions::default(),
    /// )?;
    /// assert!(matches!(file, OfficeFile::Word(_)));
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// [`MarkdownError::Package`] for a zip that is no Office file, [`MarkdownError::Io`] naming
    /// standard input for bytes that are neither a zip nor UTF-8, and the errors of
    /// [`from_markdown_text`].
    fn from_input<S>(
        system: &S,
        bytes: Vec<u8>,
        folder: Option<&Path>,
        options: OpenOptions,
        markdown: MarkdownOptions,
    ) -> Result<Self, MarkdownError>
    where
        S: System + ?Sized;
}

impl OfficeFileMarkdown for OfficeFile {
    fn from_input<S>(
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
}
