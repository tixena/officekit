//! The decimal digit families: the zero of each, read from the Unicode Character Database's
//! `UnicodeData.txt` and written to `crates/officekit/src/docx/numbering/digits.rs`.

use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};
use core::slice;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::namespaces::{self, UNICODE_NOTICE};
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::unicode_data::{self, DecimalDigit};

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/docx/numbering/digits.rs";

/// The zeros, sorted, when every digit is one of a family of ten consecutive characters valued
/// 0 to 9.
///
/// # Errors
///
/// A character given twice, a zero without its nine digits, or a digit in no family.
pub fn zeros(digits: &[DecimalDigit]) -> Result<Vec<&DecimalDigit>> {
    let mut values = BTreeMap::new();
    for digit in digits {
        if values.insert(digit.character, digit.value).is_some() {
            bail!("UnicodeData.txt gives {} twice", digit.name);
        }
    }
    let mut found: Vec<&DecimalDigit> = digits.iter().filter(|digit| digit.value == 0).collect();
    found.sort_by_key(|digit| digit.character);
    for zero in &found {
        for value in 1..=9_u8 {
            let next = char::from_u32(u32::from(zero.character) + u32::from(value));
            if next.and_then(|character| values.get(&character)) != Some(&value) {
                bail!("{} is not followed by the digit {value}", zero.name);
            }
        }
    }
    if digits.len() != found.len() * 10 {
        bail!(
            "UnicodeData.txt gives {} characters of category Nd, and {} families of ten digits",
            digits.len(),
            found.len()
        );
    }
    Ok(found)
}

/// Read `UnicodeData.txt`, check its digit families, and write the table; the count of
/// families written.
///
/// # Errors
///
/// An input that cannot be read, digits outside a family, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let text = ready_now(system.read_to_string(&inputs.unicode_data))
        .with_context(|| format!("cannot read {}", inputs.unicode_data.display()))?;
    let digits = unicode_data::decimal_digits(&text)?;
    let found = zeros(&digits)?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        slice::from_ref(&inputs.unicode_data),
        &[],
    )?;
    let out = Path::new(PATH);
    ready_now(system.write(out, render(&header, &found).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(found.len())
}

/// The table's source: `header`, then one row per zero, named, in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, zeros: &[&DecimalDigit]) -> String {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n/// The zero of each family of ten decimal digits (Unicode \
         category `Nd`), sorted; a family's\n/// digit n is its zero's code point plus n.\n\
         pub(super) static DIGIT_ZEROS: [char; {}] = [\n",
        zeros.len()
    );
    let literals: Vec<String> = zeros
        .iter()
        .map(|zero| format!("'\\u{{{:04X}}}',", u32::from(zero.character)))
        .collect();
    // rustfmt aligns the trailing comments of consecutive rows.
    let width = literals.iter().map(String::len).max().unwrap_or(0);
    for (literal, zero) in literals.iter().zip(zeros) {
        let _: fmt::Result = writeln!(text, "    {literal:<width$} // {}", zero.name);
    }
    text.push_str("];\n");
    text
}
