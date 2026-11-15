//! The red, green, blue and opacity of a colour element, with its changes applied (ECMA-376
//! Part 1, section 20.1.2.3).

mod preset;
#[cfg(test)]
mod tests;

use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{
    ColorContent, ColorTransformContent, PresetColorValueMember, StSchemeColorValMember,
};

use super::scheme::ThemeColors;
use preset::PRESET_COLORS;

/// 60 degrees in 60000ths of a degree, the unit of `ST_Angle` (section 20.1.10.3).
const SIXTY_DEGREES: i128 = 3_600_000;

/// 360 degrees in 60000ths of a degree.
const FULL_CIRCLE: i128 = 21_600_000;

/// The greatest number of percent read; a greater one changes a colour as this one does.
const PERCENT_LIMIT: i128 = 1_000_000_000_000;

/// One whole: a share is a whole number of parts of it. It is 255 × 100000, so that a component
/// of 0 to 255 and a thousandth of a percent are each a whole number of parts.
const SCALE: i128 = 25_500_000;

/// The part of a colour a change sets, multiplies or shifts (section 20.1.2.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Channel {
    Blue,
    Green,
    Hue,
    Luminance,
    Opacity,
    Red,
    Saturation,
}

/// A colour's hue in 60000ths of a degree, and its saturation and luminance as shares, those of
/// `a:hslClr` (section 20.1.2.3.13).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Hsl {
    hue: i128,
    lum: i128,
    sat: i128,
}

impl Hsl {
    /// The hue, saturation and luminance of the encoded red, green and blue of `color`.
    fn of(color: Shares) -> Self {
        let greatest = color.red.max(color.green).max(color.blue);
        let least = color.red.min(color.green).min(color.blue);
        let lum = divide(greatest + least, 2);
        let spread = greatest - least;
        if spread == 0 {
            return Self {
                hue: 0,
                lum,
                sat: 0,
            };
        }
        let sat = divide(spread * SCALE, SCALE - (greatest + least - SCALE).abs());
        let hue = if greatest == color.red {
            divide(SIXTY_DEGREES * (color.green - color.blue), spread)
        } else if greatest == color.green {
            divide(SIXTY_DEGREES * (color.blue - color.red), spread) + 2 * SIXTY_DEGREES
        } else {
            divide(SIXTY_DEGREES * (color.red - color.green), spread) + 4 * SIXTY_DEGREES
        };
        Self {
            hue: hue.rem_euclid(FULL_CIRCLE),
            lum,
            sat: clamp(sat),
        }
    }

    /// The encoded red, green and blue of this hue, saturation and luminance, with `alpha`.
    fn shares(self, alpha: i128) -> Shares {
        let chroma = product(SCALE - (2 * self.lum - SCALE).abs(), self.sat);
        let hue = self.hue.rem_euclid(FULL_CIRCLE);
        let within = hue.rem_euclid(SIXTY_DEGREES);
        let rising = divide(chroma * within, SIXTY_DEGREES);
        let falling = divide(chroma * (SIXTY_DEGREES - within), SIXTY_DEGREES);
        let (red, green, blue) = match hue / SIXTY_DEGREES {
            0 => (chroma, rising, 0),
            1 => (falling, chroma, 0),
            2 => (0, chroma, rising),
            3 => (0, falling, chroma),
            4 => (rising, 0, chroma),
            _ => (chroma, 0, falling),
        };
        let least = self.lum - divide(chroma, 2);
        Shares {
            alpha,
            blue: clamp(blue + least),
            green: clamp(green + least),
            red: clamp(red + least),
        }
    }
}

/// The values a percentage type of `DrawingML` allows: the Transitional form, a whole number of
/// thousandths of a percent, and the Strict form with a percent sign (sections 20.1.10.24,
/// 20.1.10.40, 20.1.10.45, 20.1.10.46 and 22.9.2.3, 22.9.2.9 to 22.9.2.11).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PercentKind {
    /// `ST_Percentage`: any.
    Any,
    /// `ST_FixedPercentage`: -100% to 100%.
    Fixed,
    /// `ST_PositivePercentage`: 0% and above.
    Positive,
    /// `ST_PositiveFixedPercentage`: 0% to 100%.
    PositiveFixed,
}

