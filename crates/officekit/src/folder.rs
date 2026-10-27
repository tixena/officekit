//! Expanded documents: a `.docx` as a folder of its parts, and back.
//!
//! [`down`] unpacks every `.docx` under a directory into a folder holding one
//! file per package entry plus an `info.json` that remembers the document's
//! file name; [`up`] zips those folders into `.docx` files again; [`format()`]
//! indents the XML parts of a folder in place. Everything the library does
//! with the operating system goes through an [`os_shim::System`].
//!
//! A folder is named after the file: the stem keeps its letters, digits, white
//! space and `-_.()`, turns spaces into `_`, becomes lower case, and takes the
//! suffix, so `Quarterly Report.docx` becomes `quarterly_report_docx`. The
//! `info.json` in it holds the file name, as `{"name": "Quarterly Report.docx"}`
//! with the indentation of a JSON writer. Only `.docx` files are expanded.

use alloc::string::String;
use alloc::vec::Vec;
use std::ffi::OsStr;
use std::io::{self, Cursor, Read as _};
use std::path::{Path, PathBuf};

use os_shim::archive::load_zip_seekable;
use os_shim::mock::MemorySystem;
use os_shim::{System, WriteMode};
use regex::Regex;
use serde_json::{Value, json};

use officekit_xml::element::XmlError;
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

#[cfg(test)]
mod tests;

/// The suffix that marks a folder as an expanded document.
pub const DEFAULT_SUFFIX: &str = "_docx";

const INFO_FILE: &str = "info.json";

/// What `down` writes for each `.docx`.
///
/// The struct is `#[non_exhaustive]`: start from `Default` and assign the fields. The default
/// `suffix` is the empty string, not [`DEFAULT_SUFFIX`].
///
/// ```rust
/// use officekit::folder::{DEFAULT_SUFFIX, DownOptions};
///
/// let mut options = DownOptions::default();
/// assert_eq!(options.suffix, "");
/// options.pretty = true;
/// options.suffix = DEFAULT_SUFFIX;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DownOptions<'suffix> {
    /// Write every XML part with [`Layout::Pretty`]; off, every entry is
    /// copied byte for byte.
    pub pretty: bool,
    /// What the folder's name ends with.
    pub suffix: &'suffix str,
}

/// What `up` writes for each folder.
///
/// The struct is `#[non_exhaustive]`: start from `Default` and assign the fields. The default
/// `suffix` is the empty string, not [`DEFAULT_SUFFIX`]; with it, `up` zips every folder that
/// holds an `info.json`.
///
/// ```rust
/// use officekit::folder::{DEFAULT_SUFFIX, UpOptions};
///
/// let mut options = UpOptions::default();
/// assert_eq!(options.suffix, "");
/// options.minify = true;
/// options.suffix = DEFAULT_SUFFIX;
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct UpOptions<'suffix> {
    /// Write every XML part with [`Layout::Compact`]; off, every file is
    /// copied byte for byte.
    pub minify: bool,
    /// What the name of a document folder ends with.
    pub suffix: &'suffix str,
}

/// What `format` did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FormatSummary {
    /// The XML parts read.
    pub inspected: usize,
    /// The XML parts whose bytes changed.
    pub rewritten: usize,
}

