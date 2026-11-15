//! A number written as the text a number format gives it (ECMA-376 Part 1, sections 18.8.30
//! and 18.8.31).
//!
//! ```
//! use officekit::xlsx::number_format::{FormatColor, NumberFormat, general};
//!
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let format = NumberFormat::parse("#,##0.00;[Red]-#,##0.00")?;
//! assert_eq!(format.number("1234.5").map(|shown| shown.plain()), Some("1,234.50".to_owned()));
//! let negative = format.number("-5").ok_or("a number")?;
//! assert_eq!((negative.plain().as_str(), negative.color), ("-5.00", Some(FormatColor::Red)));
//! assert_eq!(general("123456789012", 11).as_deref(), Some("1.23457E+11"));
//! # Ok(())
//! # }
//! ```

mod decimal;
mod general;
mod parse;
#[cfg(test)]
mod tests;
mod write;

pub use general::general;

use decimal::Decimal;
use parse::{Condition, Token};

/// The colour a section names (section 18.8.31, "Specify colors").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FormatColor {
    Black,
    Blue,
    Cyan,
    Green,
    Magenta,
    /// `[Color1]` to `[Color56]`: the entry of the indexed palette at this number plus 7.
    Palette(u8),
    Red,
    White,
    Yellow,
}

/// Where a format code could not be read.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("the format code cannot be read at character {at}")]
#[non_exhaustive]
pub struct FormatError {
    /// The place of the first character not understood, counted in characters from 0.
    pub at: usize,
}

/// What kind of value a format code writes, which a caller needs to align a cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FormatKind {
    /// A section for numbers holds a date's or a time's code.
    DateTime,
    /// The code is `General`, in any case.
    General,
    Number,
    /// The code holds only a section for text.
    Text,
}

/// A format code, read into its sections (section 18.8.31).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumberFormat {
    general: bool,
    /// The sections for numbers, one to three; none when the code has only a text section.
    numbers: Vec<Section>,
    text: Option<Section>,
}

impl NumberFormat {
    /// What kind of value the code writes.
    #[must_use = "this only returns the value"]
    pub fn kind(&self) -> FormatKind {
        if self.general {
            FormatKind::General
        } else if self.numbers.iter().any(|section| section.dates) {
            FormatKind::DateTime
        } else if self.numbers.is_empty() {
            FormatKind::Text
        } else {
            FormatKind::Number
        }
    }

    /// `number`, the text of an XML Schema double, written under its section (section 18.8.31);
    /// `None` when it is no number, no section serves it, or its section writes a date or a time.
    #[must_use = "this only returns the value"]
    pub fn number(&self, number: &str) -> Option<Shown> {
        let value = Decimal::parse(number)?;
        let Some(last) = self.numbers.len().checked_sub(1) else {
            let text = general::unsigned(&value.abs(), general::MOST)?;
            let sign = if value.is_negative() { "-" } else { "" };
            return Some(Shown {
                color: None,
                pieces: vec![Piece::Text(format!("{sign}{text}"))],
            });
        };
        if self
            .numbers
            .iter()
            .any(|section| section.condition.is_some())
        {
            let section = self.numbers.iter().find(|section| {
                section
                    .condition
                    .as_ref()
                    .is_none_or(|condition| condition.holds(&value))
            })?;
            return write::number(section, &value, true);
        }
        let (index, signed) = if value.is_negative() && last >= 1 {
            (1, false)
        } else if value.is_zero() && last == 2 {
            (2, true)
        } else {
            (0, true)
        };
        write::number(&self.numbers[index], &value, signed)
    }

    /// Read `code`: up to four sections at each `;` outside quotation marks and brackets and not
    /// after a backslash, for positive numbers, negative numbers, zero and text (section 18.8.31).
    ///
    /// # Errors
    ///
    /// [`FormatError`] at an unclosed quotation mark or bracket, a `\`, `_` or `*` that ends the
    /// code, a fifth section, or a character the code's language does not hold.
    pub fn parse(code: &str) -> Result<Self, FormatError> {
        let (numbers, text) = parse::sections(code)?;
        Ok(Self {
            general: code.eq_ignore_ascii_case("general"),
            numbers,
            text,
        })
    }

    /// `text` written under the code's text section, at each `@`; as it is when the code has
    /// no text section (section 18.8.31, "Include a section for text entry").
    #[must_use = "this only returns the value"]
    pub fn text(&self, text: &str) -> Shown {
        let Some(section) = &self.text else {
            return Shown {
                color: None,
                pieces: vec![Piece::Text(text.to_owned())],
            };
        };
        write::text(section, text)
    }
}

/// One section of a code: what it shows, in order, and what applies to all of it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Section {
    color: Option<FormatColor>,
    condition: Option<Condition>,
    dates: bool,
    /// The power of ten a number is multiplied by: 2 for each `%`, -3 for each `,` that scales.
    scale: i64,
    thousands: bool,
    tokens: Vec<Token>,
}

/// One piece of what a cell shows.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Piece {
    /// The character to repeat until the cell is full (`*`).
    Fill(char),
    /// A blank as wide as this character (`_`).
    Skip(char),
    Text(String),
}

/// What a cell shows under a format code.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Shown {
    pub color: Option<FormatColor>,
    pub pieces: Vec<Piece>,
}

impl Shown {
    /// The pieces as one text: a `Skip` as one space, a `Fill` as nothing.
    #[must_use = "this only returns the value"]
    pub fn plain(&self) -> String {
        self.pieces
            .iter()
            .map(|piece| match piece {
                Piece::Fill(_) => "",
                Piece::Skip(_) => " ",
                Piece::Text(text) => text,
            })
            .collect()
    }
}
