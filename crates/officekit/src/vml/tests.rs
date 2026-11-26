use alloc::borrow::ToOwned as _;
use alloc::boxed::Box;

use officekit_markup::common_simple_types::BooleanValue;
use officekit_markup::vml::{
    DocumentBackground, LineStrokeSettings, MultiplePathLine, MultiplePathLineContent, Oval,
    Rectangle, ShadowEffect, ShapeDefinition, ShapeDefinitionContent, ShapeElementContent,
    ShapeFillProperties, ShapeGroup, ShapeGroupContent, ShapeTemplate,
};

use super::{Stated, Toggle, TypedShape, filled, miter_limit, opacity, shapetype, stroked};

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
        .map(|&on| ShapeElementContent::from(fill(on)))
        .collect();
    rectangle
}

/// A `v:shape` with `stroked` and, for each item of `strokes`, a `v:stroke` with that `on`.
fn shape(attribute: Option<&str>, strokes: &[Option<&str>]) -> ShapeDefinition {
    let mut shape = ShapeDefinition::default();
    shape.stroked = attribute.map(truth);
    shape.content = strokes
        .iter()
        .map(|&on| ShapeDefinitionContent::from(stroke(on)))
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
    rectangle.content = vec![ShapeElementContent::from(stroke(Some("f")))];
    assert_eq!(stroked(&rectangle), Toggle::Decided(false));
    assert_eq!(filled(&rectangle), Toggle::Decided(true));
}

