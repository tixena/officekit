use alloc::format;
use alloc::vec::Vec;

use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    ParagraphProperties, ParagraphPropertiesPPrGeneral, RunFonts, RunProperties,
    RunPropertiesContent, SpacingBetweenLinesAndAboveBelowParagraph, StyleDefinitions,
};
use officekit_xml::parse::parse;

use super::{StyleSheet, WORDPROCESSING};
use crate::docx::Docx;
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
