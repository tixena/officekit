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
    RUN_PROPERTIES_SHAPE, RunProperties, RunPropertiesContent, StyleDefinition, StyleDefinitions,
    StyleTypesMember, write,
};
use officekit_xml::element::{Attribute, Element, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::write::Scope;

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

/// A paragraph after the document defaults, its style's chain and its own properties.
///
/// Table styles and numbering levels are not applied.
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct EffectiveParagraph {
    /// The paragraph's properties.
    pub properties: ParagraphPropertiesPPrGeneral,
    /// The run properties the document defaults and the paragraph's style give its runs.
    pub run_properties: RunProperties,
    /// The id of the paragraph style that was applied, when there is one.
    pub style: Option<String>,
}

/// The styles of a Word file, by id, with the document defaults.
///
/// Table styles and numbering levels are not applied by [`StyleSheet::paragraph`] or
/// [`StyleSheet::run`].
#[derive(Clone, Debug, Default)]
pub struct StyleSheet {
    by_id: BTreeMap<String, StyleDefinition>,
    default_paragraph: Option<String>,
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
    /// names no style takes.
    #[must_use = "this only returns the value"]
    pub fn default_paragraph(&self) -> Option<&str> {
        self.default_paragraph.as_deref()
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
            if sheet.default_paragraph.is_none() && is_default_paragraph(style) {
                sheet.default_paragraph = Some(id.clone());
            }
            sheet.by_id.entry(id).or_insert_with(|| style.clone());
        }
        sheet
    }

    /// The effective properties of a paragraph whose own properties are `properties`: the
    /// document defaults, then the paragraph style's chain from its last ancestor down, then
    /// `properties`. A paragraph that names no style takes the default paragraph style.
    #[must_use = "this only returns the value"]
    pub fn paragraph(&self, properties: Option<&ParagraphProperties>) -> EffectiveParagraph {
        let named = properties
            .and_then(|own| own.referenced_paragraph_style.as_ref())
            .and_then(|style| style.val.as_deref())
            .or(self.default_paragraph.as_deref());
        let chain = self.chain(named);
        let mut scope = Scope::default();

        let mut paragraph = self.paragraph_defaults.as_ref().map_or_else(
            || empty("pPr"),
            |defaults| write::paragraph_properties_p_pr_general(defaults, &mut scope),
        );
        for style in chain.iter().rev() {
            if let Some(general) = style.paragraph_properties_p_pr_general.as_deref() {
                apply(
                    &mut paragraph,
                    &write::paragraph_properties_p_pr_general(general, &mut scope),
                    Some(&PARAGRAPH_PROPERTIES_P_PR_GENERAL_SHAPE),
                );
            }
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

        let mut runs = self
            .run_defaults
            .as_ref()
            .map_or_else(|| empty("rPr"), run_element);
        let levels = levels_of(&chain);
        for style in chain.iter().rev() {
            if let Some(run) = style.run_properties.as_deref() {
                apply(&mut runs, &run_element(run), Some(&RUN_PROPERTIES_SHAPE));
            }
        }
        let defaults = self.run_defaults.as_ref().map(run_element);
        for name in TOGGLES {
            let default = defaults.as_ref().and_then(|found| toggle_of(found, name));
            set_toggle(
                &mut runs,
                name,
                toggle(None, default, level_value(&levels, name), None),
            );
        }

        EffectiveParagraph {
            properties: ParagraphPropertiesPPrGeneral::read(&paragraph),
            run_properties: RunProperties::read(&runs),
            style: chain.first().and_then(|style| style.style_id.clone()),
        }
    }

    /// The effective properties of a run of `paragraph` whose own properties are `properties`:
    /// what the paragraph gives its runs, then the character style's chain from its last
    /// ancestor down, then `properties`, with the toggle properties combined as section 17.7.3
    /// says.
    ///
    /// Table styles and numbering levels are not applied.
    #[must_use = "this only returns the value"]
    pub fn run(
        &self,
        paragraph: &EffectiveParagraph,
        properties: Option<&RunProperties>,
    ) -> RunProperties {
        let own = properties.map(run_element);
        let character = properties
            .map_or(&[][..], |run| run.content.as_slice())
            .iter()
            .find_map(|property| {
                if let RunPropertiesContent::ReferencedCharacterStyle(reference) = property {
                    reference.val.as_deref()
                } else {
                    None
                }
            });
        let paragraph_chain = self.chain(paragraph.style.as_deref());
        let character_chain = self.chain(character);

        let mut merged = run_element(&paragraph.run_properties);
        for style in character_chain.iter().rev() {
            if let Some(run) = style.run_properties.as_deref() {
                apply(&mut merged, &run_element(run), Some(&RUN_PROPERTIES_SHAPE));
            }
        }
        if let Some(found) = &own {
            apply(&mut merged, found, Some(&RUN_PROPERTIES_SHAPE));
        }

        let defaults = self.run_defaults.as_ref().map(run_element);
        let paragraph_levels = levels_of(&paragraph_chain);
        let character_levels = levels_of(&character_chain);
        for name in TOGGLES {
            let direct = own.as_ref().and_then(|found| toggle_of(found, name));
            let default = defaults.as_ref().and_then(|found| toggle_of(found, name));
            set_toggle(
                &mut merged,
                name,
                toggle(
                    direct,
                    default,
                    level_value(&paragraph_levels, name),
                    level_value(&character_levels, name),
                ),
            );
        }
        RunProperties::read(&merged)
    }

    #[must_use = "this only returns the value"]
    pub fn style(&self, id: &str) -> Option<&StyleDefinition> {
        self.by_id.get(id)
    }
}

/// `over` applied on `base`, both elements of `shape`. A property `base` lacks is added at its
/// schema position; one both hold takes `over`'s attributes one by one, and its children are
/// applied the same way.
fn apply(base: &mut Element, over: &Element, shape: Option<&Shape>) {
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

fn empty(local: &str) -> Element {
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

fn is_default_paragraph(style: &StyleDefinition) -> bool {
    let paragraph = style
        .r#type
        .as_ref()
        .is_none_or(|kind| matches!(kind.value(), Ok(StyleTypesMember::Paragraph)));
    let default = style
        .default
        .as_ref()
        .is_some_and(|value| value.value().unwrap_or(false));
    paragraph && default
}

fn is_word(element: &Element, local: &str) -> bool {
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

fn merge_property(held: &mut Element, over: &Element) {
    let shape = if is_word(held, "pBdr") {
        Some(&PARAGRAPH_BORDERS_SHAPE)
    } else if is_word(held, "numPr") {
        Some(&NUMBERING_DEFINITION_INSTANCE_REFERENCE_NUM_PR_SHAPE)
    } else {
        None
    };
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

/// The effective value of a toggle property: the direct one; else true when the document defaults
/// say true; else the paragraph level's value exclusive-or the character level's.
fn toggle(
    direct: Option<bool>,
    default: Option<bool>,
    paragraph: Option<bool>,
    character: Option<bool>,
) -> Option<bool> {
    if direct.is_some() {
        return direct;
    }
    if matches!(default, Some(true)) {
        return Some(true);
    }
    match (paragraph, character) {
        (None, None) => default,
        (left, right) => Some(left.unwrap_or(false) ^ right.unwrap_or(false)),
    }
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
