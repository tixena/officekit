//! The date and the time of a serial date-time, and a date written under a format code
//! (ECMA-376 Part 1, sections 18.17.4 and 18.8.31; Part 4, section 15.3.4).

#[cfg(test)]
mod tests;

use core::str::Chars;

use officekit_markup::class_rules::{BASE_1900, BASE_1904};
use officekit_xml::values::DateTime;

use super::decimal::Decimal;
use super::parse::Token;
use super::write::Writer;
use super::{Section, Shown};

/// Nanoseconds in a day.
pub const DAY: i128 = 86_400_000_000_000;

/// Nanoseconds in a second.
const NANOSECONDS: i128 = 1_000_000_000;

/// Seconds in a day (section 18.17.4.2).
const SECONDS: i128 = 86_400;

/// Days from 1970-01-01 to 0001-01-01, the lower limit of every date system (section 18.17.4.1).
const FIRST: i64 = days_from_civil(1, 1, 1);

/// Days from 1970-01-01 to 9999-12-31, the upper limit of every date system.
const LAST: i64 = days_from_civil(9999, 12, 31);

/// The most decimals of a second a code shows (section 18.8.31, `h:mm:ss.00`).
const MOST_DECIMALS: usize = 3;

/// The months, `mmmm` (section 18.8.31, "Dates and times").
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The weekdays from Sunday, `dddd`.
const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Which day a workbook's serial date-times count from (Part 1, section 18.17.4.1; Part 4,
/// section 15.3.4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DateSystem {
    /// The 1900 date system of a Strict file: day 0 is 1899-12-30.
    V1900,
    /// The 1900 date system of a Transitional file: day 1 is 1900-01-01 and day 60 is no day.
    V1900Transitional,
    /// The 1904 date system: day 0 is 1904-01-01.
    V1904,
}

impl DateSystem {
    /// The days from 1970-01-01 of the serial day `day`, when it is a day of the calendar within
    /// the system's limits: from 0001-01-01, or 1900-01-01 in a Transitional file, to 9999-12-31.
    fn civil(self, day: i64) -> Option<i64> {
        let days = match self {
            Self::V1900 => day.checked_add(BASE_1900)?,
            Self::V1904 => day.checked_add(BASE_1904)?,
            Self::V1900Transitional => match day {
                1..=59 => day + BASE_1900 + 1,
                61.. => day.checked_add(BASE_1900)?,
                _ => return None,
            },
        };
        (FIRST..=LAST).contains(&days).then_some(days)
    }

    /// Whether a time of day in the serial day `day` is within the system's limits: in a day of
    /// the calendar, or in day 0 or day 60 of a Transitional file, which no date names.
    fn holds(self, day: i64) -> bool {
        self.civil(day).is_some() || (self == Self::V1900Transitional && matches!(day, 0 | 60))
    }
}

/// A day of the proleptic Gregorian calendar.
struct Date {
    day: i64,
    month: i64,
    /// From 0 for Sunday.
    weekday: i64,
    year: i64,
}

impl Date {
    /// The day `days` from 1970-01-01, a Thursday.
    const fn from_days(days: i64) -> Self {
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let day_of_era = shifted.rem_euclid(146_097);
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        let shifted_month = (5 * day_of_year + 2) / 153;
        let month = if shifted_month < 10 {
            shifted_month + 3
        } else {
            shifted_month - 9
        };
        let year = era * 400 + year_of_era + if month <= 2 { 1 } else { 0 };
        Self {
            day: day_of_year - (153 * shifted_month + 2) / 5 + 1,
            month,
            weekday: (days + 4).rem_euclid(7),
            year,
        }
    }

    fn month_name(&self) -> &'static str {
        name(&MONTHS, self.month - 1)
    }

    fn weekday_name(&self) -> &'static str {
        name(&WEEKDAYS, self.weekday)
    }
}

/// The time of day of an instant cut into whole units, `per_second` of them to a second.
struct Clock {
    /// The units past the last whole second.
    fraction: i128,
    hour: i128,
    minute: i128,
    second: i128,
}

