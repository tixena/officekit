//! What a Word border, a Word shading and a table measurement state, with the standard's defaults
//! (ECMA-376 Part 1, sections 17.3.4, 17.3.5 and 17.4.87).

#[cfg(test)]
mod tests;

use officekit_markup::UserData;
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::drawing::StSchemeColorValMember;
use officekit_markup::wordprocessing::{
    BorderStyles, BorderStylesMember, BottomBorderWith, ColorValue, ColorValueMember,
    LeftBorderWith, LeftParagraphBorderTblWidthWith, LeftParagraphBorderWith,
    MeasurementInEighthsOfAPoint, MeasurementInPoints, MeasurementOrPercentageValue,
    MeasurementOrPercentageValueMember, ParagraphBorderAboveIdenticalParagraphsTblWidthWith,
    ParagraphBorderAboveIdenticalParagraphsWith,
    ParagraphBorderBelowIdenticalParagraphsTblWidthWith,
    ParagraphBorderBelowIdenticalParagraphsWith, ParagraphBorderBetweenFacingPagesWith,
    ParagraphBorderBetweenIdenticalParagraphsWith, ParagraphShadingWith,
    PreferredTableCellWidthWith, PreferredTableWidthWith, PreferredWidthAfterTableRowWith,
    PreferredWidthBeforeTableRowWith, RightBorderWith, RightParagraphBorderTblWidthWith,
    RightParagraphBorderWith, ShadingPatternsMember, TableCellLeadingEdgeBorderTblWidthWith,
    TableCellLeadingEdgeBorderWith, TableCellTrailingMarginExceptionTblWidthWith,
    TableCellTrailingMarginExceptionWith, TableIndentFromLeadingMarginWith,
    TableInsideHorizontalEdgesBorderWith, TableInsideVerticalEdgesBorderWith,
    TableRowCellSpacingWith, TableWidthUnits, TableWidthUnitsMember, TextBorderWith, ThemeColor,
    ThemeColorMember, TopBorderWith, TopLeftBottomRightDiagonalBorderWith,
    TopRightBottomLeftDiagonalBorderWith, TwoDigitHexadecimalValue,
};
use officekit_xml::values::MeasureUnit;

use crate::drawing::color::{color, word_shade, word_tint};
use crate::drawing::scheme::ThemeColors;

/// One reading of a border for every border element type of the typed tree.
macro_rules! borders {
    ($($kind:ident),+ $(,)?) => {$(
        impl<X: UserData> From<&$kind<X>> for BorderLine {
            fn from(border: &$kind<X>) -> Self {
                Self::read(&BorderAttributes {
                    color: border.color.as_ref(),
                    frame: border.frame.as_ref(),
                    shadow: border.shadow.as_ref(),
                    space: border.space.as_ref(),
                    sz: border.sz.as_ref(),
                    theme_color: border.theme_color.as_ref(),
                    theme_shade: border.theme_shade.as_ref(),
                    theme_tint: border.theme_tint.as_ref(),
                    val: border.val.as_ref(),
                })
            }
        }
    )+};
}

/// One reading of a table measurement for every `CT_TblWidth` element type of the typed tree.
macro_rules! table_lengths {
    ($($kind:ident),+ $(,)?) => {$(
        impl<X: UserData> From<&$kind<X>> for TableLength {
            fn from(measurement: &$kind<X>) -> Self {
                Self::read(measurement.w.as_ref(), measurement.r#type.as_ref())
            }
        }
    )+};
}

/// Fraction digits past this many are not read; they cannot move a whole result.
const FRACTION_DIGITS: usize = 18;

/// What a border element states (section 17.3.4), each attribute with the standard's default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BorderLine {
    /// `w:color`, `auto` when absent, with `w:themeColor` and its tint or shade.
    pub color: WordColor,
    /// `w:frame`: "If this attribute is omitted, then the border is not given any frame effect."
    pub frame: Stated<bool>,
    /// `w:shadow`: "If this attribute is omitted, then the border is not given the shadow effect."
    pub shadow: Stated<bool>,
    /// `w:sz`, in eighths of a point for a line border and in points for an art border; "The
    /// default value of this attribute is application-defined", so an absent one is unstated.
    pub size: Stated<u64>,
    /// `w:space` in points, 0 when absent (the schema's default).
    pub space: Stated<u64>,
    /// `w:val`, which `CT_Border` requires: the text gives no meaning to its absence.
    pub style: Stated<BorderStyle>,
}

