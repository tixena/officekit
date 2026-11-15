use alloc::borrow::ToOwned as _;
use alloc::boxed::Box;

use officekit_markup::common_simple_types::BooleanValue;
use officekit_markup::vml::{
    DocumentBackground, LineStrokeSettings, MultiplePathLine, MultiplePathLineContent, Oval,
    Rectangle, ShadowEffect, ShapeDefinition, ShapeDefinitionContent, ShapeElementContent,
    ShapeFillProperties, ShapeGroup, ShapeGroupContent,
};

use super::{Stated, Toggle, filled, miter_limit, opacity, stroked};

fn truth(text: &str) -> BooleanValue {
    BooleanValue::from(text.to_owned())
}

fn fill(on: Option<&str>) -> ShapeFillProperties {
    let mut fill = ShapeFillProperties::default();
    fill.on = on.map(truth);
    fill
}

fn stroke(on: Option<&str>) -> LineStrokeSettings {
    let mut stroke = LineStrokeSettings::default();
    stroke.on = on.map(truth);
    stroke
}

/// A `v:rect` with `filled` and, for each item of `fills`, a `v:fill` with that `on`.
fn rectangle(attribute: Option<&str>, fills: &[Option<&str>]) -> Rectangle {
    let mut rectangle = Rectangle::default();
    rectangle.filled = attribute.map(truth);
    rectangle.content = fills
        .iter()
        .map(|&on| ShapeElementContent::ShapeFillProperties(fill(on)))
        .collect();
    rectangle
}

/// A `v:shape` with `stroked` and, for each item of `strokes`, a `v:stroke` with that `on`.
fn shape(attribute: Option<&str>, strokes: &[Option<&str>]) -> ShapeDefinition {
    let mut shape = ShapeDefinition::default();
    shape.stroked = attribute.map(truth);
    shape.content = strokes
        .iter()
        .map(|&on| ShapeDefinitionContent::LineStrokeSettings(stroke(on)))
        .collect();
    shape
}

#[test]
fn each_form_of_an_opacity_states_the_same_fraction() {
    for text in [".5", "0.5", "50%", "32768f"] {
        assert_eq!(opacity(Some(text)), Stated::Value(50_000), "{text}");
    }
    for text in [".8", "80%", "52429f"] {
        assert_eq!(opacity(Some(text)), Stated::Value(80_000), "{text}");
    }
    for text in ["1", "1.0", "100%", "65536f"] {
        assert_eq!(opacity(Some(text)), Stated::Value(100_000), "{text}");
    }
    for text in ["0", "0%", "0f", ".0"] {
        assert_eq!(opacity(Some(text)), Stated::Value(0), "{text}");
    }
    assert_eq!(opacity(Some(".25")), Stated::Value(25_000));
    assert_eq!(opacity(Some("12.5%")), Stated::Value(12_500));
}

#[test]
fn an_opacity_is_read_to_the_nearest_thousandth_of_a_percent() {
    assert_eq!(opacity(Some(".333335")), Stated::Value(33_334));
    assert_eq!(opacity(Some(".333334999")), Stated::Value(33_333));
    assert_eq!(opacity(Some("33.3335%")), Stated::Value(33_334));
    assert_eq!(opacity(Some("1f")), Stated::Value(2));
}

#[test]
fn an_absent_opacity_is_opaque() {
    assert_eq!(opacity(None), Stated::Value(100_000));
}

#[test]
fn an_opacity_in_no_form_or_outside_zero_to_one_is_unstated() {
    for text in [
        "", "abc", ".", "%", "f", "-.5", "+.5", "1.5", "150%", "65537f", "1.5f", "5e-1", " .5",
        "50 %", "0x10",
    ] {
        assert_eq!(opacity(Some(text)), Stated::Unstated, "{text}");
    }
}

#[test]
fn the_opacity_of_a_fill_a_stroke_a_shadow_and_a_shape() {
    let mut fill = ShapeFillProperties::default();
    fill.opacity = Some(".5".to_owned());
    fill.opacity2 = Some("32768f".to_owned());
    assert_eq!(opacity(fill.opacity.as_deref()), Stated::Value(50_000));
    assert_eq!(opacity(fill.opacity2.as_deref()), Stated::Value(50_000));

    let mut line = LineStrokeSettings::default();
    line.opacity = Some("50%".to_owned());
    assert_eq!(opacity(line.opacity.as_deref()), Stated::Value(50_000));
    assert_eq!(
        opacity(LineStrokeSettings::default().opacity.as_deref()),
        Stated::Value(100_000)
    );

    let mut shadow = ShadowEffect::default();
    shadow.opacity = Some(".5".to_owned());
    assert_eq!(opacity(shadow.opacity.as_deref()), Stated::Value(50_000));

    let mut rectangle = Rectangle::default();
    rectangle.opacity = Some("52429f".to_owned());
    assert_eq!(opacity(rectangle.opacity.as_deref()), Stated::Value(80_000));
    let mut definition = ShapeDefinition::default();
    definition.opacity = Some(".25".to_owned());
    assert_eq!(
        opacity(definition.opacity.as_deref()),
        Stated::Value(25_000)
    );
    assert_eq!(
        opacity(Oval::default().opacity.as_deref()),
        Stated::Value(100_000)
    );
}

