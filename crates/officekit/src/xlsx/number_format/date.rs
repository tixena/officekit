//! The date and the time of a serial date-time, and a date written under a format code
//! (ECMA-376 Part 1, sections 18.17.4 and 18.8.31; Part 4, section 15.3.4).

mod calendars;
mod names;
mod system_format;
#[cfg(test)]
mod tests;

use core::str::Chars;

use officekit_markup::class_rules::{BASE_1900, BASE_1904};
use officekit_xml::values::DateTime;

use self::calendars::{HIJRI_FIRST, HIJRI_LEAP_YEARS, JAPANESE, ROC};
use self::names::{LOCALES, NAMES};
use super::decimal::Decimal;
use super::parse::Token;
use super::write::Writer;
use super::{Section, Shown, language_tag};
use crate::docx::fonts::likely_subtags;

pub(super) use self::system_format::{is_system_format, system_tag};

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

/// The years of a cycle of the tabular Hijri calendar, of which [`HIJRI_LEAP_YEARS`] have 355
/// days and the others 354 (Reingold and Dershowitz, "Calendrical Calculations", the arithmetic
/// Islamic calendar).
const HIJRI_CYCLE: i64 = 30;

/// The days of a common year of the tabular Hijri calendar.
const HIJRI_YEAR: i64 = 354;

/// The language of the Hijri months `b2` writes in a section with no language: section 18.8.31
/// writes the Gregorian ones in English.
const HIJRI_LANGUAGE: &str = "en";

/// The language of a system long date or time when the caller names none CLDR has (section
/// 18.8.31, "International Considerations").
const SYSTEM_LANGUAGE: &str = "en";

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

/// An era of a calendar, from Unicode CLDR (section 18.8.31, "International Considerations").
struct Era {
    backward: bool,
    /// Days from 1970-01-01 to its first day; `i64::MIN` for an era with no first day.
    first: i64,
    /// What `g`, `gg` and `ggg` write.
    names: [&'static str; 3],
    /// The Gregorian year its years count from: its first, or its last when `backward`.
    year: i64,
}

/// A locale section 18.8.31's "International Considerations" gives the era codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Locale {
    /// ja-jp: the Japanese emperor eras.
    Japan,
    /// zh-tw: the Republic of China's eras.
    Taiwan,
}

impl Locale {
    /// The era of the day `days` from 1970-01-01.
    fn era(self, days: i64) -> Option<&'static Era> {
        let eras: &'static [Era] = match self {
            Self::Japan => &JAPANESE,
            Self::Taiwan => &ROC,
        };
        eras.iter().rev().find(|era| era.first <= days)
    }

    /// The locale of the BCP 47 tag `tag`, by its language and its region, or the region CLDR's
    /// likely subtags give the tag when it names none.
    fn of(tag: &str) -> Option<Self> {
        let Subtags {
            language,
            script,
            region: named,
        } = Subtags::of(tag)?;
        let region = named.or_else(|| {
            let id = script.map_or_else(|| language.clone(), |own| format!("{language}_{own}"));
            let full = likely_subtags(&id).or_else(|| likely_subtags(&language))?;
            full.rsplit('_').next().map(str::to_owned)
        })?;
        match (language.as_str(), region.as_str()) {
            ("ja", "JP") => Some(Self::Japan),
            ("zh", "TW") => Some(Self::Taiwan),
            _ => None,
        }
    }
}

/// The language, script and region subtags of a BCP 47 tag before its first singleton: the
/// language in lower case, the script in title case, the region in upper case.
struct Subtags {
    language: String,
    region: Option<String>,
    script: Option<String>,
}

impl Subtags {
    fn of(tag: &str) -> Option<Self> {
        let mut subtags = tag.split(['-', '_']).take_while(|subtag| subtag.len() > 1);
        let language = subtags.next()?.to_ascii_lowercase();
        let rest: Vec<&str> = subtags.collect();
        let script = rest
            .iter()
            .find(|subtag| {
                subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic())
            })
            .map(|own| {
                let (first, others) = own.split_at(1);
                format!(
                    "{}{}",
                    first.to_ascii_uppercase(),
                    others.to_ascii_lowercase()
                )
            });
        let region = rest
            .iter()
            .find(|subtag| {
                (subtag.len() == 2 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()))
                    || (subtag.len() == 3 && subtag.bytes().all(|byte| byte.is_ascii_digit()))
            })
            .map(|subtag| subtag.to_ascii_uppercase());
        Some(Self {
            language,
            region,
            script,
        })
    }
}

