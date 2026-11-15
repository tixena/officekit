//! What a paragraph or a run of a Word file looks like once its styles are applied (ECMA-376 Part
//! 1, sections 17.7.1 to 17.7.3).

#[cfg(test)]
mod tests;

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr;

use officekit_markup::Read as _;
use officekit_markup::Shape;
use officekit_markup::wordprocessing::{
    NUMBERING_DEFINITION_INSTANCE_REFERENCE_NUM_PR_SHAPE, PARAGRAPH_BORDERS_SHAPE,
    PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE, ParagraphProperties, ParagraphPropertiesPPrGeneral,
    RUN_PROPERTIES_SHAPE, RunProperties, RunPropertiesContent, RunPropertiesForTheParagraphMark,
    SINGLE_TABLE_CELL_MARGINS_SHAPE, StyleDefinition, StyleDefinitions, StyleTypesMember,
    TABLE_BORDERS_SHAPE, TABLE_CELL_BORDERS_SHAPE, TABLE_CELL_MARGIN_EXCEPTIONS_SHAPE,
    TableProperties, write,
};
use officekit_xml::element::{Attribute, Element, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::write::Scope;

use crate::docx::numbering::{self, ListLevel, Numbering};
use crate::docx::table_style::{CellPosition, Kind, TableFormat};

const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The toggle properties of section 17.7.3.
const TOGGLES: [&str; 12] = [
    "b",
    "bCs",
    "caps",
    "emboss",
    "i",
    "iCs",
    "imprint",
    "outline",
    "shadow",
    "smallCaps",
    "strike",
    "vanish",
];

/// The children of `w:pPr`, `w:rPr`, `w:tblPr`, `w:trPr` and `w:tcPr` of the schema type
/// `CT_OnOff` (section 17.17.4), from `CT_PPrBase`, `EG_RPrBase`, `CT_TblPrBase`, `CT_TrPrBase`
/// and `CT_TcPrBase` of the schema in section A.1, sorted.
const ON_OFF: [&str; 44] = [
    "adjustRightInd",
    "autoSpaceDE",
    "autoSpaceDN",
    "b",
    "bCs",
    "bidi",
    "bidiVisual",
    "cantSplit",
    "caps",
    "contextualSpacing",
    "cs",
    "dstrike",
    "emboss",
    "hidden",
    "hideMark",
    "i",
    "iCs",
    "imprint",
    "keepLines",
    "keepNext",
    "kinsoku",
    "mirrorIndents",
    "noProof",
    "noWrap",
    "oMath",
    "outline",
    "overflowPunct",
    "pageBreakBefore",
    "rtl",
    "shadow",
    "smallCaps",
    "snapToGrid",
    "specVanish",
    "strike",
    "suppressAutoHyphens",
    "suppressLineNumbers",
    "suppressOverlap",
    "tblHeader",
    "tcFitText",
    "topLinePunct",
    "vanish",
    "webHidden",
    "widowControl",
    "wordWrap",
];

/// The plain and the theme attribute of each kind of character of `w:rFonts` (section 17.3.2.26).
const FONT_KINDS: [(&str, &str); 4] = [
    ("ascii", "asciiTheme"),
    ("hAnsi", "hAnsiTheme"),
    ("eastAsia", "eastAsiaTheme"),
    ("cs", "cstheme"),
];

/// A paragraph after the document defaults, its style's chain and its own properties.
///
/// [`StyleSheet::numbered_paragraph`] applies numbering levels, and
/// [`StyleSheet::paragraph_in_cell`] a table style.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct EffectiveParagraph {
    /// The paragraph's properties.
    pub properties: ParagraphPropertiesPPrGeneral,
    /// The run properties the document defaults and the paragraph's style give its runs. The
    /// paragraph mark's own properties are not in it; [`StyleSheet::paragraph_mark`] gives them.
    pub run_properties: RunProperties,
    /// The id of the paragraph style that was applied, when there is one.
    pub style: Option<String>,
    /// The run properties the table style gives the paragraph's cell, one for each format that
    /// applies and states any, the last applied first; empty outside a table. Section 17.7.3
    /// reads the table level's value of a toggle property from them.
    pub table_run_properties: Vec<RunProperties>,
}

