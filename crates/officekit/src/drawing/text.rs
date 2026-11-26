//! The properties of a level of `DrawingML` text, from any node that holds them, and one set of
//! levels laid over another.
//!
//! ECMA-376 Part 1, sections 21.1.2.2.2, 21.1.2.2.7, 21.1.2.3.9 and 21.1.2.4.12 to 21.1.2.4.21.

#[cfg(test)]
mod tests;

use core::array;

use officekit_markup::drawing::{
    DEFAULT_TEXT_RUN_PROPERTIES_SHAPE, DefaultParagraphStyle, DefaultTextRunProperties,
    EndParagraphRunProperties, ListLevel1TextStyle, ListLevel2TextStyle, ListLevel3TextStyle,
    ListLevel4TextStyle, ListLevel5TextStyle, ListLevel6TextStyle, ListLevel7TextStyle,
    ListLevel8TextStyle, ListLevel9TextStyle, TEXT_PARAGRAPH_PROPERTIES_SHAPE,
    TEXT_RUN_PROPERTIES_SHAPE, TextListStyles, TextParagraphProperties, TextRunProperties, write,
};
use officekit_markup::presentation::{
    NotesTextStyle, PresentationDefaultTextStyle, SlideMasterBodyTextStyle,
    SlideMasterOtherTextStyle, SlideMasterTitleTextStyle,
};
use officekit_markup::{Read, Shape, Tag, Unknown};
use officekit_xml::element::{Element, NamespaceDeclaration, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::write::Scope;

/// Implements [`ParagraphLevel`] for each `CT_TextParagraphProperties` node given with the
/// function that writes it: its element read as an `a:pPr`, under `a:pPr`'s own name.
macro_rules! paragraph_level {
    ($($node:ty => $write:ident),+ $(,)?) => {$(
        impl ParagraphLevel for $node {
            fn paragraph_properties(&self) -> TextParagraphProperties {
                let mut properties =
                    TextParagraphProperties::read(&write::$write(self, &mut scope()));
                properties.unknown = with_name(properties.unknown.take(), None);
                properties
            }
        }
    )+};
}

/// Implements [`RunLevel`] for each `CT_TextCharacterProperties` node given with the function
/// that writes it: its element read as an `a:rPr`, under `a:rPr`'s own name.
macro_rules! run_level {
    ($($node:ty => $write:ident),+ $(,)?) => {$(
        impl RunLevel for $node {
            fn run_properties(&self) -> TextRunProperties {
                let mut properties = TextRunProperties::read(&write::$write(self, &mut scope()));
                properties.unknown = with_name(properties.unknown.take(), None);
                properties
            }
        }
    )+};
}

/// Implements [`StyleList`] for each `CT_TextListStyle` node given (section 21.1.2.4.12).
macro_rules! style_list {
    ($($list:ty),+ $(,)?) => {$(
        impl StyleList for $list {
            fn default_paragraph(&self) -> Option<TextParagraphProperties> {
                paragraph_of(self.default_paragraph_style.as_deref())
            }

            fn level(&self, index: usize) -> Option<TextParagraphProperties> {
                match index {
                    0 => paragraph_of(self.list_level_1_text_style.as_deref()),
                    1 => paragraph_of(self.list_level_2_text_style.as_deref()),
                    2 => paragraph_of(self.list_level_3_text_style.as_deref()),
                    3 => paragraph_of(self.list_level_4_text_style.as_deref()),
                    4 => paragraph_of(self.list_level_5_text_style.as_deref()),
                    5 => paragraph_of(self.list_level_6_text_style.as_deref()),
                    6 => paragraph_of(self.list_level_7_text_style.as_deref()),
                    7 => paragraph_of(self.list_level_8_text_style.as_deref()),
                    8 => paragraph_of(self.list_level_9_text_style.as_deref()),
                    _ => None,
                }
            }
        }
    )+};
}

/// The namespaces of the nodes under `a:pPr` and `a:rPr`: `DrawingML`'s, and the relationships' of
/// `r:id`, `r:embed` and `r:link`.
const NAMESPACES: [(&str, &str); 2] = [
    ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
    (
        "r",
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
    ),
];

/// A node that holds a paragraph's properties: `a:pPr`, `a:defPPr`, `a:lvl1pPr` to `a:lvl9pPr`
/// (sections 21.1.2.2.7, 21.1.2.2.2 and 21.1.2.4.13 to 21.1.2.4.21).
pub trait ParagraphLevel {
    /// The node's properties as an `a:pPr`.
    #[must_use = "this only builds the value"]
    fn paragraph_properties(&self) -> TextParagraphProperties;
}

/// A node that holds a run's properties: `a:rPr`, `a:defRPr`, `a:endParaRPr` (sections 21.1.2.3.9,
/// 21.1.2.3.2 and 21.1.2.2.3).
pub trait RunLevel {
    /// The node's properties as an `a:rPr`.
    #[must_use = "this only builds the value"]
    fn run_properties(&self) -> TextRunProperties;
}

/// A list of styles: `a:lstStyle`, `p:titleStyle`, `p:bodyStyle`, `p:otherStyle`,
/// `p:defaultTextStyle`, `p:notesStyle` (sections 21.1.2.4.12, 19.3.1.49, 19.3.1.5, 19.3.1.35,
/// 19.2.1.8 and 19.3.1.28).
pub trait StyleList {
    /// The list's `a:defPPr`.
    #[must_use = "this only builds the value"]
    fn default_paragraph(&self) -> Option<TextParagraphProperties>;

    /// The list's `a:lvl1pPr` for `index` 0, to its `a:lvl9pPr` for `index` 8; `None` above 8.
    #[must_use = "this only builds the value"]
    fn level(&self, index: usize) -> Option<TextParagraphProperties>;
}

/// The nine levels of text, level 0 to level 8 (section 21.1.2.4.13).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct TextLevels {
    /// The properties of the paragraphs of each level, `lvl="0"` first.
    pub levels: [TextParagraphProperties; 9],
}

