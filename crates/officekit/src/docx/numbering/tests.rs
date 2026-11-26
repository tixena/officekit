use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, DecimalNumberValue, DisplayAllLevelsUsingArabicNumerals,
    LanguagesForRunContent, NumberingDefinitionInstance, NumberingDefinitionInstanceReferenceNumId,
    NumberingDefinitionInstanceReferenceNumPr, NumberingDefinitions, NumberingFormat,
    NumberingLevelDefinitionOverride, NumberingLevelOverrideDefinition, NumberingLevelReference,
    NumberingLevelStartingValueOverride, NumberingLevelText, NumberingStyleDefinition,
    NumberingStyleReference, ParagraphProperties, ParagraphPropertiesPPrGeneral,
    ReferencedParagraphStyle, RestartNumberingLevelSymbol, RunProperties, RunPropertiesContent,
    StNumberFormat, StNumberFormatMember, StartingValue, StyleDefinition, StyleDefinitions,
    StyleTypes, StyleTypesMember,
};

use super::{ListCounters, Numbering};
use crate::docx::effective::StyleSheet;

fn number(value: i64) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_string())
}

/// A `w:lvl` of level `ilvl` in `format` with the level text `text`, and no `w:start`.
fn unstarted(
    ilvl: i64,
    format: StNumberFormatMember,
    text: &str,
) -> NumberingLevelOverrideDefinition {
    let mut numbering_format = NumberingFormat::default();
    numbering_format.val = Some(StNumberFormat::from(format));
    let mut level_text = NumberingLevelText::default();
    level_text.val = Some(text.to_owned());
    let mut definition = NumberingLevelOverrideDefinition::default();
    definition.ilvl = Some(number(ilvl));
    definition.numbering_format = Some(Box::new(numbering_format));
    definition.numbering_level_text = Some(Box::new(level_text));
    definition
}

/// `definition` with the `w:start` `start`.
fn starting(
    mut definition: NumberingLevelOverrideDefinition,
    start: i64,
) -> NumberingLevelOverrideDefinition {
    let mut value = StartingValue::default();
    value.val = Some(number(start));
    definition.starting_value = Some(Box::new(value));
    definition
}

/// A `w:lvl` of level `ilvl` that starts at 1, in `format`, with the level text `text`.
fn level(ilvl: i64, format: StNumberFormatMember, text: &str) -> NumberingLevelOverrideDefinition {
    starting(unstarted(ilvl, format, text), 1)
}

/// `definition` with the `w:lvlRestart` `restart`.
fn restarting(
    mut definition: NumberingLevelOverrideDefinition,
    restart: i64,
) -> NumberingLevelOverrideDefinition {
    let mut symbol = RestartNumberingLevelSymbol::default();
    symbol.val = Some(number(restart));
    definition.restart_numbering_level_symbol = Some(Box::new(symbol));
    definition
}

/// `definition` with the `w:format` `format` on its `w:numFmt`.
fn custom(
    mut definition: NumberingLevelOverrideDefinition,
    format: &str,
) -> NumberingLevelOverrideDefinition {
    if let Some(numbering_format) = definition.numbering_format.as_deref_mut() {
        numbering_format.format = Some(format.to_owned());
    }
    definition
}

/// The decimal levels 0 to 2, each starting at 1, with the texts `%1.`, `%1.%2` and `%1.%2.%3`.
fn outline() -> Vec<NumberingLevelOverrideDefinition> {
    Vec::from([
        level(0, StNumberFormatMember::Decimal, "%1."),
        level(1, StNumberFormatMember::Decimal, "%1.%2"),
        level(2, StNumberFormatMember::Decimal, "%1.%2.%3"),
    ])
}

fn abstract_of(
    id: i64,
    levels: Vec<NumberingLevelOverrideDefinition>,
) -> AbstractNumberingDefinition {
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(number(id));
    definition.numbering_level_override_definition = levels;
    definition
}

/// A `w:num` `num_id` naming the abstract definition `abstract_id`, with `overrides`.
fn instance(
    num_id: i64,
    abstract_id: i64,
    overrides: Vec<NumberingLevelDefinitionOverride>,
) -> NumberingDefinitionInstance {
    let mut found = NumberingDefinitionInstance::default();
    found.num_id = Some(number(num_id));
    found.abstract_numbering_definition_reference.val = Some(number(abstract_id));
    found.numbering_level_definition_override = overrides;
    found
}