/// Why an operation on expanded documents failed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FolderError {
    /// A file or directory could not be read or written, or a `.docx` is not
    /// a zip archive.
    #[error("{}: {error}", .path.display())]
    Io {
        /// The error of the file system or the archive.
        error: io::Error,
        /// The file or directory that failed.
        path: PathBuf,
    },

    /// An `info.json` is not JSON, or has no string `name`.
    #[error("{}: {error}", .path.display())]
    Json {
        /// The error of the JSON reader.
        error: serde_json::Error,
        /// The `info.json` that failed.
        path: PathBuf,
    },

    /// A document folder holds no `info.json`.
    #[error("{} has no {INFO_FILE}", .folder.display())]
    MissingInfo {
        /// The folder with no `info.json`.
        folder: PathBuf,
    },

    /// The pattern that normalizes file names did not compile.
    #[error("file name pattern: {0}")]
    Pattern(#[from] regex::Error),

    /// An XML part is not well-formed.
    #[error("{}: {error}", .path.display())]
    Xml {
        /// The error of the XML reader.
        error: XmlError,
        /// The part that failed.
        path: PathBuf,
    },
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> FolderError {
    move |error| FolderError::Io {
        error,
        path: path.to_path_buf(),
    }
}

/// Expand every `.docx` under `src`, at any depth, into a folder under `tgt`, keeping its
/// directory; returns the folders written. A folder that exists is replaced.
///
/// # Errors
///
/// [`FolderError::Io`] for a source that is not a directory, an unreadable
/// `.docx`, or a file that cannot be written; [`FolderError::Xml`] for an XML
/// part that cannot be re-indented.
pub fn down<S>(
    system: &S,
    src: &Path,
    tgt: &Path,
    options: DownOptions<'_>,
) -> Result<Vec<PathBuf>, FolderError>
where
    S: System + ?Sized,
{
    require_dir(system, src)?;
    system.create_dir_all(tgt).map_err(io_error(tgt))?;
    let unwanted = Regex::new(r"[^\w\s\-_\.\(\)]")?;
    let (_, files) = tree(system, src).map_err(io_error(src))?;
    let mut written = Vec::new();
    for docx in files.iter().filter(|file| is_docx(file)) {
        let relative = docx
            .strip_prefix(src)
            .map_err(|_unrelated| FolderError::Io {
                error: io::Error::new(io::ErrorKind::InvalidInput, "not under the source folder"),
                path: docx.clone(),
            })?;
        let name = utf8_name(docx, docx.file_name())?;
        let stem = utf8_name(docx, docx.file_stem())?;
        let folder = tgt
            .join(relative.parent().unwrap_or_else(|| Path::new("")))
            .join(expanded_folder_name(&unwanted, stem, options.suffix));
        expand(system, docx, &folder, name, options.pretty)?;
        written.push(folder);
    }
    Ok(written)
}

/// Zips every expanded folder under `src` back into a `.docx` under `tgt`.
///
/// Each file is named as its `info.json` says. A folder is expanded when its name ends with
/// the suffix and it holds an `info.json`. Returns the files written.
///
/// # Errors
///
/// [`FolderError::Io`] for a source that is not a directory or a file that
/// cannot be read or written; [`FolderError::Json`] and
/// [`FolderError::MissingInfo`] for a folder without a usable `info.json`;
/// [`FolderError::Xml`] for an XML part that cannot be minified.
pub fn up<S>(
    system: &S,
    src: &Path,
    tgt: &Path,
    options: UpOptions<'_>,
) -> Result<Vec<PathBuf>, FolderError>
where
    S: System + ?Sized,
{
    require_dir(system, src)?;
    system.create_dir_all(tgt).map_err(io_error(tgt))?;
    let (dirs, _) = tree(system, src).map_err(io_error(src))?;
    let mut written = Vec::new();
    for folder in &dirs {
        let named = folder
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.ends_with(options.suffix));
        let info = folder.join(INFO_FILE);
        if !named || !system.is_file(&info).map_err(io_error(&info))? {
            continue;
        }
        let relative = folder
            .strip_prefix(src)
            .map_err(|_unrelated| FolderError::Io {
                error: io::Error::new(io::ErrorKind::InvalidInput, "not under the source folder"),
                path: folder.clone(),
            })?;
        let name = document_name(system, folder)?;
        let directory = tgt.join(relative.parent().unwrap_or_else(|| Path::new("")));
        system
            .create_dir_all(&directory)
            .map_err(io_error(&directory))?;
        let output = directory.join(name);
        let archive = collapse(system, folder, options.minify)?;
        system
            .write(&output, &archive, WriteMode::Replace)
            .map_err(io_error(&output))?;
        written.push(output);
    }
    Ok(written)
}

