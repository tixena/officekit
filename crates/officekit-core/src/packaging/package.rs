//! The zip container: open a package through a [`System`], enumerate its parts, and save it back.

use std::io::{Cursor, Read as _, Write as _};
use std::path::Path;

use alloc::collections::BTreeMap;
use os_shim::System;
use zip::ZipArchive;
use zip::result::ZipError;
use zip::write::{SimpleFileOptions, ZipWriter};

use crate::error::PackageError;
use crate::packaging::part_name::PartName;

/// An OPC package held in memory: every part's bytes, filed under the
/// case-folded part name so lookups ignore ASCII case as ECMA-376 requires.
///
/// Opening reads the whole container through the caller's [`System`], so the
/// package never holds a file handle and the same code serves the real
/// filesystem and an in-memory one. Directory entries in the archive are
/// dropped; they are not parts. The `[Content_Types].xml` stream is kept under
/// that name until content-type parsing lands and claims it.
#[derive(Debug, Default)]
pub struct Package {
    parts: BTreeMap<String, Entry>,
}

/// One part: its name as given, and its bytes.
#[derive(Debug)]
struct Entry {
    data: Vec<u8>,
    name: PartName,
}

impl Package {
    /// Parse a package from the bytes of a zip archive.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::Container`] when `bytes` is not a readable zip
    /// archive, [`PackageError::PartName`] when an entry name is not a valid
    /// part name, and [`PackageError::Io`] when an entry cannot be inflated.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, PackageError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes)).map_err(|err| container(&err))?;
        let mut package = Self::default();
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|err| container(&err))?;
            if entry.is_dir() {
                continue;
            }
            let name = PartName::from_zip_entry(entry.name())?;
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            package.set_part(name, data);
        }
        Ok(package)
    }

    /// Read the package at `path` through `system`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::Io`] when the file cannot be read, plus every
    /// failure [`from_bytes`](Self::from_bytes) reports.
    pub fn open<S>(system: &S, path: &Path) -> Result<Self, PackageError>
    where
        S: System + ?Sized,
    {
        let mut reader = system.open(path)?;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::from_bytes(bytes)
    }

    /// The bytes of the part called `name`, if the package holds it.
    #[must_use]
    pub fn part(&self, name: &PartName) -> Option<&[u8]> {
        self.parts
            .get(&name.folded())
            .map(|entry| entry.data.as_slice())
    }

    /// Every part name in the package, in case-folded order.
    pub fn part_names(&self) -> impl Iterator<Item = &PartName> {
        self.parts.values().map(|entry| &entry.name)
    }

    /// Write the package to `path` through `system`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::Io`] when the file cannot be written, plus
    /// every failure [`to_bytes`](Self::to_bytes) reports.
    pub fn save<S>(&self, system: &S, path: &Path) -> Result<(), PackageError>
    where
        S: System + ?Sized,
    {
        let bytes = self.to_bytes()?;
        system.write(path, &bytes)?;
        Ok(())
    }

    /// Store `data` as the part called `name`, replacing any part filed under
    /// the same case-folded name.
    pub fn set_part(&mut self, name: PartName, data: Vec<u8>) {
        self.parts.insert(name.folded(), Entry { data, name });
    }

    /// Serialize the package as a zip archive.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::Container`] when the archive cannot be built
    /// and [`PackageError::Io`] when a part's bytes cannot be written into it.
    pub fn to_bytes(&self) -> Result<Vec<u8>, PackageError> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default();
        for entry in self.parts.values() {
            writer
                .start_file(entry.name.zip_entry_name(), options)
                .map_err(|err| container(&err))?;
            writer.write_all(&entry.data)?;
        }
        let cursor = writer.finish().map_err(|err| container(&err))?;
        Ok(cursor.into_inner())
    }
}

/// Fold a zip failure into the container variant without exposing the zip crate.
fn container(err: &ZipError) -> PackageError {
    PackageError::Container(err.to_string())
}

#[cfg(test)]
mod tests;
