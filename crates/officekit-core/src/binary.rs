//! officekit's binary form: the model of a file's whole package, which one call hands to the next.
//!
//! The form is [`MAGIC`], a [`VERSION`] byte, then an rkyv archive. A part a module takes is held
//! as its typed root, never as XML; every other part as its bytes, with the content types and the
//! relationships as they were. The call that reads it parses no XML.

#[cfg(test)]
mod tests;

use officekit_packaging::content_types::ContentTypes;
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{Relationship, TargetMode, class_of_office_document};
use officekit_parts::{PartModel, bytes_of, model_of};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::write::WriteOptions;
use rkyv::rancor;
use rkyv::util::AlignedVec;

use crate::office_file::OfficeFile;

/// The bytes the binary form opens with, then a version byte.
pub const MAGIC: &[u8; 10] = b"officekit\0";

/// The package's own relationship part, which names the main part.
const PACKAGE_RELATIONSHIPS: &str = "/_rels/.rels";

/// The version of the binary form this officekit writes and reads.
pub const VERSION: u8 = 2;

/// Why bytes are not a file in officekit's binary form, or a file cannot go into it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BinaryError {
    #[error("not officekit's binary form: the bytes do not open with `officekit` and a zero byte")]
    NotOfficekit,
    #[error(transparent)]
    Package(#[from] PackageError),
    #[error(transparent)]
    Rkyv(#[from] rancor::Error),
    #[error("officekit's binary form version {found}, and this officekit reads version {VERSION}")]
    Version { found: u8 },
}

/// A package as officekit's model: every part, typed when a module takes it.
#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
struct Model {
    content_types: ContentTypes,
    /// The options the file was opened with, which `decode` opens it with again.
    options: OpenOptions,
    parts: Vec<(PartName, Part)>,
    /// As `Package::relationship_sets` returns them; `None` is the package itself.
    relationships: Vec<(Option<PartName>, Vec<Relationship>)>,
}

/// One part of the model: its bytes, or the typed root of the module that takes it.
#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)]
enum Part {
    Bytes(Vec<u8>),
    Typed(Box<PartModel>),
}

/// The file `bytes` hold in officekit's binary form.
///
/// Each typed part is written in the class of the package's main part; the package is rebuilt
/// with its content types and relationships as they were, and opened as
/// [`OfficeFile::from_bytes`] opens a package, with the options the file was opened with.
///
/// # Errors
///
/// [`BinaryError::NotOfficekit`] for bytes that do not open with [`MAGIC`] and a version byte,
/// [`BinaryError::Version`] for a version byte other than [`VERSION`], [`BinaryError::Rkyv`] for
/// an archive that does not hold a model, [`PackageError::MissingPart`] naming the package's
/// relationship part when no relationship names the main part, and the errors of [`bytes_of`],
/// [`Package::from_parts`] and [`OfficeFile::from_bytes`].
pub fn decode(bytes: &[u8]) -> Result<OfficeFile, BinaryError> {
    let (&found, archived) = bytes
        .strip_prefix(MAGIC)
        .and_then(<[u8]>::split_first)
        .ok_or(BinaryError::NotOfficekit)?;
    if found != VERSION {
        return Err(BinaryError::Version { found });
    }
    let mut aligned = AlignedVec::<16>::with_capacity(archived.len());
    aligned.extend_from_slice(archived);
    let model = rkyv::from_bytes::<Model, rancor::Error>(&aligned)?;
    let Some(class) = main_class(&model.relationships) else {
        return Err(PackageError::MissingPart(PartName::new(PACKAGE_RELATIONSHIPS)?).into());
    };
    let parts = model
        .parts
        .into_iter()
        .map(|(name, part)| {
            let held = match part {
                Part::Bytes(held) => held,
                Part::Typed(root) => bytes_of(&name, &root, class, WriteOptions::default())?,
            };
            Ok((name, held))
        })
        .collect::<Result<Vec<_>, PackageError>>()?;
    let package = Package::from_parts(model.content_types, model.relationships, parts)?;
    Ok(OfficeFile::from_package(package, model.options)?)
}

/// `file` in officekit's binary form: [`MAGIC`], [`VERSION`], then the archive of its package's
/// model and of its options.
///
/// Every part a module takes is stored as its typed root, read with markup compatibility kept
/// whatever the options, every other part as its bytes.
///
/// # Errors
///
/// The errors of [`model_of`] and of reading a part, and [`BinaryError::Rkyv`] when the archive
/// cannot be written.
pub fn encode(file: &OfficeFile) -> Result<Vec<u8>, BinaryError> {
    let package = package_of(file);
    let parts = package
        .part_names()?
        .into_iter()
        .map(|name| {
            let part = match model_of(package, &name, OpenOptions::default())? {
                Some(root) => Part::Typed(Box::new(root)),
                None => Part::Bytes(package.part_bytes(&name)?),
            };
            Ok((name, part))
        })
        .collect::<Result<Vec<_>, PackageError>>()?;
    let model = Model {
        content_types: package.content_types().clone(),
        options: file.options(),
        parts,
        relationships: package.relationship_sets(),
    };
    let archive = rkyv::to_bytes::<rancor::Error>(&model)?;
    let mut bytes = Vec::with_capacity(MAGIC.len() + 1 + archive.len());
    bytes.extend_from_slice(MAGIC);
    bytes.push(VERSION);
    bytes.extend_from_slice(&archive);
    Ok(bytes)
}

/// The class the package's `officeDocument` relationship to a part names, as
/// [`Package::main_part`] reads it; `None` when the package relates no main part.
fn main_class(relationships: &[(Option<PartName>, Vec<Relationship>)]) -> Option<ConformanceClass> {
    relationships
        .iter()
        .filter(|(source, _)| source.is_none())
        .flat_map(|(_, items)| items)
        .filter(|item| item.target_mode == TargetMode::Internal)
        .find_map(|item| class_of_office_document(&item.type_uri))
}

/// The package of `file`, whichever file type it is.
const fn package_of(file: &OfficeFile) -> &Package {
    match file {
        OfficeFile::Excel(xlsx) => xlsx.package(),
        OfficeFile::PowerPoint(pptx) => pptx.package(),
        OfficeFile::Word(docx) => docx.package(),
    }
}