/// A `w:lvlOverride` of level `ilvl` holding `definition` and the `w:startOverride` `start`.
fn level_override(
    ilvl: i64,
    definition: Option<NumberingLevelOverrideDefinition>,
    start: Option<i64>,
) -> NumberingLevelDefinitionOverride {
    let mut found = NumberingLevelDefinitionOverride::default();
    found.ilvl = Some(number(ilvl));
    found.numbering_level_override_definition = definition.map(Box::new);
    found.numbering_level_starting_value_override = start.map(|value| {
        let mut stated = NumberingLevelStartingValueOverride::default();
        stated.val = Some(number(value));
        Box::new(stated)
    });
    found
}

fn numbering_of(
    abstracts: Vec<AbstractNumberingDefinition>,
    instances: Vec<NumberingDefinitionInstance>,
    styles: &StyleSheet,
) -> Numbering {
    let mut part = NumberingDefinitions::default();
    part.abstract_numbering_definition = abstracts;
    part.numbering_definition_instance = instances;
    Numbering::new(&part, styles)
}

/// The abstract definition 4 holding `levels`, and the instance 5 naming it.
fn single(levels: Vec<NumberingLevelOverrideDefinition>) -> Numbering {
    numbering_of(
        Vec::from([abstract_of(4, levels)]),
        Vec::from([instance(5, 4, Vec::new())]),
        &StyleSheet::default(),
    )
}

/// The labels of list items numbered with the levels `items`, each a `w:numId` and a `w:ilvl`,
/// in order.
fn labels(numbering: &Numbering, items: &[(i64, u8)]) -> Vec<String> {
    let mut counters = ListCounters::default();
    items
        .iter()
        .map(|(num_id, ilvl)| counters.next(numbering, &numbering.level(*num_id, *ilvl).unwrap()))
        .collect()
}

/// The labels of items of the instance 5 numbered with the levels `levels`, in order.
fn labels_of(numbering: &Numbering, levels: &[u8]) -> Vec<String> {
    let items: Vec<(i64, u8)> = levels.iter().map(|ilvl| (5, *ilvl)).collect();
    labels(numbering, &items)
}

fn num_pr(num_id: Option<i64>, ilvl: Option<i64>) -> NumberingDefinitionInstanceReferenceNumPr {
    let mut found = NumberingDefinitionInstanceReferenceNumPr::default();
    found.numbering_definition_instance_reference_num_id = num_id.map(|value| {
        let mut reference = NumberingDefinitionInstanceReferenceNumId::default();
        reference.val = Some(number(value));
        Box::new(reference)
    });
    found.numbering_level_reference = ilvl.map(|value| {
        let mut reference = NumberingLevelReference::default();
        reference.val = Some(number(value));
        Box::new(reference)
    });
    found
}

/// A style `id` of `kind` whose paragraph properties hold `numbering`.
fn style_with(
    id: &str,
    kind: StyleTypesMember,
    numbering: NumberingDefinitionInstanceReferenceNumPr,
) -> StyleDefinition {
    let mut general = ParagraphPropertiesPPrGeneral::default();
    general.numbering_definition_instance_reference_num_pr = Some(Box::new(numbering));
    let mut style = StyleDefinition::default();
    style.style_id = Some(id.to_owned());
    style.r#type = Some(StyleTypes::from(kind));
    style.paragraph_properties_p_pr_general = Some(Box::new(general));
    style
}

fn sheet_of(styles: Vec<StyleDefinition>) -> StyleSheet {
    let mut definitions = StyleDefinitions::default();
    definitions.style_definition = styles;
    StyleSheet::new(&definitions)
}

/// A paragraph's own properties naming the style `style` and holding `numbering`.
fn own_paragraph(
    style: Option<&str>,
    numbering: Option<NumberingDefinitionInstanceReferenceNumPr>,
) -> ParagraphProperties {
    let mut own = ParagraphProperties::default();
    own.referenced_paragraph_style = style.map(|id| {
        let mut reference = ReferencedParagraphStyle::default();
        reference.val = Some(id.to_owned());
        Box::new(reference)
    });
    own.numbering_definition_instance_reference_num_pr = numbering.map(Box::new);
    own
}

