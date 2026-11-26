//! A part's typed root, and its load from and store into a package.
//!
//! [`PartRoot`] is the trait the typed root of a part implements. [`load`] reads a root from a
//! [`Package`] and [`store`] writes one back in a conformance class. Every generated part module
//! calls them with its own root.

use officekit_markup::{Events, Item, Tag};
#[cfg(feature = "strict")]
use officekit_strict::StrictRules;
use officekit_traversal::{Unchanged, WriteThrough};
use officekit_xml::conformance::ConformanceClass;
#[cfg(not(all(feature = "strict", feature = "transitional")))]
use officekit_xml::element::{Attribute, Node};
use officekit_xml::element::{Element, PartFrame, XmlError};
use officekit_xml::error::WriteError;
use officekit_xml::mc::Processed;
#[cfg(not(all(feature = "strict", feature = "transitional")))]
use officekit_xml::name::NamespaceId;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{DECLARATION, OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::parse::utf8_of;
use officekit_xml::stream::buffer::PartBuffer;
use officekit_xml::stream::class::ClassRules;
#[cfg(not(all(feature = "strict", feature = "transitional")))]
use officekit_xml::stream::class::Site;
use officekit_xml::stream::reader::PartReader;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{WriteOptions, clean_namespace_bytes};

#[cfg(feature = "transitional")]
use crate::transitional::TransitionalRules;

#[cfg(test)]
mod tests;

/// A part's typed root: what a generated part module moves between the
/// package and the typed tree.
///
/// A root names its tag in [`LOCAL_NAME`](Self::LOCAL_NAME), reads itself from a generic element
/// with [`from_tree`](Self::from_tree) or from the part's events with
/// [`from_part`](Self::from_part), writes itself as one in a class with
/// [`to_tree`](Self::to_tree), and writes itself into a part writer in a class with
/// [`write_part`](Self::write_part).
///
/// ```
/// use officekit_xml::packaging::package::{OpenOptions, Package};
/// use officekit_xml::packaging::part_name::PartName;
/// use officekit::parts::part_root::{self, PartRoot};
/// use officekit_xml::conformance::ConformanceClass;
/// use officekit_xml::element::{Element, Node};
/// use officekit_xml::error::WriteError;
/// use officekit_xml::name::QName;
/// use officekit_xml::write::WriteOptions;
///
/// struct Note {
///     text: String,
/// }
///
/// impl PartRoot for Note {
///     const LOCAL_NAME: &'static str = "note";
///
///     fn from_tree(root: &Element) -> Result<Self, String> {
///         let text = root
///             .children
///             .iter()
///             .filter_map(|node| match node {
///                 Node::Text(text) => Some(text.as_str()),
///                 _ => None,
///             })
///             .collect();
///         Ok(Self { text })
///     }
///
///     fn to_tree(&self, _class: ConformanceClass) -> Result<Element, WriteError> {
///         let mut element = Element::new(QName::unprefixed("note", ""));
///         element.children.push(Node::Text(self.text.clone()));
///         Ok(element)
///     }
/// }
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut package = Package::new();
/// let name = PartName::new("/note.xml")?;
/// let note = Note { text: "Dear Sam,".to_owned() };
/// part_root::store(
///     &mut package,
///     &name,
///     "application/xml",
///     &note,
///     ConformanceClass::Transitional,
///     WriteOptions::default(),
/// )?;
///
/// let loaded: Note = part_root::load(&package, &name, OpenOptions::default())?;
/// assert_eq!(loaded.text, "Dear Sam,");
/// # Ok(())
/// # }
/// ```
pub trait PartRoot: Sized {
    /// The root's tag, checked on load.
    const LOCAL_NAME: &'static str;

    /// Build the typed root from the part's root element: its start tag and the events after it,
    /// or the whole element when it is empty. The default reads the element whole and calls
    /// [`from_tree`](Self::from_tree).
    ///
    /// # Errors
    ///
    /// A description of the first element or value the typed root cannot hold.
    fn from_part(root: Item<'_>) -> Result<Self, String> {
        Self::from_tree(&root.into_element())
    }

    /// Build the typed root from the part's tree.
    ///
    /// # Errors
    ///
    /// A description of the first element or value the typed root cannot hold.
    fn from_tree(root: &Element) -> Result<Self, String>;

    /// The part's tree in `class`.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] naming the first element `class` has no
    /// form for.
    fn to_tree(&self, class: ConformanceClass) -> Result<Element, WriteError>;

    /// Write the root into `out` in `class`, as `out`'s layout writes [`to_tree`](Self::to_tree)'s
    /// element with every namespace declared in the class's URI. This default writes that
    /// element; a generated root writes its typed tree straight to `out`, with no tree.
    ///
    /// # Errors
    ///
    /// [`WriteError::NotInClass`] naming the first content `class` has no form for, and
    /// [`WriteError::Io`] when the sink fails.
    fn write_part(
        &self,
        class: ConformanceClass,
        out: &mut PartWriter<'_>,
    ) -> Result<(), WriteError> {
        let mut tree = self.to_tree(class)?;
        tree.set_class(class);
        out.element(&tree).map_err(WriteError::from)
    }
}

/// The rules of a class whose feature is off: the shared writer's nodes, every namespace
/// declared in the class's URI.
#[cfg(not(all(feature = "strict", feature = "transitional")))]
struct SharedRules(ConformanceClass);

#[cfg(not(all(feature = "strict", feature = "transitional")))]
impl ClassRules for SharedRules {
    fn convert(&mut self, mut element: Element, _site: &Site<'_>) -> Result<Vec<Node>, WriteError> {
        element.set_class(self.0);
        Ok(vec![Node::Element(element)])
    }

    fn converts(
        &self,
        _tag: (Option<NamespaceId>, &str),
        _declared: &[(Option<NamespaceId>, &'static str)],
        _unknown: &[Attribute],
    ) -> bool {
        false
    }

    fn open(&mut self, element: &mut Element, _site: &Site<'_>) -> Result<(), WriteError> {
        element.set_class(self.0);
        Ok(())
    }
}

/// Write a typed root into `out` in `class`, straight from its typed tree.
///
/// The class's rules are set on `out` while it is written, then those set before are put back.
/// `conformance` names the attribute a main part's root says its class in. It is what the generated roots' [`PartRoot::write_part`] calls.
///
/// # Errors
///
/// [`WriteError::NotInClass`] naming the first content `class` has no form for, and
/// [`WriteError::Io`] when the sink fails.
pub fn write_through<R>(
    root: &R,
    class: ConformanceClass,
    conformance: Option<Tag>,
    out: &mut PartWriter<'_>,
) -> Result<(), WriteError>
where
    R: WriteThrough,
{
    let rules = class_rules(class, conformance, out.respells());
    let previous = out.set_rules(Some(rules));
    let written = root.write_through(&mut Unchanged::default(), out);
    out.set_rules(previous);
    written.map_err(WriteError::from)
}

/// The rules of `class`; `respell` writes each value in the class's own spelling, which Strict's
/// rules always do.
fn class_rules(
    class: ConformanceClass,
    conformance: Option<Tag>,
    respell: bool,
) -> Box<dyn ClassRules> {
    if class == ConformanceClass::Strict {
        strict_rules(conformance)
    } else {
        transitional_rules(conformance, respell)
    }
}

/// The rules of the Strict class.
#[cfg(feature = "strict")]
fn strict_rules(conformance: Option<Tag>) -> Box<dyn ClassRules> {
    Box::new(StrictRules::new(conformance))
}

/// The rules of the Strict class with its feature off: the shared writer's nodes, every
/// namespace declared in the Strict URI.
#[cfg(not(feature = "strict"))]
fn strict_rules(_conformance: Option<Tag>) -> Box<dyn ClassRules> {
    Box::new(SharedRules(ConformanceClass::Strict))
}

/// The rules of the Transitional class.
#[cfg(feature = "transitional")]
fn transitional_rules(conformance: Option<Tag>, respell: bool) -> Box<dyn ClassRules> {
    Box::new(TransitionalRules::new(conformance, respell))
}

/// The rules of the Transitional class with its feature off: the shared writer's nodes, every
/// namespace declared in the Transitional URI.
#[cfg(not(feature = "transitional"))]
fn transitional_rules(_conformance: Option<Tag>, _respell: bool) -> Box<dyn ClassRules> {
    Box::new(SharedRules(ConformanceClass::Transitional))
}

/// Any part's typed root by part name; the root's tag must be
/// `R::LOCAL_NAME`.
///
/// The part is read forward into the typed root through [`PartRoot::from_part`], markup
/// compatibility applied as `options` says, with no generic tree of the part.
///
/// # Errors
///
/// The errors of [`Package::load_tree`], and
/// [`PackageError::Read`] when the root is another tag or holds what `R`
/// cannot.
pub fn load<R>(package: &Package, name: &PartName, options: OpenOptions) -> Result<R, PackageError>
where
    R: PartRoot,
{
    let xml = |error| PackageError::Xml {
        error,
        part: name.clone(),
    };
    let bytes = package.part_bytes(name)?;
    let text = utf8_of(&bytes).map_err(xml)?;
    let mut reader = PartReader::new(text.as_ref());
    let mut processed = Processed::new(&mut reader, options.markup_compatibility);
    let events = Events::new(&mut processed);
    let Some(root) = events.root() else {
        let error = events
            .finish()
            .err()
            .unwrap_or_else(|| XmlError::Malformed("no root element".to_owned()));
        return Err(xml(error));
    };
    if &*root.element().name.local != R::LOCAL_NAME {
        let reason = format!(
            "the root is <{}>, not <{}>",
            root.element().name.local,
            R::LOCAL_NAME
        );
        drop(root);
        events.finish().map_err(xml)?;
        return Err(PackageError::Read {
            part: name.clone(),
            reason,
        });
    }
    let typed = R::from_part(root);
    events.finish().map_err(xml)?;
    typed.map_err(|reason| PackageError::Read {
        part: name.clone(),
        reason,
    })
}

/// Store a typed root in `class`, every namespace declared with that
/// class's URI; the part's XML declaration, prolog, and epilog are kept
/// when it has them.
///
/// The root is written through [`PartRoot::write_part`] into a buffer in `options`' layout, each
/// value respelled when `options` says, and the namespace cleanup runs on its bytes.
///
/// # Errors
///
/// [`PackageError::Write`] when the root holds content `class` has no
/// form for, and nothing is written; the errors of [`Package::part_frame`] and
/// [`Package::set_part_bytes`].
pub fn store<R>(
    package: &mut Package,
    name: &PartName,
    content_type: &str,
    root: &R,
    class: ConformanceClass,
    options: WriteOptions,
) -> Result<(), PackageError>
where
    R: PartRoot,
{
    let frame = package.part_frame(name);
    let mut buffer = PartBuffer::default();
    let mut out = PartWriter::new(&mut buffer, options.layout);
    out.set_respell(options.respell);
    write_part(
        &mut out,
        frame.as_ref().ok().and_then(Option::as_ref),
        root,
        class,
    )
    .map_err(|error| PackageError::Write {
        error,
        part: name.clone(),
    })?;
    frame?;
    let mut bytes = buffer.into_bytes();
    if options.namespace_cleanup {
        clean_namespace_bytes(&mut bytes);
    }
    package.set_part_bytes(name, content_type, &bytes)
}

/// Write a part into `out`: `frame`'s declaration and prolog, or the declaration Office writes
/// with no frame, then the root in `class`, then `frame`'s epilog.
fn write_part<R>(
    out: &mut PartWriter<'_>,
    frame: Option<&PartFrame>,
    root: &R,
    class: ConformanceClass,
) -> Result<(), WriteError>
where
    R: PartRoot,
{
    if let Some(found) = frame {
        if let Some(declaration) = &found.declaration {
            out.raw(format!("<?{declaration}?>").as_bytes())?;
        }
        for node in &found.prolog {
            out.node(node)?;
        }
    } else {
        out.raw(format!("<?{DECLARATION}?>").as_bytes())?;
        out.text("\r\n")?;
    }
    root.write_part(class, out)?;
    for node in frame.map_or(&[][..], |found| found.epilog.as_slice()) {
        out.node(node)?;
    }
    Ok(())
}
