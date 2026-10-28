#![cfg(feature = "transitional")]

use crate::parts::kinds::WordprocessingKind;
use officekit_markup::Read as _;
use officekit_markup::wordprocessing::{TextRun, write};
use officekit_xml::element::Element;
use officekit_xml::name::{Namespace, QName};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::image::{Image, ImageFormat};
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};
use officekit_xml::write::Scope;

use super::PictureFit;
use crate::docx::Docx;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

const PNG_PREFIX: [u8; 16] = *b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
const PNG_SUFFIX: [u8; 5] = [8, 6, 0, 0, 0];
const WIDE: [u8; 8] = [0, 0, 1, 0x2C, 0, 0, 0, 100];

const EXTENSION_LIST: &str = "<a:extLst><a:ext uri=\"{28A0092B-C50C-407E-A947-70E740481C1C}\"><a14:useLocalDpi xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" val=\"0\"/></a:ext></a:extLst>";

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn wide_png() -> Image {
    Image::from_bytes([&PNG_PREFIX[..], &WIDE, &PNG_SUFFIX].concat()).unwrap()
}

/// A picture 3135822 by 2000000 EMU whose blip names `rId7` through `embed`, an attribute spelling.
fn drawing(embed: &str) -> String {
    format!(
        "<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"3135822\" cy=\"2000000\"/><wp:docPr id=\"1\" name=\"Picture 1\"/><a:graphic><a:graphicData uri=\"{PIC}\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"0\" name=\"p.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip{embed}>{EXTENSION_LIST}</a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"3135822\" cy=\"2000000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"
    )
}

fn named(prefix: &str) -> String {
    format!(" {prefix}:embed=\"rId7\"")
}

/// A package whose main part holds `run` inside a cell-level content control, with an image part
/// named by `rId7`; the root declares the relationships namespace as `prefix` when it is given.
fn docx_with(run: &str, prefix: Option<&str>) -> Docx {
    let mut package = Package::new();
    let main = name("/word/document.xml");
    let declaration = prefix.map_or_else(String::new, |bound| format!(" xmlns:{bound}=\"{R}\""));
    let document = format!(
        "<w:document xmlns:w=\"{W}\"{declaration} xmlns:wp=\"{WP}\" xmlns:a=\"{A}\" xmlns:pic=\"{PIC}\" xmlns:mc=\"{MC}\"><w:body><w:tbl><w:tr><w:sdt><w:sdtContent><w:tc><w:p>{run}</w:p></w:tc></w:sdtContent></w:sdt></w:tr></w:tbl></w:body></w:document>"
    );
    package
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            document.as_bytes(),
        )
        .unwrap();
    package
        .relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    let old = name("/word/media/image1.png");
    package.set_part_bytes(&old, "image/png", b"old").unwrap();
    package
        .add_relationship(
            Some(&main),
            "rId7",
            &Target::Part(old),
            RelationshipType::IMAGE.transitional,
        )
        .unwrap();
    Docx::from_package(package, OpenOptions::default()).unwrap()
}

fn picture_run(element: &Element) -> Option<&Element> {
    if element.name.local == "r" && has_blip(element) {
        return Some(element);
    }
    element.child_elements().find_map(picture_run)
}

fn has_blip(element: &Element) -> bool {
    element.name.local == "blip" || element.child_elements().any(has_blip)
}

fn run_of(docx: &Docx) -> TextRun {
    let tree = docx
        .package()
        .load_tree(docx.main_document_part(), OpenOptions::default())
        .unwrap();
    TextRun::read(picture_run(&tree.root).unwrap())
}

fn written(docx: &Docx, run: &TextRun) -> Element {
    let tree = docx
        .package()
        .load_tree(docx.main_document_part(), OpenOptions::default())
        .unwrap();
    let mut scope = Scope::default();
    scope.start(&tree.root.namespace_declarations);
    write::text_run(run, &mut scope)
}

fn values(root: &Element, element: &str, attribute: &str) -> Vec<String> {
    let mut found = Vec::new();
    collect(root, element, attribute, &mut found);
    found
}

fn collect(root: &Element, element: &str, attribute: &str, found: &mut Vec<String>) {
    if root.name.local == element {
        found.extend(
            root.attributes
                .iter()
                .filter(|a| a.name.local == attribute)
                .map(|a| a.value.clone()),
        );
    }
    for child in root.child_elements() {
        collect(child, element, attribute, found);
    }
}

fn first_named<'element>(element: &'element Element, local: &str) -> Option<&'element Element> {
    if element.name.local == local {
        return Some(element);
    }
    element
        .child_elements()
        .find_map(|child| first_named(child, local))
}

fn related_ids(docx: &Docx) -> Vec<String> {
    docx.package()
        .related(Some(docx.main_document_part()))
        .unwrap()
        .into_iter()
        .map(|related| related.id)
        .collect()
}

fn pagare_run() -> String {
    format!("<w:r>{}</w:r>", drawing(&named("r")))
}

