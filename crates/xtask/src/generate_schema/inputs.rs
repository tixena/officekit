//! Where `just inputs` leaves everything the generator reads.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;

use crate::generate_schema::cldr;

#[cfg(test)]
mod tests;

/// Where each input the generator reads lives under `inputs/`.
#[derive(Debug)]
#[non_exhaustive]
pub struct Inputs {
    /// The `SHA256SUMS` text.
    pub checksums: String,
    /// CLDR's calendar keywords, `bcp47/calendar.xml`.
    pub cldr_calendars: PathBuf,
    /// CLDR's `LICENSE`.
    pub cldr_license: PathBuf,
    /// CLDR's `likelySubtags.xml`.
    pub cldr_likely_subtags: PathBuf,
    /// CLDR's main file of each locale, sorted.
    pub cldr_main: Vec<PathBuf>,
    /// CLDR's `numberingSystems.xml`.
    pub cldr_numbering_systems: PathBuf,
    /// CLDR's ordinal plural rules, `ordinals.xml`.
    pub cldr_ordinals: PathBuf,
    /// CLDR's cardinal plural rules, `plurals.xml`.
    pub cldr_plurals: PathBuf,
    /// CLDR's rule-based number formats, one file per locale, sorted.
    pub cldr_rbnf: Vec<PathBuf>,
    /// CLDR's `supplementalData.xml`, for its parent locales.
    pub cldr_supplemental: PathBuf,
    /// The document part of [MS-LCID]'s DOCX, version 16.0, for its table of language ids.
    pub ms_lcid: PathBuf,
    /// The OPC schemas.
    pub opc_schemas: Vec<PathBuf>,
    /// The `(prefix, XSD)` pair of each Open Specifications namespace found.
    pub open_specifications: Vec<(String, PathBuf)>,
    /// `pdftotext -layout` of ECMA-376 Part 1.
    pub part1_text: PathBuf,
    /// `pdftotext -layout` of ECMA-376 Part 4.
    pub part4_text: PathBuf,
    /// ECMA-376 Part 1's `presetShapeDefinitions.xml`, the definitions of the preset shapes.
    pub preset_shapes: PathBuf,
    /// The SDK's `data/` folder.
    pub sdk_data: PathBuf,
    /// SDK 3.5.1's semantic constraint registrations, read from its package
    /// by `just semantic-inputs`.
    pub semantic_dump: PathBuf,
    /// The 21 Strict schemas.
    pub strict_schemas: Vec<PathBuf>,
    /// The `TOOLS` text.
    pub tools: String,
    /// The 26 Transitional schemas.
    pub transitional_schemas: Vec<PathBuf>,
    /// The Unicode Character Database's `UnicodeData.txt`, version 18.0.0.
    pub unicode_data: PathBuf,
    /// The Unicode Character Database's `PropList.txt`, version 18.0.0.
    pub unicode_prop_list: PathBuf,
    /// The Unicode Character Database's `SpecialCasing.txt`, version 18.0.0.
    pub unicode_special_casing: PathBuf,
}

impl Inputs {
    /// The `SHA256SUMS` lines, for the header of every generated file.
    #[must_use = "this only returns the value"]
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
        let preset_shapes = file(
            system,
            ecma.join("geometries").join("presetShapeDefinitions.xml"),
        )?;
        let strict_schemas = xsds(system, &ecma.join("strict"))?;
        let transitional_schemas = xsds(system, &ecma.join("transitional"))?;
        let opc_schemas = xsds(system, &ecma.join("opc"))?;

        let sdk_data = root.join("open-xml-sdk").join("data");
        if !ready_now(system.is_dir(&sdk_data))? {
            bail!("missing input: {}", sdk_data.display());
        }

        let specifications = root.join("open-specifications");
        file(system, specifications.join("SOURCES"))?;
        let ms_lcid = file(system, specifications.join("MS-LCID-240423.xml"))?;
        let open_specifications = xsds(system, &specifications)?
            .into_iter()
            .filter_map(|path| {
                let prefix = path.file_stem()?.to_str()?.to_owned();
                Some((prefix, path))
            })
            .collect();

