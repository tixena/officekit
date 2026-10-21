//! Strict date cells in a Transitional save.
//!
//! Transitional has no date cell type, so each date cell (`t="d"`) becomes a
//! number cell holding the date's serial date-time in the workbook's date
//! system (ECMA-376 Part 1, 18.17.4.1), and the save reports each cell it
//! converted.

use core::fmt;
use core::mem;

use officekit_markup::class_rules::{BASE_1900, BASE_1904};
use officekit_markup::spreadsheet::Worksheet;
use officekit_packaging::part_name::PartName;
use officekit_xml::values::DateTime;

#[cfg(test)]
mod tests;

/// Nanoseconds in a day.
const DAY: i128 = 86_400_000_000_000;

/// The decimal places of a serial date-time's text, past an `f64`'s precision.
const PLACES: u32 = 20;

/// Something a save changed that its caller may want to know.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveWarning {
    /// A Strict date cell written as a number cell: the cell's reference, the
    /// date text it held, the part, and the serial date-time written.
    DateCellConverted {
        cell: String,
        date: String,
        part: PartName,
        serial: String,
    },
}

impl fmt::Display for SaveWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DateCellConverted {
                cell,
                date,
                part,
                serial,
            } => write!(
                f,
                "{part} {cell}: the date {date} was written as the number {serial}"
            ),
        }
    }
}

/// Turn every date cell of `worksheet` into a number cell holding its serial
/// date-time, keeping its reference and style; one warning per converted cell.
///
/// The serial is in the 1900 date system, or the 1904 one when `date1904`. A
/// date cell with no value loses its type alone; a date cell whose text is
/// not a date stays, and a Transitional write refuses it.
pub fn convert_date_cells(
    worksheet: &mut Worksheet,
    date1904: bool,
    part: &PartName,
) -> Vec<SaveWarning> {
    let mut warnings = Vec::new();
    for row in &mut worksheet.sheet_data.row {
        for cell in &mut row.cell {
            if cell.t.as_ref().is_none_or(|kind| kind.0 != "d") {
                continue;
            }
            let Some(value) = cell.value.as_mut() else {
                cell.t = None;
                continue;
            };
            let Some(serial) = cell_date(&value.value, date1904)
                .and_then(|date| serial_date_time(&date, date1904))
            else {
                continue;
            };
            let written = serial.to_string();
            let date = mem::replace(&mut value.value, written.clone());
            cell.t = None;
            warnings.push(SaveWarning::DateCellConverted {
                cell: cell.r.clone().unwrap_or_default(),
                date,
                part: part.clone(),
                serial: written,
            });
        }
    }
    warnings
}

/// The serial date-time of `date` (ECMA-376 Part 1, 18.17.4.1).
///
/// The serial counts days from the base date of the 1900 date system, or of
/// the 1904 one when `date1904`, the time of day as a fraction of a day.
/// `None` outside 0001-01-01 to 9999-12-31; a zone the date names is ignored,
/// as a workbook's dates are local.
#[must_use]
pub fn serial_date_time(date: &DateTime, date1904: bool) -> Option<f64> {
    if !(1_i32..=9999_i32).contains(&date.year) {
        return None;
    }
    let base = if date1904 { BASE_1904 } else { BASE_1900 };
    let days = days_from_civil(
        i64::from(date.year),
        i64::from(date.month),
        i64::from(date.day),
    ) - base;
    let seconds =
        (i128::from(date.hour) * 60 + i128::from(date.minute)) * 60 + i128::from(date.second);
    let total = i128::from(days) * DAY + seconds * 1_000_000_000 + i128::from(date.nanosecond);
    decimal(total, DAY).parse().ok()
}

