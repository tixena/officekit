//! Internal developer-tooling dispatcher for the officekit workspace.
//!
//! Follows the rust-analyzer / Cargo-team `xtask` idiom: invoke via
//! `cargo xtask <subcommand>`. This binary is marked `publish = false`
//! and is never installed on end-user machines.
//!
//! Subcommands:
//!
//! - `generate-schema` -- regenerate the typed element modules from the
//!   Open XML SDK schema data. Finds its inputs under `inputs/`, reads the
//!   standard's text and the SDK data, then exits with a message saying it
//!   emits nothing yet and what it read.
//! - `schema-counts` -- read the schema sets under `inputs/` into the schema
//!   model and print its counts and the comparison of the two classes.

use std::io::{Write, stdout};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use clap::{Parser, Subcommand};
use os_shim::System as _;
use os_shim::real::RealSystem;
use xtask::generate_schema::compare;
use xtask::generate_schema::inputs::Inputs;
use xtask::generate_schema::model::{Content, Name, ParticleKind, SchemaSet, SimpleType};
use xtask::generate_schema::sdk_data::{Extension, ExtensionContent, SdkData};
use xtask::generate_schema::standard_text;

/// The namespace of `CT_P` and `CT_TblCellMar`, which the counts show.
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Internal developer tooling dispatcher.
#[derive(Parser)]
#[command(name = "xtask", about = "Officekit workspace dev tooling", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available `cargo xtask` subcommands.
#[derive(Subcommand)]
enum Commands {
    /// Regenerate the typed element modules from the Open XML SDK schema data.
    GenerateSchema,
    /// Print the counts of the schema model over `inputs/` and the comparison of the two classes.
    SchemaCounts,
}

/// The comparison of the two classes over the 21 files they share.
fn comparison(out: &mut dyn Write, transitional: &SchemaSet, strict: &SchemaSet) -> Result<()> {
    let only = compare::transitional_only(transitional, strict);
    writeln!(
        out,
        "\nTransitional only: complex types {}, attribute groups {}, groups {}, simple types {}",
        only.complex_types.len(),
        only.attribute_groups.len(),
        only.groups.len(),
        only.simple_types.len(),
    )?;
    let mut union_owners: Vec<&Name> = only.union_members.iter().map(|(owner, _)| owner).collect();
    union_owners.dedup();
    writeln!(
        out,
        "  child elements {}, attributes {}, group references {}, enumeration values {}: in {} \
         shared definitions\n  union members {}: in {} shared simple types",
        only.elements.len(),
        only.attributes.len(),
        only.group_references.len(),
        only.enumeration_values.len(),
        only.extended_definitions().len(),
        only.union_members.len(),
        union_owners.len(),
    )?;
    for (owner, member) in &only.union_members {
        writeln!(out, "    {}: {member}", owner.local)?;
    }
    for (owner, wildcard) in &only.wildcards {
        writeln!(
            out,
            "  wildcard in {}: namespace {}, processContents {}",
            owner.local, wildcard.namespace, wildcard.process_contents,
        )?;
    }
    let strict_only = compare::strict_only(transitional, strict);
    writeln!(
        out,
        "Strict only: complex types {}, attribute groups {}, groups {}, simple types {}, child \
         elements {}, attributes {}, group references {}, union members {}, wildcards {}",
        strict_only.complex_types.len(),
        strict_only.attribute_groups.len(),
        strict_only.groups.len(),
        strict_only.simple_types.len(),
        strict_only.elements.len(),
        strict_only.attributes.len(),
        strict_only.group_references.len(),
        strict_only.union_members.len(),
        strict_only.wildcards.len(),
    )?;
    for (owner, value) in &strict_only.enumeration_values {
        writeln!(out, "  enumeration value {value} in {}", owner.local)?;
    }
    Ok(())
}

/// A content model as text.
fn content(content: &Content) -> String {
    if let Content::Particle(particle) = content {
        particle.to_string()
    } else {
        "no particle".to_owned()
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::GenerateSchema => {
            let system = RealSystem::new();
            let inputs = Inputs::open(&system, Path::new("inputs"))?;
            let part_1 = system.read_to_string(&inputs.part1_text)?;
            let part_4 = system.read_to_string(&inputs.part4_text)?;
            let text = standard_text::summary(&part_1, &part_4);
            let data = SdkData::read(&system, &inputs.sdk_data)?;
            bail!(
                "generate-schema emits nothing yet ({text}; inputs found: {} Strict XSDs, {} Transitional \
                 XSDs, {} OPC XSDs, 2 texts ({}, {}), SDK data at {}, {} Open Specifications XSDs, \
                 {} checksums, {} tools lines)\n{}",
                inputs.strict_schemas.len(),
                inputs.transitional_schemas.len(),
                inputs.opc_schemas.len(),
                inputs.part1_text.display(),
                inputs.part4_text.display(),
                inputs.sdk_data.display(),
                inputs.open_specifications.len(),
                inputs.checksums().lines().count(),
                inputs.tools().lines().count(),
                sdk_report(&data),
            )
        }
        Commands::SchemaCounts => schema_counts(&mut stdout().lock()),
    }
}

