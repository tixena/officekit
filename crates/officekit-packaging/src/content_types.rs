//! The package's `[Content_Types].xml` (ECMA-376 Part 2, 10.1.2): a content
//! type for every part, by extension or by name.

use officekit_xml::element::{Attribute, Document, Element, NamespaceDeclaration, Node};
use officekit_xml::name::{Namespace, QName};

use crate::package::DECLARATION;
use crate::part_name::PartName;

#[cfg(test)]
mod tests;

/// The namespace of `[Content_Types].xml`.
pub(crate) const NAMESPACE: &str = "http://schemas.openxmlformats.org/package/2006/content-types";

/// The content type every `xml` part gets when nothing else names its type.
pub(crate) const XML_CONTENT_TYPE: &str = "application/xml";

/// The parsed `[Content_Types].xml`: `Default` entries by extension and `Override` entries by part
/// name, in the order the file has them.
///
/// [`of`](Self::of) gives a part's content type. A package returns its value from
/// [`Package::content_types`](crate::package::Package::content_types), and the entry lists from
/// [`Package::content_type_defaults`](crate::package::Package::content_type_defaults) and
/// [`Package::content_type_overrides`](crate::package::Package::content_type_overrides).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "binary-format",
    derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize)
)]
#[cfg_attr(feature = "binary-format", rkyv(attr(non_exhaustive)))]
pub struct ContentTypes {
    defaults: Vec<(String, String)>,
    overrides: Vec<(PartName, String)>,
}

impl ContentTypes {
    /// The content type of `extension`'s `Default`, compared without ASCII case.
    fn default_of(&self, extension: &str) -> Option<&str> {
        self.defaults
            .iter()
            .find(|(entry, _)| entry.eq_ignore_ascii_case(extension))
            .map(|(_, content_type)| content_type.as_str())
    }

    /// The `Default` entries: extension, as written, and content type.
    pub(crate) fn defaults(&self) -> &[(String, String)] {
        &self.defaults
    }

    /// Add a `Default` for `extension` when it has none; `true` when one was added.
    pub(crate) fn ensure_default(&mut self, extension: &str, content_type: &str) -> bool {
        if self.default_of(extension).is_some() {
            return false;
        }
        self.defaults
            .push((extension.to_owned(), content_type.to_owned()));
        true
    }

    /// The part's `Override`, else the `Default` of its extension.
    #[must_use]
    pub fn of(&self, name: &PartName) -> Option<&str> {
        let folded = name.folded();
        self.overrides
            .iter()
            .find(|(part, _)| part.folded() == folded)
            .map(|(_, content_type)| content_type.as_str())
            .or_else(|| self.default_of(&name.extension()))
    }

    /// The `Override` entries: part name and content type.
    pub(crate) fn overrides(&self) -> &[(PartName, String)] {
        &self.overrides
    }

    /// Read the entries of a parsed `[Content_Types].xml`.
    pub(crate) fn parse(document: &Document) -> Result<Self, String> {
        if document.root.name.local != "Types" {
            return Err(format!(
                "the root is <{}>, not <Types>",
                document.root.name.local
            ));
        }
        let mut types = Self::default();
        for element in document.root.child_elements() {
            let attribute = |local: &str| {
                element
                    .attribute(&Namespace::None, local)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("a <{}> without {local}", element.name.local))
            };
            match element.name.local.as_str() {
                "Default" => types
                    .defaults
                    .push((attribute("Extension")?, attribute("ContentType")?)),
                "Override" => {
                    let part = PartName::new(&attribute("PartName")?)
                        .map_err(|error| error.to_string())?;
                    types.overrides.push((part, attribute("ContentType")?));
                }
                other => return Err(format!("an unexpected <{other}>")),
            }
        }
        Ok(types)
    }

    /// Give `name` the content type `content_type`. A part whose extension has
    /// no `Default` gets one, except `xml`, whose `Default` is
    /// `application/xml` and is added when missing; otherwise the part gets an
    /// `Override` when its content type differs from its extension's `Default`.
    pub(crate) fn register(&mut self, name: &PartName, content_type: &str) {
        self.remove(name);
        let extension = name.extension();
        if extension == "xml" {
            self.ensure_default("xml", XML_CONTENT_TYPE);
        }
        match self.default_of(&extension) {
            Some(default) if default == content_type => {}
            None if !extension.is_empty() => {
                self.defaults.push((extension, content_type.to_owned()));
            }
            Some(_) | None => self.overrides.push((name.clone(), content_type.to_owned())),
        }
    }

    /// Drop `name`'s `Override`; `true` when it had one.
    pub(crate) fn remove(&mut self, name: &PartName) -> bool {
        let folded = name.folded();
        let before = self.overrides.len();
        self.overrides.retain(|(part, _)| part.folded() != folded);
        self.overrides.len() != before
    }

    /// The `[Content_Types].xml` these entries make.
    pub(crate) fn to_document(&self) -> Document {
        let mut root = Element::new(QName::unprefixed("Types", NAMESPACE));
        root.namespace_declarations.push(NamespaceDeclaration::new(
            String::new(),
            NAMESPACE.to_owned(),
        ));
        for (extension, content_type) in &self.defaults {
            root.children.push(entry(
                "Default",
                [("Extension", extension), ("ContentType", content_type)],
            ));
        }
        for (part, content_type) in &self.overrides {
            root.children.push(entry(
                "Override",
                [("PartName", part.as_str()), ("ContentType", content_type)],
            ));
        }
        Document::new(DECLARATION, root)
    }
}

/// A `Default` or `Override` element with its two attributes.
fn entry(local: &str, attributes: [(&str, &str); 2]) -> Node {
    let mut element = Element::new(QName::unprefixed(local, NAMESPACE));
    element.attributes = attributes
        .into_iter()
        .map(|(name, value)| Attribute::new(QName::unprefixed(name, ""), value.to_owned()))
        .collect();
    Node::Element(element)
}
