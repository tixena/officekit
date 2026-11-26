//! The outline of a shape: a preset or a custom geometry as paths in a frame (ECMA-376 Part 1,
//! sections 20.1.9.4, 20.1.9.11, 20.1.9.15, 20.1.9.18 and 20.1.9.22).

mod presets;
#[cfg(test)]
mod tests;

use alloc::borrow::Cow;
use alloc::collections::BTreeMap;

use officekit_markup::Spelling;
use officekit_markup::drawing::{
    CustomGeometry, ListOfShapeAdjustValues, ShapeGuide, ShapePathContent, ShapePathPoint,
};
use officekit_xml::conformance::ConformanceClass;

use presets::PRESETS;

/// The share that stands for 1 in a sine, a cosine or a tangent.
const ONE: i128 = 1 << 60;

/// Pi as a share of [`ONE`], rounded.
const PI: i128 = 3_622_009_729_038_561_421;

/// The parts of one unit in a guide's value.
const SCALE: i128 = 1 << 16;

/// A full turn, 21,600,000 60000ths of a degree, as a guide's value.
const TURN: i128 = 21_600_000 * SCALE;

/// One step of a definition's path, each value a text: a guide's name or a number.
#[derive(Clone, Copy, Debug)]
enum Command<'text> {
    ArcTo {
        h_r: &'text str,
        st_ang: &'text str,
        sw_ang: &'text str,
        w_r: &'text str,
    },
    Close,
    CubicTo(
        (&'text str, &'text str),
        (&'text str, &'text str),
        (&'text str, &'text str),
    ),
    LineTo(&'text str, &'text str),
    MoveTo(&'text str, &'text str),
    QuadTo((&'text str, &'text str), (&'text str, &'text str)),
}

/// The two control points and the end of a cubic curve of a path.
type Curve = [Place; 3];

/// The outline of a shape in its frame: its paths (section 20.1.9.15) and its text rectangle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Outline {
    /// Each `a:path`, in the order the definition gives them.
    pub paths: Vec<OutlinePath>,
    /// Where the shape's text goes.
    pub text_rectangle: TextRectangle,
}

/// One `a:path` of an outline (section 20.1.9.15).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct OutlinePath {
    /// The steps of the path, in order.
    pub commands: Vec<PathCommand>,
    /// How the path is filled: `fill`, `norm` when absent.
    pub fill: PathFill,
    /// Whether the path is stroked: `stroke`, true when absent.
    pub stroke: bool,
}

/// One step of a path (section 20.1.9.15), its points in the frame's units; an `a:arcTo` (section
/// 20.1.9.4) gives cubic curves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PathCommand {
    /// A line back to where the path last moved to.
    Close,
    /// A cubic curve from the pen.
    CubicTo {
        /// The first control point.
        control_1: Point,
        /// The second control point.
        control_2: Point,
        /// Where the curve ends.
        end: Point,
    },
    /// A line from the pen.
    LineTo(Point),
    /// A move of the pen, which starts a new part of the path.
    MoveTo(Point),
    /// A quadratic curve from the pen.
    QuadTo {
        /// The control point.
        control: Point,
        /// Where the curve ends.
        end: Point,
    },
}

/// How a path is filled (`ST_PathFillMode`, section 20.1.10.37).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PathFill {
    /// `darken`: the shape's fill, darker.
    Darken,
    /// `darkenLess`: the shape's fill, a little darker.
    DarkenLess,
    /// `lighten`: the shape's fill, lighter.
    Lighten,
    /// `lightenLess`: the shape's fill, a little lighter.
    LightenLess,
    /// `none`: not filled.
    None,
    /// `norm`: the shape's fill.
    Norm,
}

/// A point of a path in the path's own units, as guide's values.
type Place = (i128, i128);

/// A point of a path (section 20.1.9.15), in the units of the frame, from its top left corner,
/// rounded to the nearest unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Point {
    /// The distance to the right of the frame's left edge.
    pub x: i64,
    /// The distance below the frame's top edge.
    pub y: i64,
}