/// Where a numbering level's `w:pPr` goes among the layers of a paragraph (ECMA-376 Part 1,
/// section 17.7.2).
#[derive(Clone, Copy)]
enum LevelLayer {
    /// Over the paragraph style's chain and under the paragraph's own properties: the paragraph
    /// states the numbering itself.
    OverStyles,
    /// Over the document defaults and under the paragraph style's chain: the numbering comes
    /// from the style.
    UnderStyles,
}

/// The styles of a Word file, by id, with the document defaults.
///
/// [`StyleSheet::numbered_paragraph`] applies numbering levels, and
/// [`StyleSheet::paragraph_in_cell`] a table style.
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    by_id: BTreeMap<String, StyleDefinition>,
    default_paragraph: Option<String>,
    default_table: Option<String>,
    paragraph_defaults: Option<ParagraphPropertiesPPrGeneral>,
    run_defaults: Option<RunProperties>,
}

impl StyleSheet {
    /// The style `id`, then the style it is based on, and so on, until a style is missing or one
    /// comes again.
    #[must_use = "this only returns the value"]
    pub fn chain(&self, id: Option<&str>) -> Vec<&StyleDefinition> {
        let mut chain: Vec<&StyleDefinition> = Vec::new();
        let mut next = id;
        while let Some(style) = next.and_then(|current| self.by_id.get(current))
            && !chain.iter().any(|seen| ptr::eq(*seen, style))
        {
            chain.push(style);
            next = style
                .parent_style_id
                .as_ref()
                .and_then(|parent| parent.val.as_deref());
        }
        chain
    }

    /// The id of the style with `w:default="1"` and `w:type="paragraph"`, the one a paragraph that
    /// names no style takes; of several, the last wins (section 17.7.4.17).
    #[must_use = "this only returns the value"]
    pub fn default_paragraph(&self) -> Option<&str> {
        self.default_paragraph.as_deref()
    }

    /// The effective properties of a list item's label (ECMA-376 Part 1, section 17.9.24): those
    /// of the paragraph's mark, with the run properties of `level` laid over them.
    #[must_use = "this only returns the value"]
    pub fn label(
        &self,
        paragraph: &EffectiveParagraph,
        mark: Option<&RunPropertiesForTheParagraphMark>,
        level: &ListLevel,
    ) -> RunProperties {
        let mut merged = run_element(&self.paragraph_mark(paragraph, mark));
        if let Some(own) = level.definition.run_properties.as_deref() {
            let mut element = run_element(own);
            element.children.retain(
                |node| !matches!(node, Node::Element(child) if is_word(child, "rPrChange")),
            );
            apply(&mut merged, &element, Some(&RUN_PROPERTIES_SHAPE));
            for name in TOGGLES {
                if let Some(on) = toggle_of(&element, name) {
                    set_toggle(&mut merged, name, Some(on));
                }
            }
        }
        RunProperties::read(&merged)
    }

    #[must_use = "this only builds the value"]
    pub fn new(styles: &StyleDefinitions) -> Self {
        let mut sheet = Self::default();
        let defaults = styles
            .document_default_paragraph_and_run_properties
            .as_deref();
        sheet.paragraph_defaults = defaults
            .and_then(|found| found.default_paragraph_properties.as_deref())
            .and_then(|found| found.p_pr_general.as_deref())
            .cloned();
        sheet.run_defaults = defaults
            .and_then(|found| found.default_run_properties.as_deref())
            .and_then(|found| found.run_properties.as_deref())
            .cloned();
        for style in &styles.style_definition {
            let Some(id) = style.style_id.clone() else {
                continue;
            };
            if is_default_paragraph(style) {
                sheet.default_paragraph = Some(id.clone());
            }
            if is_default_table(style) {
                sheet.default_table = Some(id.clone());
            }
            sheet.by_id.entry(id).or_insert_with(|| style.clone());
        }
        sheet
    }

