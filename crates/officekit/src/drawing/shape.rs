//! The fill and the line a shape is drawn with: its own properties over its style's references
//! into the theme (ECMA-376 Part 1, sections 20.1.2.2.24, 20.1.4.2.10 and 20.1.4.2.19).

#[cfg(test)]
mod tests;

use officekit_markup::drawing::{
    AlignmentType, ColorContent, CompoundLineType, CompoundLineTypeMember, EndLineCap,
    EndLineCapMember, FillPropertiesContent, FillReference, FormatScheme, GradientFill,
    LineDashContent, LineFillContent, LineHeadEndStyle, LineJoinContent, LineReference, Outline,
    PatternFill, PictureFill, SolidFill, StyleMatrixColumnIndex, TailLineEndStyle,
};
use officekit_markup::{
    chart, chart_drawing, diagram, drawing, office_drawing_2007_8_2_chart,
    office_drawing_2008_diagram, office_drawing_2010_picture, office_drawing_2012_chart,
    office_drawing_2012_chart_style, office_drawing_2014_chart, office_drawing_2014_chartex,
    office_drawing_2016_12_diagram, office_drawing_2017_model3d,
    office_word_2010_wordprocessing_shape, picture, presentation, spreadsheet_drawing,
    wordprocessing_drawing,
};

use super::color::{Rgba, color, positive_fixed_percentage};
use super::scheme::ThemeColors;

/// Implements [`ShapeFormat`] for each `CT_ShapeProperties` node given.
macro_rules! shape_format {
    ($($properties:ty),+ $(,)?) => {$(
        impl ShapeFormat for $properties {
            fn fill(&self) -> Option<&FillPropertiesContent> {
                self.fill_properties_content.as_deref()
            }

            fn outline(&self) -> Option<&Outline> {
                self.outline.as_deref()
            }
        }
    )+};
}

/// Implements [`StyleReferences`] for each `CT_ShapeStyle` node given.
macro_rules! style_references {
    ($($style:ty),+ $(,)?) => {$(
        impl StyleReferences for $style {
            fn fill_reference(&self) -> Option<&FillReference> {
                Some(&self.fill_reference)
            }

            fn line_reference(&self) -> Option<&LineReference> {
                Some(&self.line_reference)
            }
        }
    )+};
}

/// The fill a shape is drawn with (section 20.1.8).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Fill {
    /// `a:gradFill`: its stops in the file's order, and the node for the rest.
    Gradient {
        /// The `a:gradFill` as the file states it.
        fill: GradientFill,
        /// Its `a:gs`, each with its colour worked out.
        stops: Vec<GradientStop>,
    },
    /// `a:grpFill`: the fill of the group the shape is in.
    Group,
    /// `a:noFill`, or no fill from the shape or its style.
    None,
    /// `a:pattFill`: its two colours, and the node for the pattern.
    Pattern {
        /// The colour of `a:bgClr`; `None` when nothing gives it.
        background: Option<Rgba>,
        /// The `a:pattFill` as the file states it.
        fill: PatternFill,
        /// The colour of `a:fgClr`; `None` when nothing gives it.
        foreground: Option<Rgba>,
    },
    /// `a:blipFill`.
    Picture(PictureFill),
    /// `a:solidFill`: one colour with its opacity; `None` inside when nothing gives the colour.
    Solid(Option<Rgba>),
}

/// What a fill and a line are worked out with: the theme's lists of styles and its colours.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Formats<'theme> {
    /// The colours of the scheme names.
    pub colors: &'theme ThemeColors,
    /// The theme's `a:fmtScheme`; `None` for a file with no theme part.
    pub scheme: Option<&'theme FormatScheme>,
}

impl<'theme> Formats<'theme> {
    /// The theme's colours and its `a:fmtScheme`, `None` for a file with no theme part.
    #[must_use = "this only builds the value"]
    pub const fn new(colors: &'theme ThemeColors, scheme: Option<&'theme FormatScheme>) -> Self {
        Self { colors, scheme }
    }
}

/// One stop of a gradient, an `a:gs` (section 20.1.8.36).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct GradientStop {
    /// The stop's colour; `None` when nothing gives it.
    pub color: Option<Rgba>,
    /// The stop's place in thousandths of a percent; `None` when `pos` is absent or no value.
    pub position: Option<u32>,
}

impl GradientStop {
    /// The stop of `color` at `position`, in thousandths of a percent.
    #[must_use = "this only builds the value"]
    pub const fn new(color: Option<Rgba>, position: Option<u32>) -> Self {
        Self { color, position }
    }
}

