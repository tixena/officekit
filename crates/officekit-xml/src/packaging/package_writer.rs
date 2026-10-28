//! `PackageWriter`: a package written part by part into a sink that cannot
//! seek.

use alloc::collections::BTreeMap;
use std::io::{self, Write};

use crate::element::NamespaceDeclaration;
use crate::stream::writer::PartWriter;
use crate::write::Layout;
use os_shim::archive::ZipStreamWriter;

use crate::packaging::content_types;
use crate::packaging::error::PackageError;
use crate::packaging::package::{CONTENT_TYPES_PART, PACKAGE_RELATIONSHIPS_PART, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{self, RELATIONSHIPS_CONTENT_TYPE, Relationship, TargetMode};

/// Writes a package part by part into a sink that needs `Write` only.
///
/// Copied and streamed parts go into the zip as they come, each entry
/// deflated with its sizes in a data descriptor after it; `finish` writes
/// `[Content_Types].xml` and every relationship part last, because they name
/// parts and relationships known only at the end.
///
/// Make one with [`from_template`](Self::from_template). Copy the parts of a package with
/// [`copy_parts`](Self::copy_parts), stream a new part with [`part`](Self::part), add relationships
/// with [`relate`](Self::relate), and end with [`finish`](Self::finish).
///
/// ```
/// use officekit_xml::packaging::package::Package;
/// use officekit_xml::packaging::package_writer::PackageWriter;
/// use officekit_xml::packaging::part_name::PartName;
/// use officekit_xml::packaging::relationship::{Relationship, RelationshipType, TargetMode};
/// use officekit_xml::element::NamespaceDeclaration;
/// use officekit_xml::write::Layout;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let word = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// let content_type =
///     "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
/// let main = PartName::new("/word/document.xml")?;
///
/// let mut sink = Vec::new();
/// let mut writer = PackageWriter::from_template(&mut sink, &Package::new());
/// let mut part = writer.part(&main, content_type, Layout::Compact)?;
/// part.declaration()?;
/// part.start("w:document", &[NamespaceDeclaration::new("w".to_owned(), word.to_owned())], &[])?;
/// part.end("w:document")?;
/// let relationship = Relationship::new(
///     RelationshipType::OFFICE_DOCUMENT.transitional,
///     "word/document.xml",
///     TargetMode::Internal,
/// );
/// assert_eq!(writer.relate(None, relationship), "rId1");
/// writer.finish()?;
///
/// let package = Package::from_bytes(sink)?;
/// assert_eq!(package.content_type(&main), Some(content_type));
/// assert_eq!(
///     String::from_utf8(package.part_bytes(&main)?)?,
///     format!(
///         "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document xmlns:w=\"{word}\"/>"
///     )
/// );
/// # Ok(())
/// # }
/// ```
pub struct PackageWriter<W: Write> {
    defaults: Vec<(String, String)>,
    overrides: Vec<(PartName, String)>,
    /// Keyed by the folded source part name, `""` for the package itself.
    relationships: BTreeMap<String, (Option<PartName>, Vec<Relationship>)>,
    zip: ZipStreamWriter<W>,
}

impl<W: Write> PackageWriter<W> {
    /// Write every part of `package` but those in `except`, compared
    /// case-folded, as it is. `[Content_Types].xml` and the relationship
    /// parts are left to [`Self::finish`].
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the sink fails; the errors of
    /// [`Package::part_names`] and [`Package::part_bytes`].
    pub fn copy_parts(
        &mut self,
        package: &Package,
        except: &[PartName],
    ) -> Result<(), PackageError> {
        let skipped: Vec<String> = except.iter().map(PartName::folded).collect();
        for name in package.part_names()? {
            if skipped.contains(&name.folded()) {
                continue;
            }
            self.zip.start_entry(name.zip_entry_name())?;
            self.zip.write_all(&package.part_bytes(&name)?)?;
        }
        Ok(())
    }

    /// Write `[Content_Types].xml`, every relationship part, and the zip's
    /// central directory; give the sink back.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn finish(mut self) -> io::Result<W> {
        let has_rels_default = self
            .defaults
            .iter()
            .any(|(extension, _)| extension.eq_ignore_ascii_case("rels"));
        if !self.relationships.is_empty() && !has_rels_default {
            self.defaults
                .push(("rels".to_owned(), RELATIONSHIPS_CONTENT_TYPE.to_owned()));
        }
        self.zip.start_entry(zip_entry(CONTENT_TYPES_PART))?;
        let mut types = PartWriter::new(&mut self.zip, Layout::Compact);
        types.declaration()?;
        types.start("Types", &declare(content_types::NAMESPACE), &[])?;
        for (extension, content_type) in &self.defaults {
            types.empty(
                "Default",
                &[("Extension", extension), ("ContentType", content_type)],
            )?;
        }
        for (part, content_type) in &self.overrides {
            types.empty(
                "Override",
                &[("PartName", part.as_str()), ("ContentType", content_type)],
            )?;
        }
        types.end("Types")?;
        for (source, items) in self.relationships.values() {
            let name = source.as_ref().map_or_else(
                || PACKAGE_RELATIONSHIPS_PART.to_owned(),
                PartName::relationships_part,
            );
            self.zip.start_entry(zip_entry(&name))?;
            let mut out = PartWriter::new(&mut self.zip, Layout::Compact);
            out.declaration()?;
            out.start("Relationships", &declare(relationship::NAMESPACE), &[])?;
            for item in items {
                let mut attributes = vec![
                    ("Id", item.id.as_str()),
                    ("Type", item.type_uri.as_str()),
                    ("Target", item.target.as_str()),
                ];
                if item.target_mode == TargetMode::External {
                    attributes.push(("TargetMode", "External"));
                }
                out.empty("Relationship", &attributes)?;
            }
            out.end("Relationships")?;
        }
        self.zip.finish()
    }

    /// A writer into `sink` whose content types and relationships start as
    /// `template`'s.
    #[must_use]
    pub fn from_template(sink: W, template: &Package) -> Self {
        Self {
            defaults: template.content_type_defaults().to_vec(),
            overrides: template.content_type_overrides().to_vec(),
            relationships: template
                .relationship_sets()
                .into_iter()
                .map(|(source, items)| {
                    let key = source.as_ref().map_or_else(String::new, PartName::folded);
                    (key, (source, items))
                })
                .collect(),
            zip: ZipStreamWriter::new(sink),
        }
    }

    /// Register `content_type` for the part `name` and start its zip entry;
    /// the part is written through the returned writer, and ends when the
    /// next part starts or the package finishes. No namespace cleanup runs on
    /// it: its root declares the prefixes it is given.
    ///
    /// # Errors
    ///
    /// The sink's.
    pub fn part(
        &mut self,
        name: &PartName,
        content_type: &str,
        layout: Layout,
    ) -> io::Result<PartWriter<'_>> {
        self.register(name, content_type);
        self.zip.start_entry(name.zip_entry_name())?;
        Ok(PartWriter::new(&mut self.zip, layout))
    }

    /// Give the part `name` the content type `content_type`: its `Override`
    /// changes, or one is added unless its extension's `Default` gives it.
    fn register(&mut self, name: &PartName, content_type: &str) {
        let folded = name.folded();
        if let Some((_, registered)) = self
            .overrides
            .iter_mut()
            .find(|(part, _)| part.folded() == folded)
        {
            content_type.clone_into(registered);
            return;
        }
        let extension = name.extension();
        let by_default = self.defaults.iter().any(|(entry, default)| {
            entry.eq_ignore_ascii_case(&extension) && default == content_type
        });
        if !by_default {
            self.overrides.push((name.clone(), content_type.to_owned()));
        }
    }

    /// Add a relationship from `source` (`None`: the package) that the
    /// relationship parts will hold, under the id one above the highest
    /// `rIdN` the source has; returns that id.
    pub fn relate(&mut self, source: Option<&PartName>, relationship: Relationship) -> String {
        let key = source.map_or_else(String::new, PartName::folded);
        let (_, items) = self
            .relationships
            .entry(key)
            .or_insert_with(|| (source.cloned(), Vec::new()));
        let id = relationship::next_id(items);
        items.push(Relationship {
            id: id.clone(),
            ..relationship
        });
        id
    }
}

/// The default namespace declaration of `uri`.
fn declare(uri: &str) -> [NamespaceDeclaration; 1] {
    [NamespaceDeclaration::new(String::new(), uri.to_owned())]
}

/// The zip entry of a part name: the leading `/` dropped.
fn zip_entry(name: &str) -> &str {
    name.strip_prefix('/').unwrap_or(name)
}
