use officekit_markup::common_simple_types::HexadecimalColorValue;
use officekit_markup::drawing::{
    AlignmentType, AlignmentTypeMember, BackgroundColor, ColorContent, ColorScheme,
    CompoundLineType, CompoundLineTypeMember, EndLineCap, EndLineCapMember, FillPropertiesContent,
    FillReference, ForegroundColor, FormatScheme, GradientFill, GradientStopList, GradientStops,
    GroupFill, LineDashContent, LineFillContent, LineJoinBevel, LineJoinContent, LineReference,
    LineWidth, MiterLineJoin, NoFill, Outline, PatternFill, PictureFill, PositiveFixedPercentage,
    PositivePercentageValueWithSign, PresetDash, PresetLineDashValue, PresetLineDashValueMember,
    RGBColorModelHexVariant, SchemeColor, SolidFill, StSchemeColorVal, StSchemeColorValMember,
    StyleMatrixColumnIndex,
};
use officekit_markup::{
    chart, chart_drawing, diagram, drawing, office_drawing_2007_8_2_chart,
    office_drawing_2008_diagram, office_drawing_2010_picture, office_drawing_2012_chart,
    office_drawing_2012_chart_style, office_drawing_2014_chart, office_drawing_2014_chartex,
    office_drawing_2016_12_diagram, office_drawing_2017_model3d,
    office_word_2010_wordprocessing_shape, picture, presentation, spreadsheet_drawing,
    wordprocessing_drawing,
};

use super::{Fill, Formats, GradientStop, Line, ShapeFormat, StyleReferences, fill, line};
use crate::drawing::color::Rgba;
use crate::drawing::scheme::{SchemeMap, ThemeColors};

/// For each `spPr` type given: a node stating `a:noFill` and an `a:ln`, checked by
/// [`properties_answer`].
macro_rules! check_properties {
    ($($properties:ty),+ $(,)?) => {$({
        let mut stating = <$properties>::default();
        stating.fill_properties_content = Some(Box::new(no_fill()));
        stating.outline = Some(Box::new(width_outline("25400")));
        properties_answer(stringify!($properties), &stating);
    })+};
}

/// For each `style` type given: a node with the references of scenario 1, checked by
/// [`style_answer`].
macro_rules! check_styles {
    ($($style:ty),+ $(,)?) => {$({
        let mut style = <$style>::default();
        *style.fill_reference = accent_fill_reference(2);
        *style.line_reference = accent_line_reference(2);
        style_answer(stringify!($style), &style);
    })+};
}

/// `stating` gives its `a:noFill` and its `a:ln`, and a default node of its type takes the fill
/// of scenario 1.
fn properties_answer<P>(name: &str, stating: &P)
where
    P: ShapeFormat + Default,
{
    assert_eq!(stating.fill(), Some(&no_fill()), "{name}");
    assert_eq!(stating.outline(), Some(&width_outline("25400")), "{name}");
    let style = shape_style(2, 2);
    assert_eq!(
        fill(
            &P::default(),
            Some(&style),
            formats_of(&theme_colors(), &scheme())
        ),
        Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))),
        "{name}",
    );
}

/// `style` gives the references of scenario 1, and a shape under it takes that fill.
fn style_answer<S>(name: &str, style: &S)
where
    S: StyleReferences,
{
    assert_eq!(
        style.fill_reference(),
        Some(&accent_fill_reference(2)),
        "{name}"
    );
    assert_eq!(
        style.line_reference(),
        Some(&accent_line_reference(2)),
        "{name}"
    );
    assert_eq!(
        fill(
            &drawing::ShapeProperties::default(),
            Some(style),
            formats_of(&theme_colors(), &scheme()),
        ),
        Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))),
        "{name}",
    );
}

/// An `a:fillRef` of `idx` holding `accent1`.
fn accent_fill_reference(idx: u32) -> FillReference {
    fill_reference(idx, Some(scheme_color(StSchemeColorValMember::Accent1)))
}

/// An `a:lnRef` of `idx` holding `accent1`.
fn accent_line_reference(idx: u32) -> LineReference {
    line_reference(idx, Some(scheme_color(StSchemeColorValMember::Accent1)))
}

/// The opaque colour of `red`, `green` and `blue`.
const fn rgba(red: u8, green: u8, blue: u8) -> Rgba {
    Rgba::new(red, green, blue, 100_000)
}