    /// [`StyleSheet::numbered_paragraph`] inside `cell`, or with no numbering level when
    /// `numbering` is `None`.
    fn numbered_over(
        &self,
        numbering: Option<&Numbering>,
        properties: Option<&ParagraphProperties>,
        cell: Option<(&TableFormat, CellPosition)>,
    ) -> (EffectiveParagraph, Option<ListLevel>) {
        let plain = self.paragraph_over(properties, None, cell);
        let Some(level) = numbering.and_then(|found| found.level_of(&plain, properties)) else {
            return (plain, None);
        };
        let layer = if numbering::states_num_id(properties) {
            LevelLayer::OverStyles
        } else {
            LevelLayer::UnderStyles
        };
        let own = level
            .definition
            .paragraph_properties_p_pr_general
            .as_deref();
        let numbered = self.paragraph_over(properties, own.map(|found| (found, layer)), cell);
        (numbered, Some(level))
    }

    /// The effective properties of a paragraph that may be a list item (ECMA-376 Part 1,
    /// sections 17.7.2, 17.9.18 and 17.9.22): those of [`StyleSheet::paragraph`] with the `w:pPr`
    /// of the level it is numbered with laid in, and that level.
    #[must_use = "this only returns the value"]
    pub fn numbered_paragraph(
        &self,
        numbering: &Numbering,
        properties: Option<&ParagraphProperties>,
    ) -> (EffectiveParagraph, Option<ListLevel>) {
        self.numbered_over(Some(numbering), properties, None)
    }

    /// The effective properties of a paragraph whose own properties are `properties`: the
    /// document defaults, then the paragraph style's chain from its last ancestor down, then
    /// `properties`. A paragraph that names no style takes the default paragraph style.
    #[must_use = "this only returns the value"]
    pub fn paragraph(&self, properties: Option<&ParagraphProperties>) -> EffectiveParagraph {
        self.paragraph_over(properties, None, None)
    }

    /// The effective properties of a paragraph inside the cell at `position` of a table
    /// (ECMA-376 Part 1, sections 17.7.2 and 17.7.6): the document defaults, then the table
    /// style's paragraph properties with the conditional formats that apply there, then the
    /// layers of [`StyleSheet::numbered_paragraph`]. `numbering` is `None` for a caller that
    /// applies no numbering level.
    #[must_use = "this only returns the value"]
    pub fn paragraph_in_cell(
        &self,
        table: &TableFormat,
        position: CellPosition,
        numbering: Option<&Numbering>,
        properties: Option<&ParagraphProperties>,
    ) -> (EffectiveParagraph, Option<ListLevel>) {
        self.numbered_over(numbering, properties, Some((table, position)))
    }

    /// The effective run properties of a paragraph's mark (ECMA-376 Part 1, section 17.3.1.29):
    /// what `paragraph` gives its runs, then the chain of the character style `mark` names, then
    /// `mark` without its revision marks, with the toggle properties of section 17.7.3.
    #[must_use = "this only returns the value"]
    pub fn paragraph_mark(
        &self,
        paragraph: &EffectiveParagraph,
        mark: Option<&RunPropertiesForTheParagraphMark>,
    ) -> RunProperties {
        let content = mark.map_or(&[][..], |own| own.run_properties_content.as_slice());
        let own = mark.map(mark_element);
        self.run_over(paragraph, own.as_ref(), character_style(content))
    }

