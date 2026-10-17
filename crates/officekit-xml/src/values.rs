//! Parsers of simple-type values: the text a file carries, read on demand.

pub mod parse;

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
    #[must_use]
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

impl Measure {
    #[must_use]
    pub const fn new(unit: MeasureUnit, value: f64) -> Self {
        Self { unit, value }
    }
}

impl ValueError {
    #[must_use]
    pub const fn new(simple_type: &'static str, text: String) -> Self {
        Self { simple_type, text }
    }
}

/// Whether `value` matches `pattern` whole, as an XML Schema pattern facet
/// matches: the regular expression anchored at both ends. The pattern is
/// compiled on each call.
///
/// # Errors
///
/// A pattern the `regex` crate cannot compile.
pub fn matches_pattern(pattern: &str, value: &str) -> Result<bool, regex::Error> {
    Ok(Regex::new(&format!("^(?:{pattern})$"))?.is_match(value))
}

#[cfg(test)]
mod tests;