/// An `a:srgbClr` of six hexadecimal digits.
fn hex(digits: &str) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(digits.to_owned()));
    ColorContent::RGBColorModelHexVariant(color)
}

/// An `a:schemeClr` of `name`.
fn scheme_color(name: StSchemeColorValMember) -> ColorContent {
    let mut color = SchemeColor::default();
    color.val = Some(StSchemeColorVal::from(name));
    ColorContent::SchemeColor(color)
}

fn placeholder() -> ColorContent {
    scheme_color(StSchemeColorValMember::PhClr)
}

fn solid(content: ColorContent) -> SolidFill {
    let mut solid = SolidFill::default();
    solid.content = Some(Box::new(content));
    solid
}

fn no_fill() -> FillPropertiesContent {
    FillPropertiesContent::NoFill(NoFill::default())
}

/// An `a:gs` at `position` of `content`.
fn stop(position: &str, content: ColorContent) -> GradientStops {
    let mut stop = GradientStops::default();
    stop.pos = Some(PositiveFixedPercentage::from(position.to_owned()));
    *stop.content = content;
    stop
}

/// An `a:gradFill` of black at 0 and `phClr` at 100%.
fn gradient() -> GradientFill {
    let mut list = GradientStopList::default();
    list.gradient_stops = vec![stop("0", hex("000000")), stop("100000", placeholder())];
    let mut gradient = GradientFill::default();
    gradient.gradient_stop_list = Some(Box::new(list));
    gradient
}

/// The colours of a theme whose `accent1` is 4472C4 and `accent2` ED7D31.
fn theme_colors() -> ThemeColors {
    let mut colors = ColorScheme::default();
    *colors.accent_1.content = hex("4472C4");
    *colors.accent_2.content = hex("ED7D31");
    ThemeColors::new(Some(&colors), SchemeMap::default())
}

/// An `a:ln` of `width` with a solid fill of `phClr`, a flat cap and a solid dash.
fn style_outline(width: &str) -> Outline {
    let mut outline = width_outline(width);
    outline.line_fill_content = Some(Box::new(LineFillContent::SolidFill(solid(placeholder()))));
    outline.cap = Some(EndLineCap::from(EndLineCapMember::Flat));
    outline.line_dash_content = Some(Box::new(solid_dash()));
    outline
}

fn solid_dash() -> LineDashContent {
    let mut dash = PresetDash::default();
    dash.val = Some(PresetLineDashValue::from(PresetLineDashValueMember::Solid));
    LineDashContent::PresetDash(dash)
}

/// An `a:ln` stating `w` and nothing more.
fn width_outline(width: &str) -> Outline {
    let mut outline = Outline::default();
    outline.w = Some(LineWidth::from(width.to_owned()));
    outline
}

/// A theme's `a:fmtScheme`: three fill styles, the second a solid `phClr` and the third the
/// gradient; one background fill style of 70AD47; three line styles of 6350, 12700 and 19050.
fn scheme() -> FormatScheme {
    let mut scheme = FormatScheme::default();
    scheme.fill_style_list.content = vec![
        FillPropertiesContent::SolidFill(solid(hex("111111"))),
        FillPropertiesContent::SolidFill(solid(placeholder())),
        FillPropertiesContent::GradientFill(gradient()),
    ];
    scheme.background_fill_style_list.content =
        vec![FillPropertiesContent::SolidFill(solid(hex("70AD47")))];
    scheme.line_style_list.outline = vec![
        style_outline("6350"),
        style_outline("12700"),
        style_outline("19050"),
    ];
    scheme
}

const fn formats_of<'theme>(
    colors: &'theme ThemeColors,
    scheme: &'theme FormatScheme,
) -> Formats<'theme> {
    Formats::new(colors, Some(scheme))
}

fn fill_reference(idx: u32, content: Option<ColorContent>) -> FillReference {
    let mut reference = FillReference::default();
    reference.idx = Some(StyleMatrixColumnIndex::from(idx.to_string()));
    reference.content = content.map(Box::new);
    reference
}

fn line_reference(idx: u32, content: Option<ColorContent>) -> LineReference {
    let mut reference = LineReference::default();
    reference.idx = Some(StyleMatrixColumnIndex::from(idx.to_string()));
    reference.content = content.map(Box::new);
    reference
}

