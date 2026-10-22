//! A package: an os-shim `MemorySystem` holding one file per part, with
//! `[Content_Types].xml` and every relationship part kept parsed and written
//! back into the memory filesystem whenever they change.

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt;
use std::io::{Cursor, Read as _};
use std::path::{Path, PathBuf};

use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Attribute, Document, Element, NamespaceDeclaration, Node};
use officekit_xml::mc::MarkupCompatibility;
use officekit_xml::name::QName;
use officekit_xml::parse::parse;
use officekit_xml::stream::reader::PartReader;
use officekit_xml::write::WriteOptions;
use os_shim::mock::MemorySystem;
use os_shim::{System, WriteMode};

use crate::content_types::{ContentTypes, XML_CONTENT_TYPE};
use crate::error::PackageError;
use crate::image::{Image, ImageFormat};
use crate::part_allocation;
use crate::part_name::PartName;
use crate::relationship::{
    self, RELATIONSHIPS_CONTENT_TYPE, RelatedPart, Relationship, RelationshipType, Target,
    TargetMode, class_of_office_document,
};

#[cfg(test)]
mod tests;

/// The XML declaration of every part the package writes itself.
pub const DECLARATION: &str = "xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"";

/// The part that maps every part to its content type.
pub(crate) const CONTENT_TYPES_PART: &str = "/[Content_Types].xml";

/// The relationship part of the package itself.
pub(crate) const PACKAGE_RELATIONSHIPS_PART: &str = "/_rels/.rels";

/// The namespace of a custom XML part's item properties.
const CUSTOM_XML_NAMESPACE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/customXml";

/// The content type of a custom XML part's item properties.
const CUSTOM_XML_PROPERTIES_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.customXmlProperties+xml";

/// The XML declaration Office writes on a custom XML part's item properties.
const CUSTOM_XML_PROPERTIES_DECLARATION: &str =
    "xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"";

/// Where `add_custom_xml` puts the item and its properties.
const CUSTOM_XML_ITEM: &str = "/customXml/item{}.xml";
const CUSTOM_XML_ITEM_PROPERTIES: &str = "/customXml/itemProps{}.xml";

/// What a file of the memory filesystem is to the package.
enum FileKind {
    ContentTypes,
    Part,
    /// A relationship part, with its source: `None` for the package.
    Relationships(Option<PartName>),
}

/// The part a package's `officeDocument` relationship names, that part's content type, and the
/// class the relationship's type names.
#[non_exhaustive]
pub struct MainPart {
    pub class: ConformanceClass,
    pub content_type: Option<String>,
    pub name: PartName,
}

/// What a read does with the parts it parses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpenOptions {
    pub markup_compatibility: MarkupCompatibility,
}

/// An Office Open XML package.
pub struct Package {
    content_types: ContentTypes,
    fs: MemorySystem,
    /// Keyed by the folded source part name, `""` for the package itself.
    relationships: BTreeMap<String, RelationshipSet>,
}

/// The relationships of one source, as its relationship part holds them.
#[derive(Clone, Debug, Default)]
struct RelationshipSet {
    items: Vec<Relationship>,
    /// The source part, `None` for the package itself.
    source: Option<PartName>,
}

impl Package {
    /// Add `/customXml/item{n}.xml` as `application/xml`, related from
    /// `source`, and `/customXml/itemProps{n}.xml`, related from the item,
    /// naming `item_id` and one schema reference per entry of
    /// `schema_references`; `n` is the first number free for both.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the memory filesystem refuses a write.
    pub fn add_custom_xml(
        &mut self,
        source: &PartName,
        data: &[u8],
        item_id: &str,
        schema_references: &[&str],
    ) -> Result<PartName, PackageError> {
        let number = part_allocation::first_free_number(
            &[CUSTOM_XML_ITEM, CUSTOM_XML_ITEM_PROPERTIES],
            &self.taken()?,
        )?;
        let item = PartName::new(&part_allocation::fill(CUSTOM_XML_ITEM, number))?;
        let properties = PartName::new(&part_allocation::fill(CUSTOM_XML_ITEM_PROPERTIES, number))?;
        self.set_part_bytes(&item, XML_CONTENT_TYPE, data)?;
        self.relate(
            Some(source),
            &item,
            RelationshipType::CUSTOM_XML.transitional,
        )?;
        self.set_part_bytes(
            &properties,
            CUSTOM_XML_PROPERTIES_CONTENT_TYPE,
            &item_properties(item_id, schema_references),
        )?;
        self.relate(
            Some(&item),
            &properties,
            RelationshipType::CUSTOM_XML_PROPERTIES.transitional,
        )?;
        Ok(item)
    }

