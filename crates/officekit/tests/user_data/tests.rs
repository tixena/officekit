use officekit::docx::Docx;
use officekit::parts::kinds::WordprocessingKind;
use officekit_markup::wordprocessing::{
    BlockContent, BlockContentWith, ContentCellContent, ContentCellContentWith, ContentRowContent,
    ContentRowContentWith, Document, DocumentBody, DocumentFinalSectionProperties, DocumentWith,
    PageSize, Paragraph, ParagraphContent, ParagraphWith, Table, TableCell, TableRow, Text,
    TextRun, TextRunContent,
};
use officekit_markup::{Extra, UserData, user_data};
use officekit_xml::element::{Element, NamespaceDeclaration, Node};
use officekit_xml::name::QName;

const FIRST_LINES: [f32; 2] = [72.0, 86.0];

/// Where a layout engine drew a node: a page and a box on it, in points.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct PageBox {
    height: f32,
    page: u32,
    width: f32,
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Placement;

#[user_data]
impl UserData for Placement {
    type WordprocessingParagraph = Vec<PageBox>;
}

/// A paragraph of one run holding `text`.
fn paragraph(text: &str) -> Paragraph {
    let mut characters = Text::default();
    text.clone_into(&mut characters.value);
    let mut run = TextRun::default();
    run.content = vec![TextRunContent::Text(characters)];
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::TextRun(run)];
    paragraph
}

/// A table of one row and one cell holding the paragraph `text`.
fn table(text: &str) -> Table {
    let mut cell = TableCell::default();
    cell.content = vec![BlockContent::Paragraph(paragraph(text))];
    let mut row = TableRow::default();
    row.content = vec![ContentCellContent::TableCell(cell)];
    let mut table = Table::default();
    table.content_row_content = vec![ContentRowContent::TableRow(row)];
    table
}

/// Two paragraphs, a table, the section's properties and an element no schema declares.
fn document() -> Document {
    let mut size = PageSize::default();
    size.w = Some("12240".to_owned().into());
    size.h = Some("15840".to_owned().into());
    let mut section = DocumentFinalSectionProperties::default();
    section.page_size = Some(Box::new(size));
    let mut note = Element::new(QName::prefixed("x", "note", "urn:example:extra"));
    note.namespace_declarations = vec![NamespaceDeclaration::new(
        "x".to_owned(),
        "urn:example:extra".to_owned(),
    )];
    let mut body = DocumentBody::default();
    body.content = vec![
        BlockContent::Paragraph(paragraph("first")),
        BlockContent::Paragraph(paragraph("second")),
        BlockContent::Table(table("in a cell")),
    ];
    body.document_final_section_properties = Some(Box::new(section));
    body.unknown
        .children
        .push(Extra::new(3, Node::Element(note)));
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document
}

/// Give each paragraph of the body one box, as a layout engine would while it lays them out.
fn place_paragraphs(document: &mut DocumentWith<Placement>) {
    let Some(body) = document.body.as_deref_mut() else {
        return;
    };
    let paragraphs = body.content.iter_mut().filter_map(|item| {
        if let BlockContentWith::Paragraph(paragraph) = item {
            Some(paragraph)
        } else {
            None
        }
    });
    for (paragraph, y) in paragraphs.zip(FIRST_LINES) {
        paragraph.user_data.push(PageBox {
            height: 14.0,
            page: 1,
            width: 468.0,
            x: 72.0,
            y,
        });
    }
}

#[test]
fn a_document_goes_to_a_tree_with_slots_and_comes_back_unchanged() {
    let document = document();
    let laid_out: DocumentWith<Placement> = document.clone().into_user_data();
    let back: Document = laid_out.into_user_data();
    assert_eq!(back, document);
}

#[test]
fn the_plain_tree_converted_to_the_plain_tree_is_the_identity() {
    let document = document();
    assert_eq!(document.clone().into_user_data::<()>(), document);
}

#[test]
fn the_unknown_content_survives_the_conversion() {
    let laid_out: DocumentWith<Placement> = document().into_user_data();
    let body = laid_out.body.as_deref().unwrap();
    assert_eq!(body.unknown.children.len(), 1);
}

/// The paragraph of the first cell of the first row of the table that ends the body.
fn paragraph_in_cell(document: &DocumentWith<Placement>) -> Option<&ParagraphWith<Placement>> {
    let body = document.body.as_deref()?;
    let BlockContentWith::Table(table) = body.content.last()? else {
        return None;
    };
    let ContentRowContentWith::TableRow(row) = table.content_row_content.first()? else {
        return None;
    };
    let ContentCellContentWith::TableCell(cell) = row.content.first()? else {
        return None;
    };
    let BlockContentWith::Paragraph(paragraph) = cell.content.first()? else {
        return None;
    };
    Some(paragraph)
}

#[test]
fn the_walk_fills_the_body_paragraphs_and_leaves_the_cell_paragraph_empty() {
    let mut laid_out: DocumentWith<Placement> = document().into_user_data();
    place_paragraphs(&mut laid_out);
    let body = laid_out.body.as_deref().unwrap();
    let held: Vec<usize> = body
        .content
        .iter()
        .filter_map(|item| {
            if let BlockContentWith::Paragraph(paragraph) = item {
                Some(paragraph.user_data.len())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(held, [1, 1]);
    let in_cell = paragraph_in_cell(&laid_out).unwrap();
    assert_eq!(in_cell.user_data, Vec::new());
}

#[test]
fn a_document_brought_back_after_the_walk_is_written_as_the_original() {
    let document = document();
    let mut laid_out: DocumentWith<Placement> = document.clone().into_user_data();
    place_paragraphs(&mut laid_out);
    let back: Document = laid_out.into_user_data();
    let mut original = Docx::new(WordprocessingKind::Document).unwrap();
    original.set_main_document(&document).unwrap();
    let mut returned = Docx::new(WordprocessingKind::Document).unwrap();
    returned.set_main_document(&back).unwrap();
    assert_eq!(
        returned.to_bytes_transitional().unwrap(),
        original.to_bytes_transitional().unwrap()
    );
}