#[test]
fn a_miter_limit_is_the_stated_number_in_thousandths() {
    let mut line = LineStrokeSettings::default();
    line.miterlimit = Some("2".to_owned());
    assert_eq!(miter_limit(line.miterlimit.as_deref()), Stated::Value(2000));
    assert_eq!(miter_limit(Some("2.5")), Stated::Value(2500));
    assert_eq!(miter_limit(Some("+3")), Stated::Value(3000));
    assert_eq!(miter_limit(Some("-1")), Stated::Value(-1000));
    assert_eq!(miter_limit(Some(".0005")), Stated::Value(1));
    assert_eq!(miter_limit(Some("-.0005")), Stated::Value(-1));
    assert_eq!(miter_limit(Some("10.")), Stated::Value(10_000));
}

#[test]
fn an_absent_miter_limit_is_eight() {
    assert_eq!(miter_limit(None), Stated::Value(8000));
    assert_eq!(
        miter_limit(LineStrokeSettings::default().miterlimit.as_deref()),
        Stated::Value(8000)
    );
}

#[test]
fn a_miter_limit_that_is_no_decimal_is_unstated() {
    for text in ["", "abc", ".", "-", "1e3", "8pt", "--1", " 8"] {
        assert_eq!(miter_limit(Some(text)), Stated::Unstated, "{text}");
    }
}

#[test]
fn a_shape_with_no_fill_child_is_filled_as_its_attribute_states() {
    assert_eq!(filled(&rectangle(None, &[])), Toggle::Decided(true));
    assert_eq!(filled(&rectangle(Some("t"), &[])), Toggle::Decided(true));
    assert_eq!(filled(&rectangle(Some("f"), &[])), Toggle::Decided(false));
    assert_eq!(
        filled(&rectangle(Some("false"), &[])),
        Toggle::Decided(false)
    );
    assert_eq!(
        filled(&rectangle(Some("maybe"), &[])),
        Toggle::Decided(true)
    );
}

#[test]
fn a_fill_child_that_states_on_overrides_the_shape() {
    assert_eq!(
        filled(&rectangle(Some("f"), &[Some("t")])),
        Toggle::Decided(true)
    );
    assert_eq!(
        filled(&rectangle(Some("true"), &[Some("false")])),
        Toggle::Decided(false)
    );
    assert_eq!(
        filled(&rectangle(None, &[Some("f")])),
        Toggle::Decided(false)
    );
}

#[test]
fn a_fill_child_without_on_against_an_unfilled_shape_is_open() {
    assert_eq!(
        filled(&rectangle(Some("f"), &[None])),
        Toggle::Open {
            child: true,
            shape: false,
        }
    );
    assert_eq!(
        filled(&rectangle(Some("t"), &[None])),
        Toggle::Decided(true)
    );
    assert_eq!(filled(&rectangle(None, &[None])), Toggle::Decided(true));
}

#[test]
fn the_first_fill_child_is_the_one_read() {
    assert_eq!(
        filled(&rectangle(None, &[Some("f"), Some("t")])),
        Toggle::Decided(false)
    );
}

#[test]
fn a_shape_is_stroked_from_its_attribute_and_its_stroke_child() {
    assert_eq!(stroked(&shape(None, &[])), Toggle::Decided(true));
    assert_eq!(stroked(&shape(Some("f"), &[])), Toggle::Decided(false));
    assert_eq!(
        stroked(&shape(Some("true"), &[Some("false")])),
        Toggle::Decided(false)
    );
    assert_eq!(
        stroked(&shape(Some("false"), &[Some("t")])),
        Toggle::Decided(true)
    );
    assert_eq!(stroked(&shape(Some("t"), &[None])), Toggle::Decided(true));
    assert_eq!(
        stroked(&shape(Some("f"), &[None])),
        Toggle::Open {
            child: true,
            shape: false,
        }
    );
}

#[test]
fn a_stroke_child_of_a_rectangle_decides_its_stroke() {
    let mut rectangle = Rectangle::default();
    rectangle.stroked = Some(truth("t"));
    rectangle.content = vec![ShapeElementContent::LineStrokeSettings(stroke(Some("f")))];
    assert_eq!(stroked(&rectangle), Toggle::Decided(false));
    assert_eq!(filled(&rectangle), Toggle::Decided(true));
}

#[test]
fn a_polyline_a_group_and_a_background_read_their_fill_child() {
    let mut polyline = MultiplePathLine::default();
    polyline.filled = Some(truth("t"));
    polyline.content = vec![MultiplePathLineContent::ShapeFillProperties(fill(Some(
        "f",
    )))];
    assert_eq!(filled(&polyline), Toggle::Decided(false));

    let mut group = ShapeGroup::default();
    group.filled = Some(truth("f"));
    group.content = vec![ShapeGroupContent::ShapeFillProperties(fill(Some("t")))];
    assert_eq!(filled(&group), Toggle::Decided(true));

    let mut background = DocumentBackground::default();
    background.filled = Some(truth("f"));
    assert_eq!(filled(&background), Toggle::Decided(false));
    background.shape_fill_properties = Some(Box::new(fill(None)));
    assert_eq!(
        filled(&background),
        Toggle::Open {
            child: true,
            shape: false,
        }
    );
}
