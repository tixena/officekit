//! A Word, Excel or `PowerPoint` file opened from its bytes without knowing which.

#[cfg(test)]
mod tests;

use crate::parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use crate::transitional::dates::SaveWarning;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{MainPart, OpenOptions, Package};

use crate::docx::Docx;
use crate::file;
use crate::pptx::Pptx;
use crate::xlsx::Xlsx;

/// A Word, Excel or `PowerPoint` file, whichever its main part names.
///
/// Open one with [`from_bytes`](Self::from_bytes) when you do not know which type a file is. Match
/// the variant to reach the file type, and keep a wildcard arm: the enum is `#[non_exhaustive]`.
#[derive(Debug)]
#[non_exhaustive]
pub enum OfficeFile {
    /// A workbook, the `.xlsx` family.
    Excel(Xlsx),
    /// A presentation, the `.pptx` family.
    PowerPoint(Pptx),
    /// A Word document, the `.docx` family.
    Word(Docx),
}

impl OfficeFile {
    /// The file `bytes` hold, by its main part's content type: one of `WordprocessingKind::ALL`,
    /// `SpreadsheetKind::ALL` or `PresentationKind::ALL`.
    ///
    #[cfg_attr(
        feature = "transitional",
        doc = "
```rust
use officekit::docx::Docx;
use officekit::office_file::OfficeFile;
use officekit_xml::packaging::package::OpenOptions;
use officekit::parts::kinds::WordprocessingKind;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let bytes = Docx::new(WordprocessingKind::Document)?.package().to_bytes()?;
let file = OfficeFile::from_bytes(bytes, OpenOptions::default())?;
assert!(matches!(file, OfficeFile::Word(_)));
# Ok(())
# }
```
"
    )]
    ///
    /// # Errors
    ///
    /// The errors of [`Package::from_bytes`]. [`PackageError::ClassNotEnabled`] for a file whose
    /// class has its feature off. [`PackageError::ContentType`] for a main part that is none of a
    /// Word, Excel or `PowerPoint` main part. [`PackageError::MissingPart`] when the package
    /// relates no main part.
    pub fn from_bytes(bytes: Vec<u8>, options: OpenOptions) -> Result<Self, PackageError> {
        Self::from_package(Package::from_bytes(bytes)?, options)
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

    /// The options the file was opened with.
    #[must_use]
    pub const fn options(&self) -> OpenOptions {
        match self {
            Self::Excel(xlsx) => xlsx.options(),
            Self::PowerPoint(pptx) => pptx.options(),
            Self::Word(docx) => docx.options(),
        }
    }

    /// The file in the Transitional class, and the warnings of the save. Needs the feature
    /// `transitional`.
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