/// The abstract definitions 0, which links to the numbering style `ListStyle`, and 7, which
/// holds `levels` and is that style's definition; the instances 3 naming 0 and 9 naming 7.
fn linked(levels: Vec<NumberingLevelOverrideDefinition>) -> NumberingDefinitions {
    let mut reference = NumberingStyleReference::default();
    reference.val = Some("ListStyle".to_owned());
    let mut linking = abstract_of(0, Vec::new());
    linking.numbering_style_reference = Some(Box::new(reference));
    let mut definition = NumberingStyleDefinition::default();
    definition.val = Some("ListStyle".to_owned());
    let mut underlying = abstract_of(7, levels);
    underlying.numbering_style_definition = Some(Box::new(definition));
    let mut part = NumberingDefinitions::default();
    part.abstract_numbering_definition = Vec::from([linking, underlying]);
    part.numbering_definition_instance =
        Vec::from([instance(3, 0, Vec::new()), instance(9, 7, Vec::new())]);
    part
}

#[test]
fn a_level_with_no_override_is_the_abstract_definition_s_level() {
    let numbering = single(outline());
    let found = numbering.level(5, 1).unwrap();
    assert_eq!(found.abstract_id, 4);
    assert_eq!(found.num_id, 5);
    assert_eq!(found.ilvl, 1);
    assert_eq!(Some(&found.definition), outline().get(1));
    assert_eq!(found.start_override, None);
}

#[test]
fn an_override_replaces_its_level_and_gives_its_start() {
    let replaced = level(0, StNumberFormatMember::Decimal, "%1)");
    let numbering = numbering_of(
        Vec::from([abstract_of(4, outline())]),
        Vec::from([instance(
            6,
            4,
            Vec::from([level_override(0, Some(replaced.clone()), Some(4))]),
        )]),
        &StyleSheet::default(),
    );
    let first = numbering.level(6, 0).unwrap();
    assert_eq!(first.definition, replaced);
    assert_eq!(first.start_override, Some(4));
    let second = numbering.level(6, 1).unwrap();
    assert_eq!(Some(&second.definition), outline().get(1));
    assert_eq!(second.start_override, None);
}

#[test]
fn a_num_style_link_leads_through_the_numbering_style_s_num() {
    let styles = sheet_of(Vec::from([style_with(
        "ListStyle",
        StyleTypesMember::Numbering,
        num_pr(Some(9), None),
    )]));
    let numbering = Numbering::new(&linked(outline()), &styles);
    let found = numbering.level(3, 1).unwrap();
    assert_eq!(found.abstract_id, 7);
    assert_eq!(found.num_id, 3);
    assert_eq!(Some(&found.definition), outline().get(1));
}

#[test]
fn a_num_style_link_to_a_missing_style_follows_the_style_link() {
    let numbering = Numbering::new(&linked(outline()), &StyleSheet::default());
    let found = numbering.level(3, 2).unwrap();
    assert_eq!(found.abstract_id, 7);
    assert_eq!(Some(&found.definition), outline().get(2));
}

#[test]
fn the_numbering_style_s_num_wins_over_another_style_link() {
    let mut part = linked(outline());
    let mut definition = NumberingStyleDefinition::default();
    definition.val = Some("ListStyle".to_owned());
    let mut other = abstract_of(
        8,
        Vec::from([level(0, StNumberFormatMember::UpperRoman, "%1")]),
    );
    other.numbering_style_definition = Some(Box::new(definition));
    part.abstract_numbering_definition.insert(0, other);
    let styles = sheet_of(Vec::from([style_with(
        "ListStyle",
        StyleTypesMember::Numbering,
        num_pr(Some(9), None),
    )]));
    assert_eq!(
        Numbering::new(&part, &styles)
            .level(3, 0)
            .unwrap()
            .abstract_id,
        7
    );
    assert_eq!(
        Numbering::new(&part, &StyleSheet::default())
            .level(3, 0)
            .unwrap()
            .abstract_id,
        8
    );
}

