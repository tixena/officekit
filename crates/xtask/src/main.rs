//! Internal developer-tooling dispatcher for the officekit workspace.
//!
//! Follows the rust-analyzer / Cargo-team `xtask` idiom: invoke via
//! `cargo xtask <subcommand>`. This binary is marked `publish = false`
//! and is never installed on end-user machines.
//!
//! Subcommands:
//!
//! - `generate-schema` -- regenerate the typed element modules from the
//!   Open XML SDK schema data. Finds its inputs under `inputs/`, then exits
//!   with a message saying it emits nothing yet.

use std::path::Path;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use os_shim::System as _;
use os_shim::real::RealSystem;
use xtask::generate_schema::inputs::Inputs;
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
            bail!(
                "generate-schema emits nothing yet ({text}; inputs found: {} Strict XSDs, {} Transitional \
                 XSDs, {} OPC XSDs, 2 texts ({}, {}), SDK data at {}, {} Open Specifications XSDs, \
                 {} checksums, {} tools lines)",
                inputs.strict_schemas.len(),
                inputs.transitional_schemas.len(),
                inputs.opc_schemas.len(),
                inputs.part1_text.display(),
                inputs.part4_text.display(),
                inputs.sdk_data.display(),
                inputs.open_specifications.len(),
                inputs.checksums().lines().count(),
                inputs.tools().lines().count(),
            )
        }
    }
}