/// The month and weekday names of a locale of Unicode CLDR, its lists in [`NAMES`] (section
/// 18.8.31, `mmm`, `mmmm`, `mmmmm`, `ddd` and `dddd`).
#[derive(Clone, Copy)]
struct Names(&'static [u16; 8]);

impl Names {
    /// The name of month `month`, from 1, of the Gregorian or the Hijri calendar in `width`: 0
    /// narrow, 1 abbreviated, 2 wide.
    fn month(self, hijri: bool, width: usize, month: i64) -> &'static str {
        let list = self.0.get(if hijri { 5 } else { 0 } + width);
        list.and_then(|at| names::MONTHS.get(usize::from(*at)))
            .map_or("", |months| name(months, month - 1))
    }

    /// The names of CLDR's locale of the BCP 47 tag `tag`, as [`cldr_locale`] finds it.
    fn of(tag: &str) -> Option<Self> {
        cldr_locale(tag).and_then(|at| NAMES.get(at)).map(Self)
    }

    /// The name of `weekday`, from 0 for Sunday, in `width`: 0 abbreviated, 1 wide.
    fn weekday(self, width: usize, weekday: i64) -> &'static str {
        let list = self.0.get(3 + width);
        list.and_then(|at| names::WEEKDAYS.get(usize::from(*at)))
            .map_or("", |weekdays| name(weekdays, weekday))
    }
}

