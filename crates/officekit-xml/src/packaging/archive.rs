//! The zip archive of a package, in the form ECMA-376 Part 2 Annex B asks for and Word writes.

use std::path::Path;
use std::time::SystemTime;

use os_shim::System as _;
use os_shim::archive::{Zip64, ZipOptions, to_zip_writer};
use os_shim::mock::MemorySystem;

use crate::packaging::error::PackageError;
use crate::packaging::package::{CONTENT_TYPES_PART, PACKAGE_RELATIONSHIPS_PART};
use crate::packaging::relationship::{self, Target, class_of_office_document};
use crate::parse::parse;
use crate::ready::ready_now;

#[cfg(test)]
mod tests;

/// The folded path of the part the package's `officeDocument` relationship names, in either class's
/// spelling; `None` when no such relationship can be read.
fn main_part_path(files: &MemorySystem, paths: &[String]) -> Option<String> {
    let relationships = paths
        .iter()
        .find(|path| path.eq_ignore_ascii_case(PACKAGE_RELATIONSHIPS_PART))?;
    let (bytes, _revision) = ready_now(files.read(Path::new(relationships))).ok()?;
    let document = parse(&bytes).ok()?;
    relationship::parse(&document)
        .ok()?
        .iter()
        .find_map(|item| {
            class_of_office_document(&item.type_uri)?;
            match item.resolved("").ok()? {
                Target::Part(name) => Some(name.folded()),
                Target::External(_) => None,
            }
        })
}

/// Zip every file of `files`: no folder items, ZIP64 only for a part that needs it, a fixed time,
/// and `[Content_Types].xml`, the package's relationship part and the main part first.
///
/// # Errors
///
/// [`PackageError::Io`] when a file cannot be read or the archive cannot be built.
pub fn zip_files(files: &MemorySystem) -> Result<Vec<u8>, PackageError> {
    let root = Path::new("/");
    let paths: Vec<String> = ready_now(files.walk_dir(root, false, true))?
        .into_iter()
        .filter(|entry| entry.is_file)
        .map(|entry| entry.path.to_string_lossy().into_owned())
        .collect();
    let main = main_part_path(files, &paths);
    let stored = |wanted: &str| {
        paths
            .iter()
            .find(|path| path.eq_ignore_ascii_case(wanted))
            .map(|path| path.trim_start_matches('/'))
    };
    let names: Vec<&str> = [
        CONTENT_TYPES_PART,
        PACKAGE_RELATIONSHIPS_PART,
        main.as_deref().unwrap_or(""),
    ]
    .into_iter()
    .filter_map(stored)
    .collect();
    let options = ZipOptions::default()
        .with_directories(false)
        .with_zip64(Zip64::WhenNeeded)
        .with_fixed_time(SystemTime::UNIX_EPOCH)
        .with_first_entries(&names);
    let mut bytes = Vec::new();
    ready_now(to_zip_writer(files, root, &mut bytes, options))?;
    Ok(bytes)
}