/// A shape's definition as `a:custGeom` writes it, every value a text: one form for a preset of
/// the table and for a typed custom geometry.
#[derive(Clone, Copy, Debug)]
struct Preset<'text> {
    adjust: &'text [(&'text str, &'text str)],
    guides: &'text [(&'text str, &'text str)],
    name: &'text str,
    paths: &'text [PresetPath<'text>],
    rect: Option<[&'text str; 4]>,
}

/// One `a:path` of a definition.
#[derive(Clone, Copy, Debug)]
struct PresetPath<'text> {
    commands: &'text [Command<'text>],
    fill: &'text str,
    h: Option<&'text str>,
    stroke: bool,
    w: Option<&'text str>,
}

/// The outline's `a:rect` (section 20.1.9.22), the whole frame when it has none.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct TextRectangle {
    /// The bottom edge, from the frame's top.
    pub bottom: i64,
    /// The left edge, from the frame's left.
    pub left: i64,
    /// The right edge, from the frame's left.
    pub right: i64,
    /// The top edge, from the frame's top.
    pub top: i64,
}

/// A preset name the standard's definitions do not hold (section 20.1.9.18).
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("the standard defines no preset shape named {0}")]
#[non_exhaustive]
pub struct UnknownPreset(pub String);

/// The angle whose tangent is `y` / `x`, by the signs of both (`at2`, section 20.1.9.11), as a
/// guide's value in 60000ths of a degree above minus half a turn and up to half a turn; 0 when
/// both are 0. Halving a quarter turn down to one part puts it within one part of the true angle.
fn angle(x: i128, y: i128) -> i128 {
    let largest = x.unsigned_abs().max(y.unsigned_abs());
    let shift = (128 - largest.leading_zeros()).saturating_sub(62);
    let across = i128::try_from(x.unsigned_abs() >> shift).unwrap_or(0);
    let along = i128::try_from(y.unsigned_abs() >> shift).unwrap_or(0);
    if across == 0 && along == 0 {
        return 0;
    }
    let residual = |turned: i128| {
        let (sine, cosine) = sine_cosine(turned);
        (along * cosine - across * sine).abs()
    };
    let mut low = 0;
    let mut high = TURN / 4;
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        let (sine, cosine) = sine_cosine(middle);
        if along * cosine > across * sine {
            low = middle;
        } else {
            high = middle;
        }
    }
    let first = if residual(high) < residual(low) {
        high
    } else {
        low
    };
    match (x < 0, y < 0) {
        (false, false) => first,
        (false, true) => -first,
        (true, false) => TURN / 2 - first,
        (true, true) => first - TURN / 2,
    }
}