/// A date cell's text as a date-time (ECMA-376 Part 1, 18.17.4): a date
/// alone at midnight, a time of day alone on the date system's base date, or
/// both joined by `T`; minutes, and seconds with up to nine decimal places.
fn cell_date(text: &str, date1904: bool) -> Option<DateTime> {
    let (date, time) = match text.split_once('T') {
        Some((date, time)) => (Some(date), Some(time)),
        None if text.contains(':') => (None, Some(text)),
        None => (Some(text), None),
    };
    let (year, month, day) = match date {
        Some(found) => calendar_date(found)?,
        None if date1904 => (1904_i32, 1_u8, 1_u8),
        None => (1899_i32, 12_u8, 30_u8),
    };
    let (hour, minute, second, nanosecond) = match time {
        Some(found) => time_of_day(found)?,
        None => (0, 0, 0, 0),
    };
    Some(DateTime::new(
        (year, month, day),
        (hour, minute, second, nanosecond),
        None,
    ))
}

/// `YYYY-MM-DD`: a year from 1 to 9999, a month, and a day of that month.
fn calendar_date(text: &str) -> Option<(i32, u8, u8)> {
    let mut fields = text.split('-');
    let (year_text, month_text, day_text) = (fields.next()?, fields.next()?, fields.next()?);
    let digits = |field: &str, length: usize| {
        field.len() == length && field.bytes().all(|byte| byte.is_ascii_digit())
    };
    if fields.next().is_some()
        || !digits(year_text, 4)
        || !digits(month_text, 2)
        || !digits(day_text, 2)
    {
        return None;
    }
    let year: i32 = year_text.parse().ok()?;
    let month: u8 = month_text.parse().ok()?;
    let day: u8 = day_text.parse().ok()?;
    let valid = (1_i32..=9999_i32).contains(&year)
        && (1_u8..=12_u8).contains(&month)
        && (1..=days_in_month(year, month)).contains(&day);
    valid.then_some((year, month, day))
}

/// Days from 1970-01-01 to a date of the proleptic Gregorian calendar.
const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year.rem_euclid(400);
    let shifted_month = (month + 9).rem_euclid(12);
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The days of a month in the proleptic Gregorian calendar.
const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year.rem_euclid(4) == 0
            && (year.rem_euclid(100) != 0 || year.rem_euclid(400) == 0) =>
        {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `numerator / denominator` in decimal, to `PLACES` places rounded half away
/// from zero, trailing zeros dropped.
fn decimal(numerator: i128, denominator: i128) -> String {
    let magnitude = numerator.unsigned_abs();
    let divisor = denominator.unsigned_abs();
    let whole = magnitude / divisor;
    let power = 10_u128.pow(PLACES);
    let scaled = (magnitude % divisor) * power;
    let mut fraction = scaled / divisor;
    if scaled % divisor >= divisor - divisor / 2 {
        fraction += 1;
    }
    let carried = if fraction == power { whole + 1 } else { whole };
    let kept = if fraction == power { 0 } else { fraction };
    let sign = if numerator < 0 { "-" } else { "" };
    let digits = format!("{kept:020}");
    let trimmed = digits.trim_end_matches('0');
    if trimmed.is_empty() {
        format!("{sign}{carried}")
    } else {
        format!("{sign}{carried}.{trimmed}")
    }
}

/// `hh:mm`, `hh:mm:ss`, or `hh:mm:ss.fff`: hour, minute, second, nanosecond.
fn time_of_day(text: &str) -> Option<(u8, u8, u8, u32)> {
    let mut fields = text.split(':');
    let (hour_text, minute_text) = (fields.next()?, fields.next()?);
    let seconds_text = fields.next().unwrap_or("00");
    if fields.next().is_some() {
        return None;
    }
    let (whole_seconds, fraction) = match seconds_text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (seconds_text, None),
    };
    let two_digits =
        |field: &str| field.len() == 2 && field.bytes().all(|byte| byte.is_ascii_digit());
    if !two_digits(hour_text) || !two_digits(minute_text) || !two_digits(whole_seconds) {
        return None;
    }
    let nanosecond = match fraction {
        Some(digits)
            if (1..=9).contains(&digits.len())
                && digits.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            format!("{digits:0<9}").parse().ok()?
        }
        Some(_) => return None,
        None => 0,
    };
    let hour: u8 = hour_text.parse().ok()?;
    let minute: u8 = minute_text.parse().ok()?;
    let second: u8 = whole_seconds.parse().ok()?;
    (hour < 24 && minute < 60 && second < 60).then_some((hour, minute, second, nanosecond))
}
