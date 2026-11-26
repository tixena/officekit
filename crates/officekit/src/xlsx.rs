//! The `.xlsx` file type.

pub mod color;
pub mod format;
pub mod header_footer;
pub mod number_format;
pub mod print;
pub mod reference;
pub mod sheets;
pub mod size;
#[cfg(test)]
mod tests;
pub mod value;

use std::path::Path;

use crate::parts::kinds::SpreadsheetKind;
#[cfg(feature = "strict")]
use crate::parts::pivot_table_cache_definition;
#[cfg(any(feature = "strict", feature = "transitional"))]
use crate::parts::worksheet;
#[cfg(feature = "strict")]
use crate::parts::{pivot_table, table_definition};
use crate::parts::{shared_string_table, styles, theme, workbook};
#[cfg(feature = "transitional")]
use crate::transitional::dates::{SaveWarning, convert_date_cells};
use officekit_markup::drawing::Theme;
#[cfg(feature = "strict")]
use officekit_markup::spreadsheet;
use officekit_markup::spreadsheet::{SharedStringTable, StyleSheet, Workbook};
#[cfg(feature = "strict")]
use officekit_strict::iso_date_time;
#[cfg(feature = "strict")]
use officekit_strict::traversal::Transform as _;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::values::parse::boolean;
use officekit_xml::write::WriteOptions;
use os_shim::System;
#[cfg(any(feature = "strict", feature = "transitional"))]
use os_shim::bucket::Condition;

use crate::drawing::scheme::{SchemeMap, ThemeColors};
use crate::file;
use crate::new_parts;
use crate::validation::fix::{Fix, Validation};
use crate::validation::validator::Validator;
use crate::xlsx::color::Palette;
use crate::xlsx::format::CellFormats;
use crate::xlsx::number_format::DateSystem;
use crate::xlsx::sheets::SheetEntry;
use crate::xlsx::value::SharedStrings;

/// A `.xlsx`-family file: its package, its workbook part, and the class and kind they give.
///
/// Make one with [`new`](Self::new), or open one with [`open`](Self::open) or
/// [`from_bytes`](Self::from_bytes). [`workbook`](Self::workbook) gives the workbook part as a
/// typed tree. The cells live in the worksheet parts, which `officekit::parts::worksheet` loads and
/// stores.
///
#[cfg_attr(
    feature = "transitional",
    doc = r#"
This writes a number into cell `A1` of the first sheet of a new workbook and reads it back.

