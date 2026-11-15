//! Unicode CLDR's XML files as a tree, and the parent of a locale (UTS #35, Part 1).

use alloc::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};
use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};

#[cfg(test)]
mod tests;

/// The CLDR release `just inputs` fetches.
pub const VERSION: &str = "49";

/// One element of a CLDR file: its name, attributes, child elements, and its text, CDATA
/// included.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Tree {
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Self>,
    pub name: String,
    pub text: String,
}

impl Tree {
    /// The value of the attribute `name`.
    #[must_use = "this only returns the value"]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// The first child named `name` that [`counts`](Self::counts).
    #[must_use = "this only returns the value"]
    pub fn child(&self, name: &str) -> Option<&Self> {
        self.children
            .iter()
            .find(|child| child.name == name && child.counts())
    }

    /// Whether the element holds a value a resolved locale takes: no `alt` variant, and no draft
    /// status below `contributed`.
    #[must_use = "this only answers the question"]
    pub fn counts(&self) -> bool {
        self.attribute("alt").is_none()
            && !matches!(self.attribute("draft"), Some("provisional" | "unconfirmed"))
    }

    /// The children named `name`.
    pub fn named<'tree>(&'tree self, name: &'tree str) -> impl Iterator<Item = &'tree Self> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The element at `path`, each step a child's name, each step's element one that counts.
    #[must_use = "this only returns the value"]
    pub fn path(&self, path: &[&str]) -> Option<&Self> {
        path.iter().try_fold(self, |at, step| at.child(step))
    }
}

/// The parent of `locale`: the one `parents` names, else `locale` less its last subtag, else
/// `root`; `None` for `root`.
#[must_use = "this only returns the value"]
pub fn parent(locale: &str, parents: &BTreeMap<String, String>) -> Option<String> {
    if locale == "root" {
        return None;
    }
    if let Some(named) = parents.get(locale) {
        return Some(named.clone());
    }
    Some(
        locale
            .rsplit_once('_')
            .map_or_else(|| "root".to_owned(), |(head, _)| head.to_owned()),
    )
}

/// `locale` and its parents up to `root`.
#[must_use = "this only returns the value"]
pub fn chain(locale: &str, parents: &BTreeMap<String, String>) -> Vec<String> {
    let mut found = vec![locale.to_owned()];
    while let Some(next) = found.last().and_then(|last| parent(last, parents)) {
        found.push(next);
    }
    found
}

/// The parent locales of `supplementalData.xml` that every component but those it names
/// separately uses: each locale and the parent it names.
///
/// # Errors
///
/// A file with no `parentLocales` for every component.
pub fn parent_locales(supplemental: &Tree) -> Result<BTreeMap<String, String>> {
    let list = supplemental
        .named("parentLocales")
        .find(|list| list.attribute("component").is_none())
        .context("supplementalData.xml has no parentLocales for every component")?;
    let mut parents = BTreeMap::new();
    for rule in list.named("parentLocale") {
        let parent = rule
            .attribute("parent")
            .context("a parentLocale with no parent")?;
        for locale in rule
            .attribute("locales")
            .unwrap_or_default()
            .split_whitespace()
        {
            parents.insert(locale.to_owned(), parent.to_owned());
        }
    }
    Ok(parents)
}

/// The root element of a CLDR file and everything below it.
///
/// # Errors
///
/// Text that is not well-formed XML, or an entity XML does not predefine.
pub fn read(text: &str) -> Result<Tree> {
    let mut reader = Reader::from_str(text);
    let mut open: Vec<Tree> = Vec::new();
    loop {
        let position = reader.buffer_position();
        let event = reader
            .read_event()
            .with_context(|| format!("malformed XML after byte {position}"))?;
        let closed = match event {
            Event::Start(start) => {
                open.push(element(&start)?);
                None
            }
            Event::Empty(start) => Some(element(&start)?),
            Event::End(_) => Some(open.pop().context("an end tag closes nothing")?),
            Event::Text(content) => {
                if let Some(last) = open.last_mut() {
                    last.text.push_str(&content.xml10_content());
                }
                None
            }
            Event::CData(content) => {
                if let Some(last) = open.last_mut() {
                    last.text.push_str(&content.xml10_content());
                }
                None
            }
            Event::GeneralRef(reference) => {
                if let Some(last) = open.last_mut() {
                    last.text.push(resolve(&reference)?);
                }
                None
            }
            Event::Eof => bail!("the file holds no complete root element"),
            Event::Comment(_) | Event::Decl(_) | Event::DocType(_) | Event::PI(_) => None,
        };
        if let Some(tree) = closed {
            match open.last_mut() {
                Some(parent) => parent.children.push(tree),
                None => return Ok(tree),
            }
        }
    }
}

fn element(start: &BytesStart<'_>) -> Result<Tree> {
    let mut attributes = Vec::new();
    for read in start.attributes() {
        let attribute = read?;
        let value = attribute
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)?
            .into_owned();
        attributes.push((attribute.key.as_ref().to_owned(), value));
    }
    Ok(Tree {
        attributes,
        children: Vec::new(),
        name: start.name().as_ref().to_owned(),
        text: String::new(),
    })
}

fn resolve(reference: &BytesRef<'_>) -> Result<char> {
    if let Some(character) = reference.resolve_char_ref()? {
        return Ok(character);
    }
    match &**reference {
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "gt" => Ok('>'),
        "lt" => Ok('<'),
        "quot" => Ok('"'),
        other => bail!("unknown entity `&{other};`"),
    }
}
