//! The preset colours: the red, green and blue of each name of `ST_PresetColorVal`, read from
//! section 20.1.10.48 of Part 1 and written to `crates/officekit/src/drawing/color/preset.rs`.

use alloc::collections::BTreeSet;
use core::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::namespaces;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::model::{Name, SchemaSet, SimpleType};
use crate::generate_schema::standard_text;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/drawing/color/preset.rs";

/// The namespace of `ST_PresetColorVal` in the Transitional schemas.
const DRAWINGML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The colours, sorted by name, when they give each of `names` once.
///
/// # Errors
///
/// A count other than the names', a name given twice, or a name `names` does not hold.
pub fn checked(
    mut colors: Vec<(String, [u8; 3])>,
    names: &[String],
) -> Result<Vec<(String, [u8; 3])>> {
    if colors.len() != names.len() {
        bail!(
            "section 20.1.10.48 gives {} preset colours, and ST_PresetColorVal has {} values",
            colors.len(),
            names.len()
        );
    }
    colors.sort();
    if let Some(((name, _), _)) = colors
        .iter()
        .zip(colors.iter().skip(1))
        .find(|((left, _), (right, _))| left == right)
    {
        bail!("section 20.1.10.48 gives the preset colour {name} twice");
    }
    let values: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    if let Some((name, _)) = colors
        .iter()
        .find(|(name, _)| !values.contains(name.as_str()))
    {
        bail!("section 20.1.10.48 gives {name}, which is no value of ST_PresetColorVal");
    }
    Ok(colors)
}

/// Read section 20.1.10.48 of Part 1, check it against `ST_PresetColorVal`'s values in the
/// Transitional schemas, and write the table; the count of colours written.
///
/// # Errors
///
/// An input that cannot be read, colours that do not match the values, or an output that
/// cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let text = ready_now(system.read_to_string(&inputs.part1_text))
        .with_context(|| format!("cannot read {}", inputs.part1_text.display()))?;
    let transitional = SchemaSet::read(system, &inputs.transitional_schemas)?;
    let Some(SimpleType::Restriction {
        base: _base,
        enumeration,
        facets: _facets,
        name: _name,
    }) = transitional
        .simple_types
        .get(&Name::new(DRAWINGML, "ST_PresetColorVal"))
    else {
        bail!("the Transitional schemas hold no enumeration ST_PresetColorVal");
    };
    let colors = checked(standard_text::preset_colors(&text), enumeration)?;
    let mut read: Vec<PathBuf> = inputs.transitional_schemas.clone();
    read.push(inputs.part1_text.clone());
    let header = namespaces::header(inputs.checksums(), inputs.tools(), &read, &[])?;
    let out = Path::new(PATH);
    if let Some(parent) = out.parent() {
        ready_now(system.create_dir_all(parent))
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    ready_now(system.write(out, render(&header, &colors).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(colors.len())
}

/// The table's source: `header`, then one row per colour in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, colors: &[(String, [u8; 3])]) -> String {
    let mut text = format!(
        "{header}\n/// The red, green and blue of each name of `ST_PresetColorVal` (ECMA-376 Part 1, \
         section\n/// 20.1.10.48), sorted by name.\npub(super) static PRESET_COLORS: [(&str, [u8; \
         3]); {}] = [\n",
        colors.len()
    );
    for (name, [red, green, blue]) in colors {
        let _: fmt::Result = writeln!(text, "    (\"{name}\", [{red}, {green}, {blue}]),");
    }
    text.push_str("];\n");
    text
}
