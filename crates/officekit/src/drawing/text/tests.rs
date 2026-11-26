use officekit_markup::common_simple_types::{HexadecimalColorValue, Panose1Number};
use officekit_markup::drawing::{
    CharacterBullet, ClickHyperlink, ColorContent, ColorSpecified, DefaultParagraphStyle,
    DefaultTextRunProperties, EndParagraphRunProperties, FillPropertiesContent, LatinFont,
    ListLevel1TextStyle, ListLevel2TextStyle, ListLevel3TextStyle, ListLevel4TextStyle,
    ListLevel5TextStyle, ListLevel6TextStyle, ListLevel7TextStyle, ListLevel8TextStyle,
    ListLevel9TextStyle, NoBullet, RGBColorModelHexVariant, SolidFill, TextAlignmentTypes,
    TextAlignmentTypesMember, TextBulletColorContent, TextBulletContent, TextFontSize,
    TextIndentLevelType, TextIndentation, TextListStyles, TextMargin, TextParagraphProperties,
    TextRunProperties, write,
};
use officekit_markup::presentation::{
    NotesTextStyle, PresentationDefaultTextStyle, SlideMasterBodyTextStyle,
    SlideMasterOtherTextStyle, SlideMasterTitleTextStyle,
};
use officekit_xml::write::Scope;

use super::{ParagraphLevel as _, RunLevel, StyleList as _, TextLevels, paragraph_over, run_over};

/// A node of `$node`'s type stating `marL="457200"`, `algn="ctr"` and an `a:buChar`.
macro_rules! paragraph_node {
    ($node:ty) => {{
        let mut node = <$node>::default();
        node.mar_l = Some(margin("457200"));
        node.algn = Some(alignment(TextAlignmentTypesMember::Ctr));
        node.text_bullet_content = Some(Box::new(character_bullet()));
        node
    }};
}

/// A node of `$node`'s type stating `sz="1800"`, `b="1"` and an `a:solidFill`.
macro_rules! run_node {
    ($node:ty) => {{
        let mut node = <$node>::default();
        node.sz = Some(TextFontSize::from("1800".to_owned()));
        node.b = Some("1".to_owned());
        node.fill_properties_content = Some(Box::new(solid_fill("FF0000")));
        node
    }};
}

/// A list of `$list`'s type whose `a:lvl3pPr` states `marL="300"`.
macro_rules! list_with_level_3 {
    ($list:ty) => {{
        let mut level = ListLevel3TextStyle::default();
        level.mar_l = Some(margin("300"));
        let mut list = <$list>::default();
        list.list_level_3_text_style = Some(Box::new(level));
        list
    }};
}

fn alignment(member: TextAlignmentTypesMember) -> TextAlignmentTypes {
    TextAlignmentTypes::from(member)
}

fn character_bullet() -> TextBulletContent {
    let mut bullet = CharacterBullet::default();
    bullet.char = Some("*".to_owned());
    TextBulletContent::from(bullet)
}

fn hex(text: &str) -> ColorContent {
    let mut color = RGBColorModelHexVariant::default();
    color.val = Some(HexadecimalColorValue::from(text.to_owned()));
    ColorContent::from(color)
}

fn latin(typeface: &str) -> LatinFont {
    let mut font = LatinFont::default();
    font.typeface = Some(typeface.to_owned());
    font
}

fn margin(text: &str) -> TextMargin {
    TextMargin::from(text.to_owned())
}

fn solid_fill(text: &str) -> FillPropertiesContent {
    let mut fill = SolidFill::default();
    fill.content = Some(Box::new(hex(text)));
    FillPropertiesContent::from(fill)
}

/// Asserts that `levels` holds the `a:lvl3pPr` of [`list_with_level_3`] at index 2, and nothing at
/// index 1.
fn holds_level_3_at_index_2(levels: &TextLevels) {
    assert_eq!(levels.levels[2].mar_l, Some(margin("300")));
    assert_eq!(levels.levels[1], TextParagraphProperties::default());
}

/// An `a:pPr` holding only `run` as its `a:defRPr`.
fn with_run(run: DefaultTextRunProperties) -> TextParagraphProperties {
    let mut properties = TextParagraphProperties::default();
    properties.default_text_run_properties = Some(Box::new(run));
    properties
}

