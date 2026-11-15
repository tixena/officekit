//! What a VML opacity and a VML miter limit state, whether a VML shape is filled and stroked, and
//! what a shape takes from its shapetype (ECMA-376 Part 4, sections 19.1.2.5, 19.1.2.19 to 19.1.2.21).

#[cfg(test)]
mod tests;

use core::iter;

use officekit_markup::UserData;
use officekit_markup::common_simple_types::{BooleanValue, BooleanValueMember};
use officekit_markup::vml::{
    ArcSegmentWith, BezierCurveWith, DocumentBackgroundWith, ImageFileWith, LineStrokeSettingsWith,
    LineWith, MultiplePathLineContentWith, MultiplePathLineWith, OvalWith, RectangleWith,
    RoundedRectangleWith, ShapeDefinitionContentWith, ShapeDefinitionWith, ShapeElementContentWith,
    ShapeFillPropertiesWith, ShapeGroupContentWith, ShapeGroupWith, ShapeTemplateWith,
};

use crate::docx::lines::Stated;

/// `FillToggle` for elements whose `v:fill` children are items of `$content`.
macro_rules! fill_toggles {
    ($($kind:ident => $content:ident),+ $(,)?) => {$(
        impl<X: UserData> FillToggle for $kind<X> {
            fn fill_toggle(&self) -> Toggle {
                let child = self.content.iter().find_map(|item| {
                    if let $content::ShapeFillProperties(fill) = item {
                        Some(Child::On(true_false(fill.on.as_ref())))
                    } else {
                        None
                    }
                });
                toggle(true_false(self.filled.as_ref()), child.unwrap_or(Child::Absent))
            }
        }
    )+};
}

/// `StrokeToggle` for elements whose `v:stroke` children are items of `$content`.
macro_rules! stroke_toggles {
    ($($kind:ident => $content:ident),+ $(,)?) => {$(
        impl<X: UserData> StrokeToggle for $kind<X> {
            fn stroke_toggle(&self) -> Toggle {
                let child = self.content.iter().find_map(|item| {
                    if let $content::LineStrokeSettings(stroke) = item {
                        Some(Child::On(true_false(stroke.on.as_ref())))
                    } else {
                        None
                    }
                });
                toggle(true_false(self.stroked.as_ref()), child.unwrap_or(Child::Absent))
            }
        }
    )+};
}

/// One in the 16.16 fixed form of an opacity with "a trailing f".
const FIXED_ONE: i128 = 1 << 16;

/// An opaque opacity, in the thousandths of a percent `Rgba::opacity` gives a `DrawingML` one in.
const OPAQUE: u32 = 100_000;

/// A shape's `v:fill` or `v:stroke` child, as `filled` and `stroked` read it.
#[derive(Clone, Copy)]
enum Child {
    /// The shape has no such child.
    Absent,
    /// The child, with the truth of its `on` when it states one.
    On(Option<bool>),
}

/// Whether a VML shape is filled or stroked: its own `filled` or `stroked`, which its child's `on`
/// "overrides"; each "Default is true" (Part 4, sections 19.1.2.5, 19.1.2.19 and 19.1.2.21).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Toggle {
    /// What the standard decides: the child's `on` when stated, else the shape's attribute.
    Decided(bool),
    /// The child states no `on`, whose default is `child`, and the shape states `shape`: Part 4
    /// does not say whether an absent `on` overrides the shape's attribute.
    Open {
        /// The default of the child's absent `on`.
        child: bool,
        /// The shape's own `filled` or `stroked`.
        shape: bool,
    },
}

/// A `v:shape` with the `v:shapetype` its `type` names, whose properties the shape's own override
/// one by one (Part 4, sections 19.1.2.19 and 19.1.2.20).
#[derive(Clone, Copy, Debug)]
pub struct TypedShape<'node, X: UserData> {
    shape: &'node ShapeDefinitionWith<X>,
    shapetype: Option<&'node ShapeTemplateWith<X>>,
}

/// A VML element with `filled` and `v:fill` children; the first `v:fill` is the one read.
pub trait FillToggle {
    /// Whether the element is filled: its `filled` and its `v:fill` child's `on` together.
    fn fill_toggle(&self) -> Toggle;
}