    /// Add `image` as the first free `<source folder>/media/image{n}.<extension>`, related from
    /// `source` with the image relationship type of the package's class; returns its name and the
    /// relationship id.
    ///
    /// # Errors
    ///
    /// [`PackageError::PartName`] for a source folder that gives no valid part name;
    /// [`PackageError::Io`] when the memory filesystem refuses a write.
    pub fn add_image(
        &mut self,
        source: &PartName,
        image: &Image,
    ) -> Result<(PartName, String), PackageError> {
        let class = match self.main_part() {
            Ok(main) => main.class,
            Err(PackageError::MissingPart(_)) => ConformanceClass::Transitional,
            Err(error) => return Err(error),
        };
        let template = format!(
            "{}/media/image{{}}.{}",
            source.directory(),
            image.format().extension()
        );
        self.add_part(
            Some(source),
            RelationshipType::IMAGE.in_class(class),
            image.content_type(),
            &template,
            image.bytes(),
        )
    }

    /// Add a part related from `source` (`None`: the package); returns its
    /// name, the first free one `template` gives, and its relationship id.
    ///
    /// # Errors
    ///
    /// [`PackageError::PartName`] for a template without `{}` or one that
    /// gives no valid part name; [`PackageError::Io`] when the memory
    /// filesystem refuses a write.
    pub fn add_part(
        &mut self,
        source: Option<&PartName>,
        relationship_type: &str,
        content_type: &str,
        template: &str,
        bytes: &[u8],
    ) -> Result<(PartName, String), PackageError> {
        let name = self.free_part_name(template)?;
        self.set_part_bytes(&name, content_type, bytes)?;
        let id = self.relate(source, &name, relationship_type)?;
        Ok((name, id))
    }

    /// Add a relationship from `source` under the id the caller gives, as a
    /// copied part keeps its relationship ids.
    ///
    /// # Errors
    ///
    /// [`PackageError::DuplicateRelationship`] when the source already has
    /// the id; [`PackageError::Io`] when the memory filesystem refuses a write.
    pub fn add_relationship(
        &mut self,
        source: Option<&PartName>,
        id: &str,
        target: &Target,
        relationship_type: &str,
    ) -> Result<(), PackageError> {
        let key = source_key(source);
        let set = self
            .relationships
            .entry(key.clone())
            .or_insert_with(|| RelationshipSet {
                items: Vec::new(),
                source: source.cloned(),
            });
        if set.items.iter().any(|relationship| relationship.id == id) {
            return Err(PackageError::DuplicateRelationship {
                id: id.to_owned(),
                part: source.cloned(),
            });
        }
        let (written, target_mode) = match target {
            Target::External(uri) => (uri.clone(), TargetMode::External),
            Target::Part(part) => (
                relationship::relative(set.directory(), part),
                TargetMode::Internal,
            ),
        };
        set.items.push(Relationship {
            id: id.to_owned(),
            target: written,
            target_mode,
            type_uri: relationship_type.to_owned(),
        });
        self.write_relationships(&key)
    }

    /// Whether the archive holds the part called `name`, compared case-folded.
    #[must_use]
    pub fn contains(&self, name: &PartName) -> bool {
        matches!(self.locate(name.as_str()), Ok(Some(_)))
    }

    /// The content type of a part the package holds: its `Override`, else its extension's
    /// `Default`. `None` for a part the package does not hold, whatever the extension's
    /// `Default` says.
    #[must_use]
    pub fn content_type(&self, name: &PartName) -> Option<&str> {
        if self.contains(name) {
            self.content_types.of(name)
        } else {
            None
        }
    }

