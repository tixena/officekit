//! A `.docx`, `.xlsx`, or `.pptx` stored in a part of another document.

use crate::docx::Docx;
use crate::pptx::Pptx;
use crate::xlsx::Xlsx;

/// The same file type a file on disk opens as, with its own package, class, and typed trees.
#[derive(Debug)]
#[non_exhaustive]
pub enum Embedded {
    Docx(Docx),
    Pptx(Pptx),
    Xlsx(Xlsx),
}

#[cfg(test)]
mod tests;