/// Indent every XML part under `dir`, in place.
///
/// An element whose children are all elements gets each child on its own
/// indented line. An element with text among its children, whitespace
/// included, keeps them as they are, so a part that is already indented, even
/// unevenly, keeps its layout.
///
/// # Errors
///
/// [`FolderError::Io`] for a directory or file that cannot be read or
/// written; [`FolderError::Xml`] for an XML part that is not well-formed.
pub fn format<S>(system: &S, dir: &Path) -> Result<FormatSummary, FolderError>
where
    S: System + ?Sized,
{
    require_dir(system, dir)?;
    let (_, files) = tree(system, dir).map_err(io_error(dir))?;
    let mut summary = FormatSummary::default();
    for file in files.iter().filter(|file| is_xml_part(file)) {
        let bytes = read(system, file)?;
        let pretty = relayout(&bytes, Layout::Pretty, file)?;
        summary.inspected += 1;
        if pretty != bytes {
            system
                .write(file, &pretty, WriteMode::Replace)
                .map_err(io_error(file))?;
            summary.rewritten += 1;
        }
    }
    Ok(summary)
}

fn require_dir<S>(system: &S, dir: &Path) -> Result<(), FolderError>
where
    S: System + ?Sized,
{
    if system.is_dir(dir).map_err(io_error(dir))? {
        Ok(())
    } else {
        Err(FolderError::Io {
            error: io::Error::new(io::ErrorKind::NotFound, "not a directory"),
            path: dir.to_path_buf(),
        })
    }
}

/// Every directory and every file under `root`, each sorted by path.
fn tree<S>(system: &S, root: &Path) -> io::Result<(Vec<PathBuf>, Vec<PathBuf>)>
where
    S: System + ?Sized,
{
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut pending = alloc::vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in system.read_dir(&dir)? {
            if system.is_dir(&entry)? {
                dirs.push(entry.clone());
                pending.push(entry);
            } else {
                files.push(entry);
            }
        }
    }
    dirs.sort();
    files.sort();
    Ok((dirs, files))
}

fn read<S>(system: &S, path: &Path) -> Result<Vec<u8>, FolderError>
where
    S: System + ?Sized,
{
    let mut bytes = Vec::new();
    system
        .open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(io_error(path))?;
    Ok(bytes)
}

fn is_docx(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
}

fn is_xml_part(path: &Path) -> bool {
    let root_rels = path
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(".rels"));
    root_rels
        || path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("xml") || extension.eq_ignore_ascii_case("rels")
            })
}

fn utf8_name<'name>(path: &Path, name: Option<&'name OsStr>) -> Result<&'name str, FolderError> {
    name.and_then(OsStr::to_str).ok_or_else(|| FolderError::Io {
        error: io::Error::new(io::ErrorKind::InvalidData, "not a UTF-8 file name"),
        path: path.to_path_buf(),
    })
}

/// The stem without the characters outside letters, digits, whitespace, and
/// `-_.()`, spaces as `_`, lower case, then the suffix.
fn expanded_folder_name(unwanted: &Regex, stem: &str, suffix: &str) -> String {
    let kept = unwanted.replace_all(stem, "");
    let mut name = kept.replace(' ', "_").to_lowercase();
    name.push_str(suffix);
    name
}

fn relayout(bytes: &[u8], layout: Layout, path: &Path) -> Result<Vec<u8>, FolderError> {
    let document = parse(bytes).map_err(|error| FolderError::Xml {
        error,
        path: path.to_path_buf(),
    })?;
    Ok(document.to_xml(WriteOptions::new(layout, false)))
}