/// An `a:style` whose `a:fillRef` and `a:lnRef` name `fill_idx` and `line_idx`, each holding
/// `accent1`.
fn shape_style(fill_idx: u32, line_idx: u32) -> drawing::ShapeStyle {
    let mut style = drawing::ShapeStyle::default();
    *style.fill_reference = accent_fill_reference(fill_idx);
    *style.line_reference = accent_line_reference(line_idx);
    style
}

fn properties_with_fill(content: FillPropertiesContent) -> drawing::ShapeProperties {
    let mut properties = drawing::ShapeProperties::default();
    properties.fill_properties_content = Some(Box::new(content));
    properties
}

fn properties_with_line(outline: Outline) -> drawing::ShapeProperties {
    let mut properties = drawing::ShapeProperties::default();
    properties.outline = Some(Box::new(outline));
    properties
}

/// The fill of `properties` under `style` in the theme of [`scheme`].
fn themed_fill(properties: &drawing::ShapeProperties, style: Option<&drawing::ShapeStyle>) -> Fill {
    fill(properties, style, formats_of(&theme_colors(), &scheme()))
}

/// The line of `properties` under `style` in the theme of [`scheme`].
fn themed_line(
    properties: &drawing::ShapeProperties,
    style: Option<&drawing::ShapeStyle>,
) -> Option<Line> {
    line(properties, style, formats_of(&theme_colors(), &scheme()))
}

#[test]
fn a_fill_reference_takes_the_themes_fill_style_in_the_references_colour() {
    let style = shape_style(2, 2);
    assert_eq!(
        themed_fill(&drawing::ShapeProperties::default(), Some(&style)),
        Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))),
    );
}

#[test]
fn no_fill_on_the_shape_wins_over_the_style() {
    let style = shape_style(2, 2);
    assert_eq!(
        themed_fill(&properties_with_fill(no_fill()), Some(&style)),
        Fill::None
    );
}

#[test]
fn the_shapes_own_colour_wins_over_the_style() {
    let style = shape_style(2, 2);
    let properties = properties_with_fill(FillPropertiesContent::SolidFill(solid(hex("FF0000"))));
    assert_eq!(
        themed_fill(&properties, Some(&style)),
        Fill::Solid(Some(rgba(0xFF, 0, 0))),
    );
}

#[test]
fn the_shapes_own_placeholder_has_no_colour() {
    let style = shape_style(2, 2);
    let properties = properties_with_fill(FillPropertiesContent::SolidFill(solid(placeholder())));
    assert_eq!(themed_fill(&properties, Some(&style)), Fill::Solid(None));
    let empty = properties_with_fill(FillPropertiesContent::SolidFill(SolidFill::default()));
    assert_eq!(themed_fill(&empty, Some(&style)), Fill::Solid(None));
}

#[test]
fn the_references_0_1000_and_1001() {
    let properties = drawing::ShapeProperties::default();
    assert_eq!(
        themed_fill(&properties, Some(&shape_style(0, 0))),
        Fill::None
    );
    assert_eq!(
        themed_fill(&properties, Some(&shape_style(1000, 0))),
        Fill::None
    );
    assert_eq!(
        themed_fill(&properties, Some(&shape_style(1001, 0))),
        Fill::Solid(Some(rgba(0x70, 0xAD, 0x47))),
    );
    assert_eq!(
        themed_fill(&properties, Some(&shape_style(1002, 0))),
        Fill::None
    );
}

#[test]
fn the_first_background_fill_style_takes_the_references_colour() {
    let mut scheme = scheme();
    scheme.background_fill_style_list.content =
        vec![FillPropertiesContent::SolidFill(solid(placeholder()))];
    let style = shape_style(1001, 0);
    assert_eq!(
        fill(
            &drawing::ShapeProperties::default(),
            Some(&style),
            formats_of(&theme_colors(), &scheme),
        ),
        Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))),
    );
}

#[test]
fn an_index_past_the_list_no_theme_and_no_style_are_no_fill() {
    let properties = drawing::ShapeProperties::default();
    assert_eq!(
        themed_fill(&properties, Some(&shape_style(9, 0))),
        Fill::None
    );
    let colors = theme_colors();
    assert_eq!(
        fill(
            &properties,
            Some(&shape_style(2, 0)),
            Formats::new(&colors, None)
        ),
        Fill::None,
    );
    assert_eq!(themed_fill(&properties, None), Fill::None);
}

