//! Internal developer-tooling dispatcher for the officekit workspace.
//!
//! Follows the rust-analyzer / Cargo-team `xtask` idiom: invoke via
//! `cargo xtask <subcommand>`. This binary is marked `publish = false`
//! and is never installed on end-user machines.
//!
//! Subcommands:
//!
//! - `generate-schema` -- regenerate the typed element modules from the
//!   Open XML SDK schema data. Not implemented yet: it exits with a
//!   message saying so.

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};

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
        Commands::GenerateSchema => bail!("generate-schema is not implemented yet"),
    }
}