/// An `a:arcTo` from `pen` (section 20.1.9.4), every value a guide's value: the cubic curves,
/// one for each quarter turn of the ellipse's own angle or less, and where the arc ends. Each
/// curve's handles are 4/3 tan(step / 4) of the radius long, which keeps it within 0.03% of the
/// larger radius from the ellipse. `None` for no radius, no swing, or a value that does not fit.
fn arc(pen: Place, radii: (i128, i128), start: i128, swing: i128) -> Option<(Vec<Curve>, Place)> {
    let (w_r, h_r) = radii;
    if w_r == 0 || h_r == 0 || swing == 0 {
        return None;
    }
    let turn = TURN.unsigned_abs();
    let magnitude = swing.unsigned_abs();
    let kept = i128::try_from(if magnitude > turn {
        turn + magnitude.rem_euclid(turn)
    } else {
        magnitude
    })
    .ok()?;
    let first = start.rem_euclid(TURN);
    let begin = parameter(first, radii)?;
    let end = parameter(first + kept * swing.signum(), radii)?;
    let (sine, cosine) = sine_cosine(begin);
    let centre = (
        pen.0.checked_sub(shares(w_r, cosine)?)?,
        pen.1.checked_sub(shares(h_r, sine)?)?,
    );
    let on = |turned: i128| {
        let (on_sine, on_cosine) = sine_cosine(turned);
        Some((
            centre.0.checked_add(shares(w_r, on_cosine)?)?,
            centre.1.checked_add(shares(h_r, on_sine)?)?,
        ))
    };
    let span = end - begin;
    let count = i128::try_from(span.unsigned_abs().div_ceil(turn / 4)).ok()?;
    let mut curves = Vec::new();
    let mut from = begin;
    for index in 1..=count {
        let to = begin + span * index / count;
        let (quarter_sine, quarter_cosine) = sine_cosine((to - from) / 4);
        let handle = divide(
            quarter_sine.checked_mul(4 * ONE)?,
            quarter_cosine.checked_mul(3)?,
        )?;
        let (from_sine, from_cosine) = sine_cosine(from);
        let (to_sine, to_cosine) = sine_cosine(to);
        let (start_x, start_y) = on(from)?;
        let (end_x, end_y) = on(to)?;
        curves.push([
            (
                start_x.checked_sub(shares(shares(w_r, from_sine)?, handle)?)?,
                start_y.checked_add(shares(shares(h_r, from_cosine)?, handle)?)?,
            ),
            (
                end_x.checked_add(shares(shares(w_r, to_sine)?, handle)?)?,
                end_y.checked_sub(shares(shares(h_r, to_cosine)?, handle)?)?,
            ),
            (end_x, end_y),
        ]);
        from = to;
    }
    Some((curves, on(end)?))
}

/// The outline of the custom geometry `geometry`, an `a:custGeom` (section 20.1.9.8), in a frame
/// of `width` by `height`.
#[must_use = "this only returns the value"]
pub fn custom(geometry: &CustomGeometry, width: i64, height: i64) -> Outline {
    let adjust = geometry
        .list_of_shape_adjust_values
        .as_deref()
        .map_or_else(Vec::new, |list| named(&list.shape_guide));
    let guides = geometry
        .list_of_shape_guides
        .as_deref()
        .map_or_else(Vec::new, |list| named(&list.shape_guide));
    let shape_paths = &geometry.list_of_shape_paths.shape_path;
    let texts: Vec<Vec<Vec<Cow<'_, str>>>> = shape_paths
        .iter()
        .map(|path| path.content.iter().map(step_texts).collect())
        .collect();
    let commands: Vec<Vec<Command<'_>>> = shape_paths
        .iter()
        .zip(&texts)
        .map(|(path, steps)| {
            path.content
                .iter()
                .zip(steps)
                .filter_map(|(content, values)| step(content, values))
                .collect()
        })
        .collect();
    let fills: Vec<Cow<'_, str>> = shape_paths
        .iter()
        .map(|path| path.fill.as_ref().map_or(Cow::Borrowed("norm"), spelled))
        .collect();
    let paths: Vec<PresetPath<'_>> = shape_paths
        .iter()
        .zip(&commands)
        .zip(&fills)
        .map(|((path, steps), fill)| PresetPath {
            commands: steps,
            fill,
            h: path.h.as_ref().map(|h| &*h.0),
            stroke: path
                .stroke
                .as_deref()
                .is_none_or(|stroke| !matches!(stroke.trim(), "false" | "0")),
            w: path.w.as_ref().map(|w| &*w.0),
        })
        .collect();
    let rect_texts = geometry.shape_text_rectangle.as_deref().map(|found| {
        [
            found.l.as_ref(),
            found.t.as_ref(),
            found.r.as_ref(),
            found.b.as_ref(),
        ]
        .map(text)
    });
    let rect = rect_texts
        .as_ref()
        .map(|edges| edges.each_ref().map(|edge| &**edge));
    let definition = Preset {
        adjust: &adjust,
        guides: &guides,
        name: "",
        paths: &paths,
        rect,
    };
    outline(&definition, &[], width, height)
}

