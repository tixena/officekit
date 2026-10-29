//! The part modules: one module per part type, `officekit::parts::<part>`, the
//! relationship types of `officekit_xml::packaging`'s `relationship_type.rs`, and `kinds.rs`.
//!
//! Each part module holds its part's content types, its relationship type in
//! both classes' spelling, and the load and store of its root. Part 1's part
//! definitions give the Strict relationship type, the content types, and the
//! root element of each part; Part 4's restatements the Transitional
//! relationship type; the typed tree the root's struct. SDK 3.5.1's part data
//! gives the three parts Part 1 leaves to Part 2, and the parts Microsoft's
//! extensions define, whose relationship type is one URI in both classes.

mod render;

use alloc::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::{System, WriteMode};

use super::markup::structs::Catalog;
use super::markup::{self, Data, NodeKey, Source, names};
use super::namespaces::{self, Row};
use super::validation::{self, Emitted};
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::{Name, SchemaSet, TypeRef};
use crate::generate_schema::sdk_data::{DataPart, DataVersion, SdkData};
use crate::generate_schema::standard_text::{
    self, PartDefinition, RelationshipSection, SummaryRow, TransitionalPart,
};

#[cfg(test)]
mod tests;

/// The hand-written table of document kinds, from the workspace root.
pub const DOCUMENT_KINDS_PATH: &str = "crates/xtask/data/document_kinds.toml";

/// The source folder of the part modules, from the workspace root; the index
/// `lib.rs` sits in it.
pub const PARTS_PATH: &str = "crates/officekit/src/parts";

/// The generated impl of `RelationshipType`, from the workspace root.
pub const RELATIONSHIP_TYPE_PATH: &str = "crates/officekit-xml/src/packaging/relationship_type.rs";

/// The file types, by the Part 1 section their markup language's parts are
/// defined in.
const FILE_TYPES: [(&str, &str); 3] = [
    ("11.3.", "wordprocessing"),
    ("12.3.", "spreadsheet"),
    ("13.3.", "presentation"),
];

/// The markup languages of Part 1's part definitions, by section.
const LANGUAGES: [(&str, &str); 5] = [
    ("11.3.", "WordprocessingML"),
    ("12.3.", "SpreadsheetML"),
    ("13.3.", "PresentationML"),
    ("14.2.", "DrawingML"),
    ("15.2.", "shared"),
];

/// The Transitional `officeDocument` relationship type, which names a main part.
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";

/// The family of Strict relationship types.
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/";

/// Where a part type is defined.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Definition {
    Extension(ExtensionPart),
    Part1(Part1Definition),
}

/// A part of Microsoft's extensions, as SDK 3.5.1's data gives it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ExtensionPart {
    /// The name SDK 3.5.1's data gives the part.
    pub name: String,
    /// The Office version that brought the part.
    pub version: Option<DataVersion>,
}

/// Every part type joined from the sources, the relationship types'
/// constants, and the document kinds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Joined {
    pub constants: Vec<RelationshipConstant>,
    pub kinds: Vec<Kind>,
    pub notes: Vec<String>,
    /// Part 1's definitions in the standard's order, then the extension parts
    /// by module name.
    pub parts: Vec<PartType>,
}

/// A document kind of a file type: its variant and its main part's content
/// type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Kind {
    pub content_type: String,
    /// `wordprocessing`, `spreadsheet`, or `presentation`.
    pub file: String,
    pub name: String,
}

/// What `roots` looks a root up in.
struct Lookup<'lookup> {
    data: &'lookup Data<'lookup>,
    /// Each struct of the catalog by its key.
    keys: BTreeMap<&'lookup NodeKey, usize>,
    rows: &'lookup [Row],
    transitional: &'lookup SchemaSet,
}

/// A definition of Part 1.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Part1Definition {
    /// Whether Part 1 leaves the part's fields to Part 2.
    pub part_2: bool,
    /// The section of its Part 4 restatement.
    pub part_4: Option<String>,
    pub section: String,
    pub title: String,
}

/// One part type, its sources joined.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PartType {
    /// Every content type, in the order its sources list them.
    pub content_types: Vec<String>,
    pub definition: Definition,
    pub module: String,
    /// The root element as Part 1 names it: the URIs that may name its
    /// namespace, and its local name.
    pub root_element: Option<(Vec<String>, String)>,
    /// The element type SDK 3.5.1's data gives the root, `w:CT_HdrFtr/w:hdr`.
    pub root_type: Option<String>,
    pub strict: String,
    pub transitional: String,
}