    /// The `Default` entries of `[Content_Types].xml`, extension and content
    /// type, in the order the package has them.
    #[must_use]
    pub fn content_type_defaults(&self) -> &[(String, String)] {
        self.content_types.defaults()
    }

    /// The `Override` entries of `[Content_Types].xml`, part and content type,
    /// in the order the package has them.
    #[must_use]
    pub fn content_type_overrides(&self) -> &[(PartName, String)] {
        self.content_types.overrides()
    }

    /// An independent copy: `MemorySystem::clone` shares its files, so the
    /// copy goes through the package's bytes into a new filesystem.
    ///
    /// # Errors
    ///
    /// The errors of [`to_bytes`](Self::to_bytes) and
    /// [`from_bytes`](Self::from_bytes).
    pub fn duplicate(&self) -> Result<Self, PackageError> {
        Self::from_bytes(self.to_bytes()?)
    }

    /// Every file of the memory filesystem, by path.
    fn files(&self) -> Result<Vec<String>, PackageError> {
        Ok(self
            .fs
            .walk_dir(Path::new("/"), false, true)?
            .into_iter()
            .filter(|entry| entry.is_file)
            .map(|entry| entry.path.to_string_lossy().into_owned())
            .collect())
    }

    /// The first free name `template` gives: its `{}` replaced by the first
    /// number from 1 that gives a name no part has, compared case-folded.
    ///
    /// # Errors
    ///
    /// [`PackageError::PartName`] for a template without `{}` or one that
    /// gives no valid part name.
    pub fn free_part_name(&self, template: &str) -> Result<PartName, PackageError> {
        part_allocation::first_free(template, &self.taken()?)
    }

