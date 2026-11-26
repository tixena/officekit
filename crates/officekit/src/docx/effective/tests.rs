use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::Spelling;
use officekit_markup::common_simple_types::{MeasurementInTwentiethsOfAPoint, OnOffValue};
use officekit_markup::drawing::Theme;
use officekit_markup::office_word_2010_wordml::ConflictInsTrackChange;
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, Bold, ColorValue, ConditionalTableStyleFormattingTypes,
    ConditionalTableStyleFormattingTypesMember, ConflictChangeContent, DecimalNumberValue,
    DefaultRunProperties, DeletedTableRow, DocumentDefaultParagraphAndRunProperties,
    HorizontalAlignmentType, HorizontalAlignmentTypeMember, InsertedTableRow,
    MeasurementInHalfPoints, MoveDestinationParagraph, MoveSourceParagraph,
    NonComplexScriptFontSize, NumberingDefinitionInstance,
    NumberingDefinitionInstanceReferenceNumId, NumberingDefinitionInstanceReferenceNumPr,
    NumberingDefinitions, NumberingLevelOverrideDefinition, NumberingLevelReference,
    ParagraphAlignment, ParagraphIndentation, ParagraphMarkRunPropertiesRevision,
    ParagraphProperties, ParagraphPropertiesPPrGeneral, ParentStyleID, ReferencedCharacterStyle,
    ReferencedParagraphStyle, ReferencedTableStyle, RunContentColor, RunFonts, RunProperties,
    RunPropertiesContent, RunPropertiesForTheParagraphMark, SignedMeasurementInTwentiethsOfPoint,
    SpacingBetweenLinesAndAboveBelowParagraph, StyleConditionalTableFormattingProperties,
    StyleDefinition, StyleDefinitions, StyleTypes, StyleTypesMember, TableCellProperties,
    TableProperties, TableRowProperties, TableRowPropertiesContent,
    TableStyleConditionalFormattingSettingsException, ThemeFont, ThemeFontMember, write,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::Element;
use officekit_xml::parse::parse;
use officekit_xml::write::Scope;

use super::{StyleSheet, WORDPROCESSING};
use crate::docx::Docx;
use crate::docx::fonts::ThemeFonts;
use crate::docx::numbering::Numbering;
use crate::docx::table_style::{CellPosition, TableFormat};
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

/// A property of the `CT_OnOff` type `$kind`, with no `w:val` or with `w:val="0"`.
macro_rules! on_off {
    ($kind:ident) => {
        officekit_markup::wordprocessing::$kind::default()
    };
    ($kind:ident, off) => {{
        let mut property = officekit_markup::wordprocessing::$kind::default();
        property.val = Some(OnOffValue::from(false));
        property
    }};
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

/// The text of a simple-type value.
fn spelled<T>(value: &T) -> String
where
    T: Spelling,
{
    value.spelling(ConformanceClass::Transitional).into_owned()
}

fn size(run: &RunProperties) -> Option<String> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::NonComplexScriptFontSize(found) = property {
            found.val.as_ref().map(spelled)
        } else {
            None
        }
    })
}

fn color(run: &RunProperties) -> Option<String> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::RunContentColor(found) = property {
            found.val.as_ref().map(spelled)
        } else {
            None
        }
    })
}

fn fonts(run: &RunProperties) -> Option<&RunFonts> {
    run.content.iter().find_map(|property| {
        if let RunPropertiesContent::RunFonts(found) = property {
            Some::<&RunFonts>(found)
        } else {
            None
        }
    })
}

fn bold(run: &RunProperties) -> Option<bool> {
    toggle_of(run, "b")
}

fn justification(paragraph: &ParagraphPropertiesPPrGeneral) -> Option<String> {
    paragraph
        .paragraph_alignment
        .as_ref()
        .and_then(|found| found.val.as_ref())
        .map(spelled)
}

fn spacing(
    paragraph: &ParagraphPropertiesPPrGeneral,
) -> &SpacingBetweenLinesAndAboveBelowParagraph {
    paragraph
        .spacing_between_lines_and_above_below_paragraph
        .as_deref()
        .unwrap()
}

