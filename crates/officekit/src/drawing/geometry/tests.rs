use officekit_markup::drawing::{
    AdjustableAngleMethods, AdjustableCoordinateMethods, CustomGeometry, DrawArcTo, DrawLineTo,
    ListOfShapeAdjustValues, ListOfShapeGuides, ListOfShapePaths, MovePathTo, PathFillMode,
    PositiveCoordinate, PresetShapeTypes, ShapeGuide, ShapePath, ShapePathContent, ShapePathPoint,
    ShapeTextRectangle,
};

use super::presets::PRESETS;
use super::{
    ONE, Outline, OutlinePath, PathCommand, PathFill, Point, SCALE, TURN, TextRectangle,
    UnknownPreset, angle, custom, preset, sine_cosine, square_root,
};

/// A 60000th of a degree as a guide's value.
const DEGREE: i128 = 60_000 * SCALE;

/// How far a sine or a cosine may be from the true value: 2^-54.
const SHARE_ERROR: i128 = ONE >> 54;

/// The point at `x` and `y`.
const fn at(x: i64, y: i64) -> Point {
    Point { x, y }
}

/// An `a:arcTo` of the radii `w_r` and `h_r` from the angle `start` through `swing`.
fn arc_to(w_r: &str, h_r: &str, start: &str, swing: &str) -> ShapePathContent {
    let mut arc = DrawArcTo::default();
    arc.w_r = Some(AdjustableCoordinateMethods::from(w_r.to_owned()));
    arc.h_r = Some(AdjustableCoordinateMethods::from(h_r.to_owned()));
    arc.st_ang = Some(AdjustableAngleMethods::from(start.to_owned()));
    arc.sw_ang = Some(AdjustableAngleMethods::from(swing.to_owned()));
    ShapePathContent::DrawArcTo(arc)
}

/// A custom geometry of `guides` and `paths`.
fn geometry(guides: Vec<ShapeGuide>, paths: Vec<ShapePath>) -> CustomGeometry {
    let mut geometry = CustomGeometry::default();
    if !guides.is_empty() {
        let mut list = ListOfShapeGuides::default();
        list.shape_guide = guides;
        geometry.list_of_shape_guides = Some(Box::new(list));
    }
    let mut list = ListOfShapePaths::default();
    list.shape_path = paths;
    geometry.list_of_shape_paths = Box::new(list);
    geometry
}

/// An `a:gd` named `name` of the formula `fmla`.
fn guide(name: &str, fmla: &str) -> ShapeGuide {
    let mut guide = ShapeGuide::default();
    guide.name = Some(name.to_owned());
    guide.fmla = Some(fmla.to_owned());
    guide
}

/// Each command's letter: `M` a move, `L` a line, `Q` and `C` the curves, `Z` a close.
fn letters(commands: &[PathCommand]) -> String {
    commands
        .iter()
        .map(|command| match *command {
            PathCommand::Close => 'Z',
            PathCommand::CubicTo {
                control_1: _first,
                control_2: _second,
                end: _end,
            } => 'C',
            PathCommand::LineTo(_) => 'L',
            PathCommand::MoveTo(_) => 'M',
            PathCommand::QuadTo {
                control: _control,
                end: _end,
            } => 'Q',
        })
        .collect()
}

/// An `a:lnTo` to `x` and `y`.
fn line_to(x: &str, y: &str) -> ShapePathContent {
    let mut line = DrawLineTo::default();
    line.shape_path_point = Box::new(point(x, y));
    ShapePathContent::DrawLineTo(line)
}

/// An `a:moveTo` to `x` and `y`.
fn move_to(x: &str, y: &str) -> ShapePathContent {
    let mut moved = MovePathTo::default();
    moved.shape_path_point = Box::new(point(x, y));
    ShapePathContent::MovePathTo(moved)
}

/// An `a:path` of `content`.
fn path(content: Vec<ShapePathContent>) -> ShapePath {
    let mut path = ShapePath::default();
    path.content = content;
    path
}

/// An `a:pt` at `x` and `y`.
fn point(x: &str, y: &str) -> ShapePathPoint {
    let mut point = ShapePathPoint::default();
    point.x = Some(AdjustableCoordinateMethods::from(x.to_owned()));
    point.y = Some(AdjustableCoordinateMethods::from(y.to_owned()));
    point
}

