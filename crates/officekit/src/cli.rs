//! CLI grammar: `Cli`, `Commands`, and the per-subcommand argument groups.
//!
//! Pure clap grammar: no IO and no business logic. The dispatch module
//! matches on the variants defined here.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};
use officekit_core::validation::validator::Validator;
use officekit_core::xml::name::OfficeVersion;

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
    /// Write the bytes of one part to standard output.
    Cat(CatArgs),
    /// List the parts inside a package.
    Parts(PartsArgs),
    /// Check a package against the schema of an Office version.
    Validate(ValidateArgs),
}

#[derive(Args)]
pub struct CatArgs {
    /// Path to a `.docx`, `.xlsx`, `.pptx`, or any other OPC package.
    pub file: PathBuf,

    /// Name of the part, such as `/word/document.xml`.
    pub part: String,
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

#[derive(Args)]
pub struct ValidateArgs {
    /// Path to a `.docx`, `.xlsx`, or `.pptx` file.
    pub file: PathBuf,

    /// Output one JSON array of records.
    #[arg(long)]
    pub json: bool,

    /// Stop after this many errors; 0 means no limit.
    #[arg(long, default_value_t = Validator::DEFAULT_MAX_ERRORS)]
    pub max_errors: usize,

    /// The Office version to check against.
    #[arg(
        long,
        default_value = "365",
        value_name = "2010|2013|2016|2019|2021|365",
        value_parser = parse_target
    )]
    pub target: OfficeVersion,
}

fn parse_target(text: &str) -> Result<OfficeVersion, String> {
    match text {
        "2010" => Ok(OfficeVersion::Office2010),
        "2013" => Ok(OfficeVersion::Office2013),
        "2016" => Ok(OfficeVersion::Office2016),
        "2019" => Ok(OfficeVersion::Office2019),
        "2021" => Ok(OfficeVersion::Office2021),
        "365" => Ok(OfficeVersion::Microsoft365),
        other => Err(format!(
            "'{other}' is not a target; use 2010, 2013, 2016, 2019, 2021, or 365"
        )),
    }
}
