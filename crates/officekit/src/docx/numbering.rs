//! The numbering of a Word file's list items: the level a paragraph is numbered with, its
//! counters and the text of its label (ECMA-376 Part 1, sections 17.9.1 to 17.9.29 and 17.18.59).

mod digits;
mod format;
mod rule_based;
#[cfg(test)]
mod tests;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;

use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, DecimalNumberValue, NumberingDefinitionInstance,
    NumberingDefinitions, NumberingLevelOverrideDefinition, ParagraphProperties,
    RunPropertiesContent, StNumberFormatMember, StyleDefinition,
};

pub use format::{format_custom, format_number, format_number_in};

use crate::docx::effective::{EffectiveParagraph, StyleSheet};

/// The number of levels a numbering definition holds, `w:ilvl` 0 to 8.
const LEVELS: u8 = 9;

/// The counters of the list items of one part, fed in the part's order (ECMA-376 Part 1,
/// sections 17.9.10, 17.9.11, 17.9.25 and 17.9.26).
///
/// The main document has one, and each header and footer part one of its own.
#[derive(Clone, Debug, Default)]
pub struct ListCounters {
    /// The `w:numId` and `w:ilvl` of each level an item has been numbered with.
    begun: BTreeSet<(i64, u8)>,
    /// For each abstract definition, the counter of each level that has one.
    counters: BTreeMap<i64, BTreeMap<u8, i64>>,
}

impl ListCounters {
    /// Counts the next list item, numbered with `level`, and gives the text of its label
    /// (sections 17.9.11 and 17.9.17).
    pub fn next(&mut self, numbering: &Numbering, level: &ListLevel) -> String {
        let first = self.begun.insert((level.num_id, level.ilvl));
        let counters = self.counters.entry(level.abstract_id).or_default();
        let value = match (first, level.start_override) {
            (true, Some(start)) => start,
            _ => counters.get(&level.ilvl).map_or_else(
                || start_of(&level.definition),
                |count| count.saturating_add(1),
            ),
        };
        counters.insert(level.ilvl, value);
        counters.retain(|deeper, _| *deeper <= level.ilvl || !restarts(numbering, level, *deeper));
        label_text(numbering, level, counters)
    }
}

/// One level of a numbering definition instance, after the instance's overrides and the links
/// between a numbering and a style (ECMA-376 Part 1, sections 17.9.5, 17.9.6 and 17.9.8).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ListLevel {
    /// The `w:abstractNumId` of the abstract definition that holds the level: the one the
    /// counters of this level belong to.
    pub abstract_id: i64,
    /// The level's definition: the `w:lvl` of the instance's `w:lvlOverride` when it has one,
    /// else the abstract definition's (sections 17.9.5 and 17.9.6).
    pub definition: NumberingLevelOverrideDefinition,
    /// The level's index, `w:ilvl`, from 0.
    pub ilvl: u8,
    /// The `w:numId` of the instance.
    pub num_id: i64,
    /// The `w:startOverride` of the instance's `w:lvlOverride` for this level (section 17.9.26).
    pub start_override: Option<i64>,
}

/// The numbering definitions of a Word file, by id (ECMA-376 Part 1, section 17.9.16).
#[derive(Clone, Debug, Default)]
pub struct Numbering {
    abstracts: BTreeMap<i64, AbstractNumberingDefinition>,
    instances: BTreeMap<i64, NumberingDefinitionInstance>,
    /// For each style an abstract definition's `w:styleLink` names, that definition's id.
    style_links: BTreeMap<String, i64>,
    /// For each style an abstract definition's `w:numStyleLink` names, the `w:numId` of the
    /// style's `w:numPr`.
    style_nums: BTreeMap<String, i64>,
}