/// `bytes` laid out as `layout`, or as they are when there is none.
fn transform(bytes: &[u8], layout: Option<Layout>, path: &Path) -> Result<Vec<u8>, FolderError> {
    layout.map_or_else(
        || Ok(bytes.to_vec()),
        |chosen| relayout(bytes, chosen, path),
    )
}

/// Write the `.docx` at `docx` into `folder`, with `info.json` after it.
fn expand<S>(
    system: &S,
    docx: &Path,
    folder: &Path,
    name: &str,
    pretty: bool,
) -> Result<(), FolderError>
where
    S: System + ?Sized,
{
    let archive = MemorySystem::new();
    let root = Path::new("/");
    load_zip_seekable(&archive, Cursor::new(read(system, docx)?), root).map_err(io_error(docx))?;
    if system.exists(folder).map_err(io_error(folder))? {
        system.remove_dir_all(folder).map_err(io_error(folder))?;
    }
    system.create_dir_all(folder).map_err(io_error(folder))?;
    let (dirs, files) = tree(&archive, root).map_err(io_error(docx))?;
    for dir in &dirs {
        let target = folder.join(dir.strip_prefix(root).unwrap_or(dir));
        system.create_dir_all(&target).map_err(io_error(&target))?;
    }
    for file in &files {
        let target = folder.join(file.strip_prefix(root).unwrap_or(file));
        let layout = (pretty && is_xml_part(file)).then_some(Layout::Pretty);
        let bytes = transform(&read(&archive, file)?, layout, &target)?;
        system
            .write(&target, &bytes, WriteMode::Replace)
            .map_err(io_error(&target))?;
    }
    let info = folder.join(INFO_FILE);
    let text = serde_json::to_string_pretty(&json!({ "name": name })).map_err(|error| {
        FolderError::Json {
            error,
            path: info.clone(),
        }
    })?;
    system
        .write(&info, text.as_bytes(), WriteMode::Replace)
        .map_err(io_error(&info))
}

/// The file name a folder's `info.json` gives, with `.docx` appended when it
/// lacks it.
fn document_name<S>(system: &S, folder: &Path) -> Result<String, FolderError>
where
    S: System + ?Sized,
{
    let info = folder.join(INFO_FILE);
    let text = system.read_to_string(&info).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            FolderError::MissingInfo {
                folder: folder.to_path_buf(),
            }
        } else {
            FolderError::Io {
                error,
                path: info.clone(),
            }
        }
    })?;
    let json_error = |error| FolderError::Json {
        error,
        path: info.clone(),
    };
    let mut value: Value = serde_json::from_str(&text).map_err(json_error)?;
    let mut name: String = serde_json::from_value(value["name"].take()).map_err(json_error)?;
    if !is_docx(Path::new(&name)) {
        name.push_str(".docx");
    }
    Ok(name)
}

/// The zip archive of `folder`, without its top-level `info.json`.
fn collapse<S>(system: &S, folder: &Path, minify: bool) -> Result<Vec<u8>, FolderError>
where
    S: System + ?Sized,
{
    let archive = MemorySystem::new();
    let root = Path::new("/");
    archive.create_dir_all(root).map_err(io_error(root))?;
    let (dirs, files) = tree(system, folder).map_err(io_error(folder))?;
    for dir in &dirs {
        let target = root.join(dir.strip_prefix(folder).unwrap_or(dir));
        archive.create_dir_all(&target).map_err(io_error(&target))?;
    }
    let info = folder.join(INFO_FILE);
    for file in files.iter().filter(|file| **file != info) {
        let target = root.join(file.strip_prefix(folder).unwrap_or(file));
        let layout = (minify && is_xml_part(file)).then_some(Layout::Compact);
        let bytes = transform(&read(system, file)?, layout, file)?;
        archive
            .write(&target, &bytes, WriteMode::Replace)
            .map_err(io_error(&target))?;
    }
    archive.to_zip_vec(root).map_err(io_error(folder))
}
