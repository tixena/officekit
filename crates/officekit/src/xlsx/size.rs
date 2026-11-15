//! The lengths the standard writes for a sheet: a column's width in pixels (ECMA-376 Part 1,
//! section 18.3.1.13) and the size of a paper code (section 18.3.1.63).

mod paper;
#[cfg(test)]
mod tests;

use officekit_markup::common_simple_types::PositiveUniversalMeasurement;
use officekit_markup::spreadsheet::PageSetupSettings;
use officekit_xml::values::MeasureUnit;

use paper::PAPER_SIZES;

/// A paper's width and height, upright, in EMU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PaperSize {
    /// The paper's height.
    pub height: i64,
    /// The paper's width.
    pub width: i64,
}

impl PaperSize {
    /// A paper `width` wide and `height` high, in EMU.
    #[must_use = "this only builds the value"]
    pub const fn new(width: i64, height: i64) -> Self {
        Self { height, width }
    }
}

/// The characters a column of `pixels` holds (section 18.3.1.13).
///
/// `Truncate((pixels - 5) / digit * 100 + 0.5) / 100`, the widest digit `digit` pixels wide.
/// `None` for a digit of 0 and for a count below 0.
#[must_use = "this only returns the value"]
pub fn column_characters(pixels: u32, digit: u32) -> Option<String> {
    if digit == 0 {
        return None;
    }
    let wide = i128::from(digit);
    let hundredths = (200 * (i128::from(pixels) - 5) + wide) / (2 * wide);
    Some(decimal_text(u128::try_from(hundredths).ok()?, 100, 2))
}

/// The width in pixels of a column whose stored width is `width` (section 18.3.1.13).
///
/// `digit` is the width in pixels of the widest digit of the normal style's font. `None` for a
/// width that is no decimal number or is negative, and for a digit of 0.
#[must_use = "this only returns the value"]
pub fn column_pixels(width: &str, digit: u32) -> Option<u32> {
    if digit == 0 {
        return None;
    }
    let (numerator, denominator) = decimal(width)?;
    let wide = u128::from(digit);
    let padding = (128 / wide).checked_mul(denominator)?;
    let scaled = numerator.checked_mul(256)?.checked_add(padding)?;
    let pixels = scaled.checked_mul(wide)? / denominator.checked_mul(256)?;
    u32::try_from(pixels).ok()
}

/// The stored width of a column that holds `characters` of the widest digit (section 18.3.1.13).
///
/// `Truncate([characters * digit + 5] / digit * 256) / 256`, with every decimal it has, the widest
/// digit `digit` pixels wide. `None` for a digit of 0.
#[must_use = "this only returns the value"]
pub fn column_width(characters: u32, digit: u32) -> Option<String> {
    if digit == 0 {
        return None;
    }
    let wide = u128::from(digit);
    let scaled = (u128::from(characters) * wide + 5) * 256 / wide;
    Some(decimal_text(scaled, 256, 8))
}

/// `text`, a decimal number with no sign or a `+`, as a whole number over a power of ten.
fn decimal(text: &str) -> Option<(u128, u128)> {
    let trimmed = text.trim_matches([' ', '\t', '\n', '\r']);
    let unsigned = trimmed.strip_prefix('+').unwrap_or(trimmed);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut numerator: u128 = 0;
    for byte in whole.bytes().chain(fraction.bytes()) {
        if !byte.is_ascii_digit() {
            return None;
        }
        numerator = numerator
            .checked_mul(10)?
            .checked_add(u128::from(byte - b'0'))?;
    }
    let denominator = 10_u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    Some((numerator, denominator))
}

/// `numerator / denominator` in decimal, trailing zeros dropped; `denominator` divides ten to the
/// power `places`, so the text is exact.
fn decimal_text(numerator: u128, denominator: u128, places: usize) -> String {
    let whole = numerator / denominator;
    let power = (0..places).fold(1_u128, |power, _| power * 10);
    let fraction = numerator % denominator * power / denominator;
    let digits = format!("{fraction:0places$}");
    let trimmed = digits.trim_end_matches('0');
    if trimmed.is_empty() {
        whole.to_string()
    } else {
        format!("{whole}.{trimmed}")
    }
}

/// A positive universal measure (section 22.9.2.12) in EMU, to the nearest one.
fn measure_emu(measure: &PositiveUniversalMeasurement) -> Option<i64> {
    let per_unit: u128 = match measure.value().ok()?.unit {
        MeasureUnit::Centimeters => 360_000,
        MeasureUnit::Inches => 914_400,
        MeasureUnit::Millimeters => 36_000,
        MeasureUnit::Picas => 152_400,
        MeasureUnit::Points => 12_700,
        _ => return None,
    };
    let number = measure.0.get(..measure.0.len().checked_sub(2)?)?;
    let (numerator, denominator) = decimal(number)?;
    let doubled = numerator.checked_mul(per_unit)?.checked_mul(2)?;
    i64::try_from((doubled + denominator) / (2 * denominator)).ok()
}

/// The paper a page setup states, upright (section 18.3.1.63): its `paperWidth` and `paperHeight`
/// when it states both, else the paper of its `paperSize`, 1 when absent.
#[must_use = "this only returns the value"]
pub fn page_setup_size(setup: &PageSetupSettings) -> Option<PaperSize> {
    let stated_width = setup.paper_width.as_ref().and_then(measure_emu);
    let stated_height = setup.paper_height.as_ref().and_then(measure_emu);
    if let Some((width, height)) = stated_width.zip(stated_height) {
        return Some(PaperSize::new(width, height));
    }
    let code = setup
        .paper_size
        .as_deref()
        .map_or(Some(1), |text| text.parse().ok())?;
    paper_size(code)
}

/// The paper of the code `code` of `paperSize`, upright as section 18.3.1.63 writes it; `None`
/// for a code the section gives no size.
#[must_use = "this only returns the value"]
pub fn paper_size(code: u32) -> Option<PaperSize> {
    let index = PAPER_SIZES
        .binary_search_by_key(&code, |&(found, _, _)| found)
        .ok()?;
    let &(_, width, height) = PAPER_SIZES.get(index)?;
    Some(PaperSize::new(width, height))
}