    /// `paragraph`'s properties with a numbering level's `w:pPr`, less its `w:numPr`, laid in
    /// where `level` says, and the table style of `cell` over the document defaults (section
    /// 17.7.2).
    fn paragraph_over(
        &self,
        properties: Option<&ParagraphProperties>,
        level: Option<(&ParagraphPropertiesPPrGeneral, LevelLayer)>,
        cell: Option<(&TableFormat, CellPosition)>,
    ) -> EffectiveParagraph {
        let named = properties
            .and_then(|own| own.referenced_paragraph_style.as_ref())
            .and_then(|style| style.val.as_deref())
            .or(self.default_paragraph.as_deref());
        let chain = self.chain(named);
        let mut scope = Scope::default();
        let (table_paragraph, table_runs) = cell.map_or_else(
            || (Vec::new(), Vec::new()),
            |(table, position)| {
                let formats = table.formats(position);
                (
                    table.layers(&formats, Kind::Paragraph),
                    table.layers(&formats, Kind::Run),
                )
            },
        );

        let mut paragraph = self.paragraph_defaults.as_ref().map_or_else(
            || empty("pPr"),
            |defaults| write::paragraph_properties_p_pr_general(defaults, &mut scope),
        );
        for element in table_paragraph.iter().flatten() {
            apply(
                &mut paragraph,
                element,
                Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
            );
        }
        let numbered = level.map(|(general, layer)| {
            let mut element = write::paragraph_properties_p_pr_general(general, &mut scope);
            element
                .children
                .retain(|node| !matches!(node, Node::Element(child) if is_word(child, "numPr")));
            (element, layer)
        });
        if let Some((element, LevelLayer::UnderStyles)) = &numbered {
            apply(
                &mut paragraph,
                element,
                Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
            );
        }
        for style in chain.iter().rev() {
            if let Some(general) = style.paragraph_properties_p_pr_general.as_deref() {
                apply(
                    &mut paragraph,
                    &write::paragraph_properties_p_pr_general(general, &mut scope),
                    Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
                );
            }
        }
        if let Some((element, LevelLayer::OverStyles)) = &numbered {
            apply(
                &mut paragraph,
                element,
                Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
            );
        }
        if let Some(own) = properties {
            let mut element = write::paragraph_properties(own, &mut scope);
            element.children.retain(|node| {
                !matches!(node, Node::Element(child)
                    if is_word(child, "rPr") || is_word(child, "sectPr") || is_word(child, "pPrChange"))
            });
            apply(
                &mut paragraph,
                &element,
                Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
            );
        }

        let (runs, table_levels) = self.paragraph_runs(&chain, &table_runs);
        EffectiveParagraph {
            properties: ParagraphPropertiesPPrGeneral::read(&paragraph),
            run_properties: RunProperties::read(&runs),
            style: chain.first().and_then(|style| style.style_id.clone()),
            table_run_properties: table_levels.iter().map(RunProperties::read).collect(),
        }
    }

    /// The run properties a paragraph gives its runs: the document defaults, the table style's
    /// `table_runs`, then the paragraph style's `chain` from its last ancestor down, with the
    /// toggle properties of section 17.7.3; and the table level's run properties, one for each
    /// format, the last applied first.
    fn paragraph_runs(
        &self,
        chain: &[&StyleDefinition],
        table_runs: &[Vec<Element>],
    ) -> (Element, Vec<Element>) {
        let mut runs = self
            .run_defaults
            .as_ref()
            .map_or_else(|| empty("rPr"), run_element);
        for element in table_runs.iter().flatten() {
            apply(&mut runs, element, Some(&RUN_PROPERTIES_SHAPE));
        }
        for style in chain.iter().rev() {
            if let Some(run) = style.run_properties.as_deref() {
                apply(&mut runs, &run_element(run), Some(&RUN_PROPERTIES_SHAPE));
            }
        }
        let table_levels: Vec<Element> = table_runs
            .iter()
            .rev()
            .filter_map(|format| format_run(format))
            .collect();
        let levels = levels_of(chain);
        let defaults = self.run_defaults.as_ref().map(run_element);
        for name in TOGGLES {
            let default = defaults.as_ref().and_then(|found| toggle_of(found, name));
            set_toggle(
                &mut runs,
                name,
                toggle(
                    None,
                    default,
                    [
                        level_value(&table_levels, name),
                        level_value(&levels, name),
                        None,
                    ],
                ),
            );
        }
        (runs, table_levels)
    }

    /// The effective properties of a run of `paragraph` whose own properties are `properties`:
    /// what the paragraph gives its runs, then the character style's chain from its last
    /// ancestor down, then `properties`, with the toggle properties combined over the table,
    /// paragraph and character levels as section 17.7.3 says.
    #[must_use = "this only returns the value"]
    pub fn run(
        &self,
        paragraph: &EffectiveParagraph,
        properties: Option<&RunProperties>,
    ) -> RunProperties {
        let content = properties.map_or(&[][..], |run| run.content.as_slice());
        let own = properties.map(run_element);
        self.run_over(paragraph, own.as_ref(), character_style(content))
    }

