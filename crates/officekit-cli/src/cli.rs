//! CLI grammar: `Cli`, `Commands`, and the per-subcommand argument groups.
//!
//! Pure clap grammar: no IO and no business logic. The dispatch module
//! matches on the variants defined here.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};
use officekit::folder::DEFAULT_SUFFIX;
use officekit_validation::validator::Validator;
use officekit_xml::name::OfficeVersion;

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
    /// Expand every `.docx` under a folder into a folder of its parts.
    Down(DownArgs),
    /// Re-indent every XML part under a folder, in place.
    Format(FormatArgs),
    /// Load a Word, Excel or `PowerPoint` file, or a markdown file, and write it in another form.
    Get(GetArgs),
    /// List the parts inside a package.
    Parts(PartsArgs),
    /// Write a document from officekit's binary form, read from standard input or `-i`.
    Save(SaveArgs),
    /// Zip every expanded folder under a folder back into a `.docx`.
    Up(UpArgs),
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
pub struct DownArgs {
    /// Indent the XML parts.
    #[arg(long)]
    pub pretty: bool,

    /// The folder searched, at any depth, for `.docx` files.
    #[arg(long)]
    pub src: PathBuf,

    /// What the name of an expanded folder ends with.
    #[arg(long, default_value = DEFAULT_SUFFIX)]
    pub suffix: String,

    /// The folder the expanded documents are written under.
    #[arg(long)]
    pub tgt: PathBuf,
}

#[derive(Args)]
pub struct UpArgs {
    /// Strip the whitespace between elements of the XML parts.
    #[arg(long)]
    pub minify: bool,

    /// The folder searched, at any depth, for expanded documents.
    #[arg(long)]
    pub src: PathBuf,

    /// What the name of an expanded folder ends with.
    #[arg(long, default_value = DEFAULT_SUFFIX)]
    pub suffix: String,

    /// The folder the `.docx` files are written under.
    #[arg(long)]
    pub tgt: PathBuf,
}

#[derive(Args)]
pub struct GetArgs {
    /// The file to load; `-` for standard input.
    pub file: PathBuf,

    /// The form to write.
    #[arg(short, long, value_enum)]
    pub format: Form,

    /// Carry a markdown frontmatter into the Word file and back.
    #[arg(long)]
    pub frontmatter: bool,

    /// The file to write; standard output when absent or `-`.
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Args)]
pub struct SaveArgs {
    /// The form to write.
    #[arg(short, long, value_enum)]
    pub format: Form,

    /// Carry a markdown frontmatter into the Word file and back.
    #[arg(long)]
    pub frontmatter: bool,

    /// The binary form to read; standard input when absent or `-`.
    #[arg(short, long, value_name = "FILE")]
    pub input: Option<PathBuf>,

    /// The file to write; standard output when absent or `-`.
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Form {
    Binary,
    Docx,
    #[value(alias = "md")]
    Markdown,
    Pptx,
    Xlsx,
}

#[derive(Args)]
pub struct FormatArgs {
    /// The folder whose XML parts are re-indented.
    #[arg(long)]
    pub dir: PathBuf,
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
