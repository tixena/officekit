//! The paper sizes: the width and the height of each code of `paperSize` the standard gives a size,
//! read from section 18.3.1.63 of Part 1 and written to `crates/officekit/src/xlsx/size/paper.rs`.

use core::fmt::{self, Write as _};
use core::slice;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::namespaces;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::standard_text::{self, Paper, PaperUnit};

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/size/paper.rs";

/// EMU in an inch.
const EMU_PER_INCH: i64 = 914_400;

/// EMU in a millimetre.
const EMU_PER_MILLIMETRE: i64 = 36_000;

/// The papers as rows of code, width and height in EMU, sorted by code.
///
/// # Errors
///
/// No paper, a code given twice, or a number that is no whole count of EMU.
pub fn checked(papers: &[Paper]) -> Result<Vec<(u32, i64, i64)>> {
    if papers.is_empty() {
        bail!("section 18.3.1.63 gives no paper size");
    }
    let mut rows = papers
        .iter()
        .map(|paper| {
            Ok((
                paper.code,
                emu(&paper.width.0, paper.width.1)?,
                emu(&paper.height.0, paper.height.1)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    rows.sort_unstable();
    if let Some(((code, _, _), _)) = rows
        .iter()
        .zip(rows.iter().skip(1))
        .find(|((left, _, _), (right, _, _))| left == right)
    {
        bail!("section 18.3.1.63 gives the paper code {code} twice");
    }
    Ok(rows)
}

/// `number` of `unit` in EMU, exactly.
///
/// # Errors
///
/// A number that is not digits with at most one point, or that is no whole count of EMU.
pub fn emu(number: &str, unit: PaperUnit) -> Result<i64> {
    let per_unit = match unit {
        PaperUnit::Inch => EMU_PER_INCH,
        PaperUnit::Millimetre => EMU_PER_MILLIMETRE,
    };
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let digits: i64 = format!("{whole}{fraction}")
        .parse()
        .with_context(|| format!("the paper length {number} is no decimal number"))?;
    let places = u32::try_from(fraction.len())?;
    let denominator = 10_i64
        .checked_pow(places)
        .with_context(|| format!("the paper length {number} has too many decimals"))?;
    let scaled = digits
        .checked_mul(per_unit)
        .with_context(|| format!("the paper length {number} is too long"))?;
    if scaled % denominator != 0 {
        bail!("the paper length {number} is no whole count of EMU");
    }
    Ok(scaled / denominator)
}

/// Read section 18.3.1.63 of Part 1 and write the table; the count of papers written.
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
    let rows = checked(&standard_text::paper_sizes(&text))?;
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
    ready_now(system.write(out, render(&header, &rows).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(rows.len())
}

/// `value` with an underscore between each group of three digits: `7_772_400`.
fn grouped(value: i64) -> String {
    let digits = value.to_string();
    let mut text = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push('_');
        }
        text.push(digit);
    }
    text
}

/// The table's source: `header`, then one row per paper in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, rows: &[(u32, i64, i64)]) -> String {
    let mut text = format!(
        "{header}\n/// The width and the height in EMU of each paper code that ECMA-376 Part 1, section \
         18.3.1.63,\n/// gives a size, sorted by code.\npub(super) static PAPER_SIZES: [(u32, i64, \
         i64); {}] = [\n",
        rows.len()
    );
    for (code, width, height) in rows {
        let _: fmt::Result = writeln!(
            text,
            "    ({code}, {}, {}),",
            grouped(*width),
            grouped(*height)
        );
    }
    text.push_str("];\n");
    text
}