#[test]
fn a_link_back_to_the_same_definition_ends() {
    let mut reference = NumberingStyleReference::default();
    reference.val = Some("Loop".to_owned());
    let mut looping = abstract_of(
        0,
        Vec::from([level(0, StNumberFormatMember::Decimal, "%1")]),
    );
    looping.numbering_style_reference = Some(Box::new(reference));
    let styles = sheet_of(Vec::from([style_with(
        "Loop",
        StyleTypesMember::Numbering,
        num_pr(Some(3), None),
    )]));
    let numbering = numbering_of(
        Vec::from([looping]),
        Vec::from([instance(3, 0, Vec::new())]),
        &styles,
    );
    assert_eq!(numbering.level(3, 0).unwrap().abstract_id, 0);
}

#[test]
fn num_id_zero_a_missing_instance_and_a_missing_level_give_no_level() {
    let numbering = numbering_of(
        Vec::from([abstract_of(4, outline())]),
        Vec::from([instance(0, 4, Vec::new()), instance(5, 4, Vec::new())]),
        &StyleSheet::default(),
    );
    assert_eq!(numbering.level(0, 0), None);
    assert_eq!(numbering.level(6, 0), None);
    assert_eq!(numbering.level(5, 3), None);
    assert_eq!(Numbering::default().level(5, 0), None);
}

#[test]
fn a_level_naming_the_paragraph_style_numbers_its_paragraphs() {
    let mut named = level(1, StNumberFormatMember::Decimal, "%1.%2");
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some("Heading2".to_owned());
    named.referenced_paragraph_style = Some(Box::new(reference));
    let levels = Vec::from([
        level(0, StNumberFormatMember::Decimal, "%1."),
        named,
        level(2, StNumberFormatMember::Decimal, "%1.%2.%3"),
    ]);
    let styles = sheet_of(Vec::from([
        style_with(
            "Heading2",
            StyleTypesMember::Paragraph,
            num_pr(Some(5), None),
        ),
        style_with(
            "Heading3",
            StyleTypesMember::Paragraph,
            num_pr(Some(5), Some(2)),
        ),
    ]));
    let numbering = numbering_of(
        Vec::from([abstract_of(4, levels)]),
        Vec::from([instance(5, 4, Vec::new())]),
        &styles,
    );

    let through_style = own_paragraph(Some("Heading2"), None);
    let paragraph = styles.paragraph(Some(&through_style));
    let found = numbering.level_of(&paragraph, Some(&through_style));
    assert_eq!(found.map(|item| item.ilvl), Some(1));

    let stated = own_paragraph(Some("Heading2"), Some(num_pr(Some(5), Some(0))));
    let direct = styles.paragraph(Some(&stated));
    let own_level = numbering.level_of(&direct, Some(&stated));
    assert_eq!(own_level.map(|item| item.ilvl), Some(0));

    let unnamed = own_paragraph(Some("Heading3"), None);
    let other = styles.paragraph(Some(&unnamed));
    let style_level = numbering.level_of(&other, Some(&unnamed));
    assert_eq!(style_level.map(|item| item.ilvl), Some(2));
}

#[test]
fn a_paragraph_with_no_num_pr_or_num_id_zero_has_no_level() {
    let styles = sheet_of(Vec::from([style_with(
        "Listed",
        StyleTypesMember::Paragraph,
        num_pr(Some(5), None),
    )]));
    let numbering = single(outline());
    let plain = own_paragraph(None, None);
    assert_eq!(
        numbering.level_of(&styles.paragraph(Some(&plain)), Some(&plain)),
        None
    );
    let removed = own_paragraph(Some("Listed"), Some(num_pr(Some(0), None)));
    assert_eq!(
        numbering.level_of(&styles.paragraph(Some(&removed)), Some(&removed)),
        None
    );
}

#[test]
fn counters_over_three_levels_restart_the_deeper_levels() {
    assert_eq!(
        labels_of(&single(outline()), &[0, 1, 1, 2, 0, 1]),
        ["1.", "1.1", "1.2", "1.2.1", "2.", "2.1"]
    );
}

#[test]
fn a_level_restart_of_zero_never_restarts() {
    let mut levels = outline();
    if let Some(second) = levels.get_mut(1) {
        *second = restarting(second.clone(), 0);
    }
    let found = labels_of(&single(levels), &[0, 1, 1, 2, 0, 1]);
    assert_eq!(found.last().map(String::as_str), Some("2.3"));
}

