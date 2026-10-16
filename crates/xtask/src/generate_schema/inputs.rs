//! Where `just inputs` leaves everything the generator reads.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use os_shim::System;

#[cfg(test)]
mod tests;

/// Where each input the generator reads lives under `inputs/`.
#[derive(Debug)]
pub struct Inputs {
    /// The `SHA256SUMS` text.
    pub checksums: String,
    /// The OPC schemas.
    pub opc_schemas: Vec<PathBuf>,
    /// The `(prefix, XSD)` pair of each Open Specifications namespace found.
    pub open_specifications: Vec<(String, PathBuf)>,
    /// `pdftotext -layout` of ECMA-376 Part 1.
    pub part1_text: PathBuf,
    /// `pdftotext -layout` of ECMA-376 Part 4.
    pub part4_text: PathBuf,
    /// The SDK's `data/` folder.
    pub sdk_data: PathBuf,
    /// The 21 Strict schemas.
    pub strict_schemas: Vec<PathBuf>,
    /// The `TOOLS` text.
    pub tools: String,
    /// The 26 Transitional schemas.
    pub transitional_schemas: Vec<PathBuf>,
}

impl Inputs {
    /// The `SHA256SUMS` lines, for the header of every generated file.
    #[must_use]
    pub fn checksums(&self) -> &str {
        &self.checksums
    }

    /// Find every input under `root`; `Err` names the first one missing.
    ///
    /// # Errors
    ///
    /// A missing file or folder, or an unreadable `SHA256SUMS` or `TOOLS`.
    pub fn open<S>(system: &S, root: &Path) -> Result<Self>
    where
        S: System + ?Sized,
    {
        let ecma = root.join("ecma-376");
        let part1_text = file(system, ecma.join("part1.txt"))?;
        let part4_text = file(system, ecma.join("part4.txt"))?;
        let strict_schemas = xsds(system, &ecma.join("strict"))?;
        let transitional_schemas = xsds(system, &ecma.join("transitional"))?;
        let opc_schemas = xsds(system, &ecma.join("opc"))?;

        let sdk_data = root.join("open-xml-sdk").join("data");
        if !system.is_dir(&sdk_data)? {
            bail!("missing input: {}", sdk_data.display());
        }

        let specifications = root.join("open-specifications");
        file(system, specifications.join("SOURCES"))?;
        let open_specifications = xsds(system, &specifications)?
            .into_iter()
            .filter_map(|path| {
                let prefix = path.file_stem()?.to_str()?.to_owned();
                Some((prefix, path))
            })
            .collect();

        let checksums = read(system, &file(system, root.join("SHA256SUMS"))?)?;
        let tools = read(system, &file(system, root.join("TOOLS"))?)?;

        Ok(Self {
            checksums,
            opc_schemas,
            open_specifications,
            part1_text,
            part4_text,
            sdk_data,
            strict_schemas,
            tools,
            transitional_schemas,
        })
    }

    /// The `TOOLS` lines: `pdftotext`'s version and the SDK commit.
    #[must_use]
    pub fn tools(&self) -> &str {
        &self.tools
    }
}

fn file<S>(system: &S, path: PathBuf) -> Result<PathBuf>
where
    S: System + ?Sized,
{
    if system.is_file(&path)? {
        Ok(path)
    } else {
        bail!("missing input: {}", path.display())
    }
}

fn read<S>(system: &S, path: &Path) -> Result<String>
where
    S: System + ?Sized,
{
    system
        .read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))
}

fn xsds<S>(system: &S, folder: &Path) -> Result<Vec<PathBuf>>
where
    S: System + ?Sized,
{
    if !system.is_dir(folder)? {
        bail!("missing input: {}", folder.display());
    }
    let mut found: Vec<PathBuf> = system
        .read_dir(folder)
        .with_context(|| format!("cannot read {}", folder.display()))?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "xsd"))
        .collect();
    found.sort();
    Ok(found)
}