impl PercentKind {
    /// The thousandths of a percent `text` states; `None` when it is no value of the type.
    fn thousandths(self, text: &str) -> Option<i128> {
        let Some(number) = text.strip_suffix('%') else {
            let value: i128 = text.parse().ok()?;
            let (low, high) = match self {
                Self::Any => (i128::from(i32::MIN), i128::from(i32::MAX)),
                Self::Fixed => (-100_000, 100_000),
                Self::Positive => (0, i128::from(i32::MAX)),
                Self::PositiveFixed => (0, 100_000),
            };
            return (low..=high).contains(&value).then_some(value);
        };
        let (negative, digits) = match number.strip_prefix('-') {
            Some(rest) if matches!(self, Self::Any | Self::Fixed) => (true, rest),
            Some(_) => return None,
            None => (false, number),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
        let decimal =
            |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        let fixed = matches!(self, Self::Fixed | Self::PositiveFixed);
        let in_fixed_range = (whole == "100" || whole.len() <= 2) && fraction.len() <= 2;
        if !decimal(whole) || !decimal(fraction) || (fixed && !in_fixed_range) {
            return None;
        }
        let percent = whole
            .parse::<i128>()
            .map_or(PERCENT_LIMIT, |value| value.min(PERCENT_LIMIT));
        let place = |at: usize| {
            fraction
                .as_bytes()
                .get(at)
                .map_or(0, |byte| i128::from(byte - b'0'))
        };
        let magnitude =
            percent * 1000 + place(0) * 100 + place(1) * 10 + place(2) + i128::from(place(3) >= 5);
        Some(if negative { -magnitude } else { magnitude })
    }
}

/// A colour: red, green and blue from 0 to 255, and its opacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Rgba {
    /// The blue component.
    pub blue: u8,
    /// The green component.
    pub green: u8,
    /// The opacity in thousandths of a percent, the unit of `ST_PositiveFixedPercentage`
    /// (section 20.1.10.45): 100000 is opaque and 0 transparent.
    pub opacity: u32,
    /// The red component.
    pub red: u8,
}

impl Rgba {
    /// The colour of `red`, `green` and `blue` with `opacity` in thousandths of a percent.
    #[must_use = "this only builds the value"]
    pub const fn new(red: u8, green: u8, blue: u8, opacity: u32) -> Self {
        Self {
            blue,
            green,
            opacity,
            red,
        }
    }
}

/// What a change written inside a colour element does (section 20.1.2.3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rule {
    /// The channel shifted by the share, or by the angle for the hue.
    Add(Channel, i128),
    /// The hue turned half a circle, `a:comp` (section 20.1.2.3.7).
    Complement,
    /// Each of red, green and blue from linear light to the encoded value, `a:gamma` (section
    /// 20.1.2.3.8).
    Gamma,
    /// Each of red, green and blue the luminance of the colour in linear light, `a:gray` (section
    /// 20.1.2.3.9).
    Gray,
    /// Each of red, green and blue one whole less itself, `a:inv` (section 20.1.2.3.17).
    Inverse,
    /// Each of red, green and blue from the encoded value to linear light, `a:invGamma` (section
    /// 20.1.2.3.18).
    InverseGamma,
    /// The channel multiplied by the share.
    Multiply(Channel, i128),
    /// The channel set to the share, or to the angle for the hue.
    Set(Channel, i128),
    /// Each of red, green and blue multiplied by the share in linear light, `a:shade` (section
    /// 20.1.2.3.31).
    Shade(i128),
    /// Each of red, green and blue multiplied by the share in linear light and one whole less the
    /// share added, `a:tint` (section 20.1.2.3.34).
    Tint(i128),
}

/// A colour while its changes apply: its sRGB-encoded red, green and blue and its opacity, each a
/// share from 0 to `SCALE`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Shares {
    alpha: i128,
    blue: i128,
    green: i128,
    red: i128,
}

impl From<Rgba> for Shares {
    fn from(color: Rgba) -> Self {
        Self {
            alpha: clamp(i128::from(color.opacity) * SCALE / 100_000),
            ..Self::opaque([color.red, color.green, color.blue])
        }
    }
}

