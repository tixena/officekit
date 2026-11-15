use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;

use officekit_markup::common_simple_types::{MeasurementInTwentiethsOfAPoint, OnOffValue};
use officekit_markup::office_word_2010_wordml::ConflictInsTrackChange;
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, Bold, ColorValue, ConflictChangeContent, DecimalNumberValue,
    DeletedTableRow, InsertedTableRow, MeasurementInHalfPoints, MoveDestinationParagraph,
    MoveSourceParagraph, NonComplexScriptFontSize, NumberingDefinitionInstance,
    NumberingDefinitionInstanceReferenceNumId, NumberingDefinitionInstanceReferenceNumPr,
    NumberingDefinitions, NumberingLevelOverrideDefinition, NumberingLevelReference,
    ParagraphIndentation, ParagraphMarkRunPropertiesRevision, ParagraphProperties,
    ParagraphPropertiesPPrGeneral, ReferencedCharacterStyle, ReferencedParagraphStyle,
    RunContentColor, RunFonts, RunProperties, RunPropertiesContent,
    RunPropertiesForTheParagraphMark, SignedMeasurementInTwentiethsOfPoint,
    SpacingBetweenLinesAndAboveBelowParagraph, StyleDefinition, StyleDefinitions, StyleTypes,
    StyleTypesMember,
};
use officekit_xml::parse::parse;

use super::{StyleSheet, WORDPROCESSING};
use crate::docx::Docx;
use crate::docx::numbering::Numbering;
use crate::parts::kinds::WordprocessingKind;

macro_rules! toggle_arms {
    ($property:expr, $name:expr, $($variant:ident => $tag:literal),*) => {
        $(
            if let RunPropertiesContent::$variant(found) = $property
                && $name == $tag
            {
                return Some(onoff(found.val.as_ref()));
            }
        )*
    };
}

const NORMAL: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;

fn xml<T>(text: &str) -> T
where
    T: officekit_markup::Read,
{
    let document =
        parse(format!(r#"<w:root xmlns:w="{WORDPROCESSING}">{text}</w:root>"#).as_bytes()).unwrap();
    T::read(document.root.child_elements().next().unwrap())
}

fn sheet(body: &str) -> StyleSheet {
    StyleSheet::new(&xml::<StyleDefinitions>(&format!(
        "<w:styles>{body}</w:styles>"
    )))
}

fn onoff(value: Option<&OnOffValue>) -> bool {
    value.is_none_or(|found| found.value().unwrap_or(true))
}

fn toggle_of(run: &RunProperties, name: &str) -> Option<bool> {
    run.content.iter().find_map(|property| {
        toggle_arms!(
            property, name,
            Bold => "b",
            ComplexScriptBold => "bCs",
            DisplayAllCharactersAsCapitalLetters => "caps",
            Embossing => "emboss",
            Italics => "i",
            ComplexScriptItalics => "iCs",
            Imprinting => "imprint",
            DisplayCharacterOutline => "outline",
            Shadow => "shadow",
            SmallCaps => "smallCaps",
            SingleStrikethrough => "strike",
            HiddenText => "vanish"
        );
        None
    })
}

fn size(run: &RunProperties) -> Option<&str> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::NonComplexScriptFontSize(found) = property {
            found.val.as_ref().map(|value| value.0.as_str())
        } else {
            None
        }
    })
}

fn color(run: &RunProperties) -> Option<&str> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::RunContentColor(found) = property {
            found.val.as_ref().map(|value| value.0.as_str())
        } else {
            None
        }
    })
}

fn fonts(run: &RunProperties) -> Option<&RunFonts> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::RunFonts(found) = property {
            Some(found)
        } else {
            None
        }
    })
}

fn bold(run: &RunProperties) -> Option<bool> {
    toggle_of(run, "b")
}

fn justification(paragraph: &ParagraphPropertiesPPrGeneral) -> Option<&str> {
    paragraph
        .paragraph_alignment
        .as_ref()
        .and_then(|found| found.val.as_ref())
        .map(|value| value.0.as_str())
}

