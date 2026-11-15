//! The Unicode Character Database's `UnicodeData.txt`: one character per line, its fields
//! separated by `;` (Unicode Standard Annex #44, section 4.2).

use anyhow::{Context as _, Result};

#[cfg(test)]
mod tests;

/// A character of general category `Nd`, a decimal digit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecimalDigit {
    /// The character, field 0.
    pub character: char,
    /// Its name, field 1.
    pub name: String,
    /// Its decimal digit value, field 6.
    pub value: u8,
}

/// Every character of category `Nd` (field 2), in the text's order.
///
/// # Errors
///
/// A line of category `Nd` whose code point or decimal digit value does not parse.
pub fn decimal_digits(text: &str) -> Result<Vec<DecimalDigit>> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(';').collect();
            (fields.get(2) == Some(&"Nd")).then_some((line, fields))
        })
        .map(|(line, fields)| decimal_digit(&fields).with_context(|| format!("cannot read {line}")))
        .collect()
}

/// The digit of the fields of one line of category `Nd`.
fn decimal_digit(fields: &[&str]) -> Result<DecimalDigit> {
    let code = u32::from_str_radix(fields.first().context("no code point")?, 16)?;
    let character = char::from_u32(code).context("no character")?;
    let name = (*fields.get(1).context("no name")?).to_owned();
    let value = fields.get(6).context("no decimal digit value")?.parse()?;
    Ok(DecimalDigit {
        character,
        name,
        value,
    })
}