/// A VML shape with `stroked` and `v:stroke` children; the first `v:stroke` is the one read.
pub trait StrokeToggle {
    /// Whether the shape is stroked: its `stroked` and its `v:stroke` child's `on` together.
    fn stroke_toggle(&self) -> Toggle;
}

impl<X: UserData> FillToggle for DocumentBackgroundWith<X> {
    fn fill_toggle(&self) -> Toggle {
        let child = self
            .shape_fill_properties
            .as_ref()
            .map_or(Child::Absent, |fill| {
                Child::On(true_false(fill.on.as_ref()))
            });
        toggle(true_false(self.filled.as_ref()), child)
    }
}

impl<'node, X: UserData> TypedShape<'node, X> {
    /// An attribute of the shape's first `v:fill`, else of its shapetype's (Part 4, 19.1.2.20).
    pub fn fill_attribute<T, F>(&self, attribute: F) -> Option<&'node T>
    where
        T: ?Sized,
        F: Fn(&'node ShapeFillPropertiesWith<X>) -> Option<&'node T>,
    {
        shape_fill(self.shape)
            .and_then(&attribute)
            .or_else(|| self.shapetype.and_then(shapetype_fill).and_then(attribute))
    }

    /// `shape` with the first of `shapetypes` its `type` names (see `shapetype`).
    #[must_use = "this only returns the value"]
    pub fn new<I>(shape: &'node ShapeDefinitionWith<X>, shapetypes: I) -> Self
    where
        I: IntoIterator<Item = &'node ShapeTemplateWith<X>>,
    {
        Self {
            shape,
            shapetype: shapetype(shape, shapetypes),
        }
    }

    /// The `v:shape`.
    #[must_use = "this only returns the value"]
    pub const fn shape(&self) -> &'node ShapeDefinitionWith<X> {
        self.shape
    }

    /// The `v:shapetype` the shape's `type` names, if any.
    #[must_use = "this only returns the value"]
    pub const fn shapetype(&self) -> Option<&'node ShapeTemplateWith<X>> {
        self.shapetype
    }

    /// An attribute of the shape's first `v:stroke`, else of its shapetype's (Part 4, 19.1.2.20).
    pub fn stroke_attribute<T, F>(&self, attribute: F) -> Option<&'node T>
    where
        T: ?Sized,
        F: Fn(&'node LineStrokeSettingsWith<X>) -> Option<&'node T>,
    {
        shape_stroke(self.shape).and_then(&attribute).or_else(|| {
            self.shapetype
                .and_then(shapetype_stroke)
                .and_then(attribute)
        })
    }
}

impl<X: UserData> FillToggle for TypedShape<'_, X> {
    fn fill_toggle(&self) -> Toggle {
        layered_toggle(
            [
                (self.shape.filled.as_ref(), shape_fill(self.shape)),
                self.shapetype.map_or((None, None), |template| {
                    (template.filled.as_ref(), shapetype_fill(template))
                }),
            ],
            |fill| fill.on.as_ref(),
        )
    }
}

impl<X: UserData> StrokeToggle for TypedShape<'_, X> {
    fn stroke_toggle(&self) -> Toggle {
        layered_toggle(
            [
                (self.shape.stroked.as_ref(), shape_stroke(self.shape)),
                self.shapetype.map_or((None, None), |template| {
                    (template.stroked.as_ref(), shapetype_stroke(template))
                }),
            ],
            |stroke| stroke.on.as_ref(),
        )
    }
}

fill_toggles!(
    ArcSegmentWith => ShapeElementContentWith,
    BezierCurveWith => ShapeElementContentWith,
    ImageFileWith => ShapeElementContentWith,
    LineWith => ShapeElementContentWith,
    MultiplePathLineWith => MultiplePathLineContentWith,
    OvalWith => ShapeElementContentWith,
    RectangleWith => ShapeElementContentWith,
    RoundedRectangleWith => ShapeElementContentWith,
    ShapeDefinitionWith => ShapeDefinitionContentWith,
    ShapeGroupWith => ShapeGroupContentWith,
    ShapeTemplateWith => ShapeElementContentWith,
);