#[test]
fn a_body_style_level_over_a_default_text_style_level_holds_both() {
    let mut body_level = ListLevel1TextStyle::default();
    body_level.mar_l = Some(margin("342900"));
    let mut body = SlideMasterBodyTextStyle::default();
    body.list_level_1_text_style = Some(Box::new(body_level));

    let mut run = DefaultTextRunProperties::default();
    run.sz = Some(TextFontSize::from("1800".to_owned()));
    let mut default_level = ListLevel1TextStyle::default();
    default_level.default_text_run_properties = Some(Box::new(run));
    let mut defaults = PresentationDefaultTextStyle::default();
    defaults.list_level_1_text_style = Some(Box::new(default_level));

    let levels = TextLevels::of(&body).over(&TextLevels::of(&defaults));
    let first = &levels.levels[0];
    assert_eq!(
        first.mar_l.as_ref().map(|value| value.0.as_str()),
        Some("342900")
    );
    let size = first
        .default_text_run_properties
        .as_deref()
        .map(RunLevel::run_properties);
    assert_eq!(
        size.and_then(|found| found.sz).map(|value| value.0),
        Some("1800".to_owned())
    );
}

#[test]
fn a_level_takes_its_lists_default_and_nothing_of_another_level() {
    let mut default = DefaultParagraphStyle::default();
    default.algn = Some(alignment(TextAlignmentTypesMember::Ctr));
    let mut first = ListLevel1TextStyle::default();
    first.mar_l = Some(margin("100"));
    let mut second = ListLevel2TextStyle::default();
    second.indent = Some(TextIndentation::from("50".to_owned()));
    let mut list = TextListStyles::default();
    list.default_paragraph_style = Some(Box::new(default));
    list.list_level_1_text_style = Some(Box::new(first));
    list.list_level_2_text_style = Some(Box::new(second));

    let levels = TextLevels::of(&list);
    let mut expected = TextParagraphProperties::default();
    expected.algn = Some(alignment(TextAlignmentTypesMember::Ctr));
    expected.indent = Some(TextIndentation::from("50".to_owned()));
    assert_eq!(levels.levels[1], expected);
    assert_eq!(levels.levels[1].mar_l, None);
    assert_eq!(levels.levels[0].mar_l, Some(margin("100")));
}

#[test]
fn a_level_a_list_does_not_state_is_its_default() {
    let mut default = DefaultParagraphStyle::default();
    default.algn = Some(alignment(TextAlignmentTypesMember::R));
    let mut list = TextListStyles::default();
    list.default_paragraph_style = Some(Box::new(default));

    let levels = TextLevels::of(&list);
    for level in &levels.levels {
        assert_eq!(level.algn, Some(alignment(TextAlignmentTypesMember::R)));
    }
}

#[test]
fn the_nearer_level_wins_value_by_value() {
    let mut near = ListLevel1TextStyle::default();
    near.mar_l = Some(margin("100"));
    let mut nearer = TextListStyles::default();
    nearer.list_level_1_text_style = Some(Box::new(near));
    let mut far = ListLevel1TextStyle::default();
    far.mar_l = Some(margin("200"));
    far.indent = Some(TextIndentation::from("50".to_owned()));
    let mut farther = TextListStyles::default();
    farther.list_level_1_text_style = Some(Box::new(far));

    let levels = TextLevels::of(&nearer).over(&TextLevels::of(&farther));
    assert_eq!(levels.levels[0].mar_l, Some(margin("100")));
    assert_eq!(
        levels.levels[0].indent,
        Some(TextIndentation::from("50".to_owned()))
    );
}

#[test]
fn a_bullet_choice_replaces_another_and_the_colour_stays() {
    let mut near = TextParagraphProperties::default();
    near.text_bullet_content = Some(Box::new(TextBulletContent::from(NoBullet::default())));
    let mut far = TextParagraphProperties::default();
    far.text_bullet_content = Some(Box::new(character_bullet()));
    let mut color = ColorSpecified::default();
    color.content = Box::new(hex("00FF00"));
    far.text_bullet_color_content = Some(Box::new(TextBulletColorContent::from(color.clone())));

    let laid = paragraph_over(&near, &far);
    assert_eq!(
        laid.text_bullet_content.as_deref(),
        Some(&TextBulletContent::from(NoBullet::default()))
    );
    assert_eq!(
        laid.text_bullet_color_content.as_deref(),
        Some(&TextBulletColorContent::from(color))
    );
}