    /// Unpack the bytes of a zip archive into memory, and parse its
    /// `[Content_Types].xml` and relationship parts. The archive's central
    /// directory is read, so entries that carry data descriptors load too.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] for bytes that are not a zip archive;
    /// [`PackageError::Xml`] for a content-types or relationship part that is
    /// not well-formed, and [`PackageError::Read`] for one whose elements are
    /// not what the part holds; [`PackageError::PartName`] for a relationship
    /// part whose source is no valid part name.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PackageError> {
        let mut package = Self {
            content_types: ContentTypes::default(),
            fs: MemorySystem::from_zip_reader(Cursor::new(bytes))?,
            relationships: BTreeMap::new(),
        };
        for path in package.files()? {
            match file_kind(&path) {
                FileKind::ContentTypes => {
                    let part = PartName::new(&path)?;
                    let document = package.tree_at(&part)?;
                    package.content_types = ContentTypes::parse(&document)
                        .map_err(|reason| PackageError::Read { part, reason })?;
                }
                FileKind::Part => {}
                FileKind::Relationships(source) => {
                    let part = PartName::new(&path)?;
                    let items = relationship::parse(&package.tree_at(&part)?)
                        .map_err(|reason| PackageError::Read { part, reason })?;
                    package.relationships.insert(
                        source_key(source.as_ref()),
                        RelationshipSet { items, source },
                    );
                }
            }
        }
        Ok(package)
    }

    /// The image a part holds.
    ///
    /// # Errors
    ///
    /// [`PackageError::ContentType`] when the part's content type does not start with `image/`;
    /// the errors of [`part_bytes`](Self::part_bytes).
    pub fn image(&self, name: &PartName) -> Result<Image, PackageError> {
        let content_type = self.content_type(name);
        let format = content_type
            .and_then(ImageFormat::from_content_type)
            .ok_or_else(|| PackageError::ContentType {
                expected: "image/*",
                found: content_type.map(str::to_owned),
                part: name.clone(),
            })?;
        Ok(Image::with_format(format, self.part_bytes(name)?))
    }

    /// The generic tree of any XML part, with markup compatibility applied as
    /// `options` says.
    ///
    /// # Errors
    ///
    /// [`PackageError::MissingPart`] when the package has no such part;
    /// [`PackageError::Xml`] for a part that is not well-formed or whose
    /// markup compatibility cannot be processed.
    pub fn load_tree(
        &self,
        name: &PartName,
        options: OpenOptions,
    ) -> Result<Document, PackageError> {
        let mut document = self.tree_at(name)?;
        document
            .process_markup_compatibility(options.markup_compatibility)
            .map_err(|error| PackageError::Xml {
                error,
                part: name.clone(),
            })?;
        Ok(document)
    }

    /// The file holding `path`, compared case-folded, as part names are.
    fn locate(&self, path: &str) -> Result<Option<PathBuf>, PackageError> {
        if self.fs.is_file(Path::new(path))? {
            return Ok(Some(PathBuf::from(path)));
        }
        let folded = path.to_ascii_lowercase();
        Ok(self
            .files()?
            .into_iter()
            .find(|file| file.to_ascii_lowercase() == folded)
            .map(PathBuf::from))
    }

    /// The main part, by the `officeDocument` relationship in either class's spelling.
    ///
    /// # Errors
    ///
    /// [`PackageError::MissingPart`] naming the package's relationship part when no relationship of
    /// that type names a part of the package; the errors of [`Package::related`].
    pub fn main_part(&self) -> Result<MainPart, PackageError> {
        let relationships = PartName::new(PACKAGE_RELATIONSHIPS_PART)?;
        let found = self.related(None)?.into_iter().find_map(|related| {
            let class = class_of_office_document(&related.type_uri)?;
            match related.target {
                Target::Part(name) => Some(MainPart {
                    class,
                    content_type: related.content_type,
                    name,
                }),
                Target::External(_) => None,
            }
        });
        found.ok_or(PackageError::MissingPart(relationships))
    }

    /// A package with no parts: its `[Content_Types].xml` holds no entry.
    #[must_use]
    pub fn new() -> Self {
        let package = Self {
            content_types: ContentTypes::default(),
            fs: MemorySystem::new(),
            relationships: BTreeMap::new(),
        };
        // A new memory filesystem has its root folder and no other user, so
        // writing a file at the root cannot fail.
        let _: Result<(), PackageError> = package.write_content_types();
        package
    }

    /// The id a new relationship from `source` takes.
    fn next_relationship_id(&self, source: Option<&PartName>) -> String {
        self.relationships.get(&source_key(source)).map_or_else(
            || RelationshipSet::default().next_id(),
            RelationshipSet::next_id,
        )
    }

    /// Read the file at `path` through `system` and unpack it into memory.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the file cannot be read, and every error of
    /// [`from_bytes`](Self::from_bytes).
    pub fn open<S>(system: &S, path: &Path) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let mut bytes = Vec::new();
        system.open(path)?.read_to_end(&mut bytes)?;
        Self::from_bytes(bytes)
    }

    /// The bytes of the part called `name`, compared case-folded.
    ///
    /// # Errors
    ///
    /// [`PackageError::MissingPart`] when the package has no such part;
    /// [`PackageError::Io`] when it cannot be read.
    pub fn part_bytes(&self, name: &PartName) -> Result<Vec<u8>, PackageError> {
        let path = self
            .locate(name.as_str())?
            .ok_or_else(|| PackageError::MissingPart(name.clone()))?;
        let mut bytes = Vec::new();
        self.fs.open(&path)?.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    /// Every part but `[Content_Types].xml` and the relationship parts, which
    /// the package keeps parsed, in case-folded order.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the memory filesystem cannot be walked;
    /// [`PackageError::PartName`] for a file whose path is no valid part name.
    pub fn part_names(&self) -> Result<Vec<PartName>, PackageError> {
        let mut names = self
            .files()?
            .iter()
            .filter(|path| matches!(file_kind(path), FileKind::Part))
            .map(|path| PartName::new(path))
            .collect::<Result<Vec<_>, _>>()?;
        names.sort_by_cached_key(PartName::folded);
        Ok(names)
    }

    /// A reader of the part's events, from its tree written again with the prefixes it uses.
    ///
    /// # Errors
    ///
    /// The errors of [`Package::load_tree`].
    pub fn part_reader(
        &self,
        name: &PartName,
    ) -> Result<PartReader<Cursor<Vec<u8>>>, PackageError> {
        let document = self.load_tree(name, OpenOptions::default())?;
        Ok(PartReader::new(Cursor::new(
            document.to_xml(WriteOptions::default()),
        )))
    }

    /// Relate `source` (`None`: the package) to a part already in the
    /// package; returns the id, one above the highest `rIdN` the source has.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the memory filesystem refuses a write.
    pub fn relate(
        &mut self,
        source: Option<&PartName>,
        target: &PartName,
        relationship_type: &str,
    ) -> Result<String, PackageError> {
        let id = self.next_relationship_id(source);
        self.add_relationship(
            source,
            &id,
            &Target::Part(target.clone()),
            relationship_type,
        )?;
        Ok(id)
    }

    /// Relate `source` (`None`: the package) to an external target, such as a
    /// hyperlink's URL; returns the id, one above the highest `rIdN` the
    /// source has.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the memory filesystem refuses a write.
    pub fn relate_external(
        &mut self,
        source: Option<&PartName>,
        target: &str,
        relationship_type: &str,
    ) -> Result<String, PackageError> {
        let id = self.next_relationship_id(source);
        self.add_relationship(
            source,
            &id,
            &Target::External(target.to_owned()),
            relationship_type,
        )?;
        Ok(id)
    }

    /// The parts a part relates to (`None`: the package's own relationships),
    /// in the order the relationship part has them, each target resolved
    /// against the source's folder.
    ///
    /// # Errors
    ///
    /// [`PackageError::PartName`] for an internal target that resolves to no
    /// valid part name.
    pub fn related(&self, source: Option<&PartName>) -> Result<Vec<RelatedPart>, PackageError> {
        self.relationships_of(source).into_iter().collect()
    }

    /// The parts a part relates to by one relationship type, in either
    /// class's spelling of it.
    ///
    /// # Errors
    ///
    /// The errors of [`related`](Self::related).
    pub fn related_by_type(
        &self,
        source: Option<&PartName>,
        relationship_type: &RelationshipType,
    ) -> Result<Vec<RelatedPart>, PackageError> {
        Ok(self
            .related(source)?
            .into_iter()
            .filter(|part| relationship_type.matches(&part.type_uri))
            .collect())
    }

    /// Every relationship part's entries as written, by source (`None`: the
    /// package).
    #[must_use]
    pub fn relationship_sets(&self) -> Vec<(Option<PartName>, Vec<Relationship>)> {
        self.relationships
            .values()
            .map(|set| (set.source.clone(), set.items.clone()))
            .collect()
    }

    /// Each relationship of a source (`None`: the package), in the order the
    /// relationship part has them, its target resolved against the source's
    /// folder, or the error that target gives.
    #[must_use]
    pub fn relationships_of(
        &self,
        source: Option<&PartName>,
    ) -> Vec<Result<RelatedPart, PackageError>> {
        let Some(set) = self.relationships.get(&source_key(source)) else {
            return Vec::new();
        };
        set.items
            .iter()
            .map(|item| {
                let target = item.resolved(set.directory())?;
                let content_type = match &target {
                    Target::External(_) => None,
                    Target::Part(part) => self.content_types.of(part).map(str::to_owned),
                };
                Ok(RelatedPart {
                    content_type,
                    id: item.id.clone(),
                    target,
                    type_uri: item.type_uri.clone(),
                })
            })
            .collect()
    }

    /// Remove a part: its file, its `Override`, its own relationship part,
    /// and every relationship in the package that targets it. Returns those
    /// relationships, by source and id, so the caller can drop the markup
    /// that names them; parts that only the removed part related to stay.
    ///
    /// # Errors
    ///
    /// [`PackageError::MissingPart`] when the package has no such part;
    /// [`PackageError::Io`] when the memory filesystem refuses a change.
    pub fn remove_part(
        &mut self,
        name: &PartName,
    ) -> Result<Vec<(Option<PartName>, String)>, PackageError> {
        let path = self
            .locate(name.as_str())?
            .ok_or_else(|| PackageError::MissingPart(name.clone()))?;
        self.fs.remove_file(&path)?;
        if self.content_types.remove(name) {
            self.write_content_types()?;
        }
        let folded = name.folded();
        self.relationships.remove(&folded);
        if let Some(own) = self.locate(&name.relationships_part())? {
            self.fs.remove_file(&own)?;
        }
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for (key, set) in &mut self.relationships {
            let directory = set.directory().to_owned();
            let source = set.source.clone();
            let before = set.items.len();
            set.items.retain(|item| {
                let hit = targets(item, &directory, &folded);
                if hit {
                    removed.push((source.clone(), item.id.clone()));
                }
                !hit
            });
            if set.items.len() != before {
                changed.push(key.clone());
            }
        }
        for key in changed {
            self.write_relationships(&key)?;
        }
        Ok(removed)
    }

    /// Remove one relationship of `source` (`None`: the package). When it was
    /// internal and no relationship left in the package targets its part,
    /// that part is removed as [`remove_part`](Self::remove_part) removes it,
    /// and returned.
    ///
    /// # Errors
    ///
    /// [`PackageError::MissingRelationship`] when the source has no such id;
    /// [`PackageError::PartName`] for a target that resolves to no valid part
    /// name; the errors of [`remove_part`](Self::remove_part).
    pub fn remove_relationship(
        &mut self,
        source: Option<&PartName>,
        id: &str,
    ) -> Result<Option<PartName>, PackageError> {
        let key = source_key(source);
        let missing = || PackageError::MissingRelationship {
            id: id.to_owned(),
            part: source.cloned(),
        };
        let set = self.relationships.get_mut(&key).ok_or_else(missing)?;
        let index = set
            .items
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(missing)?;
        let target = set.items[index].resolved(set.directory())?;
        set.items.remove(index);
        self.write_relationships(&key)?;
        let Target::Part(part) = target else {
            return Ok(None);
        };
        let folded = part.folded();
        let still_targeted = self.relationships.values().any(|other| {
            other
                .items
                .iter()
                .any(|item| targets(item, other.directory(), &folded))
        });
        if still_targeted || self.locate(part.as_str())?.is_none() {
            return Ok(None);
        }
        self.remove_part(&part)?;
        Ok(Some(part))
    }

    /// Zip the memory filesystem and write it to `path` through `system`.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the archive cannot be built or written.
    pub fn save<S>(&self, system: &S, path: &Path) -> Result<(), PackageError>
    where
        S: System + ?Sized,
    {
        system.write(path, &self.to_bytes()?, WriteMode::Replace)?;
        Ok(())
    }

    /// Put an image in place of the one a part holds. Of the same format, the bytes are replaced.
    /// Of another, the image moves to the old stem with the first free number and the new
    /// extension in the same folder, every relationship to the old part targets the new one under
    /// the same id, the new content type is registered, and the old part and its override go.
    /// Returns the image's name.
    ///
    /// # Errors
    ///
    /// The errors of [`image`](Self::image);
    /// [`PackageError::Io`] when the memory filesystem refuses a change.
    pub fn set_image(&mut self, name: &PartName, image: &Image) -> Result<PartName, PackageError> {
        if self.image(name)?.format() == image.format() {
            self.set_part_bytes(name, image.content_type(), image.bytes())?;
            return Ok(name.clone());
        }
        let file_name = name
            .as_str()
            .rsplit_once('/')
            .map_or(name.as_str(), |(_, file_name)| file_name);
        let stem = file_name
            .rsplit_once('.')
            .map_or(file_name, |(stem, _)| stem)
            .trim_end_matches(|c: char| c.is_ascii_digit());
        let new_name = self.free_part_name(&format!(
            "{}/{stem}{{}}.{}",
            name.directory(),
            image.format().extension()
        ))?;
        self.set_part_bytes(&new_name, image.content_type(), image.bytes())?;
        let folded = name.folded();
        let mut changed = Vec::new();
        for (key, set) in &mut self.relationships {
            let directory = set.directory().to_owned();
            let mut any = false;
            for item in &mut set.items {
                if targets(item, &directory, &folded) {
                    item.target = relationship::relative(&directory, &new_name);
                    any = true;
                }
            }
            if any {
                changed.push(key.clone());
            }
        }
        for key in changed {
            self.write_relationships(&key)?;
        }
        self.remove_part(name)?;
        Ok(new_name)
    }

    /// Replace a part's bytes, or add the part; its content type is
    /// registered either way, and its folders are created first.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the memory filesystem refuses the write.
    pub fn set_part_bytes(
        &mut self,
        name: &PartName,
        content_type: &str,
        bytes: &[u8],
    ) -> Result<(), PackageError> {
        let path = self
            .locate(name.as_str())?
            .unwrap_or_else(|| name.path().to_path_buf());
        self.write_file(&path, bytes)?;
        if self.content_types.of(name) != Some(content_type) {
            self.content_types.register(name, content_type);
            self.write_content_types()?;
        }
        Ok(())
    }

    /// Write every relationship type the standard defines in `class`'s
    /// spelling. A type no standard defines, such as an extension's, is left
    /// as written.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when a relationship part cannot be written.
    pub fn set_relationship_class(&mut self, class: ConformanceClass) -> Result<(), PackageError> {
        let mut changed = Vec::new();
        for (key, set) in &mut self.relationships {
            let mut any = false;
            for item in &mut set.items {
                if let Some(known) = RelationshipType::find(&item.type_uri) {
                    let spelled = known.in_class(class);
                    if spelled != item.type_uri {
                        spelled.clone_into(&mut item.type_uri);
                        any = true;
                    }
                }
            }
            if any {
                changed.push(key.clone());
            }
        }
        for key in changed {
            self.write_relationships(&key)?;
        }
        Ok(())
    }

    /// Write a generic tree into a part, laid out as `options` says.
    ///
    /// # Errors
    ///
    /// The errors of [`set_part_bytes`](Self::set_part_bytes).
    pub fn store_tree(
        &mut self,
        name: &PartName,
        content_type: &str,
        document: &Document,
        options: WriteOptions,
    ) -> Result<(), PackageError> {
        self.set_part_bytes(name, content_type, &document.to_xml(options))
    }

    /// The folded path of every file, for the naming rule.
    fn taken(&self) -> Result<BTreeSet<String>, PackageError> {
        Ok(self
            .files()?
            .iter()
            .map(|path| path.to_ascii_lowercase())
            .collect())
    }

    /// Zip the memory filesystem.
    ///
    /// # Errors
    ///
    /// [`PackageError::Io`] when the archive cannot be built.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        Ok(self.fs.to_zip_vec(Path::new("/"))?)
    }

    /// Parse the part called `name`, its markup compatibility left as written.
    ///
    /// # Errors
    ///
    /// The errors of [`part_bytes`](Self::part_bytes); [`PackageError::Xml`] for a part that is
    /// not well-formed.
    pub fn tree_at(&self, name: &PartName) -> Result<Document, PackageError> {
        parse(&self.part_bytes(name)?).map_err(|error| PackageError::Xml {
            error,
            part: name.clone(),
        })
    }

    /// Write `[Content_Types].xml` from the parsed entries.
    fn write_content_types(&self) -> Result<(), PackageError> {
        let path = self
            .locate(CONTENT_TYPES_PART)?
            .unwrap_or_else(|| PathBuf::from(CONTENT_TYPES_PART));
        let bytes = self
            .content_types
            .to_document()
            .to_xml(WriteOptions::default());
        self.write_file(&path, &bytes)
    }

    /// Write a file, creating its folders first: the memory filesystem
    /// writes a file only into a folder that exists.
    fn write_file(&self, path: &Path, bytes: &[u8]) -> Result<(), PackageError> {
        if let Some(parent) = path.parent() {
            self.fs.create_dir_all(parent)?;
        }
        self.fs.write(path, bytes, WriteMode::Replace)?;
        Ok(())
    }

    /// Write the relationship part of the set filed under `key`, adding the
    /// `rels` `Default` when missing; a set left empty goes, with its file.
    fn write_relationships(&mut self, key: &str) -> Result<(), PackageError> {
        let Some(set) = self.relationships.get(key) else {
            return Ok(());
        };
        let name = set.source.as_ref().map_or_else(
            || PACKAGE_RELATIONSHIPS_PART.to_owned(),
            PartName::relationships_part,
        );
        let existing = self.locate(&name)?;
        if set.items.is_empty() {
            self.relationships.remove(key);
            if let Some(path) = existing {
                self.fs.remove_file(&path)?;
            }
            return Ok(());
        }
        let bytes = relationship::to_document(&set.items).to_xml(WriteOptions::default());
        self.write_file(&existing.unwrap_or_else(|| PathBuf::from(name)), &bytes)?;
        if self
            .content_types
            .ensure_default("rels", RELATIONSHIPS_CONTENT_TYPE)
        {
            self.write_content_types()?;
        }
        Ok(())
    }
}