impl Shares {
    /// The colour after `rule`.
    fn applied(self, rule: Rule) -> Self {
        match rule {
            Rule::Add(channel, value) => self.with(channel, |current| current + value),
            Rule::Complement => self.in_hsl(|hsl| Hsl {
                hue: (hsl.hue + FULL_CIRCLE / 2).rem_euclid(FULL_CIRCLE),
                ..hsl
            }),
            Rule::Gamma => self.map(encoded),
            Rule::Gray => {
                let gray = encoded(divide(
                    2126 * linear(self.red) + 7152 * linear(self.green) + 722 * linear(self.blue),
                    10_000,
                ));
                Self {
                    alpha: self.alpha,
                    blue: gray,
                    green: gray,
                    red: gray,
                }
            }
            Rule::Inverse => self.map(|component| SCALE - component),
            Rule::InverseGamma => self.map(linear),
            Rule::Multiply(channel, share) => self.with(channel, |current| product(current, share)),
            Rule::Set(channel, value) => self.with(channel, |_| value),
            Rule::Shade(share) => self.map(|component| encoded(product(linear(component), share))),
            Rule::Tint(share) => {
                self.map(|component| encoded(product(linear(component), share) + SCALE - share))
            }
        }
    }

    /// The colour after `change`; the same colour for a node the schemas do not declare, and for
    /// a change whose value is absent or no number of its type.
    fn apply(self, change: &ColorTransformContent) -> Self {
        rule(change).map_or(self, |found| self.applied(found))
    }

    /// The colour with its hue, saturation and luminance changed by `change`.
    fn in_hsl(self, change: impl FnOnce(Hsl) -> Hsl) -> Self {
        change(Hsl::of(self)).shares(self.alpha)
    }

    /// The colour with each of red, green and blue put through `change`.
    fn map(self, change: impl Fn(i128) -> i128) -> Self {
        Self {
            alpha: self.alpha,
            blue: change(self.blue),
            green: change(self.green),
            red: change(self.red),
        }
    }

    /// An opaque colour of red, green and blue from 0 to 255.
    fn opaque(rgb: [u8; 3]) -> Self {
        let [red, green, blue] = rgb.map(|component| i128::from(component) * SCALE / 255);
        Self {
            alpha: SCALE,
            blue,
            green,
            red,
        }
    }

    /// Each component rounded to the nearest of 0 to 255, and the opacity to the nearest
    /// thousandth of a percent, a half upward.
    fn rgba(self) -> Rgba {
        let byte = |share: i128| u8::try_from(divide(share * 255, SCALE)).unwrap_or(u8::MAX);
        Rgba {
            blue: byte(self.blue),
            green: byte(self.green),
            opacity: u32::try_from(divide(self.alpha * 100_000, SCALE)).unwrap_or(0),
            red: byte(self.red),
        }
    }

    /// The colour with `channel` put through `change`, held between 0 and one whole, and the hue
    /// between 0 and 360 degrees (section 20.1.2.3.16).
    fn with(self, channel: Channel, change: impl FnOnce(i128) -> i128) -> Self {
        match channel {
            Channel::Blue => Self {
                blue: clamp(change(self.blue)),
                ..self
            },
            Channel::Green => Self {
                green: clamp(change(self.green)),
                ..self
            },
            Channel::Hue => self.in_hsl(|hsl| Hsl {
                hue: change(hsl.hue).clamp(0, FULL_CIRCLE),
                ..hsl
            }),
            Channel::Luminance => self.in_hsl(|hsl| Hsl {
                lum: clamp(change(hsl.lum)),
                ..hsl
            }),
            Channel::Opacity => Self {
                alpha: clamp(change(self.alpha)),
                ..self
            },
            Channel::Red => Self {
                red: clamp(change(self.red)),
                ..self
            },
            Channel::Saturation => self.in_hsl(|hsl| Hsl {
                sat: clamp(change(hsl.sat)),
                ..hsl
            }),
        }
    }
}

/// `value` held between 0 and one whole.
fn clamp(value: i128) -> i128 {
    value.clamp(0, SCALE)
}

