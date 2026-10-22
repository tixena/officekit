//! Command handlers: one `cmd_*` function per subcommand.

use std::io::{self, Read, Write};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use archie_core::parser::ast::RawDocument;
use archie_core::writer::{self, WriteOptions};
use os_shim::{System, WriteMode};
use serde_json::{Value, json};

use officekit_core::folder::{self, DownOptions, UpOptions};
use officekit_core::markdown::MarkdownOptions;
use officekit_core::office_file::OfficeFile;
use officekit_packaging::package::OpenOptions;
use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_validation::error::ValidationError;
use officekit_validation::validator::Validator;
use officekit_xml::name::OfficeVersion;

use crate::cli::{CatArgs, DownArgs, Form, FormatArgs, GetArgs, PartsArgs, UpArgs, ValidateArgs};
use crate::dispatch::EXIT_ERROR;

#[cfg(test)]
mod tests;

/// What the `validate` handler needs from the validator; the handler's tests
/// give it one that returns fixed errors.
pub trait Validate {
    /// # Errors
    ///
    /// A description of why the check could not run.
    fn validate(
        &self,
        file: &Path,
        target: OfficeVersion,
        max_errors: usize,
    ) -> Result<Vec<ValidationError>, String>;
}

/// Runs officekit's [`Validator`] on the package at a path, opened through a
/// [`System`].
pub struct PackageValidator<'system, S>
where
    S: System + ?Sized,
{
    system: &'system S,
}

impl<'system, S> PackageValidator<'system, S>
where
    S: System + ?Sized,
{
    pub const fn new(system: &'system S) -> Self {
        Self { system }
    }
}

impl<S> Validate for PackageValidator<'_, S>
where
    S: System + ?Sized,
{
    fn validate(
        &self,
        file: &Path,
        target: OfficeVersion,
        max_errors: usize,
    ) -> Result<Vec<ValidationError>, String> {
        let package = Package::open(self.system, file).map_err(|err| err.to_string())?;
        Ok(Validator::new(target)
            .with_max_errors(max_errors)
            .validate(&package))
    }
}

/// `officekit cat <FILE> <PART>`: write the part's bytes to standard output.
pub fn cmd_cat<S>(system: &S, args: &CatArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let path = args.file.as_path();
    let package =
        Package::open(system, path).with_context(|| format!("opening {}", path.display()))?;
    let name = PartName::new(&args.part)?;
    stdout.write_all(&package.part_bytes(&name)?)?;
    Ok(())
}

/// `officekit down --src DIR --tgt DIR`: one `expanded:` line per folder written.
pub fn cmd_down<S>(system: &S, args: &DownArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let mut options = DownOptions::default();
    options.pretty = args.pretty;
    options.suffix = &args.suffix;
    for folder in folder::down(system, &args.src, &args.tgt, options)? {
        writeln!(stdout, "expanded: {}", folder.display())?;
    }
    Ok(())
}

/// `officekit up --src DIR --tgt DIR`: one `created:` line per file written.
pub fn cmd_up<S>(system: &S, args: &UpArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let mut options = UpOptions::default();
    options.minify = args.minify;
    options.suffix = &args.suffix;
    for file in folder::up(system, &args.src, &args.tgt, options)? {
        writeln!(stdout, "created: {}", file.display())?;
    }
    Ok(())
}

/// `officekit format --dir DIR`: how many XML parts were inspected and rewritten.
pub fn cmd_format<S>(system: &S, args: &FormatArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let summary = folder::format(system, &args.dir)?;
    writeln!(
        stdout,
        "inspected {} XML parts, rewrote {}",
        summary.inspected, summary.rewritten
    )?;
    Ok(())
}