impl Default for Package {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Package {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Package")
            .field("content_types", &self.content_types)
            .field("relationships", &self.relationships)
            .finish_non_exhaustive()
    }
}

impl RelationshipSet {
    /// The source's folder, against which internal targets resolve.
    fn directory(&self) -> &str {
        self.source.as_ref().map_or("", PartName::directory)
    }

    /// The id a new relationship takes.
    fn next_id(&self) -> String {
        relationship::next_id(&self.items)
    }
}

/// What the file at `path` is to the package: `[Content_Types].xml`, the
/// relationship part `<folder>/_rels/<file>.rels` of the part
/// `<folder>/<file>` (or `/_rels/.rels`, the package's), or a part.
fn file_kind(path: &str) -> FileKind {
    let folded = path.to_ascii_lowercase();
    if folded == CONTENT_TYPES_PART.to_ascii_lowercase() {
        return FileKind::ContentTypes;
    }
    let Some(index) = folded.rfind("/_rels/") else {
        return FileKind::Part;
    };
    let directory = &path[..index];
    let file = &path[index + "/_rels/".len()..];
    let Some((stem, extension)) = file.rsplit_once('.') else {
        return FileKind::Part;
    };
    if file.contains('/') || !extension.eq_ignore_ascii_case("rels") {
        return FileKind::Part;
    }
    if stem.is_empty() {
        return if directory.is_empty() {
            FileKind::Relationships(None)
        } else {
            FileKind::Part
        };
    }
    PartName::new(&format!("{directory}/{stem}")).map_or(FileKind::Part, |source| {
        FileKind::Relationships(Some(source))
    })
}