fn spacing(
    paragraph: &ParagraphPropertiesPPrGeneral,
) -> &SpacingBetweenLinesAndAboveBelowParagraph {
    paragraph
        .spacing_between_lines_and_above_below_paragraph
        .as_deref()
        .unwrap()
}

fn stop_positions(paragraph: &ParagraphPropertiesPPrGeneral) -> Vec<&str> {
    paragraph
        .set_of_custom_tab_stops
        .iter()
        .flat_map(|tabs| &tabs.custom_tab_stop)
        .filter_map(|stop| stop.pos.as_ref().map(|value| value.0.as_str()))
        .collect()
}

fn own_run(body: &str) -> RunProperties {
    xml(&format!("<w:rPr>{body}</w:rPr>"))
}

fn own_paragraph(body: &str) -> ParagraphProperties {
    xml(&format!("<w:pPr>{body}</w:pPr>"))
}

#[test]
fn defaults_reach_a_paragraph_that_names_no_style() {
    let styles = sheet(&format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val="24"/></w:rPr></w:rPrDefault></w:docDefaults>{NORMAL}"#
    ));
    let paragraph = styles.paragraph(None);
    assert_eq!(paragraph.style.as_deref(), Some("Normal"));
    assert_eq!(size(&paragraph.run_properties), Some("24"));
}

#[test]
fn nearest_style_of_a_chain_of_three_wins() {
    let styles = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="BodyText"><w:basedOn w:val="Normal"/></w:style>
        <w:style w:type="paragraph" w:styleId="First"><w:basedOn w:val="BodyText"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="First"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    assert_eq!(size(&paragraph.run_properties), Some("20"));
    let ancestors = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="BodyText"><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="First"><w:basedOn w:val="BodyText"/></w:style>"#
    ));
    let inherited = ancestors.paragraph(Some(&named));
    assert_eq!(size(&inherited.run_properties), Some("24"));
}

#[test]
fn a_property_one_level_omits_keeps_the_earlier_value() {
    let styles = sheet(&format!(
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="200"/></w:pPr></w:pPrDefault></w:docDefaults>{NORMAL}
        <w:style w:type="paragraph" w:styleId="Spaced"><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="360"/></w:pPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Spaced"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    assert_eq!(
        spacing(&paragraph.properties)
            .before
            .as_ref()
            .map(|value| value.0.as_str()),
        Some("360")
    );
    assert_eq!(
        spacing(&paragraph.properties)
            .after
            .as_ref()
            .map(|value| value.0.as_str()),
        Some("200")
    );
}

#[test]
fn a_property_at_one_level_only_is_kept() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Centered"><w:pPr><w:jc w:val="center"/></w:pPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Centered"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    assert_eq!(justification(&paragraph.properties), Some("center"));
    assert_eq!(paragraph.style.as_deref(), Some("Centered"));
}

#[test]
fn fonts_merge_attribute_by_attribute() {
    let styles = sheet(&format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/></w:rPr></w:rPrDefault></w:docDefaults>{NORMAL}
        <w:style w:type="paragraph" w:styleId="Arial"><w:rPr><w:rFonts w:cs="Arial"/></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Arial"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    assert_eq!(
        fonts(&paragraph.run_properties)
            .and_then(|found| found.ascii_theme.as_ref())
            .map(|value| value.0.as_str()),
        Some("minorHAnsi")
    );
    assert_eq!(
        fonts(&paragraph.run_properties).and_then(|found| found.cs.as_deref()),
        Some("Arial")
    );
}

#[test]
fn direct_formatting_applies_last() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Big"><w:rPr><w:sz w:val="40"/></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Big"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:sz w:val="18"/>"#)));
    assert_eq!(size(&run), Some("18"));
    let plain = styles.run(&paragraph, None);
    assert_eq!(size(&plain), Some("40"));
}

#[test]
fn character_style_applies_over_the_paragraph_style() {
    let styles = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="Red"><w:rPr><w:color w:val="FF0000"/></w:rPr></w:style>
        <w:style w:type="character" w:styleId="Green"><w:rPr><w:color w:val="00FF00"/></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Red"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:rStyle w:val="Green"/>"#)));
    assert_eq!(color(&run), Some("00FF00"));
}

