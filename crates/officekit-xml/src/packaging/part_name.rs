//! Part names: the `/word/document.xml` style identifiers of OPC parts.

use core::fmt;
use std::path::Path;

use crate::packaging::error::PackageError;

/// A validated OPC part name (ECMA-376 Part 2, section 9.1.1.1).
///
/// A part name starts with `/`, holds one or more non-empty segments separated
/// by single `/` characters, never ends in `/`, and never has a segment that is
/// `.`, `..`, or ends in `.`. Comparisons are exact; a package resolves names
/// without regard to ASCII case, as the specification requires, by filing each
/// part under its [`folded`](Self::folded) form.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
pub struct PartName {
    name: String,
}

impl PartName {
    /// The name exactly as it was given, leading `/` included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// The folder holding the part: `/word` for `/word/document.xml`, the
    /// empty string for `/a.xml`.
    #[must_use]
    pub fn directory(&self) -> &str {
        self.name
            .rsplit_once('/')
            .map_or("", |(directory, _)| directory)
    }

    /// The last segment's extension, ASCII-lowercased: `png` for
    /// `/word/media/image1.PNG`, the empty string when the segment has no `.`.
    #[must_use]
    pub fn extension(&self) -> String {
        self.name
            .rsplit_once('/')
            .and_then(|(_, file)| file.rsplit_once('.'))
            .map_or_else(String::new, |(_, extension)| extension.to_ascii_lowercase())
    }

    /// The ASCII-lowercased form: the key a package files this name under.
    #[must_use]
    pub fn folded(&self) -> String {
        self.name.to_ascii_lowercase()
    }

    /// Build a part name from a zip entry name, which carries no leading `/`.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::PartName`] when the entry name, once prefixed
    /// with `/`, is not a valid part name.
    pub fn from_zip_entry(entry: &str) -> Result<Self, PackageError> {
        Self::new(&format!("/{entry}"))
    }

    /// Validate `name` as a part name.
    ///
    /// # Errors
    ///
    /// Returns [`PackageError::PartName`] when `name` breaks any rule listed
    /// on [`PartName`].
    pub fn new(name: &str) -> Result<Self, PackageError> {
        if let Some(reason) = violation(name) {
            return Err(PackageError::PartName {
                name: name.to_owned(),
                reason,
            });
        }
        Ok(Self {
            name: name.to_owned(),
        })
    }

    /// The part's file in a package's memory filesystem.
    pub(crate) fn path(&self) -> &Path {
        Path::new(&self.name)
    }

    /// The name of this part's relationship part:
    /// `/word/_rels/document.xml.rels` for `/word/document.xml`.
    #[must_use]
    pub fn relationships_part(&self) -> String {
        let (directory, file) = self.name.rsplit_once('/').unwrap_or(("", &self.name));
        format!("{directory}/_rels/{file}.rels")
    }

    /// The name as a zip entry: the leading `/` dropped.
    #[must_use]
    pub fn zip_entry_name(&self) -> &str {
        self.name.strip_prefix('/').unwrap_or(&self.name)
    }
}

impl fmt::Display for PartName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

/// The first part-name rule `name` breaks, if any.
fn violation(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        return Some("must not be empty");
    }
    if !name.starts_with('/') {
        return Some("must start with '/'");
    }
    if name.ends_with('/') {
        return Some("must not end with '/'");
    }
    if name.split('/').skip(1).any(str::is_empty) {
        return Some("must not hold an empty segment");
    }
    if name
        .split('/')
        .skip(1)
        .any(|segment| segment.ends_with('.'))
    {
        return Some("must not hold a segment that ends with '.'");
    }
    None
}

#[cfg(test)]
mod tests;