/// The item-properties part of a custom XML part:
/// `<ds:datastoreItem ds:itemID="..."><ds:schemaRefs><ds:schemaRef ds:uri="..."/>...`.
fn item_properties(item_id: &str, schema_references: &[&str]) -> Vec<u8> {
    let mut references = Element::new(QName::prefixed("ds", "schemaRefs", CUSTOM_XML_NAMESPACE));
    for uri in schema_references {
        let mut reference = Element::new(QName::prefixed("ds", "schemaRef", CUSTOM_XML_NAMESPACE));
        reference.attributes.push(Attribute::new(
            QName::prefixed("ds", "uri", CUSTOM_XML_NAMESPACE),
            (*uri).to_owned(),
        ));
        references.children.push(Node::Element(reference));
    }
    let mut root = Element::new(QName::prefixed("ds", "datastoreItem", CUSTOM_XML_NAMESPACE));
    root.namespace_declarations.push(NamespaceDeclaration::new(
        "ds".to_owned(),
        CUSTOM_XML_NAMESPACE.to_owned(),
    ));
    root.attributes.push(Attribute::new(
        QName::prefixed("ds", "itemID", CUSTOM_XML_NAMESPACE),
        item_id.to_owned(),
    ));
    root.children.push(Node::Element(references));
    Document::new(CUSTOM_XML_PROPERTIES_DECLARATION, root).to_xml(WriteOptions::default())
}

/// The key a source's relationships are filed under: its folded name, `""`
/// for the package.
fn source_key(source: Option<&PartName>) -> String {
    source.map_or_else(String::new, PartName::folded)
}

/// Whether `item`, from a source in `directory`, targets the part whose
/// folded name is `folded`.
fn targets(item: &Relationship, directory: &str, folded: &str) -> bool {
    item.target_mode == TargetMode::Internal
        && relationship::resolve(directory, &item.target).is_ok_and(|part| part.folded() == folded)
}
