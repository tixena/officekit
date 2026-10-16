//! The parsers every simple type's value method calls.

use super::{DateTime, Measure, MeasureUnit};

/// `xsd:boolean`: `true`, `false`, `1`, `0`.
#[must_use]
pub fn boolean(text: &str) -> Option<bool> {
    match text {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// `xsd:integer`.
#[must_use]
pub fn integer(text: &str) -> Option<i64> {
    text.parse().ok()
}

/// `-?[0-9]+(\.[0-9]+)?` followed by `suffix`.
#[must_use]
pub fn decimal_with_suffix(text: &str, suffix: &str) -> Option<f64> {
    let number = text.strip_suffix(suffix)?;
    let digits = number.strip_prefix('-').unwrap_or(number);
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    let valid = !whole.is_empty()
        && !fraction.is_empty()
        && whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit());
    valid.then(|| number.parse().ok()).flatten()
}

/// `ST_UniversalMeasure`: `-?[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)`.
#[must_use]
pub fn measure(text: &str) -> Option<Measure> {
    let unit_text = text.get(text.len().checked_sub(2)?..)?;
    let unit = match unit_text {
        "cm" => MeasureUnit::Centimeters,
        "in" => MeasureUnit::Inches,
        "mm" => MeasureUnit::Millimeters,
        "pc" | "pi" => MeasureUnit::Picas,
        "pt" => MeasureUnit::Points,
        _ => return None,
    };
    Some(Measure {
        unit,
        value: decimal_with_suffix(text, unit_text)?,
    })
}

/// The items of an `xsd:list`, split on XML whitespace.
pub fn list(text: &str) -> impl Iterator<Item = &str> {
    text.split([' ', '\t', '\n', '\r'])
        .filter(|item| !item.is_empty())
}

/// `xsd:dateTime`.
#[must_use]
pub fn date_time(text: &str) -> Option<DateTime> {
    let (negative, rest) = text
        .strip_prefix('-')
        .map_or((false, text), |unsigned| (true, unsigned));
    let (date, time) = rest.split_once('T')?;
    let mut fields = date.splitn(3, '-');
    let (year_text, month_text, day_text) = (fields.next()?, fields.next()?, fields.next()?);
    if year_text.len() < 4 || month_text.len() != 2 || day_text.len() != 2 {
        return None;
    }
    let magnitude: i32 = year_text.parse().ok()?;
    let (clock, offset_minutes) = split_zone(time)?;
    let mut parts = clock.splitn(3, ':');
    let (hour_text, minute_text, seconds_text) = (parts.next()?, parts.next()?, parts.next()?);
    if hour_text.len() != 2 || minute_text.len() != 2 {
        return None;
    }
    let (whole_seconds, fraction) = seconds_text.split_once('.').unwrap_or((seconds_text, "0"));
    if whole_seconds.len() != 2
        || fraction.is_empty()
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let nanosecond = format!("{fraction:0<9}").get(..9)?.parse().ok()?;
    let year = if negative { -magnitude } else { magnitude };
    let value = DateTime {
        day: day_text.parse().ok()?,
        hour: hour_text.parse().ok()?,
        minute: minute_text.parse().ok()?,
        month: month_text.parse().ok()?,
        nanosecond,
        offset_minutes,
        second: whole_seconds.parse().ok()?,
        year,
    };
    let valid = (1..=12).contains(&value.month)
        && (1..=days_in_month(value.year, value.month)).contains(&value.day)
        && value.hour < 24
        && value.minute < 60
        && value.second < 60;
    valid.then_some(value)
}

fn split_zone(time: &str) -> Option<(&str, Option<i16>)> {
    if let Some(clock) = time.strip_suffix('Z') {
        return Some((clock, Some(0)));
    }
    let Some(at) = time.rfind(['+', '-']).filter(|&at| at >= 8) else {
        return Some((time, None));
    };
    let (hours, minutes) = time[at + 1..].split_once(':')?;
    let total = i16::from(hours.parse::<u8>().ok()?) * 60 + i16::from(minutes.parse::<u8>().ok()?);
    let offset = if time.as_bytes()[at] == b'-' {
        -total
    } else {
        total
    };
    Some((&time[..at], Some(offset)))
}

const fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