/// A relationship type and its constant.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RelationshipConstant {
    /// The modules of the parts of this type, in module order.
    pub modules: Vec<String>,
    pub name: String,
    /// What the standard states for a type that belongs to no part module:
    /// `Part 1, 15.3 "Hyperlinks", and Part 4, 13.3`.
    pub source: Option<String>,
    pub strict: String,
    pub transitional: String,
}

/// A part module's root: the struct of the typed tree, and whether an earlier
/// module already implements `PartRoot` for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Root {
    pub node: usize,
    pub shared: bool,
}

/// What `generate-schema` wrote of the part modules.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    /// Modules of the parts Microsoft's extensions define.
    pub extensions: usize,
    /// The kinds of each file type, by its name.
    pub kinds: Vec<(String, usize)>,
    /// One line per module: its name, its definition, its root, and its
    /// relationship type's constant.
    pub list: Vec<String>,
    /// Modules that load bytes.
    pub no_root: usize,
    /// What a source states that the generator did not follow, and why.
    pub notes: Vec<String>,
    /// Modules of Part 1's definitions, by markup language.
    pub part_1: Vec<(&'static str, usize)>,
    pub relationship_types: usize,
    /// Modules whose root's `PartRoot` another module implements.
    pub shared_roots: usize,
    /// Modules with a typed root.
    pub typed_roots: usize,
}

impl PartType {
    /// The Part 1 definition, when the part has one.
    #[must_use]
    pub const fn part_1(&self) -> Option<&Part1Definition> {
        match &self.definition {
            Definition::Part1(definition) => Some(definition),
            Definition::Extension(_) => None,
        }
    }

    /// Whether Part 1 states the part's fields itself.
    #[must_use]
    pub fn stated_in_part_1(&self) -> bool {
        self.part_1().is_some_and(|definition| !definition.part_2)
    }
}

