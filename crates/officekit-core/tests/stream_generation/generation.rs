//! Section 3.12's generation, as a service streams it: a template's
//! `placeholder` row written once per item into a package written part by
//! part, every other part copied as it is, and an external hyperlink related
//! from the streamed part.

use core::error::Error;
use core::slice;
use std::io::{self, Write};

use officekit_core::parts::{main_document, style_definitions};
use officekit_markup::wordprocessing::{self, TableRow, Text, read};
use officekit_markup::{Descend, Written};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::Package;
use officekit_packaging::package_writer::PackageWriter;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{Relationship, RelationshipType, TargetMode};
use officekit_traversal::{Visit as _, WriteThrough as _};
use officekit_xml::element::Element;
use officekit_xml::name::Namespace;
use officekit_xml::stream::reader::PartEvent;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::Layout;

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
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

impl officekit_markup::Context for Fill {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for Fill {
    fn text(&mut self, node: &Text) -> Option<Text> {
        let mut text = node.clone();
        text.value.clone_from(&self.value);
        Some(text)
    }
}

impl officekit_markup::Visitor for FindPlaceholder {
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

impl officekit_markup::Context for RepeatRows {
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

/// Write `template` into `sink` with its `placeholder` row written `count`
/// times: its other parts copied, an external hyperlink related from its main
/// part, the main part read forward, the table loaded as a typed node and
/// written through `RepeatRows`, everything else written as read.
///
/// # Errors
///
/// The sink's, and the template's when a part cannot be read.
pub fn generate<S>(template: &Package, count: usize, sink: S) -> Result<S, Box<dyn Error>>
where
    S: Write,
{
    let main = main()?;
    let mut out = PackageWriter::from_template(sink, template);
    out.copy_parts(template, slice::from_ref(&main))?;
    out.relate(
        Some(&main),
        Relationship::new(
            RelationshipType::HYPERLINK.transitional,
            "https://example.com/",
            TargetMode::External,
        ),
    );
    let mut part = out.part(&main, main_document::CONTENT_TYPES[0], Layout::Compact)?;
    let mut reader = template.part_reader(&main)?;
    while let Some(event) = reader.read_event()? {
        match event {
            PartEvent::Start(start) if is_table(&start) => {
                let table = read::table(&reader.read_element(start)?);
                table.write_through(&mut RepeatRows { count }, &mut part)?;
            }
            PartEvent::Declaration(_)
            | PartEvent::End
            | PartEvent::Node(_)
            | PartEvent::Start(_) => {
                part.write_event(event)?;
            }
            _ => return Err("an event the generation does not know".into()),
        }
    }
    Ok(out.finish()?)
}

fn is_table(element: &Element) -> bool {
    element.name.local == "tbl" && element.name.namespace == Namespace::from_uri(W)
}

fn main() -> Result<PartName, PackageError> {
    PartName::new("/word/document.xml")
}

/// A document of a paragraph, a table with a header row and a `placeholder`
/// row, a paragraph, and the section's properties, and a styles part the
/// document relates to.
///
/// # Errors
///
/// The memory filesystem's.
pub fn template() -> Result<Package, PackageError> {
    let cell = |text: &str| format!("<w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>");
    let document = format!(
        "{DECLARATION}<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p>\
         <w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid>\
         <w:tr>{}</w:tr><w:tr>{}</w:tr></w:tbl><w:p><w:r><w:t>After</w:t></w:r></w:p><w:sectPr/></w:body></w:document>",
        cell("Name"),
        cell("placeholder")
    );
    let main = main()?;
    let styles = PartName::new("/word/styles.xml")?;
    let mut package = Package::new();
    package.set_part_bytes(&main, main_document::CONTENT_TYPES[0], document.as_bytes())?;
    package.relate(None, &main, main_document::RELATIONSHIP_TYPE.transitional)?;
    package.set_part_bytes(
        &styles,
        style_definitions::CONTENT_TYPE,
        format!("{DECLARATION}<w:styles xmlns:w=\"{W}\"/>").as_bytes(),
    )?;
    package.relate(
        Some(&main),
        &styles,
        style_definitions::RELATIONSHIP_TYPE.transitional,
    )?;
    Ok(package)
}
