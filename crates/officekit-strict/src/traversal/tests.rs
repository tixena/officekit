//! The typed tree's three ways in and its write-through form, over the
//! generated `WordprocessingML` and Office Math, which embed each other.

mod presentation;
mod runtime;
mod write_through;

use core::mem;
use std::io;

use officekit_markup::drawing::{AnyElementContent, GraphicObjectData};
use officekit_markup::picture::{self, Picture};
use officekit_markup::wordprocessing::{
    BlockContent, BlockLevelStructuredDocumentTag, BlockLevelStructuredDocumentTagContent,
    ContentBlockContent, ContentCellContent, ContentRowContent, Document, DocumentBody,
    EmbeddedObject, EmbeddedObjectContent, FriendlyName, Paragraph, ParagraphContent,
    ParagraphProperties, StructuredDocumentTagProperties, Table, TableCell, TableRow, Text,
    TextRun, TextRunContent, read, write,
};
use officekit_markup::{Descend, Extra, Written, math, wordprocessing};
use officekit_xml::element::{Element, Node};
use officekit_xml::name::QName;
use officekit_xml::parse::parse;
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::{Layout, Scope};

use crate::traversal::{Transform as _, Unchanged, Visit, WriteThrough};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// Collects every piece of text under a node, in both namespaces.
#[derive(Default)]
struct CollectText(String);

/// Puts the comment `x` in place of each embedded control.
struct CommentForControl;

/// Puts copies of each paragraph of a block list in its place, as many as
/// `copies` gives for it, in the order the list hook is asked about them.
struct CopyParagraphs {
    copies: Vec<usize>,
    next: usize,
}

/// Counts the content controls a context's hook is shown.
#[derive(Default)]
struct Count(usize);

/// Replaces the content control with the given alias by a paragraph holding
/// `value`, in both block lists.
struct FillControl {
    alias: &'static str,
    value: String,
}

/// Finds a content control with the given alias.
struct Find {
    alias: &'static str,
    found: bool,
}

/// Every block-level content control becomes one paragraph, in both block
/// lists.
struct FlattenControls;

/// Numbers every paragraph, keeping on into its children.
struct NumberParagraphs {
    next: usize,
}

/// Puts an empty paragraph in place of each `Other` item of a block list.
struct ParagraphForOther;

/// Counts pictures and generic elements, and renames each generic element
/// `x:chart` to `x:graph`.
#[derive(Default)]
struct Pictures {
    elements: usize,
    pictures: usize,
}