#[test]
fn a_reference_with_no_index_is_no_fill() {
    let mut style = shape_style(2, 2);
    style.fill_reference.idx = None;
    assert_eq!(
        themed_fill(&drawing::ShapeProperties::default(), Some(&style)),
        Fill::None
    );
}

#[test]
fn a_gradient_gives_its_stops_in_the_references_colour() {
    let style = shape_style(3, 0);
    assert_eq!(
        themed_fill(&drawing::ShapeProperties::default(), Some(&style)),
        Fill::Gradient {
            fill: gradient(),
            stops: vec![
                GradientStop::new(Some(rgba(0, 0, 0)), Some(0)),
                GradientStop::new(Some(rgba(0x44, 0x72, 0xC4)), Some(100_000)),
            ],
        },
    );
}

#[test]
fn a_stops_position_in_either_class() {
    let mut list = GradientStopList::default();
    list.gradient_stops = vec![
        stop("50%", hex("000000")),
        stop("12.5%", hex("000000")),
        stop("fifty", hex("000000")),
    ];
    let mut gradient = GradientFill::default();
    gradient.gradient_stop_list = Some(Box::new(list));
    let properties = properties_with_fill(FillPropertiesContent::GradientFill(gradient.clone()));
    let black = Some(rgba(0, 0, 0));
    assert_eq!(
        themed_fill(&properties, None),
        Fill::Gradient {
            fill: gradient,
            stops: vec![
                GradientStop::new(black, Some(50_000)),
                GradientStop::new(black, Some(12_500)),
                GradientStop::new(black, None),
            ],
        },
    );
}

#[test]
fn a_pattern_a_picture_and_a_group_fill() {
    let mut foreground = ForegroundColor::default();
    *foreground.content = hex("FF0000");
    let mut background = BackgroundColor::default();
    *background.content = scheme_color(StSchemeColorValMember::Accent2);
    let mut pattern = PatternFill::default();
    pattern.foreground_color = Some(Box::new(foreground));
    pattern.background_color = Some(Box::new(background));
    assert_eq!(
        themed_fill(
            &properties_with_fill(FillPropertiesContent::PatternFill(pattern.clone())),
            None,
        ),
        Fill::Pattern {
            background: Some(rgba(0xED, 0x7D, 0x31)),
            fill: pattern,
            foreground: Some(rgba(0xFF, 0, 0)),
        },
    );
    assert_eq!(
        themed_fill(
            &properties_with_fill(FillPropertiesContent::PictureFill(PictureFill::default())),
            None,
        ),
        Fill::Picture(PictureFill::default()),
    );
    assert_eq!(
        themed_fill(
            &properties_with_fill(FillPropertiesContent::GroupFill(GroupFill::default())),
            None,
        ),
        Fill::Group,
    );
}

#[test]
fn a_line_from_the_style() {
    let style = shape_style(0, 2);
    let line = themed_line(&drawing::ShapeProperties::default(), Some(&style)).unwrap();
    assert_eq!(line.width, 12_700);
    assert_eq!(line.fill, Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))));
    assert_eq!(line.cap, EndLineCap::from(EndLineCapMember::Flat));
    assert_eq!(line.dash, Some(solid_dash()));
}

#[test]
fn the_shapes_line_over_the_styles_one_thing_at_a_time() {
    let style = shape_style(0, 2);
    let properties = properties_with_line(width_outline("25400"));
    let line = themed_line(&properties, Some(&style)).unwrap();
    assert_eq!(line.width, 25_400);
    assert_eq!(line.fill, Fill::Solid(Some(rgba(0x44, 0x72, 0xC4))));
    assert_eq!(line.cap, EndLineCap::from(EndLineCapMember::Flat));
    assert_eq!(line.dash, Some(solid_dash()));
}

#[test]
fn the_shapes_own_line_colour_has_no_placeholder() {
    let style = shape_style(0, 2);
    let mut outline = Outline::default();
    outline.line_fill_content = Some(Box::new(LineFillContent::SolidFill(solid(placeholder()))));
    let line = themed_line(&properties_with_line(outline), Some(&style)).unwrap();
    assert_eq!(line.fill, Fill::Solid(None));
}