/// The line a shape is drawn with: each thing from the shape's own `a:ln`, else from the theme's
/// line style its `a:lnRef` names (section 20.1.2.2.24).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Line {
    /// `algn` as stated; the standard states no default.
    pub alignment: Option<AlignmentType>,
    /// `cap`; `sq` when neither states it.
    pub cap: EndLineCap,
    /// `cmpd`; `sng` when neither states it.
    pub compound: CompoundLineType,
    /// `a:prstDash` or `a:custDash`, as stated.
    pub dash: Option<LineDashContent>,
    /// The line's fill, read as a shape's; [`Fill::None`] when neither states one.
    pub fill: Fill,
    /// `a:headEnd`, as stated.
    pub head_end: Option<LineHeadEndStyle>,
    /// `a:round`, `a:bevel` or `a:miter` with its `lim`, as stated.
    pub join: Option<LineJoinContent>,
    /// `a:tailEnd`, as stated.
    pub tail_end: Option<TailLineEndStyle>,
    /// `w` in English Metric Units; 0 when neither states one that is a value of `ST_LineWidth`.
    pub width: i64,
}

/// The properties of a shape: an `spPr` of any namespace.
pub trait ShapeFormat {
    /// The fill the shape states.
    fn fill(&self) -> Option<&FillPropertiesContent>;
    /// The shape's own `a:ln`.
    fn outline(&self) -> Option<&Outline>;
}

/// A shape's style: a `style` element of any namespace (section 20.1.2.2.37).
pub trait StyleReferences {
    /// The style's `a:fillRef`.
    fn fill_reference(&self) -> Option<&FillReference>;
    /// The style's `a:lnRef`.
    fn line_reference(&self) -> Option<&LineReference>;
}