impl BorderLine {
    /// Whether the border draws: not for `nil` and `none` ("no border shall be applied", section
    /// 17.18.2), yes for a line or an art border; `None` when the style is unstated.
    #[must_use = "this only returns the value"]
    pub const fn draws(&self) -> Option<bool> {
        match self.style {
            Stated::Value(BorderStyle::NoBorder) => Some(false),
            Stated::Value(BorderStyle::Art(_) | BorderStyle::Line(_)) => Some(true),
            Stated::Unstated => None,
        }
    }

    /// The border the attributes of one `CT_Border` element state.
    fn read(attributes: &BorderAttributes<'_>) -> Self {
        Self {
            color: WordColor::read(
                attributes.color,
                ColorTheme {
                    name: attributes.theme_color,
                    shade: attributes.theme_shade,
                    tint: attributes.theme_tint,
                    tint_overrides_shade: true,
                },
            ),
            frame: on_off(attributes.frame),
            shadow: on_off(attributes.shadow),
            size: attributes
                .sz
                .map_or(Stated::Unstated, |size| unsigned(size.value().ok())),
            space: attributes
                .space
                .map_or(Stated::Value(0), |space| unsigned(space.value().ok())),
            style: attributes.val.map_or(Stated::Unstated, |style| {
                stated(style.value().ok().map(BorderStyle::of))
            }),
        }
    }
}

/// The attributes of a `CT_Border` element, borrowed.
struct BorderAttributes<'border> {
    color: Option<&'border ColorValue>,
    frame: Option<&'border OnOffValue>,
    shadow: Option<&'border OnOffValue>,
    space: Option<&'border MeasurementInPoints>,
    sz: Option<&'border MeasurementInEighthsOfAPoint>,
    theme_color: Option<&'border ThemeColor>,
    theme_shade: Option<&'border TwoDigitHexadecimalValue>,
    theme_tint: Option<&'border TwoDigitHexadecimalValue>,
    val: Option<&'border BorderStyles>,
}

/// A border style of `ST_Border` (section 17.18.2), by the kind the section gives it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BorderStyle {
    /// An art border: "a repeated image to be used when drawing a border"; its width is in points.
    Art(BorderStylesMember),
    /// A line border: "a pattern to be used when drawing a line"; its width is in eighths of a
    /// point.
    Line(BorderStylesMember),
    /// `nil` or `none`: "no border shall be applied to the current item".
    NoBorder,
}

impl BorderStyle {
    /// The kind section 17.18.2 gives `member`: the 25 line styles, the two with no border, and
    /// every other an art border.
    const fn of(member: BorderStylesMember) -> Self {
        use BorderStylesMember as Member;
        match member {
            Member::Nil | Member::None => Self::NoBorder,
            Member::DashDotStroked
            | Member::Dashed
            | Member::DashSmallGap
            | Member::DotDash
            | Member::DotDotDash
            | Member::Dotted
            | Member::Double
            | Member::DoubleWave
            | Member::Inset
            | Member::Outset
            | Member::Single
            | Member::Thick
            | Member::ThickThinLargeGap
            | Member::ThickThinMediumGap
            | Member::ThickThinSmallGap
            | Member::ThinThickLargeGap
            | Member::ThinThickMediumGap
            | Member::ThinThickSmallGap
            | Member::ThinThickThinLargeGap
            | Member::ThinThickThinMediumGap
            | Member::ThinThickThinSmallGap
            | Member::ThreeDEmboss
            | Member::ThreeDEngrave
            | Member::Triple
            | Member::Wave => Self::Line(member),
            Member::Custom | _ => Self::Art(member),
        }
    }
}

