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

use std::path::Path;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use os_shim::System as _;
use os_shim::real::RealSystem;
use xtask::generate_schema::inputs::Inputs;
use xtask::generate_schema::sdk_data::{Extension, ExtensionContent, SdkData};
use xtask::generate_schema::standard_text;

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
    }
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