fn bold_styles(defaults: &str, paragraph: &str, character: &str) -> StyleSheet {
    sheet(&format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr>{defaults}</w:rPr></w:rPrDefault></w:docDefaults>{NORMAL}
        <w:style w:type="paragraph" w:styleId="P"><w:rPr>{paragraph}</w:rPr></w:style>
        <w:style w:type="character" w:styleId="C"><w:rPr>{character}</w:rPr></w:style>"#
    ))
}

fn bold_of(styles: &StyleSheet, own: &str) -> Option<bool> {
    let named = own_paragraph(r#"<w:pStyle w:val="P"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(
        &paragraph,
        Some(&own_run(&format!(r#"<w:rStyle w:val="C"/>{own}"#))),
    );
    bold(&run)
}

#[test]
fn toggle_true_at_both_levels_is_off() {
    let styles = bold_styles("", "<w:b/>", "<w:b/>");
    assert_eq!(bold_of(&styles, ""), Some(false));
}

#[test]
fn toggle_direct_value_wins() {
    let styles = bold_styles("", "<w:b/>", "<w:b/>");
    assert_eq!(bold_of(&styles, r#"<w:b w:val="0"/>"#), Some(false));
    let off = bold_styles("", "", "");
    assert_eq!(bold_of(&off, "<w:b/>"), Some(true));
}

#[test]
fn toggle_true_in_the_defaults_is_on() {
    let styles = bold_styles("<w:b/>", r#"<w:b w:val="0"/>"#, "");
    assert_eq!(bold_of(&styles, ""), Some(true));
}

#[test]
fn toggle_at_one_level_only_is_that_levels_value() {
    let styles = bold_styles("", "<w:b/>", "");
    assert_eq!(bold_of(&styles, ""), Some(true));
    let character = bold_styles("", "", "<w:b/>");
    assert_eq!(bold_of(&character, ""), Some(true));
    let none = bold_styles("", "", "");
    assert_eq!(bold_of(&none, ""), None);
}

#[test]
fn toggle_level_value_is_the_first_one_up_the_chain() {
    let styles = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="Parent"><w:basedOn w:val="Normal"/><w:rPr><w:b/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="Child"><w:basedOn w:val="Parent"/></w:style>
        <w:style w:type="paragraph" w:styleId="Off"><w:basedOn w:val="Parent"/><w:rPr><w:b w:val="0"/></w:rPr></w:style>"#
    ));
    let child = styles.paragraph(Some(&own_paragraph(r#"<w:pStyle w:val="Child"/>"#)));
    assert_eq!(bold(&styles.run(&child, None)), Some(true));
    let off = styles.paragraph(Some(&own_paragraph(r#"<w:pStyle w:val="Off"/>"#)));
    assert_eq!(bold(&styles.run(&off, None)), Some(false));
}

#[test]
fn every_toggle_property_combines_like_bold() {
    let styles = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="P"><w:rPr><w:bCs/><w:caps/><w:emboss/><w:i/><w:iCs/><w:imprint/><w:outline/><w:shadow/><w:smallCaps/><w:strike/><w:vanish/></w:rPr></w:style>
        <w:style w:type="character" w:styleId="C"><w:rPr><w:caps/><w:i/><w:vanish/></w:rPr></w:style>"#
    ));
    let paragraph = styles.paragraph(Some(&own_paragraph(r#"<w:pStyle w:val="P"/>"#)));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:rStyle w:val="C"/>"#)));
    let found: Vec<(&str, Option<bool>)> = super::TOGGLES
        .iter()
        .map(|name| (*name, toggle_of(&run, name)))
        .collect();
    let expected = [
        ("b", None),
        ("bCs", Some(true)),
        ("caps", Some(false)),
        ("emboss", Some(true)),
        ("i", Some(false)),
        ("iCs", Some(true)),
        ("imprint", Some(true)),
        ("outline", Some(true)),
        ("shadow", Some(true)),
        ("smallCaps", Some(true)),
        ("strike", Some(true)),
        ("vanish", Some(false)),
    ];
    assert_eq!(found, expected);
}

#[test]
fn tab_stops_are_added_and_cleared() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Tabbed"><w:pPr><w:tabs><w:tab w:val="left" w:pos="1440"/><w:tab w:val="left" w:pos="2880"/></w:tabs></w:pPr></w:style>"#
    ));
    let own = own_paragraph(
        r#"<w:pStyle w:val="Tabbed"/><w:tabs><w:tab w:val="left" w:pos="4320"/><w:tab w:val="clear" w:pos="1440"/></w:tabs>"#,
    );
    let paragraph = styles.paragraph(Some(&own));
    let positions = stop_positions(&paragraph.properties);
    assert_eq!(positions, ["2880", "4320"]);
}

#[test]
fn a_clear_with_no_earlier_stop_adds_nothing() {
    let styles = sheet(NORMAL);
    let own = own_paragraph(r#"<w:tabs><w:tab w:val="clear" w:pos="1440"/></w:tabs>"#);
    let paragraph = styles.paragraph(Some(&own));
    assert!(paragraph.properties.set_of_custom_tab_stops.is_none());
}

#[test]
fn a_missing_parent_stops_the_chain() {
    let styles = sheet(
        r#"<w:style w:type="paragraph" w:styleId="Orphan"><w:basedOn w:val="Gone"/><w:rPr><w:sz w:val="30"/></w:rPr></w:style>"#,
    );
    let ids: Vec<Option<&str>> = styles
        .chain(Some("Orphan"))
        .iter()
        .map(|style| style.style_id.as_deref())
        .collect();
    assert_eq!(ids, [Some("Orphan")]);
    let paragraph = styles.paragraph(Some(&own_paragraph(r#"<w:pStyle w:val="Orphan"/>"#)));
    assert_eq!(size(&paragraph.run_properties), Some("30"));
    assert_eq!(styles.chain(Some("Gone")).len(), 0);
}

#[test]
fn a_loop_applies_each_style_once() {
    let styles = sheet(
        r#"<w:style w:type="paragraph" w:styleId="A"><w:basedOn w:val="B"/><w:rPr><w:b/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="B"><w:basedOn w:val="A"/><w:rPr><w:b/></w:rPr></w:style>"#,
    );
    let ids: Vec<Option<&str>> = styles
        .chain(Some("A"))
        .iter()
        .map(|style| style.style_id.as_deref())
        .collect();
    assert_eq!(ids, [Some("A"), Some("B")]);
    let paragraph = styles.paragraph(Some(&own_paragraph(r#"<w:pStyle w:val="A"/>"#)));
    assert_eq!(bold(&paragraph.run_properties), Some(true));
}

#[test]
fn a_file_with_no_styles_part_gives_an_empty_sheet() {
    let docx = Docx::new(WordprocessingKind::Document).unwrap();
    let styles = docx.style_sheet().unwrap();
    assert_eq!(styles.default_paragraph(), None);
    let paragraph = styles.paragraph(None);
    assert_eq!(paragraph.style, None);
    assert_eq!(
        paragraph.properties,
        ParagraphPropertiesPPrGeneral::default()
    );
    assert_eq!(paragraph.run_properties, RunProperties::default());
}

#[test]
fn theme_fonts_stay_references() {
    let styles = sheet(&format!(
        r#"<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme="minorHAnsi"/></w:rPr></w:rPrDefault></w:docDefaults>{NORMAL}"#
    ));
    let paragraph = styles.paragraph(None);
    let run = styles.run(&paragraph, None);
    assert_eq!(
        fonts(&run)
            .and_then(|found| found.ascii_theme.as_ref())
            .map(|value| value.0.as_str()),
        Some("minorHAnsi")
    );
}

#[test]
fn a_style_keep_properties_follow_the_default_spacing_in_their_typed_fields() {
    let styles = sheet(&format!(
        r#"<w:docDefaults><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults>{NORMAL}
        <w:style w:type="paragraph" w:styleId="Heading1"><w:basedOn w:val="Normal"/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240"/><w:outlineLvl w:val="0"/></w:pPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Heading1"/>"#);
    let properties = styles.paragraph(Some(&named)).properties;
    assert!(properties.keep_paragraph_with_next_paragraph.is_some());
    assert!(properties.keep_all_lines_on_one_page.is_some());
    assert!(properties.associated_outline_level.is_some());
    assert_eq!(
        spacing(&properties)
            .before
            .as_ref()
            .map(|value| value.0.as_str()),
        Some("240")
    );
    assert_eq!(
        spacing(&properties)
            .after
            .as_ref()
            .map(|value| value.0.as_str()),
        Some("160")
    );
    assert_eq!(properties.unknown.children.len(), 0);
}

#[test]
fn the_own_style_reference_follows_a_style_alignment_in_its_typed_field() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Centered"><w:pPr><w:jc w:val="center"/></w:pPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Centered"/>"#);
    let properties = styles.paragraph(Some(&named)).properties;
    assert_eq!(
        properties
            .referenced_paragraph_style
            .as_ref()
            .and_then(|style| style.val.as_deref()),
        Some("Centered")
    );
    assert_eq!(justification(&properties), Some("center"));
    assert_eq!(properties.unknown.children.len(), 0);
}

#[test]
fn borders_of_two_levels_keep_their_typed_fields() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Boxed"><w:pPr><w:pBdr><w:left w:val="single" w:sz="4"/></w:pBdr></w:pPr></w:style>"#
    ));
    let own = own_paragraph(
        r#"<w:pStyle w:val="Boxed"/><w:pBdr><w:top w:val="single" w:sz="8"/></w:pBdr>"#,
    );
    let properties = styles.paragraph(Some(&own)).properties;
    let borders = properties.paragraph_borders.unwrap();
    assert!(borders.left_paragraph_border.is_some());
    assert!(
        borders
            .paragraph_border_above_identical_paragraphs
            .is_some()
    );
    assert_eq!(borders.unknown.children.len(), 0);
}

#[test]
fn numbering_of_two_levels_keeps_its_typed_fields() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Listed"><w:pPr><w:numPr><w:numId w:val="3"/></w:numPr></w:pPr></w:style>"#
    ));
    let own = own_paragraph(r#"<w:pStyle w:val="Listed"/><w:numPr><w:ilvl w:val="1"/></w:numPr>"#);
    let properties = styles.paragraph(Some(&own)).properties;
    let numbering = properties
        .numbering_definition_instance_reference_num_pr
        .unwrap();
    assert!(numbering.numbering_level_reference.is_some());
    assert!(
        numbering
            .numbering_definition_instance_reference_num_id
            .is_some()
    );
    assert_eq!(numbering.unknown.children.len(), 0);
}

#[test]
fn a_run_property_added_after_a_style_revision_keeps_its_typed_field() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Tracked"><w:rPr><w:sz w:val="20"/><w:rPrChange w:id="1" w:author="a"><w:rPr/></w:rPrChange></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Tracked"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:color w:val="FF0000"/>"#)));
    assert_eq!(size(&run), Some("20"));
    assert_eq!(color(&run), Some("FF0000"));
    assert_eq!(run.unknown.children.len(), 0);
}

#[test]
fn a_toggle_pushed_after_a_style_revision_keeps_its_typed_field() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Tracked"><w:rPr><w:sz w:val="20"/><w:rPrChange w:id="1" w:author="a"><w:rPr/></w:rPrChange></w:rPr></w:style>
        <w:style w:type="character" w:styleId="Strong"><w:rPr><w:b/></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Tracked"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:rStyle w:val="Strong"/>"#)));
    assert_eq!(bold(&run), Some(true));
    assert_eq!(run.unknown.children.len(), 0);
}

fn font_size(half_points: &str) -> RunPropertiesContent {
    let mut size = NonComplexScriptFontSize::default();
    size.val = Some(MeasurementInHalfPoints::from(half_points.to_owned()));
    RunPropertiesContent::NonComplexScriptFontSize(size)
}

fn character_reference(id: &str) -> RunPropertiesContent {
    let mut reference = ReferencedCharacterStyle::default();
    reference.val = Some(id.to_owned());
    RunPropertiesContent::ReferencedCharacterStyle(reference)
}

fn style_of(
    id: &str,
    kind: StyleTypesMember,
    content: Vec<RunPropertiesContent>,
) -> StyleDefinition {
    let mut run = RunProperties::default();
    run.content = content;
    let mut style = StyleDefinition::default();
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(kind));
    style.run_properties = Some(Box::new(run));
    style
}

/// A sheet whose default paragraph style, `Normal`, holds `normal`, beside `others`.
fn typed_sheet(normal: Vec<RunPropertiesContent>, others: Vec<StyleDefinition>) -> StyleSheet {
    let mut default = style_of("Normal", StyleTypesMember::Paragraph, normal);
    default.default = Some(true.into());
    let mut styles = StyleDefinitions::default();
    styles.style_definition.push(default);
    styles.style_definition.extend(others);
    StyleSheet::new(&styles)
}

fn mark_of(content: Vec<RunPropertiesContent>) -> RunPropertiesForTheParagraphMark {
    let mut mark = RunPropertiesForTheParagraphMark::default();
    mark.run_properties_content = content;
    mark
}

/// The effective properties of the mark of an empty paragraph whose own properties hold only
/// `mark`.
fn effective_mark(styles: &StyleSheet, mark: RunPropertiesForTheParagraphMark) -> RunProperties {
    let mut own = ParagraphProperties::default();
    own.run_properties_for_the_paragraph_mark = Some(Box::new(mark));
    let paragraph = styles.paragraph(Some(&own));
    styles.paragraph_mark(
        &paragraph,
        own.run_properties_for_the_paragraph_mark.as_deref(),
    )
}

#[test]
fn the_mark_size_wins_over_the_paragraph_style() {
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let effective = effective_mark(&styles, mark_of(vec![font_size("48")]));
    assert_eq!(size(&effective), Some("48"));
}

#[test]
fn a_paragraph_with_no_mark_gives_its_runs_properties() {
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let paragraph = styles.paragraph(Some(&ParagraphProperties::default()));
    let effective = styles.paragraph_mark(&paragraph, None);
    assert_eq!(size(&effective), Some("22"));
    assert_eq!(effective, styles.run(&paragraph, None));
}

#[test]
fn the_mark_character_style_gives_its_size() {
    let big = style_of("Big", StyleTypesMember::Character, vec![font_size("40")]);
    let styles = typed_sheet(vec![font_size("22")], vec![big]);
    let effective = effective_mark(&styles, mark_of(vec![character_reference("Big")]));
    assert_eq!(size(&effective), Some("40"));
}

#[test]
fn the_mark_own_size_wins_over_its_character_style() {
    let big = style_of("Big", StyleTypesMember::Character, vec![font_size("40")]);
    let styles = typed_sheet(vec![font_size("22")], vec![big]);
    let effective = effective_mark(
        &styles,
        mark_of(vec![character_reference("Big"), font_size("48")]),
    );
    assert_eq!(size(&effective), Some("48"));
}

#[test]
fn a_mark_toggle_combines_like_a_run_toggle() {
    let strong = style_of(
        "Strong",
        StyleTypesMember::Character,
        vec![RunPropertiesContent::Bold(Bold::default())],
    );
    let styles = typed_sheet(
        vec![RunPropertiesContent::Bold(Bold::default())],
        vec![strong],
    );
    let effective = effective_mark(&styles, mark_of(vec![character_reference("Strong")]));
    assert_eq!(bold(&effective), Some(false));

    let mut own_run = RunProperties::default();
    own_run.content.push(character_reference("Strong"));
    let paragraph = styles.paragraph(Some(&ParagraphProperties::default()));
    assert_eq!(bold(&styles.run(&paragraph, Some(&own_run))), Some(false));
}

#[test]
fn the_mark_revision_marks_are_left_out() {
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let mut tracked = mark_of(vec![font_size("48")]);
    tracked.inserted_table_row = Some(Box::new(InsertedTableRow::default()));
    tracked.deleted_table_row = Some(Box::new(DeletedTableRow::default()));
    tracked.move_source_paragraph = Some(Box::new(MoveSourceParagraph::default()));
    tracked.move_destination_paragraph = Some(Box::new(MoveDestinationParagraph::default()));
    tracked.conflict_change_content = Some(Box::new(
        ConflictChangeContent::ConflictInsTrackChange(ConflictInsTrackChange::default()),
    ));
    tracked.run_properties_revision = Some(Box::new(ParagraphMarkRunPropertiesRevision::default()));

    let effective = effective_mark(&styles, tracked);
    assert_eq!(size(&effective), Some("48"));
    assert_eq!(effective.unknown.children.len(), 0);
    assert_eq!(effective.revision_information, None);
    assert_eq!(
        effective,
        effective_mark(&styles, mark_of(vec![font_size("48")]))
    );
}

fn decimal_number(value: i64) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_string())
}

fn indentation(left: &str, hanging: Option<&str>) -> ParagraphIndentation {
    let mut found = ParagraphIndentation::default();
    found.left = Some(SignedMeasurementInTwentiethsOfPoint::from(left.to_owned()));
    found.hanging = hanging.map(|value| MeasurementInTwentiethsOfAPoint::from(value.to_owned()));
    found
}

fn numbered(num_id: i64, ilvl: Option<i64>) -> NumberingDefinitionInstanceReferenceNumPr {
    let mut reference = NumberingDefinitionInstanceReferenceNumId::default();
    reference.val = Some(decimal_number(num_id));
    let mut found = NumberingDefinitionInstanceReferenceNumPr::default();
    found.numbering_definition_instance_reference_num_id = Some(Box::new(reference));
    found.numbering_level_reference = ilvl.map(|value| {
        let mut level = NumberingLevelReference::default();
        level.val = Some(decimal_number(value));
        Box::new(level)
    });
    found
}

/// A paragraph style `id` whose paragraph properties are `general`.
fn paragraph_style(id: &str, general: ParagraphPropertiesPPrGeneral) -> StyleDefinition {
    let mut style = StyleDefinition::default();
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(StyleTypesMember::Paragraph));
    style.paragraph_properties_p_pr_general = Some(Box::new(general));
    style
}

fn sheet_of(styles: Vec<StyleDefinition>) -> StyleSheet {
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = styles;
    StyleSheet::new(&definitions)
}

/// The instance 5 of the abstract definition 4, whose level 0 has the indentation left 720 and
/// hanging 360, a `w:numPr` naming the instance 9, and the run properties `run`.
fn list_of(styles: &StyleSheet, run: Vec<RunPropertiesContent>) -> Numbering {
    let mut general = ParagraphPropertiesPPrGeneral::default();
    general.paragraph_indentation = Some(Box::new(indentation("720", Some("360"))));
    general.numbering_definition_instance_reference_num_pr = Some(Box::new(numbered(9, None)));
    let mut label = RunProperties::default();
    label.content = run;
    let mut level = NumberingLevelOverrideDefinition::default();
    level.ilvl = Some(decimal_number(0));
    level.paragraph_properties_p_pr_general = Some(Box::new(general));
    level.run_properties = Some(Box::new(label));
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(decimal_number(4));
    definition.numbering_level_override_definition.push(level);
    let mut instance = NumberingDefinitionInstance::default();
    instance.num_id = Some(decimal_number(5));
    instance.abstract_numbering_definition_reference.val = Some(decimal_number(4));
    let mut part = NumberingDefinitions::default();
    part.abstract_numbering_definition.push(definition);
    part.numbering_definition_instance.push(instance);
    Numbering::new(&part, styles)
}

/// A paragraph's own properties naming the style `style`, with `numbering` and `indent`.
fn own_of(
    style: &str,
    numbering: Option<NumberingDefinitionInstanceReferenceNumPr>,
    indent: Option<ParagraphIndentation>,
) -> ParagraphProperties {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(style.to_owned());
    let mut own = ParagraphProperties::default();
    own.referenced_paragraph_style = Some(Box::new(reference));
    own.numbering_definition_instance_reference_num_pr = numbering.map(Box::new);
    own.paragraph_indentation = indent.map(Box::new);
    own
}

/// The left and hanging indentation of `paragraph`.
fn indents(paragraph: &ParagraphPropertiesPPrGeneral) -> (Option<&str>, Option<&str>) {
    let found = paragraph.paragraph_indentation.as_deref();
    (
        found
            .and_then(|indent| indent.left.as_ref())
            .map(|value| value.0.as_str()),
        found
            .and_then(|indent| indent.hanging.as_ref())
            .map(|value| value.0.as_str()),
    )
}

#[test]
fn a_paragraph_with_no_num_pr_or_num_id_zero_is_no_list_item() {
    let mut listed = ParagraphPropertiesPPrGeneral::default();
    listed.numbering_definition_instance_reference_num_pr = Some(Box::new(numbered(5, None)));
    let styles = sheet_of(vec![paragraph_style("Listed", listed)]);
    let numbering = list_of(&styles, vec![]);

    let (plain, none) = styles.numbered_paragraph(&numbering, None);
    assert_eq!(none, None);
    assert_eq!(plain, styles.paragraph(None));

    let removed = own_of("Listed", Some(numbered(0, None)), None);
    let (paragraph, level) = styles.numbered_paragraph(&numbering, Some(&removed));
    assert_eq!(level, None);
    assert_eq!(paragraph, styles.paragraph(Some(&removed)));
}

#[test]
fn the_level_s_indents_go_over_the_style_s_and_under_the_paragraph_s_own() {
    let mut indented = ParagraphPropertiesPPrGeneral::default();
    indented.paragraph_indentation = Some(Box::new(indentation("100", None)));
    let styles = sheet_of(vec![paragraph_style("Indented", indented)]);
    let numbering = list_of(&styles, vec![]);

    let own = own_of("Indented", Some(numbered(5, Some(0))), None);
    let (paragraph, level) = styles.numbered_paragraph(&numbering, Some(&own));
    assert_eq!(level.map(|found| found.ilvl), Some(0));
    assert_eq!(indents(&paragraph.properties), (Some("720"), Some("360")));
    let num_id = paragraph
        .properties
        .numbering_definition_instance_reference_num_pr
        .as_deref()
        .and_then(|found| {
            found
                .numbering_definition_instance_reference_num_id
                .as_deref()
        })
        .and_then(|found| found.val.as_ref())
        .map(|value| value.0.as_str());
    assert_eq!(num_id, Some("5"));

    let wider = own_of(
        "Indented",
        Some(numbered(5, Some(0))),
        Some(indentation("1440", None)),
    );
    let (own_indent, _) = styles.numbered_paragraph(&numbering, Some(&wider));
    assert_eq!(indents(&own_indent.properties), (Some("1440"), Some("360")));
}

#[test]
fn numbering_from_the_style_goes_under_the_style_s_indents() {
    let mut listed = ParagraphPropertiesPPrGeneral::default();
    listed.numbering_definition_instance_reference_num_pr = Some(Box::new(numbered(5, None)));
    listed.paragraph_indentation = Some(Box::new(indentation("100", None)));
    let styles = sheet_of(vec![paragraph_style("Listed", listed)]);
    let numbering = list_of(&styles, vec![]);

    let own = own_of("Listed", None, None);
    let (paragraph, level) = styles.numbered_paragraph(&numbering, Some(&own));
    assert_eq!(level.map(|found| found.ilvl), Some(0));
    assert_eq!(indents(&paragraph.properties), (Some("100"), Some("360")));
}

#[test]
fn the_label_takes_the_mark_s_properties_under_the_level_s() {
    let mut red = RunContentColor::default();
    red.val = Some(ColorValue::from("FF0000".to_owned()));
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let numbering = list_of(
        &styles,
        vec![
            RunPropertiesContent::Bold(Bold::default()),
            RunPropertiesContent::RunContentColor(red),
        ],
    );
    let mut off = Bold::default();
    off.val = Some(false.into());
    let mark = mark_of(vec![RunPropertiesContent::Bold(off), font_size("48")]);
    let own = own_of("Normal", Some(numbered(5, Some(0))), None);
    let (paragraph, level) = styles.numbered_paragraph(&numbering, Some(&own));
    let label = styles.label(&paragraph, Some(&mark), &level.unwrap());
    assert_eq!(size(&label), Some("48"));
    assert_eq!(bold(&label), Some(true));
    assert_eq!(color(&label), Some("FF0000"));
}
