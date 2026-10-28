//! `PartReader`: one part's XML read forward as events, and the element it is
//! on loaded whole as the generic tree.

use core::mem;
use std::io::BufRead;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::element::{Element, Node, XmlError};
use crate::parse::{Scope, malformed, resolve, start_element};

#[cfg(test)]
mod tests;

/// One step of a part read forward.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PartEvent {
    /// The XML declaration's text as written, without `<?` and `?>`.
    Declaration(String),
    /// The end of the element the last unmatched `Start` began.
    End,
    /// Text, a comment, CDATA, a processing instruction, or an element with
    /// no content (`<x/>`).
    Node(Node),
    /// An element's start: the element with its attributes and declarations, and no children.
    Start(Element),
}

/// Reads a part's XML forward as events, and loads the element it is on, with
/// everything inside it, as the generic tree.
///
/// It keeps the namespace declarations of the elements started and not ended,
/// so every element's names resolve, and gives the text between two other
/// events as one node, references resolved, as [`parse`] does.
///
/// [`parse`]: crate::parse::parse
pub struct PartReader<R: BufRead> {
    buffer: Vec<u8>,
    depth: usize,
    /// The event read past the end of a text node, given next.
    next: Option<PartEvent>,
    reader: Reader<R>,
    scope: Scope,
    /// The text read since the last other event.
    text: String,
}

impl<R: BufRead> PartReader<R> {
    /// A reader over `source`, before the first event.
    #[must_use]
    pub fn new(source: R) -> Self {
        let mut reader = Reader::from_reader(source);
        reader.config_mut().trim_text(false);
        Self {
            buffer: Vec::new(),
            depth: 0,
            next: None,
            reader,
            scope: Scope::default(),
            text: String::new(),
        }
    }

    /// The element `start` began, with everything up to its end; the reader
    /// moves past the end.
    ///
    /// # Errors
    ///
    /// The errors of [`Self::read_event`], and [`XmlError::Malformed`] when
    /// the part ends inside the element.
    pub fn read_element(&mut self, start: Element) -> Result<Element, XmlError> {
        let mut element = start;
        let mut open: Vec<Element> = Vec::new();
        while let Some(event) = self.read_event()? {
            let node = match event {
                PartEvent::Declaration(_) => continue,
                PartEvent::End => match open.pop() {
                    Some(done) => Node::Element(done),
                    None => return Ok(element),
                },
                PartEvent::Node(node) => node,
                PartEvent::Start(child) => {
                    open.push(child);
                    continue;
                }
            };
            open.last_mut().unwrap_or(&mut element).children.push(node);
        }
        Err(XmlError::Malformed(
            "the part ends inside an element".to_owned(),
        ))
    }

    /// The next event; `None` at the end of the part.
    ///
    /// # Errors
    ///
    /// [`XmlError::UndeclaredPrefix`] for a prefix with no declaration in
    /// scope, [`XmlError::Malformed`] for bytes that are not well-formed XML,
    /// that hold a document type declaration, or that end inside an element.
    pub fn read_event(&mut self) -> Result<Option<PartEvent>, XmlError> {
        if let Some(event) = self.next.take() {
            return Ok(Some(event));
        }
        loop {
            self.buffer.clear();
            let event = match self
                .reader
                .read_event_into(&mut self.buffer)
                .map_err(malformed)?
            {
                Event::CData(content) => PartEvent::Node(Node::CData((*content).to_owned())),
                Event::Comment(content) => PartEvent::Node(Node::Comment((*content).to_owned())),
                Event::Decl(content) => PartEvent::Declaration((*content).to_owned()),
                Event::DocType(_) => {
                    return Err(XmlError::Malformed(
                        "a part has no document type declaration".to_owned(),
                    ));
                }
                Event::Empty(start) => {
                    let element = start_element(&start, &mut self.scope)?;
                    self.scope.pop();
                    PartEvent::Node(Node::Element(element))
                }
                Event::End(_) => {
                    self.scope.pop();
                    self.depth = self.depth.checked_sub(1).ok_or_else(|| {
                        XmlError::Malformed("an end tag with no start tag".to_owned())
                    })?;
                    PartEvent::End
                }
                Event::Eof if self.depth > 0 => {
                    return Err(XmlError::Malformed("an element is not closed".to_owned()));
                }
                Event::Eof => return Ok(self.take_text()),
                Event::GeneralRef(reference) => {
                    self.text.push(resolve(&reference)?);
                    continue;
                }
                Event::PI(instruction) => PartEvent::Node(Node::ProcessingInstruction {
                    content: instruction.content().trim_start().to_owned(),
                    target: instruction.target().to_owned(),
                }),
                Event::Start(start) => {
                    let element = start_element(&start, &mut self.scope)?;
                    self.depth += 1;
                    PartEvent::Start(element)
                }
                // Outside the root, text is whitespace kept as written, line endings included.
                Event::Text(content) if self.depth == 0 => {
                    self.text.push_str(&content);
                    continue;
                }
                Event::Text(content) => {
                    self.text.push_str(&content.xml10_content());
                    continue;
                }
            };
            return Ok(Some(match self.take_text() {
                Some(text) => {
                    self.next = Some(event);
                    text
                }
                None => event,
            }));
        }
    }

    /// The text read since the last other event, as a node, when there is any.
    fn take_text(&mut self) -> Option<PartEvent> {
        (!self.text.is_empty()).then(|| PartEvent::Node(Node::Text(mem::take(&mut self.text))))
    }
}