/// The constant name a relationship type's URI gives.
///
/// The words of its path after `relationships/`, a leading `metadata` segment
/// left out and each `props` spelled `properties`, in `SCREAMING_SNAKE_CASE`.
///
/// # Errors
///
/// A URI without `/relationships/`.
pub fn constant_name(uri: &str) -> Result<String> {
    let (_, after) = uri
        .split_once("/relationships/")
        .with_context(|| format!("the relationship type {uri} has no `/relationships/`"))?;
    let words: Vec<String> = after
        .strip_prefix("metadata/")
        .unwrap_or(after)
        .split('/')
        .flat_map(|segment| {
            names::snake(segment)
                .split('_')
                .map(|word| {
                    if word == "props" {
                        "properties".to_owned()
                    } else {
                        word.to_owned()
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    Ok(words.join("_").to_ascii_uppercase())
}

/// The relationship types' constants, one per Transitional URI.
///
/// Its Strict spelling is the one Part 1 states; where two parts of one type
/// state two, the one in Strict's own family is kept, and a note says so.
/// Where two types give one name, the type of a Part 1 definition keeps it,
/// and another takes its first module's name.
///
/// # Errors
///
/// A URI `constant_name` refuses, or two names no rule tells apart.
pub fn constants(
    parts: &mut [PartType],
    notes: &mut Vec<String>,
) -> Result<Vec<RelationshipConstant>> {
    let mut by_type: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, part) in parts.iter().enumerate() {
        by_type
            .entry(part.transitional.clone())
            .or_default()
            .push(index);
    }
    let mut found = Vec::new();
    for (transitional, members) in &by_type {
        let strict = strict_spelling(parts, transitional, members, notes)?;
        for index in members {
            parts[*index].strict.clone_from(&strict);
        }
        found.push(RelationshipConstant {
            modules: members
                .iter()
                .map(|index| parts[*index].module.clone())
                .collect(),
            name: constant_name(transitional)?,
            source: None,
            strict,
            transitional: transitional.clone(),
        });
    }
    rename_collisions(parts, &mut found)?;
    found.sort_by(|left, right| left.name.cmp(&right.name));
    check_distinct(&found)?;
    Ok(found)
}

/// Whether no two constants share a name.
///
/// # Errors
///
/// Two types that give one name.
fn check_distinct(found: &[RelationshipConstant]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for constant in found {
        if !seen.insert(constant.name.as_str()) {
            bail!(
                "two relationship types give the constant name {}",
                constant.name
            );
        }
    }
    Ok(())
}

/// Add the types of the sections of Part 1 that define no part, each with
/// Part 4's restatement; a section whose type a part module has is not added
/// twice.
///
/// # Errors
///
/// A section with no Part 4 restatement, or two types that give one name.
pub fn add_sections(
    joined: &mut Joined,
    sections: &[RelationshipSection],
    restatements: &[TransitionalPart],
) -> Result<()> {
    for section in sections {
        let restated = restatements
            .iter()
            .find(|restated| restated.part_1_section == section.section)
            .with_context(|| {
                format!(
                    "Part 1, {} \"{}\" has no restatement in Part 4",
                    section.section, section.title
                )
            })?;
        let transitional = restated
            .source_relationship
            .clone()
            .with_context(|| format!("Part 4, {} states no relationship type", restated.section))?;
        if joined
            .constants
            .iter()
            .any(|constant| constant.transitional == transitional)
        {
            continue;
        }
        joined.constants.push(RelationshipConstant {
            modules: Vec::new(),
            name: constant_name(&transitional)?,
            source: Some(format!(
                "Part 1, {} \"{}\", and Part 4, {}",
                section.section, section.title, restated.section
            )),
            strict: section.source_relationship.clone(),
            transitional,
        });
    }
    joined
        .constants
        .sort_by(|left, right| left.name.cmp(&right.name));
    check_distinct(&joined.constants)
}

/// Whether a Part 1 definition defines a part of SDK 3.5.1's data.
///
/// They share the Transitional relationship type, and the data's content type
/// is one of the definition's when both name one; when the data names none,
/// the root elements are the same where both name one.
fn defines(part: &PartType, found: &DataPart) -> bool {
    if !part.stated_in_part_1()
        || found.relationship_type.as_deref() != Some(part.transitional.as_str())
    {
        return false;
    }
    match (&found.content_type, &part.root_element, &found.root_element) {
        (Some(content_type), _, _) => {
            part.content_types.is_empty() || part.content_types.contains(content_type)
        }
        (None, Some((_, local)), Some(element)) => local == element,
        (None, _, _) => true,
    }
}

/// A part type as a note names it.
fn describe(part: &PartType) -> String {
    match &part.definition {
        Definition::Part1(definition) => {
            format!("Part 1, {} {}", definition.section, definition.title)
        }
        Definition::Extension(_) => format!("the extension part {}", part.module),
    }
}

/// The element type SDK 3.5.1's data gives a part's root: its `Root` type, or
/// its base's in the table of part roots, with its root element.
fn data_root(found: &DataPart, part_roots: &BTreeMap<String, String>) -> Option<String> {
    let element = found.root_element.as_deref()?;
    found.root.as_ref().map_or_else(
        || {
            found
                .base
                .as_deref()
                .and_then(|base| part_roots.get(base))
                .cloned()
        },
        |root| {
            let prefix = root.split_once(':').map_or("", |(prefix, _)| prefix);
            Some(format!("{root}/{prefix}:{element}"))
        },
    )
}

/// Give a module name that several Part 1 definitions share its file type's
/// name in front: `wordprocessing_comments`, `spreadsheet_comments`.
///
/// # Errors
///
/// Two modules of one name the rule does not tell apart, or a name that is no
/// Rust identifier.
fn disambiguate(parts: &mut [PartType]) -> Result<()> {
    let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, part) in parts.iter().enumerate() {
        by_name.entry(part.module.clone()).or_default().push(index);
    }
    for (name, members) in by_name.into_iter().filter(|(_, members)| members.len() > 1) {
        for index in members {
            let part = &mut parts[index];
            let file = part
                .part_1()
                .and_then(|definition| file_type(&definition.section))
                .with_context(|| format!("two part modules are named {name}"))?;
            part.module = format!("{file}_{name}");
        }
    }
    let mut seen = BTreeSet::new();
    for part in parts.iter() {
        names::identifier(&part.module)?;
        if !seen.insert(part.module.as_str()) {
            bail!("two part modules are named {}", part.module);
        }
    }
    Ok(())
}

/// Read the inputs, join the part data, and write the part modules, the
/// relationship types, and the document kinds.
///
/// # Errors
///
/// An input that cannot be read, sources that cannot be joined, or an output
/// that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs, emitted: &Emitted, catalog: &Catalog) -> Result<Summary>
where
    S: System + ?Sized,
{
    let kinds_text = read(system, Path::new(DOCUMENT_KINDS_PATH))?;
    let part_roots_text = read(system, Path::new(validation::PART_ROOTS_PATH))?;
    let sdk_corrections_text = read(system, Path::new(validation::SDK_CORRECTIONS_PATH))?;
    let transitional = SchemaSet::read(system, &inputs.transitional_schemas)?;
    let part_1_text = read(system, &inputs.part1_text)?;
    let part_4_text = read(system, &inputs.part4_text)?;
    let mut joined = join(
        &standard_text::part_definitions(&part_1_text),
        &standard_text::transitional_parts(&part_4_text),
        &standard_text::part_summary(&part_1_text),
        &emitted.data,
        &kinds(&kinds_text)?,
        &validation::part_roots(&part_roots_text)?,
    )?;
    add_sections(
        &mut joined,
        &standard_text::relationship_sections(&part_1_text),
        &standard_text::transitional_relationships(&part_4_text),
    )?;
    let data = Data::new(&emitted.data, &transitional);
    let roots = roots(
        &joined.parts,
        &Lookup {
            data: &data,
            keys: catalog
                .nodes
                .iter()
                .enumerate()
                .map(|(index, node)| (&node.key, index))
                .collect(),
            rows: &emitted.namespaces,
            transitional: &transitional,
        },
        &mut joined.notes,
    );
    let header = header(
        system,
        inputs,
        &kinds_text,
        &part_roots_text,
        &sdk_corrections_text,
    )?;
    let folder = Path::new(PARTS_PATH);
    let tests = system.is_file(&folder.join("tests.rs")).unwrap_or(false);
    let files = render::files(
        &header,
        &joined,
        &roots,
        catalog,
        &emitted.namespaces,
        tests,
    )?;
    system
        .create_dir_all(folder)
        .with_context(|| format!("cannot create {}", folder.display()))?;
    for (path, text) in files {
        system
            .write(&path, text.as_bytes(), WriteMode::Replace)
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(summary(&joined, &roots))
}

/// The extension parts: every part of SDK 3.5.1's data that no definition of
/// Part 1 defines and that is not one of Part 2's, by module name.
///
/// # Errors
///
/// A part the data gives no relationship type.
fn extensions(
    parts: &[PartType],
    data_parts: &[&DataPart],
    claimed: &BTreeSet<&str>,
    part_roots: &BTreeMap<String, String>,
) -> Result<Vec<PartType>> {
    let mut found = Vec::new();
    for data_part in data_parts {
        if claimed.contains(data_part.name.as_str())
            || parts.iter().any(|part| defines(part, data_part))
        {
            continue;
        }
        let relationship = data_part.relationship_type.clone().with_context(|| {
            format!(
                "the part data of {} gives no relationship type",
                data_part.name
            )
        })?;
        let name = data_part
            .name
            .strip_suffix("Part")
            .unwrap_or(&data_part.name);
        found.push(PartType {
            content_types: data_part.content_type.iter().cloned().collect(),
            definition: Definition::Extension(ExtensionPart {
                name: data_part.name.clone(),
                version: data_part.version,
            }),
            module: names::snake_words(&names::name_words(name)),
            root_element: None,
            root_type: data_root(data_part, part_roots),
            strict: relationship.clone(),
            transitional: relationship,
        });
    }
    found.sort_by(|left, right| left.module.cmp(&right.module));
    Ok(found)
}

/// The file type of a Part 1 section, when its markup language is one.
fn file_type(section: &str) -> Option<&'static str> {
    FILE_TYPES
        .iter()
        .find(|(prefix, _)| section.starts_with(prefix))
        .map(|(_, file)| *file)
}

/// The global element `local` of the Transitional schemas: the only one of
/// that name, or the one in the namespace a hint names in either class.
fn global_element(
    transitional: &SchemaSet,
    rows: &[Row],
    hints: &[String],
    local: &str,
) -> Option<(Name, TypeRef)> {
    let candidates: Vec<_> = transitional
        .elements
        .values()
        .filter(|element| element.name.local == local)
        .collect();
    let chosen = if let [one] = candidates.as_slice() {
        *one
    } else {
        candidates.iter().copied().find(|element| {
            hints.iter().any(|hint| {
                rows.iter().any(|row| {
                    row.uri == element.name.namespace
                        && (row.uri == *hint || row.strict_uri.as_deref() == Some(hint.as_str()))
                })
            })
        })?
    };
    Some((chosen.name.clone(), chosen.type_ref.clone()))
}

/// The header of every file, naming its inputs.
fn header<S>(
    system: &S,
    inputs: &Inputs,
    kinds_text: &str,
    part_roots_text: &str,
    sdk_corrections_text: &str,
) -> Result<String>
where
    S: System + ?Sized,
{
    let enum_names_text = read(system, Path::new(markup::ENUM_NAMES_PATH))?;
    let overrides_text = read(system, Path::new(markup::NAME_OVERRIDES_PATH))?;
    let mut read_inputs: Vec<PathBuf> = inputs.strict_schemas.clone();
    read_inputs.extend(inputs.transitional_schemas.iter().cloned());
    read_inputs.push(inputs.part1_text.clone());
    read_inputs.push(inputs.part4_text.clone());
    read_inputs.push(inputs.sdk_data.join("namespaces.json"));
    for folder in ["schemas", "parts"] {
        let in_data = inputs.sdk_data.join(folder);
        let mut files: Vec<PathBuf> = system
            .read_dir(&in_data)
            .with_context(|| format!("cannot read {}", in_data.display()))?
            .into_iter()
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect();
        files.sort();
        read_inputs.extend(files);
    }
    namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &read_inputs,
        &[
            (Path::new(DOCUMENT_KINDS_PATH), kinds_text.as_bytes()),
            (
                Path::new(markup::ENUM_NAMES_PATH),
                enum_names_text.as_bytes(),
            ),
            (
                Path::new(markup::NAME_OVERRIDES_PATH),
                overrides_text.as_bytes(),
            ),
            (
                Path::new(validation::PART_ROOTS_PATH),
                part_roots_text.as_bytes(),
            ),
            (
                Path::new(validation::SDK_CORRECTIONS_PATH),
                sdk_corrections_text.as_bytes(),
            ),
        ],
    )
}

/// Join Part 1's definitions, Part 4's restatements, SDK 3.5.1's part data,
/// and the document kinds into one list of part types.
///
/// # Errors
///
/// A definition with no relationship type, a Part 2 part the data does not
/// give once, a main part content type with no kind, or two modules of one
/// name.
pub fn join(
    definitions: &[PartDefinition],
    restatements: &[TransitionalPart],
    summary: &[SummaryRow],
    data: &SdkData,
    kinds: &[Kind],
    part_roots: &BTreeMap<String, String>,
) -> Result<Joined> {
    let mut notes = Vec::new();
    let mut parts = Vec::new();
    let mut claimed: BTreeSet<&str> = BTreeSet::new();
    let data_parts: Vec<&DataPart> = data
        .parts
        .iter()
        .filter(|candidate| candidate.base.as_deref() != Some("OpenXmlPackage"))
        .collect();
    for definition in definitions {
        let restatement = restatements
            .iter()
            .find(|restated| restated.part_1_section == definition.section);
        let part = if definition.defined_in_part_2 {
            let data_part = part_2_part(&definition.title, &data_parts)?;
            claimed.insert(data_part.name.as_str());
            part_2_type(definition, restatement, data_part, part_roots, &mut notes)?
        } else {
            part_1_type(definition, restatement, summary, kinds, &mut notes)?
        };
        parts.push(part);
    }
    for part in &mut parts {
        let defined: Vec<&&DataPart> = data_parts
            .iter()
            .filter(|data_part| defines(part, data_part))
            .collect();
        if let [data_part] = defined.as_slice()
            && part.root_type.is_none()
        {
            part.root_type = data_root(data_part, part_roots);
        }
    }
    let extension_parts = extensions(&parts, &data_parts, &claimed, part_roots)?;
    parts.extend(extension_parts);
    disambiguate(&mut parts)?;
    let constants = constants(&mut parts, &mut notes)?;
    Ok(Joined {
        constants,
        kinds: kinds.to_vec(),
        notes,
        parts,
    })
}

/// The document kinds of `document_kinds.toml`.
///
/// # Errors
///
/// Text that is not TOML, or a row without its three strings.
pub fn kinds(text: &str) -> Result<Vec<Kind>> {
    let table: toml::Table = text
        .parse()
        .context("the table of document kinds is not TOML")?;
    let rows = table
        .get("kind")
        .and_then(toml::Value::as_array)
        .context("the table of document kinds has no `[[kind]]` rows")?;
    rows.iter()
        .map(|row| {
            let field = |key: &str| {
                row.get(key)
                    .and_then(toml::Value::as_str)
                    .map(str::to_owned)
                    .with_context(|| format!("a document kind without `{key}`"))
            };
            Ok(Kind {
                content_type: field("content_type")?,
                file: field("file")?,
                name: field("name")?,
            })
        })
        .collect()
}

/// The part type of a definition whose fields Part 1 states.
///
/// A main part's content types are Part 1's, then the other kinds of its file
/// type.
///
/// # Errors
///
/// A definition with no relationship type, or a main part content type with
/// no kind.
fn part_1_type(
    definition: &PartDefinition,
    restatement: Option<&TransitionalPart>,
    summary: &[SummaryRow],
    kinds: &[Kind],
    notes: &mut Vec<String>,
) -> Result<PartType> {
    let strict = definition
        .source_relationship
        .clone()
        .with_context(|| format!("Part 1, {} states no relationship type", definition.section))?;
    let transitional = restatement
        .and_then(|restated| restated.source_relationship.clone())
        .unwrap_or_else(|| strict.clone());
    let mut content_types = definition.content_types.clone();
    if transitional == OFFICE_DOCUMENT {
        let file = file_type(&definition.section).with_context(|| {
            format!(
                "Part 1, {} is a main part of no file type",
                definition.section
            )
        })?;
        let rows: Vec<&Kind> = kinds.iter().filter(|kind| kind.file == file).collect();
        if let Some(missing) = definition
            .content_types
            .iter()
            .find(|content_type| !rows.iter().any(|kind| kind.content_type == **content_type))
        {
            bail!(
                "the content type {missing} of Part 1, {} has no row in {DOCUMENT_KINDS_PATH}",
                definition.section
            );
        }
        for kind in rows {
            if !content_types.contains(&kind.content_type) {
                content_types.push(kind.content_type.clone());
            }
        }
    }
    let title = definition
        .title
        .strip_suffix(" Part")
        .unwrap_or(&definition.title);
    let from_summary = summary_root(summary, title);
    if let (Some(stated), Some(summarized)) = (&definition.root_element, from_summary)
        && stated != summarized
    {
        notes.push(format!(
            "Part 1, {} {}: the definition names the root element {stated}, the part summary {summarized}; \
             the definition is kept",
            definition.section, definition.title
        ));
    }
    let root_element = definition
        .root_element
        .clone()
        .or_else(|| from_summary.map(str::to_owned))
        .map(|local| {
            let hints = [
                definition.root_namespace.as_deref(),
                restatement.and_then(|restated| restated.root_namespace.as_deref()),
            ]
            .into_iter()
            .flatten()
            .map(str::to_owned)
            .collect();
            (hints, local)
        });
    Ok(PartType {
        content_types,
        definition: Definition::Part1(Part1Definition {
            part_2: false,
            part_4: restatement.map(|restated| restated.section.clone()),
            section: definition.section.clone(),
            title: definition.title.clone(),
        }),
        module: names::snake_words(&names::title_words(title)),
        root_element,
        root_type: None,
        strict,
        transitional,
    })
}

/// The root element the part summary gives a part: the one root the rows of that part name, when they name one.
fn summary_root<'rows>(summary: &'rows [SummaryRow], title: &str) -> Option<&'rows str> {
    let mut roots = summary
        .iter()
        .filter(|row| row.part == title)
        .filter_map(|row| row.root_element.as_deref());
    let first = roots.next()?;
    roots.all(|other| other == first).then_some(first)
}