    /// `own`, a `w:rPr` element, over what `paragraph` gives its runs and the chain of the
    /// character style `character`, with the toggle properties of section 17.7.3.
    fn run_over(
        &self,
        paragraph: &EffectiveParagraph,
        own: Option<&Element>,
        character: Option<&str>,
    ) -> RunProperties {
        let paragraph_chain = self.chain(paragraph.style.as_deref());
        let character_chain = self.chain(character);

        let mut merged = run_element(&paragraph.run_properties);
        for style in character_chain.iter().rev() {
            if let Some(run) = style.run_properties.as_deref() {
                apply(&mut merged, &run_element(run), Some(&RUN_PROPERTIES_SHAPE));
            }
        }
        if let Some(found) = own {
            apply(&mut merged, found, Some(&RUN_PROPERTIES_SHAPE));
        }

        let defaults = self.run_defaults.as_ref().map(run_element);
        let table_levels: Vec<Element> = paragraph
            .table_run_properties
            .iter()
            .map(run_element)
            .collect();
        let paragraph_levels = levels_of(&paragraph_chain);
        let character_levels = levels_of(&character_chain);
        for name in TOGGLES {
            let direct = own.and_then(|found| toggle_of(found, name));
            let default = defaults.as_ref().and_then(|found| toggle_of(found, name));
            set_toggle(
                &mut merged,
                name,
                toggle(
                    direct,
                    default,
                    [
                        level_value(&table_levels, name),
                        level_value(&paragraph_levels, name),
                        level_value(&character_levels, name),
                    ],
                ),
            );
        }
        RunProperties::read(&merged)
    }

    #[must_use = "this only returns the value"]
    pub fn style(&self, id: &str) -> Option<&StyleDefinition> {
        self.by_id.get(id)
    }

    /// The table `properties` belong to, with its style (ECMA-376 Part 1, sections 17.4.62 and
    /// 17.7.6): the one its `w:tblStyle` names, else the default table style.
    #[must_use = "this only returns the value"]
    pub fn table(&self, properties: &TableProperties) -> TableFormat {
        let named = properties
            .referenced_table_style
            .as_ref()
            .and_then(|style| style.val.as_deref())
            .or(self.default_table.as_deref());
        let chain = self.chain(named).into_iter().cloned().collect();
        TableFormat::new(chain, properties)
    }
}

/// `over` applied on `base`, both elements of `shape`. A property `base` lacks is added at its
/// schema position; one both hold takes `over`'s attributes one by one, an on/off property's
/// `w:val` with its absence, and its children are applied the same way.
pub(super) fn apply(base: &mut Element, over: &Element, shape: Option<&Shape>) {
    for child in over.child_elements() {
        if is_word(child, "tabs") {
            merge_tabs(base, child, shape);
            continue;
        }
        let held = base.children.iter_mut().find_map(|node| {
            if let Node::Element(held) = node
                && same_name(held, child)
            {
                Some(held)
            } else {
                None
            }
        });
        if let Some(found) = held {
            merge_property(found, child);
        } else {
            let at = schema_position(base, shape, child);
            base.children.insert(at, Node::Element(child.clone()));
        }
    }
}

/// The id of the character style `content` names with `w:rStyle`.
fn character_style(content: &[RunPropertiesContent]) -> Option<&str> {
    content.iter().find_map(|property| {
        if let RunPropertiesContent::ReferencedCharacterStyle(reference) = property {
            reference.val.as_deref()
        } else {
            None
        }
    })
}

pub(super) fn empty(local: &str) -> Element {
    Element::new(word_name(local))
}

/// Where `child` goes in `parent`: after the last child whose position in `shape` is not later
/// than its own, and at the end when `shape` has no position for it.
fn schema_position(parent: &Element, shape: Option<&Shape>, child: &Element) -> usize {
    let Some(slot) = shape.and_then(|found| found.slot_of(child)) else {
        return parent.children.len();
    };
    parent
        .children
        .iter()
        .rposition(|node| {
            matches!(node, Node::Element(held)
                if shape.and_then(|found| found.slot_of(held)).is_some_and(|held_slot| held_slot <= slot))
        })
        .map_or(0, |last| last + 1)
}