/// Records the hooks it is asked, in the order it is asked them.
#[derive(Default)]
struct Record(Vec<&'static str>);

/// Renames each generic element `x:a` to `x:b`, and writes each `x:c` itself
/// as `x:written`.
struct Rename;

/// A row whose cell holds a "repeat" control becomes one row per item.
struct RepeatRows {
    items: Vec<String>,
}

/// A row is replaced because of a control found inside one of its cells.
#[derive(Default)]
struct ReplaceRowsWithControls {
    seen: bool,
}

/// The write-through form of `RepeatRows`: the row holding a "repeat" control
/// written once per item, one at a time.
struct StreamRepeatRows {
    count: usize,
}

/// Content controls replaced by their own transformed content, and empty
/// paragraphs dropped.
struct UnwrapControls;

/// Upper-cases math text and `WordprocessingML` text alike.
struct UpperCaseEverywhere;

impl officekit_markup::Visitor for CollectText {
    fn math(&mut self) -> Option<&mut dyn math::Visitor> {
        Some(self)
    }

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

impl math::Visitor for CollectText {
    fn text(&mut self, node: &math::Text) -> Descend {
        self.0.push_str(&node.value);
        Descend::Yes
    }
}

impl officekit_markup::Context for CommentForControl {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for CommentForControl {
    fn embedded_object_content(
        &mut self,
        node: &EmbeddedObjectContent,
    ) -> Option<EmbeddedObjectContent> {
        matches!(node, EmbeddedObjectContent::EmbeddedControl(_))
            .then(|| EmbeddedObjectContent::Other(Node::Comment("x".to_owned())))
    }
}

impl officekit_markup::Context for CopyParagraphs {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for CopyParagraphs {
    fn block_items(&mut self, node: &BlockContent) -> Option<Vec<BlockContent>> {
        if !matches!(node, BlockContent::Paragraph(_)) {
            return None;
        }
        let copies = *self.copies.get(self.next)?;
        self.next += 1;
        Some(vec![node.clone(); copies])
    }
}

impl officekit_markup::Context for Count {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for Count {
    fn block_level_structured_document_tag(
        &mut self,
        _node: &BlockLevelStructuredDocumentTag,
    ) -> Option<BlockLevelStructuredDocumentTag> {
        self.0 += 1;
        None
    }
}

impl officekit_markup::Context for FillControl {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for FillControl {
    fn block(&mut self, node: &BlockContent) -> Option<BlockContent> {
        let BlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        filled(sdt, self.alias, &self.value).map(BlockContent::Paragraph)
    }

    fn content_block(&mut self, node: &ContentBlockContent) -> Option<ContentBlockContent> {
        let ContentBlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        filled(sdt, self.alias, &self.value).map(ContentBlockContent::Paragraph)
    }
}

impl officekit_markup::Visitor for Find {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl wordprocessing::Visitor for Find {
    fn block_level_structured_document_tag(
        &mut self,
        node: &BlockLevelStructuredDocumentTag,
    ) -> Descend {
        self.found |= alias(node) == Some(self.alias);
        Descend::Yes
    }
}

impl officekit_markup::Context for FlattenControls {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for FlattenControls {
    fn block(&mut self, node: &BlockContent) -> Option<BlockContent> {
        let BlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        Some(BlockContent::Paragraph(paragraph_from(sdt)))
    }

    fn content_block(&mut self, node: &ContentBlockContent) -> Option<ContentBlockContent> {
        let ContentBlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        Some(ContentBlockContent::Paragraph(paragraph_from(sdt)))
    }
}

impl officekit_markup::Context for NumberParagraphs {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for NumberParagraphs {
    fn paragraph(&mut self, node: &Paragraph) -> Option<Paragraph> {
        self.next += 1;
        let label = ParagraphContent::TextRun(run(preserved(&format!("{}. ", self.next))));
        let mut rebuilt = node.transform_children(self);
        rebuilt.content.insert(0, label);
        Some(rebuilt)
    }
}

impl officekit_markup::Context for ParagraphForOther {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for ParagraphForOther {
    fn block(&mut self, node: &BlockContent) -> Option<BlockContent> {
        matches!(node, BlockContent::Other(_))
            .then(|| BlockContent::Paragraph(Paragraph::default()))
    }
}

// The hook `picture` and the accessor `picture` share their name, each in its
// own trait.
impl officekit_markup::Context for Pictures {
    fn element(&mut self, node: &Element) -> Option<Element> {
        self.elements += 1;
        (node.name.local == "chart").then(|| renamed(node, "graph"))
    }

    fn picture(&mut self) -> Option<&mut dyn picture::Context> {
        Some(self)
    }
}

impl picture::Context for Pictures {
    fn picture(&mut self, _node: &Picture) -> Option<Picture> {
        self.pictures += 1;
        None
    }
}

impl officekit_markup::Visitor for Pictures {
    fn element(&mut self, _node: &Element) -> Descend {
        self.elements += 1;
        Descend::Yes
    }

    fn picture(&mut self) -> Option<&mut dyn picture::Visitor> {
        Some(self)
    }
}

impl picture::Visitor for Pictures {
    fn picture(&mut self, _node: &Picture) -> Descend {
        self.pictures += 1;
        Descend::Yes
    }
}

impl officekit_markup::Context for Record {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for Record {
    fn paragraph_properties(&mut self, _node: &ParagraphProperties) -> Option<ParagraphProperties> {
        self.0.push("paragraph_properties");
        None
    }

    fn text_run(&mut self, _node: &TextRun) -> Option<TextRun> {
        self.0.push("text_run");
        None
    }
}

impl officekit_markup::Context for Rename {
    fn element(&mut self, node: &Element) -> Option<Element> {
        (node.name.local == "a").then(|| renamed(node, "b"))
    }

    fn write_element(&mut self, node: &Element, out: &mut PartWriter<'_>) -> io::Result<Written> {
        if node.name.local != "c" {
            return Ok(Written::No);
        }
        out.empty("x:written", &[])?;
        Ok(Written::Yes)
    }
}

impl officekit_markup::Context for RepeatRows {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for RepeatRows {
    fn table(&mut self, node: &Table) -> Option<Table> {
        let mut rows = Vec::new();
        for row in &node.content_row_content {
            if holds_control(row, "repeat") {
                for item in &self.items {
                    rows.push(row.transform(&mut FillControl {
                        alias: "repeat",
                        value: item.clone(),
                    }));
                }
            } else {
                rows.push(row.transform(self));
            }
        }
        let mut table = node.clone();
        table.content_row_content = rows;
        Some(table)
    }
}

impl officekit_markup::Context for ReplaceRowsWithControls {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for ReplaceRowsWithControls {
    fn block_level_structured_document_tag(
        &mut self,
        _node: &BlockLevelStructuredDocumentTag,
    ) -> Option<BlockLevelStructuredDocumentTag> {
        self.seen = true;
        None
    }

    fn table_row(&mut self, node: &TableRow) -> Option<TableRow> {
        let outer = mem::replace(&mut self.seen, false);
        let rebuilt = node.transform_children(self);
        let seen_below = mem::replace(&mut self.seen, outer);
        Some(if seen_below {
            replacement_row()
        } else {
            rebuilt
        })
    }
}

impl officekit_markup::Context for StreamRepeatRows {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for StreamRepeatRows {
    fn write_content_row_items(
        &mut self,
        node: &ContentRowContent,
        out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        if !holds_control(node, "repeat") {
            return Ok(Written::No);
        }
        for index in 0..self.count {
            node.write_through(
                &mut FillControl {
                    alias: "repeat",
                    value: format!("item {index:06}"),
                },
                out,
            )?;
        }
        Ok(Written::Yes)
    }
}

impl officekit_markup::Context for UnwrapControls {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for UnwrapControls {
    fn block_items(&mut self, node: &BlockContent) -> Option<Vec<BlockContent>> {
        if let BlockContent::BlockLevelStructuredDocumentTag(sdt) = node {
            let content = sdt.transform_children(self).content.unwrap_or_default();
            return Some(
                content
                    .content
                    .into_iter()
                    .map(BlockContent::from)
                    .collect(),
            );
        }
        if let BlockContent::Paragraph(paragraph) = node
            && paragraph.content.is_empty()
        {
            return Some(Vec::new());
        }
        None
    }

    fn content_block_items(
        &mut self,
        node: &ContentBlockContent,
    ) -> Option<Vec<ContentBlockContent>> {
        if let ContentBlockContent::BlockLevelStructuredDocumentTag(sdt) = node {
            let content = sdt.transform_children(self).content.unwrap_or_default();
            return Some(content.content);
        }
        if let ContentBlockContent::Paragraph(paragraph) = node
            && paragraph.content.is_empty()
        {
            return Some(Vec::new());
        }
        None
    }
}

impl officekit_markup::Context for UpperCaseEverywhere {
    fn math(&mut self) -> Option<&mut dyn math::Context> {
        Some(self)
    }

    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for UpperCaseEverywhere {
    fn text(&mut self, node: &Text) -> Option<Text> {
        let mut upper = node.clone();
        upper.value = node.value.to_uppercase();
        Some(upper)
    }
}

impl math::Context for UpperCaseEverywhere {
    fn text(&mut self, node: &math::Text) -> Option<math::Text> {
        let mut upper = node.clone();
        upper.value = node.value.to_uppercase();
        Some(upper)
    }
}

/// The alias a content control carries.
fn alias(sdt: &BlockLevelStructuredDocumentTag) -> Option<&str> {
    sdt.properties
        .as_ref()?
        .friendly_name
        .as_ref()?
        .val
        .as_deref()
}

/// The block, when it is a content control.
fn as_control(block: &BlockContent) -> Option<&BlockLevelStructuredDocumentTag> {
    if let BlockContent::BlockLevelStructuredDocumentTag(sdt) = block {
        Some(sdt)
    } else {
        None
    }
}

/// The block, when it is a table.
fn as_table(block: &BlockContent) -> Option<&Table> {
    if let BlockContent::Table(table) = block {
        Some(table)
    } else {
        None
    }
}

/// The blocks of a document's body.
fn body(document: &Document) -> &[BlockContent] {
    document
        .body
        .as_deref()
        .map_or(&[], |body| body.content.as_slice())
}

/// The items of a content control's content.
fn content(sdt: &BlockLevelStructuredDocumentTag) -> &[ContentBlockContent] {
    sdt.content
        .as_deref()
        .map_or(&[], |content| content.content.as_slice())
}

/// A block-level content control holding one paragraph of `inner`.
fn control(alias: &str, inner: &str) -> BlockContent {
    BlockContent::BlockLevelStructuredDocumentTag(control_of(
        Some(alias),
        vec![ContentBlockContent::Paragraph(text_paragraph(inner))],
    ))
}

/// A block-level content control with an alias, when given, and `items`.
fn control_of(
    alias: Option<&str>,
    items: Vec<ContentBlockContent>,
) -> BlockLevelStructuredDocumentTag {
    let properties = alias.map(|name| {
        let mut friendly_name = FriendlyName::default();
        friendly_name.val = Some(name.to_owned());
        let mut found = StructuredDocumentTagProperties::default();
        found.friendly_name = Some(Box::new(friendly_name));
        Box::new(found)
    });
    let mut content = BlockLevelStructuredDocumentTagContent::default();
    content.content = items;
    let mut sdt = BlockLevelStructuredDocumentTag::default();
    sdt.content = Some(Box::new(content));
    sdt.properties = properties;
    sdt
}

/// The XML of an element, written in the compact layout.
fn compact(element: &Element) -> String {
    let mut bytes = Vec::new();
    PartWriter::new(&mut bytes, Layout::Compact)
        .element(element)
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

/// A context putting `counts[i]` copies of the i-th paragraph asked about in
/// its place.
fn copies(counts: &[usize]) -> CopyParagraphs {
    CopyParagraphs {
        copies: counts.to_vec(),
        next: 0,
    }
}

/// A document whose body holds `blocks`.
fn document(blocks: Vec<BlockContent>) -> Document {
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut node = Document::default();
    node.body = Some(Box::new(body));
    node
}

/// The XML the tree writer writes for an embedded object.
fn object_xml(object: &EmbeddedObject) -> String {
    compact(&write::embedded_object(object, &mut Scope::default()))
}

/// The paragraph holding `value` in place of the control with `alias`.
fn filled(sdt: &BlockLevelStructuredDocumentTag, wanted: &str, value: &str) -> Option<Paragraph> {
    (alias(sdt) == Some(wanted)).then(|| paragraph_of(preserved(value)))
}

/// Whether any content control under `node` carries `alias`.
fn holds_control<N>(node: &N, alias: &'static str) -> bool
where
    N: Visit,
{
    let mut find = Find {
        alias,
        found: false,
    };
    node.visit(&mut find);
    find.found
}

/// "x" in math text, then "=1" in `WordprocessingML` text inside the math run.
fn math_paragraph() -> Paragraph {
    let mut math_text = math::Text::default();
    math_text.value = "x".to_owned();
    let mut math_run = math::Run::default();
    math_run.content = vec![
        math::RunContent::Text(math_text),
        math::RunContent::WordprocessingText(text("=1")),
    ];
    let mut office_math = math::OfficeMath::default();
    office_math.content = vec![math::OfficeMathContent::Run(math_run)];
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::OfficeMath(office_math)];
    paragraph
}

/// One paragraph holding the text of a content control's children.
fn paragraph_from(sdt: &BlockLevelStructuredDocumentTag) -> Paragraph {
    let mut collected = CollectText::default();
    sdt.visit_children(&mut collected);
    paragraph_of(preserved(&collected.0))
}

fn paragraph_of(value: Text) -> Paragraph {
    let mut paragraph = Paragraph::default();
    paragraph.content = vec![ParagraphContent::TextRun(run(value))];
    paragraph
}

/// The root element of an XML text.
fn parsed(text: &str) -> Element {
    parse(text.as_bytes()).unwrap().root
}

/// Text whose spaces are kept.
fn preserved(value: &str) -> Text {
    let mut node = Text::default();
    node.space = Some("preserve".to_owned());
    node.value = value.to_owned();
    node
}

/// A generic element named `local` in place of `node`.
fn renamed(node: &Element, local: &str) -> Element {
    let mut element = node.clone();
    element.name.local = local.to_owned();
    element
}

fn replacement_row() -> TableRow {
    row_of(BlockContent::Paragraph(text_paragraph("replaced")))
}

fn row_of(block: BlockContent) -> TableRow {
    row_of_blocks(vec![block])
}

/// A row of one cell holding `blocks`.
fn row_of_blocks(blocks: Vec<BlockContent>) -> TableRow {
    let mut cell = TableCell::default();
    cell.content = blocks;
    let mut row = TableRow::default();
    row.content = vec![ContentCellContent::TableCell(cell)];
    row
}

fn run(value: Text) -> TextRun {
    let mut node = TextRun::default();
    node.content = vec![TextRunContent::Text(value)];
    node
}

/// A paragraph, a content control holding a paragraph, a table, and math,
/// and a paragraph of math.
fn sample() -> Document {
    document(vec![
        BlockContent::Paragraph(text_paragraph("Hello")),
        BlockContent::BlockLevelStructuredDocumentTag(control_of(
            Some("control"),
            vec![
                ContentBlockContent::Paragraph(text_paragraph("inside ")),
                ContentBlockContent::Table(Table::default()),
                ContentBlockContent::Paragraph(math_paragraph()),
            ],
        )),
        BlockContent::Paragraph(math_paragraph()),
    ])
}

/// A header row, a row whose cell holds a "repeat" control, and a total row.
fn table() -> Table {
    let mut node = Table::default();
    node.content_row_content = vec![
        ContentRowContent::TableRow(row_of(BlockContent::Paragraph(text_paragraph("Name")))),
        ContentRowContent::TableRow(row_of(control("repeat", "placeholder"))),
        ContentRowContent::TableRow(row_of(BlockContent::Paragraph(text_paragraph("Total")))),
    ];
    node
}

fn text(value: &str) -> Text {
    let mut node = Text::default();
    node.value = value.to_owned();
    node
}

fn text_paragraph(value: &str) -> Paragraph {
    paragraph_of(text(value))
}

fn texts<N>(node: &N) -> String
where
    N: Visit,
{
    let mut collected = CollectText::default();
    node.visit(&mut collected);
    collected.0
}

/// The XML the tree writer writes for a body.
fn tree_xml(body: &DocumentBody) -> String {
    compact(&write::document_body(body, &mut Scope::default()))
}

fn upper_case_first_paragraph(document: &mut Document) {
    let first = document
        .body
        .as_deref_mut()
        .and_then(|body| body.content.first_mut());
    if let Some(BlockContent::Paragraph(paragraph)) = first {
        for item in &mut paragraph.content {
            if let ParagraphContent::TextRun(run) = item {
                for inner in &mut run.content {
                    if let TextRunContent::Text(value) = inner {
                        value.value = value.value.to_uppercase();
                    }
                }
            }
        }
    }
}

/// The XML a node writes through `ctx`.
fn written<N>(node: &N, ctx: &mut dyn officekit_markup::Context) -> String
where
    N: WriteThrough,
{
    let mut bytes = Vec::new();
    node.write_through(ctx, &mut PartWriter::new(&mut bytes, Layout::Compact))
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

/// The XML a node writes as it is.
fn xml<N>(node: &N) -> String
where
    N: WriteThrough,
{
    written(node, &mut Unchanged)
}

#[test]
fn flatten_from_the_root_changes_kind_and_leaves_the_source_untouched() {
    let source = sample();
    let flattened = source.transform(&mut FlattenControls);
    assert_eq!(source, sample());
    let blocks = body(&flattened);
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0], BlockContent::Paragraph(text_paragraph("Hello")));
    // Text from both namespaces.
    assert_eq!(
        blocks[1],
        BlockContent::Paragraph(paragraph_of(preserved("inside x=1")))
    );
    assert_eq!(blocks[2], body(&source)[2]);
}

#[test]
fn any_node_is_an_entry_point() {
    let source = sample();
    let block = body(&source)[1].transform(&mut FlattenControls);
    assert!(matches!(block, BlockContent::Paragraph(_)));
    let sdt = as_control(&body(&source)[1]).unwrap();
    // A struct-typed node keeps its kind.
    assert_eq!(&sdt.transform(&mut FlattenControls), sdt);
}

#[test]
fn a_hook_carries_state_and_descends_through_transform_children() {
    let mut ctx = NumberParagraphs { next: 0 };
    let numbered = sample().transform(&mut ctx);
    // "Hello", "inside ", the math paragraph inside the control, and the last one.
    assert_eq!(ctx.next, 4);
    assert_eq!(texts(&numbered), "1. Hello2. inside 3. x=14. x=1");
}

#[test]
fn one_context_reaches_both_namespaces_through_each_other() {
    let upper = sample().transform(&mut UpperCaseEverywhere);
    assert_eq!(texts(&upper), "HELLOINSIDE X=1X=1");
    assert_eq!(texts(&sample()), "Helloinside x=1x=1");
}

#[test]
fn a_context_for_one_namespace_leaves_the_other_alone() {
    let flattened = sample().transform(&mut FlattenControls);
    assert_eq!(
        body(&flattened)[2],
        BlockContent::Paragraph(math_paragraph())
    );
}

#[test]
fn edit_in_place() {
    let mut document = sample();
    upper_case_first_paragraph(&mut document);
    assert_eq!(
        body(&document)[0],
        BlockContent::Paragraph(text_paragraph("HELLO"))
    );
}

#[test]
fn the_tables_hook_looks_down_and_turns_one_row_into_one_per_item() {
    let source = table();
    let repeated = source.transform(&mut RepeatRows {
        items: vec!["Ana".to_owned(), "Luis".to_owned()],
    });
    let rows: Vec<String> = repeated.content_row_content.iter().map(texts).collect();
    assert_eq!(rows, ["Name", "Ana", "Luis", "Total"]);
    assert_eq!(source, table());
}

#[test]
fn a_decision_made_inside_a_cell_replaces_the_row() {
    let source = document(vec![BlockContent::Table(table())]);
    let mut ctx = ReplaceRowsWithControls::default();
    let replaced = source.transform(&mut ctx);
    let rows = as_table(&body(&replaced)[0]).unwrap();
    let expected = table();
    // Rows without a control are rebuilt as they were; the row around the
    // control is replaced.
    assert_eq!(rows.content_row_content[0], expected.content_row_content[0]);
    assert_eq!(
        rows.content_row_content[1],
        ContentRowContent::TableRow(replacement_row())
    );
    assert_eq!(rows.content_row_content[2], expected.content_row_content[2]);
    // The note does not leak past the row that read it.
    assert!(!ctx.seen);
}

#[test]
fn a_hook_that_changes_kind_reaches_only_the_list_its_enum_is() {
    struct BodyListsOnly;
    impl officekit_markup::Context for BodyListsOnly {
        fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
            Some(self)
        }
    }
    impl wordprocessing::Context for BodyListsOnly {
        fn block(&mut self, node: &BlockContent) -> Option<BlockContent> {
            let BlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
                return None;
            };
            (alias(sdt) == Some("repeat"))
                .then(|| BlockContent::Paragraph(text_paragraph("filled")))
        }
    }
    let inner = ContentBlockContent::BlockLevelStructuredDocumentTag(control_of(
        Some("repeat"),
        vec![ContentBlockContent::Paragraph(text_paragraph(
            "placeholder",
        ))],
    ));
    let outer = control_of(Some("outer"), vec![inner]);
    let source = document(vec![
        control("repeat", "placeholder"),
        BlockContent::BlockLevelStructuredDocumentTag(outer),
    ]);

    let body_only = source.transform(&mut BodyListsOnly);
    assert_eq!(
        body(&body_only)[0],
        BlockContent::Paragraph(text_paragraph("filled"))
    );
    let kept = as_control(&body(&body_only)[1]).unwrap();
    // Inside a control: not reached.
    assert!(matches!(
        content(kept)[0],
        ContentBlockContent::BlockLevelStructuredDocumentTag(_)
    ));

    let both = source.transform(&mut FillControl {
        alias: "repeat",
        value: "filled".to_owned(),
    });
    let reached = as_control(&body(&both)[1]).unwrap();
    // Reached through `content_block`.
    assert_eq!(
        content(reached)[0],
        ContentBlockContent::Paragraph(paragraph_of(preserved("filled")))
    );
}

#[test]
fn a_hook_on_the_control_itself_reaches_it_in_both_lists() {
    let inner =
        ContentBlockContent::BlockLevelStructuredDocumentTag(control_of(Some("inner"), Vec::new()));
    let source = document(vec![BlockContent::BlockLevelStructuredDocumentTag(
        control_of(Some("outer"), vec![inner]),
    )]);
    let mut count = Count::default();
    let _: Document = source.transform(&mut count);
    // The control in the body and the control inside it.
    assert_eq!(count.0, 2);
}

#[test]
fn a_list_hook_splices_any_number_of_blocks_in_any_block_list() {
    let nested = control_of(
        None,
        vec![ContentBlockContent::Paragraph(text_paragraph("deep"))],
    );
    let unwrapped_control = BlockContent::BlockLevelStructuredDocumentTag(control_of(
        None,
        vec![
            ContentBlockContent::Paragraph(text_paragraph("a")),
            ContentBlockContent::BlockLevelStructuredDocumentTag(nested),
            ContentBlockContent::Paragraph(Paragraph::default()),
        ],
    ));
    let mut wrapped = Table::default();
    wrapped.content_row_content = vec![ContentRowContent::TableRow(row_of(
        unwrapped_control.clone(),
    ))];
    let source = document(vec![unwrapped_control, BlockContent::Table(wrapped)]);
    let unwrapped = source.transform(&mut UnwrapControls);
    let blocks = body(&unwrapped);
    // "a", "deep", the table: the control became two blocks, the empty
    // paragraph none.
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0], BlockContent::Paragraph(text_paragraph("a")));
    assert_eq!(blocks[1], BlockContent::Paragraph(text_paragraph("deep")));
    let cells = as_table(&blocks[2]).unwrap();
    assert_eq!(
        cells.content_row_content,
        [ContentRowContent::TableRow(row_of_blocks(vec![
            BlockContent::Paragraph(text_paragraph("a")),
            BlockContent::Paragraph(text_paragraph("deep")),
        ]))]
    );
}

#[test]
fn writing_through_a_context_writes_what_its_transform_returns() {
    let source = sample();
    assert_eq!(
        written(&source, &mut FlattenControls),
        xml(&source.transform(&mut FlattenControls))
    );
    assert_eq!(
        written(&source, &mut NumberParagraphs { next: 0 }),
        xml(&source.transform(&mut NumberParagraphs { next: 0 }))
    );
    assert_eq!(
        written(&source, &mut UpperCaseEverywhere),
        xml(&source.transform(&mut UpperCaseEverywhere))
    );
    assert_eq!(
        written(&source, &mut UnwrapControls),
        xml(&source.transform(&mut UnwrapControls))
    );
    let rows = table();
    assert_eq!(
        written(&rows, &mut ReplaceRowsWithControls::default()),
        xml(&rows.transform(&mut ReplaceRowsWithControls::default()))
    );
    assert!(xml(&source).starts_with(&format!(
        "<w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p><w:sdt>\
         <w:sdtPr><w:alias w:val=\"control\"/></w:sdtPr>"
    )));
}

#[test]
fn writing_through_unchanged_writes_what_the_tree_writer_writes() {
    let source = sample();
    let element = write::document(&source, &mut Scope::default());
    let mut bytes = Vec::new();
    PartWriter::new(&mut bytes, Layout::Compact)
        .element(&element)
        .unwrap();
    assert_eq!(xml(&source), String::from_utf8(bytes).unwrap());
}

#[test]
fn a_row_list_write_hook_writes_a_repeated_row_one_at_a_time() {
    let source = table();
    let streamed = written(&source, &mut StreamRepeatRows { count: 3 });
    let items = (0..3_usize)
        .map(|index| format!("item {index:06}"))
        .collect();
    // The same rows the tree form builds.
    assert_eq!(streamed, xml(&source.transform(&mut RepeatRows { items })));
    // "Name", three items, "Total".
    assert_eq!(streamed.matches("<w:tr>").count(), 5);
}

#[test]
fn unknown_content_goes_through_the_root_hooks_at_its_place() {
    let generic = |local: &str| Node::Element(Element::new(QName::prefixed("x", local, "urn:x")));
    let mut source = Paragraph::default();
    source.content = vec![
        ParagraphContent::Other(generic("a")),
        ParagraphContent::TextRun(run(text("t"))),
        ParagraphContent::Other(generic("c")),
    ];
    source.properties = Some(Box::default());
    source.unknown.children = vec![Extra::new(0, generic("a"))];
    let renamed = source.transform(&mut Rename);
    assert_eq!(renamed.content[0], ParagraphContent::Other(generic("b")));
    assert_eq!(renamed.unknown.children[0].node, generic("b"));
    assert_eq!(
        written(&source, &mut Rename),
        format!(
            "<w:p xmlns:w=\"{W}\"><x:b/><w:pPr/><x:b/><w:r><w:t>t</w:t></w:r><x:written/></w:p>"
        )
    );
    assert_eq!(
        xml(&renamed),
        format!("<w:p xmlns:w=\"{W}\"><x:b/><w:pPr/><x:b/><w:r><w:t>t</w:t></w:r><x:c/></w:p>")
    );
}

#[test]
fn a_wildcard_position_reaches_its_typed_items_through_their_namespace() {
    let chart = Node::Element(Element::new(QName::prefixed("x", "chart", "urn:x")));
    let mut source = GraphicObjectData::default();
    source.content = vec![
        AnyElementContent::Picture(Picture::default()),
        AnyElementContent::Other(chart),
    ];
    let mut seen = Pictures::default();
    source.visit(&mut seen);
    assert_eq!((seen.pictures, seen.elements), (1, 1));

    let mut ctx = Pictures::default();
    let transformed = source.transform(&mut ctx);
    assert_eq!((ctx.pictures, ctx.elements), (1, 1));
    assert_eq!(
        transformed.content[1],
        AnyElementContent::Other(Node::Element(Element::new(QName::prefixed(
            "x", "graph", "urn:x"
        ))))
    );
    assert_eq!(
        written(&source, &mut Pictures::default()),
        xml(&transformed)
    );
}

/// A paragraph holding one run of `value`, the XML written through and by the
/// tree writer in the compact layout.
fn both_paths(value: Text) -> (String, String) {
    let source = paragraph_of(value);
    let element = write::paragraph(&source, &mut Scope::default());
    let mut bytes = Vec::new();
    PartWriter::new(&mut bytes, Layout::Compact)
        .element(&element)
        .unwrap();
    (xml(&source), String::from_utf8(bytes).unwrap())
}

#[test]
fn whitespace_only_text_written_through_is_dropped_as_the_compact_writer_drops_it() {
    let mut blank = Text::default();
    blank.value = " ".to_owned();
    let (through, tree) = both_paths(blank);
    assert_eq!(through, tree);
    assert!(through.contains("<w:t/>"));
}

#[test]
fn whitespace_only_text_in_a_preserve_scope_is_written_through() {
    let (through, tree) = both_paths(preserved(" "));
    assert_eq!(through, tree);
    assert!(through.contains("<w:t xml:space=\"preserve\"> </w:t>"));
}

#[test]
fn an_extra_after_a_list_a_hook_lengthens_stays_after_its_neighbours() {
    let as_read = format!("<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:sectPr/><!--c--></w:body>");
    let source = read::document_body(&parsed(&as_read));
    // Two blocks and `w:sectPr` come before the comment.
    assert_eq!(source.unknown.children[0].after, 3);
    assert_eq!(tree_xml(&source.transform(&mut Unchanged)), as_read);
    let transformed = source.transform(&mut copies(&[3, 1]));
    assert_eq!(
        tree_xml(&transformed),
        format!("<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:p/><w:p/><w:sectPr/><!--c--></w:body>")
    );
    assert_eq!(
        tree_xml(&transformed),
        written(&source, &mut copies(&[3, 1]))
    );
}

#[test]
fn an_extra_after_a_list_a_hook_empties_stays_after_its_neighbours() {
    let source = read::document_body(&parsed(&format!(
        "<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:sectPr/><!--c--></w:body>"
    )));
    let transformed = source.transform(&mut copies(&[0, 0]));
    assert_eq!(
        tree_xml(&transformed),
        format!("<w:body xmlns:w=\"{W}\"><w:sectPr/><!--c--></w:body>")
    );
    assert_eq!(
        tree_xml(&transformed),
        written(&source, &mut copies(&[0, 0]))
    );
}

#[test]
fn a_transform_asks_the_hooks_of_a_nodes_children_in_document_order() {
    let source = read::paragraph(&parsed(&format!(
        "<w:p xmlns:w=\"{W}\"><w:pPr/><w:r/></w:p>"
    )));
    let mut record = Record::default();
    let _: Paragraph = source.transform(&mut record);
    assert_eq!(record.0, ["paragraph_properties", "text_run"]);
}

#[test]
fn an_extra_between_list_items_stays_between_their_replacements() {
    // Read, a comment between two blocks joins the block list; built here,
    // it is an `Extra` after the first block.
    let mut source = read::document_body(&parsed(&format!(
        "<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:sectPr/></w:body>"
    )));
    source
        .unknown
        .children
        .push(Extra::new(1, Node::Comment("c".to_owned())));
    let transformed = source.transform(&mut copies(&[3, 2]));
    assert_eq!(
        tree_xml(&transformed),
        format!(
            "<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:p/><!--c--><w:p/><w:p/><w:sectPr/></w:body>"
        )
    );
    assert_eq!(
        tree_xml(&transformed),
        written(&source, &mut copies(&[3, 2]))
    );
}

#[test]
fn an_extra_after_the_blocks_stays_after_a_paragraph_a_hook_gives_for_an_unknown_item() {
    let source = read::document_body(&parsed(&format!(
        "<w:body xmlns:w=\"{W}\"><x:u xmlns:x=\"urn:x\"/><w:p/><w:sectPr/><!--c--></w:body>"
    )));
    assert!(matches!(source.content[0], BlockContent::Other(_)));
    assert_eq!(source.unknown.children[0].after, 2);
    let through = written(&source, &mut ParagraphForOther);
    assert_eq!(
        through,
        format!("<w:body xmlns:w=\"{W}\"><w:p/><w:p/><w:sectPr/><!--c--></w:body>")
    );
    assert_eq!(tree_xml(&source.transform(&mut ParagraphForOther)), through);
}

#[test]
fn an_extra_before_a_child_a_hook_turns_unknown_stays_before_it() {
    let source = read::embedded_object(&parsed(&format!(
        "<w:object xmlns:w=\"{W}\"><w:drawing/><!--c--><w:control/></w:object>"
    )));
    assert_eq!(source.unknown.children[0].after, 1);
    let through = written(&source, &mut CommentForControl);
    assert_eq!(
        through,
        format!("<w:object xmlns:w=\"{W}\"><w:drawing/><!--c--><!--x--></w:object>")
    );
    assert_eq!(
        object_xml(&source.transform(&mut CommentForControl)),
        through
    );
}

#[test]
fn an_extra_after_a_child_a_hook_turns_unknown_stays_after_it() {
    let source = read::embedded_object(&parsed(&format!(
        "<w:object xmlns:w=\"{W}\"><w:drawing/><w:control/><!--c--></w:object>"
    )));
    assert_eq!(source.unknown.children[0].after, 2);
    let through = written(&source, &mut CommentForControl);
    assert_eq!(
        through,
        format!("<w:object xmlns:w=\"{W}\"><w:drawing/><!--x--><!--c--></w:object>")
    );
    assert_eq!(
        object_xml(&source.transform(&mut CommentForControl)),
        through
    );
}

#[test]
fn an_extra_after_the_first_block_stays_there_when_an_unknown_item_before_it_becomes_a_paragraph() {
    // Read, a comment between two blocks joins the block list; built here,
    // it is an `Extra` after the first block.
    let mut source = read::document_body(&parsed(&format!(
        "<w:body xmlns:w=\"{W}\"><x:u xmlns:x=\"urn:x\"/><w:p/><w:p/><w:sectPr/></w:body>"
    )));
    source
        .unknown
        .children
        .push(Extra::new(1, Node::Comment("c".to_owned())));
    let through = written(&source, &mut ParagraphForOther);
    assert_eq!(
        through,
        format!("<w:body xmlns:w=\"{W}\"><w:p/><w:p/><!--c--><w:p/><w:sectPr/></w:body>")
    );
    assert_eq!(tree_xml(&source.transform(&mut ParagraphForOther)), through);
}