#[test]
fn the_run_properties_of_two_levels_are_laid_value_by_value() {
    let mut near = DefaultTextRunProperties::default();
    near.b = Some("1".to_owned());
    let mut far = DefaultTextRunProperties::default();
    far.sz = Some(TextFontSize::from("1800".to_owned()));
    far.fill_properties_content = Some(Box::new(solid_fill("0000FF")));

    let laid = paragraph_over(&with_run(near), &with_run(far));
    let mut expected = DefaultTextRunProperties::default();
    expected.b = Some("1".to_owned());
    expected.sz = Some(TextFontSize::from("1800".to_owned()));
    expected.fill_properties_content = Some(Box::new(solid_fill("0000FF")));
    assert_eq!(laid, with_run(expected));
}

#[test]
fn a_child_is_replaced_whole() {
    let mut near = DefaultTextRunProperties::default();
    near.latin_font = Some(Box::new(latin("Arial")));
    let mut calibri = latin("Calibri");
    calibri.panose = Some(Panose1Number::from("020F0502020204030204".to_owned()));
    let mut far = DefaultTextRunProperties::default();
    far.latin_font = Some(Box::new(calibri));

    let laid = paragraph_over(&with_run(near), &with_run(far));
    let font = laid
        .default_text_run_properties
        .and_then(|run| run.latin_font)
        .map(|font| *font);
    assert_eq!(font, Some(latin("Arial")));
}

#[test]
fn a_run_child_is_replaced_whole() {
    let mut near = TextRunProperties::default();
    near.latin_font = Some(Box::new(latin("Arial")));
    let mut calibri = latin("Calibri");
    calibri.panose = Some(Panose1Number::from("020F0502020204030204".to_owned()));
    let mut far = TextRunProperties::default();
    far.latin_font = Some(Box::new(calibri));
    far.b = Some("1".to_owned());

    let laid = run_over(&near, &far);
    assert_eq!(laid.latin_font.as_deref(), Some(&latin("Arial")));
    assert_eq!(laid.b.as_deref(), Some("1"));
}

#[test]
fn every_paragraph_type_gives_its_properties_as_an_a_ppr() {
    let expected = paragraph_node!(TextParagraphProperties);
    let found = [
        paragraph_node!(TextParagraphProperties).paragraph_properties(),
        paragraph_node!(DefaultParagraphStyle).paragraph_properties(),
        paragraph_node!(ListLevel1TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel2TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel3TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel4TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel5TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel6TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel7TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel8TextStyle).paragraph_properties(),
        paragraph_node!(ListLevel9TextStyle).paragraph_properties(),
    ];
    for (index, properties) in found.iter().enumerate() {
        assert_eq!(properties, &expected, "type {index}");
    }
}

#[test]
fn an_a_ppr_from_an_a_lvl1ppr_writes_as_an_a_ppr() {
    let properties = paragraph_node!(ListLevel1TextStyle).paragraph_properties();
    assert!(
        properties
            .unknown()
            .is_none_or(|unknown| unknown.name.is_none())
    );
    let element = write::text_paragraph_properties(&properties, &mut Scope::default());
    assert_eq!(element.name.local, "pPr");
    assert_eq!(element.name.prefix, "a");
}

#[test]
fn every_run_type_gives_its_properties_as_an_a_rpr() {
    let expected = run_node!(TextRunProperties);
    let found = [
        run_node!(TextRunProperties).run_properties(),
        run_node!(DefaultTextRunProperties).run_properties(),
        run_node!(EndParagraphRunProperties).run_properties(),
    ];
    for (index, properties) in found.iter().enumerate() {
        assert_eq!(properties, &expected, "type {index}");
    }
}

#[test]
fn an_a_rpr_from_an_a_endpararpr_writes_as_an_a_rpr() {
    let properties = run_node!(EndParagraphRunProperties).run_properties();
    assert!(
        properties
            .unknown()
            .is_none_or(|unknown| unknown.name.is_none())
    );
    let element = write::text_run_properties(&properties, &mut Scope::default());
    assert_eq!(element.name.local, "rPr");
}

