//! The `.xlsx` file type.

#[cfg(test)]
mod tests;

use std::path::Path;

#[cfg(feature = "strict")]
use officekit_markup::spreadsheet;
use officekit_markup::spreadsheet::Workbook;
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_parts::kinds::SpreadsheetKind;
#[cfg(feature = "strict")]
use officekit_parts::pivot_table_cache_definition;
use officekit_parts::workbook;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_parts::worksheet;
#[cfg(feature = "strict")]
use officekit_parts::{pivot_table, table_definition};
#[cfg(feature = "strict")]
use officekit_strict::iso_date_time;
#[cfg(feature = "transitional")]
use officekit_transitional::dates::{SaveWarning, convert_date_cells};
#[cfg(feature = "strict")]
use officekit_traversal::Transform as _;
use officekit_xml::conformance::ConformanceClass;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_xml::values::parse::boolean;
use officekit_xml::write::WriteOptions;
use os_shim::System;

use crate::file;
use crate::new_parts;

/// A `.xlsx`-family file: its package, its workbook part, and the class and kind they give.
#[derive(Debug)]
pub struct Xlsx {
    class: ConformanceClass,
    kind: SpreadsheetKind,
    main: PartName,
    options: OpenOptions,
    package: Package,
    write_options: WriteOptions,
}

/// A Strict save's context over the worksheet, table definition, pivot table, and pivot cache
/// definition parts: each serial date-time Strict writes in ISO 8601 is given its ISO attribute in
/// the workbook's date system, which the Strict write keeps.
#[cfg(feature = "strict")]
struct IsoDates {
    date1904: bool,
}

impl Xlsx {
    /// The class the file was read in, the class every store through it writes.
    #[must_use]
    pub const fn conformance_class(&self) -> ConformanceClass {
        self.class
    }

