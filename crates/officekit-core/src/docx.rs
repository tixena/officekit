//! The `.docx` file type, and hand-written operations over the generated `WordprocessingML` types.

pub mod chart;
mod header_footer;
pub mod image;
mod merge;
pub mod protection;
pub mod table;
#[cfg(test)]
mod tests;

use std::path::Path;

use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentProtectionTypesMember, DocumentSettings, Footer, Header,
    HeaderOrFooterTypeMember, TextRun,
};
use officekit_packaging::error::PackageError;
use officekit_packaging::image::Image;
use officekit_packaging::package::{OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelatedPart, RelationshipType, Target};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::write::WriteOptions;
use os_shim::System;

use crate::compact::{self, Comments};
use crate::file;
use crate::new_parts;
use crate::parts::kinds::WordprocessingKind;
use crate::parts::{document_settings, header, main_document};
#[cfg(feature = "transitional")]
use crate::transitional::dates::SaveWarning;

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
    /// Give section `section` a footer of type `kind`, holding `content`: a new footer part,
    /// referenced from the section's properties. A footer of the same type already there is
    /// replaced, and its part removed once no section references it. An even-page footer turns on
    /// `w:evenAndOddHeaders` in the settings. Sections count from 0; the last is the body's own.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] for a footer with no block or a section the document does not have,
    /// and the errors of the package operations.
    pub fn add_footer(
        &mut self,
        section: usize,
        kind: HeaderOrFooterTypeMember,
        content: &Footer,
    ) -> Result<PartName, PackageError> {
        header_footer::add(
            self,
            section,
            kind,
            header_footer::Which::Footer,
            content,
            content.content.len(),
        )
    }

    /// Give section `section` a header of type `kind`, holding `content`: a new header part,
    /// referenced from the section's properties. A header of the same type already there is
    /// replaced, and its part removed once no section references it. A first-page header turns on
    /// `w:titlePg` in the section, an even-page one `w:evenAndOddHeaders` in the settings.
    /// Sections count from 0; the last is the body's own.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] for a header with no block or a section the document does not have,
    /// and the errors of the package operations.
    pub fn add_header(
        &mut self,
        section: usize,
        kind: HeaderOrFooterTypeMember,
        content: &Header,
    ) -> Result<PartName, PackageError> {
        header_footer::add(
            self,
            section,
            kind,
            header_footer::Which::Header,
            content,
            content.content.len(),
        )
    }

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
    /// the file loads.
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

    /// `source`'s body blocks without its final `w:sectPr`, rewritten so that every id they use
    /// resolves in this file, for its main document `document`: each relationship, comment,
    /// footnote, endnote, style, list, range, and drawing id the blocks use names a part or entry
    /// this package holds, copied from `source` when it is not this file's own. `document`'s root
    /// gains the source root's namespace declarations and ignorable prefixes; the caller places
    /// the blocks and stores `document`.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] when a prefix is bound to different namespaces in the two roots of
    /// the main documents, or of a notes, styles, or numbering part the import adds to; the errors
    /// of reading and writing the parts the blocks name.
    pub fn import_body(
        &mut self,
        document: &mut Document,
        source: &Self,
    ) -> Result<Vec<BlockContent>, PackageError> {
        merge::import_body(self, document, source)
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

    pub const fn package_mut(&mut self) -> &mut Package {
        &mut self.package
    }

    /// Put one `w:documentProtection` entry into the settings part, in place of the one there or
    /// else at its place in the schema's order, making and relating the part when the file has
    /// none. `class` picks the form of the hash (see [`entry`](protection::entry)); an empty
    /// `password` gives the restriction alone.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::related_by_type`], [`Package::free_part_name`],
    /// [`Package::relate`], [`document_settings::load`], and
    /// [`part_root::store`](crate::part_root::store).
    pub fn protect(
        &mut self,
        edit: DocumentProtectionTypesMember,
        password: &str,
        salt: &[u8; 16],
        class: ConformanceClass,
    ) -> Result<(), PackageError> {
        let existing = self
            .package
            .related_by_type(Some(&self.main), &RelationshipType::SETTINGS)?
            .into_iter()
            .find_map(|related| {
                if let Target::Part(name) = related.target {
                    Some(name)
                } else {
                    None
                }
            });
        let (name, mut settings) = if let Some(name) = existing {
            let settings = document_settings::load(&self.package, &name, self.options)?;
            (name, settings)
        } else {
            let usual = PartName::new("/word/settings.xml")?;
            let name = if self.package.contains(&usual) {
                self.package.free_part_name("/word/settings{}.xml")?
            } else {
                usual
            };
            self.package.relate(
                Some(&self.main),
                &name,
                RelationshipType::SETTINGS.in_class(self.class),
            )?;
            (name, DocumentSettings::default())
        };
        settings.document_editing_restrictions =
            Some(Box::new(protection::entry(edit, password, salt, class)));
        file::store(
            &mut self.package,
            &name,
            &settings,
            document_settings::CONTENT_TYPE,
            self.class,
            self.write_options,
        )
    }

    /// Refill every `c:numCache` and `c:strCache` of the chart part `chart` from the workbook
    /// embedded in it, the target of the chart's `package` relationship: `c:ptCount` is the number
    /// of cells the `c:f` names, and each cell holding a value gives one `c:pt` at its place in the
    /// area, counted row by row. A `c:f` that is not one area of one sheet of the workbook (several
    /// areas, a defined name, a missing sheet) leaves its cache as it is. The workbook is not
    /// changed; the chart is stored back with the file's write options.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] for a chart that embeds no workbook, and the errors of reading the
    /// workbook and the chart and of storing the chart.
    pub fn refresh_chart_caches(
        &mut self,
        chart: &PartName,
        ranges: chart::ChartRanges,
    ) -> Result<(), PackageError> {
        chart::refresh(self, chart, ranges)
    }

    /// Remove section `section`'s footer of type `kind`; its part goes once no section references
    /// it. `w:titlePg` and `w:evenAndOddHeaders` stay as they are. Returns the removed part, if
    /// one was.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] for a section the document does not have, and the errors of the
    /// package operations.
    pub fn remove_footer(
        &mut self,
        section: usize,
        kind: HeaderOrFooterTypeMember,
    ) -> Result<Option<PartName>, PackageError> {
        header_footer::remove(self, section, kind, header_footer::Which::Footer)
    }

    /// Remove section `section`'s header of type `kind`; its part goes once no section references
    /// it. `w:titlePg` and `w:evenAndOddHeaders` stay as they are. Returns the removed part, if
    /// one was.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] for a section the document does not have, and the errors of the
    /// package operations.
    pub fn remove_header(
        &mut self,
        section: usize,
        kind: HeaderOrFooterTypeMember,
    ) -> Result<Option<PartName>, PackageError> {
        header_footer::remove(self, section, kind, header_footer::Which::Header)
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
    /// The errors of [`part_root::store`](crate::part_root::store).
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
    /// The errors of [`part_root::store`](crate::part_root::store).
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

    /// Put `image` into a new image part related from `part` (`media/image{n}.<extension>` beside
    /// it), point the run's first `a:blip` at it through `r:embed`, and size the picture's frame as
    /// `fit` says. The blip is the first one inside a `w:drawing`, in typed content or in the
    /// markup compatibility around one. Returns the relationship id the blip named before; it and
    /// its part stay, for the caller to remove with [`Package::remove_relationship`] once no
    /// markup names it.
    ///
    /// # Errors
    ///
    /// [`PackageError::Read`] when the run holds no picture, and the errors of
    /// [`Package::add_image`].
    pub fn set_picture(
        &mut self,
        part: &PartName,
        run: &mut TextRun,
        image: &Image,
        fit: image::PictureFit,
    ) -> Result<Option<String>, PackageError> {
        image::set_picture(self, part, run, image, fit)
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