/// One format's run properties, `layers` laid down in order with each toggle property the last
/// layer's that states it; `None` when they state none.
fn format_run(layers: &[Element]) -> Option<Element> {
    let mut merged = empty("rPr");
    for layer in layers {
        apply(&mut merged, layer, Some(&RUN_PROPERTIES_SHAPE));
    }
    for name in TOGGLES {
        let value = layers.iter().rev().find_map(|layer| toggle_of(layer, name));
        set_toggle(&mut merged, name, value);
    }
    let states = merged.child_elements().next().is_some();
    states.then_some(merged)
}

fn is_default(style: &StyleDefinition) -> bool {
    style
        .default
        .as_ref()
        .is_some_and(|value| value.value().unwrap_or(false))
}

fn is_default_paragraph(style: &StyleDefinition) -> bool {
    let paragraph = style
        .r#type
        .as_ref()
        .is_none_or(|kind| matches!(kind.value(), Ok(StyleTypesMember::Paragraph)));
    paragraph && is_default(style)
}

/// Whether `style` may be the default table style: `w:default` on and `w:type="table"`; the last
/// such style is the one (section 17.7.4.17).
fn is_default_table(style: &StyleDefinition) -> bool {
    let table = style
        .r#type
        .as_ref()
        .is_some_and(|kind| matches!(kind.value(), Ok(StyleTypesMember::Table)));
    table && is_default(style)
}

pub(super) fn is_word(element: &Element, local: &str) -> bool {
    element.name.local == local && element.name.namespace == Namespace::from_uri(WORDPROCESSING)
}

/// The value of the first level, from the style upward, that sets the toggle property `name`.
fn level_value(levels: &[Element], name: &str) -> Option<bool> {
    levels.iter().find_map(|level| toggle_of(level, name))
}

fn levels_of(chain: &[&StyleDefinition]) -> Vec<Element> {
    chain
        .iter()
        .filter_map(|style| style.run_properties.as_deref())
        .map(run_element)
        .collect()
}

/// The mark's `w:rPr` with only the children a run's `w:rPr` declares, and no `w:rPrChange`: the
/// revision and conflict marks of `CT_ParaRPr` are no run property.
fn mark_element(mark: &RunPropertiesForTheParagraphMark) -> Element {
    let mut element = write::run_properties_for_the_paragraph_mark(mark, &mut Scope::default());
    element.children.retain(|node| {
        matches!(node, Node::Element(child)
            if RUN_PROPERTIES_SHAPE.slot_of(child).is_some() && !is_word(child, "rPrChange"))
    });
    element
}

fn merge_property(held: &mut Element, over: &Element) {
    let shape = if is_word(held, "pBdr") {
        Some(&PARAGRAPH_BORDERS_SHAPE)
    } else if is_word(held, "numPr") {
        Some(&NUMBERING_DEFINITION_INSTANCE_REFERENCE_NUM_PR_SHAPE)
    } else if is_word(held, "tblBorders") {
        Some(&TABLE_BORDERS_SHAPE)
    } else if is_word(held, "tcBorders") {
        Some(&TABLE_CELL_BORDERS_SHAPE)
    } else if is_word(held, "tblCellMar") {
        Some(&TABLE_CELL_MARGIN_EXCEPTIONS_SHAPE)
    } else if is_word(held, "tcMar") {
        Some(&SINGLE_TABLE_CELL_MARGINS_SHAPE)
    } else {
        None
    };
    if is_word(held, "rFonts") {
        let namespace = Namespace::from_uri(WORDPROCESSING);
        for (plain, theme) in FONT_KINDS {
            if over.attribute(&namespace, plain).is_some()
                || over.attribute(&namespace, theme).is_some()
            {
                held.attributes.retain(|existing| {
                    existing.name.namespace != namespace
                        || (existing.name.local != plain && existing.name.local != theme)
                });
            }
        }
    }
    if ON_OFF.iter().any(|local| is_word(held, local)) {
        // An absent `w:val` of an on/off property means on, so `over` replaces it whole.
        let namespace = Namespace::from_uri(WORDPROCESSING);
        held.attributes.retain(|existing| {
            existing.name.namespace != namespace || existing.name.local != "val"
        });
    }
    for attribute in &over.attributes {
        let found = held.attributes.iter_mut().find(|existing| {
            existing.name.local == attribute.name.local
                && existing.name.namespace == attribute.name.namespace
        });
        match found {
            Some(existing) => existing.value.clone_from(&attribute.value),
            None => held.attributes.push(attribute.clone()),
        }
    }
    apply(held, over, shape);
}