/// The colour `content` writes, with its changes applied in the file's order (section 20.1.2.3).
///
/// `theme` gives the colour of a scheme name and `placeholder` is the colour `phClr` stands for.
/// `None` when nothing gives the colour.
#[must_use = "this only returns the value"]
pub fn color(
    content: &ColorContent,
    theme: &ThemeColors,
    placeholder: Option<Rgba>,
) -> Option<Rgba> {
    let (start, changes) = match content {
        // Hue, saturation and luminance as section 20.1.2.3.13 defines them; its example's 000080
        // for 240 degrees, 100% and 50% is not followed.
        ColorContent::HueSaturationLuminanceColorModel(hsl) => {
            let hue = hsl.hue.as_ref().and_then(|hue| hue.value().ok());
            let shares = Hsl {
                hue: hue.map_or(0, i128::from),
                lum: clamp(share(hsl.lum.as_ref(), PercentKind::Any).unwrap_or(0)),
                sat: clamp(share(hsl.sat.as_ref(), PercentKind::Any).unwrap_or(0)),
            }
            .shares(SCALE);
            (shares, &hsl.content)
        }
        ColorContent::PresetColor(preset) => {
            let rgb = preset_color(preset.val.as_ref()?.value().ok()?)?;
            (Shares::opaque(rgb), &preset.content)
        }
        ColorContent::RGBColorModelHexVariant(rgb) => (hex(rgb.val.as_ref())?, &rgb.content),
        ColorContent::RGBColorModelPercentageVariant(rgb) => {
            let component = |value| encoded(clamp(share(value, PercentKind::Any).unwrap_or(0)));
            let shares = Shares {
                alpha: SCALE,
                blue: component(rgb.b.as_ref()),
                green: component(rgb.g.as_ref()),
                red: component(rgb.r.as_ref()),
            };
            (shares, &rgb.content)
        }
        ColorContent::SchemeColor(scheme) => {
            let name = scheme.val.as_ref()?.value().ok()?;
            let named = if name == StSchemeColorValMember::PhClr {
                placeholder?
            } else {
                color(theme.color(name)?, &ThemeColors::default(), None)?
            };
            (Shares::from(named), &scheme.content)
        }
        ColorContent::SystemColor(system) => (hex(system.last_clr.as_ref())?, &system.content),
        ColorContent::Other(_) | _ => return None,
    };
    Some(changes.iter().fold(start, Shares::apply).rgba())
}

/// `numerator / denominator` rounded to the nearest whole number, a half away from zero, for a
/// positive `denominator`.
const fn divide(numerator: i128, denominator: i128) -> i128 {
    let half = denominator / 2;
    if numerator < 0 {
        (numerator - half) / denominator
    } else {
        (numerator + half) / denominator
    }
}

