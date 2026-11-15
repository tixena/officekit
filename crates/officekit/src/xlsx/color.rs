//! The red, green and blue of a colour element of a cell format (ECMA-376 Part 1, sections
//! 18.8.19 and 18.8.27).
//!
//! ```
//! use officekit::drawing::color::Rgba;
//! use officekit::parts::kinds::SpreadsheetKind;
//! use officekit::xlsx::Xlsx;
//! use officekit::xlsx::color::{SheetColor, color};
//! use officekit_markup::spreadsheet::ForegroundColor;
//!
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let xlsx = Xlsx::new(SpreadsheetKind::Workbook)?;
//! let mut fill = ForegroundColor::default();
//! fill.indexed = Some("10".to_owned());
//! fill.tint = Some("-0.5".to_owned());
//! let found = color(&SheetColor::from(&fill), &xlsx.theme_colors()?, &xlsx.palette()?);
//! assert_eq!(found, Some(Rgba::new(128, 0, 0, 100_000)));
//! # Ok(())
//! # }
//! ```

mod palette;
#[cfg(test)]
mod tests;

use officekit_markup::spreadsheet::{
    BackgroundColor, Colors, DataBarColor, ForegroundColor, SheetTabColor,
};
use officekit_xml::values::parse::boolean;

use crate::drawing::color::{self as drawing_color, Rgba};
use crate::drawing::scheme::{ThemeColors, ThemeSlot};
use palette::INDEXED_COLORS;

/// `SheetColor::from` for each type of the typed tree that holds a colour element.
macro_rules! sheet_color_from {
    ($($node:ty),+) => {$(
        impl<'color> From<&'color $node> for SheetColor<'color> {
            fn from(node: &'color $node) -> Self {
                Self {
                    auto: node.auto.as_deref().and_then(boolean).unwrap_or(false),
                    indexed: node.indexed.as_deref().and_then(|text| text.parse().ok()),
                    rgb: node.rgb.as_ref().and_then(|value| rgb(value.as_ref())),
                    theme: node.theme.as_deref().and_then(|text| text.parse().ok()),
                    tint: node.tint.as_deref(),
                }
            }
        }
    )+};
}

/// The decimal places a tint is read to: a tint is a whole number of `10^-24`.
const TINT_PLACES: i64 = 24;

/// A tint of 1 in units of `10^-24`.
const TINT_ONE: i128 = 1_000_000_000_000_000_000_000_000;

/// The indexed colours of a workbook: its own, or the standard's 64 (section 18.8.27).
#[derive(Clone, Debug, Default)]
pub struct Palette {
    own: Option<Vec<Option<[u8; 3]>>>,
}

impl Palette {
    /// The red, green and blue of `index`: the workbook's own `rgbColor` at it when it states
    /// `indexedColors`, else the standard's. `None` from 64 up, the system colours among them.
    #[must_use = "this only returns the value"]
    pub fn get(&self, index: u32) -> Option<[u8; 3]> {
        let at = usize::try_from(index).ok()?;
        if at >= INDEXED_COLORS.len() {
            return None;
        }
        self.own.as_ref().map_or_else(
            || INDEXED_COLORS.get(at).copied(),
            |own| own.get(at).copied().flatten(),
        )
    }

    /// The palette `colors`, the styles part's `colors`, states; the standard's with none.
    #[must_use = "this only builds the value"]
    pub fn new(colors: Option<&Colors>) -> Self {
        let own = colors
            .and_then(|stated| stated.color_indexes.as_deref())
            .map(|indexes| {
                indexes
                    .rgb_color
                    .iter()
                    .map(|entry| rgb(entry.rgb.as_ref()?.as_ref()))
                    .collect()
            });
        Self { own }
    }
}

/// What a colour element states (section 18.8.19). Each value is `None` when the element does not
/// state it, or states no number of its type.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct SheetColor<'color> {
    /// The attribute `auto`.
    pub auto: bool,
    /// The attribute `indexed`: an index into the palette.
    pub indexed: Option<u32>,
    /// The attribute `rgb`: red, green and blue, from its last six digits.
    pub rgb: Option<[u8; 3]>,
    /// The attribute `theme`: the index of a theme colour.
    pub theme: Option<u32>,
    /// The attribute `tint`, as the file writes it.
    pub tint: Option<&'color str>,
}

sheet_color_from!(
    BackgroundColor,
    DataBarColor,
    ForegroundColor,
    SheetTabColor
);

/// The colour `stated` names, opaque, with its tint applied (section 18.8.19): its `rgb`, else its
/// `theme` colour, else its `indexed` one. `None` when it names none; `auto` names none.
#[must_use = "this only returns the value"]
pub fn color(stated: &SheetColor<'_>, theme: &ThemeColors, palette: &Palette) -> Option<Rgba> {
    let base = if let Some(rgb) = stated.rgb {
        rgb
    } else if let Some(index) = stated.theme {
        let element = theme.slot(theme_slot(index)?)?;
        let found = drawing_color::color(element, theme, None)?;
        [found.red, found.green, found.blue]
    } else {
        palette.get(stated.indexed?)?
    };
    let [red, green, blue] = stated
        .tint
        .and_then(tint)
        .map_or(base, |amount| tinted(base, amount));
    Some(Rgba::new(red, green, blue, 100_000))
}