#[test]
fn a_level_restart_names_the_level_whose_items_restart_it() {
    let levels = Vec::from([
        level(0, StNumberFormatMember::Decimal, "%1"),
        level(1, StNumberFormatMember::Decimal, "%2"),
        restarting(level(2, StNumberFormatMember::Decimal, "%3"), 1),
    ]);
    assert_eq!(
        labels_of(&single(levels), &[0, 1, 2, 1, 2, 0, 2]),
        ["1", "1", "1", "2", "2", "2", "1"]
    );
}

#[test]
fn a_level_restart_past_the_level_is_ignored() {
    let levels = Vec::from([
        level(0, StNumberFormatMember::Decimal, "%1"),
        restarting(level(1, StNumberFormatMember::Decimal, "%2"), 5),
    ]);
    assert_eq!(
        labels_of(&single(levels), &[0, 1, 1, 0, 1]),
        ["1", "1", "2", "2", "1"]
    );
}

#[test]
fn a_start_override_restarts_the_list_at_its_instance_s_first_item() {
    let numbering = numbering_of(
        Vec::from([abstract_of(
            4,
            Vec::from([level(0, StNumberFormatMember::Decimal, "%1")]),
        )]),
        Vec::from([
            instance(5, 4, Vec::new()),
            instance(6, 4, Vec::from([level_override(0, None, Some(1))])),
        ]),
        &StyleSheet::default(),
    );
    assert_eq!(
        labels(&numbering, &[(5, 0), (5, 0), (6, 0), (6, 0)]),
        ["1", "2", "1", "2"]
    );
    assert_eq!(
        labels(&numbering, &[(5, 0), (5, 0), (6, 0), (5, 0)]),
        ["1", "2", "1", "2"]
    );
}

#[test]
fn a_level_with_no_start_counts_from_zero() {
    let numbering = single(Vec::from([unstarted(
        0,
        StNumberFormatMember::Decimal,
        "%1",
    )]));
    assert_eq!(labels_of(&numbering, &[0, 0]), ["0", "1"]);
}

#[test]
fn each_level_writes_its_number_in_its_format() {
    let numbering = single(Vec::from([
        level(0, StNumberFormatMember::Decimal, "%1."),
        level(1, StNumberFormatMember::LowerLetter, "%2)"),
        level(2, StNumberFormatMember::UpperRoman, "%3."),
    ]));
    assert_eq!(labels_of(&numbering, &[0, 1, 2]), ["1.", "a)", "I."]);
}

#[test]
fn the_first_item_of_level_one_after_two_of_level_zero_is_two_one() {
    let found = labels_of(&single(outline()), &[0, 0, 1]);
    assert_eq!(found.last().map(String::as_str), Some("2.1"));
}

#[test]
fn an_earlier_level_no_item_has_counted_writes_its_start() {
    let levels = Vec::from([
        starting(unstarted(0, StNumberFormatMember::Decimal, "%1."), 3),
        level(1, StNumberFormatMember::Decimal, "%1.%2"),
    ]);
    assert_eq!(labels_of(&single(levels), &[1]), ["3.1"]);
}

#[test]
fn a_legal_level_writes_every_number_in_decimal() {
    let mut legal = level(2, StNumberFormatMember::Decimal, "%1.%2.%3");
    legal.display_all_levels_using_arabic_numerals =
        Some(Box::new(DisplayAllLevelsUsingArabicNumerals::default()));
    let numbering = single(Vec::from([
        level(0, StNumberFormatMember::UpperLetter, "%1"),
        level(1, StNumberFormatMember::LowerLetter, "%1.%2"),
        legal,
    ]));
    assert_eq!(
        labels_of(&numbering, &[0, 1, 1, 2, 2, 0, 1, 2]),
        ["A", "A.a", "A.b", "1.2.1", "1.2.2", "B", "B.a", "2.1.1"]
    );
}

#[test]
fn a_number_of_a_deeper_level_writes_nothing() {
    let numbering = single(Vec::from([level(
        0,
        StNumberFormatMember::Decimal,
        "%1 %2",
    )]));
    assert_eq!(labels_of(&numbering, &[0]), ["1 "]);
}