stroke_toggles!(
    ArcSegmentWith => ShapeElementContentWith,
    BezierCurveWith => ShapeElementContentWith,
    ImageFileWith => ShapeElementContentWith,
    LineWith => ShapeElementContentWith,
    MultiplePathLineWith => MultiplePathLineContentWith,
    OvalWith => ShapeElementContentWith,
    RectangleWith => ShapeElementContentWith,
    RoundedRectangleWith => ShapeElementContentWith,
    ShapeDefinitionWith => ShapeDefinitionContentWith,
    ShapeTemplateWith => ShapeElementContentWith,
);

/// Whether `shape` is filled: `filled` "is overridden by the fill on attribute", and `on` "overrides
/// the shape's fill attribute"; each "Default is true" (Part 4, sections 19.1.2.19 and 19.1.2.5).
#[must_use = "this only returns the value"]
pub fn filled<S>(shape: &S) -> Toggle
where
    S: FillToggle + ?Sized,
{
    shape.fill_toggle()
}

/// The toggle from `layers` of attribute and child, each property taken from the first layer that
/// states it; the child is present when any layer has one.
fn layered_toggle<C, const N: usize>(
    layers: [(Option<&BooleanValue>, Option<&C>); N],
    on: impl Fn(&C) -> Option<&BooleanValue>,
) -> Toggle {
    let attribute = layers
        .iter()
        .find_map(|&(attribute, _)| true_false(attribute));
    let child = if layers.iter().any(|(_, child)| child.is_some()) {
        Child::On(layers.iter().find_map(|&(_, child)| true_false(on(child?))))
    } else {
        Child::Absent
    };
    toggle(attribute, child)
}

/// The VML `miterlimit` of `v:stroke` in thousandths of the line's thickness, an `xsd:decimal`;
/// absent, "Default is 8" (Part 4, section 19.1.2.21); `Unstated` when the text is no decimal.
#[must_use = "this only returns the value"]
pub fn miter_limit(text: Option<&str>) -> Stated<i64> {
    text.map_or(Stated::Value(8000), |written| {
        stated(signed_thousandths(written))
    })
}

/// A VML `opacity` in thousandths of a percent, from ".25", "50%" or "52429f" (65536ths); absent,
/// "Default is 1.0" (Part 4, sections 19.1.2.5, 19.1.2.19, 19.1.2.21); past 0 to 1, `Unstated`.
#[must_use = "this only returns the value"]
pub fn opacity(text: Option<&str>) -> Stated<u32> {
    text.map_or(Stated::Value(OPAQUE), |written| {
        stated(
            opacity_thousandths(written)
                .and_then(|value| u32::try_from(value).ok())
                .filter(|&value| value <= OPAQUE),
        )
    })
}

/// The thousandths of a percent an opacity's `text` states in any of its three forms.
fn opacity_thousandths(text: &str) -> Option<i128> {
    if let Some(percent) = text.strip_suffix('%') {
        return scaled(percent, 3);
    }
    let Some(units) = text.strip_suffix('f') else {
        return scaled(text, 5);
    };
    if !units.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let half = FIXED_ONE / 2;
    scaled(units, 0)?
        .checked_mul(i128::from(OPAQUE))?
        .checked_add(half)?
        .checked_div(FIXED_ONE)
}

/// The unsigned decimal `text`, `[0-9]*(\.[0-9]*)?` with a digit, times ten to `places`, to the
/// nearest whole number, a half up.
fn scaled(text: &str, places: usize) -> Option<i128> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if (whole.is_empty() && fraction.is_empty()) || !digits(whole) || !digits(fraction) {
        return None;
    }
    let kept = fraction.bytes().chain(iter::repeat(b'0')).take(places);
    let mut number: i128 = 0;
    for byte in whole.bytes().chain(kept) {
        number = number
            .checked_mul(10)?
            .checked_add(i128::from(byte - b'0'))?;
    }
    let round_up = fraction
        .as_bytes()
        .get(places)
        .is_some_and(|&byte| byte >= b'5');
    number.checked_add(i128::from(round_up))
}

/// The first `v:fill` of a `v:shape`.
fn shape_fill<X>(shape: &ShapeDefinitionWith<X>) -> Option<&ShapeFillPropertiesWith<X>>
where
    X: UserData,
{
    shape.content.iter().find_map(|item| {
        if let ShapeDefinitionContentWith::ShapeFillProperties(fill) = item {
            Some(fill)
        } else {
            None
        }
    })
}