/// The stops of `over` added to the `w:tabs` of `base`; a stop at the position of an earlier one
/// replaces it, and a stop with `w:val="clear"` only removes it.
fn merge_tabs(base: &mut Element, over: &Element, shape: Option<&Shape>) {
    let existing = base
        .children
        .iter()
        .position(|node| matches!(node, Node::Element(held) if is_word(held, "tabs")));
    let at = existing.unwrap_or_else(|| schema_position(base, shape, over));
    if existing.is_none() {
        let mut created = Element::new(over.name.clone());
        created.attributes.clone_from(&over.attributes);
        created
            .namespace_declarations
            .clone_from(&over.namespace_declarations);
        base.children.insert(at, Node::Element(created));
    }
    let Some(Node::Element(held)) = base.children.get_mut(at) else {
        return;
    };
    let namespace = Namespace::from_uri(WORDPROCESSING);
    for stop in over.child_elements().filter(|child| is_word(child, "tab")) {
        let position = stop.attribute(&namespace, "pos");
        held.children.retain(|node| {
            !matches!(node, Node::Element(found)
                if is_word(found, "tab") && found.attribute(&namespace, "pos") == position)
        });
        if stop.attribute(&namespace, "val") != Some("clear") {
            held.children.push(Node::Element(stop.clone()));
        }
    }
    if held.child_elements().next().is_none() {
        base.children.remove(at);
    }
}

fn run_element(properties: &RunProperties) -> Element {
    write::run_properties(properties, &mut Scope::default())
}

fn same_name(left: &Element, right: &Element) -> bool {
    left.name.local == right.name.local && left.name.namespace == right.name.namespace
}

fn set_toggle(element: &mut Element, name: &str, value: Option<bool>) {
    let position = element
        .children
        .iter()
        .position(|node| matches!(node, Node::Element(found) if is_word(found, name)));
    match (position, value) {
        (Some(at), None) => {
            element.children.remove(at);
        }
        (Some(at), Some(on)) => {
            if let Some(slot) = element.children.get_mut(at) {
                *slot = Node::Element(toggle_element(name, on));
            }
        }
        (None, Some(on)) => {
            let added = toggle_element(name, on);
            let at = schema_position(element, Some(&RUN_PROPERTIES_SHAPE), &added);
            element.children.insert(at, Node::Element(added));
        }
        (None, None) => {}
    }
}

/// The effective value of a toggle property (section 17.7.3): the direct one; else true when
/// the document defaults say true; else the table level's value exclusive-or the paragraph
/// level's exclusive-or the character level's.
fn toggle(direct: Option<bool>, default: Option<bool>, levels: [Option<bool>; 3]) -> Option<bool> {
    if direct.is_some() {
        return direct;
    }
    if matches!(default, Some(true)) {
        return Some(true);
    }
    if levels.iter().all(Option::is_none) {
        return default;
    }
    Some(
        levels
            .iter()
            .fold(false, |value, level| value ^ level.unwrap_or(false)),
    )
}

fn toggle_element(name: &str, on: bool) -> Element {
    let mut element = empty(name);
    if !on {
        element.attributes.push(Attribute::new(
            QName::prefixed("w", "val", WORDPROCESSING),
            "0".to_owned(),
        ));
    }
    element
}

/// The value the toggle property `name` has in `properties`: none when it is absent, true when it
/// has no `w:val`.
fn toggle_of(properties: &Element, name: &str) -> Option<bool> {
    let namespace = Namespace::from_uri(WORDPROCESSING);
    properties
        .child_elements()
        .find(|child| is_word(child, name))
        .map(|child| {
            child
                .attribute(&namespace, "val")
                .is_none_or(|value| !matches!(value, "0" | "false" | "off"))
        })
}

fn word_name(local: &str) -> QName {
    QName::prefixed("w", local, WORDPROCESSING)
}