        let semantic_dump = file(system, root.join("sdk-3.5.1-semantic.json"))?;
        let unicode = root.join("unicode").join("18.0.0");
        let unicode_data = file(system, unicode.join("UnicodeData.txt"))?;
        let unicode_prop_list = file(system, unicode.join("PropList.txt"))?;
        let unicode_special_casing = file(system, unicode.join("SpecialCasing.txt"))?;

        let cldr = root.join("cldr").join(cldr::VERSION);
        let cldr_license = file(system, cldr.join("LICENSE"))?;
        let common = cldr.join("common");
        let cldr_calendars = file(system, common.join("bcp47").join("calendar.xml"))?;
        let cldr_main = files(system, &common.join("main"), "xml")?;
        let cldr_rbnf = files(system, &common.join("rbnf"), "xml")?;
        let supplemental = common.join("supplemental");
        let cldr_likely_subtags = file(system, supplemental.join("likelySubtags.xml"))?;
        let cldr_numbering_systems = file(system, supplemental.join("numberingSystems.xml"))?;
        let cldr_ordinals = file(system, supplemental.join("ordinals.xml"))?;
        let cldr_plurals = file(system, supplemental.join("plurals.xml"))?;
        let cldr_supplemental = file(system, supplemental.join("supplementalData.xml"))?;

        let checksums = read(system, &file(system, root.join("SHA256SUMS"))?)?;
        let tools = read(system, &file(system, root.join("TOOLS"))?)?;

        Ok(Self {
            checksums,
            cldr_calendars,
            cldr_license,
            cldr_likely_subtags,
            cldr_main,
            cldr_numbering_systems,
            cldr_ordinals,
            cldr_plurals,
            cldr_rbnf,
            cldr_supplemental,
            ms_lcid,
            opc_schemas,
            open_specifications,
            part1_text,
            part4_text,
            preset_shapes,
            sdk_data,
            semantic_dump,
            strict_schemas,
            tools,
            transitional_schemas,
            unicode_data,
            unicode_prop_list,
            unicode_special_casing,
        })
    }

    /// The `TOOLS` lines: `pdftotext`'s version and the SDK commit.
    #[must_use = "this only returns the value"]
    pub fn tools(&self) -> &str {
        &self.tools
    }
}

/// The entries of `folder`, spelled from `folder` as given: `read_dir` gives a path resolved
/// against the current folder, and `SHA256SUMS` names each input from the repository's root.
///
/// # Errors
///
/// A folder that cannot be read.
pub fn listed<S>(system: &S, folder: &Path) -> Result<Vec<PathBuf>>
where
    S: System + ?Sized,
{
    Ok(ready_now(system.read_dir(folder))
        .with_context(|| format!("cannot read {}", folder.display()))?
        .iter()
        .filter_map(|path| path.file_name())
        .map(|name| folder.join(name))
        .collect())
}

fn file<S>(system: &S, path: PathBuf) -> Result<PathBuf>
where
    S: System + ?Sized,
{
    if ready_now(system.is_file(&path))? {
        Ok(path)
    } else {
        bail!("missing input: {}", path.display())
    }
}

fn read<S>(system: &S, path: &Path) -> Result<String>
where
    S: System + ?Sized,
{
    ready_now(system.read_to_string(path))
        .with_context(|| format!("cannot read {}", path.display()))
}

/// The files of `folder` with the extension `extension`, sorted.
fn files<S>(system: &S, folder: &Path, extension: &str) -> Result<Vec<PathBuf>>
where
    S: System + ?Sized,
{
    if !ready_now(system.is_dir(folder))? {
        bail!("missing input: {}", folder.display());
    }
    let mut found: Vec<PathBuf> = listed(system, folder)?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    found.sort();
    Ok(found)
}

fn xsds<S>(system: &S, folder: &Path) -> Result<Vec<PathBuf>>
where
    S: System + ?Sized,
{
    files(system, folder, "xsd")
}