```rust
use officekit::xlsx::Xlsx;
use officekit_markup::spreadsheet::{Cell, CellValue, Row};
use officekit_xml::packaging::part_name::PartName;
use officekit::parts::kinds::SpreadsheetKind;
use officekit::parts::worksheet;
use officekit_xml::write::WriteOptions;

# fn main() -> Result<(), Box<dyn core::error::Error>> {
let mut xlsx = Xlsx::new(SpreadsheetKind::Workbook)?;
let sheet = PartName::new("/xl/worksheets/sheet1.xml")?;

let mut value = CellValue::default();
value.value = "42".to_owned();
let mut cell = Cell::default();
cell.r = Some("A1".to_owned());
cell.value = Some(Box::new(value));
let mut row = Row::default();
row.r = Some("1".to_owned());
row.cell.push(cell);

let mut root = worksheet::load(xlsx.package(), &sheet, xlsx.options())?;
root.sheet_data.row.push(row);
worksheet::store_transitional(xlsx.package_mut(), &sheet, &root, WriteOptions::default())?;

let again = worksheet::load(xlsx.package(), &sheet, xlsx.options())?;
let first = again.sheet_data.row.first().and_then(|row| row.cell.first());
let text = first
    .and_then(|cell| cell.value.as_ref())
    .map(|value| value.value.as_str());
assert_eq!(text, Some("42"));
# Ok(())
# }
```
"#
)]
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
    /// The cell formats of the styles part (ECMA-376 Part 1, section 18.8.10); empty when the
    /// workbook part relates to none.
    ///
    /// # Errors
    ///
    /// The errors of [`styles`](Self::styles).
    pub fn cell_formats(&self) -> Result<CellFormats, PackageError> {
        Ok(CellFormats::from_styles(self.styles()?))
    }

    /// The class the file was read in, the class every store through it writes.
    #[must_use = "this only returns the value"]
    pub const fn conformance_class(&self) -> ConformanceClass {
        self.class
    }

    /// A new workbook of `kind`, as [`new`](Self::new) makes it, written to `path` as the package
    /// is built, with no change of class. Needs the feature `transitional`.
    ///
    /// # Errors
    ///
    /// The errors of [`new`](Self::new), and [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub async fn create<S>(
        system: &S,
        path: &Path,
        kind: SpreadsheetKind,
    ) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let xlsx = Self::new(kind)?;
        xlsx.package.save(system, path).await?;
        Ok(xlsx)
    }

    /// The workbook's date system: 1904 when `workbookPr/@date1904` is on, else 1900 as the
    /// file's class counts it (Part 1, section 18.17.4.1; Part 4, section 15.3.4).
    ///
    /// # Errors
    ///
    /// The errors of [`workbook`](Self::workbook).
    pub fn date_system(&self) -> Result<DateSystem, PackageError> {
        Ok(if date1904(&self.workbook()?) {
            DateSystem::V1904
        } else if self.class == ConformanceClass::Transitional {
            DateSystem::V1900Transitional
        } else {
            DateSystem::V1900
        })
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
    #[must_use = "this only returns the value"]
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
    pub async fn open<S>(
        system: &S,
        path: &Path,
        options: OpenOptions,
    ) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        Self::from_package(Package::open(system, path).await?, options)
    }

    /// The options every typed read of this file applies.
    #[must_use = "this only returns the value"]
    pub const fn options(&self) -> OpenOptions {
        self.options
    }

    /// The package, whose parts the generated part modules' `load` and `store_*` read and write.
    #[must_use = "this only returns the value"]
    pub const fn package(&self) -> &Package {
        &self.package
    }

    /// The package, to change its parts, relationships and content types directly.
    pub const fn package_mut(&mut self) -> &mut Package {
        &mut self.package
    }

    /// The indexed colours of the styles part's `colors` (ECMA-376 Part 1, section 18.8.27); the
    /// standard's when it states none or the workbook part relates to no styles part.
    ///
    /// # Errors
    ///
    /// The errors of [`styles`](Self::styles).
    pub fn palette(&self) -> Result<Palette, PackageError> {
        let styles = self.styles()?;
        Ok(Palette::new(
            styles.as_ref().and_then(|sheet| sheet.colors.as_deref()),
        ))
    }

    /// The root of the first part the workbook part relates to by `relationship_type`, loaded by
    /// `load`; `None` when it relates to none the package holds.
    fn related_root<R>(
        &self,
        relationship_type: &RelationshipType,
        load: fn(&Package, &PartName, OpenOptions) -> Result<R, PackageError>,
    ) -> Result<Option<R>, PackageError> {
        sheets::first_related(&self.package, &self.main, relationship_type)?
            .map(|name| load(&self.package, &name, self.options))
            .transpose()
    }

    /// Write the file to `path` in the Strict class; nothing is written when it fails. Needs the
    /// feature `strict`.
    ///
    /// # Errors
    ///
    /// The errors of [`to_bytes_strict`](Self::to_bytes_strict), and [`PackageError::Io`] when the
    /// file cannot be written.
    #[cfg(feature = "strict")]
    pub async fn save_strict<S>(&self, system: &S, path: &Path) -> Result<(), PackageError>
    where
        S: System + ?Sized,
    {
        let bytes = self.to_bytes_strict()?;
        system.write(path, &bytes, Condition::None).await?;
        Ok(())
    }

    /// Write the file to `path` in the Transitional class; nothing is written when it fails.
    /// Returns one warning for each date cell written as a number. Needs the feature
    /// `transitional`.
    ///
    /// # Errors
    ///
    /// The errors of [`to_bytes_transitional`](Self::to_bytes_transitional), and
    /// [`PackageError::Io`] when the file cannot be written.
    #[cfg(feature = "transitional")]
    pub async fn save_transitional<S>(
        &self,
        system: &S,
        path: &Path,
    ) -> Result<Vec<SaveWarning>, PackageError>
    where
        S: System + ?Sized,
    {
        let (bytes, warnings) = self.to_bytes_transitional()?;
        system.write(path, &bytes, Condition::None).await?;
        Ok(warnings)
    }

    /// Store `workbook` into the workbook part in the file's class.
    ///
    /// # Errors
    ///
    /// The errors of [`part_root::store`](crate::parts::part_root::store).
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

    /// The shared string table's root (ECMA-376 Part 1, section 12.3.15); `None` when the workbook
    /// part relates to none.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`] and [`shared_string_table::load`].
    pub fn shared_strings(&self) -> Result<Option<SharedStringTable>, PackageError> {
        self.related_root(
            &shared_string_table::RELATIONSHIP_TYPE,
            shared_string_table::load,
        )
    }

    /// The sheets of this workbook in the order of its sheet list (ECMA-376 Part 1, sections
    /// 18.2.19 and 18.2.20). An entry whose relationship names no part of this package is passed
    /// over.
    ///
    /// # Errors
    ///
    /// The errors of [`Xlsx::workbook`] and [`Package::related`].
    pub fn sheets(&self) -> Result<Vec<SheetEntry>, PackageError> {
        sheets::of(&self.package, &self.main, &self.workbook()?)
    }

    /// Everything [`to_bytes_strict`](Self::to_bytes_strict) refuses, in one call: one row per part
    /// and refused content. Empty when the save takes every part. Needs the feature `strict`.
    ///
    /// # Errors
    ///
    /// The errors of reading a part, as the save has them.
    #[cfg(feature = "strict")]
    pub fn strict_refusals(&self) -> Result<Vec<crate::StrictRefusal>, PackageError> {
        file::strict_refusals(&self.package, self.class, self.options)
    }

    /// The string items of the shared string table (ECMA-376 Part 1, section 18.4.9), ready to
    /// answer for an index; none when the workbook part relates to no such part.
    ///
    /// # Errors
    ///
    /// The errors of [`shared_strings`](Self::shared_strings).
    pub fn string_table(&self) -> Result<SharedStrings, PackageError> {
        Ok(SharedStrings::from_table(self.shared_strings()?))
    }

    /// The styles part's root (ECMA-376 Part 1, section 12.3.20); `None` when the workbook part
    /// relates to none.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`] and [`styles::load`].
    pub fn styles(&self) -> Result<Option<StyleSheet>, PackageError> {
        self.related_root(&styles::RELATIONSHIP_TYPE, styles::load)
    }

    /// The theme part's root (ECMA-376 Part 1, section 14.2.7); `None` when the workbook part
    /// relates to none.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`] and [`theme::load`].
    pub fn theme(&self) -> Result<Option<Theme>, PackageError> {
        self.related_root(&theme::RELATIONSHIP_TYPE, theme::load)
    }

    /// The twelve colours of the theme part's `a:clrScheme` (ECMA-376 Part 1, section 20.1.6.2),
    /// under the default mapping; no colour when the workbook part relates to no theme part.
    ///
    /// # Errors
    ///
    /// The errors of [`theme`](Self::theme).
    pub fn theme_colors(&self) -> Result<ThemeColors, PackageError> {
        let theme = self.theme()?;
        Ok(ThemeColors::new(
            theme.as_ref().map(|root| &*root.elements.color_scheme),
            SchemeMap::default(),
        ))
    }

    /// The file's bytes in the Strict class: as they are when it was read Strict, else every
    /// part a part module knows written again in Strict, every relationship type in Strict's
    /// spelling, and every other part, an embedded workbook among them, as it is. Before a
    /// worksheet, table definition, pivot table, or pivot cache definition is written, each
    /// `x:dynamicFilter/@maxVal` gives its `maxValIso` and each
    /// `x:pivotCacheDefinition/@refreshedDate` its `refreshedDateIso`, in the workbook's date
    /// system. Needs the feature `strict`.
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
            &mut |package, name, write| {
                match package.content_type(name) {
                    Some(worksheet::CONTENT_TYPE) => {
                        let sheet =
                            worksheet::load(package, name, self.options)?.transform(&mut dates);
                        worksheet::store_strict(package, name, &sheet, write)?;
                    }
                    Some(pivot_table_cache_definition::CONTENT_TYPE) => {
                        let definition =
                            pivot_table_cache_definition::load(package, name, self.options)?
                                .transform(&mut dates);
                        pivot_table_cache_definition::store_strict(
                            package,
                            name,
                            &definition,
                            write,
                        )?;
                    }
                    Some(table_definition::CONTENT_TYPE) => {
                        let table = table_definition::load(package, name, self.options)?
                            .transform(&mut dates);
                        table_definition::store_strict(package, name, &table, write)?;
                    }
                    Some(pivot_table::CONTENT_TYPE) => {
                        let pivot =
                            pivot_table::load(package, name, self.options)?.transform(&mut dates);
                        pivot_table::store_strict(package, name, &pivot, write)?;
                    }
                    _ => return Ok(false),
                }
                Ok(true)
            },
        )
    }

    /// The file's bytes in the Transitional class: as they are when it was read Transitional,
    /// else every part a part module knows written again in Transitional, each of its values in
    /// Transitional's own spelling (`50%` of an `a:alpha` is `50000`), every relationship type in
    /// Transitional's spelling, and every other part, an embedded workbook among them, as it is.
    /// Each date cell of a worksheet that holds a date becomes a number cell in the workbook's date
    /// system, with one warning per cell. A date cell with no value loses its type with no
    /// warning, and one whose text is not a date, or whose date comes before the 1900 date
    /// system's range, stays, so the write refuses it. Needs the feature `transitional`.
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
            &mut |package, name, write| {
                if package.content_type(name) != Some(worksheet::CONTENT_TYPE) {
                    return Ok(false);
                }
                let mut sheet = worksheet::load(package, name, self.options)?;
                warnings.extend(convert_date_cells(&mut sheet, date1904, name));
                worksheet::store_transitional(package, name, &sheet, write)?;
                Ok(true)
            },
        )?;
        Ok((bytes, warnings))
    }

    /// Validation of the file's package by `validator`, after the repairs `fix` names.
    ///
    /// # Errors
    ///
    /// The errors of [`Validator::validate_fixing`].
    pub fn validate(
        &mut self,
        validator: &Validator,
        fix: Fix,
    ) -> Result<Validation, PackageError> {
        validator.validate_fixing(self.package_mut(), fix)
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
    #[must_use = "this only returns the value"]
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
fn date1904(workbook: &Workbook) -> bool {
    workbook
        .properties
        .as_ref()
        .and_then(|properties| properties.date1904.as_deref())
        .and_then(boolean)
        .unwrap_or(false)
}