/// The part SDK 3.5.1's data gives for a part Part 1 leaves to Part 2: the
/// one whose name's words, `Part` aside, are all words of the title.
///
/// # Errors
///
/// No such part, or more than one.
fn part_2_part<'data>(title: &str, data_parts: &[&'data DataPart]) -> Result<&'data DataPart> {
    let words: BTreeSet<String> = names::title_words(title)
        .iter()
        .map(|word| word.to_ascii_lowercase())
        .collect();
    let found: Vec<&DataPart> = data_parts
        .iter()
        .copied()
        .filter(|candidate| {
            let name = candidate
                .name
                .strip_suffix("Part")
                .unwrap_or(&candidate.name);
            names::name_words(name)
                .iter()
                .all(|word| words.contains(&word.to_ascii_lowercase()))
        })
        .collect();
    if let [one] = found.as_slice() {
        Ok(one)
    } else {
        bail!(
            "SDK 3.5.1's part data gives {} parts for {title}",
            found.len()
        )
    }
}

/// The part type of a definition whose fields Part 2 states: its content
/// type, relationship type, and root from SDK 3.5.1's part data.
///
/// # Errors
///
/// A part the data gives no relationship type.
fn part_2_type(
    definition: &PartDefinition,
    restatement: Option<&TransitionalPart>,
    data_part: &DataPart,
    part_roots: &BTreeMap<String, String>,
    notes: &mut Vec<String>,
) -> Result<PartType> {
    let title = definition
        .title
        .strip_suffix(" Part")
        .unwrap_or(&definition.title);
    let relationship = data_part
        .relationship_type
        .clone()
        .with_context(|| format!("the part data of {title} gives no relationship type"))?;
    if let Some(restated) = restatement
        .and_then(|restated| restated.source_relationship.as_deref())
        .filter(|restated| *restated != relationship)
    {
        notes.push(format!(
            "Part 4 restates {title} with the relationship type {restated}; Part 2's {relationship}, from SDK 3.5.1's part data, is kept"
        ));
    }
    Ok(PartType {
        content_types: data_part.content_type.iter().cloned().collect(),
        definition: Definition::Part1(Part1Definition {
            part_2: true,
            part_4: restatement.map(|restated| restated.section.clone()),
            section: definition.section.clone(),
            title: definition.title.clone(),
        }),
        module: names::snake_words(&names::title_words(title)),
        root_element: None,
        root_type: data_root(data_part, part_roots),
        strict: relationship.clone(),
        transitional: relationship,
    })
}