#[test]
fn width_keeps_cx_sets_cy_from_the_pixel_size_and_leaves_the_extension_list_alone() {
    let mut docx = docx_with(&pagare_run(), Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    let replaced = docx
        .set_picture(&main, &mut run, &wide_png(), PictureFit::Width)
        .unwrap();
    assert_eq!(replaced.as_deref(), Some("rId7"));
    let element = written(&docx, &run);
    assert_eq!(values(&element, "extent", "cx"), ["3135822"]);
    assert_eq!(values(&element, "extent", "cy"), ["1045274"]);
    assert_eq!(values(&element, "ext", "cx"), ["3135822"]);
    assert_eq!(values(&element, "ext", "cy"), ["1045274"]);
    assert_eq!(values(&element, "ext", "uri").len(), 1);
    assert_eq!(values(&element, "off", "x"), ["0"]);
    assert_eq!(values(&element, "docPr", "name"), ["Picture 1"]);
}

#[test]
fn the_image_joins_the_folder_of_the_part_and_the_old_part_stays_until_it_is_removed() {
    let mut docx = docx_with(&pagare_run(), Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    docx.set_picture(&main, &mut run, &wide_png(), PictureFit::Width)
        .unwrap();
    let element = written(&docx, &run);
    let embed = values(&element, "blip", "embed");
    let related = docx.package().related(Some(&main)).unwrap();
    let new = related.iter().find(|part| part.id == embed[0]).unwrap();
    assert_ne!(new.id, "rId7");
    assert_eq!(new.content_type.as_deref(), Some("image/png"));
    assert_eq!(new.target, Target::Part(name("/word/media/image2.png")));
    assert!(related.iter().any(|part| part.id == "rId7"));
    docx.package_mut()
        .remove_relationship(Some(&main), "rId7")
        .unwrap();
    assert!(!related_ids(&docx).contains(&"rId7".to_owned()));
    assert!(
        !docx
            .package()
            .part_names()
            .unwrap()
            .contains(&name("/word/media/image1.png"))
    );
}

#[test]
fn frame_changes_no_extent() {
    let mut docx = docx_with(&pagare_run(), Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    docx.set_picture(&main, &mut run, &wide_png(), PictureFit::Frame)
        .unwrap();
    let element = written(&docx, &run);
    assert_eq!(values(&element, "extent", "cy"), ["2000000"]);
    assert_eq!(values(&element, "ext", "cy"), ["2000000"]);
    assert_ne!(values(&element, "blip", "embed"), ["rId7"]);
}

#[test]
fn an_image_without_a_pixel_size_leaves_the_frame() {
    let mut docx = docx_with(&pagare_run(), Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    let emf = Image::with_format(ImageFormat::Emf, b"emf".to_vec());
    docx.set_picture(&main, &mut run, &emf, PictureFit::Width)
        .unwrap();
    let element = written(&docx, &run);
    assert_eq!(values(&element, "extent", "cy"), ["2000000"]);
    assert_eq!(values(&element, "ext", "cy"), ["2000000"]);
    let related = docx.package().related(Some(&main)).unwrap();
    let embed = values(&element, "blip", "embed");
    let new = related.iter().find(|part| part.id == embed[0]).unwrap();
    assert_eq!(new.target, Target::Part(name("/word/media/image1.emf")));
}

#[test]
fn a_run_without_a_drawing_is_a_read_error_and_adds_nothing() {
    let mut docx = docx_with(&pagare_run(), Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = TextRun::read(&Element::new(QName::prefixed("w", "r", W)));
    let before = related_ids(&docx);
    let error = docx
        .set_picture(&main, &mut run, &wide_png(), PictureFit::Width)
        .unwrap_err();
    assert!(matches!(
        error,
        PackageError::Read { part, reason } if part == main && reason == "the run holds no picture"
    ));
    assert_eq!(related_ids(&docx), before);
}

fn alternate_content(embed: &str) -> String {
    format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\">{}</mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r>",
        drawing(embed)
    )
}

#[test]
fn a_picture_inside_markup_compatibility_is_found_and_fitted() {
    let mut docx = docx_with(&alternate_content(&named("rel")), Some("rel"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    let replaced = docx
        .set_picture(&main, &mut run, &wide_png(), PictureFit::Width)
        .unwrap();
    assert_eq!(replaced.as_deref(), Some("rId7"));
    let element = written(&docx, &run);
    assert_eq!(values(&element, "extent", "cy"), ["1045274"]);
    assert_eq!(values(&element, "ext", "cy"), ["1045274"]);
    assert_eq!(values(&element, "Choice", "Requires"), ["wps"]);
    let blip = first_named(&element, "blip").unwrap();
    let embed = blip
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == "embed")
        .unwrap();
    assert_eq!(embed.name.prefix, "rel");
    assert_ne!(embed.value, "rId7");
}

#[test]
fn a_blip_naming_nothing_gets_r_declared_where_no_prefix_is_in_scope() {
    let mut docx = docx_with(&alternate_content(""), None);
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    let replaced = docx
        .set_picture(&main, &mut run, &wide_png(), PictureFit::Frame)
        .unwrap();
    assert_eq!(replaced, None);
    let element = written(&docx, &run);
    let blip = first_named(&element, "blip").unwrap();
    let embed = blip
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == "embed")
        .unwrap();
    assert_eq!(embed.name.prefix, "r");
    assert_eq!(embed.name.namespace, Namespace::from_uri(R));
    assert!(
        blip.namespace_declarations
            .iter()
            .any(|declaration| declaration.prefix == "r" && declaration.uri == R)
    );
}

#[test]
fn only_the_first_picture_of_the_run_changes() {
    let markup = format!(
        "<w:r>{}{}</w:r>",
        drawing(&named("r")),
        drawing(&named("r"))
    );
    let mut docx = docx_with(&markup, Some("r"));
    let main = docx.main_document_part().clone();
    let mut run = run_of(&docx);
    docx.set_picture(&main, &mut run, &wide_png(), PictureFit::Width)
        .unwrap();
    let element = written(&docx, &run);
    let embeds = values(&element, "blip", "embed");
    assert_eq!(embeds.len(), 2);
    assert_ne!(embeds[0], "rId7");
    assert_eq!(embeds[1], "rId7");
    assert_eq!(values(&element, "extent", "cy"), ["1045274", "2000000"]);
}