/// `numerator / denominator` rounded to the nearest whole number, a half away from zero; `None`
/// for a denominator of 0 or a value that does not fit.
fn divide(numerator: i128, denominator: i128) -> Option<i128> {
    if denominator == 0 {
        return None;
    }
    let below = denominator.unsigned_abs().checked_mul(2)?;
    let magnitude = numerator
        .unsigned_abs()
        .checked_mul(2)?
        .checked_add(denominator.unsigned_abs())?
        / below;
    let value = i128::try_from(magnitude).ok()?;
    Some(if (numerator < 0) == (denominator < 0) {
        value
    } else {
        -value
    })
}

/// One of the 17 formulas of section 20.1.9.11 on its first three arguments; `None` for a
/// formula that is none of them, divides by zero, or has a value that does not fit.
fn evaluate(operator: &str, [x, y, z]: [i128; 3]) -> Option<i128> {
    match operator {
        "*/" => divide(x.checked_mul(y)?, z),
        "+-" => x.checked_add(y)?.checked_sub(z),
        "+/" => divide(x.checked_add(y)?.checked_mul(SCALE)?, z),
        "?:" => Some(if x > 0 { y } else { z }),
        "abs" => x.checked_abs(),
        "at2" => Some(angle(x, y)),
        "cat2" => divide(x.checked_mul(y)?, modulus([y, z, 0])?),
        "cos" => shares(x, sine_cosine(y).1),
        "max" => Some(x.max(y)),
        "min" => Some(x.min(y)),
        "mod" => modulus([x, y, z]),
        "pin" => Some(if y < x {
            x
        } else if y > z {
            z
        } else {
            y
        }),
        "sat2" => divide(x.checked_mul(z)?, modulus([y, z, 0])?),
        "sin" => shares(x, sine_cosine(y).0),
        "sqrt" => Some(square_root(x.checked_mul(SCALE)?)),
        "tan" => {
            let (sine, cosine) = sine_cosine(y);
            divide(x.checked_mul(sine)?, cosine)
        }
        "val" => Some(x),
        _ => None,
    }
}

/// The value of the formula `text` (section 20.1.9.11) over `names`: 0 for a formula that is
/// none of the 17, divides by zero, or has a value that does not fit.
fn formula(text: &str, names: &BTreeMap<&str, i128>) -> i128 {
    let mut tokens = text.split_whitespace();
    let operator = tokens.next().unwrap_or_default();
    let mut arguments = [0; 3];
    for (slot, token) in arguments.iter_mut().zip(tokens) {
        *slot = value(token, names);
    }
    evaluate(operator, arguments).unwrap_or(0)
}

/// The names every formula can use (section 20.1.10.56), for a frame of `width` by `height`
/// parts, with `wd12` and `wd32`, which the standard's own definitions use.
fn frame_names<'text>(width: i128, height: i128) -> BTreeMap<&'text str, i128> {
    let short = width.min(height);
    let part = |whole: i128, by: i128| divide(whole, by).unwrap_or_default();
    BTreeMap::from([
        ("3cd4", TURN * 3 / 4),
        ("3cd8", TURN * 3 / 8),
        ("5cd8", TURN * 5 / 8),
        ("7cd8", TURN * 7 / 8),
        ("b", height),
        ("cd2", TURN / 2),
        ("cd4", TURN / 4),
        ("cd8", TURN / 8),
        ("h", height),
        ("hc", part(width, 2)),
        ("hd2", part(height, 2)),
        ("hd3", part(height, 3)),
        ("hd4", part(height, 4)),
        ("hd5", part(height, 5)),
        ("hd6", part(height, 6)),
        ("hd8", part(height, 8)),
        ("l", 0),
        ("ls", width.max(height)),
        ("r", width),
        ("ss", short),
        ("ssd16", part(short, 16)),
        ("ssd2", part(short, 2)),
        ("ssd32", part(short, 32)),
        ("ssd4", part(short, 4)),
        ("ssd6", part(short, 6)),
        ("ssd8", part(short, 8)),
        ("t", 0),
        ("vc", part(height, 2)),
        ("w", width),
        ("wd10", part(width, 10)),
        ("wd12", part(width, 12)),
        ("wd2", part(width, 2)),
        ("wd3", part(width, 3)),
        ("wd32", part(width, 32)),
        ("wd4", part(width, 4)),
        ("wd5", part(width, 5)),
        ("wd6", part(width, 6)),
        ("wd8", part(width, 8)),
    ])
}

