use core::slice;
use std::io::{self, Write};

use super::PackageWriter;
use crate::markup::wordprocessing::{self, TableRow, Text, read};
use crate::markup::{self, Descend, Visit as _};
use crate::packaging::package::{OpenOptions, Package};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{
    RELATIONSHIPS_CONTENT_TYPE, Relationship, Target, TargetMode,
};
use crate::parts::{main_document, style_definitions};
use crate::stream::reader::{PartEvent, PartReader};
use crate::stream::transform::{WriteThrough as _, Written};
use crate::stream::writer::PartWriter;
use crate::xml::element::{Element, NamespaceDeclaration, Node};
use crate::xml::name::Namespace;
use crate::xml::write::Layout;

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n\
                      <w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Gives every text `value`.
struct Fill {
    value: String,
}

/// Finds the text `placeholder`.
struct FindPlaceholder {
    found: bool,
}

/// Writes the row holding `placeholder` once per item, with `item 000000` and
/// on in its place.
struct RepeatRows {
    count: usize,
}

impl markup::Context for Fill {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for Fill {
    fn text(&mut self, node: &Text) -> Option<Text> {
        Some(Text {
            value: self.value.clone(),
            ..node.clone()
        })
    }
}

impl markup::Visitor for FindPlaceholder {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl wordprocessing::Visitor for FindPlaceholder {
    fn text(&mut self, node: &Text) -> Descend {
        self.found |= node.value == "placeholder";
        Descend::Yes
    }
}

impl markup::Context for RepeatRows {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for RepeatRows {
    fn write_table_row(
        &mut self,
        node: &TableRow,
        out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        let mut find = FindPlaceholder { found: false };
        node.visit(&mut find);
        if !find.found {
            return Ok(Written::No);
        }
        for index in 0..self.count {
            node.write_through(
                &mut Fill {
                    value: format!("item {index:06}"),
                },
                out,
            )?;
        }
        Ok(Written::Yes)
    }
}

/// Section 3.12's generation into `sink`: the template's other parts copied,
/// its main part read forward, the table loaded as a typed node and written
/// through `RepeatRows`, everything else written as read.
fn generate<W>(template: &Package, count: usize, sink: W) -> W
where
    W: Write,
{
    let main = main();
    let mut out = PackageWriter::from_template(sink, template);
    out.copy_parts(template, slice::from_ref(&main)).unwrap();
    let mut part = out
        .part(&main, main_document::CONTENT_TYPES[0], Layout::Compact)
        .unwrap();
    part.declaration().unwrap();
    let mut reader = PartReader::from_part(template, &main).unwrap();
    while let Some(event) = reader.read_event().unwrap() {
        match event {
            PartEvent::Start(start) if is_table(&start) => {
                let table = read::table(&reader.read_element(start).unwrap());
                table
                    .write_through(&mut RepeatRows { count }, &mut part)
                    .unwrap();
            }
            PartEvent::End | PartEvent::Node(_) | PartEvent::Start(_) => {
                part.write_event(event).unwrap();
            }
        }
    }
    out.finish().unwrap()
}

fn is_table(element: &Element) -> bool {
    element.name.local == "tbl" && element.name.namespace == Namespace::from_uri(W)
}

fn main() -> PartName {
    PartName::new("/word/document.xml").unwrap()
}

fn styles() -> PartName {
    PartName::new("/word/styles.xml").unwrap()
}

/// A document of a paragraph, a table with a header row and a `placeholder`
/// row, a paragraph, and the section's properties, and a styles part the
/// document relates to.
fn template() -> Package {
    let cell = |text: &str| format!("<w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>");
    let document = format!(
        "{DECLARATION}<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p>\
         <w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid>\
         <w:tr>{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p><w:sectPr/></w:body></w:document>",
        cell("Name"),
        cell("placeholder")
    );
    let mut package = Package::new();
    package
        .set_part_bytes(
            &main(),
            main_document::CONTENT_TYPES[0],
            document.as_bytes(),
        )
        .unwrap();
    package
        .relate(None, &main(), main_document::RELATIONSHIP_TYPE.transitional)
        .unwrap();
    package
        .set_part_bytes(
            &styles(),
            style_definitions::CONTENT_TYPE,
            STYLES.as_bytes(),
        )
        .unwrap();
    package
        .relate(
            Some(&main()),
            &styles(),
            style_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .unwrap();
    package
}

/// The text of an element's subtree, in order.
fn text_of(element: &Element) -> String {
    let mut text = String::new();
    for child in &element.children {
        if let Node::Element(inner) = child {
            text.push_str(&text_of(inner));
        }
        if let Node::Text(value) = child {
            text.push_str(value);
        }
    }
    text
}

#[test]
fn a_part_written_in_each_layout_gives_its_bytes() {
    let template = template();
    let name = PartName::new("/word/note.xml").unwrap();
    let declarations = [NamespaceDeclaration {
        prefix: "x".to_owned(),
        uri: "urn:x".to_owned(),
    }];
    for (layout, expected) in [
        (
            Layout::Compact,
            format!(
                "{DECLARATION}<x:a xmlns:x=\"urn:x\" x:b=\"1\"><x:c>d &lt; e</x:c><x:f/></x:a>"
            ),
        ),
        (
            Layout::Pretty,
            format!(
                "{DECLARATION}<x:a xmlns:x=\"urn:x\" x:b=\"1\">\n  <x:c>d &lt; e</x:c>\n  <x:f/>\n</x:a>"
            ),
        ),
    ] {
        let mut out = PackageWriter::from_template(Vec::new(), &template);
        out.copy_parts(&template, &[]).unwrap();
        let mut part = out.part(&name, "application/vnd.test+xml", layout).unwrap();
        part.declaration().unwrap();
        part.start("x:a", &declarations, &[("x:b", "1")]).unwrap();
        part.start("x:c", &[], &[]).unwrap();
        part.text("d < e").unwrap();
        part.end("x:c").unwrap();
        part.empty("x:f", &[]).unwrap();
        part.end("x:a").unwrap();
        let id = out.relate(
            Some(&main()),
            Relationship::new("urn:test", "note.xml", TargetMode::Internal),
        );
        assert_eq!(id, "rId2");
        let package = Package::from_bytes(out.finish().unwrap()).unwrap();
        assert_eq!(
            String::from_utf8(package.part_bytes(&name).unwrap()).unwrap(),
            expected
        );
        assert_eq!(
            package.content_type(&name),
            Some("application/vnd.test+xml")
        );
        let targets: Vec<(String, Target)> = package
            .related(Some(&main()))
            .unwrap()
            .into_iter()
            .map(|related| (related.id, related.target))
            .collect();
        assert_eq!(
            targets,
            [
                ("rId1".to_owned(), Target::Part(styles())),
                ("rId2".to_owned(), Target::Part(name.clone()))
            ]
        );
    }
}

#[test]
fn a_relationship_added_to_a_template_with_none_gets_the_relationship_content_type() {
    let mut out = PackageWriter::from_template(Vec::new(), &Package::new());
    let relationship = Relationship::new("urn:test", "urn:x", TargetMode::External);
    assert_eq!(out.relate(None, relationship), "rId1");
    let package = Package::from_bytes(out.finish().unwrap()).unwrap();
    assert_eq!(
        package.content_type_defaults(),
        [("rels".to_owned(), RELATIONSHIPS_CONTENT_TYPE.to_owned())]
    );
    assert_eq!(
        package.related(None).unwrap()[0].target,
        Target::External("urn:x".to_owned())
    );
}

#[test]
fn a_streamed_package_of_1_000_rows_has_data_descriptors_and_opens_with_every_row() {
    let template = template();
    let bytes = generate(&template, 1_000, Vec::new());
    assert_eq!(&bytes[..4], b"PK\x03\x04");
    // The first local header's general-purpose flags: bit 3 says the sizes follow the entry, in a data descriptor.
    assert_ne!(bytes[6] & 0x08, 0);
    let package = Package::from_bytes(bytes).unwrap();
    assert_eq!(package.part_bytes(&styles()).unwrap(), STYLES.as_bytes());
    assert_eq!(
        package.content_type(&main()),
        Some(main_document::CONTENT_TYPES[0])
    );
    let document = package.load_tree(&main(), OpenOptions::default()).unwrap();
    let body = document.root.child_elements().next().unwrap();
    let blocks: Vec<&Element> = body.child_elements().collect();
    let names: Vec<&str> = blocks
        .iter()
        .map(|block| block.name.local.as_str())
        .collect();
    assert_eq!(names, ["p", "tbl", "p", "sectPr"]);
    let rows: Vec<String> = blocks[1]
        .child_elements()
        .filter(|child| child.name.local == "tr")
        .map(text_of)
        .collect();
    assert_eq!(rows.len(), 1_001);
    assert_eq!(
        (rows[0].as_str(), rows[1].as_str(), rows[1_000].as_str()),
        ("Name", "item 000000", "item 000999")
    );
    assert_eq!(text_of(blocks[2]), "After");
}