/// The theme attributes of one colour, borrowed.
#[derive(Clone, Copy)]
struct ColorTheme<'element> {
    name: Option<&'element ThemeColor>,
    shade: Option<&'element TwoDigitHexadecimalValue>,
    tint: Option<&'element TwoDigitHexadecimalValue>,
    /// Whether "If the themeTint is supplied, the value of this attribute shall be ignored" holds.
    tint_overrides_shade: bool,
}

/// A value of `ST_HexColor` (section 17.18.38).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum HexColor {
    /// `auto`: "the color value can automatically be defined when this document is processed".
    Automatic,
    /// Six hexadecimal digits: red, green and blue.
    Rgb(Rgb),
}

/// A colour's red, green and blue, from 0 to 255.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Rgb {
    /// The blue component.
    pub blue: u8,
    /// The green component.
    pub green: u8,
    /// The red component.
    pub red: u8,
}

impl Rgb {
    /// The colour of `red`, `green` and `blue`.
    #[must_use = "this only builds the value"]
    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { blue, green, red }
    }
}

/// What a shading element fills with (section 17.3.5): its `w:val` pattern laid in `w:color`
/// over `w:fill` (section 17.18.78).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ShadingFill {
    /// One colour fills: `w:fill` for `clear`, whose mask is all fill colour; `w:color` for
    /// `solid`, whose mask is all pattern colour.
    Color(WordColor),
    /// `nil`: section 17.18.78 names it "No Pattern" and draws its mask all in the pattern colour,
    /// so the text gives no single fill.
    Nil {
        /// `w:color` with its theme attributes, `auto` when absent.
        color: WordColor,
        /// `w:fill` with its theme attributes, `auto` when absent.
        fill: WordColor,
    },
    /// Another pattern: the mask `pattern` shows `color` over `fill`.
    Pattern {
        /// `w:color` with its theme attributes, `auto` when absent.
        color: WordColor,
        /// `w:fill` with its theme attributes, `auto` when absent.
        fill: WordColor,
        /// The pattern's name.
        pattern: ShadingPatternsMember,
    },
    /// `w:val` is absent or no value of `ST_Shd`: `CT_Shd` requires it and the text gives no
    /// meaning to its absence.
    Unstated,
}

/// A value an attribute states once the standard's default is applied.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Stated<T> {
    /// The standard gives no value: the attribute is absent and has no default, or its text is no
    /// value of its type.
    Unstated,
    /// The value.
    Value(T),
}

/// What a table measurement element states (section 17.4.87).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TableLength {
    /// `auto`: "automatically determined by the table layout algorithm" (section 17.18.90).
    Automatic,
    /// Fiftieths of a percent: `w` with `pct` (Part 4, section 14.11.10), or `w` with a percent
    /// sign, to the nearest fiftieth.
    Fiftieths(i64),
    /// Twentieths of a point: `w` with `dxa`, or a universal measure (section 22.9.2.15) to the
    /// nearest twentieth.
    Twentieths(i64),
    /// `w:type` or `w:w` is no value of its type.
    Unstated,
    /// `nil`: "the current width is zero, regardless of any width value specified".
    Zero,
}

impl TableLength {
    /// A number with no unit of its own, in the unit `unit` gives it.
    const fn by_unit(unit: TableWidthUnitsMember, number: i64) -> Self {
        match unit {
            TableWidthUnitsMember::Auto => Self::Automatic,
            TableWidthUnitsMember::Nil => Self::Zero,
            TableWidthUnitsMember::Pct => Self::Fiftieths(number),
            TableWidthUnitsMember::Dxa | _ => Self::Twentieths(number),
        }
    }