/// A universal measure (section 22.9.2.15), such as `2.54cm`, as a guide's value counting EMUs
/// (section 20.1.2.1); `None` for any other text or a value that does not fit.
fn measure(text: &str) -> Option<i128> {
    let split = text.len().checked_sub(2)?;
    let (number, unit) = (text.get(..split)?, text.get(split..)?);
    let per_unit: i128 = match unit {
        "in" => 914_400,
        "cm" => 360_000,
        "mm" => 36_000,
        "pt" => 12_700,
        "pc" | "pi" => 152_400,
        _ => return None,
    };
    let (negative, unsigned) = number
        .strip_prefix('-')
        .map_or((false, number), |rest| (true, rest));
    let (whole, fraction) = match unsigned.split_once('.') {
        Some((_, "")) => return None,
        Some(parts) => parts,
        None => (unsigned, ""),
    };
    if whole.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let digits: i128 = format!("{whole}{fraction}").parse().ok()?;
    let places = 10_i128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let magnitude = divide(digits.checked_mul(per_unit)?.checked_mul(SCALE)?, places)?;
    Some(if negative { -magnitude } else { magnitude })
}

/// The square root of the sum of the squares of `sides`, each a guide's value (`mod`, section
/// 20.1.9.11); `None` when the sum does not fit.
fn modulus(sides: [i128; 3]) -> Option<i128> {
    let mut sum: i128 = 0;
    for side in sides {
        sum = sum.checked_add(side.checked_mul(side)?)?;
    }
    Some(square_root(sum))
}

/// The name and the formula of each guide, an absent one empty.
fn named(guides: &[ShapeGuide]) -> Vec<(&str, &str)> {
    guides
        .iter()
        .map(|guide| {
            (
                guide.name.as_deref().unwrap_or_default(),
                guide.fmla.as_deref().unwrap_or_default(),
            )
        })
        .collect()
}

/// The outline of `definition` in a frame of `width` by `height`, each adjust value replaced by
/// the last of `own` of the same name.
fn outline(definition: &Preset<'_>, own: &[(&str, &str)], width: i64, height: i64) -> Outline {
    let mut names = frame_names(i128::from(width) * SCALE, i128::from(height) * SCALE);
    for &(name, given) in definition.adjust {
        let chosen = own
            .iter()
            .rev()
            .find(|(own_name, _)| *own_name == name)
            .map_or(given, |&(_, replaced)| replaced);
        let found = formula(chosen, &names);
        names.insert(name, found);
    }
    for &(name, text) in definition.guides {
        let found = formula(text, &names);
        names.insert(name, found);
    }
    let paths = definition
        .paths
        .iter()
        .map(|path| outline_path(path, &names, width, height))
        .collect();
    let text_rectangle = definition.rect.map_or(
        TextRectangle {
            bottom: height,
            left: 0,
            right: width,
            top: 0,
        },
        |[left, top, right, bottom]| TextRectangle {
            bottom: unit(value(bottom, &names)),
            left: unit(value(left, &names)),
            right: unit(value(right, &names)),
            top: unit(value(top, &names)),
        },
    );
    Outline {
        paths,
        text_rectangle,
    }
}

