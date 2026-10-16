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

mod generate_schema;

use std::path::Path;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use generate_schema::inputs::Inputs;
use os_shim::real::RealSystem;

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
            let inputs = Inputs::open(&RealSystem::new(), Path::new("inputs"))?;
            bail!(
                "generate-schema emits nothing yet (inputs found: {} Strict XSDs, {} Transitional \
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