/// A day of the proleptic Gregorian calendar, or of the tabular Hijri calendar.
struct Date {
    day: i64,
    /// Days from 1970-01-01.
    days: i64,
    hijri: bool,
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
            days,
            hijri: false,
            month,
            weekday: (days + 4).rem_euclid(7),
            year,
        }
    }

    /// The day `days` from 1970-01-01 in CLDR's `islamic-tbla`, the tabular Hijri calendar: odd
    /// months of 30 days, even months of 29, the twelfth of 30 in a leap year; `None` before its
    /// first day.
    fn hijri(days: i64) -> Option<Self> {
        let since = days.checked_sub(HIJRI_FIRST).filter(|since| *since >= 0)?;
        let leap = |year: i64| HIJRI_LEAP_YEARS.contains(&((year - 1).rem_euclid(HIJRI_CYCLE) + 1));
        let cycle = HIJRI_CYCLE * HIJRI_YEAR + i64::try_from(HIJRI_LEAP_YEARS.len()).ok()?;
        let mut year = since.div_euclid(cycle) * HIJRI_CYCLE + 1;
        let mut rest = since.rem_euclid(cycle);
        while rest >= HIJRI_YEAR + i64::from(leap(year)) {
            rest -= HIJRI_YEAR + i64::from(leap(year));
            year += 1;
        }
        let length =
            |month: i64| 29 + i64::from(month.rem_euclid(2) == 1 || (month == 12 && leap(year)));
        let mut month = 1;
        while rest >= length(month) {
            rest -= length(month);
            month += 1;
        }
        Some(Self {
            day: rest + 1,
            days,
            hijri: true,
            month,
            weekday: (days + 4).rem_euclid(7),
            year,
        })
    }

    /// The month's name in `names`: `mmm` when `length` is 3, `mmmm` when 4, `mmmmm` when more;
    /// with no names, the Gregorian ones in English and the Hijri ones in CLDR's English.
    fn month_text(&self, length: usize, names: Option<Names>) -> String {
        let width = match length {
            3 => 1,
            4 => 2,
            _ => 0,
        };
        let chosen = names.or_else(|| self.hijri.then(|| Names::of(HIJRI_LANGUAGE)).flatten());
        if let Some(own) = chosen {
            return own.month(self.hijri, width, self.month).to_owned();
        }
        let full = name(&MONTHS, self.month - 1);
        match length {
            3 => full[..3].to_owned(),
            4 => full.to_owned(),
            _ => full[..1].to_owned(),
        }
    }

    /// The weekday's name in `names`: `ddd` when `length` is 3, `dddd` when more; with no
    /// names, in English.
    fn weekday_text(&self, length: usize, names: Option<Names>) -> String {
        let width = usize::from(length > 3);
        if let Some(own) = names {
            return own.weekday(width, self.weekday).to_owned();
        }
        let full = name(&WEEKDAYS, self.weekday);
        if length == 3 { &full[..3] } else { full }.to_owned()
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

/// `value` under `section` in `system`; a system long date or time by CLDR's pattern of
/// `language`, or of English when CLDR has no locale of it (section 18.8.31).
pub(super) fn write(
    section: &Section,
    value: &Decimal,
    system: DateSystem,
    language: Option<&str>,
) -> Option<Shown> {
    if !section.language.is_some_and(is_system_format) {
        return write_codes(section, value, system, language);
    }
    let (tag, at) = language
        .and_then(|tag| Some((tag, cldr_locale(tag)?)))
        .or_else(|| Some((SYSTEM_LANGUAGE, cldr_locale(SYSTEM_LANGUAGE)?)))?;
    let own = system_format::section(section, at)?;
    write_codes(&own, value, system, Some(tag))
}

/// `value` under the date and time codes of `section` in `system` (section 18.8.31, "Dates and
/// times"): rounded to the second or to the decimals of a second the section shows, a half
/// upward, before it is cut into its parts. `None` when a date code finds no date.
fn write_codes(
    section: &Section,
    value: &Decimal,
    system: DateSystem,
    language: Option<&str>,
) -> Option<Shown> {
    let tokens = &section.tokens;
    let (hijri, tag) = calendar(section, language);
    let locale = tag.filter(|_| !hijri).and_then(Locale::of);
    let names = tag.and_then(Names::of);
    let decimals = decimals(tokens);
    let per_second = 10_i128.pow(u32::try_from(decimals).ok()?);
    let per_day = SECONDS * per_second;
    let units = value.times_rounded(per_day.unsigned_abs())?;
    let serial_day = i64::try_from(units.div_euclid(per_day)).ok()?;
    if !system.holds(serial_day) {
        return None;
    }
    let date = if is_dated(tokens, locale.is_some()) {
        let days = system.civil(serial_day)?;
        Some(if hijri {
            Date::hijri(days)?
        } else {
            Date::from_days(days)
        })
    } else {
        None
    };
    let era = locale.zip(date.as_ref()).and_then(|(place, on)| {
        let era = place.era(on.days)?;
        Some((place, era, on))
    });
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
        if let (Some((place, of, on)), Token::Date(letter @ ('g' | 'e' | 'r'), length)) =
            (era, token)
        {
            let after_g = tokens[..at]
                .iter()
                .any(|before| matches!(before, Token::Date('g', _)));
            writer.text(&era_text(place, of, on, (*letter, *length), after_g));
            continue;
        }
        let text = match (token, &date) {
            (Token::Date('y', length), Some(on)) => year(on.year, *length >= 3),
            (Token::Date('e', length), Some(on)) => year(on.year, *length == 1),
            (Token::Date('m', length), _) if is_minute(tokens, at) => padded(clock.minute, *length),
            (Token::Date('m', length), Some(on)) => match length {
                1 | 2 => padded(on.month.into(), *length),
                _ => on.month_text(*length, names),
            },
            (Token::Date('d', length), Some(on)) => match length {
                1 | 2 => padded(on.day.into(), *length),
                _ => on.weekday_text(*length, names),
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

/// Whether `section` writes the Hijri calendar, its last calendar code being `b2`, and the
/// language tag of its `[$-<language info>]`, or `language` when it names none (section 18.8.31).
fn calendar<'tag>(section: &Section, language: Option<&'tag str>) -> (bool, Option<&'tag str>) {
    let hijri = section
        .tokens
        .iter()
        .rev()
        .find(|token| matches!(token, Token::Calendar(_)))
        == Some(&Token::Calendar(2));
    (hijri, section.language.map_or(language, language_tag))
}

/// The index in [`LOCALES`] of CLDR's locale of the BCP 47 tag `tag`: of its language, script and
/// region, the script CLDR's likely subtags give when it names none; CLDR's ids leave out a
/// language's own script.
fn cldr_locale(tag: &str) -> Option<usize> {
    let Subtags {
        language,
        region,
        script: named,
    } = Subtags::of(tag)?;
    let likely = |id: &str| -> Option<&'static str> { likely_subtags(id)?.split('_').nth(1) };
    let own = likely(&language);
    let script = named
        .as_deref()
        .or_else(|| likely(&format!("{language}_{}", region.as_ref()?)))
        .or(own);
    let plain = script == own;
    let mut ids = Vec::new();
    if let (Some(writing), Some(place)) = (script, &region) {
        ids.push(format!("{language}_{writing}_{place}"));
    }
    if let (true, Some(place)) = (plain, &region) {
        ids.push(format!("{language}_{place}"));
    }
    if let Some(writing) = script {
        ids.push(format!("{language}_{writing}"));
    }
    if plain {
        ids.push(language.clone());
    }
    ids.iter()
        .find_map(|id| LOCALES.binary_search(&id.as_str()).ok())
}

/// What the era code `letter`, `length` times, writes of `on` in `era` of `locale` (section
/// 18.8.31, "International Considerations"); `after_g` when a `g` code comes before it.
fn era_text(
    locale: Locale,
    era: &Era,
    on: &Date,
    (letter, length): (char, usize),
    after_g: bool,
) -> String {
    let year = if era.backward {
        era.year - on.year + 1
    } else {
        on.year - era.year + 1
    };
    let name = era.names[length.clamp(1, 3) - 1];
    match (locale, letter) {
        (_, 'g') => name.to_owned(),
        (Locale::Japan, 'e') => padded(year.into(), length),
        (Locale::Japan, _) if length == 1 => padded(year.into(), 2),
        (Locale::Japan, _) => format!("{}{}", era.names[2], padded(year.into(), 2)),
        (Locale::Taiwan, _) => {
            let shown = if after_g || !era.backward {
                year
            } else {
                on.year
            };
            padded(shown.into(), if letter == 'e' { length } else { 1 })
        }
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

/// Whether a code of `tokens` writes a part of the date; `g`, `r` and `rr` do when `eras`.
fn is_dated(tokens: &[Token], eras: bool) -> bool {
    (0..tokens.len()).any(|at| {
        matches!(tokens[at], Token::Date('y' | 'd' | 'e', _))
            || (matches!(tokens[at], Token::Date('m', _)) && !is_minute(tokens, at))
            || (eras && matches!(tokens[at], Token::Date('g' | 'r', _)))
    })
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
