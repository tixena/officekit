use officekit::drawing::geometry::{Outline, PathCommand, PathFill, UnknownPreset, custom, preset};
use officekit_markup::drawing::{
    AdjustableCoordinateMethods, CustomGeometry, DrawLineTo, GeometryContent, ListOfShapePaths,
    MovePathTo, PresetGeometry, PresetShapeTypes, ShapePath, ShapePathContent, ShapePathPoint,
};

/// The outline of a shape's geometry in a frame of `width` by `height`; a shape that states
/// none has the rectangle of its frame.
fn outline_of(
    geometry: Option<&GeometryContent>,
    width: i64,
    height: i64,
) -> Result<Outline, UnknownPreset> {
    match geometry {
        Some(GeometryContent::PresetGeometry(found)) => {
            let name = found.prst.as_ref().map_or("rect", |value| value.0.as_str());
            preset(
                name,
                found.list_of_shape_adjust_values.as_deref(),
                width,
                height,
            )
        }
        Some(GeometryContent::CustomGeometry(found)) => Ok(custom(found, width, height)),
        Some(GeometryContent::Other(_) | _) | None => preset("rect", None, width, height),
    }
}

/// An `a:pt` at `x` and `y`.
fn point(x: &str, y: &str) -> Box<ShapePathPoint> {
    let mut point = ShapePathPoint::default();
    point.x = Some(AdjustableCoordinateMethods::from(x.to_owned()));
    point.y = Some(AdjustableCoordinateMethods::from(y.to_owned()));
    Box::new(point)
}

/// The points the moves and lines of the first path go to.
fn corners(outline: &Outline) -> Vec<(i64, i64)> {
    outline.paths[0]
        .commands
        .iter()
        .filter_map(|command| {
            if let PathCommand::MoveTo(found) | PathCommand::LineTo(found) = *command {
                Some((found.x, found.y))
            } else {
                None
            }
        })
        .collect()
}

/// An `a:prstGeom` of the preset `name`.
fn preset_geometry(name: &str) -> GeometryContent {
    let mut geometry = PresetGeometry::default();
    geometry.prst = Some(PresetShapeTypes::from(name.to_owned()));
    GeometryContent::PresetGeometry(geometry)
}

#[test]
fn a_preset_geometry_gives_its_paths_and_text_rectangle() {
    let outline = outline_of(Some(&preset_geometry("rightArrow")), 100, 60).unwrap();
    assert_eq!(
        corners(&outline),
        [
            (0, 15),
            (70, 15),
            (70, 0),
            (100, 30),
            (70, 60),
            (70, 45),
            (0, 45)
        ]
    );
    assert!(matches!(
        outline.paths[0].commands.last(),
        Some(PathCommand::Close)
    ));
    assert_eq!(outline.paths[0].fill, PathFill::Norm);
    assert!(outline.paths[0].stroke);
    let rectangle = outline.text_rectangle;
    assert_eq!(
        (
            rectangle.left,
            rectangle.top,
            rectangle.right,
            rectangle.bottom
        ),
        (0, 15, 85, 45)
    );
}

#[test]
fn a_custom_geometry_gives_its_paths() {
    let mut moved = MovePathTo::default();
    moved.shape_path_point = point("l", "b");
    let mut line = DrawLineTo::default();
    line.shape_path_point = point("hc", "t");
    let mut path = ShapePath::default();
    path.content = vec![
        ShapePathContent::MovePathTo(moved),
        ShapePathContent::DrawLineTo(line),
    ];
    let mut paths = ListOfShapePaths::default();
    paths.shape_path = vec![path];
    let mut geometry = CustomGeometry::default();
    geometry.list_of_shape_paths = Box::new(paths);
    let outline = outline_of(Some(&GeometryContent::CustomGeometry(geometry)), 80, 40).unwrap();
    assert_eq!(corners(&outline), [(0, 40), (40, 0)]);
}

#[test]
fn an_unknown_preset_is_an_error_naming_it() {
    let error = outline_of(Some(&preset_geometry("noSuchShape")), 100, 60).unwrap_err();
    assert_eq!(error.0, "noSuchShape");
    assert_eq!(
        error.to_string(),
        "the standard defines no preset shape named noSuchShape"
    );
}

#[test]
fn a_shape_with_no_geometry_has_the_rectangle_of_its_frame() {
    let outline = outline_of(None, 100, 60).unwrap();
    assert_eq!(corners(&outline), [(0, 0), (100, 0), (100, 60), (0, 60)]);
}
