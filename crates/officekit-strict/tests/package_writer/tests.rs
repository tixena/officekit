use core::slice;
use std::io::{self, Write};

use officekit_markup::common_simple_types::MeasurementInTwentiethsOfAPoint;
use officekit_markup::wordprocessing::{
    self, BlockContent, ContentCellContent, ContentRowContent, Document, DocumentBody,
    GridColumnDefinition, MeasurementOrPercentageValue, Paragraph, ParagraphContent,
    PreferredTableWidth, StyleDefinitions, Table, TableCell, TableRow, TableWidthUnits, Text,
    TextRun, TextRunContent, read, write,
};
use officekit_markup::{Descend, Written};
use officekit_strict::traversal::{Visit, WriteThrough as _};
use officekit_xml::element::{Document as Part, Element, NamespaceDeclaration};
use officekit_xml::name::Namespace;
use officekit_xml::packaging::package::{DECLARATION as XML_DECLARATION, OpenOptions, Package};
use officekit_xml::packaging::package_writer::PackageWriter;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{
    RELATIONSHIPS_CONTENT_TYPE, Relationship, RelationshipType, Target, TargetMode,
};
use officekit_xml::stream::reader::PartEvent;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{Layout, Scope, WriteOptions};

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MAIN_DOCUMENT_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const STYLE_DEFINITIONS_CONTENT_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";

/// The value of every text it visits, in order.
struct CollectText(String);

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

impl officekit_markup::Visitor for CollectText {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl wordprocessing::Visitor for CollectText {
    fn text(&mut self, node: &Text) -> Descend {
        self.0.push_str(&node.value);
        Descend::Yes
    }
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
        .part(&main, MAIN_DOCUMENT_CONTENT_TYPE, Layout::Compact)
        .unwrap();
    let mut reader = template.part_reader(&main).unwrap();
    while let Some(event) = reader.read_event().unwrap() {
        if let PartEvent::Start(start) = event {
            if is_table(&start) {
                let table = read::table(&reader.read_element(start).unwrap());
                table
                    .write_through(&mut RepeatRows { count }, &mut part)
                    .unwrap();
            } else {
                part.write_event(PartEvent::Start(start)).unwrap();
            }
        } else {
            part.write_event(event).unwrap();
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

/// The bytes of a part whose root is `root`, after the declaration Office writes.
fn part_bytes(root: Element) -> Vec<u8> {
    Part::new(XML_DECLARATION, root).to_xml(WriteOptions::default())
}

/// A document of a paragraph, a table with a header row and a `placeholder`
/// row, a paragraph, and the section's properties, and a styles part the
/// document relates to.
fn template() -> Package {
    let paragraph = |value: &str| {
        let mut text = Text::default();
        text.value = value.to_owned();
        let mut run = TextRun::default();
        run.content = vec![TextRunContent::Text(text)];
        let mut paragraph = Paragraph::default();
        paragraph.content = vec![ParagraphContent::TextRun(run)];
        paragraph
    };
    let row = |value: &str| {
        let mut cell = TableCell::default();
        cell.content = vec![BlockContent::Paragraph(paragraph(value))];
        let mut row = TableRow::default();
        row.content = vec![ContentCellContent::TableCell(cell)];
        ContentRowContent::TableRow(row)
    };
    let mut width = PreferredTableWidth::default();
    width.r#type = Some(TableWidthUnits::from("auto".to_owned()));
    width.w = Some(MeasurementOrPercentageValue::from("0".to_owned()));
    let mut column = GridColumnDefinition::default();
    column.w = Some(MeasurementInTwentiethsOfAPoint::from("4000".to_owned()));
    let mut table = Table::default();
    table.content_row_content = vec![row("Name"), row("placeholder")];
    table.grid.column_definition = vec![column];
    table.properties.preferred_table_width = Some(Box::new(width));
    let mut body = DocumentBody::default();
    body.content = vec![
        BlockContent::Paragraph(paragraph("Before")),
        BlockContent::Table(table),
        BlockContent::Paragraph(paragraph("After")),
    ];
    body.document_final_section_properties = Some(Box::default());
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    let mut package = Package::new();
    package
        .set_part_bytes(
            &main(),
            MAIN_DOCUMENT_CONTENT_TYPE,
            &part_bytes(write::document(&document, &mut Scope::default())),
        )
        .unwrap();
    package
        .relate(
            None,
            &main(),
            RelationshipType::OFFICE_DOCUMENT.transitional,
        )
        .unwrap();
    package
        .set_part_bytes(
            &styles(),
            STYLE_DEFINITIONS_CONTENT_TYPE,
            &part_bytes(write::style_definitions(
                &StyleDefinitions::default(),
                &mut Scope::default(),
            )),
        )
        .unwrap();
    package
        .relate(
            Some(&main()),
            &styles(),
            RelationshipType::STYLES.transitional,
        )
        .unwrap();
    package
}

/// The text of every `w:t` under `node`, in order.
fn text_of<N>(node: &N) -> String
where
    N: Visit,
{
    let mut collected = CollectText(String::new());
    node.visit(&mut collected);
    collected.0
}

#[test]
fn a_part_written_in_each_layout_gives_its_bytes() {
    let template = template();
    let name = PartName::new("/word/note.xml").unwrap();
    let declarations = [NamespaceDeclaration::new(
        "x".to_owned(),
        "urn:x".to_owned(),
    )];
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
    assert_eq!(
        package.part_bytes(&styles()).unwrap(),
        template.part_bytes(&styles()).unwrap()
    );
    assert_eq!(
        package.content_type(&main()),
        Some(MAIN_DOCUMENT_CONTENT_TYPE)
    );
    let document = package.load_tree(&main(), OpenOptions::default()).unwrap();
    let body = document.root.child_elements().next().unwrap();
    let blocks: Vec<&Element> = body.child_elements().collect();
    let names: Vec<&str> = blocks
        .iter()
        .map(|block| block.name.local.as_str())
        .collect();
    assert_eq!(names, ["p", "tbl", "p", "sectPr"]);
    let rows: Vec<String> = read::table(blocks[1])
        .content_row_content
        .iter()
        .filter_map(|row| {
            if let ContentRowContent::TableRow(found) = row {
                Some(text_of(found))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(rows.len(), 1_001);
    assert_eq!(
        (rows[0].as_str(), rows[1].as_str(), rows[1_000].as_str()),
        ("Name", "item 000000", "item 000999")
    );
    assert_eq!(text_of(&read::paragraph(blocks[2])), "After");
}