impl Numbering {
    /// The definition of level `ilvl` of the instance `num_id`, the id of the abstract definition
    /// that holds it, and the instance's `w:startOverride` for it (sections 17.9.5 and 17.9.8).
    fn found(
        &self,
        num_id: i64,
        ilvl: u8,
    ) -> Option<(i64, &NumberingLevelOverrideDefinition, Option<i64>)> {
        if num_id == 0 {
            return None;
        }
        let instance = self.instances.get(&num_id)?;
        let named = decimal(
            instance
                .abstract_numbering_definition_reference
                .val
                .as_ref(),
        )?;
        let (abstract_id, held) = self.underlying(named)?;
        let wanted = Some(i64::from(ilvl));
        let level_override = instance
            .numbering_level_definition_override
            .iter()
            .find(|item| decimal(item.ilvl.as_ref()) == wanted);
        let definition = level_override
            .and_then(|item| item.numbering_level_override_definition.as_deref())
            .or_else(|| {
                held.numbering_level_override_definition
                    .iter()
                    .find(|level| decimal(level.ilvl.as_ref()) == wanted)
            })?;
        let start_override = level_override
            .and_then(|item| item.numbering_level_starting_value_override.as_deref())
            .and_then(|start| decimal(start.val.as_ref()));
        Some((abstract_id, definition, start_override))
    }

    /// Level `ilvl` of the instance `num_id` (ECMA-376 Part 1, section 17.9.15); `None` for
    /// `num_id` 0, an instance the part does not hold, or a level the definition does not hold.
    #[must_use = "this only returns the value"]
    pub fn level(&self, num_id: i64, ilvl: u8) -> Option<ListLevel> {
        let (abstract_id, definition, start_override) = self.found(num_id, ilvl)?;
        Some(ListLevel {
            abstract_id,
            definition: definition.clone(),
            ilvl,
            num_id,
            start_override,
        })
    }

    /// The level `paragraph` is numbered with, `own` being its own properties (ECMA-376 Part 1,
    /// sections 17.9.3, 17.9.18 and 17.9.23): `None` when its effective properties hold no
    /// `w:numPr` or a `w:numId` of 0.
    #[must_use = "this only returns the value"]
    pub fn level_of(
        &self,
        paragraph: &EffectiveParagraph,
        own: Option<&ParagraphProperties>,
    ) -> Option<ListLevel> {
        let numbering = paragraph
            .properties
            .numbering_definition_instance_reference_num_pr
            .as_deref()?;
        let num_id = decimal(
            numbering
                .numbering_definition_instance_reference_num_id
                .as_deref()?
                .val
                .as_ref(),
        )?;
        let by_style = paragraph
            .style
            .as_deref()
            .filter(|_| !states_num_id(own))
            .and_then(|style| {
                (0..LEVELS)
                    .filter_map(|ilvl| self.level(num_id, ilvl))
                    .find(|level| names_style(&level.definition, style))
            });
        by_style.or_else(|| {
            let stated = numbering
                .numbering_level_reference
                .as_deref()
                .and_then(|level| decimal(level.val.as_ref()))
                .unwrap_or(0);
            self.level(num_id, u8::try_from(stated).ok()?)
        })
    }

    /// `styles` gives the numbering styles that `w:numStyleLink` names.
    #[must_use = "this only builds the value"]
    pub fn new(numbering: &NumberingDefinitions, styles: &StyleSheet) -> Self {
        let mut built = Self::default();
        for definition in &numbering.abstract_numbering_definition {
            let Some(id) = decimal(definition.abstract_num_id.as_ref()) else {
                continue;
            };
            if let Some(linked) = definition
                .numbering_style_definition
                .as_deref()
                .and_then(|link| link.val.clone())
            {
                built.style_links.entry(linked).or_insert(id);
            }
            if let Some(named) = definition
                .numbering_style_reference
                .as_deref()
                .and_then(|link| link.val.as_deref())
                && let Some(num_id) = styles.style(named).and_then(style_num_id)
            {
                built.style_nums.entry(named.to_owned()).or_insert(num_id);
            }
            built
                .abstracts
                .entry(id)
                .or_insert_with(|| definition.clone());
        }
        for instance in &numbering.numbering_definition_instance {
            if let Some(id) = decimal(instance.num_id.as_ref()) {
                built
                    .instances
                    .entry(id)
                    .or_insert_with(|| instance.clone());
            }
        }
        built
    }