    /// The measurement `width` and `kind` state. "If the value of the type attribute and the
    /// actual measurement specified by the w attribute are contradictory, the type specified by
    /// the type attribute shall be ignored" (section 17.4.87).
    fn read(width: Option<&MeasurementOrPercentageValue>, kind: Option<&TableWidthUnits>) -> Self {
        // "If this attribute is omitted, then its value shall be assumed to be dxa."
        let Some(unit) = kind.map_or(Some(TableWidthUnitsMember::Dxa), |found| found.value().ok())
        else {
            return Self::Unstated;
        };
        if matches!(unit, TableWidthUnitsMember::Nil) {
            return Self::Zero;
        }
        // "If this attribute is omitted, then its value shall be assumed to be 0."
        let Some(text) = width else {
            return Self::by_unit(unit, 0);
        };
        let read = match text.value() {
            Ok(MeasurementOrPercentageValueMember::PercentageValueWithoutPercentSign(number)) => {
                Some(Self::by_unit(unit, number))
            }
            Ok(MeasurementOrPercentageValueMember::PercentageValueWithSign(_)) => text
                .0
                .strip_suffix('%')
                .and_then(|number| scaled(number, 50, 1))
                .map(Self::Fiftieths),
            Ok(MeasurementOrPercentageValueMember::UniversalMeasurement(measure)) => {
                universal_twentieths(&text.0, measure.unit).map(Self::Twentieths)
            }
            Ok(_) | Err(_) => None,
        };
        read.unwrap_or(Self::Unstated)
    }
}

/// A colour of a border or a shading: the hexadecimal value, and the theme colour that supersedes
/// it, with its tint or shade (sections 17.3.4 and 17.3.5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct WordColor {
    /// `w:themeColor`: `None` when absent or `none` ("no theme color shall be applied").
    pub theme: Stated<Option<ThemeReference>>,
    /// The hexadecimal value, `auto` when absent.
    pub value: Stated<HexColor>,
}

impl WordColor {
    /// The colour `value` and `theme` state; an absent `value` is `auto`.
    fn read(text: Option<&ColorValue>, theme: ColorTheme<'_>) -> Self {
        let value = text.map_or(Stated::Value(HexColor::Automatic), |found| {
            stated(found.value().ok().and_then(|member| match member {
                ColorValueMember::HexadecimalColorValue(rgb) => Some(HexColor::Rgb(rgb_of(rgb))),
                ColorValueMember::HexColorAuto(_) => Some(HexColor::Automatic),
                _ => None,
            }))
        });
        let reference = theme.name.map_or(Stated::Value(None), |name| {
            stated(name.value().ok().map(|member| {
                (member != ThemeColorMember::None).then(|| ThemeReference {
                    change: change_of(theme),
                    name: member,
                })
            }))
        });
        Self {
            theme: reference,
            value,
        }
    }

    /// The colour shown: the theme colour from `theme`, with its tint or shade, when one is stated
    /// ("superseded by the theme color value"), else the hexadecimal value. Unstated when a theme
    /// colour is stated and `theme` has none for it.
    #[must_use = "this only returns the value"]
    pub fn resolve(&self, theme: &ThemeColors) -> Stated<HexColor> {
        match self.theme {
            Stated::Value(None) => self.value,
            Stated::Value(Some(reference)) => stated(reference.resolve(theme).map(HexColor::Rgb)),
            Stated::Unstated => Stated::Unstated,
        }
    }
}

/// How a theme tint or shade changes a theme colour (section 17.3.4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ThemeChange {
    /// No tint and no shade.
    Plain,
    /// The luminance times the shade over 255: "L′ = L ∗ Shadepercentage".
    Shade(u8),
    /// The luminance moved toward white: "L′ = L ∗ Tint pct + (1 − Tint pct)".
    Tint(u8),
}

/// A theme colour a border or a shading names, with its tint or shade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ThemeReference {
    /// The tint or shade; unstated when its text is no value, or when a shading's fill states both.
    pub change: Stated<ThemeChange>,
    /// The `ST_ThemeColor` value (section 17.18.97).
    pub name: ThemeColorMember,
}

impl ThemeReference {
    /// The red, green and blue of this theme colour in `theme`, changed by its tint or shade.
    fn resolve(self, theme: &ThemeColors) -> Option<Rgb> {
        let content = theme.color(scheme_name(self.name)?)?;
        let base = color(content, &ThemeColors::default(), None)?;
        let rgb = [base.red, base.green, base.blue];
        let [red, green, blue] = match self.change {
            Stated::Value(ThemeChange::Plain) => rgb,
            Stated::Value(ThemeChange::Shade(shade)) => word_shade(rgb, shade),
            Stated::Value(ThemeChange::Tint(tint)) => word_tint(rgb, tint),
            Stated::Unstated => return None,
        };
        Some(Rgb::new(red, green, blue))
    }
}