#[test]
fn a_polyline_a_group_and_a_background_read_their_fill_child() {
    let mut polyline = MultiplePathLine::default();
    polyline.filled = Some(truth("t"));
    polyline.content = vec![MultiplePathLineContent::from(fill(Some("f")))];
    assert_eq!(filled(&polyline), Toggle::Decided(false));

    let mut group = ShapeGroup::default();
    group.filled = Some(truth("f"));
    group.content = vec![ShapeGroupContent::from(fill(Some("t")))];
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

/// A `v:shapetype` with `id`, `filled`, `stroked`, and the `v:fill` and `v:stroke` children given.
fn template(
    id: &str,
    toggles: [Option<&str>; 2],
    fill: Option<ShapeFillProperties>,
    stroke: Option<LineStrokeSettings>,
) -> ShapeTemplate {
    let mut template = ShapeTemplate::default();
    template.id = Some(id.to_owned());
    template.filled = toggles[0].map(truth);
    template.stroked = toggles[1].map(truth);
    template.content = fill
        .map(ShapeElementContent::from)
        .into_iter()
        .chain(stroke.map(ShapeElementContent::from))
        .collect();
    template
}

/// A `v:shape` of `type`, with `filled`, `stroked`, and the `v:fill` and `v:stroke` children given.
fn typed(
    r#type: Option<&str>,
    toggles: [Option<&str>; 2],
    fill: Option<ShapeFillProperties>,
    stroke: Option<LineStrokeSettings>,
) -> ShapeDefinition {
    let mut shape = ShapeDefinition::default();
    shape.r#type = r#type.map(ToOwned::to_owned);
    shape.filled = toggles[0].map(truth);
    shape.stroked = toggles[1].map(truth);
    shape.content = fill
        .map(ShapeDefinitionContent::from)
        .into_iter()
        .chain(stroke.map(ShapeDefinitionContent::from))
        .collect();
    shape
}

#[test]
fn a_type_names_the_shapetype_with_its_id() {
    let types = [
        template("mytype", [None; 2], None, None),
        template("_x0000_t202", [Some("f"), None], None, None),
        template("_x0000_t202", [Some("t"), None], None, None),
    ];
    let shape = typed(Some("#_x0000_t202"), [None; 2], None, None);
    assert_eq!(shapetype(&shape, &types), Some(&types[1]));
    let bare = typed(Some("mytype"), [None; 2], None, None);
    assert_eq!(shapetype(&bare, &types), Some(&types[0]));
    assert_eq!(TypedShape::new(&shape, &types).shapetype(), Some(&types[1]));
    assert_eq!(TypedShape::new(&shape, &types).shape(), &shape);
}

#[test]
fn a_type_that_names_no_shapetype_resolves_to_none() {
    let types = [template("_x0000_t75", [None; 2], None, None)];
    for named in [
        Some("#_x0000_t202"),
        Some("#"),
        Some(""),
        Some("##_x0000_t75"),
        None,
    ] {
        let shape = typed(named, [None; 2], None, None);
        assert_eq!(shapetype(&shape, &types), None, "{named:?}");
    }
    assert_eq!(
        shapetype(&typed(Some("#_x0000_t75"), [None; 2], None, None), &[]),
        None
    );
}

#[test]
fn a_shape_with_no_attribute_and_no_child_takes_the_shapetypes() {
    let types = [
        template("off", [Some("f"), Some("false")], None, None),
        template(
            "children",
            [None; 2],
            Some(fill(Some("f"))),
            Some(stroke(Some("f"))),
        ),
        template("open", [Some("f"); 2], Some(fill(None)), Some(stroke(None))),
    ];
    let off = typed(Some("#off"), [None; 2], None, None);
    assert_eq!(
        filled(&TypedShape::new(&off, &types)),
        Toggle::Decided(false)
    );
    assert_eq!(
        stroked(&TypedShape::new(&off, &types)),
        Toggle::Decided(false)
    );

    let children = typed(Some("#children"), [None; 2], None, None);
    assert_eq!(
        filled(&TypedShape::new(&children, &types)),
        Toggle::Decided(false)
    );
    assert_eq!(
        stroked(&TypedShape::new(&children, &types)),
        Toggle::Decided(false)
    );

    let open = typed(Some("#open"), [None; 2], None, None);
    let unstated_on = Toggle::Open {
        child: true,
        shape: false,
    };
    assert_eq!(filled(&TypedShape::new(&open, &types)), unstated_on);
    assert_eq!(stroked(&TypedShape::new(&open, &types)), unstated_on);
}

#[test]
fn the_shapes_own_attribute_or_child_wins_where_stated() {
    let types = [
        template("off", [Some("f"); 2], None, None),
        template(
            "children",
            [None; 2],
            Some(fill(Some("f"))),
            Some(stroke(Some("f"))),
        ),
    ];
    let attribute = typed(Some("#off"), [Some("t"); 2], None, None);
    assert_eq!(
        filled(&TypedShape::new(&attribute, &types)),
        Toggle::Decided(true)
    );
    assert_eq!(
        stroked(&TypedShape::new(&attribute, &types)),
        Toggle::Decided(true)
    );

    let child = typed(
        Some("#children"),
        [None; 2],
        Some(fill(Some("t"))),
        Some(stroke(Some("true"))),
    );
    assert_eq!(
        filled(&TypedShape::new(&child, &types)),
        Toggle::Decided(true)
    );
    assert_eq!(
        stroked(&TypedShape::new(&child, &types)),
        Toggle::Decided(true)
    );

    let own_on_against_attribute = typed(
        Some("#off"),
        [None; 2],
        Some(fill(Some("t"))),
        Some(stroke(Some("t"))),
    );
    assert_eq!(
        filled(&TypedShape::new(&own_on_against_attribute, &types)),
        Toggle::Decided(true)
    );
    assert_eq!(
        stroked(&TypedShape::new(&own_on_against_attribute, &types)),
        Toggle::Decided(true)
    );
}

#[test]
fn each_property_not_stated_on_the_shape_comes_from_the_shapetype() {
    let types = [template(
        "children",
        [Some("t"); 2],
        Some(fill(Some("f"))),
        Some(stroke(Some("f"))),
    )];
    let child_without_on = typed(
        Some("#children"),
        [None; 2],
        Some(fill(None)),
        Some(stroke(None)),
    );
    assert_eq!(
        filled(&TypedShape::new(&child_without_on, &types)),
        Toggle::Decided(false)
    );
    assert_eq!(
        stroked(&TypedShape::new(&child_without_on, &types)),
        Toggle::Decided(false)
    );

    let attribute_only = typed(Some("#children"), [Some("t"); 2], None, None);
    assert_eq!(
        filled(&TypedShape::new(&attribute_only, &types)),
        Toggle::Decided(false)
    );

    let no_value = typed(
        Some("#children"),
        [Some("maybe"); 2],
        Some(fill(Some("maybe"))),
        None,
    );
    assert_eq!(
        filled(&TypedShape::new(&no_value, &types)),
        Toggle::Decided(false)
    );
}

#[test]
fn a_fill_and_stroke_attribute_comes_from_the_shapetypes_child_where_the_shape_states_none() {
    let mut type_fill = fill(None);
    type_fill.opacity = Some(".5".to_owned());
    let mut type_stroke = stroke(None);
    type_stroke.opacity = Some("25%".to_owned());
    type_stroke.miterlimit = Some("2".to_owned());
    let types = [template("t", [None; 2], Some(type_fill), Some(type_stroke))];

    let bare = typed(Some("#t"), [None; 2], None, None);
    let bare_typed = TypedShape::new(&bare, &types);
    assert_eq!(
        opacity(bare_typed.fill_attribute(|fill| fill.opacity.as_deref())),
        Stated::Value(50_000)
    );
    assert_eq!(
        opacity(bare_typed.stroke_attribute(|stroke| stroke.opacity.as_deref())),
        Stated::Value(25_000)
    );
    assert_eq!(
        miter_limit(bare_typed.stroke_attribute(|stroke| stroke.miterlimit.as_deref())),
        Stated::Value(2000)
    );

    let silent = typed(Some("#t"), [None; 2], Some(fill(None)), Some(stroke(None)));
    let silent_typed = TypedShape::new(&silent, &types);
    assert_eq!(
        opacity(silent_typed.fill_attribute(|fill| fill.opacity.as_deref())),
        Stated::Value(50_000)
    );
    assert_eq!(
        miter_limit(silent_typed.stroke_attribute(|stroke| stroke.miterlimit.as_deref())),
        Stated::Value(2000)
    );

    let mut own_fill = fill(None);
    own_fill.opacity = Some("80%".to_owned());
    let mut own_stroke = stroke(None);
    own_stroke.miterlimit = Some("3".to_owned());
    let stated = typed(Some("#t"), [None; 2], Some(own_fill), Some(own_stroke));
    let stated_typed = TypedShape::new(&stated, &types);
    assert_eq!(
        opacity(stated_typed.fill_attribute(|fill| fill.opacity.as_deref())),
        Stated::Value(80_000)
    );
    assert_eq!(
        opacity(stated_typed.stroke_attribute(|stroke| stroke.opacity.as_deref())),
        Stated::Value(25_000)
    );
    assert_eq!(
        miter_limit(stated_typed.stroke_attribute(|stroke| stroke.miterlimit.as_deref())),
        Stated::Value(3000)
    );
}

#[test]
fn a_shape_with_no_type_or_an_undefined_one_reads_as_it_does_alone() {
    let mut type_fill = fill(Some("f"));
    type_fill.opacity = Some(".5".to_owned());
    let mut type_stroke = stroke(Some("f"));
    type_stroke.miterlimit = Some("2".to_owned());
    let types = [template(
        "_x0000_t75",
        [Some("f"); 2],
        Some(type_fill),
        Some(type_stroke),
    )];
    let ons = [None, Some("t"), Some("f"), Some("maybe")];
    for named in [None, Some("#_x0000_t202"), Some("#_x0000_t1")] {
        for attribute in ons {
            for child in [None, Some(None), Some(Some("t")), Some(Some("f"))] {
                let shape = typed(named, [attribute; 2], child.map(fill), child.map(stroke));
                let alone = TypedShape::new(&shape, &types);
                assert_eq!(alone.shapetype(), None);
                assert_eq!(
                    filled(&alone),
                    filled(&shape),
                    "{named:?} {attribute:?} {child:?}"
                );
                assert_eq!(
                    stroked(&alone),
                    stroked(&shape),
                    "{named:?} {attribute:?} {child:?}"
                );
                assert_eq!(
                    opacity(alone.fill_attribute(|fill| fill.opacity.as_deref())),
                    Stated::Value(100_000)
                );
                assert_eq!(
                    miter_limit(alone.stroke_attribute(|stroke| stroke.miterlimit.as_deref())),
                    Stated::Value(8000)
                );
            }
        }
    }
}