/// One path of a definition over `names`, its points stretched from the path's own `w` and `h`
/// to `width` and `height` and rounded.
fn outline_path(
    path: &PresetPath<'_>,
    names: &BTreeMap<&str, i128>,
    width: i64,
    height: i64,
) -> OutlinePath {
    let own = |size: Option<&str>| size?.trim().parse::<i64>().ok();
    let across = own(path.w);
    let down = own(path.h);
    let stretch = |parts: i128, frame: i64, size: Option<i64>| {
        size.map_or(Some(parts), |found| {
            divide(parts.checked_mul(i128::from(frame))?, i128::from(found))
        })
        .map_or(0, unit)
    };
    let point = |(x, y): Place| Point {
        x: stretch(x, width, across),
        y: stretch(y, height, down),
    };
    let place = |(x, y): (&str, &str)| (value(x, names), value(y, names));
    let mut commands = Vec::new();
    let mut pen = (0, 0);
    let mut start = (0, 0);
    for command in path.commands {
        match *command {
            Command::ArcTo {
                h_r,
                st_ang,
                sw_ang,
                w_r,
            } => {
                let radii = (value(w_r, names), value(h_r, names));
                if let Some((curves, end)) =
                    arc(pen, radii, value(st_ang, names), value(sw_ang, names))
                {
                    commands.extend(curves.into_iter().map(|[control_1, control_2, to]| {
                        PathCommand::CubicTo {
                            control_1: point(control_1),
                            control_2: point(control_2),
                            end: point(to),
                        }
                    }));
                    pen = end;
                }
            }
            Command::Close => {
                commands.push(PathCommand::Close);
                pen = start;
            }
            Command::CubicTo(control_1, control_2, to) => {
                pen = place(to);
                commands.push(PathCommand::CubicTo {
                    control_1: point(place(control_1)),
                    control_2: point(place(control_2)),
                    end: point(pen),
                });
            }
            Command::LineTo(x, y) => {
                pen = place((x, y));
                commands.push(PathCommand::LineTo(point(pen)));
            }
            Command::MoveTo(x, y) => {
                pen = place((x, y));
                start = pen;
                commands.push(PathCommand::MoveTo(point(pen)));
            }
            Command::QuadTo(control, to) => {
                pen = place(to);
                commands.push(PathCommand::QuadTo {
                    control: point(place(control)),
                    end: point(pen),
                });
            }
        }
    }
    OutlinePath {
        commands,
        fill: match path.fill.trim() {
            "darken" => PathFill::Darken,
            "darkenLess" => PathFill::DarkenLess,
            "lighten" => PathFill::Lighten,
            "lightenLess" => PathFill::LightenLess,
            "none" => PathFill::None,
            _ => PathFill::Norm,
        },
        stroke: path.stroke,
    }
}

/// Where the ray at the angle `ray` meets the ellipse of `radii`, as the ellipse's own angle
/// within half a turn of `ray` (section 20.1.9.4).
fn parameter(ray: i128, (w_r, h_r): (i128, i128)) -> Option<i128> {
    let (sine, cosine) = sine_cosine(ray);
    let own = angle(h_r.checked_mul(cosine)?, w_r.checked_mul(sine)?);
    Some(own + divide(ray - own, TURN)? * TURN)
}

/// The `x` and `y` of a typed point, each empty when absent.
fn place(point: Option<&ShapePathPoint>) -> [Cow<'_, str>; 2] {
    point.map_or([Cow::Borrowed(""), Cow::Borrowed("")], |found| {
        [text(found.x.as_ref()), text(found.y.as_ref())]
    })
}

