//! The `.pptx` file type.

#[cfg(test)]
mod tests;

use std::path::Path;

use officekit_markup::presentation::Presentation;
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_parts::kinds::PresentationKind;
use officekit_parts::presentation;
#[cfg(feature = "transitional")]
use officekit_transitional::dates::SaveWarning;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::write::WriteOptions;
use os_shim::System;
#[cfg(any(feature = "strict", feature = "transitional"))]
use os_shim::WriteMode;

use crate::compact::{self, Comments};
use crate::file;
use crate::new_parts;

/// A `.pptx`-family file: its package, its presentation part, and the class and kind they give.
#[derive(Debug)]
pub struct Pptx {
    class: ConformanceClass,
    kind: PresentationKind,
    main: PartName,
    options: OpenOptions,
    package: Package,
    write_options: WriteOptions,
}

impl Pptx {
    /// Merge every stretch of neighbouring runs that render the same, in every part of the file
    /// and every Office document embedded in it.
    ///
    /// # Errors
    ///
    /// A part that cannot be read, parsed, or stored.
    pub fn compact_runs(&mut self, comments: Comments) -> Result<(), PackageError> {
        compact::compact_package(self.package_mut(), comments).map(|_| ())
    }

    /// The class the file was read in, the class every store through it writes.
    #[must_use]
    pub const fn conformance_class(&self) -> ConformanceClass {
        self.class
    }

    /// A new presentation of `kind`, as [`new`](Self::new) makes it, written to `path`.
    ///
    /// # Errors
    ///
    /// The errors of [`new`](Self::new), and [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub fn create<S>(system: &S, path: &Path, kind: PresentationKind) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let pptx = Self::new(kind)?;
        pptx.package.save(system, path)?;
        Ok(pptx)
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
    /// the file loads.
    ///
    /// # Errors
    ///
    /// [`PackageError::ClassNotEnabled`] for a class whose feature is off;
    /// [`PackageError::ContentType`] for a main part that is not a `PresentationML` one;
    /// [`PackageError::MissingPart`] when the package relates no main part.
    pub fn from_package(package: Package, options: OpenOptions) -> Result<Self, PackageError> {
        let main = file::main_part(&package)?;
        let kind = file::kind(
            &main,
            &PresentationKind::ALL,
            PresentationKind::main_content_type,
            "a PresentationML main part",
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

    /// The kind its main part's content type names.
    #[must_use]
    pub const fn kind(&self) -> PresentationKind {
        self.kind
    }

    /// A new Transitional presentation of `kind`, in memory, with no slides: its presentation part,
    /// with the kind's content type, a 16:9 slide size, one slide master with one blank layout, and
    /// the theme the presentation and the master relate to.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
    pub fn new(kind: PresentationKind) -> Result<Self, PackageError> {
        let (package, main) = new_parts::powerpoint(kind.main_content_type())?;
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

    /// The options every typed read of this file applies.
    #[must_use]
    pub const fn options(&self) -> OpenOptions {
        self.options
    }

    /// The package, whose parts the generated part modules' `load` and `store_*` read and write.
    #[must_use]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    pub const fn package_mut(&mut self) -> &mut Package {
        &mut self.package
    }

    /// The typed root of the presentation part.
    ///
    /// # Errors
    ///
    /// The errors of [`presentation::load`].
    pub fn presentation(&self) -> Result<Presentation, PackageError> {
        presentation::load(&self.package, &self.main, self.options)
    }

    /// The presentation part, the one the `officeDocument` relationship names.
    #[must_use]
    pub const fn presentation_part(&self) -> &PartName {
        &self.main
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
        system.write(path, &bytes, WriteMode::Replace)?;
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
        system.write(path, &bytes, WriteMode::Replace)?;
        Ok(warnings)
    }

    /// Store `presentation` into the presentation part in the file's class.
    ///
    /// # Errors
    ///
    /// The errors of [`part_root::store`](officekit_parts::part_root::store).
    pub fn set_presentation(&mut self, presentation: &Presentation) -> Result<(), PackageError> {
        file::store(
            &mut self.package,
            &self.main,
            presentation,
            self.kind.main_content_type(),
            self.class,
            self.write_options,
        )
    }

    /// The options every store through the file writes with, its saves included.
    pub const fn set_write_options(&mut self, options: WriteOptions) {
        self.write_options = options;
    }

    /// The file's bytes in the Strict class: as they are when it was read Strict, else every
    /// part a part module knows written again in Strict, every relationship type in Strict's
    /// spelling, and every other part as it is.
    ///
    /// # Errors
    ///
    /// [`PackageError::Write`] naming the first part that holds content Strict has no form for
    /// and the errors of reading a part and of building the archive.
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

    /// The file's bytes in the Transitional class: as they are when it was read Transitional,
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