/// The date and the time of `serial`, a serial date-time as the file writes it (Part 1, section
/// 18.17.4).
///
/// The time is rounded to the nanosecond, a half upward. `None` when `serial` is no number, is
/// outside the system's limits, or is in day 60 or below day 1 of a Transitional file.
#[must_use = "this only returns the value"]
pub fn date_time(serial: &str, system: DateSystem) -> Option<DateTime> {
    let nanoseconds = Decimal::parse(serial)?.times_rounded(DAY.unsigned_abs())?;
    let date = Date::from_days(system.civil(i64::try_from(nanoseconds.div_euclid(DAY)).ok()?)?);
    let clock = clock(nanoseconds.rem_euclid(DAY), NANOSECONDS);
    Some(DateTime::new(
        (
            i32::try_from(date.year).ok()?,
            u8::try_from(date.month).ok()?,
            u8::try_from(date.day).ok()?,
        ),
        (
            u8::try_from(clock.hour).ok()?,
            u8::try_from(clock.minute).ok()?,
            u8::try_from(clock.second).ok()?,
            u32::try_from(clock.fraction).ok()?,
        ),
        None,
    ))
}

/// Days from 1970-01-01 to a date of the proleptic Gregorian calendar.
pub const fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let shifted_year = if month <= 2 { year - 1 } else { year };
    let era = shifted_year.div_euclid(400);
    let year_of_era = shifted_year.rem_euclid(400);
    let shifted_month = (month + 9).rem_euclid(12);
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `value` under the date and time codes of `section` in `system` (section 18.8.31, "Dates and
/// times"): rounded to the second or to the decimals of a second the section shows, a half
/// upward, before it is cut into its parts. `None` when a date code finds no date.
pub(super) fn write(section: &Section, value: &Decimal, system: DateSystem) -> Option<Shown> {
    let tokens = &section.tokens;
    let decimals = decimals(tokens);
    let per_second = 10_i128.pow(u32::try_from(decimals).ok()?);
    let per_day = SECONDS * per_second;
    let units = value.times_rounded(per_day.unsigned_abs())?;
    let serial_day = i64::try_from(units.div_euclid(per_day)).ok()?;
    if !system.holds(serial_day) {
        return None;
    }
    let dated = (0..tokens.len()).any(|at| {
        matches!(tokens[at], Token::Date('y' | 'd' | 'e', _))
            || (matches!(tokens[at], Token::Date('m', _)) && !is_minute(tokens, at))
    });
    let date = if dated {
        Some(Date::from_days(system.civil(serial_day)?))
    } else {
        None
    };
    let clock = clock(units.rem_euclid(per_day), per_second);
    let twelve = tokens.iter().any(|token| matches!(token, Token::AmPm(_)));
    let hour = if twelve {
        (clock.hour + 11).rem_euclid(12) + 1
    } else {
        clock.hour
    };
    let fraction = format!("{:0decimals$}", clock.fraction);
    let mut writer = Writer::default();
    let mut shown_decimals: Option<Chars<'_>> = None;
    for (at, token) in tokens.iter().enumerate() {
        if !matches!(token, Token::Digit(_)) {
            shown_decimals = None;
        }
        let text = match (token, &date) {
            (Token::Date('y', length), Some(on)) => year(on.year, *length >= 3),
            (Token::Date('e', length), Some(on)) => year(on.year, *length == 1),
            (Token::Date('m', length), _) if is_minute(tokens, at) => padded(clock.minute, *length),
            (Token::Date('m', length), Some(on)) => match length {
                1 | 2 => padded(on.month.into(), *length),
                3 => on.month_name()[..3].to_owned(),
                4 => on.month_name().to_owned(),
                _ => on.month_name()[..1].to_owned(),
            },
            (Token::Date('d', length), Some(on)) => match length {
                1 | 2 => padded(on.day.into(), *length),
                3 => on.weekday_name()[..3].to_owned(),
                _ => on.weekday_name().to_owned(),
            },
            (Token::Date('h', length), _) => padded(hour, *length),
            (Token::Date('s', length), _) => padded(clock.second, *length),
            (Token::Elapsed(letter, length), _) => {
                let seconds = match letter {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
                padded(units.div_euclid(per_second * seconds), *length)
            }
            (Token::AmPm(halves), _) => {
                let (morning, afternoon) = halves.split_once('/').unwrap_or((halves, halves));
                (if clock.hour < 12 { morning } else { afternoon }).to_owned()
            }
            (Token::Point, _) => {
                if at > 0 && is_second(&tokens[at - 1]) {
                    shown_decimals = Some(fraction.chars());
                }
                ".".to_owned()
            }
            (Token::Digit(_), _) => shown_decimals
                .as_mut()
                .and_then(Iterator::next)
                .map_or_else(String::new, String::from),
            (Token::Comma, _) => ",".to_owned(),
            (Token::Slash, _) => "/".to_owned(),
            (Token::Denominator(digits), _) => digits.clone(),
            (Token::Exponent(letter, plus), _) => {
                format!("{letter}{}", if *plus { '+' } else { '-' })
            }
            _ => {
                writer.token(token);
                continue;
            }
        };
        writer.text(&text);
    }
    Some(writer.shown(section.color))
}

/// `into_day` units into a day, `per_second` of them to a second, as a time of day.
const fn clock(into_day: i128, per_second: i128) -> Clock {
    let second_of_day = into_day.div_euclid(per_second);
    Clock {
        fraction: into_day.rem_euclid(per_second),
        hour: second_of_day.div_euclid(3600),
        minute: second_of_day.div_euclid(60).rem_euclid(60),
        second: second_of_day.rem_euclid(60),
    }
}

/// The decimals of a second the section shows: the most `0` after a `.` that follows a second's
/// code, at most three (section 18.8.31, `h:mm:ss.00`).
fn decimals(tokens: &[Token]) -> usize {
    (1..tokens.len())
        .filter(|at| tokens[*at] == Token::Point && is_second(&tokens[at - 1]))
        .map(|at| {
            tokens[at + 1..]
                .iter()
                .take_while(|token| matches!(token, Token::Digit(_)))
                .count()
        })
        .max()
        .unwrap_or(0)
        .min(MOST_DECIMALS)
}

const fn is_code(token: &Token) -> bool {
    matches!(token, Token::Date(..) | Token::Elapsed(..))
}

const fn is_hour(token: &Token) -> bool {
    matches!(token, Token::Date('h', _) | Token::Elapsed('h', _))
}

/// Whether the `m` or `mm` at `at` is the minute: after an hour's code or before a second's,
/// what is no code between them passed over (section 18.8.31, "Month versus minutes").
fn is_minute(tokens: &[Token], at: usize) -> bool {
    matches!(tokens[at], Token::Date('m', 1 | 2))
        && (tokens[..at]
            .iter()
            .rev()
            .find(|token| is_code(token))
            .is_some_and(is_hour)
            || tokens[at + 1..]
                .iter()
                .find(|token| is_code(token))
                .is_some_and(is_second))
}

const fn is_second(token: &Token) -> bool {
    matches!(token, Token::Date('s', _) | Token::Elapsed('s', _))
}

/// The entry `at` of `names`, or none.
fn name(names: &[&'static str], at: i64) -> &'static str {
    usize::try_from(at)
        .ok()
        .and_then(|index| names.get(index))
        .copied()
        .unwrap_or("")
}

/// `number` in at least `length` digits, zeros first; one digit is no padding.
fn padded(number: i128, length: usize) -> String {
    let width = length.min(2);
    format!("{number:0width$}")
}

/// The year, in four digits when `full`, else its last two (section 18.8.31, `yyyy` and `yy`).
fn year(year: i64, full: bool) -> String {
    if full {
        format!("{year:04}")
    } else {
        format!("{:02}", year.rem_euclid(100))
    }
}