fn read<S>(system: &S, path: &Path) -> Result<String>
where
    S: System + ?Sized,
{
    system
        .read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))
}

/// Give a constant whose name another type's also gives its first module's
/// name, unless it is the one type of a Part 1 definition among them.
///
/// # Errors
///
/// Two types of Part 1 definitions that give one name.
fn rename_collisions(parts: &[PartType], found: &mut [RelationshipConstant]) -> Result<()> {
    let mut by_name: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, constant) in found.iter().enumerate() {
        by_name
            .entry(constant.name.clone())
            .or_default()
            .push(index);
    }
    for (name, members) in by_name.into_iter().filter(|(_, members)| members.len() > 1) {
        let standard: Vec<usize> = members
            .iter()
            .copied()
            .filter(|index| {
                found[*index].modules.iter().any(|module| {
                    parts
                        .iter()
                        .any(|part| part.module == *module && part.part_1().is_some())
                })
            })
            .collect();
        if standard.len() > 1 {
            bail!("two relationship types of Part 1 give the constant name {name}");
        }
        for index in members
            .into_iter()
            .filter(|index| !standard.contains(index))
        {
            let constant = &mut found[index];
            constant.name = constant
                .modules
                .first()
                .map(|module| module.to_ascii_uppercase())
                .unwrap_or_default();
        }
    }
    Ok(())
}