/// Every point of an outline.
fn points(outline: &Outline) -> Vec<Point> {
    outline
        .paths
        .iter()
        .flat_map(|path| &path.commands)
        .flat_map(|command| match *command {
            PathCommand::CubicTo {
                control_1,
                control_2,
                end,
            } => vec![control_1, control_2, end],
            PathCommand::LineTo(to) | PathCommand::MoveTo(to) => vec![to],
            PathCommand::QuadTo { control, end } => vec![control, end],
            PathCommand::Close => Vec::new(),
        })
        .collect()
}

/// The commands of the only path of `outline`.
fn only_path(outline: &Outline) -> &[PathCommand] {
    assert_eq!(outline.paths.len(), 1);
    &outline.paths[0].commands
}

#[test]
fn right_arrow_in_100_by_60() {
    let outline = preset("rightArrow", None, 100, 60).unwrap();
    assert_eq!(
        only_path(&outline),
        [
            PathCommand::MoveTo(at(0, 15)),
            PathCommand::LineTo(at(70, 15)),
            PathCommand::LineTo(at(70, 0)),
            PathCommand::LineTo(at(100, 30)),
            PathCommand::LineTo(at(70, 60)),
            PathCommand::LineTo(at(70, 45)),
            PathCommand::LineTo(at(0, 45)),
            PathCommand::Close,
        ]
    );
    assert_eq!(outline.paths[0].fill, PathFill::Norm);
    assert!(outline.paths[0].stroke);
}

#[test]
fn the_table_and_up_arrow_are_the_187_members_of_st_shape_type() {
    // The file's 187 definitions hold `upDownArrow` twice and no `upArrow`.
    assert_eq!(PRESETS.len(), 186);
    assert!(PRESETS.is_sorted_by(|left, right| left.name < right.name));
    assert!(PRESETS.iter().all(|found| found.name != "upArrow"));
    let names = PRESETS.iter().map(|found| found.name).chain(["upArrow"]);
    for name in names {
        assert!(
            PresetShapeTypes::from(name.to_owned()).value().is_ok(),
            "{name}"
        );
    }
}

#[test]
fn every_preset_gives_a_path_in_both_frames() {
    let names = PRESETS.iter().map(|found| found.name).chain(["upArrow"]);
    for name in names {
        let sized = PRESETS
            .iter()
            .find(|found| found.name == name)
            .is_some_and(|found| {
                found
                    .paths
                    .iter()
                    .any(|path| path.w.is_some() || path.h.is_some())
            });
        for (width, height) in [(100, 60), (914_400, 914_400)] {
            let outline = preset(name, None, width, height).unwrap();
            assert!(!outline.paths.is_empty(), "{name} has no path");
            if sized {
                continue;
            }
            for found in points(&outline) {
                assert!(
                    (-width..=2 * width).contains(&found.x)
                        && (-height..=2 * height).contains(&found.y),
                    "{name} in {width} by {height} has the point {found:?}"
                );
            }
        }
    }
}

#[test]
fn an_arc_of_a_quarter_turn_is_one_cubic_curve() {
    let shape = geometry(
        Vec::new(),
        vec![path(vec![
            move_to("100", "50"),
            arc_to("50", "50", "0", "5400000"),
        ])],
    );
    assert_eq!(
        only_path(&custom(&shape, 100, 100)),
        [
            PathCommand::MoveTo(at(100, 50)),
            PathCommand::CubicTo {
                control_1: at(100, 78),
                control_2: at(78, 100),
                end: at(50, 100),
            },
        ]
    );
}

#[test]
fn an_arc_of_three_quarters_is_three_curves_ending_on_the_ray() {
    let shape = geometry(
        Vec::new(),
        vec![path(vec![
            move_to("90", "30"),
            arc_to("40", "20", "0", "16200000"),
        ])],
    );
    let outline = custom(&shape, 100, 100);
    assert_eq!(letters(only_path(&outline)), "MCCC");
    let ends: Vec<Point> = points(&outline).into_iter().skip(3).step_by(3).collect();
    assert_eq!(ends, [at(50, 50), at(10, 30), at(50, 10)]);
}

#[test]
fn an_arc_starts_and_ends_where_the_rays_meet_the_ellipse() {
    // The ray at 45 degrees meets the ellipse of radii 40 and 20 at (17.889, 17.889) from its
    // centre, so the centre is at (50.111, 50.111) and the ray at 90 degrees ends the arc 20 below.
    let shape = geometry(
        Vec::new(),
        vec![path(vec![
            move_to("68", "68"),
            arc_to("40", "20", "2700000", "2700000"),
        ])],
    );
    let outline = custom(&shape, 100, 100);
    assert_eq!(letters(only_path(&outline)), "MC");
    assert_eq!(points(&outline).last(), Some(&at(50, 70)));
}