#[test]
fn every_list_type_holds_its_level_3_at_index_2() {
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(TextListStyles)));
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(
        SlideMasterTitleTextStyle
    )));
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(
        SlideMasterBodyTextStyle
    )));
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(
        SlideMasterOtherTextStyle
    )));
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(
        PresentationDefaultTextStyle
    )));
    holds_level_3_at_index_2(&TextLevels::of(&list_with_level_3!(NotesTextStyle)));
}

#[test]
fn a_list_has_no_level_above_8() {
    let list = list_with_level_3!(TextListStyles);
    assert!(list.level(2).is_some());
    assert_eq!(list.level(9), None);
    assert_eq!(list.default_paragraph(), None);
}

#[test]
fn a_paragraph_takes_the_level_its_lvl_names_under_its_own_properties() {
    let mut second = ListLevel2TextStyle::default();
    second.mar_l = Some(margin("200"));
    second.algn = Some(alignment(TextAlignmentTypesMember::Ctr));
    let mut list = TextListStyles::default();
    list.list_level_2_text_style = Some(Box::new(second));
    let levels = TextLevels::of(&list);

    let mut own = TextParagraphProperties::default();
    own.lvl = Some(TextIndentLevelType::from("1".to_owned()));
    own.algn = Some(alignment(TextAlignmentTypesMember::R));
    let paragraph = levels.paragraph(Some(&own));
    assert_eq!(paragraph.algn, Some(alignment(TextAlignmentTypesMember::R)));
    assert_eq!(paragraph.mar_l, Some(margin("200")));
}

#[test]
fn a_paragraph_with_no_properties_of_its_own_takes_level_0() {
    let mut first = ListLevel1TextStyle::default();
    first.mar_l = Some(margin("100"));
    let mut list = TextListStyles::default();
    list.list_level_1_text_style = Some(Box::new(first));
    let levels = TextLevels::of(&list);

    assert_eq!(levels.paragraph(None), levels.levels[0]);
}

#[test]
fn a_level_out_of_range_or_no_number_takes_level_0() {
    let mut first = ListLevel1TextStyle::default();
    first.mar_l = Some(margin("100"));
    let mut list = TextListStyles::default();
    list.list_level_1_text_style = Some(Box::new(first));
    let levels = TextLevels::of(&list);

    for text in ["12", "two", "-1"] {
        let mut own = TextParagraphProperties::default();
        own.lvl = Some(TextIndentLevelType::from(text.to_owned()));
        let paragraph = levels.paragraph(Some(&own));
        assert_eq!(paragraph.mar_l, Some(margin("100")), "lvl {text}");
    }
}

#[test]
fn nothing_over_nothing_is_nothing() {
    let levels = TextLevels::default().over(&TextLevels::default());
    assert_eq!(levels, TextLevels::default());
    for level in &levels.levels {
        assert_eq!(level, &TextParagraphProperties::default());
    }
}

#[test]
fn a_value_over_the_default_and_the_default_over_a_value_are_the_value() {
    let mut hyperlink = ClickHyperlink::default();
    hyperlink.id = Some("rId3".to_owned());
    let mut run = DefaultTextRunProperties::default();
    run.latin_font = Some(Box::new(latin("Arial")));
    run.click_hyperlink = Some(Box::new(hyperlink));
    let mut value = with_run(run);
    value.mar_l = Some(margin("342900"));
    value.lvl = Some(TextIndentLevelType::from("2".to_owned()));
    value.text_bullet_content = Some(Box::new(character_bullet()));
    let default = TextParagraphProperties::default();

    assert_eq!(paragraph_over(&value, &default), value);
    assert_eq!(paragraph_over(&default, &value), value);
}

#[test]
fn a_run_over_the_default_is_the_run() {
    let mut hyperlink = ClickHyperlink::default();
    hyperlink.id = Some("rId7".to_owned());
    let mut run = run_node!(TextRunProperties);
    run.click_hyperlink = Some(Box::new(hyperlink));

    assert_eq!(run_over(&run, &TextRunProperties::default()), run);
    assert_eq!(run_over(&TextRunProperties::default(), &run), run);
}