/// The struct of the typed tree that holds a part's root.
///
/// A Part 1 root is the global element of the name its definition gives, in
/// the namespace its definition or restatement names when there are several;
/// failing that, and for another part, the element type SDK 3.5.1's data gives.
/// A root no struct holds is noted, and its module loads bytes.
fn root_node(part: &PartType, lookup: &Lookup<'_>, notes: &mut Vec<String>) -> Option<usize> {
    let from_data = || {
        let key = lookup.data.resolve(part.root_type.as_deref()?)?;
        lookup.keys.get(&key).copied()
    };
    let Some((hints, local)) = &part.root_element else {
        let found = from_data();
        if found.is_none()
            && let Some(root_type) = &part.root_type
        {
            notes.push(format!(
                "{}: no struct of the typed tree holds its root {root_type}, so its module loads bytes",
                describe(part)
            ));
        }
        return found;
    };
    let stated = global_element(lookup.transitional, lookup.rows, hints, local).and_then(
        |(tag, type_ref)| {
            lookup
                .keys
                .get(&NodeKey {
                    source: Source::Xsd(type_ref),
                    tag,
                })
                .copied()
        },
    );
    if stated.is_some() {
        return stated;
    }
    let found = from_data();
    notes.push(match (found, &part.root_type) {
        (Some(_), Some(root_type)) => format!(
            "{} names the root element {local}, which no Transitional schema declares; SDK 3.5.1's part data gives {root_type}, which is used",
            describe(part)
        ),
        _ => format!(
            "{} names the root element {local}, which no struct of the typed tree holds, so its module loads bytes",
            describe(part)
        ),
    });
    found
}