/// The least encoded share whose linear-light share is not below `value`: the sRGB curve from
/// linear light, the shift of `a:gamma` (section 20.1.2.3.8).
const fn encoded(value: i128) -> i128 {
    let (mut low, mut high) = (0, SCALE);
    while low < high {
        let middle = low.midpoint(high);
        if linear(middle) >= value {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    low
}

/// The greatest share whose fifth power is not above the share `value`.
const fn fifth_root(value: i128) -> i128 {
    let target = value * SCALE.pow(4);
    let (mut low, mut high) = (0, SCALE);
    while low < high {
        let middle = (low + high + 1) / 2;
        if middle.pow(5) <= target {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

/// The opaque colour of the six hexadecimal digits of `value` (section 20.1.2.3.32); `None` when
/// it is absent or no value.
fn hex(value: Option<&HexadecimalColorValue>) -> Option<Shares> {
    let rgb = value?.value().ok()?;
    let byte = |shift: u32| u8::try_from((rgb >> shift) & 0xFF).unwrap_or(u8::MAX);
    Some(Shares::opaque([byte(16), byte(8), byte(0)]))
}

/// The linear-light share of the sRGB-encoded share `value`: `value / 12.92` up to 0.04045, and
/// `((value + 0.055) / 1.055)` to the power 2.4 above it, the shift of `a:invGamma` (section
/// 20.1.2.3.18).
const fn linear(value: i128) -> i128 {
    if value * 100_000 <= 4045 * SCALE {
        return divide(value * 10_000, 129_200);
    }
    let base = divide(value * 1000 + 55 * SCALE, 1055);
    let square = product(base, base);
    product(square, fifth_root(square))
}

/// The thousandths of a percent an `ST_PositiveFixedPercentage` states, in either class (section
/// 20.1.10.45); `None` when `text` is no value of the type.
pub(super) fn positive_fixed_percentage(text: &str) -> Option<u32> {
    u32::try_from(PercentKind::PositiveFixed.thousandths(text)?).ok()
}

/// The red, green and blue section 20.1.10.48 states for `name`.
fn preset_color(name: PresetColorValueMember) -> Option<[u8; 3]> {
    let index = PRESET_COLORS
        .binary_search_by_key(&name.as_str(), |&(text, _)| text)
        .ok()?;
    PRESET_COLORS.get(index).map(|&(_, rgb)| rgb)
}

/// The product of two shares.
const fn product(left: i128, right: i128) -> i128 {
    divide(left * right, SCALE)
}

/// What `change` does; `None` for a node the schemas do not declare, and for a change whose
/// value is absent or no number of its type.
fn rule(change: &ColorTransformContent) -> Option<Rule> {
    use Channel::{Blue, Green, Hue, Luminance, Opacity, Red, Saturation};
    use ColorTransformContent as Change;
    use PercentKind::{Any, Fixed, Positive, PositiveFixed};
    Some(match change {
        Change::Alpha(alpha) => Rule::Set(Opacity, share(alpha.val.as_ref(), PositiveFixed)?),
        Change::AlphaModulation(alpha) => {
            Rule::Multiply(Opacity, share(alpha.val.as_ref(), Positive)?)
        }
        Change::AlphaOffset(alpha) => Rule::Add(Opacity, share(alpha.val.as_ref(), Fixed)?),
        Change::Blue(blue) => Rule::Set(Blue, share(blue.val.as_ref(), Any)?),
        Change::BlueModulation(blue) => Rule::Multiply(Blue, share(blue.val.as_ref(), Any)?),
        Change::BlueOffset(blue) => Rule::Add(Blue, share(blue.val.as_ref(), Any)?),
        Change::Complement(_) => Rule::Complement,
        Change::Gamma(_) => Rule::Gamma,
        Change::Gray(_) => Rule::Gray,
        Change::Green(green) => Rule::Set(Green, share(green.val.as_ref(), Any)?),
        Change::GreenModulation(green) => Rule::Multiply(Green, share(green.val.as_ref(), Any)?),
        Change::GreenOffset(green) => Rule::Add(Green, share(green.val.as_ref(), Any)?),
        Change::Hue(hue) => Rule::Set(Hue, i128::from(hue.val.as_ref()?.value().ok()?)),
        Change::HueModulate(hue) => Rule::Multiply(Hue, share(hue.val.as_ref(), Positive)?),
        Change::HueOffset(hue) => Rule::Add(Hue, i128::from(hue.val.as_ref()?.value().ok()?)),
        Change::Inverse(_) => Rule::Inverse,
        Change::InverseGamma(_) => Rule::InverseGamma,
        Change::Luminance(lum) => Rule::Set(Luminance, share(lum.val.as_ref(), Any)?),
        // Half the luminance at 50%, as section 20.1.2.3.20 says; its example's 007500 is not.
        Change::LuminanceModulation(lum) => {
            Rule::Multiply(Luminance, share(lum.val.as_ref(), Any)?)
        }
        Change::LuminanceOffset(lum) => Rule::Add(Luminance, share(lum.val.as_ref(), Any)?),
        Change::Red(red) => Rule::Set(Red, share(red.val.as_ref(), Any)?),
        Change::RedModulation(red) => Rule::Multiply(Red, share(red.val.as_ref(), Any)?),
        Change::RedOffset(red) => Rule::Add(Red, share(red.val.as_ref(), Any)?),
        Change::Saturation(sat) => Rule::Set(Saturation, share(sat.val.as_ref(), Any)?),
        Change::SaturationModulation(sat) => {
            Rule::Multiply(Saturation, share(sat.val.as_ref(), Any)?)
        }
        Change::SaturationOffset(sat) => Rule::Add(Saturation, share(sat.val.as_ref(), Any)?),
        Change::Shade(shade) => Rule::Shade(share(shade.val.as_ref(), PositiveFixed)?),
        Change::Tint(tint) => Rule::Tint(share(tint.val.as_ref(), PositiveFixed)?),
        Change::Other(_) | _ => return None,
    })
}

/// The share a percentage of the type `kind` states, read from its text; `None` when it is
/// absent or no value of the type.
fn share<T>(value: Option<&T>, kind: PercentKind) -> Option<i128>
where
    T: AsRef<str>,
{
    Some(kind.thousandths(value?.as_ref())? * SCALE / 100_000)
}
