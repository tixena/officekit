//! Putting an image into a part and pointing a picture at it.

#[cfg(test)]
mod tests;

use core::mem::replace;

use officekit_markup::Read as _;
use officekit_markup::drawing::{
    AnyElementContent, Blip, Coordinate, DrawingElementID, Extents, FillModeContent,
    GeometryContent, GraphicObject, GraphicObjectData, Offset, PositiveCoordinate, PresetGeometry,
    PresetShapeTypes, Stretch, XfrmTransform2D,
};
use officekit_markup::picture::{
    NonVisualDrawingProperties, NonVisualPictureProperties, Picture, PictureFill, ShapeProperties,
};
use officekit_markup::wordprocessing::{DrawingMLObject, DrawingObjectContent, TextRun, write};
use officekit_markup::wordprocessing_drawing::{
    DrawingObjectNonVisualProperties, DrawingObjectSize, InlineDrawingMLObject,
};
use officekit_xml::element::{Attribute, Element, NamespaceDeclaration, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::image::Image;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::Scope;

use super::Docx;

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
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

/// Where a picture's image is: a relationship of the part that holds the picture's run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PictureSource<'id> {
    /// An image part of the package, named by `a:blip/@r:embed`.
    Embedded(&'id str),
    /// An image outside the package, named by `a:blip/@r:link`.
    Linked(&'id str),
}

/// A `w:drawing` showing one picture in the line of text, `size` being its width and height in
/// EMUs. `id` is the drawing's `wp:docPr/@id`, which a document holds once.
#[must_use]
pub fn inline_picture(
    source: PictureSource<'_>,
    id: u32,
    name: &str,
    description: Option<&str>,
    size: (u64, u64),
) -> DrawingMLObject {
    let (cx, cy) = (size.0.to_string(), size.1.to_string());
    let mut blip = Blip::default();
    match source {
        PictureSource::Embedded(relationship) => blip.embed = Some(relationship.to_owned()),
        PictureSource::Linked(relationship) => blip.link = Some(relationship.to_owned()),
    }
    let mut fill = PictureFill::default();
    fill.blip = Some(Box::new(blip));
    fill.content = Some(Box::new(FillModeContent::Stretch(Stretch::default())));
    let mut drawing_properties = NonVisualDrawingProperties::default();
    drawing_properties.id = Some(DrawingElementID::from("0".to_owned()));
    drawing_properties.name = Some(name.to_owned());
    let mut non_visual = NonVisualPictureProperties::default();
    non_visual.non_visual_drawing_properties = Box::new(drawing_properties);
    let mut offset = Offset::default();
    offset.x = Some(Coordinate::from("0".to_owned()));
    offset.y = Some(Coordinate::from("0".to_owned()));
    let mut extents = Extents::default();
    extents.cx = Some(PositiveCoordinate::from(cx.clone()));
    extents.cy = Some(PositiveCoordinate::from(cy.clone()));
    let mut transform = XfrmTransform2D::default();
    transform.offset = Some(Box::new(offset));
    transform.extents = Some(Box::new(extents));
    let mut geometry = PresetGeometry::default();
    geometry.prst = Some(PresetShapeTypes::from("rect".to_owned()));
    let mut shape = ShapeProperties::default();
    shape.geometry_content = Some(Box::new(GeometryContent::PresetGeometry(geometry)));
    shape.xfrm_transform2d = Some(Box::new(transform));
    let mut picture = Picture::default();
    picture.fill = Box::new(fill);
    picture.non_visual_picture_properties = Box::new(non_visual);
    picture.shape_properties = Box::new(shape);
    let mut data = GraphicObjectData::default();
    data.content = alloc::vec![AnyElementContent::Picture(picture)];
    data.uri = Some(PICTURE.to_owned());
    let mut graphic = GraphicObject::default();
    graphic.data = Box::new(data);
    let mut frame_size = DrawingObjectSize::default();
    frame_size.cx = Some(PositiveCoordinate::from(cx));
    frame_size.cy = Some(PositiveCoordinate::from(cy));
    let mut properties = DrawingObjectNonVisualProperties::default();
    properties.descr = description.map(str::to_owned);
    properties.id = Some(DrawingElementID::from(id.to_string()));
    properties.name = Some(name.to_owned());
    let mut inline = InlineDrawingMLObject::default();
    inline.drawing_object_non_visual_properties = Box::new(properties);
    inline.drawing_object_size = Box::new(frame_size);
    inline.graphic_object = Box::new(graphic);
    let mut drawing = DrawingMLObject::default();
    drawing.content = alloc::vec![DrawingObjectContent::InlineDrawingMLObject(inline)];
    drawing
}

/// See [`Docx::set_picture`].
///
/// # Errors
///
/// [`PackageError::Read`] when the run holds no picture, and the errors of [`Package::add_image`](officekit_xml::packaging::package::Package::add_image).
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