impl ParagraphLevel for TextParagraphProperties {
    fn paragraph_properties(&self) -> TextParagraphProperties {
        self.clone()
    }
}

impl RunLevel for TextRunProperties {
    fn run_properties(&self) -> TextRunProperties {
        self.clone()
    }
}

impl TextLevels {
    /// The nine levels of `list`, each laid over the list's `a:defPPr` alone (sections 21.1.2.2.2
    /// and 21.1.2.4.13).
    #[must_use = "this only builds the value"]
    pub fn of<L>(list: &L) -> Self
    where
        L: StyleList + ?Sized,
    {
        let default = list.default_paragraph().unwrap_or_default();
        Self {
            levels: array::from_fn(|index| {
                list.level(index)
                    .map_or_else(|| default.clone(), |level| paragraph_over(&level, &default))
            }),
        }
    }

    /// These levels laid over `base`, level by level, these winning value by value.
    #[must_use = "this only returns the value"]
    pub fn over(&self, base: &Self) -> Self {
        Self {
            levels: array::from_fn(|index| {
                paragraph_over(&self.levels[index], &base.levels[index])
            }),
        }
    }

    /// The properties of a paragraph whose own `a:pPr` is `own`: `own` laid over the level its
    /// `lvl` names, level 0 when it names none of the nine or `own` is `None` (section 21.1.2.2.7).
    #[must_use = "this only returns the value"]
    pub fn paragraph(&self, own: Option<&TextParagraphProperties>) -> TextParagraphProperties {
        let level = own
            .and_then(|properties| properties.lvl.as_ref())
            .and_then(|lvl| lvl.value().ok())
            .and_then(|value| usize::try_from(value).ok())
            .and_then(|index| self.levels.get(index))
            .unwrap_or(&self.levels[0]);
        own.map_or_else(
            || level.clone(),
            |properties| paragraph_over(properties, level),
        )
    }
}

paragraph_level!(
    DefaultParagraphStyle => default_paragraph_style,
    ListLevel1TextStyle => list_level1text_style,
    ListLevel2TextStyle => list_level2text_style,
    ListLevel3TextStyle => list_level3text_style,
    ListLevel4TextStyle => list_level4text_style,
    ListLevel5TextStyle => list_level5text_style,
    ListLevel6TextStyle => list_level6text_style,
    ListLevel7TextStyle => list_level7text_style,
    ListLevel8TextStyle => list_level8text_style,
    ListLevel9TextStyle => list_level9text_style,
);

run_level!(
    DefaultTextRunProperties => default_text_run_properties,
    EndParagraphRunProperties => end_paragraph_run_properties,
);

style_list!(
    TextListStyles,
    SlideMasterTitleTextStyle,
    SlideMasterBodyTextStyle,
    SlideMasterOtherTextStyle,
    PresentationDefaultTextStyle,
    NotesTextStyle,
);