/// `officekit get <FILE> -f <FORM>`: the file, or standard input, loaded and written in `FORM`.
pub fn cmd_get<S>(
    system: &S,
    args: &GetArgs,
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<()>
where
    S: System + ?Sized,
{
    let (bytes, folder) = if args.file == Path::new("-") {
        let mut bytes = Vec::new();
        stdin.read_to_end(&mut bytes)?;
        (bytes, None)
    } else {
        let path = args.file.as_path();
        let mut bytes = Vec::new();
        system
            .open(path)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .with_context(|| format!("reading {}", path.display()))?;
        (bytes, path.parent())
    };
    let mut markdown = MarkdownOptions::default();
    markdown.embed_images = true;
    markdown.frontmatter = args.frontmatter;
    let file = OfficeFile::from_input(system, bytes, folder, OpenOptions::default(), markdown)?;
    let output = match (args.format, &file) {
        (Form::Markdown, OfficeFile::Word(docx)) => {
            let exported = docx.to_archie("media", markdown)?;
            writer::write(
                &RawDocument::Valid(exported.document),
                &WriteOptions::default(),
            )?
            .into_bytes()
        }
        (Form::Markdown, other) => bail!("markdown is not available for {} yet", kind_of(other)),
        (Form::Docx, OfficeFile::Word(_))
        | (Form::Xlsx, OfficeFile::Excel(_))
        | (Form::Pptx, OfficeFile::PowerPoint(_)) => {
            let (saved, warnings) = file.to_bytes_transitional()?;
            for warning in warnings {
                writeln!(stderr, "warning: {warning}")?;
            }
            saved
        }
        (Form::Docx | Form::Xlsx | Form::Pptx, _) => {
            bail!(
                "the input is {}; it cannot be written as {}",
                kind_of(&file),
                form_name(args.format)
            )
        }
    };
    match args.output.as_deref() {
        Some(path) if path != Path::new("-") => system
            .write(path, &output, WriteMode::Replace)
            .with_context(|| format!("writing {}", path.display()))?,
        _ => stdout.write_all(&output)?,
    }
    Ok(())
}

const fn kind_of(file: &OfficeFile) -> &'static str {
    match file {
        OfficeFile::Excel(_) => "an Excel workbook",
        OfficeFile::PowerPoint(_) => "a PowerPoint presentation",
        OfficeFile::Word(_) => "a Word document",
        _ => "an Office file",
    }
}

const fn form_name(form: Form) -> &'static str {
    match form {
        Form::Markdown => "markdown",
        Form::Docx => "docx",
        Form::Xlsx => "xlsx",
        Form::Pptx => "pptx",
    }
}

/// `officekit parts <FILE>`: list every part in the package, one per line
/// or as a JSON object with `--json`.
pub fn cmd_parts<S>(system: &S, args: &PartsArgs, stdout: &mut dyn Write) -> Result<()>
where
    S: System + ?Sized,
{
    let path = args.file();
    let package =
        Package::open(system, path).with_context(|| format!("opening {}", path.display()))?;
    let parts = archive_entries(&package)?;
    if args.json() {
        let payload = json!({ "file": path.display().to_string(), "parts": parts });
        writeln!(stdout, "{}", serde_json::to_string_pretty(&payload)?)?;
    } else {
        for name in &parts {
            writeln!(stdout, "{name}")?;
        }
    }
    Ok(())
}

/// What the archive holds: `[Content_Types].xml`, every relationship part,
/// and every other part, sorted by case-folded name.
fn archive_entries(package: &Package) -> Result<Vec<String>> {
    let mut entries = vec!["/[Content_Types].xml".to_owned()];
    entries.extend(package.relationship_sets().into_iter().map(|(source, _)| {
        source.map_or_else(
            || "/_rels/.rels".to_owned(),
            |part| part.relationships_part(),
        )
    }));
    entries.extend(
        package
            .part_names()?
            .iter()
            .map(|name| name.as_str().to_owned()),
    );
    entries.sort_by_cached_key(|entry| entry.to_ascii_lowercase());
    Ok(entries)
}

/// `officekit validate <FILE>`: one line per error, or with `--json` one array
/// of records. The status is 0 with no errors, 1 with errors (on `stdout`), and
/// 2 when the check could not run (the reason on `stderr`).
///
/// # Errors
///
/// The streams'.
pub fn cmd_validate(
    validator: &dyn Validate,
    args: &ValidateArgs,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> io::Result<u8> {
    let errors = match validator.validate(&args.file, args.target, args.max_errors) {
        Ok(errors) => errors,
        Err(reason) => {
            writeln!(
                stderr,
                "officekit validate: {}: {reason}",
                args.file.display()
            )?;
            return Ok(EXIT_ERROR);
        }
    };
    if args.json {
        let file = args
            .file
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let records: Vec<Value> = errors
            .iter()
            .map(|error| error.record(&file, args.target))
            .collect();
        writeln!(stdout, "{}", Value::Array(records))?;
    } else {
        for error in &errors {
            writeln!(stdout, "{}", error.line())?;
        }
    }
    Ok(u8::from(!errors.is_empty()))
}
