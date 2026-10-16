//! CLI grammar: `Cli`, `Commands`, and the per-subcommand argument groups.
//!
//! Pure clap grammar: no IO and no business logic. The dispatch module
//! matches on the variants defined here.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "officekit",
    version,
    about = "Inspect and edit Office Open XML documents"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub const fn cmd(&self) -> &Commands {
        &self.command
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// List the parts inside a package.
    Parts(PartsArgs),
}

#[derive(Args)]
pub struct PartsArgs {
    /// Path to a `.docx`, `.xlsx`, `.pptx`, or any other OPC package.
    file: PathBuf,

    /// Output as JSON.
    #[arg(long)]
    json: bool,
}

impl PartsArgs {
    pub fn file(&self) -> &Path {
        &self.file
    }

    pub const fn json(&self) -> bool {
        self.json
    }
}