#[test]
fn a_percent_sign_not_followed_by_one_to_nine_is_text() {
    let numbering = single(Vec::from([level(
        0,
        StNumberFormatMember::Decimal,
        "%0%a%%1",
    )]));
    assert_eq!(labels_of(&numbering, &[0]), ["%0%a%1"]);
}

#[test]
fn a_bullet_is_its_level_text() {
    let numbering = single(Vec::from([
        level(0, StNumberFormatMember::Bullet, "\u{F0B7}"),
        level(1, StNumberFormatMember::Bullet, "%2"),
    ]));
    assert_eq!(labels_of(&numbering, &[0, 1]), ["\u{F0B7}", "%2"]);
}

#[test]
fn a_null_level_text_is_the_null_character() {
    let mut null = level(0, StNumberFormatMember::Decimal, "");
    if let Some(text) = null.numbering_level_text.as_deref_mut() {
        text.val = None;
        text.null = Some(true.into());
    }
    let mut empty_null = null.clone();
    if let Some(text) = empty_null.numbering_level_text.as_deref_mut() {
        text.val = Some(String::new());
    }
    let mut no_text = level(0, StNumberFormatMember::Decimal, "");
    no_text.numbering_level_text = None;
    assert_eq!(labels_of(&single(Vec::from([null])), &[0]), ["\0"]);
    assert_eq!(labels_of(&single(Vec::from([empty_null])), &[0]), ["\0"]);
    assert_eq!(labels_of(&single(Vec::from([no_text])), &[0]), [""]);
}

/// `definition` whose run properties hold the `w:lang` `language`.
fn in_language(
    mut definition: NumberingLevelOverrideDefinition,
    language: &str,
) -> NumberingLevelOverrideDefinition {
    let mut languages = LanguagesForRunContent::default();
    languages.val = Some(language.to_owned());
    let mut properties = RunProperties::default();
    properties.content = Vec::from([RunPropertiesContent::from(languages)]);
    definition.run_properties = Some(Box::new(properties));
    definition
}

#[test]
fn a_level_writes_its_label_in_the_language_of_its_run_properties() {
    let numbering = single(Vec::from([
        in_language(level(0, StNumberFormatMember::CardinalText, "%1."), "es-ES"),
        in_language(
            level(1, StNumberFormatMember::OrdinalText, "%1, %2"),
            "es-ES",
        ),
    ]));
    assert_eq!(
        labels_of(&numbering, &[0, 0, 0, 1, 1]),
        ["Uno.", "Dos.", "Tres.", "Tres, Primero", "Tres, Segundo"]
    );
}

#[test]
fn an_ordinal_text_level_is_decimal_in_welsh_and_latvian() {
    for language in ["cy", "cy-GB", "lv", "lv-LV"] {
        let numbering = single(Vec::from([in_language(
            level(0, StNumberFormatMember::OrdinalText, "%1"),
            language,
        )]));
        assert_eq!(labels_of(&numbering, &[0, 0]), ["1", "2"], "{language}");
    }
}

#[test]
fn a_korean_ordinal_text_level_writes_native_ordinals() {
    let numbering = single(Vec::from([in_language(
        level(0, StNumberFormatMember::OrdinalText, "%1"),
        "ko-KR",
    )]));
    assert_eq!(
        labels_of(&numbering, &[0, 0, 0]),
        ["\u{CCAB}\u{C9F8}", "\u{B458}\u{C9F8}", "\u{C14B}\u{C9F8}"]
    );
}

#[test]
fn a_turkish_level_cases_i_as_turkish_does() {
    let numbering = single(Vec::from([
        in_language(level(0, StNumberFormatMember::CardinalText, "%1."), "tr-TR"),
        in_language(
            starting(unstarted(1, StNumberFormatMember::LowerLetter, "%2)"), 10),
            "tr-TR",
        ),
    ]));
    assert_eq!(
        labels_of(&numbering, &[0, 0, 1, 1]),
        ["Bir.", "\u{130}ki.", "\u{131})", "i)"]
    );
}