/// The outline of the preset `name` in a frame of `width` by `height` (section 20.1.9.18).
///
/// `adjust`, the shape's own `a:avLst`, replaces the preset's adjust values of the same name.
/// `upArrow`, which `presetShapeDefinitions.xml` leaves out, is `downArrow` turned top to bottom.
///
/// # Errors
///
/// [`UnknownPreset`] for a name `presetShapeDefinitions.xml` does not define, `upArrow` apart.
pub fn preset(
    name: &str,
    adjust: Option<&ListOfShapeAdjustValues>,
    width: i64,
    height: i64,
) -> Result<Outline, UnknownPreset> {
    let own = adjust.map_or_else(Vec::new, |list| named(&list.shape_guide));
    let found = |looked_up: &str| {
        PRESETS
            .binary_search_by(|candidate| candidate.name.cmp(looked_up))
            .ok()
            .and_then(|index| PRESETS.get(index))
            .ok_or_else(|| UnknownPreset(name.to_owned()))
    };
    if name == "upArrow" {
        return Ok(turned(
            outline(found("downArrow")?, &own, width, height),
            height,
        ));
    }
    Ok(outline(found(name)?, &own, width, height))
}

/// The sine and the cosine of the first eighth of a turn, `angle` a guide's value, each a share
/// of [`ONE`], by their series summed until a term is 0.
const fn series(angle: i128) -> (i128, i128) {
    let radians = (angle * PI + TURN / 4) / (TURN / 2);
    let square = radians * radians / ONE;
    let mut sine = radians;
    let mut term = radians;
    let mut index = 1;
    while term != 0 {
        term = -term * square / ONE / ((2 * index) * (2 * index + 1));
        sine += term;
        index += 1;
    }
    let mut cosine = ONE;
    term = ONE;
    index = 1;
    while term != 0 {
        term = -term * square / ONE / ((2 * index - 1) * (2 * index));
        cosine += term;
        index += 1;
    }
    (sine, cosine)
}

/// `value` times `share`, a share of [`ONE`], rounded; `None` when it does not fit.
fn shares(value: i128, share: i128) -> Option<i128> {
    divide(value.checked_mul(share)?, ONE)
}

/// The sine and the cosine of `angle`, a guide's value in 60000ths of a degree, each a share of
/// [`ONE`]: within 2^-54 of the true value, and exact at each multiple of a quarter turn.
const fn sine_cosine(angle: i128) -> (i128, i128) {
    let quarter = TURN / 4;
    let turned = angle.rem_euclid(TURN);
    let within = turned.rem_euclid(quarter);
    let (sine, cosine) = if within <= quarter / 2 {
        series(within)
    } else {
        let (other_sine, other_cosine) = series(quarter - within);
        (other_cosine, other_sine)
    };
    match turned / quarter {
        0 => (sine, cosine),
        1 => (cosine, -sine),
        2 => (-sine, -cosine),
        _ => (-cosine, sine),
    }
}

/// The greatest whole number whose square is not above `value`, 0 for a negative value: never
/// above the true square root, and less than 1 below it.
fn square_root(value: i128) -> i128 {
    u128::try_from(value).map_or(0, |unsigned| {
        i128::try_from(unsigned.isqrt()).unwrap_or_default()
    })
}

/// The spelling of a typed value.
fn spelled<T>(value: &T) -> Cow<'_, str>
where
    T: Spelling,
{
    value.spelling(ConformanceClass::Transitional)
}