#[test]
fn an_arc_with_no_radius_or_no_swing_draws_nothing() {
    let shape = geometry(
        Vec::new(),
        vec![
            path(vec![move_to("10", "10"), arc_to("0", "20", "0", "5400000")]),
            path(vec![move_to("10", "10"), arc_to("20", "20", "0", "0")]),
        ],
    );
    let outline = custom(&shape, 100, 100);
    for found in &outline.paths {
        assert_eq!(found.commands, [PathCommand::MoveTo(at(10, 10))]);
    }
}

#[test]
fn the_shapes_own_adjust_value_replaces_the_presets() {
    let mut own = ListOfShapeAdjustValues::default();
    own.shape_guide = vec![guide("adj1", "val 100000")];
    let outline = preset("rightArrow", Some(&own), 100, 60).unwrap();
    assert_eq!(only_path(&outline)[0], PathCommand::MoveTo(at(0, 0)));
}

#[test]
fn up_arrow_is_down_arrow_turned_top_to_bottom() {
    let up = preset("upArrow", None, 100, 60).unwrap();
    let down = preset("downArrow", None, 100, 60).unwrap();
    let turned: Vec<Point> = points(&down)
        .into_iter()
        .map(|found| at(found.x, 60 - found.y))
        .collect();
    assert_eq!(points(&up), turned);
    assert_eq!(
        (up.text_rectangle.top, up.text_rectangle.bottom),
        (
            60 - down.text_rectangle.bottom,
            60 - down.text_rectangle.top
        )
    );
}

#[test]
fn an_unknown_name_is_an_error() {
    assert_eq!(
        preset("noSuchShape", None, 1, 1),
        Err(UnknownPreset("noSuchShape".to_owned()))
    );
}

#[test]
fn the_text_rectangle_is_the_rect_or_the_whole_frame() {
    let rounded = preset("roundRect", None, 100, 60).unwrap();
    assert_eq!(
        rounded.text_rectangle,
        TextRectangle {
            bottom: 57,
            left: 3,
            right: 97,
            top: 3,
        }
    );
    let plain = custom(
        &geometry(Vec::new(), vec![path(vec![move_to("0", "0")])]),
        100,
        60,
    );
    assert_eq!(
        plain.text_rectangle,
        TextRectangle {
            bottom: 60,
            left: 0,
            right: 100,
            top: 0,
        }
    );
    let mut rect = ShapeTextRectangle::default();
    rect.l = Some(AdjustableCoordinateMethods::from("wd4".to_owned()));
    rect.t = Some(AdjustableCoordinateMethods::from("5".to_owned()));
    rect.r = Some(AdjustableCoordinateMethods::from("hc".to_owned()));
    rect.b = Some(AdjustableCoordinateMethods::from("vc".to_owned()));
    let mut shape = geometry(Vec::new(), vec![path(vec![move_to("0", "0")])]);
    shape.shape_text_rectangle = Some(Box::new(rect));
    assert_eq!(
        custom(&shape, 100, 60).text_rectangle,
        TextRectangle {
            bottom: 30,
            left: 25,
            right: 50,
            top: 5,
        }
    );
}

#[test]
fn a_path_of_its_own_size_is_stretched_to_the_frame() {
    let mut sized = path(vec![move_to("0", "0"), line_to("10", "10")]);
    sized.w = Some(PositiveCoordinate::from("10".to_owned()));
    sized.h = Some(PositiveCoordinate::from("10".to_owned()));
    let outline = custom(&geometry(Vec::new(), vec![sized]), 100, 60);
    assert_eq!(
        only_path(&outline),
        [
            PathCommand::MoveTo(at(0, 0)),
            PathCommand::LineTo(at(100, 60))
        ]
    );
}

