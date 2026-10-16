//! Reading a part's bytes into the generic element tree, over `quick-xml`'s
//! `Reader` (D29).

use core::fmt::Display;
use core::mem;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::element::{Attribute, Document, Element, NamespaceDeclaration, Node, XmlError};
use super::name::{Namespace, QName};

#[cfg(test)]
mod tests;

/// The namespace the `xml` prefix is bound to without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// A stack of the namespace declarations in scope, innermost last.
#[derive(Debug, Default)]
pub(crate) struct Scope {
    frames: Vec<Vec<NamespaceDeclaration>>,
}

/// Where the nodes read so far go: the open elements, and the nodes before and
/// after the root.
#[derive(Default)]
struct Builder {
    epilog: Vec<Node>,
    open: Vec<Element>,
    prolog: Vec<Node>,
    root: Option<Element>,
}

impl Builder {
    /// Close an element: it becomes its parent's last child, or the root.
    fn close(&mut self, element: Element) -> Result<(), XmlError> {
        if let Some(parent) = self.open.last_mut() {
            parent.children.push(Node::Element(element));
            Ok(())
        } else if self.root.is_some() {
            Err(XmlError::Malformed("more than one root element".to_owned()))
        } else {
            self.root = Some(element);
            Ok(())
        }
    }

    /// A node that is not an element goes into the open element, else before
    /// or after the root.
    fn push(&mut self, node: Node) {
        if let Some(parent) = self.open.last_mut() {
            parent.children.push(node);
        } else if self.root.is_some() {
            self.epilog.push(node);
        } else {
            self.prolog.push(node);
        }
    }
}

impl Scope {
    /// Close the innermost element's declarations.
    pub(crate) fn pop(&mut self) {
        self.frames.pop();
    }

    /// Open an element's declarations.
    pub(crate) fn push(&mut self, declarations: &[NamespaceDeclaration]) {
        self.frames.push(declarations.to_vec());
    }

    /// The URI `prefix` stands for, innermost declaration first; `xml` is
    /// bound without one.
    pub(crate) fn uri(&self, prefix: &str) -> Option<&str> {
        if prefix == "xml" {
            return Some(XML_NAMESPACE);
        }
        self.frames
            .iter()
            .rev()
            .flat_map(|frame| frame.iter().rev())
            .find(|declaration| declaration.prefix == prefix)
            .map(|declaration| declaration.uri.as_str())
    }
}

fn malformed<E>(error: E) -> XmlError
where
    E: Display,
{
    XmlError::Malformed(error.to_string())
}

/// Parse a part's bytes. A namespace the table does not know is kept as
/// [`Namespace::Other`].
///
/// # Errors
///
/// [`XmlError::UndeclaredPrefix`] for a prefix with no declaration in scope,
/// [`XmlError::Malformed`] for bytes that are not well-formed XML or that hold
/// a document type declaration.
pub fn parse(bytes: &[u8]) -> Result<Document, XmlError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut scope = Scope::default();
    let mut declaration = None;
    let mut builder = Builder::default();
    let mut text = String::new();
    loop {
        let event = reader.read_event().map_err(malformed)?;
        if !matches!(event, Event::Text(_) | Event::GeneralRef(_)) && !text.is_empty() {
            builder.push(Node::Text(mem::take(&mut text)));
        }
        match event {
            Event::CData(content) => builder.push(Node::CData((*content).to_owned())),
            Event::Comment(content) => builder.push(Node::Comment((*content).to_owned())),
            Event::Decl(content) => declaration = Some((*content).to_owned()),
            Event::DocType(_) => {
                return Err(XmlError::Malformed(
                    "a part has no document type declaration".to_owned(),
                ));
            }
            Event::Empty(start) => {
                let element = start_element(&start, &mut scope)?;
                scope.pop();
                builder.close(element)?;
            }
            Event::End(_) => {
                scope.pop();
                let element = builder.open.pop().ok_or_else(|| {
                    XmlError::Malformed("an end tag with no start tag".to_owned())
                })?;
                builder.close(element)?;
            }
            Event::Eof => break,
            Event::GeneralRef(reference) => text.push(resolve(&reference)?),
            Event::PI(instruction) => builder.push(Node::ProcessingInstruction {
                content: instruction.content().trim_start().to_owned(),
                target: instruction.target().to_owned(),
            }),
            Event::Start(start) => builder.open.push(start_element(&start, &mut scope)?),
            // Outside the root, text is whitespace kept as written, line endings included.
            Event::Text(content) if builder.open.is_empty() => text.push_str(&content),
            Event::Text(content) => text.push_str(&content.xml10_content()),
        }
    }
    if !builder.open.is_empty() {
        return Err(XmlError::Malformed("an element is not closed".to_owned()));
    }
    let root = builder
        .root
        .ok_or_else(|| XmlError::Malformed("no root element".to_owned()))?;
    Ok(Document {
        declaration,
        epilog: builder.epilog,
        prolog: builder.prolog,
        root,
    })
}