/// Each part's typed root, the first module of a struct implementing its
/// `PartRoot`.
fn roots(parts: &[PartType], lookup: &Lookup<'_>, notes: &mut Vec<String>) -> Vec<Option<Root>> {
    let mut implemented = BTreeSet::new();
    parts
        .iter()
        .map(|part| {
            root_node(part, lookup, notes).map(|node| Root {
                node,
                shared: !implemented.insert(node),
            })
        })
        .collect()
}

/// The Strict spelling of a type: the one its parts state, or, when they
/// state several, the one in Strict's own family, with a note per other.
///
/// # Errors
///
/// Several spellings none of which is in Strict's family.
fn strict_spelling(
    parts: &[PartType],
    transitional: &str,
    members: &[usize],
    notes: &mut Vec<String>,
) -> Result<String> {
    let spellings: BTreeSet<&str> = members
        .iter()
        .map(|index| parts[*index].strict.as_str())
        .collect();
    if let [one] = spellings.iter().copied().collect::<Vec<_>>().as_slice() {
        return Ok((*one).to_owned());
    }
    let kept = spellings
        .iter()
        .find(|spelling| spelling.starts_with(STRICT_RELATIONSHIPS))
        .with_context(|| {
            format!("the parts of {transitional} state Strict types none of which is Strict's")
        })?;
    for index in members {
        let part = &parts[*index];
        if part.strict != *kept {
            notes.push(format!(
                "{} states the Strict type {}; the other parts of {transitional} state {kept}, which is kept",
                describe(part),
                part.strict
            ));
        }
    }
    Ok((*kept).to_owned())
}