    /// A new workbook of `kind`, as [`new`](Self::new) makes it, written to `path`.
    ///
    /// # Errors
    ///
    /// The errors of [`new`](Self::new), and [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub fn create<S>(system: &S, path: &Path, kind: SpreadsheetKind) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let xlsx = Self::new(kind)?;
        xlsx.package.save(system, path)?;
        Ok(xlsx)
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
    /// [`PackageError::ContentType`] for a main part that is not a `SpreadsheetML` one;
    /// [`PackageError::MissingPart`] when the package relates no main part.
    pub fn from_package(package: Package, options: OpenOptions) -> Result<Self, PackageError> {
        let main = file::main_part(&package)?;
        let kind = file::kind(
            &main,
            &SpreadsheetKind::ALL,
            SpreadsheetKind::main_content_type,
            "a SpreadsheetML main part",
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
    pub const fn kind(&self) -> SpreadsheetKind {
        self.kind
    }

    /// A new Transitional workbook of `kind`, in memory: its workbook part, with the kind's content
    /// type, and one empty sheet, `Sheet1`.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
    pub fn new(kind: SpreadsheetKind) -> Result<Self, PackageError> {
        let (package, main) = new_parts::excel(kind.main_content_type())?;
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

    /// Store `workbook` into the workbook part in the file's class.
    ///
    /// # Errors
    ///
    /// The errors of [`part_root::store`](officekit_parts::part_root::store).
    pub fn set_workbook(&mut self, workbook: &Workbook) -> Result<(), PackageError> {
        file::store(
            &mut self.package,
            &self.main,
            workbook,
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
    /// spelling, and every other part, an embedded workbook among them, as it is. Before a
    /// worksheet, table definition, pivot table, or pivot cache definition is written, each
    /// `x:dynamicFilter/@maxVal` gives its `maxValIso` and each
    /// `x:pivotCacheDefinition/@refreshedDate` its `refreshedDateIso`, in the workbook's date
    /// system.
    ///
    /// # Errors
    ///
    /// [`PackageError::Write`] naming the first part that holds content Strict has no form for
    /// and the errors of reading a part and of building the archive.
    #[cfg(feature = "strict")]
    pub fn to_bytes_strict(&self) -> Result<Vec<u8>, PackageError> {
        // A save in the class the file was read in writes the package as it is, so it never reads
        // the workbook.
        let mut dates = IsoDates {
            date1904: self.class != ConformanceClass::Strict && date1904(&self.workbook()?),
        };
        file::to_bytes_in_class(
            &self.package,
            self.class,
            ConformanceClass::Strict,
            self.options,
            self.write_options,
            &mut |package, name| {
                match package.content_type(name) {
                    Some(worksheet::CONTENT_TYPE) => {
                        let sheet =
                            worksheet::load(package, name, self.options)?.transform(&mut dates);
                        worksheet::store_strict(package, name, &sheet, self.write_options)?;
                    }
                    Some(pivot_table_cache_definition::CONTENT_TYPE) => {
                        let definition =
                            pivot_table_cache_definition::load(package, name, self.options)?
                                .transform(&mut dates);
                        pivot_table_cache_definition::store_strict(
                            package,
                            name,
                            &definition,
                            self.write_options,
                        )?;
                    }
                    Some(table_definition::CONTENT_TYPE) => {
                        let table = table_definition::load(package, name, self.options)?
                            .transform(&mut dates);
                        table_definition::store_strict(package, name, &table, self.write_options)?;
                    }
                    Some(pivot_table::CONTENT_TYPE) => {
                        let pivot =
                            pivot_table::load(package, name, self.options)?.transform(&mut dates);
                        pivot_table::store_strict(package, name, &pivot, self.write_options)?;
                    }
                    _ => return Ok(false),
                }
                Ok(true)
            },
        )
    }

    /// The file's bytes in the Transitional class: as they are when it was read Transitional,
    /// else every part a part module knows written again in Transitional, every relationship type
    /// in Transitional's spelling, and every other part, an embedded workbook among them, as it is.
    /// Each date cell of a worksheet becomes a number cell in the workbook's date system, with one
    /// warning per cell.
    ///
    /// # Errors
    ///
    /// The errors of reading a part and of building the archive.
    #[cfg(feature = "transitional")]
    pub fn to_bytes_transitional(&self) -> Result<(Vec<u8>, Vec<SaveWarning>), PackageError> {
        // A save in the class the file was read in writes the package as it is, so it never reads
        // the workbook.
        let date1904 = self.class != ConformanceClass::Transitional && date1904(&self.workbook()?);
        let mut warnings = Vec::new();
        let bytes = file::to_bytes_in_class(
            &self.package,
            self.class,
            ConformanceClass::Transitional,
            self.options,
            self.write_options,
            &mut |package, name| {
                if package.content_type(name) != Some(worksheet::CONTENT_TYPE) {
                    return Ok(false);
                }
                let mut sheet = worksheet::load(package, name, self.options)?;
                warnings.extend(convert_date_cells(&mut sheet, date1904, name));
                worksheet::store_transitional(package, name, &sheet, self.write_options)?;
                Ok(true)
            },
        )?;
        Ok((bytes, warnings))
    }

    /// The typed root of the workbook part.
    ///
    /// # Errors
    ///
    /// The errors of [`workbook::load`].
    pub fn workbook(&self) -> Result<Workbook, PackageError> {
        workbook::load(&self.package, &self.main, self.options)
    }

    /// The workbook part, the one the `officeDocument` relationship names.
    #[must_use]
    pub const fn workbook_part(&self) -> &PartName {
        &self.main
    }
}

#[cfg(feature = "strict")]
impl officekit_markup::Context for IsoDates {
    fn spreadsheet(&mut self) -> Option<&mut dyn spreadsheet::Context> {
        Some(self)
    }
}

#[cfg(feature = "strict")]
impl spreadsheet::Context for IsoDates {
    fn dynamic_filter(
        &mut self,
        node: &spreadsheet::DynamicFilter,
    ) -> Option<spreadsheet::DynamicFilter> {
        let iso = iso_date_time(node.max_val.as_deref()?, self.date1904)?;
        let mut filter = node.transform_children(self);
        filter.max_val_iso = Some(iso);
        Some(filter)
    }

    fn pivot_cache_definition(
        &mut self,
        node: &spreadsheet::PivotCacheDefinition,
    ) -> Option<spreadsheet::PivotCacheDefinition> {
        let iso = iso_date_time(node.refreshed_date.as_deref()?, self.date1904)?;
        let mut definition = node.transform_children(self);
        definition.refreshed_date_iso = Some(iso);
        Some(definition)
    }
}

/// Whether the workbook counts its dates in the 1904 date system (`workbookPr/@date1904`).
#[cfg(any(feature = "strict", feature = "transitional"))]
fn date1904(workbook: &Workbook) -> bool {
    workbook
        .properties
        .as_ref()
        .and_then(|properties| properties.date1904.as_deref())
        .and_then(boolean)
        .unwrap_or(false)
}
