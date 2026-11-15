//! The default indexed colours: the red, green and blue of each index from 0 to 63, read from
//! section 18.8.27 of Part 1 and written to `crates/officekit/src/xlsx/color/palette.rs`.

use core::fmt::{self, Write as _};
use core::slice;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::namespaces;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::standard_text;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/color/palette.rs";

/// The count of colours the section gives, indexes 0 to 63.
pub const COUNT: usize = 64;

/// The red, green and blue of each index from 0 to 63, in order, when the section gives each of
/// them once. The first two of the eight digits are dropped.
///
/// # Errors
///
/// Another count, or an index given twice or past 63.
pub fn checked(mut colors: Vec<(u32, u32)>) -> Result<Vec<[u8; 3]>> {
    if colors.len() != COUNT {
        bail!(
            "section 18.8.27 gives {} indexed colours, not {COUNT}",
            colors.len()
        );
    }
    colors.sort_unstable();
    for (expected, (index, _)) in (0_u32..).zip(&colors) {
        if *index != expected {
            bail!("section 18.8.27 gives no indexed colour {expected}, or gives one twice");
        }
    }
    Ok(colors
        .iter()
        .map(|&(_, argb)| {
            [16_u32, 8, 0].map(|shift| u8::try_from((argb >> shift) & 0xFF).unwrap_or(u8::MAX))
        })
        .collect())
}

/// Read section 18.8.27 of Part 1 and write the table; the count of colours written.
///
/// # Errors
///
/// An input that cannot be read, a list that gives no table, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let text = ready_now(system.read_to_string(&inputs.part1_text))
        .with_context(|| format!("cannot read {}", inputs.part1_text.display()))?;
    let colors = checked(standard_text::indexed_colors(&text))?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        slice::from_ref(&inputs.part1_text),
        &[],
    )?;
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
pub fn render(header: &str, colors: &[[u8; 3]]) -> String {
    let mut text = format!(
        "{header}\n/// The default indexed colours (ECMA-376 Part 1, section 18.8.27): red, green and blue \
         of\n/// indexes 0 to {}.\npub(super) static INDEXED_COLORS: [[u8; 3]; {}] = [\n",
        colors.len().saturating_sub(1),
        colors.len()
    );
    for [red, green, blue] in colors {
        let _: fmt::Result = writeln!(text, "    [{red}, {green}, {blue}],");
    }
    text.push_str("];\n");
    text
}