/// The first `v:stroke` of a `v:shape`.
fn shape_stroke<X>(shape: &ShapeDefinitionWith<X>) -> Option<&LineStrokeSettingsWith<X>>
where
    X: UserData,
{
    shape.content.iter().find_map(|item| {
        if let ShapeDefinitionContentWith::LineStrokeSettings(stroke) = item {
            Some(stroke)
        } else {
            None
        }
    })
}

/// The first of `shapetypes` whose `id` the `type` of `shape` names, as `#_x0000_t202` or with no
/// `#` (Part 4, section 19.1.2.19); `None` when `shape` has no `type` or it names none of them.
#[must_use = "this only returns the value"]
pub fn shapetype<'node, X, I>(
    shape: &ShapeDefinitionWith<X>,
    shapetypes: I,
) -> Option<&'node ShapeTemplateWith<X>>
where
    X: UserData,
    I: IntoIterator<Item = &'node ShapeTemplateWith<X>>,
{
    let named = shape.r#type.as_deref()?;
    let id = named.strip_prefix('#').unwrap_or(named);
    shapetypes
        .into_iter()
        .find(|template| template.id.as_deref() == Some(id))
}

/// The first `v:fill` of a `v:shapetype`.
fn shapetype_fill<X>(template: &ShapeTemplateWith<X>) -> Option<&ShapeFillPropertiesWith<X>>
where
    X: UserData,
{
    template.content.iter().find_map(|item| {
        if let ShapeElementContentWith::ShapeFillProperties(fill) = item {
            Some(fill)
        } else {
            None
        }
    })
}

/// The first `v:stroke` of a `v:shapetype`.
fn shapetype_stroke<X>(template: &ShapeTemplateWith<X>) -> Option<&LineStrokeSettingsWith<X>>
where
    X: UserData,
{
    template.content.iter().find_map(|item| {
        if let ShapeElementContentWith::LineStrokeSettings(stroke) = item {
            Some(stroke)
        } else {
            None
        }
    })
}

/// The `xsd:decimal` `text`, signed, in thousandths, to the nearest whole number.
fn signed_thousandths(text: &str) -> Option<i64> {
    let (negative, digits) = text.strip_prefix('-').map_or_else(
        || (false, text.strip_prefix('+').unwrap_or(text)),
        |rest| (true, rest),
    );
    let magnitude = scaled(digits, 3)?;
    i64::try_from(if negative { -magnitude } else { magnitude }).ok()
}

/// `Value` of what was read, `Unstated` when nothing was.
fn stated<T>(value: Option<T>) -> Stated<T> {
    value.map_or(Stated::Unstated, Stated::Value)
}

/// Whether `shape` is stroked: `stroked` is overridden by "The on attribute of the stroke element",
/// and `on` "overrides the shape's stroke attribute"; each "Default is true" (Part 4, 19.1.2.21).
#[must_use = "this only returns the value"]
pub fn stroked<S>(shape: &S) -> Toggle
where
    S: StrokeToggle + ?Sized,
{
    shape.stroke_toggle()
}

/// The toggle from the truth of a shape's own `filled` or `stroked`, `shape`, and its `child`; a
/// text that is no `ST_TrueFalse` value counts as absent.
fn toggle(shape: Option<bool>, child: Child) -> Toggle {
    match child {
        Child::On(on) => match (on, shape) {
            (Some(stated_on), _) => Toggle::Decided(stated_on),
            (None, Some(false)) => Toggle::Open {
                child: true,
                shape: false,
            },
            (None, Some(true) | None) => Toggle::Decided(true),
        },
        Child::Absent => Toggle::Decided(shape.unwrap_or(true)),
    }
}

/// The truth an `ST_TrueFalse` attribute states (Part 4, section 20.1.2.5); `None` when it is
/// absent or no value.
fn true_false(value: Option<&BooleanValue>) -> Option<bool> {
    match value?.value().ok()? {
        BooleanValueMember::T | BooleanValueMember::True => Some(true),
        BooleanValueMember::F | BooleanValueMember::False => Some(false),
        _ => None,
    }
}