#[test]
fn fill_modes_and_stroke_are_kept_in_order() {
    let mut paths = Vec::new();
    for fill in ["none", "darken", "lightenLess"] {
        let mut filled = path(vec![move_to("0", "0")]);
        filled.fill = Some(PathFillMode::from(fill.to_owned()));
        paths.push(filled);
    }
    let mut unstroked = path(vec![move_to("0", "0")]);
    unstroked.stroke = Some("false".to_owned());
    paths.push(unstroked);
    let outline = custom(&geometry(Vec::new(), paths), 10, 10);
    let found: Vec<(PathFill, bool)> = outline
        .paths
        .iter()
        .map(|path: &OutlinePath| (path.fill, path.stroke))
        .collect();
    assert_eq!(
        found,
        [
            (PathFill::None, true),
            (PathFill::Darken, true),
            (PathFill::LightenLess, true),
            (PathFill::Norm, false),
        ]
    );
}

#[test]
fn each_formula_gives_the_value_its_definition_gives() {
    let formulas = [
        ("*/ 6 4 3", 8),
        ("+- 5 4 2", 7),
        ("+/ 5 4 3", 3),
        ("?: -1 4 9", 9),
        ("?: 1 4 9", 4),
        ("abs -6", 6),
        ("at2 1 1", 2_700_000),
        ("at2 -1 -1", -8_100_000),
        ("cat2 10 3 4", 6),
        ("cat2 10 -3 4", -6),
        ("cos 10 3600000", 5),
        ("max 3 8", 8),
        ("min 3 8", 3),
        ("mod 2 3 6", 7),
        ("pin 0 15 10", 10),
        ("sat2 10 3 4", 8),
        ("sin 10 1800000", 5),
        ("sqrt 49", 7),
        ("tan 10 2700000", 10),
        ("val 7", 7),
    ];
    let guides = formulas
        .iter()
        .enumerate()
        .map(|(index, (formula, _))| guide(&format!("g{index}"), formula))
        .collect();
    let moves = (0..formulas.len())
        .map(|index| move_to(&format!("g{index}"), "0"))
        .collect();
    let outline = custom(&geometry(guides, vec![path(moves)]), 100, 100);
    let found: Vec<i64> = points(&outline).iter().map(|place| place.x).collect();
    let expected: Vec<i64> = formulas.iter().map(|(_, value)| *value).collect();
    assert_eq!(found, expected);
}

#[test]
fn a_name_nothing_defines_and_a_division_by_zero_are_0() {
    let shape = geometry(
        vec![guide("zero", "*/ 1 1 0")],
        vec![path(vec![move_to("zero", "nothing")])],
    );
    assert_eq!(
        only_path(&custom(&shape, 100, 100)),
        [PathCommand::MoveTo(at(0, 0))]
    );
}

#[test]
fn the_sine_is_exact_at_each_quarter_turn_and_close_at_30_degrees() {
    assert_eq!(sine_cosine(0), (0, ONE));
    assert_eq!(sine_cosine(90 * DEGREE), (ONE, 0));
    assert_eq!(sine_cosine(180 * DEGREE), (0, -ONE));
    assert_eq!(sine_cosine(270 * DEGREE), (-ONE, 0));
    assert_eq!(sine_cosine(-90 * DEGREE), (-ONE, 0));
    let (sine, cosine) = sine_cosine(30 * DEGREE);
    assert!((sine - ONE / 2).abs() <= SHARE_ERROR, "{sine}");
    let (other_sine, other_cosine) = sine_cosine(60 * DEGREE);
    assert!(
        (other_cosine - ONE / 2).abs() <= SHARE_ERROR,
        "{other_cosine}"
    );
    assert_eq!((sine, cosine), (other_cosine, other_sine));
}

#[test]
fn the_angle_of_the_tangents_0_1_and_minus_1() {
    assert_eq!(angle(SCALE, 0), 0);
    assert!((angle(SCALE, SCALE) - TURN / 8).abs() <= 1);
    assert!((angle(SCALE, -SCALE) + TURN / 8).abs() <= 1);
    assert_eq!(angle(-SCALE, 0), TURN / 2);
    assert_eq!(angle(0, SCALE), TURN / 4);
    assert_eq!(angle(0, -SCALE), -TURN / 4);
    assert_eq!(angle(0, 0), 0);
}

#[test]
fn the_square_roots_of_0_1_2_and_10_to_the_18() {
    assert_eq!(square_root(0), 0);
    assert_eq!(square_root(1), 1);
    assert_eq!(square_root(2), 1);
    assert_eq!(square_root(1_000_000_000_000_000_000), 1_000_000_000);
    assert_eq!(square_root(-4), 0);
    // sqrt 2 as a guide's value: 1.41421356 units are 92681.9 parts.
    assert_eq!(square_root(2 * SCALE * SCALE), 92_681);
}