shape_format!(
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

style_references!(
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

/// The fill of a shape whose properties are `properties` and whose style is `style`: the shape's
/// own fill, else the fill style its `a:fillRef` names (section 20.1.4.2.10).
#[must_use = "this only returns the value"]
pub fn fill<P, S>(properties: &P, style: Option<&S>, formats: Formats<'_>) -> Fill
where
    P: ShapeFormat + ?Sized,
    S: StyleReferences + ?Sized,
{
    if let Some(own) = properties.fill() {
        return read_fill(own, formats.colors, None);
    }
    style
        .and_then(StyleReferences::fill_reference)
        .and_then(|reference| referenced_fill(reference, formats))
        .unwrap_or(Fill::None)
}

/// The fill of an `a:gradFill`, each stop's colour with `placeholder` for `phClr`.
fn gradient_fill(gradient: &GradientFill, colors: &ThemeColors, placeholder: Option<Rgba>) -> Fill {
    let stops = gradient
        .gradient_stop_list
        .iter()
        .flat_map(|list| &list.gradient_stops)
        .map(|stop| GradientStop {
            color: color(&stop.content, colors, placeholder),
            position: stop
                .pos
                .as_ref()
                .and_then(|pos| positive_fixed_percentage(&pos.0)),
        })
        .collect();
    Fill::Gradient {
        fill: gradient.clone(),
        stops,
    }
}

/// The 1-based `index` a style matrix reference states; `None` when it is absent or no value.
fn index(idx: Option<&StyleMatrixColumnIndex>) -> Option<usize> {
    usize::try_from(idx?.value().ok()?).ok()
}

/// The line of a shape: its own `a:ln` over the line style its `a:lnRef` names, one thing at a
/// time (sections 20.1.2.2.24 and 20.1.4.2.19); `None` when there is neither.
#[must_use = "this only returns the value"]
pub fn line<P, S>(properties: &P, style: Option<&S>, formats: Formats<'_>) -> Option<Line>
where
    P: ShapeFormat + ?Sized,
    S: StyleReferences + ?Sized,
{
    let own = properties.outline();
    let reference = style.and_then(StyleReferences::line_reference);
    let base = reference.and_then(|line_reference| {
        formats
            .scheme?
            .line_style_list
            .outline
            .get(index(line_reference.idx.as_ref())?.checked_sub(1)?)
    });
    if own.is_none() && base.is_none() {
        return None;
    }
    let placeholder = reference_color(
        reference.and_then(|found| found.content.as_deref()),
        formats,
    );
    let fill = own
        .and_then(|outline| outline.line_fill_content.as_deref())
        .map(|content| read_line_fill(content, formats.colors, None))
        .or_else(|| {
            base.and_then(|outline| outline.line_fill_content.as_deref())
                .map(|content| read_line_fill(content, formats.colors, placeholder))
        })
        .unwrap_or(Fill::None);
    Some(Line {
        alignment: stated(own, base, |outline| outline.algn.clone()),
        cap: stated(own, base, |outline| outline.cap.clone())
            .unwrap_or_else(|| EndLineCap::from(EndLineCapMember::Sq)),
        compound: stated(own, base, |outline| outline.cmpd.clone())
            .unwrap_or_else(|| CompoundLineType::from(CompoundLineTypeMember::Sng)),
        dash: stated(own, base, |outline| {
            outline.line_dash_content.as_deref().cloned()
        }),
        fill,
        head_end: stated(own, base, |outline| {
            outline.line_head_end_style.as_deref().cloned()
        }),
        join: stated(own, base, |outline| {
            outline.line_join_content.as_deref().cloned()
        }),
        tail_end: stated(own, base, |outline| {
            outline.tail_line_end_style.as_deref().cloned()
        }),
        width: stated(own, base, |outline| outline.w.as_ref()?.value().ok()).unwrap_or(0),
    })
}

/// The fill of an `a:pattFill`, its two colours with `placeholder` for `phClr`.
fn pattern_fill(pattern: &PatternFill, colors: &ThemeColors, placeholder: Option<Rgba>) -> Fill {
    Fill::Pattern {
        background: pattern
            .background_color
            .as_ref()
            .and_then(|background| color(&background.content, colors, placeholder)),
        fill: pattern.clone(),
        foreground: pattern
            .foreground_color
            .as_ref()
            .and_then(|foreground| color(&foreground.content, colors, placeholder)),
    }
}

/// The fill `content` states, with `placeholder` for `phClr`; a node the schemas do not declare
/// is no fill.
fn read_fill(
    content: &FillPropertiesContent,
    colors: &ThemeColors,
    placeholder: Option<Rgba>,
) -> Fill {
    match content {
        FillPropertiesContent::GradientFill(gradient) => {
            gradient_fill(gradient, colors, placeholder)
        }
        FillPropertiesContent::GroupFill(_) => Fill::Group,
        FillPropertiesContent::PatternFill(pattern) => pattern_fill(pattern, colors, placeholder),
        FillPropertiesContent::PictureFill(picture) => Fill::Picture(picture.clone()),
        FillPropertiesContent::SolidFill(solid) => solid_fill(solid, colors, placeholder),
        FillPropertiesContent::NoFill(_) | FillPropertiesContent::Other(_) | _ => Fill::None,
    }
}

/// The fill of a line `content` states, read as [`read_fill`] reads a shape's.
fn read_line_fill(
    content: &LineFillContent,
    colors: &ThemeColors,
    placeholder: Option<Rgba>,
) -> Fill {
    match content {
        LineFillContent::GradientFill(gradient) => gradient_fill(gradient, colors, placeholder),
        LineFillContent::PatternFill(pattern) => pattern_fill(pattern, colors, placeholder),
        LineFillContent::SolidFill(solid) => solid_fill(solid, colors, placeholder),
        LineFillContent::NoFill(_) | LineFillContent::Other(_) | _ => Fill::None,
    }
}

/// The colour a style matrix reference holds, which `phClr` stands for in the style it names.
fn reference_color(content: Option<&ColorContent>, formats: Formats<'_>) -> Option<Rgba> {
    color(content?, formats.colors, None)
}

/// The fill style `reference` names: 0 and 1000 none, 1 to 999 a fill style, 1001 and above a
/// background fill style (section 20.1.4.2.10). `None` too past the list's end or with no theme.
fn referenced_fill(reference: &FillReference, formats: Formats<'_>) -> Option<Fill> {
    let scheme = formats.scheme?;
    let style = match index(reference.idx.as_ref())? {
        0 | 1000 => return None,
        position @ 1..=999 => scheme.fill_style_list.content.get(position - 1),
        position => scheme
            .background_fill_style_list
            .content
            .get(position - 1001),
    }?;
    let placeholder = reference_color(reference.content.as_deref(), formats);
    Some(read_fill(style, formats.colors, placeholder))
}

/// The fill of an `a:solidFill`, with `placeholder` for `phClr`.
fn solid_fill(solid: &SolidFill, colors: &ThemeColors, placeholder: Option<Rgba>) -> Fill {
    Fill::Solid(
        solid
            .content
            .as_deref()
            .and_then(|content| color(content, colors, placeholder)),
    )
}

/// What `field` reads from the shape's own `a:ln`, else from the theme's line style.
fn stated<T>(
    own: Option<&Outline>,
    base: Option<&Outline>,
    field: impl Fn(&Outline) -> Option<T>,
) -> Option<T> {
    own.and_then(&field).or_else(|| base.and_then(field))
}