#[test]
fn a_level_in_a_tag_with_a_region_writes_in_the_region_s_script() {
    let numbering = single(Vec::from([
        in_language(
            starting(
                unstarted(0, StNumberFormatMember::CardinalText, "%1"),
                10_000,
            ),
            "zh-TW",
        ),
        in_language(level(1, StNumberFormatMember::CardinalText, "%2"), "sr-ME"),
    ]));
    assert_eq!(
        labels_of(&numbering, &[0, 1]),
        ["\u{4E00}\u{842C}", "Jedan"]
    );
}

#[test]
fn a_language_cldr_has_no_rules_for_counts_in_decimal() {
    let numbering = single(Vec::from([in_language(
        level(0, StNumberFormatMember::CardinalText, "%1."),
        "xx",
    )]));
    assert_eq!(labels_of(&numbering, &[0, 0]), ["1.", "2."]);
}

#[test]
fn a_format_with_no_text_counts_in_decimal() {
    let numbering = single(Vec::from([
        level(0, StNumberFormatMember::CardinalText, "%1."),
        level(1, StNumberFormatMember::Ordinal, "%1.%2"),
    ]));
    assert_eq!(labels_of(&numbering, &[0, 1]), ["1.", "1.1"]);
}

#[test]
fn an_earlier_level_writes_its_format_in_the_item_s_instance() {
    let numbering = numbering_of(
        Vec::from([abstract_of(4, outline())]),
        Vec::from([instance(
            6,
            4,
            Vec::from([level_override(
                0,
                Some(level(0, StNumberFormatMember::UpperRoman, "%1.")),
                None,
            )]),
        )]),
        &StyleSheet::default(),
    );
    assert_eq!(labels(&numbering, &[(6, 0), (6, 1)]), ["I.", "I.1"]);
}

#[test]
fn a_level_s_custom_format_writes_its_first_token() {
    let numbering = single(Vec::from([custom(
        level(0, StNumberFormatMember::Custom, "%1."),
        "001, 002, 003, ...",
    )]));
    assert_eq!(labels_of(&numbering, &[0, 0, 0]), ["001.", "002.", "003."]);
}

#[test]
fn a_level_s_custom_format_in_full_width_digits_writes_full_width_labels() {
    let numbering = single(Vec::from([custom(
        level(0, StNumberFormatMember::Custom, "%1."),
        "\u{FF10}\u{FF11}",
    )]));
    assert_eq!(
        labels_of(&numbering, &[0, 0]),
        ["\u{FF10}\u{FF11}.", "\u{FF10}\u{FF12}."]
    );
}

#[test]
fn a_custom_format_wins_over_the_val_and_numbers_the_deeper_levels_labels() {
    let numbering = single(Vec::from([
        custom(level(0, StNumberFormatMember::Decimal, "%1"), "(A)"),
        level(1, StNumberFormatMember::LowerRoman, "%1-%2"),
    ]));
    assert_eq!(labels_of(&numbering, &[0, 1, 1]), ["A", "A-i", "A-ii"]);
}

#[test]
fn an_unknown_custom_format_is_written_in_the_val() {
    let numbering = single(Vec::from([
        custom(level(0, StNumberFormatMember::LowerRoman, "%1"), "?"),
        custom(level(1, StNumberFormatMember::Custom, "%2"), "\u{30A2}"),
    ]));
    assert_eq!(labels_of(&numbering, &[0, 0, 0]), ["i", "ii", "iii"]);
    assert_eq!(labels_of(&numbering, &[0, 1, 1]), ["i", "1", "2"]);
}

#[test]
fn a_legal_level_writes_a_custom_format_in_decimal() {
    let mut legal = level(1, StNumberFormatMember::Decimal, "%1.%2");
    legal.display_all_levels_using_arabic_numerals =
        Some(Box::new(DisplayAllLevelsUsingArabicNumerals::default()));
    let numbering = single(Vec::from([
        custom(level(0, StNumberFormatMember::Custom, "%1"), "01"),
        legal,
    ]));
    assert_eq!(labels_of(&numbering, &[0, 1]), ["01", "1.1"]);
}

#[test]
fn a_custom_format_of_a_level_counted_from_zero_writes_zero_in_the_val() {
    let numbering = single(Vec::from([custom(
        unstarted(0, StNumberFormatMember::DecimalZero, "%1"),
        "001",
    )]));
    assert_eq!(labels_of(&numbering, &[0, 0]), ["0", "001"]);
}