/// Read the four schema sets under `inputs/` and print their counts.
fn schema_counts(out: &mut dyn Write) -> Result<()> {
    let system = RealSystem::new();
    let inputs = Inputs::open(&system, Path::new("inputs"))?;
    let transitional = SchemaSet::read(&system, &inputs.transitional_schemas)?;
    let strict = SchemaSet::read(&system, &inputs.strict_schemas)?;
    let opc = SchemaSet::read(&system, &inputs.opc_schemas)?;
    let open_specifications: Vec<&PathBuf> = inputs
        .open_specifications
        .iter()
        .map(|(_, path)| path)
        .collect();
    let open = SchemaSet::read(&system, &open_specifications)?;
    for (label, set) in [
        ("Transitional", &transitional),
        ("Strict", &strict),
        ("OPC", &opc),
        ("Open Specifications", &open),
    ] {
        writeln!(
            out,
            "{label}: files read {}; global elements {}, global attributes {}, complex types {}, \
             simple types {}, groups {}, attribute groups {}",
            set.namespaces.len(),
            set.elements.len(),
            set.attributes.len(),
            set.complex_types.len(),
            set.simple_types.len(),
            set.groups.len(),
            set.attribute_groups.len(),
        )?;
    }
    transitional_counts(out, &transitional)?;
    comparison(out, &transitional, &strict)
}

/// What the SDK data reader read, in the counts the generator relies on.
fn sdk_report(data: &SdkData) -> String {
    let (covered, empty) = data.extension_namespaces();
    let w14: Vec<Extension<'_>> = data
        .types
        .keys()
        .flat_map(|name| data.extensions_in(name))
        .filter(|extension| extension.namespace == "w14")
        .collect();
    let w14_elements = w14
        .iter()
        .filter(|extension| matches!(extension.content, ExtensionContent::Element(_)))
        .count();
    let mut paragraph = Vec::new();
    for extension in data.extensions_in("w:CT_P/w:p") {
        if let ExtensionContent::Attribute(attribute) = extension.content {
            paragraph.push(format!("{} {:?}", attribute.qname, extension.version));
        }
    }
    format!(
        "SDK data read: {} namespaces, {} types, {} enumerations, {} parts with {} child entries, \
         {} Schematron rules, {} typed namespaces\n\
         namespaces dated Office 2010 to Microsoft 365: {} with types or attributes, {} without \
         ({})\n\
         w14 content inside other namespaces' types: {} elements, {} attributes\n\
         w:CT_P/w:p extension attributes: {}",
        data.namespaces.len(),
        data.types.len(),
        data.enumerations.len(),
        data.parts.len(),
        data.parts
            .iter()
            .map(|part| part.children.len())
            .sum::<usize>(),
        data.schematrons.len(),
        data.typed_namespaces.len(),
        covered.len(),
        empty.len(),
        empty
            .iter()
            .map(|namespace| namespace.prefix.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        w14_elements,
        w14.len() - w14_elements,
        paragraph.join(", "),
    )
}

/// What the Transitional set holds: its declarations, tags, wildcards, and two types.
fn transitional_counts(out: &mut dyn Write, transitional: &SchemaSet) -> Result<()> {
    let declarations = transitional.declarations();
    let pairs = transitional.element_tags();
    let mut tags: Vec<&Name> = pairs.iter().map(|(name, _)| *name).collect();
    tags.dedup();
    writeln!(
        out,
        "\nTransitional: element declarations {}, element references {}, distinct qualified tag \
         names {}, distinct pairs of tag and schema type {}",
        declarations.elements,
        declarations.element_references,
        tags.len(),
        pairs.len(),
    )?;
    let mut files: Vec<_> = transitional.namespaces.values().collect();
    files.sort_by(|left, right| {
        let left_count = left.declarations.elements + left.declarations.element_references;
        let right_count = right.declarations.elements + right.declarations.element_references;
        right_count
            .cmp(&left_count)
            .then_with(|| left.path.cmp(&right.path))
    });
    writeln!(out, "  xsd:element / xsd:attribute per file:")?;
    for file in files {
        writeln!(
            out,
            "    {} {} / {}",
            file.path.file_name().unwrap_or_default().display(),
            file.declarations.elements + file.declarations.element_references,
            file.declarations.attributes,
        )?;
    }
    let wildcards = transitional.wildcards();
    writeln!(out, "  wildcards (xsd:any) {}:", wildcards.len())?;
    for (owner, particle) in wildcards {
        if let ParticleKind::Any {
            namespace,
            process_contents,
        } = &particle.kind
        {
            let max = particle
                .max
                .map_or_else(|| "unbounded".to_owned(), |max| max.to_string());
            writeln!(
                out,
                "    {}: namespace {namespace}, processContents {process_contents}, minOccurs {}, \
                 maxOccurs {max}",
                owner.local, particle.min,
            )?;
        }
    }
    let paragraph = transitional
        .complex_types
        .get(&Name::new(WORDPROCESSING, "CT_P"))
        .context("no CT_P")?;
    let attributes: Vec<&str> = paragraph
        .attributes
        .iter()
        .map(|attribute| attribute.name.local.as_str())
        .collect();
    writeln!(out, "  CT_P attributes: {}", attributes.join(", "))?;
    writeln!(out, "  CT_P content: {}", content(&paragraph.content))?;
    let margins = transitional
        .complex_types
        .get(&Name::new(WORDPROCESSING, "CT_TblCellMar"))
        .context("no CT_TblCellMar")?;
    writeln!(
        out,
        "  CT_TblCellMar content: {}",
        content(&margins.content)
    )?;
    let width = transitional
        .complex_types
        .get(&Name::new(WORDPROCESSING, "CT_TblWidth"))
        .context("no CT_TblWidth")?;
    for attribute in &width.attributes {
        writeln!(
            out,
            "  CT_TblWidth @{}: {}",
            attribute.name.local, attribute.type_ref
        )?;
    }
    if let Some(SimpleType::Union {
        members,
        name: _name,
    }) = transitional
        .simple_types
        .get(&Name::new(WORDPROCESSING, "ST_MeasurementOrPercent"))
    {
        let listed: Vec<String> = members.iter().map(ToString::to_string).collect();
        writeln!(
            out,
            "  ST_MeasurementOrPercent: union of {}",
            listed.join(", ")
        )?;
    }
    Ok(())
}