fn stop_positions(paragraph: &ParagraphPropertiesPPrGeneral) -> Vec<String> {
    paragraph
        .set_of_custom_tab_stops
        .iter()
        .flat_map(|tabs| &tabs.custom_tab_stop)
        .filter_map(|stop| stop.pos.as_ref().map(spelled))
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
    assert_eq!(size(&paragraph.run_properties).as_deref(), Some("24"));
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
    assert_eq!(size(&paragraph.run_properties).as_deref(), Some("20"));
    let ancestors = sheet(&format!(
        r#"{NORMAL}
        <w:style w:type="paragraph" w:styleId="BodyText"><w:basedOn w:val="Normal"/><w:rPr><w:sz w:val="24"/></w:rPr></w:style>
        <w:style w:type="paragraph" w:styleId="First"><w:basedOn w:val="BodyText"/></w:style>"#
    ));
    let inherited = ancestors.paragraph(Some(&named));
    assert_eq!(size(&inherited.run_properties).as_deref(), Some("24"));
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
            .map(spelled)
            .as_deref(),
        Some("360")
    );
    assert_eq!(
        spacing(&paragraph.properties)
            .after
            .as_ref()
            .map(spelled)
            .as_deref(),
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
    assert_eq!(
        justification(&paragraph.properties).as_deref(),
        Some("center")
    );
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
            .map(spelled)
            .as_deref(),
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
    assert_eq!(size(&run).as_deref(), Some("18"));
    let plain = styles.run(&paragraph, None);
    assert_eq!(size(&plain).as_deref(), Some("40"));
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
    assert_eq!(color(&run).as_deref(), Some("00FF00"));
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
    assert_eq!(size(&paragraph.run_properties).as_deref(), Some("30"));
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
            .map(spelled)
            .as_deref(),
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
        spacing(&properties).before.as_ref().map(spelled).as_deref(),
        Some("240")
    );
    assert_eq!(
        spacing(&properties).after.as_ref().map(spelled).as_deref(),
        Some("160")
    );
    assert!(
        properties
            .unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
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
    assert_eq!(justification(&properties).as_deref(), Some("center"));
    assert!(
        properties
            .unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
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
    assert!(
        borders
            .unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
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
    assert!(
        numbering
            .unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
}

#[test]
fn a_run_property_added_after_a_style_revision_keeps_its_typed_field() {
    let styles = sheet(&format!(
        r#"{NORMAL}<w:style w:type="paragraph" w:styleId="Tracked"><w:rPr><w:sz w:val="20"/><w:rPrChange w:id="1" w:author="a"><w:rPr/></w:rPrChange></w:rPr></w:style>"#
    ));
    let named = own_paragraph(r#"<w:pStyle w:val="Tracked"/>"#);
    let paragraph = styles.paragraph(Some(&named));
    let run = styles.run(&paragraph, Some(&own_run(r#"<w:color w:val="FF0000"/>"#)));
    assert_eq!(size(&run).as_deref(), Some("20"));
    assert_eq!(color(&run).as_deref(), Some("FF0000"));
    assert!(
        run.unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
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
    assert!(
        run.unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
}

fn font_size(half_points: &str) -> RunPropertiesContent {
    let mut size = NonComplexScriptFontSize::default();
    size.val = Some(MeasurementInHalfPoints::from(half_points.to_owned()));
    RunPropertiesContent::from(size)
}

fn character_reference(id: &str) -> RunPropertiesContent {
    let mut reference = ReferencedCharacterStyle::default();
    reference.val = Some(id.to_owned());
    RunPropertiesContent::from(reference)
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
    assert_eq!(size(&effective).as_deref(), Some("48"));
}

#[test]
fn a_paragraph_with_no_mark_gives_its_runs_properties() {
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let paragraph = styles.paragraph(Some(&ParagraphProperties::default()));
    let effective = styles.paragraph_mark(&paragraph, None);
    assert_eq!(size(&effective).as_deref(), Some("22"));
    assert_eq!(effective, styles.run(&paragraph, None));
}

#[test]
fn the_mark_character_style_gives_its_size() {
    let big = style_of("Big", StyleTypesMember::Character, vec![font_size("40")]);
    let styles = typed_sheet(vec![font_size("22")], vec![big]);
    let effective = effective_mark(&styles, mark_of(vec![character_reference("Big")]));
    assert_eq!(size(&effective).as_deref(), Some("40"));
}

#[test]
fn the_mark_own_size_wins_over_its_character_style() {
    let big = style_of("Big", StyleTypesMember::Character, vec![font_size("40")]);
    let styles = typed_sheet(vec![font_size("22")], vec![big]);
    let effective = effective_mark(
        &styles,
        mark_of(vec![character_reference("Big"), font_size("48")]),
    );
    assert_eq!(size(&effective).as_deref(), Some("48"));
}

#[test]
fn a_mark_toggle_combines_like_a_run_toggle() {
    let strong = style_of(
        "Strong",
        StyleTypesMember::Character,
        vec![RunPropertiesContent::from(Bold::default())],
    );
    let styles = typed_sheet(
        vec![RunPropertiesContent::from(Bold::default())],
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
    tracked.conflict_change_content = Some(Box::new(ConflictChangeContent::from(
        ConflictInsTrackChange::default(),
    )));
    tracked.run_properties_revision = Some(Box::new(ParagraphMarkRunPropertiesRevision::default()));

    let effective = effective_mark(&styles, tracked);
    assert_eq!(size(&effective).as_deref(), Some("48"));
    assert!(
        effective
            .unknown()
            .is_none_or(|unknown| unknown.children.is_empty())
    );
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
    list_with(styles, general, run)
}

/// The instance 5 of the abstract definition 4, whose level 0 has the paragraph properties
/// `general` and the run properties `run`.
fn list_with(
    styles: &StyleSheet,
    general: ParagraphPropertiesPPrGeneral,
    run: Vec<RunPropertiesContent>,
) -> Numbering {
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
fn indents(paragraph: &ParagraphPropertiesPPrGeneral) -> (Option<String>, Option<String>) {
    let found = paragraph.paragraph_indentation.as_deref();
    (
        found.and_then(|indent| indent.left.as_ref()).map(spelled),
        found
            .and_then(|indent| indent.hanging.as_ref())
            .map(spelled),
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
    assert_eq!(
        indents(&paragraph.properties),
        (Some("720".to_owned()), Some("360".to_owned()))
    );
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
        .map(spelled);
    assert_eq!(num_id.as_deref(), Some("5"));

    let wider = own_of(
        "Indented",
        Some(numbered(5, Some(0))),
        Some(indentation("1440", None)),
    );
    let (own_indent, _) = styles.numbered_paragraph(&numbering, Some(&wider));
    assert_eq!(
        indents(&own_indent.properties),
        (Some("1440".to_owned()), Some("360".to_owned()))
    );
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
    assert_eq!(
        indents(&paragraph.properties),
        (Some("100".to_owned()), Some("360".to_owned()))
    );
}

#[test]
fn the_label_takes_the_mark_s_properties_under_the_level_s() {
    let mut red = RunContentColor::default();
    red.val = Some(ColorValue::from("FF0000".to_owned()));
    let styles = typed_sheet(vec![font_size("22")], vec![]);
    let numbering = list_of(
        &styles,
        vec![
            RunPropertiesContent::from(Bold::default()),
            RunPropertiesContent::from(red),
        ],
    );
    let mut off = Bold::default();
    off.val = Some(false.into());
    let mark = mark_of(vec![RunPropertiesContent::from(off), font_size("48")]);
    let own = own_of("Normal", Some(numbered(5, Some(0))), None);
    let (paragraph, level) = styles.numbered_paragraph(&numbering, Some(&own));
    let label = styles.label(&paragraph, Some(&mark), &level.unwrap());
    assert_eq!(size(&label).as_deref(), Some("48"));
    assert_eq!(bold(&label), Some(true));
    assert_eq!(color(&label).as_deref(), Some("FF0000"));
}

fn aligned(alignment: HorizontalAlignmentTypeMember) -> ParagraphPropertiesPPrGeneral {
    let mut found = ParagraphAlignment::default();
    found.val = Some(HorizontalAlignmentType::from(alignment));
    let mut general = ParagraphPropertiesPPrGeneral::default();
    general.paragraph_alignment = Some(Box::new(found));
    general
}

fn run_of(content: Vec<RunPropertiesContent>) -> RunProperties {
    let mut run = RunProperties::default();
    run.content = content;
    run
}

/// A table style `Grid` with the paragraph properties `paragraph`, the run properties `run` and a
/// first-row format whose runs have `first_row`.
fn grid_style(
    paragraph: Option<ParagraphPropertiesPPrGeneral>,
    run: Vec<RunPropertiesContent>,
    first_row: Vec<RunPropertiesContent>,
) -> StyleDefinition {
    let mut format = StyleConditionalTableFormattingProperties::default();
    format.r#type = Some(ConditionalTableStyleFormattingTypes::from(
        ConditionalTableStyleFormattingTypesMember::FirstRow,
    ));
    format.run_properties = Some(Box::new(run_of(first_row)));
    let mut style = style_of("Grid", StyleTypesMember::Table, run);
    style.paragraph_properties_p_pr_general = paragraph.map(Box::new);
    style
        .style_conditional_table_formatting_properties
        .push(format);
    style
}

/// A table that names `Grid` and turns its first-row format on.
fn grid_table(styles: &StyleSheet) -> TableFormat {
    let mut reference = ReferencedTableStyle::default();
    reference.val = Some("Grid".to_owned());
    let mut look = TableStyleConditionalFormattingSettingsException::default();
    look.first_row = Some(true.into());
    let mut own = TableProperties::default();
    own.referenced_table_style = Some(Box::new(reference));
    own.table_style_conditional_formatting_settings_exception = Some(Box::new(look));
    styles.table(&own)
}

#[test]
fn a_paragraph_in_a_cell_lays_the_table_style_under_the_paragraph_style() {
    let mut default_run = DefaultRunProperties::default();
    default_run.run_properties = Some(Box::new(run_of(vec![font_size("20")])));
    let mut defaults = DocumentDefaultParagraphAndRunProperties::default();
    defaults.default_run_properties = Some(Box::new(default_run));
    let mut definitions = StyleDefinitions::default();
    definitions.document_default_paragraph_and_run_properties = Some(Box::new(defaults));
    definitions.style_definition = vec![
        grid_style(
            Some(aligned(HorizontalAlignmentTypeMember::Center)),
            vec![font_size("18")],
            vec![],
        ),
        paragraph_style("Right", aligned(HorizontalAlignmentTypeMember::Right)),
    ];
    let styles = StyleSheet::new(&definitions);
    let table = grid_table(&styles);
    let at = CellPosition::new(1, 2, 0, 1, false);

    let named = own_of("Right", None, None);
    let (right, _) = styles.paragraph_in_cell(&table, at, None, Some(&named));
    assert_eq!(size(&styles.run(&right, None)).as_deref(), Some("18"));
    assert_eq!(justification(&right.properties).as_deref(), Some("right"));

    let (plain, _) = styles.paragraph_in_cell(&table, at, None, None);
    assert_eq!(size(&styles.run(&plain, None)).as_deref(), Some("18"));
    assert_eq!(justification(&plain.properties).as_deref(), Some("center"));
}

#[test]
fn a_toggle_combines_the_table_paragraph_and_character_levels() {
    let styles = sheet_of(vec![
        grid_style(
            None,
            vec![],
            vec![RunPropertiesContent::from(Bold::default())],
        ),
        style_of(
            "Strong",
            StyleTypesMember::Paragraph,
            vec![RunPropertiesContent::from(Bold::default())],
        ),
    ]);
    let table = grid_table(&styles);
    let own = own_of("Strong", None, None);
    let at = CellPosition::new(0, 2, 0, 1, false);
    let (paragraph, _) = styles.paragraph_in_cell(&table, at, None, Some(&own));
    assert_eq!(paragraph.table_run_properties.len(), 1);
    assert_eq!(bold(&styles.run(&paragraph, None)), Some(false));
    let direct = run_of(vec![RunPropertiesContent::from(Bold::default())]);
    assert_eq!(bold(&styles.run(&paragraph, Some(&direct))), Some(true));

    let below = CellPosition::new(1, 2, 0, 1, false);
    let (outside_first_row, _) = styles.paragraph_in_cell(&table, below, None, Some(&own));
    assert_eq!(outside_first_row.table_run_properties.len(), 0);
    assert_eq!(bold(&styles.run(&outside_first_row, None)), Some(true));
}

#[test]
fn a_numbered_paragraph_in_a_cell_takes_its_level_as_outside_a_table() {
    let styles = sheet_of(vec![
        grid_style(
            Some(aligned(HorizontalAlignmentTypeMember::Center)),
            vec![],
            vec![],
        ),
        paragraph_style("Body", ParagraphPropertiesPPrGeneral::default()),
    ]);
    let numbering = list_of(&styles, vec![]);
    let table = grid_table(&styles);
    let at = CellPosition::new(0, 1, 0, 1, false);
    let own = own_of("Body", Some(numbered(5, Some(0))), None);

    let (inside, level) = styles.paragraph_in_cell(&table, at, Some(&numbering), Some(&own));
    let (_, outside) = styles.numbered_paragraph(&numbering, Some(&own));
    assert_eq!(level.as_ref().map(|found| found.ilvl), Some(0));
    assert_eq!(level, outside);
    assert_eq!(
        indents(&inside.properties),
        (Some("720".to_owned()), Some("360".to_owned()))
    );
    assert_eq!(justification(&inside.properties).as_deref(), Some("center"));

    let (unnumbered, none) = styles.paragraph_in_cell(&table, at, None, Some(&own));
    assert_eq!(none, None);
    assert_eq!(indents(&unnumbered.properties), (None, None));
}

/// A sheet whose document defaults hold the run properties `defaults`, and no style.
fn defaults_sheet(defaults: Vec<RunPropertiesContent>) -> StyleSheet {
    let mut default_run = DefaultRunProperties::default();
    default_run.run_properties = Some(Box::new(run_of(defaults)));
    let mut document = DocumentDefaultParagraphAndRunProperties::default();
    document.default_run_properties = Some(Box::new(default_run));
    let mut definitions = StyleDefinitions::default();
    definitions.document_default_paragraph_and_run_properties = Some(Box::new(document));
    StyleSheet::new(&definitions)
}

fn ascii_theme(member: ThemeFontMember) -> RunPropertiesContent {
    let mut fonts = RunFonts::default();
    fonts.ascii_theme = Some(ThemeFont::from(member));
    RunPropertiesContent::from(fonts)
}

fn ascii(family: &str) -> RunPropertiesContent {
    let mut fonts = RunFonts::default();
    fonts.ascii = Some(family.to_owned());
    RunPropertiesContent::from(fonts)
}

#[test]
fn a_run_s_own_font_replaces_the_defaults_theme_font_of_its_kind() {
    let styles = defaults_sheet(vec![ascii_theme(ThemeFontMember::MinorHAnsi)]);
    let paragraph = styles.paragraph(None);
    let run = styles.run(&paragraph, Some(&run_of(vec![ascii("Courier New")])));
    let effective = fonts(&run);
    assert_eq!(
        effective.and_then(|found| found.ascii.as_deref()),
        Some("Courier New")
    );
    assert_eq!(effective.and_then(|found| found.ascii_theme.as_ref()), None);

    let mut theme = Theme::default();
    theme.elements.font_scheme.minor_fonts.latin_font.typeface = Some("Calibri".to_owned());
    let families = ThemeFonts::new(Some(&theme), None).families(effective);
    assert_eq!(families.ascii.as_deref(), Some("Courier New"));
}

#[test]
fn a_run_s_own_theme_font_replaces_the_style_s_font_of_its_kind() {
    let styles = typed_sheet(vec![ascii("Arial")], vec![]);
    let paragraph = styles.paragraph(None);
    let own = run_of(vec![ascii_theme(ThemeFontMember::MajorAscii)]);
    let run = styles.run(&paragraph, Some(&own));
    let effective = fonts(&run);
    assert_eq!(
        effective.and_then(|found| found.ascii_theme.as_ref()),
        Some(&ThemeFont::from(ThemeFontMember::MajorAscii))
    );
    assert_eq!(effective.and_then(|found| found.ascii.as_deref()), None);
}

#[test]
fn a_kind_the_later_level_does_not_state_keeps_the_earlier_font() {
    let styles = defaults_sheet(vec![ascii_theme(ThemeFontMember::MinorHAnsi)]);
    let paragraph = styles.paragraph(None);
    let mut own = RunFonts::default();
    own.cs = Some("Arial".to_owned());
    let run = styles.run(
        &paragraph,
        Some(&run_of(vec![RunPropertiesContent::from(own)])),
    );
    let effective = fonts(&run);
    assert_eq!(
        effective.and_then(|found| found.ascii_theme.as_ref()),
        Some(&ThemeFont::from(ThemeFontMember::MinorHAnsi))
    );
    assert_eq!(
        effective.and_then(|found| found.cs.as_deref()),
        Some("Arial")
    );
}

/// The local names of the children of `element`, in the order written.
fn child_names(element: &Element) -> Vec<String> {
    element
        .child_elements()
        .map(|child| child.name.local.clone())
        .collect()
}

#[test]
fn the_on_off_names_are_the_on_off_children_of_the_paragraph_run_table_row_and_cell_properties() {
    let mut paragraph = ParagraphPropertiesPPrGeneral::default();
    paragraph.adjust_spacing_east_asian_text_numbers =
        Some(Box::new(on_off!(AdjustSpacingEastAsianTextNumbers)));
    paragraph.adjust_spacing_latin_east_asian_text =
        Some(Box::new(on_off!(AdjustSpacingLatinEastAsianText)));
    paragraph.allow_first_last_line_display = Some(Box::new(on_off!(AllowFirstLastLineDisplay)));
    paragraph.allow_line_breaking_at_character_level =
        Some(Box::new(on_off!(AllowLineBreakingAtCharacterLevel)));
    paragraph.allow_punctuation_extend_past_text_extents =
        Some(Box::new(on_off!(AllowPunctuationExtendPastTextExtents)));
    paragraph.automatically_adjust_right_indent =
        Some(Box::new(on_off!(AutomaticallyAdjustRightIndent)));
    paragraph.compress_punctuation_at_start_of_line =
        Some(Box::new(on_off!(CompressPunctuationAtStartOfLine)));
    paragraph.document_grid_inter_line_paragraph_spacing =
        Some(Box::new(on_off!(DocumentGridInterLineParagraphSpacing)));
    paragraph.east_asian_typography_rules = Some(Box::new(on_off!(EastAsianTypographyRules)));
    paragraph.ignore_spacing_above_and_below = Some(Box::new(on_off!(IgnoreSpacingAboveAndBelow)));
    paragraph.keep_all_lines_on_one_page = Some(Box::new(on_off!(KeepAllLinesOnOnePage)));
    paragraph.keep_paragraph_with_next_paragraph =
        Some(Box::new(on_off!(KeepParagraphWithNextParagraph)));
    paragraph.prevent_text_frames_from_overlapping =
        Some(Box::new(on_off!(PreventTextFramesFromOverlapping)));
    paragraph.right_to_left_paragraph_layout = Some(Box::new(on_off!(RightToLeftParagraphLayout)));
    paragraph.start_paragraph_on_next_page = Some(Box::new(on_off!(StartParagraphOnNextPage)));
    paragraph.suppress_hyphenation_for_paragraph =
        Some(Box::new(on_off!(SuppressHyphenationForParagraph)));
    paragraph.suppress_line_numbers_for_paragraph =
        Some(Box::new(on_off!(SuppressLineNumbersForParagraph)));
    paragraph.use_left_right_indents_as_inside_outside_indents =
        Some(Box::new(on_off!(UseLeftRightIndentsAsInsideOutsideIndents)));
    let run = run_of(vec![
        RunPropertiesContent::from(on_off!(Bold)),
        RunPropertiesContent::from(on_off!(ComplexScriptBold)),
        RunPropertiesContent::from(on_off!(ComplexScriptItalics)),
        RunPropertiesContent::from(on_off!(DisplayAllCharactersAsCapitalLetters)),
        RunPropertiesContent::from(on_off!(DisplayCharacterOutline)),
        RunPropertiesContent::from(on_off!(DoNotCheckSpellingOrGrammar)),
        RunPropertiesContent::from(on_off!(DocumentGridInterLineParagraphSpacing)),
        RunPropertiesContent::from(on_off!(DoubleStrikethrough)),
        RunPropertiesContent::from(on_off!(Embossing)),
        RunPropertiesContent::from(on_off!(HiddenText)),
        RunPropertiesContent::from(on_off!(Imprinting)),
        RunPropertiesContent::from(on_off!(Italics)),
        RunPropertiesContent::from(on_off!(OfficeOpenXMLMath)),
        RunPropertiesContent::from(on_off!(ParagraphMarkIsAlwaysHidden)),
        RunPropertiesContent::from(on_off!(RightToLeftText)),
        RunPropertiesContent::from(on_off!(Shadow)),
        RunPropertiesContent::from(on_off!(SingleStrikethrough)),
        RunPropertiesContent::from(on_off!(SmallCaps)),
        RunPropertiesContent::from(on_off!(UseComplexScriptFormattingOnRun)),
        RunPropertiesContent::from(on_off!(WebHiddenText)),
    ]);
    let mut names = child_names(&write::paragraph_properties_p_pr_general(
        &paragraph,
        &mut Scope::default(),
    ));
    names.extend(child_names(&write::run_properties(
        &run,
        &mut Scope::default(),
    )));
    let mut table = TableProperties::default();
    table.visually_right_to_left_table = Some(Box::new(on_off!(VisuallyRightToLeftTable, off)));
    names.extend(child_names(&write::table_properties(
        &table,
        &mut Scope::default(),
    )));
    let mut row = TableRowProperties::default();
    row.content = alloc::vec![
        TableRowPropertiesContent::from(on_off!(HiddenTableRowMarker, off)),
        TableRowPropertiesContent::from(on_off!(RepeatTableRow, off)),
        TableRowPropertiesContent::from(on_off!(TableRowCannotBreakAcrossPages, off)),
    ];
    names.extend(child_names(&write::table_row_properties(
        &row,
        &mut Scope::default(),
    )));
    let mut cell = TableCellProperties::default();
    cell.dont_wrap_cell_content = Some(Box::new(on_off!(DontWrapCellContent, off)));
    cell.fit_text_within_cell = Some(Box::new(on_off!(FitTextWithinCell, off)));
    cell.ignore_end_of_cell_marker = Some(Box::new(on_off!(IgnoreEndOfCellMarker, off)));
    names.extend(child_names(&write::table_cell_properties(
        &cell,
        &mut Scope::default(),
    )));
    names.sort_unstable();
    names.dedup();
    let mut listed = super::ON_OFF.to_vec();
    listed.sort_unstable();
    assert_eq!(names, listed);
}

#[test]
fn the_on_off_names_are_sorted() {
    assert!(super::ON_OFF.is_sorted_by(|left, right| left < right));
}

/// The style `id`, based on `parent`, with the paragraph properties `general`.
fn child_style(id: &str, parent: &str, general: ParagraphPropertiesPPrGeneral) -> StyleDefinition {
    let mut based = ParentStyleID::default();
    based.val = Some(parent.to_owned());
    let mut style = paragraph_style(id, general);
    style.parent_style_id = Some(Box::new(based));
    style
}

fn keep_next(paragraph: &ParagraphPropertiesPPrGeneral) -> Option<bool> {
    paragraph
        .keep_paragraph_with_next_paragraph
        .as_deref()
        .map(|found| onoff(found.val.as_ref()))
}

#[test]
fn an_on_off_property_with_no_val_turns_on_a_style_s_off() {
    let mut off = ParagraphPropertiesPPrGeneral::default();
    off.keep_paragraph_with_next_paragraph =
        Some(Box::new(on_off!(KeepParagraphWithNextParagraph, off)));
    let styles = sheet_of(vec![paragraph_style("Loose", off)]);
    let mut own = own_of("Loose", None, None);
    own.keep_paragraph_with_next_paragraph =
        Some(Box::new(on_off!(KeepParagraphWithNextParagraph)));
    let paragraph = styles.paragraph(Some(&own));
    assert_eq!(keep_next(&paragraph.properties), Some(true));
}

#[test]
fn a_run_property_with_no_val_turns_on_the_defaults_off() {
    let styles = defaults_sheet(vec![RunPropertiesContent::from(on_off!(
        DoNotCheckSpellingOrGrammar,
        off
    ))]);
    let paragraph = styles.paragraph(None);
    let own = run_of(vec![RunPropertiesContent::from(on_off!(
        DoNotCheckSpellingOrGrammar
    ))]);
    let run = styles.run(&paragraph, Some(&own));
    let no_proof = run.content.iter().find_map(|property| {
        if let RunPropertiesContent::DoNotCheckSpellingOrGrammar(found) = property {
            Some(onoff(found.val.as_ref()))
        } else {
            None
        }
    });
    assert_eq!(no_proof, Some(true));
}

#[test]
fn an_on_off_property_off_turns_off_a_style_s_on() {
    let mut on = ParagraphPropertiesPPrGeneral::default();
    on.keep_all_lines_on_one_page = Some(Box::new(on_off!(KeepAllLinesOnOnePage)));
    let styles = sheet_of(vec![paragraph_style("Kept", on)]);
    let mut own = own_of("Kept", None, None);
    own.keep_all_lines_on_one_page = Some(Box::new(on_off!(KeepAllLinesOnOnePage, off)));
    let paragraph = styles.paragraph(Some(&own));
    let keep_lines = paragraph
        .properties
        .keep_all_lines_on_one_page
        .as_deref()
        .map(|found| onoff(found.val.as_ref()));
    assert_eq!(keep_lines, Some(false));
}

#[test]
fn a_child_style_s_on_off_property_with_no_val_turns_on_its_parent_s_off() {
    let mut off = ParagraphPropertiesPPrGeneral::default();
    off.allow_first_last_line_display = Some(Box::new(on_off!(AllowFirstLastLineDisplay, off)));
    let mut on = ParagraphPropertiesPPrGeneral::default();
    on.allow_first_last_line_display = Some(Box::new(on_off!(AllowFirstLastLineDisplay)));
    let styles = sheet_of(vec![
        paragraph_style("Parent", off),
        child_style("Child", "Parent", on),
    ]);
    let paragraph = styles.paragraph(Some(&own_of("Child", None, None)));
    let widow_control = paragraph
        .properties
        .allow_first_last_line_display
        .as_deref()
        .map(|found| onoff(found.val.as_ref()));
    assert_eq!(widow_control, Some(true));
}

#[test]
fn a_level_s_on_off_property_with_no_val_turns_on_a_style_s_off() {
    let mut off = ParagraphPropertiesPPrGeneral::default();
    off.keep_paragraph_with_next_paragraph =
        Some(Box::new(on_off!(KeepParagraphWithNextParagraph, off)));
    let styles = sheet_of(vec![paragraph_style("Loose", off)]);
    let mut level = ParagraphPropertiesPPrGeneral::default();
    level.keep_paragraph_with_next_paragraph =
        Some(Box::new(on_off!(KeepParagraphWithNextParagraph)));
    let numbering = list_with(&styles, level, vec![]);

    let own = own_of("Loose", Some(numbered(5, Some(0))), None);
    let (paragraph, found) = styles.numbered_paragraph(&numbering, Some(&own));
    assert_eq!(found.map(|listed| listed.ilvl), Some(0));
    assert_eq!(keep_next(&paragraph.properties), Some(true));
}

#[test]
fn an_indentation_still_merges_attribute_by_attribute() {
    let mut indented = ParagraphPropertiesPPrGeneral::default();
    indented.paragraph_indentation = Some(Box::new(indentation("100", Some("50"))));
    let styles = sheet_of(vec![paragraph_style("Indented", indented)]);
    let own = own_of("Indented", None, Some(indentation("200", None)));
    let paragraph = styles.paragraph(Some(&own));
    assert_eq!(
        indents(&paragraph.properties),
        (Some("200".to_owned()), Some("50".to_owned()))
    );
}

#[test]
fn the_last_default_paragraph_style_is_the_one() {
    let mut first = paragraph_style("A", ParagraphPropertiesPPrGeneral::default());
    first.default = Some(true.into());
    let mut last = paragraph_style("B", aligned(HorizontalAlignmentTypeMember::Center));
    last.default = Some(true.into());
    let styles = sheet_of(vec![first, last]);
    assert_eq!(styles.default_paragraph(), Some("B"));
    let paragraph = styles.paragraph(None);
    assert_eq!(paragraph.style.as_deref(), Some("B"));
    assert_eq!(
        justification(&paragraph.properties).as_deref(),
        Some("center")
    );
}

#[test]
fn a_default_table_style_after_the_default_paragraph_style_leaves_it() {
    let mut paragraph = paragraph_style("A", ParagraphPropertiesPPrGeneral::default());
    paragraph.default = Some(true.into());
    let mut table = style_of("T", StyleTypesMember::Table, vec![]);
    table.default = Some(true.into());
    let styles = sheet_of(vec![paragraph, table]);
    assert_eq!(styles.default_paragraph(), Some("A"));
}