impl<X: UserData> From<&ParagraphShadingWith<X>> for ShadingFill {
    fn from(shading: &ParagraphShadingWith<X>) -> Self {
        let Some(pattern) = shading.val.as_ref().and_then(|val| val.value().ok()) else {
            return Self::Unstated;
        };
        let color = WordColor::read(
            shading.color.as_ref(),
            ColorTheme {
                name: shading.theme_color.as_ref(),
                shade: shading.theme_shade.as_ref(),
                tint: shading.theme_tint.as_ref(),
                tint_overrides_shade: true,
            },
        );
        let fill = WordColor::read(
            shading.fill.as_ref(),
            ColorTheme {
                name: shading.theme_fill.as_ref(),
                shade: shading.theme_fill_shade.as_ref(),
                tint: shading.theme_fill_tint.as_ref(),
                tint_overrides_shade: false,
            },
        );
        match pattern {
            ShadingPatternsMember::Clear => Self::Color(fill),
            ShadingPatternsMember::Solid => Self::Color(color),
            ShadingPatternsMember::Nil => Self::Nil { color, fill },
            ShadingPatternsMember::DiagCross | _ => Self::Pattern {
                color,
                fill,
                pattern,
            },
        }
    }
}

borders!(
    BottomBorderWith,
    LeftBorderWith,
    LeftParagraphBorderWith,
    ParagraphBorderAboveIdenticalParagraphsWith,
    ParagraphBorderBelowIdenticalParagraphsWith,
    ParagraphBorderBetweenFacingPagesWith,
    ParagraphBorderBetweenIdenticalParagraphsWith,
    RightBorderWith,
    RightParagraphBorderWith,
    TableCellLeadingEdgeBorderWith,
    TableCellTrailingMarginExceptionWith,
    TableInsideHorizontalEdgesBorderWith,
    TableInsideVerticalEdgesBorderWith,
    TextBorderWith,
    TopBorderWith,
    TopLeftBottomRightDiagonalBorderWith,
    TopRightBottomLeftDiagonalBorderWith,
);

table_lengths!(
    LeftParagraphBorderTblWidthWith,
    ParagraphBorderAboveIdenticalParagraphsTblWidthWith,
    ParagraphBorderBelowIdenticalParagraphsTblWidthWith,
    PreferredTableCellWidthWith,
    PreferredTableWidthWith,
    PreferredWidthAfterTableRowWith,
    PreferredWidthBeforeTableRowWith,
    RightParagraphBorderTblWidthWith,
    TableCellLeadingEdgeBorderTblWidthWith,
    TableCellTrailingMarginExceptionTblWidthWith,
    TableIndentFromLeadingMarginWith,
    TableRowCellSpacingWith,
);

/// The tint or shade of `theme`. A tint "is applied to the RGB value of the theme color" and so
/// is a shade; only a border's and a pattern colour's shade is "ignored" beside a tint.
fn change_of(theme: ColorTheme<'_>) -> Stated<ThemeChange> {
    let tint = theme.tint.map(|text| text.value().ok());
    let shade = theme.shade.map(|text| text.value().ok());
    match (tint, shade) {
        (None, None) => Stated::Value(ThemeChange::Plain),
        (Some(Some(value)), None) => Stated::Value(ThemeChange::Tint(value)),
        (Some(Some(value)), Some(_)) if theme.tint_overrides_shade => {
            Stated::Value(ThemeChange::Tint(value))
        }
        (None, Some(Some(value))) => Stated::Value(ThemeChange::Shade(value)),
        (Some(_), Some(_)) | (Some(None), None) | (None, Some(None)) => Stated::Unstated,
    }
}

/// `numerator / denominator` to the nearest whole number, a half away from zero, for a positive
/// `denominator`.
fn nearest(numerator: i128, denominator: i128) -> Option<i128> {
    let half = denominator / 2;
    let shifted = if numerator < 0 {
        numerator.checked_sub(half)?
    } else {
        numerator.checked_add(half)?
    };
    shifted.checked_div(denominator)
}

