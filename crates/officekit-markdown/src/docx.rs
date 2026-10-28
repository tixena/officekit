//! [`DocxMarkdown`]: the markdown methods of a [`Docx`].

use std::path::Path;

use arky::parser::ast::Document;
use arky::parser::extension::Extension;
use officekit::docx::Docx;
use os_shim::System;

use crate::{Exported, MarkdownError, MarkdownOptions};

/// The markdown methods of a [`Docx`]. Import the trait, `use officekit_markdown::DocxMarkdown as
/// _;`, to call them on a `Docx`.
///
/// ```rust
/// use std::path::Path;
///
/// use officekit::docx::Docx;
/// use officekit::parts::kinds::WordprocessingKind;
/// use officekit_markdown::{DocxMarkdown as _, MarkdownOptions};
/// use os_shim::System as _;
/// use os_shim::mock::MemorySystem;
///
/// # fn main() -> Result<(), Box<dyn core::error::Error>> {
/// let system = MemorySystem::new().with_dir("/notes")?;
/// let docx = Docx::new(WordprocessingKind::Document)?;
/// docx.save_markdown(&system, Path::new("/notes/empty.md"), MarkdownOptions::default())?;
/// assert!(system.is_file(Path::new("/notes/empty.md"))?);
/// # Ok(())
/// # }
/// ```
pub trait DocxMarkdown: Sized {
    /// A new Word document holding `document`. Local picture targets are read through `system`,
    /// relative to `base`.
    ///
    /// # Errors
    ///
    /// [`MarkdownError::Image`] when a picture cannot be read or is in no format it stores,
    /// [`MarkdownError::Write`] when `options.frontmatter` is on and the frontmatter holds a null,
    /// and [`MarkdownError::Package`] when the new package cannot be built or its parts stored.
    fn from_arky<S, E>(
        system: &S,
        base: &Path,
        document: &Document<E>,
        options: MarkdownOptions,
    ) -> Result<Self, MarkdownError>
    where
        S: System + ?Sized,
        E: Extension;

    /// The document written as markdown to `path`, as [`save_markdown`](crate::save_markdown)
    /// writes it.
    ///
    /// # Errors
    ///
    /// The errors of [`save_markdown`](crate::save_markdown).
    fn save_markdown<S>(
        &self,
        system: &S,
        path: &Path,
        options: MarkdownOptions,
    ) -> Result<(), MarkdownError>
    where
        S: System + ?Sized;

    /// The main document as an arky document. Each picture's target is `{media}/{file name}`,
    /// the file name that of its image part (`image1.png`).
    ///
    /// # Errors
    ///
    /// [`MarkdownError::Package`] when the main document, its styles or numbering part, a picture
    /// or an HTML part cannot be read, and [`MarkdownError::Frontmatter`] when
    /// `options.frontmatter` is on and the frontmatter part does not read.
    fn to_arky(&self, media: &str, options: MarkdownOptions) -> Result<Exported, MarkdownError>;
}

impl DocxMarkdown for Docx {
    fn from_arky<S, E>(
        system: &S,
        base: &Path,
        document: &Document<E>,
        options: MarkdownOptions,
    ) -> Result<Self, MarkdownError>
    where
        S: System + ?Sized,
        E: Extension,
    {
        crate::from_arky(system, Some(base), document, options)
    }

    fn save_markdown<S>(
        &self,
        system: &S,
        path: &Path,
        options: MarkdownOptions,
    ) -> Result<(), MarkdownError>
    where
        S: System + ?Sized,
    {
        crate::save_markdown(self, system, path, options)
    }

    fn to_arky(&self, media: &str, options: MarkdownOptions) -> Result<Exported, MarkdownError> {
        crate::to_arky(self, self.options(), media, options)
    }
}
