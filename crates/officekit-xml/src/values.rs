//! Parsers of simple-type values: the text a file carries, read on demand.

pub mod parse;

use core::fmt::{self, Display, Formatter};
use std::sync::OnceLock;

use regex::Regex;

/// Why a stored text is not a value of its simple type; the text stays stored
/// and is written back as read.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ValueError {
    /// The schema name of the simple type, such as `ST_Jc`.
    pub simple_type: &'static str,
    /// The stored text that is no value of that type.
    pub text: String,
}

/// The pattern facets of a simple type, each compiled once, the first time a
/// value is matched against them.
#[derive(Debug, Default)]
pub struct Patterns {
    compiled: OnceLock<Vec<Regex>>,
    sources: &'static [&'static str],
}

/// `xsd:dateTime`: `2026-10-15T09:30:00Z`, `2026-10-15T09:30:00.25-04:00`, or
/// with no zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DateTime {
    /// Day of the month, 1 to 31.
    pub day: u8,
    /// Hour, 0 to 23.
    pub hour: u8,
    /// Minute, 0 to 59.
    pub minute: u8,
    /// Month, 1 to 12.
    pub month: u8,
    /// Fraction of the second, in nanoseconds.
    pub nanosecond: u32,
    /// Minutes east of UTC; `None` when the text names no zone.
    pub offset_minutes: Option<i16>,
    /// Second, 0 to 59.
    pub second: u8,
    /// Year; negative for years before 1 CE.
    pub year: i32,
}

/// A measurement with its unit, `ST_UniversalMeasure`'s value: `2.5cm`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Measure {
    /// The unit the text spelled.
    pub unit: MeasureUnit,
    /// The number the text carried, in that unit.
    pub value: f64,
}

/// The unit of a [`Measure`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MeasureUnit {
    /// `cm`.
    Centimeters,
    /// `in`.
    Inches,
    /// `mm`.
    Millimeters,
    /// `pc` or `pi`.
    Picas,
    /// `pt`.
    Points,
}

impl DateTime {
    /// A date and time from its `(year, month, day)`, its
    /// `(hour, minute, second, nanosecond)`, and its zone.
    #[must_use = "this only builds the value"]
    pub const fn new(
        (year, month, day): (i32, u8, u8),
        (hour, minute, second, nanosecond): (u8, u8, u8, u32),
        offset_minutes: Option<i16>,
    ) -> Self {
        Self {
            day,
            hour,
            minute,
            month,
            nanosecond,
            offset_minutes,
            second,
            year,
        }
    }
}

impl Display for DateTime {
    /// The value as `xsd:dateTime` spells it: a year of four digits at least, the fraction of the
    /// second with no trailing zero, and `Z` for UTC.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.year < 0_i32 {
            f.write_str("-")?;
        }
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
            self.year.unsigned_abs(),
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second
        )?;
        if self.nanosecond > 0 {
            let mut digits = self.nanosecond;
            let mut width = 9;
            while digits.is_multiple_of(10) {
                digits /= 10;
                width -= 1;
            }
            write!(f, ".{digits:0width$}")?;
        }
        match self.offset_minutes {
            None => Ok(()),
            Some(0) => f.write_str("Z"),
            Some(offset) => {
                let sign = if offset < 0 { '-' } else { '+' };
                let minutes = offset.unsigned_abs();
                write!(f, "{sign}{:02}:{:02}", minutes / 60, minutes % 60)
            }
        }
    }
}

impl Display for Measure {
    /// The value as `ST_UniversalMeasure` spells it, `2.5cm`; picas as `pc`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let unit = match self.unit {
            MeasureUnit::Centimeters => "cm",
            MeasureUnit::Inches => "in",
            MeasureUnit::Millimeters => "mm",
            MeasureUnit::Picas => "pc",
            MeasureUnit::Points => "pt",
        };
        write!(f, "{}{unit}", self.value)
    }
}

impl Measure {
    /// A measure of `value` in `unit`.
    #[must_use = "this only builds the value"]
    pub const fn new(unit: MeasureUnit, value: f64) -> Self {
        Self { unit, value }
    }
}

impl ValueError {
    /// The error for `text` that is no value of `simple_type`.
    #[must_use = "this only builds the value"]
    pub const fn new(simple_type: &'static str, text: String) -> Self {
        Self { simple_type, text }
    }
}

impl Patterns {
    /// Whether there are no facets.
    #[must_use = "this only answers the question"]
    pub const fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// Whether `value` matches one of the facets whole; a facet the `regex`
    /// crate cannot compile matches nothing.
    #[must_use = "this only answers the question"]
    pub fn matches(&self, value: &str) -> bool {
        self.compiled
            .get_or_init(|| {
                self.sources
                    .iter()
                    .filter_map(|pattern| anchored(pattern).ok())
                    .collect()
            })
            .iter()
            .any(|regex| regex.is_match(value))
    }

    /// The facets `sources`, none compiled yet.
    #[must_use = "this only builds the value"]
    pub const fn new(sources: &'static [&'static str]) -> Self {
        Self {
            compiled: OnceLock::new(),
            sources,
        }
    }
}

/// Whether `value` matches `pattern` whole, as an XML Schema pattern facet
/// matches: the regular expression anchored at both ends. The pattern is
/// compiled on each call; [`Patterns`] compiles once.
///
/// # Errors
///
/// A pattern the `regex` crate cannot compile.
pub fn matches_pattern(pattern: &str, value: &str) -> Result<bool, regex::Error> {
    Ok(anchored(pattern)?.is_match(value))
}

/// `pattern` compiled to match a whole text.
fn anchored(pattern: &str) -> Result<Regex, regex::Error> {
    Regex::new(&format!("^(?:{pattern})$"))
}

#[cfg(test)]
mod tests;
