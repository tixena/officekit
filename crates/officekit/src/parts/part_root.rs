//! A part's typed root, and its load from and store into a package.
//!
//! [`PartRoot`] is the trait the typed root of a part implements. [`load`] reads a root from a
//! [`Package`] and [`store`] writes one back in a conformance class. Every generated part module
//! calls them with its own root.

use officekit_markup::{Events, Item};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::{Document, Element, XmlError};
use officekit_xml::error::WriteError;
use officekit_xml::mc::Processed;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{DECLARATION, OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::parse::utf8_of;
use officekit_xml::stream::reader::PartReader;
use officekit_xml::write::WriteOptions;

#[cfg(test)]
mod tests;

/// A part's typed root: what a generated part module moves between the
/// package and the typed tree.
///
/// A root names its tag in [`LOCAL_NAME`](Self::LOCAL_NAME), reads itself from a generic element
/// with [`from_tree`](Self::from_tree) or from the part's events with
/// [`from_part`](Self::from_part), and writes itself as one in a class with
/// [`to_tree`](Self::to_tree).
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
    if root.element().name.local != R::LOCAL_NAME {
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
/// # Errors
///
/// [`PackageError::Write`] when the root holds content `class` has no
/// form for, and nothing is written; the errors of [`Package::part_frame`] and
/// [`Package::store_tree`].
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
    let mut tree = root.to_tree(class).map_err(|error| PackageError::Write {
        error,
        part: name.clone(),
    })?;
    tree.set_class(class);
    if options.namespace_cleanup {
        tree.clean_namespaces();
    }
    let document = match package.part_frame(name)? {
        Some(frame) => frame.with_root(tree),
        None => Document::new(DECLARATION, tree),
    };
    package.store_tree(
        name,
        content_type,
        &document,
        WriteOptions::new(options.layout, false),
    )
}
