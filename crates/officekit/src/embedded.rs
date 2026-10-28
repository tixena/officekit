//! A `.docx`, `.xlsx`, or `.pptx` stored in a part of another document.

use crate::parts::kinds::{PresentationKind, SpreadsheetKind, WordprocessingKind};
#[cfg(feature = "transitional")]
use crate::transitional::dates::SaveWarning;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;

use crate::docx::Docx;
use crate::file;
use crate::pptx::Pptx;
use crate::xlsx::Xlsx;

/// The same file type a file on disk opens as, with its own package, class, and typed trees.
#[derive(Debug)]
#[non_exhaustive]
pub enum Embedded {
    /// An embedded Word document.
    Docx(Docx),
    /// An embedded presentation.
    Pptx(Pptx),
    /// An embedded workbook, such as the data behind a chart.
    Xlsx(Xlsx),
}

/// Open the document a part of `package` holds, by its main part's content type.
///
#[cfg_attr(
    feature = "transitional",
    doc = r#"
```rust
use officekit::docx::Docx;
use officekit::embedded::{self, Embedded};
use officekit::xlsx::Xlsx;
use officekit_xml::packaging::package::OpenOptions;
use officekit_xml::packaging::part_name::PartName;
use officekit::parts::kinds::{SpreadsheetKind, WordprocessingKind};

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let workbook = Xlsx::new(SpreadsheetKind::Workbook)?;
let mut docx = Docx::new(WordprocessingKind::Document)?;

let name = PartName::new("/word/embeddings/budget.xlsx")?;
let content_type = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
docx.package_mut()
    .set_part_bytes(&name, content_type, &workbook.package().to_bytes()?)?;

let found = embedded::open(docx.package(), &name, OpenOptions::default())?;
assert!(matches!(found, Embedded::Xlsx(_)));
# Ok(())
# }
```
"#
)]
///
/// # Errors
///
/// [`PackageError::NotAPackage`] for bytes that are not a `.docx`, `.xlsx`, or `.pptx`, an OLE
/// object for one; [`PackageError::MissingPart`] when the package has no such part; the errors
/// of the file type's `from_package`.
pub fn open(
    package: &Package,
    name: &PartName,
    options: OpenOptions,
) -> Result<Embedded, PackageError> {
    let not_a_package = |reason: String| PackageError::NotAPackage {
        part: name.clone(),
        reason,
    };
    let stored = Package::from_bytes(package.part_bytes(name)?)
        .map_err(|error| not_a_package(error.to_string()))?;
    let main = file::main_part(&stored).map_err(|error| not_a_package(error.to_string()))?;
    let content_type = main.content_type.as_deref();
    if WordprocessingKind::ALL
        .iter()
        .any(|kind| Some(kind.main_content_type()) == content_type)
    {
        Docx::from_package(stored, options).map(Embedded::Docx)
    } else if SpreadsheetKind::ALL
        .iter()
        .any(|kind| Some(kind.main_content_type()) == content_type)
    {
        Xlsx::from_package(stored, options).map(Embedded::Xlsx)
    } else if PresentationKind::ALL
        .iter()
        .any(|kind| Some(kind.main_content_type()) == content_type)
    {
        Pptx::from_package(stored, options).map(Embedded::Pptx)
    } else {
        Err(not_a_package(format!(
            "the main part's content type is {content_type:?}"
        )))
    }
}

/// Write a document back into the part of `package`, saved as Strict; the part keeps its name
/// and content type. Needs the feature `strict`.
///
/// # Errors
///
/// The errors of the document's own save; [`PackageError::MissingPart`] when the package has
/// no such part.
#[cfg(feature = "strict")]
pub fn store_strict(
    package: &mut Package,
    name: &PartName,
    document: &Embedded,
) -> Result<(), PackageError> {
    let bytes = match document {
        Embedded::Docx(docx) => docx.to_bytes_strict()?,
        Embedded::Pptx(pptx) => pptx.to_bytes_strict()?,
        Embedded::Xlsx(xlsx) => xlsx.to_bytes_strict()?,
    };
    let content_type = package
        .content_type(name)
        .ok_or_else(|| PackageError::MissingPart(name.clone()))?
        .to_owned();
    package.set_part_bytes(name, &content_type, &bytes)
}

/// Write a document back into the part of `package`, saved as Transitional; the part keeps its
/// name and content type. Returns the save's warnings. Needs the feature `transitional`.
///
/// # Errors
///
/// The errors of the document's own save; [`PackageError::MissingPart`] when the package has
/// no such part.
#[cfg(feature = "transitional")]
pub fn store_transitional(
    package: &mut Package,
    name: &PartName,
    document: &Embedded,
) -> Result<Vec<SaveWarning>, PackageError> {
    let (bytes, warnings) = match document {
        Embedded::Docx(docx) => docx.to_bytes_transitional()?,
        Embedded::Pptx(pptx) => pptx.to_bytes_transitional()?,
        Embedded::Xlsx(xlsx) => xlsx.to_bytes_transitional()?,
    };
    let content_type = package
        .content_type(name)
        .ok_or_else(|| PackageError::MissingPart(name.clone()))?
        .to_owned();
    package.set_part_bytes(name, &content_type, &bytes)?;
    Ok(warnings)
}

#[cfg(test)]
mod tests;