/// The character a character or entity reference in text stands for.
fn resolve(reference: &BytesRef<'_>) -> Result<char, XmlError> {
    if let Some(character) = reference.resolve_char_ref().map_err(malformed)? {
        return Ok(character);
    }
    match &**reference {
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "gt" => Ok('>'),
        "lt" => Ok('<'),
        "quot" => Ok('"'),
        other => Err(XmlError::Malformed(format!("unknown entity `&{other};`"))),
    }
}

/// Split a qualified name on its colon into prefix and local name.
pub(crate) fn split_qualified(name: &str) -> (&str, &str) {
    name.split_once(':').unwrap_or(("", name))
}

/// Build an element from a start tag, its declarations pushed onto `scope`;
/// the caller pops them when the element closes.
///
/// # Errors
///
/// [`XmlError::UndeclaredPrefix`] for a prefix with no declaration in scope,
/// [`XmlError::Malformed`] for an attribute that is not well-formed.
pub(crate) fn start_element(
    start: &BytesStart<'_>,
    scope: &mut Scope,
) -> Result<Element, XmlError> {
    let mut namespace_declarations = Vec::new();
    let mut qualified_attributes = Vec::new();
    for read in start.attributes() {
        let attribute = read.map_err(malformed)?;
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .map_err(malformed)?
            .into_owned();
        let key = attribute.key.as_ref();
        if key == "xmlns" {
            namespace_declarations.push(NamespaceDeclaration {
                prefix: String::new(),
                uri: value,
            });
        } else if let Some(prefix) = key.strip_prefix("xmlns:") {
            namespace_declarations.push(NamespaceDeclaration {
                prefix: prefix.to_owned(),
                uri: value,
            });
        } else {
            qualified_attributes.push((key.to_owned(), value));
        }
    }
    scope.push(&namespace_declarations);
    let (prefix, local) = split_qualified(start.name().into_inner());
    let namespace = match scope.uri(prefix) {
        Some(uri) => Namespace::from_uri(uri),
        None if prefix.is_empty() => Namespace::None,
        None => return Err(XmlError::UndeclaredPrefix(prefix.to_owned())),
    };
    let name = QName {
        local: local.to_owned(),
        namespace,
        prefix: prefix.to_owned(),
    };
    let mut attributes = Vec::with_capacity(qualified_attributes.len());
    for (key, value) in qualified_attributes {
        let (attribute_prefix, attribute_local) = split_qualified(&key);
        let attribute_namespace = if attribute_prefix.is_empty() {
            Namespace::None
        } else {
            scope
                .uri(attribute_prefix)
                .map(Namespace::from_uri)
                .ok_or_else(|| XmlError::UndeclaredPrefix(attribute_prefix.to_owned()))?
        };
        attributes.push(Attribute {
            name: QName {
                local: attribute_local.to_owned(),
                namespace: attribute_namespace,
                prefix: attribute_prefix.to_owned(),
            },
            value,
        });
    }
    Ok(Element {
        attributes,
        children: Vec::new(),
        name,
        namespace_declarations,
    })
}