    /// The abstract definition `abstract_id` stands for, with its id: through the numbering
    /// style a `w:numStyleLink` names, the `w:num` of its `w:numPr` (section 17.9.21), else the
    /// definition whose `w:styleLink` names it (section 17.9.27). Each definition is left once.
    fn underlying(&self, abstract_id: i64) -> Option<(i64, &AbstractNumberingDefinition)> {
        let mut seen = BTreeSet::new();
        let mut current = abstract_id;
        loop {
            let held = self.abstracts.get(&current)?;
            seen.insert(current);
            let next = held
                .numbering_style_reference
                .as_deref()
                .and_then(|link| link.val.as_deref())
                .and_then(|style| {
                    let through_style = self
                        .style_nums
                        .get(style)
                        .and_then(|num_id| self.instances.get(num_id))
                        .and_then(|instance| {
                            decimal(
                                instance
                                    .abstract_numbering_definition_reference
                                    .val
                                    .as_ref(),
                            )
                        });
                    [through_style, self.style_links.get(style).copied()]
                        .into_iter()
                        .flatten()
                        .find(|id| !seen.contains(id) && self.abstracts.contains_key(id))
                });
            match next {
                Some(id) => current = id,
                None => return Some((current, held)),
            }
        }
    }
}

/// Whether the paragraph's own `w:numPr` holds a `w:numId`.
pub(super) fn states_num_id(own: Option<&ParagraphProperties>) -> bool {
    own.and_then(|found| {
        found
            .numbering_definition_instance_reference_num_pr
            .as_deref()
    })
    .is_some_and(|numbering| {
        numbering
            .numbering_definition_instance_reference_num_id
            .is_some()
    })
}

/// The first format token of a level's `w:format`: its first run of letters and digits (section
/// 17.9.17; XSLT 1.0, section 7.7.1).
fn custom_token(definition: &NumberingLevelOverrideDefinition) -> Option<&str> {
    definition
        .numbering_format
        .as_deref()?
        .format
        .as_deref()?
        .split(|character: char| !character.is_alphanumeric())
        .find(|token| !token.is_empty())
}

fn decimal(value: Option<&DecimalNumberValue>) -> Option<i64> {
    value?.value().ok()
}

/// A level's number format, `decimal` when it states none (section 17.9.17).
fn format_of(definition: &NumberingLevelOverrideDefinition) -> StNumberFormatMember {
    definition
        .numbering_format
        .as_deref()
        .and_then(|format| format.val.as_ref())
        .and_then(|value| value.value().ok())
        .unwrap_or(StNumberFormatMember::Decimal)
}

/// The `w:val` of the `w:lang` of a level's run properties, the language of its label (sections
/// 17.3.2.20 and 17.9.24).
fn language_of(definition: &NumberingLevelOverrideDefinition) -> Option<&str> {
    definition
        .run_properties
        .as_deref()?
        .content
        .iter()
        .find_map(|item| {
            if let RunPropertiesContent::LanguagesForRunContent(languages) = item {
                languages.val.as_deref()
            } else {
                None
            }
        })
}