/// The red, green and blue of six hexadecimal digits, or of the last six of eight; the first two
/// of eight are no opacity, since section 18.8.27 writes opaque red `00FF0000`.
fn rgb(text: &str) -> Option<[u8; 3]> {
    let digits = text.get(text.len().checked_sub(6)?..)?;
    let valid = matches!(text.len(), 6 | 8) && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    let value = u32::from_str_radix(digits, 16).ok().filter(|_| valid)?;
    let byte = |shift: u32| u8::try_from((value >> shift) & 0xFF).ok();
    Some([byte(16)?, byte(8)?, byte(0)?])
}

/// The theme colour a `theme` index names. The index counts the scheme's colours with its first
/// two pairs turned about, as files write it: 0 `lt1`, 1 `dk1`, 2 `lt2`, 3 `dk2`, then the order of
/// section 20.1.6.2.
#[must_use = "this only returns the value"]
pub const fn theme_slot(index: u32) -> Option<ThemeSlot> {
    Some(match index {
        0 => ThemeSlot::Light1,
        1 => ThemeSlot::Dark1,
        2 => ThemeSlot::Light2,
        3 => ThemeSlot::Dark2,
        4 => ThemeSlot::Accent1,
        5 => ThemeSlot::Accent2,
        6 => ThemeSlot::Accent3,
        7 => ThemeSlot::Accent4,
        8 => ThemeSlot::Accent5,
        9 => ThemeSlot::Accent6,
        10 => ThemeSlot::Hyperlink,
        11 => ThemeSlot::FollowedHyperlink,
        _ => return None,
    })
}

/// The tint `text` writes, an `xsd:double`, in units of `10^-24` rounded half upward and held
/// between -1 and 1; `None` when it is no decimal number.
fn tint(text: &str) -> Option<i128> {
    let (negative, unsigned) = text.strip_prefix('-').map_or_else(
        || (false, text.strip_prefix('+').unwrap_or(text)),
        |rest| (true, rest),
    );
    let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (unsigned, 0_i32),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return None;
    }
    // The digits at or above 10^-24, then the one below it, which rounds.
    let kept = i64::try_from(whole.len()).ok()? + i64::from(exponent) + TINT_PLACES;
    let limit = 10 * TINT_ONE;
    let mut value: i128 = 0;
    let mut count: i64 = 0;
    for byte in whole.bytes().chain(fraction.bytes()) {
        let digit = i128::from(byte - b'0');
        if count == kept {
            value += i128::from(digit >= 5);
            break;
        }
        if count > kept {
            break;
        }
        value = (value * 10 + digit).min(limit);
        count += 1;
    }
    let missing = kept - count;
    if missing > 0 && value > 0 {
        value = u32::try_from(missing)
            .ok()
            .and_then(|power| 10_i128.checked_pow(power))
            .and_then(|scale| value.checked_mul(scale))
            .map_or(limit, |scaled| scaled.min(limit));
    }
    let held = value.min(TINT_ONE);
    Some(if negative { -held } else { held })
}

/// `rgb` with its lightness changed by `amount`, a tint in units of `10^-24`, and its hue and
/// saturation kept (section 18.8.19), worked as fractions and rounded once, a half upward.
fn tinted(rgb: [u8; 3], amount: i128) -> [u8; 3] {
    let [red, green, blue] = rgb.map(i128::from);
    let greatest = red.max(green).max(blue);
    let least = red.min(green).min(blue);
    let sum = greatest + least;
    // The new lightness, a fraction of 1, is `lightness / whole`.
    let whole = 510 * TINT_ONE;
    let lightness = if amount < 0 {
        sum * (TINT_ONE + amount)
    } else {
        sum * (TINT_ONE - amount) + 510 * amount
    };
    let component = |value: i128| {
        let (numerator, denominator) = if greatest == least {
            (255 * lightness, whole)
        } else {
            // A component is the lightness plus its share of the chroma, which the hue fixes and
            // the saturation scales: `(2c - M - m) / 2(255 - |M + m - 255|)` of the new chroma.
            let room = 255 - (sum - 255).abs();
            let chroma = whole - (2 * lightness - whole).abs();
            (
                255 * (2 * room * lightness + chroma * (2 * value - sum)),
                2 * room * whole,
            )
        };
        let rounded = (2 * numerator + denominator).div_euclid(2 * denominator);
        u8::try_from(rounded.clamp(0, 255)).unwrap_or(u8::MAX)
    };
    [component(red), component(green), component(blue)]
}
