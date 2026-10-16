//! Internal developer-tooling dispatcher for the officekit workspace.
//!
//! Follows the rust-analyzer / Cargo-team `xtask` idiom: invoke via
//! `cargo xtask <subcommand>`. This binary is marked `publish = false`
//! and is never installed on end-user machines.
//!
//! Subcommands:
//!
//! - `generate-schema` -- regenerate the generated modules of `officekit-core`
//!   from the inputs under `inputs/`. Writes the namespace table, the
//!   validation tables, and the semantic constraint table.
//! - `schema-counts` -- read the schema sets under `inputs/` into the schema
//!   model and print its counts and the comparison of the two classes.
//! - `parity` -- print every error of the given kinds that the parity
//!   oracle's records hold and officekit's records lack; exits 1 when one is
//!   missing.

use std::io::{Write, stdout};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use clap::{Parser, Subcommand};
use os_shim::real::RealSystem;
use xtask::generate_schema::compare;
use xtask::generate_schema::emit::{namespaces, semantic, validation};
use xtask::generate_schema::inputs::Inputs;
use xtask::generate_schema::model::{Content, Name, ParticleKind, SchemaSet, SimpleType};
use xtask::parity;

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
    /// Print every error of the given kinds the oracle reports and officekit's
    /// records lack, and a count per kind; exit 1 when one is missing.
    Parity {
        /// The oracle's JSON array of records.
        oracle: PathBuf,
        /// A folder of officekit's `validate --json` results, one JSON array per file and target.
        officekit: PathBuf,
        /// The kinds compared, comma-separated: schema, markup-compatibility, package, semantic.
        #[arg(long, value_delimiter = ',', required = true)]
        kinds: Vec<String>,
    },
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

fn main() -> Result<ExitCode> {
    let cli = Cli::parse();
    match cli.command {
        Commands::GenerateSchema => {
            let system = RealSystem::new();
            let inputs = Inputs::open(&system, Path::new("inputs"))?;
            let summary = namespaces::emit(&system, &inputs, Path::new(namespaces::TABLE_PATH))?;
            let mut out = stdout().lock();
            writeln!(
                out,
                "wrote {}: {} namespaces, {} with a Strict URI, {} dated Office 2010 or later",
                namespaces::TABLE_PATH,
                summary.rows,
                summary.strict_uris,
                summary.introduced,
            )?;
            let emitted = validation::emit(
                &system,
                &inputs,
                Path::new(validation::SIMPLE_TYPES_PATH),
                Path::new(validation::PART_ROOTS_PATH),
                Path::new(validation::GENERATED_PATH),
            )?;
            validation_summary(&mut out, &emitted.summary)?;
            let semantic_table = semantic::emit(
                &system,
                &inputs,
                &emitted,
                Path::new(semantic::GENERATED_PATH),
            )?;
            semantic_summary(&mut out, &semantic_table)?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Parity {
            oracle,
            officekit,
            kinds,
        } => {
            let report = parity::run(&RealSystem::new(), &oracle, &officekit, &kinds)?;
            let mut out = stdout().lock();
            for record in &report.missing {
                writeln!(out, "missing: {record}")?;
            }
            writeln!(out, "{}", report.summary())?;
            Ok(if report.complete() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            })
        }
        Commands::SchemaCounts => {
            schema_counts(&mut stdout().lock())?;
            Ok(ExitCode::SUCCESS)
        }
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

/// What `generate-schema` wrote of the semantic constraint table.
fn semantic_summary(out: &mut dyn Write, summary: &semantic::Summary) -> Result<()> {
    writeln!(
        out,
        "wrote {}: {} rows over {} element types, {} from Schematron rules, {} from the \
         reflection dump",
        semantic::GENERATED_PATH,
        summary.rows,
        summary.types,
        summary.from_rules,
        summary.from_dump,
    )?;
    let kinds: Vec<String> = summary
        .by_kind
        .iter()
        .map(|(kind, count)| format!("{count} {kind}"))
        .collect();
    writeln!(out, "  by kind: {}", kinds.join(", "))?;
    Ok(())
}

/// What `generate-schema` wrote of the validation tables, the comparison items
/// no row of the data carries, and the Strict checks and namespaces it added.
fn validation_summary(out: &mut dyn Write, summary: &validation::Summary) -> Result<()> {
    writeln!(
        out,
        "wrote {}: {} types, {} attribute rows, {} checks, {} content models, {} patterns; \
         {} part rules with {} child entries, {} required, {} allowed once",
        validation::GENERATED_PATH,
        summary.types,
        summary.attribute_rows,
        summary.checks,
        summary.content_models,
        summary.patterns,
        summary.parts,
        summary.part_children,
        summary.part_required,
        summary.part_once,
    )?;
    writeln!(
        out,
        "  Transitional-only marks: {} children, {} attributes, {} enumeration values; \
         comparison items no row carries: {}",
        summary.marked_children,
        summary.marked_attributes,
        summary.marked_values,
        summary.unmatched.len(),
    )?;
    for item in &summary.unmatched {
        writeln!(out, "    {item}")?;
    }
    writeln!(
        out,
        "  Strict checks: {} attribute rows; Transitional-only namespaces: {}",
        summary.strict_checks,
        summary.transitional_only_namespaces.len(),
    )?;
    for (prefix, namespace) in &summary.transitional_only_namespaces {
        writeln!(out, "    {prefix} {namespace}")?;
    }
    Ok(())
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