/// The text of a label of `item`: its level's `w:lvlText` with `%1` to `%9` replaced by the
/// counters (sections 17.9.4, 17.9.11 and 17.9.17).
fn label_text(numbering: &Numbering, item: &ListLevel, counters: &BTreeMap<u8, i64>) -> String {
    let definition = &item.definition;
    let level_text = definition.numbering_level_text.as_deref();
    let text = level_text
        .and_then(|found| found.val.as_deref())
        .unwrap_or_default();
    if text.is_empty() {
        let null = level_text
            .and_then(|found| found.null.as_ref())
            .is_some_and(on);
        return if null { "\0".to_owned() } else { String::new() };
    }
    if matches!(format_of(definition), StNumberFormatMember::Bullet) {
        return text.to_owned();
    }
    let legal = definition
        .display_all_levels_using_arabic_numerals
        .as_deref()
        .is_some_and(|found| found.val.as_ref().is_none_or(on));
    let mut label = String::new();
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        let place = characters
            .peek()
            .and_then(|next| next.to_digit(10))
            .and_then(|found| u8::try_from(found).ok())
            .filter(|found| (1..=LEVELS).contains(found));
        match (character, place) {
            ('%', Some(found)) => {
                characters.next();
                label.push_str(&number(numbering, item, counters, found - 1, legal));
            }
            _ => label.push(character),
        }
    }
    label
}

/// Whether `definition` holds a `w:pStyle` naming `style` (section 17.9.23).
fn names_style(definition: &NumberingLevelOverrideDefinition, style: &str) -> bool {
    definition
        .referenced_paragraph_style
        .as_deref()
        .and_then(|found| found.val.as_deref())
        == Some(style)
}

/// The text of level `at`'s number in a label of `item`, in the language of the item's level:
/// nothing for a level deeper than the item's; for a level no item has counted, its `w:start`
/// (section 17.9.11).
fn number(
    numbering: &Numbering,
    item: &ListLevel,
    counters: &BTreeMap<u8, i64>,
    at: u8,
    legal: bool,
) -> String {
    if at > item.ilvl {
        return String::new();
    }
    let definition = if at == item.ilvl {
        Some(&item.definition)
    } else {
        numbering.found(item.num_id, at).map(|(_, found, _)| found)
    };
    let value = counters
        .get(&at)
        .copied()
        .unwrap_or_else(|| definition.map_or(0, start_of));
    if !legal
        && let Some(text) = definition
            .and_then(custom_token)
            .and_then(|token| format_custom(value, token))
    {
        return text;
    }
    let format = if legal {
        StNumberFormatMember::Decimal
    } else {
        definition.map_or(StNumberFormatMember::Decimal, format_of)
    };
    language_of(&item.definition)
        .map_or_else(
            || format_number(value, format),
            |language| format_number_in(value, format, language),
        )
        .unwrap_or_else(|| value.to_string())
}

fn on(value: &OnOffValue) -> bool {
    value.value().unwrap_or(true)
}

/// Whether the counter of level `deeper` restarts after an item of `item`'s level, by its
/// `w:lvlRestart` (section 17.9.10).
fn restarts(numbering: &Numbering, item: &ListLevel, deeper: u8) -> bool {
    let restart = numbering
        .found(item.num_id, deeper)
        .and_then(|(_, definition, _)| definition.restart_numbering_level_symbol.as_deref())
        .and_then(|symbol| decimal(symbol.val.as_ref()));
    match restart {
        Some(0) => false,
        Some(after) if (1..=i64::from(deeper)).contains(&after) => i64::from(item.ilvl) < after,
        _ => true,
    }
}

/// A level's starting value, 0 when it states none (section 17.9.25).
fn start_of(definition: &NumberingLevelOverrideDefinition) -> i64 {
    decimal(
        definition
            .starting_value
            .as_deref()
            .and_then(|start| start.val.as_ref()),
    )
    .unwrap_or(0)
}

/// The `w:numId` of a numbering style's own `w:numPr`.
fn style_num_id(style: &StyleDefinition) -> Option<i64> {
    decimal(
        style
            .paragraph_properties_p_pr_general
            .as_deref()?
            .numbering_definition_instance_reference_num_pr
            .as_deref()?
            .numbering_definition_instance_reference_num_id
            .as_deref()?
            .val
            .as_ref(),
    )
}