/// An `ST_OnOff` attribute that is false when absent.
fn on_off(value: Option<&OnOffValue>) -> Stated<bool> {
    value.map_or(Stated::Value(false), |text| stated(text.value().ok()))
}

/// The red, green and blue of six hexadecimal digits read as one number.
fn rgb_of(value: u32) -> Rgb {
    let byte = |shift: u32| u8::try_from((value >> shift) & 0xFF).unwrap_or(u8::MAX);
    Rgb::new(byte(16), byte(8), byte(0))
}

/// The decimal number `text`, `-?[0-9]+(\.[0-9]+)?`, times `numerator / denominator`, to the
/// nearest whole number.
fn scaled(text: &str, numerator: i128, denominator: i128) -> Option<i64> {
    let (negative, digits) = text
        .strip_prefix('-')
        .map_or((false, text), |rest| (true, rest));
    let (whole, written) = digits.split_once('.').unwrap_or((digits, ""));
    let fraction = written.get(..FRACTION_DIGITS).unwrap_or(written);
    let mut number: i128 = 0;
    for byte in whole.bytes().chain(fraction.bytes()) {
        if !byte.is_ascii_digit() {
            return None;
        }
        number = number
            .checked_mul(10)?
            .checked_add(i128::from(byte - b'0'))?;
    }
    let places = u32::try_from(fraction.len()).ok()?;
    let below = 10_i128.checked_pow(places)?.checked_mul(denominator)?;
    let magnitude = nearest(number.checked_mul(numerator)?, below)?;
    i64::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// The name `ThemeColors` maps for an `ST_ThemeColor` value: each names the `w:clrSchemeMapping`
/// attribute section 17.18.97 gives it; `none` names none.
const fn scheme_name(name: ThemeColorMember) -> Option<StSchemeColorValMember> {
    match name {
        ThemeColorMember::Accent1 => Some(StSchemeColorValMember::Accent1),
        ThemeColorMember::Accent2 => Some(StSchemeColorValMember::Accent2),
        ThemeColorMember::Accent3 => Some(StSchemeColorValMember::Accent3),
        ThemeColorMember::Accent4 => Some(StSchemeColorValMember::Accent4),
        ThemeColorMember::Accent5 => Some(StSchemeColorValMember::Accent5),
        ThemeColorMember::Accent6 => Some(StSchemeColorValMember::Accent6),
        ThemeColorMember::Background1 | ThemeColorMember::Light1 => {
            Some(StSchemeColorValMember::Bg1)
        }
        ThemeColorMember::Background2 | ThemeColorMember::Light2 => {
            Some(StSchemeColorValMember::Bg2)
        }
        ThemeColorMember::Dark1 | ThemeColorMember::Text1 => Some(StSchemeColorValMember::Tx1),
        ThemeColorMember::Dark2 | ThemeColorMember::Text2 => Some(StSchemeColorValMember::Tx2),
        ThemeColorMember::FollowedHyperlink => Some(StSchemeColorValMember::FolHlink),
        ThemeColorMember::Hyperlink => Some(StSchemeColorValMember::Hlink),
        ThemeColorMember::None | _ => None,
    }
}

/// `Value` of what was read, `Unstated` when nothing was.
fn stated<T>(value: Option<T>) -> Stated<T> {
    value.map_or(Stated::Unstated, Stated::Value)
}

/// An unsigned number read as `i64`.
fn unsigned(value: Option<i64>) -> Stated<u64> {
    stated(value.and_then(|number| u64::try_from(number).ok()))
}

/// A universal measure (section 22.9.2.15) in twentieths of a point: 1440 an inch, 20 a point,
/// 240 a pica, and an inch of 2.54 cm.
fn universal_twentieths(text: &str, unit: MeasureUnit) -> Option<i64> {
    let (per_unit, below) = match unit {
        MeasureUnit::Centimeters => (72_000, 127),
        MeasureUnit::Inches => (1440, 1),
        MeasureUnit::Millimeters => (7200, 127),
        MeasureUnit::Picas => (240, 1),
        MeasureUnit::Points => (20, 1),
        _ => return None,
    };
    scaled(text.get(..text.len().checked_sub(2)?)?, per_unit, below)
}