#[test]
fn nothing_stated_is_the_standards_defaults() {
    let properties = properties_with_line(Outline::default());
    assert_eq!(
        themed_line(&properties, None),
        Some(Line {
            alignment: None,
            cap: EndLineCap::from(EndLineCapMember::Sq),
            compound: CompoundLineType::from(CompoundLineTypeMember::Sng),
            dash: None,
            fill: Fill::None,
            head_end: None,
            join: None,
            tail_end: None,
            width: 0,
        }),
    );
}

#[test]
fn a_width_that_is_no_value_falls_to_the_style() {
    let style = shape_style(0, 2);
    let properties = properties_with_line(width_outline("wide"));
    assert_eq!(
        themed_line(&properties, Some(&style)).unwrap().width,
        12_700
    );
    assert_eq!(themed_line(&properties, None).unwrap().width, 0);
}

#[test]
fn the_alignment_and_the_compound_as_stated() {
    let mut outline = Outline::default();
    outline.algn = Some(AlignmentType::from(AlignmentTypeMember::In));
    outline.cmpd = Some(CompoundLineType::from(CompoundLineTypeMember::Dbl));
    let line = themed_line(&properties_with_line(outline), None).unwrap();
    assert_eq!(
        line.alignment,
        Some(AlignmentType::from(AlignmentTypeMember::In))
    );
    assert_eq!(
        line.compound,
        CompoundLineType::from(CompoundLineTypeMember::Dbl)
    );
}

#[test]
fn a_join_as_stated() {
    let mut limited = MiterLineJoin::default();
    limited.lim = Some(PositivePercentageValueWithSign::from("800000".to_owned()));
    let joins = [
        LineJoinContent::MiterLineJoin(limited),
        LineJoinContent::MiterLineJoin(MiterLineJoin::default()),
        LineJoinContent::LineJoinBevel(LineJoinBevel::default()),
    ];
    for join in joins {
        let mut outline = Outline::default();
        outline.line_join_content = Some(Box::new(join.clone()));
        let line = themed_line(&properties_with_line(outline), None).unwrap();
        assert_eq!(line.join, Some(join));
    }
}

#[test]
fn no_line_at_all() {
    let properties = drawing::ShapeProperties::default();
    assert_eq!(themed_line(&properties, Some(&shape_style(2, 0))), None);
    assert_eq!(themed_line(&properties, Some(&shape_style(2, 9))), None);
    assert_eq!(themed_line(&properties, None), None);
}

#[test]
fn a_lines_no_fill_wins_over_the_styles_fill() {
    let style = shape_style(0, 2);
    let mut outline = Outline::default();
    outline.line_fill_content = Some(Box::new(LineFillContent::NoFill(NoFill::default())));
    let line = themed_line(&properties_with_line(outline), Some(&style)).unwrap();
    assert_eq!(line.fill, Fill::None);
}

#[test]
fn every_shape_properties_type_gives_the_same_answer() {
    check_properties!(
        chart::ShapeProperties,
        chart_drawing::ShapeProperties,
        diagram::ShapeProperties,
        drawing::ShapeProperties,
        office_drawing_2007_8_2_chart::SpPr,
        office_drawing_2008_diagram::SpPr,
        office_drawing_2012_chart::SpPr,
        office_drawing_2012_chart_style::SpPr,
        office_drawing_2014_chart::SpPr,
        office_drawing_2014_chartex::SpPr,
        office_drawing_2016_12_diagram::SpPr,
        office_drawing_2017_model3d::SpPr,
        office_word_2010_wordprocessing_shape::SpPr,
        picture::ShapeProperties,
        presentation::ShapeProperties,
        spreadsheet_drawing::ShapeProperties,
        wordprocessing_drawing::ShapeProperties,
    );
}

#[test]
fn every_style_type_gives_the_same_answer() {
    check_styles!(
        chart_drawing::ShapeStyle,
        diagram::ShapeStyle,
        drawing::ShapeStyle,
        office_drawing_2008_diagram::Style,
        office_drawing_2010_picture::Style,
        office_word_2010_wordprocessing_shape::Style,
        presentation::ShapeStyle,
        spreadsheet_drawing::ShapeStyle,
        wordprocessing_drawing::ShapeStyle,
    );
}