/// A path step of a typed custom geometry over `texts`, the values `step_texts` gave it; `None`
/// for an element the schemas do not declare.
fn step<'text>(content: &ShapePathContent, texts: &'text [Cow<'_, str>]) -> Option<Command<'text>> {
    let at = |index: usize| texts.get(index).map_or("", |found| &**found);
    match content {
        ShapePathContent::CloseShapePath(_) => Some(Command::Close),
        ShapePathContent::DrawArcTo(_) => Some(Command::ArcTo {
            h_r: at(0),
            st_ang: at(1),
            sw_ang: at(2),
            w_r: at(3),
        }),
        ShapePathContent::DrawCubicBezierCurveTo(_) => Some(Command::CubicTo(
            (at(0), at(1)),
            (at(2), at(3)),
            (at(4), at(5)),
        )),
        ShapePathContent::DrawLineTo(_) => Some(Command::LineTo(at(0), at(1))),
        ShapePathContent::DrawQuadraticBezierCurveTo(_) => {
            Some(Command::QuadTo((at(0), at(1)), (at(2), at(3))))
        }
        ShapePathContent::MovePathTo(_) => Some(Command::MoveTo(at(0), at(1))),
        ShapePathContent::Other(_) | _ => None,
    }
}

/// The values of a path step of a typed custom geometry, each as its text, in the order `step`
/// reads them: an arc's radii and angles, then each point's `x` and `y`.
fn step_texts(content: &ShapePathContent) -> Vec<Cow<'_, str>> {
    match content {
        ShapePathContent::DrawArcTo(arc_to) => vec![
            text(arc_to.h_r.as_ref()),
            text(arc_to.st_ang.as_ref()),
            text(arc_to.sw_ang.as_ref()),
            text(arc_to.w_r.as_ref()),
        ],
        ShapePathContent::DrawCubicBezierCurveTo(curve) => (0..3)
            .flat_map(|index| place(curve.shape_path_point.get(index)))
            .collect(),
        ShapePathContent::DrawQuadraticBezierCurveTo(curve) => (0..2)
            .flat_map(|index| place(curve.shape_path_point.get(index)))
            .collect(),
        ShapePathContent::DrawLineTo(line) => place(Some(&line.shape_path_point)).into(),
        ShapePathContent::MovePathTo(moved) => place(Some(&moved.shape_path_point)).into(),
        ShapePathContent::CloseShapePath(_) | ShapePathContent::Other(_) | _ => Vec::new(),
    }
}

/// The text of a typed value, empty when absent.
fn text<T>(value: Option<&T>) -> Cow<'_, str>
where
    T: Spelling,
{
    value.map_or(Cow::Borrowed(""), spelled)
}

/// `outline` turned top to bottom in a frame `height` high: every y replaced by `height - y`.
fn turned(outline: Outline, height: i64) -> Outline {
    let flip = |y: i64| height.checked_sub(y).unwrap_or_default();
    let point = |found: Point| Point {
        x: found.x,
        y: flip(found.y),
    };
    let paths = outline
        .paths
        .into_iter()
        .map(|path| OutlinePath {
            commands: path
                .commands
                .into_iter()
                .map(|command| match command {
                    PathCommand::Close => PathCommand::Close,
                    PathCommand::CubicTo {
                        control_1,
                        control_2,
                        end,
                    } => PathCommand::CubicTo {
                        control_1: point(control_1),
                        control_2: point(control_2),
                        end: point(end),
                    },
                    PathCommand::LineTo(to) => PathCommand::LineTo(point(to)),
                    PathCommand::MoveTo(to) => PathCommand::MoveTo(point(to)),
                    PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                        control: point(control),
                        end: point(end),
                    },
                })
                .collect(),
            ..path
        })
        .collect();
    let rectangle = outline.text_rectangle;
    Outline {
        paths,
        text_rectangle: TextRectangle {
            bottom: flip(rectangle.top),
            top: flip(rectangle.bottom),
            ..rectangle
        },
    }
}

/// A guide's value rounded to the nearest unit, a half away from zero; 0 when it does not fit.
fn unit(parts: i128) -> i64 {
    divide(parts, SCALE)
        .and_then(|units| i64::try_from(units).ok())
        .unwrap_or_default()
}

/// The value of `text`, a whole number, a universal measure or a name `names` holds, as a guide's
/// value; 0 for anything else.
fn value(text: &str, names: &BTreeMap<&str, i128>) -> i128 {
    let trimmed = text.trim();
    trimmed.parse::<i64>().map_or_else(
        |_| {
            measure(trimmed)
                .or_else(|| names.get(text).copied())
                .unwrap_or_default()
        },
        |number| i128::from(number) * SCALE,
    )
}