/// What the generator prints.
fn summary(joined: &Joined, roots: &[Option<Root>]) -> Summary {
    let part_1 = LANGUAGES
        .iter()
        .map(|(prefix, language)| {
            let count = joined
                .parts
                .iter()
                .filter_map(PartType::part_1)
                .filter(|definition| definition.section.starts_with(prefix))
                .count();
            (*language, count)
        })
        .collect();
    let mut kinds: Vec<(String, usize)> = Vec::new();
    for kind in &joined.kinds {
        if let Some((_, count)) = kinds.iter_mut().find(|(file, _)| *file == kind.file) {
            *count += 1;
        } else {
            kinds.push((kind.file.clone(), 1));
        }
    }
    Summary {
        extensions: joined
            .parts
            .iter()
            .filter(|part| part.part_1().is_none())
            .count(),
        kinds,
        list: joined
            .parts
            .iter()
            .zip(roots)
            .map(|(part, root)| summary_line(joined, part, *root))
            .collect(),
        no_root: roots.iter().filter(|root| root.is_none()).count(),
        notes: joined.notes.clone(),
        part_1,
        relationship_types: joined.constants.len(),
        shared_roots: roots
            .iter()
            .filter(|root| root.is_some_and(|known| known.shared))
            .count(),
        typed_roots: roots.iter().filter(|root| root.is_some()).count(),
    }
}

/// One module as the generator lists it.
fn summary_line(joined: &Joined, part: &PartType, root: Option<Root>) -> String {
    let source = match &part.definition {
        Definition::Part1(definition) if definition.part_2 => {
            format!("Part 1, {}, fields of Part 2", definition.section)
        }
        Definition::Part1(definition) => definition.part_4.as_ref().map_or_else(
            || format!("Part 1, {}", definition.section),
            |restated| format!("Part 1, {}; Part 4, {restated}", definition.section),
        ),
        Definition::Extension(extension) => format!(
            "extension, {}",
            extension
                .version
                .map_or_else(|| "Office2007".to_owned(), |known| format!("{known:?}"))
        ),
    };
    let constant = joined
        .constants
        .iter()
        .find(|constant| constant.transitional == part.transitional)
        .map_or("", |constant| constant.name.as_str());
    let shown_root = match root {
        Some(known) if known.shared => "typed root, shared",
        Some(_) => "typed root",
        None => "bytes",
    };
    format!("{} ({source}): {shown_root}, {constant}", part.module)
}
