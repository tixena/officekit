//! Putting an image into a part and pointing a picture at it.

#[cfg(test)]
mod tests;

use core::mem::replace;

use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::write::Scope;

use crate::error::PackageError;
use crate::image::Image;
use crate::markup::Read as _;
use crate::markup::wordprocessing::{TextRun, write};
use crate::packaging::part_name::PartName;

use super::Docx;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

/// How [`Docx::set_picture`] sizes the picture's frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PictureFit {
    /// The frame stays as drawn.
    Frame,
    /// The frame keeps its width, and its height follows the image's pixel aspect ratio.
    Width,
}

/// See [`Docx::set_picture`].
///
/// # Errors
///
/// [`PackageError::Read`] when the run holds no picture, and the errors of [`Package::add_image`](crate::packaging::package::Package::add_image).
pub fn set_picture(
    docx: &mut Docx,
    part: &PartName,
    run: &mut TextRun,
    image: &Image,
    fit: PictureFit,
) -> Result<Option<String>, PackageError> {
    let declarations = docx
        .package
        .load_tree(part, docx.options)?
        .root
        .namespace_declarations;
    let mut scope = Scope::default();
    scope.start(&declarations);
    let mut element = write::text_run(run, &mut scope);
    let class = docx.class;
    let Some(drawing) = drawing_with_blip(&mut element) else {
        return Err(PackageError::Read {
            part: part.clone(),
            reason: "the run holds no picture".to_owned(),
        });
    };
    let (_, id) = docx.package.add_image(part, image)?;
    let namespace = Namespace::from_uri(RELATIONSHIPS);
    let found = relationships_prefix(drawing, &declarations);
    let declare = found
        .is_none()
        .then(|| NamespaceDeclaration::new("r".to_owned(), namespace.uri(class).to_owned()));
    let prefix = found.unwrap_or_else(|| "r".to_owned());
    let replaced = first_blip(drawing).and_then(|blip| {
        blip.namespace_declarations.extend(declare);
        set_embed(blip, &prefix, id)
    });
    if fit == PictureFit::Width {
        fit_width(drawing, image);
    }
    *run = TextRun::read(&element);
    Ok(replaced)
}

fn is(element: &Element, uri: &str, local: &str) -> bool {
    element.name.local == local && element.name.namespace == Namespace::from_uri(uri)
}

const fn element_mut(node: &mut Node) -> Option<&mut Element> {
    if let Node::Element(element) = node {
        Some(element)
    } else {
        None
    }
}

fn has_blip(element: &Element) -> bool {
    is(element, DRAWING, "blip") || element.child_elements().any(has_blip)
}

fn drawing_with_blip(element: &mut Element) -> Option<&mut Element> {
    if is(element, WORDPROCESSING, "drawing") && has_blip(element) {
        return Some(element);
    }
    element
        .children
        .iter_mut()
        .filter_map(element_mut)
        .find_map(drawing_with_blip)
}

fn first_blip(element: &mut Element) -> Option<&mut Element> {
    if is(element, DRAWING, "blip") {
        return Some(element);
    }
    element
        .children
        .iter_mut()
        .filter_map(element_mut)
        .find_map(first_blip)
}

fn find_mut<'element>(
    element: &'element mut Element,
    uri: &str,
    local: &str,
) -> Option<&'element mut Element> {
    if is(element, uri, local) {
        return Some(element);
    }
    element
        .children
        .iter_mut()
        .filter_map(element_mut)
        .find_map(|child| find_mut(child, uri, local))
}

fn set_embed(blip: &mut Element, prefix: &str, id: String) -> Option<String> {
    let namespace = Namespace::from_uri(RELATIONSHIPS);
    if let Some(attribute) = blip
        .attributes
        .iter_mut()
        .find(|attribute| attribute.name.local == "embed" && attribute.name.namespace == namespace)
    {
        return Some(replace(&mut attribute.value, id));
    }
    blip.attributes.push(Attribute::new(
        QName::prefixed(prefix, "embed", RELATIONSHIPS),
        id,
    ));
    None
}

fn set(element: &mut Element, local: &str, value: &str) {
    if let Some(attribute) = element.attributes.iter_mut().find(|attribute| {
        attribute.name.local == local && attribute.name.namespace == Namespace::None
    }) {
        value.clone_into(&mut attribute.value);
    } else {
        element.attributes.push(Attribute::new(
            QName::unprefixed(local, ""),
            value.to_owned(),
        ));
    }
}

fn fit_width(drawing: &mut Element, image: &Image) {
    let Some(size) = image
        .pixel_size()
        .filter(|size| size.width > 0 && size.height > 0)
    else {
        return;
    };
    let Some(extent) = find_mut(drawing, WORDPROCESSING_DRAWING, "extent") else {
        return;
    };
    let Some(width) = extent
        .attribute(&Namespace::None, "cx")
        .and_then(|cx| cx.parse::<u64>().ok())
    else {
        return;
    };
    let Some(height) = u128::from(width)
        .checked_mul(u128::from(size.height))
        .and_then(|product| product.checked_div(u128::from(size.width)))
    else {
        return;
    };
    let (cx, cy) = (width.to_string(), height.to_string());
    set(extent, "cy", &cy);
    for_each_transform_extent(drawing, &mut |ext| {
        set(ext, "cx", &cx);
        set(ext, "cy", &cy);
    });
}

fn for_each_transform_extent(element: &mut Element, apply: &mut impl FnMut(&mut Element)) {
    let transform = is(element, DRAWING, "xfrm");
    for node in &mut element.children {
        let Node::Element(child) = node else {
            continue;
        };
        if transform && is(child, DRAWING, "ext") {
            apply(child);
        } else {
            for_each_transform_extent(child, apply);
        }
    }
}

fn relationships_prefix(drawing: &Element, root: &[NamespaceDeclaration]) -> Option<String> {
    let namespace = Namespace::from_uri(RELATIONSHIPS);
    prefix_in(drawing, &namespace).or_else(|| {
        root.iter()
            .find(|declaration| {
                !declaration.prefix.is_empty() && Namespace::from_uri(&declaration.uri) == namespace
            })
            .map(|declaration| declaration.prefix.clone())
    })
}

fn prefix_in(element: &Element, namespace: &Namespace) -> Option<String> {
    element
        .attributes
        .iter()
        .find(|attribute| attribute.name.namespace == *namespace)
        .map(|attribute| attribute.name.prefix.clone())
        .or_else(|| {
            element
                .namespace_declarations
                .iter()
                .find(|declaration| {
                    !declaration.prefix.is_empty()
                        && Namespace::from_uri(&declaration.uri) == *namespace
                })
                .map(|declaration| declaration.prefix.clone())
        })
        .or_else(|| {
            element
                .child_elements()
                .find_map(|child| prefix_in(child, namespace))
        })
}