/// `over` laid on `base`, two elements of `shape`, as [`paragraph_over`] says; a child named as
/// `inner`'s element is laid over the one `base` holds by `inner`, not replaced.
fn lay(base: &mut Element, over: &Element, shape: &Shape, inner: Option<&Shape>) {
    for attribute in &over.attributes {
        let held = base.attributes.iter_mut().find(|held| {
            held.name.local == attribute.name.local
                && held.name.namespace == attribute.name.namespace
        });
        match held {
            Some(found) => found.value.clone_from(&attribute.value),
            None => base.attributes.push(attribute.clone()),
        }
    }
    for declaration in &over.namespace_declarations {
        let held = base
            .namespace_declarations
            .iter_mut()
            .find(|held| held.prefix == declaration.prefix);
        match held {
            Some(found) => found.uri.clone_from(&declaration.uri),
            None => base.namespace_declarations.push(declaration.clone()),
        }
    }
    if over.children.iter().any(|node| slot(shape, node).is_none()) {
        base.children.retain(|node| slot(shape, node).is_some());
    }
    for node in &over.children {
        let Some(position) = slot(shape, node) else {
            base.children.push(node.clone());
            continue;
        };
        if let Some(at) = base
            .children
            .iter()
            .position(|kept| slot(shape, kept) == Some(position))
        {
            match (&mut base.children[at], node, inner) {
                (Node::Element(kept), Node::Element(child), Some(run))
                    if named(child, run.name) =>
                {
                    lay(kept, child, run, None);
                }
                (kept, ..) => kept.clone_from(node),
            }
        } else {
            let at = base
                .children
                .iter()
                .position(|kept| slot(shape, kept).is_some_and(|later| later > position))
                .unwrap_or(base.children.len());
            base.children.insert(at, node.clone());
        }
    }
}

/// `over` laid on `base`: both written as their elements by `written`, laid by [`lay`], and read
/// back.
fn laid<T>(
    over: &T,
    base: &T,
    written: fn(&T, &mut Scope) -> Element,
    shape: &Shape,
    inner: Option<&Shape>,
) -> T
where
    T: Read,
{
    let mut bound = scope();
    let mut element = written(base, &mut bound);
    lay(&mut element, &written(over, &mut bound), shape, inner);
    T::read(&element)
}

/// Whether `element` is named `tag`.
fn named(element: &Element, (namespace, local): Tag) -> bool {
    element.name.local == local
        && element.name.namespace == namespace.map_or(Namespace::None, Namespace::Known)
}

/// The properties `node` holds as an `a:pPr`, when there is a node.
fn paragraph_of<N>(node: Option<&N>) -> Option<TextParagraphProperties>
where
    N: ParagraphLevel,
{
    node.map(ParagraphLevel::paragraph_properties)
}

/// `over` laid on `base`, value by value (section 21.1.2.2.7).
///
/// An attribute or a child `over` states replaces `base`'s, a child by its declared position, and
/// `a:defRPr` is laid over `base`'s by [`run_over`]. Undeclared children are `over`'s, if it holds
/// any, else `base`'s.
#[must_use = "this only returns the value"]
pub fn paragraph_over(
    over: &TextParagraphProperties,
    base: &TextParagraphProperties,
) -> TextParagraphProperties {
    let mut properties = laid(
        over,
        base,
        write::text_paragraph_properties,
        &TEXT_PARAGRAPH_PROPERTIES_SHAPE,
        Some(&DEFAULT_TEXT_RUN_PROPERTIES_SHAPE),
    );
    let name = name_of(over.unknown()).or_else(|| name_of(base.unknown()));
    properties.unknown = with_name(properties.unknown.take(), name);
    properties
}

/// `over` laid on `base`, value by value (section 21.1.2.3.2).
///
/// An attribute or a child `over` states replaces `base`'s, a child whole and by its declared
/// position. Undeclared children are `over`'s, if it holds any, else `base`'s.
#[must_use = "this only returns the value"]
pub fn run_over(over: &TextRunProperties, base: &TextRunProperties) -> TextRunProperties {
    let mut properties = laid(
        over,
        base,
        write::text_run_properties,
        &TEXT_RUN_PROPERTIES_SHAPE,
        None,
    );
    let name = name_of(over.unknown()).or_else(|| name_of(base.unknown()));
    properties.unknown = with_name(properties.unknown.take(), name);
    properties
}

/// The name `unknown` keeps the element it was read from under; `None` without a record.
fn name_of(unknown: Option<&Unknown>) -> Option<Box<QName>> {
    unknown?.name.clone()
}

/// `held` named `name`; `None` when that leaves it empty.
fn with_name(held: Option<Box<Unknown>>, name: Option<Box<QName>>) -> Option<Box<Unknown>> {
    let mut unknown = held.unwrap_or_default();
    unknown.name = name;
    (!unknown.is_empty()).then_some(unknown)
}

/// A scope that binds [`NAMESPACES`], so that an element written in it carries only its node's own
/// declarations and reads back as the same node.
fn scope() -> Scope {
    let mut bound = Scope::default();
    bound.start(
        &NAMESPACES
            .map(|(prefix, uri)| NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned())),
    );
    bound
}

/// The position of `shape` that takes `node`; `None` for a node no declared position takes.
fn slot(shape: &Shape, node: &Node) -> Option<usize> {
    if let Node::Element(element) = node {
        shape.slot_of(element)
    } else {
        None
    }
}
