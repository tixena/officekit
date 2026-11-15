//! A number under the general format (ECMA-376 Part 1, section 18.8.30, "General" Format).

#[cfg(test)]
mod tests;

use super::decimal::{Decimal, count};

/// The most characters the general format writes, the sign not counted.
pub(super) const MOST: u32 = 11;

/// A number under the general format (section 18.8.30).
///
/// `number` is the text of an XML Schema double, written in at most `characters` characters and
/// never more than 11, the sign not counted; `None` when it does not fit or is no number.
#[must_use = "this only returns the value"]
pub fn general(number: &str, characters: u32) -> Option<String> {
    let value = Decimal::parse(number)?;
    let text = unsigned(&value.abs(), characters)?;
    Some(if value.is_negative() {
        format!("-{text}")
    } else {
        text
    })
}

/// The magnitude `value` under the general format in at most `characters` characters: whole
/// with as many decimals as fit, or else with an exponent (section 18.8.30, "Numbers").
pub(super) fn unsigned(value: &Decimal, characters: u32) -> Option<String> {
    let width = i64::from(characters.min(MOST));
    let Some(first) = value.exponent() else {
        return (width > 0).then(|| "0".to_owned());
    };
    if first >= 0 {
        let whole = first + 1;
        if whole <= width {
            let text = value.rounded((width - whole - 1).max(0)).fixed();
            if count(text.len()) <= width {
                return Some(text);
            }
        }
        return scientific(value, first, width);
    }
    let rounded = value.rounded(width - 2);
    if width < 3 || rounded.is_zero() {
        return scientific(value, first, width);
    }
    let text = rounded.fixed();
    // More than three zeros after the point: the exponent shows more digits (condition 4).
    if rounded != *value && first < -4 {
        return scientific(value, first, width).or(Some(text));
    }
    Some(text)
}

/// `value`, whose first digit is worth ten to `first`, as `d.dddE+xx` with as many decimals as
/// fit in `width`, trailing zeros dropped; `None` when even `dE+xx` does not fit.
fn scientific(value: &Decimal, first: i64, width: i64) -> Option<String> {
    let exponent = |power: i64| {
        let sign = if power < 0 { '-' } else { '+' };
        format!("E{sign}{:02}", power.unsigned_abs())
    };
    let written = exponent(first);
    let room = width - count(written.len()) - 1;
    if room < 0 {
        return None;
    }
    let mantissa = value.scaled(-first).rounded((room - 1).max(0));
    let text = if mantissa.exponent() == Some(0) {
        format!("{}{written}", mantissa.fixed())
    } else {
        format!("1{}", exponent(first + 1))
    };
    (count(text.len()) <= width).then_some(text)
}
