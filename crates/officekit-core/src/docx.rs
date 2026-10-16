//! The `.docx` file type, and hand-written operations over the generated `WordprocessingML` types.

pub mod table;
#[cfg(test)]
mod tests;

use std::path::Path;

use os_shim::System;

use crate::error::PackageError;
use crate::file;
use crate::markup::wordprocessing::{Document, Header};
use crate::new_parts;
use crate::packaging::package::{OpenOptions, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{RelatedPart, RelationshipType};
use crate::parts::kinds::WordprocessingKind;
use crate::parts::{header, main_document};
#[cfg(feature = "transitional")]
use crate::transitional::dates::SaveWarning;
use crate::xml::conformance::ConformanceClass;
use crate::xml::write::WriteOptions;

/// A `.docx`-family file: its package, its main document part, and the class and kind they give.
#[derive(Debug)]
pub struct Docx {
    class: ConformanceClass,
    kind: WordprocessingKind,
    main: PartName,
    options: OpenOptions,
    package: Package,
    write_options: WriteOptions,
}

impl Docx {
    /// The class the file was read in, the class every store through it writes.
    #[must_use]
    pub const fn conformance_class(&self) -> ConformanceClass {
        self.class
    }

    /// A new document of `kind`, as [`new`](Self::new) makes it, written to `path`.
    ///
    /// # Errors
    ///
    /// The errors of [`new`](Self::new), and [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub fn create<S>(
        system: &S,
        path: &Path,
        kind: WordprocessingKind,
    ) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let docx = Self::new(kind)?;
        docx.package.save(system, path)?;
        Ok(docx)
    }

    /// The footer parts the main document relates to.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`].
    pub fn footers(&self) -> Result<Vec<RelatedPart>, PackageError> {
        self.package
            .related_by_type(Some(&self.main), &RelationshipType::FOOTER)
    }

    /// The file whose bytes are `bytes`.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::from_bytes`] and [`from_package`](Self::from_package).
    pub fn from_bytes(bytes: Vec<u8>, options: OpenOptions) -> Result<Self, PackageError> {
        Self::from_package(Package::from_bytes(bytes)?, options)
    }

    /// The file `package` holds. Its class is the one its `officeDocument` relationship's type
    /// names, its kind the one its main part's content type names; `options` applies to every part
    /// the file loads (D5).
    ///
    /// # Errors
    ///
    /// [`PackageError::ClassNotEnabled`] for a class whose feature is off;
    /// [`PackageError::ContentType`] for a main part that is not a `WordprocessingML` one;
    /// [`PackageError::MissingPart`] when the package relates no main part.
    pub fn from_package(package: Package, options: OpenOptions) -> Result<Self, PackageError> {
        let main = file::main_part(&package)?;
        let kind = file::kind(
            &main,
            &WordprocessingKind::ALL,
            WordprocessingKind::main_content_type,
            "a WordprocessingML main part",
        )?;
        Ok(Self {
            class: main.class,
            kind,
            main: main.name,
            options,
            package,
            write_options: WriteOptions::default(),
        })
    }

    /// The typed root of the header part `name`.
    ///
    /// # Errors
    ///
    /// The errors of [`header::load`].
    pub fn header(&self, name: &PartName) -> Result<Header, PackageError> {
        header::load(&self.package, name, self.options)
    }

    /// The header parts the main document relates to.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`].
    pub fn headers(&self) -> Result<Vec<RelatedPart>, PackageError> {
        self.package
            .related_by_type(Some(&self.main), &RelationshipType::HEADER)
    }

    /// The kind its main part's content type names.
    #[must_use]
    pub const fn kind(&self) -> WordprocessingKind {
        self.kind
    }

    /// The typed root of the main document part.
    ///
    /// # Errors
    ///
    /// The errors of [`main_document::load`].
    pub fn main_document(&self) -> Result<Document, PackageError> {
        main_document::load(&self.package, &self.main, self.options)
    }

    /// The main document part, the one the `officeDocument` relationship names.
    #[must_use]
    pub const fn main_document_part(&self) -> &PartName {
        &self.main
    }

    /// A new Transitional document of `kind`, in memory: its main document part, holding one empty
    /// paragraph, with the kind's content type.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
    pub fn new(kind: WordprocessingKind) -> Result<Self, PackageError> {
        let (package, main) = new_parts::word(kind.main_content_type())?;
        Ok(Self {
            class: ConformanceClass::Transitional,
            kind,
            main,
            options: OpenOptions::default(),
            package,
            write_options: WriteOptions::default(),
        })
    }

    /// The file at `path`, read through `system`.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::open`] and [`from_package`](Self::from_package).
    pub fn open<S>(system: &S, path: &Path, options: OpenOptions) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        Self::from_package(Package::open(system, path)?, options)
    }

    /// The package, whose parts the generated part modules' `load` and `store_*` read and write.
    #[must_use]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    /// The package, to change.
    pub const fn package_mut(&mut self) -> &mut Package {
        &mut self.package
    }

    /// Write the file to `path` in the Strict class; nothing is written when it fails.
    ///
    /// # Errors
    ///
    /// The errors of [`to_bytes_strict`](Self::to_bytes_strict), and [`PackageError::Io`] when the
    /// file cannot be written.
    #[cfg(feature = "strict")]
    pub fn save_strict<S>(&self, system: &S, path: &Path) -> Result<(), PackageError>
    where
        S: System + ?Sized,
    {
        let bytes = self.to_bytes_strict()?;
        system.write(path, &bytes)?;
        Ok(())
    }

    /// Write the file to `path` in the Transitional class; nothing is written when it fails.
    ///
    /// # Errors
    ///
    /// The errors of [`to_bytes_transitional`](Self::to_bytes_transitional), and
    /// [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub fn save_transitional<S>(
        &self,
        system: &S,
        path: &Path,
    ) -> Result<Vec<SaveWarning>, PackageError>
    where
        S: System + ?Sized,
    {
        let (bytes, warnings) = self.to_bytes_transitional()?;
        system.write(path, &bytes)?;
        Ok(warnings)
    }

    /// Store `content` into the header part `name` in the file's class.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::store`].
    pub fn set_header(&mut self, name: &PartName, content: &Header) -> Result<(), PackageError> {
        file::store(
            &mut self.package,
            name,
            content,
            header::CONTENT_TYPE,
            self.class,
            self.write_options,
        )
    }

    /// Store `document` into the main document part in the file's class.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::store`].
    pub fn set_main_document(&mut self, document: &Document) -> Result<(), PackageError> {
        file::store(
            &mut self.package,
            &self.main,
            document,
            self.kind.main_content_type(),
            self.class,
            self.write_options,
        )
    }

    /// The options every store through the file writes with, its saves included (D29).
    pub const fn set_write_options(&mut self, options: WriteOptions) {
        self.write_options = options;
    }

    /// The file's bytes in the Strict class: as they are when it was read Strict (D9), else every
    /// part a part module knows written again in Strict, every relationship type in Strict's
    /// spelling, and every other part as it is.
    ///
    /// # Errors
    ///
    /// [`PackageError::Write`] naming the first part that holds content Strict has no form for
    /// (D22), and the errors of reading a part and of building the archive.
    #[cfg(feature = "strict")]
    pub fn to_bytes_strict(&self) -> Result<Vec<u8>, PackageError> {
        file::to_bytes_in_class(
            &self.package,
            self.class,
            ConformanceClass::Strict,
            self.options,
            self.write_options,
            &mut |_, _| Ok(false),
        )
    }

    /// The file's bytes in the Transitional class: as they are when it was read Transitional (D9),
    /// else every part a part module knows written again in Transitional, every relationship type
    /// in Transitional's spelling, and every other part as it is. The list of warnings is empty:
    /// an embedded workbook is a part carried as it is.
    ///
    /// # Errors
    ///
    /// The errors of reading a part and of building the archive.
    #[cfg(feature = "transitional")]
    pub fn to_bytes_transitional(&self) -> Result<(Vec<u8>, Vec<SaveWarning>), PackageError> {
        let bytes = file::to_bytes_in_class(
            &self.package,
            self.class,
            ConformanceClass::Transitional,
            self.options,
            self.write_options,
            &mut |_, _| Ok(false),
        )?;
        Ok((bytes, Vec::new()))
    }
}
