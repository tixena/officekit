#![cfg(feature = "transitional")]

use alloc::collections::BTreeMap;

use crate::parts::kinds::WordprocessingKind;
use crate::parts::{
    endnotes, footnotes, numbering_definitions, style_definitions, wordprocessing_comments,
};
use officekit_markup::chart::{self, ChartSpace, ReferenceToChartPart};
use officekit_markup::drawing::{self, AnyElementContent, Blip, PositiveCoordinate};
use officekit_markup::wordprocessing::{
    self, AbstractNumberingDefinition, AbstractNumberingDefinitionIdentifier,
    AbstractNumberingDefinitionReference, BlockContent, BookmarkEnd, BookmarkStart,
    CommentAnchorRangeEnd, CommentAnchorRangeStart, CommentContent, CommentContentReferenceMark,
    CommentsCollection, ContinuationSeparatorMark, DecimalNumberValue, DeletedRunContent,
    DeletedText, Document, DocumentBody, DocumentEndnotes, DocumentFinalSectionProperties,
    DocumentFootnotes, DrawingMLObject, DrawingObjectContent, EightDigitHexadecimalValue,
    EndnoteContent, EndnoteReference, FootnoteContent, FootnoteEndnoteSeparatorMark,
    FootnoteOrEndnoteType, FootnoteReference, FootnoteReferenceMark, Header, HeaderOrFooterType,
    HeaderReference, Hyperlink, InsertedRunContent, LastReviewedAbstractNumberingDefinition,
    LinkedStyleReference, MeasurementInHalfPoints, NonComplexScriptFontSize,
    NumberingDefinitionInstance, NumberingDefinitionInstanceReferenceNumId,
    NumberingDefinitionInstanceReferenceNumPr, NumberingDefinitions, NumberingFormat,
    NumberingLevelOverrideDefinition, NumberingLevelReference, PageSize, Paragraph,
    ParagraphContent, ParagraphProperties, ParentStyleID, PictureNumberingSymbolDefinition,
    PictureNumberingSymbolDefinitionContent, PictureNumberingSymbolDefinitionReference,
    PrimaryStyleName, ReferencedParagraphStyle, RunProperties, RunPropertiesContent,
    SectionPropertiesContent, StNumberFormat, StyleDefinition, StyleDefinitions,
    StyleForNextParagraph, StyleTypes, Text, TextRun, TextRunContent, TrackedRunContent,
};
use officekit_markup::wordprocessing_drawing::{self, InlineDrawingMLObject};
use officekit_markup::{Descend, Unknown};
use officekit_strict::traversal::Visit;
use officekit_xml::element::{Attribute, Document as XmlDocument, Element, NamespaceDeclaration};
use officekit_xml::name::QName;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{DECLARATION, OpenOptions};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};
use officekit_xml::write::{Layout, Scope, WriteOptions};

use super::super::Docx;
use super::super::image::{PictureSource, inline_picture};

/// The bytes of every image part.
const PNG: &[u8] = b"\x89PNG not really";
const HEADER_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
const MARKUP_COMPATIBILITY: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// The ids a visit meets, by the element that carries them, in document order: a paragraph's
/// `paraId` and `textId` among them.
#[derive(Default)]
struct Ids(BTreeMap<&'static str, Vec<String>>);

impl Ids {
    fn note(&mut self, element: &'static str, value: Option<&str>) {
        if let Some(found) = value {
            self.0.entry(element).or_default().push(found.to_owned());
        }
    }

    /// The ids of `element`, as numbers.
    fn numbers(&self, element: &str) -> Vec<i64> {
        self.of(element)
            .iter()
            .map(|value| value.parse().unwrap())
            .collect()
    }

    /// The ids of `element`.
    fn of(&self, element: &str) -> Vec<String> {
        self.0.get(element).cloned().unwrap_or_default()
    }
}

impl officekit_markup::Visitor for Ids {
    fn chart(&mut self) -> Option<&mut dyn chart::Visitor> {
        Some(self)
    }

    fn drawing(&mut self) -> Option<&mut dyn drawing::Visitor> {
        Some(self)
    }

    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }

    fn wordprocessing_drawing(&mut self) -> Option<&mut dyn wordprocessing_drawing::Visitor> {
        Some(self)
    }
}

impl chart::Visitor for Ids {
    fn reference_to_chart_part(&mut self, node: &ReferenceToChartPart) -> Descend {
        self.note("chart", node.id.as_deref());
        Descend::Yes
    }
}

impl drawing::Visitor for Ids {
    fn blip(&mut self, node: &Blip) -> Descend {
        self.note("blip", node.embed.as_deref());
        Descend::Yes
    }
}

impl wordprocessing::Visitor for Ids {
    fn bookmark_end(&mut self, node: &BookmarkEnd) -> Descend {
        self.note("bookmarkEnd", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn bookmark_start(&mut self, node: &BookmarkStart) -> Descend {
        self.note("bookmarkStart", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn comment_anchor_range_end(&mut self, node: &CommentAnchorRangeEnd) -> Descend {
        self.note("commentRangeEnd", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn comment_anchor_range_start(&mut self, node: &CommentAnchorRangeStart) -> Descend {
        self.note("commentRangeStart", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn comment_content_reference_mark(&mut self, node: &CommentContentReferenceMark) -> Descend {
        self.note("commentReference", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn deleted_run_content(&mut self, node: &DeletedRunContent) -> Descend {
        self.note("del", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn endnote_reference(&mut self, node: &EndnoteReference) -> Descend {
        self.note("endnoteReference", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn footnote_reference(&mut self, node: &FootnoteReference) -> Descend {
        self.note("footnoteReference", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn header_reference(&mut self, node: &HeaderReference) -> Descend {
        self.note("headerReference", node.id.as_deref());
        Descend::Yes
    }

    fn hyperlink(&mut self, node: &Hyperlink) -> Descend {
        self.note("hyperlink", node.id.as_deref());
        Descend::Yes
    }

    fn inserted_run_content(&mut self, node: &InsertedRunContent) -> Descend {
        self.note("ins", node.id.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn numbering_definition_instance_reference_num_id(
        &mut self,
        node: &NumberingDefinitionInstanceReferenceNumId,
    ) -> Descend {
        self.note("numId", node.val.as_ref().map(decimal_text));
        Descend::Yes
    }

    fn paragraph(&mut self, node: &Paragraph) -> Descend {
        self.note("paraId", node.para_id.as_deref());
        self.note("textId", node.text_id.as_deref());
        Descend::Yes
    }
}

impl wordprocessing_drawing::Visitor for Ids {
    fn drawing_object_non_visual_properties(
        &mut self,
        node: &wordprocessing_drawing::DrawingObjectNonVisualProperties,
    ) -> Descend {
        self.note("docPr", node.id.as_ref().map(|id| id.0.as_str()));
        Descend::Yes
    }
}

/// The texts it visits, in document order.
struct Texts(Vec<String>);

impl officekit_markup::Visitor for Texts {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl wordprocessing::Visitor for Texts {
    fn text(&mut self, node: &Text) -> Descend {
        self.0.push(node.value.clone());
        Descend::Yes
    }
}

fn decimal_text(value: &DecimalNumberValue) -> &str {
    value.0.as_str()
}

fn number(value: &str) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_owned())
}

/// The ids under `node`.
fn ids_in<N>(node: &N) -> Ids
where
    N: Visit,
{
    let mut ids = Ids::default();
    node.visit(&mut ids);
    ids
}

/// The ids under `blocks`.
fn ids_of(blocks: &[BlockContent]) -> Ids {
    let mut body = DocumentBody::default();
    body.content = blocks.to_vec();
    ids_in(&body)
}

/// Declare on a fixture root the prefixes its content uses, those the source's roots add as
/// `LibreOffice` writes them when `source` is set, and the prefixes it ignores.
fn declare(unknown: &mut Unknown, source: bool, ignorable: &str) {
    let mut declarations = vec![
        (
            "w",
            "http://schemas.openxmlformats.org/wordprocessingml/2006/main",
        ),
        (
            "r",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships",
        ),
        (
            "wp",
            "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
        ),
        ("a", "http://schemas.openxmlformats.org/drawingml/2006/main"),
        (
            "pic",
            "http://schemas.openxmlformats.org/drawingml/2006/picture",
        ),
        (
            "c",
            "http://schemas.openxmlformats.org/drawingml/2006/chart",
        ),
        ("mc", MARKUP_COMPATIBILITY),
        (
            "w14",
            "http://schemas.microsoft.com/office/word/2010/wordml",
        ),
    ];
    if source {
        declarations.push((
            "wp14",
            "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing",
        ));
        declarations.push((
            "w15",
            "http://schemas.microsoft.com/office/word/2012/wordml",
        ));
    }
    unknown.namespace_declarations.extend(
        declarations
            .into_iter()
            .map(|(prefix, uri)| NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned())),
    );
    if !ignorable.is_empty() {
        unknown.attributes.push(Attribute::new(
            QName::prefixed("mc", "Ignorable", MARKUP_COMPATIBILITY),
            ignorable.to_owned(),
        ));
    }
}

/// The part bytes of `root`.
fn bytes(root: Element) -> Vec<u8> {
    XmlDocument::new(DECLARATION, root).to_xml(WriteOptions::new(Layout::Compact, false))
}

/// A `.docx` whose main document is `document`.
fn docx(document: &Document) -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_write_options(WriteOptions::new(Layout::Compact, false));
    docx.set_main_document(document).unwrap();
    docx
}

/// A main document whose root declares the fixtures' prefixes, ignoring `ignorable`, and whose
/// body holds `blocks`, ending in `section` when given.
fn document_of(
    blocks: Vec<BlockContent>,
    section: Option<DocumentFinalSectionProperties>,
    source: bool,
    ignorable: &str,
) -> Document {
    let mut body = DocumentBody::default();
    body.content = blocks;
    body.document_final_section_properties = section.map(Box::new);
    let mut document = Document::default();
    declare(&mut document.unknown, source, ignorable);
    document.body = Some(Box::new(body));
    document
}

/// Add the part `name`, related from `from` under `id`.
fn add(
    docx: &mut Docx,
    from: &str,
    id: &str,
    (name, content_type, relationship): (&str, &str, RelationshipType),
    content: &[u8],
) {
    let part = PartName::new(name).unwrap();
    let package = docx.package_mut();
    package
        .set_part_bytes(&part, content_type, content)
        .unwrap();
    package
        .add_relationship(
            Some(&PartName::new(from).unwrap()),
            id,
            &Target::Part(part),
            relationship.transitional,
        )
        .unwrap();
}

/// Relate `from` under `id` to an external hyperlink target.
fn link(docx: &mut Docx, from: &str, id: &str, uri: &str) {
    docx.package_mut()
        .add_relationship(
            Some(&PartName::new(from).unwrap()),
            id,
            &Target::External(uri.to_owned()),
            RelationshipType::HYPERLINK.transitional,
        )
        .unwrap();
}

/// Import `source`'s body into `target`; `target`'s main document is not stored.
fn import(target: &mut Docx, source: &Docx) -> (Document, Vec<BlockContent>) {
    let mut document = target.main_document().unwrap();
    let blocks = target.import_body(&mut document, source).unwrap();
    (document, blocks)
}

/// The part the main part relates to first by `relationship`.
fn related_part(docx: &Docx, relationship: &RelationshipType) -> PartName {
    docx.package()
        .related_by_type(Some(docx.main_document_part()), relationship)
        .unwrap()
        .into_iter()
        .find_map(|related| {
            if let Target::Part(part) = related.target {
                Some(part)
            } else {
                None
            }
        })
        .unwrap()
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// The target of the relationship `id` of `from`.
fn target_of(docx: &Docx, from: &str, id: &str) -> Target {
    docx.package()
        .related(Some(&name(from)))
        .unwrap()
        .into_iter()
        .find(|related| related.id == id)
        .unwrap()
        .target
}

/// A run holding `content`.
fn run_of(content: Vec<TextRunContent>) -> TextRun {
    let mut run = TextRun::default();
    run.content = content;
    run
}

/// A run of the text `value`.
fn text_run(value: &str) -> ParagraphContent {
    let mut text = Text::default();
    text.value = value.to_owned();
    ParagraphContent::TextRun(run_of(vec![TextRunContent::Text(text)]))
}

/// A paragraph with `properties`, when given, holding `content`.
fn paragraph(properties: Option<ParagraphProperties>, content: Vec<ParagraphContent>) -> Paragraph {
    let mut paragraph = Paragraph::default();
    paragraph.content = content;
    paragraph.properties = properties.map(Box::new);
    paragraph
}

/// Paragraph properties of the style `style`.
fn styled(style: &str) -> ParagraphProperties {
    let mut reference = ReferencedParagraphStyle::default();
    reference.val = Some(style.to_owned());
    let mut properties = ParagraphProperties::default();
    properties.referenced_paragraph_style = Some(Box::new(reference));
    properties
}

/// Paragraph properties numbering a paragraph by list `list`, at level 0 when `level` is set.
fn numbered(list: &str, level: bool) -> ParagraphProperties {
    let mut id = NumberingDefinitionInstanceReferenceNumId::default();
    id.val = Some(number(list));
    let mut reference = NumberingDefinitionInstanceReferenceNumPr::default();
    reference.numbering_definition_instance_reference_num_id = Some(Box::new(id));
    if level {
        let mut depth = NumberingLevelReference::default();
        depth.val = Some(number("0"));
        reference.numbering_level_reference = Some(Box::new(depth));
    }
    let mut properties = ParagraphProperties::default();
    properties.numbering_definition_instance_reference_num_pr = Some(Box::new(reference));
    properties
}

fn bookmark_start(id: &str, mark: &str) -> ParagraphContent {
    let mut start = BookmarkStart::default();
    start.id = Some(number(id));
    start.name = Some(mark.to_owned());
    ParagraphContent::BookmarkStart(start)
}

fn bookmark_end(id: &str) -> ParagraphContent {
    let mut end = BookmarkEnd::default();
    end.id = Some(number(id));
    ParagraphContent::BookmarkEnd(end)
}

/// A note's one paragraph, whose run holds the separator `mark`.
fn separator_paragraphs(mark: TextRunContent) -> Vec<BlockContent> {
    vec![BlockContent::Paragraph(paragraph(
        None,
        vec![ParagraphContent::TextRun(run_of(vec![mark]))],
    ))]
}

/// A footnotes part holding its separators from `first`, then `notes`.
fn footnotes_of(first: i64, notes: Vec<FootnoteContent>, source: bool) -> Vec<u8> {
    let separator = |kind: &str, id: i64, mark: TextRunContent| {
        let mut note = FootnoteContent::default();
        note.content = separator_paragraphs(mark);
        note.id = Some(number(&id.to_string()));
        note.r#type = Some(FootnoteOrEndnoteType::from(kind.to_owned()));
        note
    };
    let mut part = DocumentFootnotes::default();
    declare(
        &mut part.unknown,
        source,
        if source { "w14 wp14 w15" } else { "" },
    );
    part.footnote_content = vec![
        separator(
            "separator",
            first,
            TextRunContent::FootnoteEndnoteSeparatorMark(FootnoteEndnoteSeparatorMark::default()),
        ),
        separator(
            "continuationSeparator",
            first + 1,
            TextRunContent::ContinuationSeparatorMark(ContinuationSeparatorMark::default()),
        ),
    ];
    part.footnote_content.extend(notes);
    bytes(wordprocessing::write::document_footnotes(
        &part,
        &mut Scope::default(),
    ))
}

/// An endnotes part holding its separators from `first`, then `notes`.
fn endnotes_of(first: i64, notes: Vec<EndnoteContent>, source: bool) -> Vec<u8> {
    let separator = |kind: &str, id: i64, mark: TextRunContent| {
        let mut note = EndnoteContent::default();
        note.content = separator_paragraphs(mark);
        note.id = Some(number(&id.to_string()));
        note.r#type = Some(FootnoteOrEndnoteType::from(kind.to_owned()));
        note
    };
    let mut part = DocumentEndnotes::default();
    declare(
        &mut part.unknown,
        source,
        if source { "w14 wp14 w15" } else { "" },
    );
    part.endnote_content = vec![
        separator(
            "separator",
            first,
            TextRunContent::FootnoteEndnoteSeparatorMark(FootnoteEndnoteSeparatorMark::default()),
        ),
        separator(
            "continuationSeparator",
            first + 1,
            TextRunContent::ContinuationSeparatorMark(ContinuationSeparatorMark::default()),
        ),
    ];
    part.endnote_content.extend(notes);
    bytes(wordprocessing::write::document_endnotes(
        &part,
        &mut Scope::default(),
    ))
}

/// A style of `kind` whose id and name are `id`.
fn style(kind: &str, id: &str) -> StyleDefinition {
    let mut style_name = PrimaryStyleName::default();
    style_name.val = Some(id.to_owned());
    let mut definition = StyleDefinition::default();
    definition.primary_style_name = Some(Box::new(style_name));
    definition.style_id = Some(id.to_owned());
    definition.r#type = Some(StyleTypes::from(kind.to_owned()));
    definition
}

/// `definition` whose run text is `half_points` half points.
fn sized(mut definition: StyleDefinition, half_points: &str) -> StyleDefinition {
    let mut size = NonComplexScriptFontSize::default();
    size.val = Some(MeasurementInHalfPoints::from(half_points.to_owned()));
    let mut properties = RunProperties::default();
    properties.content = vec![RunPropertiesContent::NonComplexScriptFontSize(size)];
    definition.run_properties = Some(Box::new(properties));
    definition
}

/// `definition` based on `parent`.
fn based_on(mut definition: StyleDefinition, parent: &str) -> StyleDefinition {
    let mut reference = ParentStyleID::default();
    reference.val = Some(parent.to_owned());
    definition.parent_style_id = Some(Box::new(reference));
    definition
}

/// `definition` linked to the style `linked`.
fn linked(mut definition: StyleDefinition, linked_style: &str) -> StyleDefinition {
    let mut reference = LinkedStyleReference::default();
    reference.val = Some(linked_style.to_owned());
    definition.linked_style_reference = Some(Box::new(reference));
    definition
}

/// A styles part of `definitions`.
fn styles_of(definitions: Vec<StyleDefinition>, source: bool) -> Vec<u8> {
    let mut styles = StyleDefinitions::default();
    declare(
        &mut styles.unknown,
        source,
        if source { "w14 wp14 w15" } else { "" },
    );
    styles.style_definition = definitions;
    bytes(wordprocessing::write::style_definitions(
        &styles,
        &mut Scope::default(),
    ))
}

/// An abstract numbering `id` of identifier `nsid`, its level 0 in `format`.
fn abstract_numbering(id: &str, nsid: &str, format: &str) -> AbstractNumberingDefinition {
    let mut kind = NumberingFormat::default();
    kind.val = Some(StNumberFormat::from(format.to_owned()));
    let mut level = NumberingLevelOverrideDefinition::default();
    level.ilvl = Some(number("0"));
    level.numbering_format = Some(Box::new(kind));
    let mut identifier = AbstractNumberingDefinitionIdentifier::default();
    identifier.val = Some(EightDigitHexadecimalValue::from(nsid.to_owned()));
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(number(id));
    definition.identifier = Some(Box::new(identifier));
    definition.numbering_level_override_definition = vec![level];
    definition
}

/// List `id` over the abstract numbering `abstract_id`.
fn list(id: &str, abstract_id: &str) -> NumberingDefinitionInstance {
    let mut reference = AbstractNumberingDefinitionReference::default();
    reference.val = Some(number(abstract_id));
    let mut instance = NumberingDefinitionInstance::default();
    instance.abstract_numbering_definition_reference = Box::new(reference);
    instance.num_id = Some(number(id));
    instance
}

/// A numbering part of `definitions`.
fn numbering_bytes(mut definitions: NumberingDefinitions) -> Vec<u8> {
    declare(&mut definitions.unknown, false, "");
    bytes(wordprocessing::write::numbering_definitions(
        &definitions,
        &mut Scope::default(),
    ))
}

/// The part `part` of `docx`, read back through its part module.
fn numbering_of(docx: &Docx, part: &PartName) -> NumberingDefinitions {
    numbering_definitions::load(docx.package(), part, OpenOptions::default()).unwrap()
}

/// The ids of the abstract numberings and of the lists of `numbering`, in order.
fn numbering_ids(numbering: &NumberingDefinitions) -> (Vec<String>, Vec<(String, String)>) {
    (
        numbering
            .abstract_numbering_definition
            .iter()
            .filter_map(|definition| {
                definition
                    .abstract_num_id
                    .as_ref()
                    .map(decimal_text)
                    .map(str::to_owned)
            })
            .collect(),
        numbering
            .numbering_definition_instance
            .iter()
            .filter_map(|instance| {
                Some((
                    instance.num_id.as_ref().map(decimal_text)?.to_owned(),
                    instance
                        .abstract_numbering_definition_reference
                        .val
                        .as_ref()
                        .map(decimal_text)?
                        .to_owned(),
                ))
            })
            .collect(),
    )
}

/// The texts under `node`, in order.
fn texts_in<N>(node: &N) -> Vec<String>
where
    N: Visit,
{
    let mut texts = Texts(Vec::new());
    node.visit(&mut texts);
    texts.0
}

/// A target shaped as `pagare` is for the notes import: footnotes and endnotes holding only their
/// separators, -1 and 0, a `Normal` style, a bookmark, and neither comments nor numbering.
fn notes_target() -> Docx {
    let mut target = docx(&document_of(
        vec![BlockContent::Paragraph(paragraph(
            None,
            vec![
                bookmark_start("0", "own"),
                text_run("Own"),
                bookmark_end("0"),
            ],
        ))],
        Some(DocumentFinalSectionProperties::default()),
        false,
        "w14",
    ));
    let main = "/word/document.xml";
    add(
        &mut target,
        main,
        "rId1",
        (
            "/word/styles.xml",
            style_definitions::CONTENT_TYPE,
            RelationshipType::STYLES,
        ),
        &styles_of(vec![sized(style("paragraph", "Normal"), "22")], false),
    );
    add(
        &mut target,
        main,
        "rId2",
        (
            "/word/footnotes.xml",
            footnotes::CONTENT_TYPE,
            RelationshipType::FOOTNOTES,
        ),
        &footnotes_of(-1, Vec::new(), false),
    );
    add(
        &mut target,
        main,
        "rId3",
        (
            "/word/endnotes.xml",
            endnotes::CONTENT_TYPE,
            RelationshipType::ENDNOTES,
        ),
        &endnotes_of(-1, Vec::new(), false),
    );
    target
}

/// The source's main document: a clause with a footnote and an endnote, a paragraph with a
/// comment, a bookmark and a hyperlink, and a two-item list.
fn notes_source_document() -> Document {
    let mut footnote_reference = FootnoteReference::default();
    footnote_reference.id = Some(number("2"));
    let mut endnote_reference = EndnoteReference::default();
    endnote_reference.id = Some(number("2"));
    let mut clause = paragraph(
        Some(styled("ClausulaImportada")),
        vec![
            text_run("Imported clause"),
            ParagraphContent::TextRun(run_of(vec![TextRunContent::FootnoteReference(
                footnote_reference,
            )])),
            ParagraphContent::TextRun(run_of(vec![TextRunContent::EndnoteReference(
                endnote_reference,
            )])),
        ],
    );
    clause.para_id = Some("1A2B3C4D".to_owned());
    clause.text_id = Some("77777777".to_owned());
    let mut range_start = CommentAnchorRangeStart::default();
    range_start.id = Some(number("0"));
    let mut range_end = CommentAnchorRangeEnd::default();
    range_end.id = Some(number("0"));
    let mut mark = CommentContentReferenceMark::default();
    mark.id = Some(number("0"));
    let mut hyperlink = Hyperlink::default();
    hyperlink.content = vec![text_run("a hyperlink")];
    hyperlink.id = Some("rId2".to_owned());
    let commented = paragraph(
        Some(styled("Normal")),
        vec![
            ParagraphContent::CommentAnchorRangeStart(range_start),
            bookmark_start("0", "imported_mark"),
            text_run("a bookmark"),
            bookmark_end("0"),
            ParagraphContent::CommentAnchorRangeEnd(range_end),
            ParagraphContent::TextRun(run_of(vec![TextRunContent::CommentContentReferenceMark(
                mark,
            )])),
            ParagraphContent::Hyperlink(hyperlink),
        ],
    );
    let item = |value: &str| {
        BlockContent::Paragraph(paragraph(Some(numbered("1", true)), vec![text_run(value)]))
    };
    let mut page = PageSize::default();
    page.h = Some("15840".to_owned().into());
    page.w = Some("12240".to_owned().into());
    let mut section = DocumentFinalSectionProperties::default();
    section.page_size = Some(Box::new(page));
    document_of(
        vec![
            BlockContent::Paragraph(clause),
            BlockContent::Paragraph(commented),
            item("First"),
            item("Second"),
        ],
        Some(section),
        true,
        "w14 wp14 w15",
    )
}

/// The source's styles: `ClausulaImportada` and the styles it names, `FootnoteText`, a `Normal`
/// of its own, and one style nothing names.
fn notes_source_styles() -> Vec<u8> {
    let mut clause_style = linked(
        based_on(style("paragraph", "ClausulaImportada"), "BaseImportada"),
        "ClausulaChar",
    );
    let mut next = StyleForNextParagraph::default();
    next.val = Some("Normal".to_owned());
    clause_style.style_for_next_paragraph = Some(Box::new(next));
    styles_of(
        vec![
            sized(style("paragraph", "Normal"), "24"),
            clause_style,
            based_on(style("paragraph", "BaseImportada"), "Normal"),
            linked(style("character", "ClausulaChar"), "ClausulaImportada"),
            based_on(style("paragraph", "FootnoteText"), "Normal"),
            style("paragraph", "Unused"),
        ],
        true,
    )
}

/// The source's footnote 2, whose text is a hyperlink.
fn source_footnote() -> FootnoteContent {
    let mut note_link = Hyperlink::default();
    note_link.content = vec![text_run("Imported footnote text.")];
    note_link.id = Some("rId1".to_owned());
    let mut footnote = FootnoteContent::default();
    footnote.content = vec![BlockContent::Paragraph(paragraph(
        Some(styled("FootnoteText")),
        vec![
            ParagraphContent::TextRun(run_of(vec![TextRunContent::FootnoteReferenceMark(
                FootnoteReferenceMark::default(),
            )])),
            ParagraphContent::Hyperlink(note_link),
        ],
    ))];
    footnote.id = Some(number("2"));
    footnote
}

/// The source's endnote 2.
fn source_endnote() -> EndnoteContent {
    let mut endnote = EndnoteContent::default();
    endnote.content = vec![BlockContent::Paragraph(paragraph(
        None,
        vec![text_run("Imported endnote text.")],
    ))];
    endnote.id = Some(number("2"));
    endnote
}

/// The source's comments part, holding comment 0.
fn source_comments() -> Vec<u8> {
    let mut comment = CommentContent::default();
    comment.author = Some("Reviewer".to_owned());
    comment.content = vec![BlockContent::Paragraph(paragraph(
        None,
        vec![text_run("Imported comment text.")],
    ))];
    comment.id = Some(number("0"));
    let mut comments = CommentsCollection::default();
    declare(&mut comments.unknown, true, "w14 wp14 w15");
    comments.comment_content = vec![comment];
    bytes(wordprocessing::write::comments_collection(
        &comments,
        &mut Scope::default(),
    ))
}

/// The source's numbering: list 1 over the bullets of abstract numbering 1.
fn source_numbering() -> Vec<u8> {
    let mut numbering = NumberingDefinitions::default();
    numbering.abstract_numbering_definition = vec![abstract_numbering("1", "0A1B2C3D", "bullet")];
    numbering.numbering_definition_instance = vec![list("1", "1")];
    numbering_bytes(numbering)
}

/// A source made as `LibreOffice` makes `notes_source`: a footnote, an endnote, a comment, a
/// bookmark, a hyperlink, a two-item list, and the paragraph style `ClausulaImportada`, with the
/// styles it names.
fn notes_source() -> Docx {
    let mut source = docx(&notes_source_document());
    let main = "/word/document.xml";
    let parts = [
        (
            "rId1",
            (
                "/word/styles.xml",
                style_definitions::CONTENT_TYPE,
                RelationshipType::STYLES,
            ),
            notes_source_styles(),
        ),
        (
            "rId3",
            (
                "/word/footnotes.xml",
                footnotes::CONTENT_TYPE,
                RelationshipType::FOOTNOTES,
            ),
            footnotes_of(0, vec![source_footnote()], true),
        ),
        (
            "rId4",
            (
                "/word/endnotes.xml",
                endnotes::CONTENT_TYPE,
                RelationshipType::ENDNOTES,
            ),
            endnotes_of(0, vec![source_endnote()], true),
        ),
        (
            "rId5",
            (
                "/word/comments.xml",
                wordprocessing_comments::CONTENT_TYPE,
                RelationshipType::COMMENTS,
            ),
            source_comments(),
        ),
        (
            "rId6",
            (
                "/word/numbering.xml",
                numbering_definitions::CONTENT_TYPE,
                RelationshipType::NUMBERING,
            ),
            source_numbering(),
        ),
    ];
    for (id, part, content) in parts {
        add(&mut source, main, id, part, &content);
    }
    link(&mut source, main, "rId2", "https://example.org/imported");
    link(
        &mut source,
        "/word/footnotes.xml",
        "rId1",
        "https://example.org/note",
    );
    source
}

/// The notes source imported into the notes target.
fn notes_import() -> (Docx, Document, Vec<BlockContent>) {
    let mut target = notes_target();
    let (document, blocks) = import(&mut target, &notes_source());
    (target, document, blocks)
}

/// A bare drawing frame of `id` whose graphic data is of `uri`, holding `content`.
fn frame(id: &str, frame_name: &str, uri: &str, content: Vec<AnyElementContent>) -> TextRun {
    let mut inline = InlineDrawingMLObject::default();
    inline.drawing_object_size.cx = Some(PositiveCoordinate::from("1".to_owned()));
    inline.drawing_object_size.cy = Some(PositiveCoordinate::from("1".to_owned()));
    inline.drawing_object_non_visual_properties.id = Some(id.to_owned().into());
    inline.drawing_object_non_visual_properties.name = Some(frame_name.to_owned());
    inline.graphic_object.data.content = content;
    inline.graphic_object.data.uri = Some(uri.to_owned());
    let mut drawing = DrawingMLObject::default();
    drawing.content = vec![DrawingObjectContent::InlineDrawingMLObject(inline)];
    run_of(vec![TextRunContent::DrawingMLObject(drawing)])
}

/// A document shaped as `formulario_cumplimiento_fatca` is for the self import: lists 1 and 2 over
/// abstract numberings 0 and 1, the body using list 1, bookmarks, and drawings.
fn lists() -> Docx {
    let item = |bookmark: &str, drawing: &str| {
        BlockContent::Paragraph(paragraph(
            Some(numbered("1", true)),
            vec![
                bookmark_start(bookmark, &format!("m{bookmark}")),
                text_run("Item"),
                bookmark_end(bookmark),
                ParagraphContent::TextRun(frame(
                    drawing,
                    &format!("Picture {drawing}"),
                    "http://schemas.openxmlformats.org/drawingml/2006/picture",
                    Vec::new(),
                )),
            ],
        ))
    };
    let mut docx = docx(&document_of(
        vec![
            item("3", "4"),
            item("7", "6"),
            BlockContent::Paragraph(paragraph(Some(numbered("0", false)), Vec::new())),
        ],
        Some(DocumentFinalSectionProperties::default()),
        false,
        "",
    ));
    let mut numbering = NumberingDefinitions::default();
    numbering.abstract_numbering_definition = vec![
        abstract_numbering("0", "11111111", "decimal"),
        abstract_numbering("1", "22222222", "decimal"),
    ];
    numbering.numbering_definition_instance = vec![list("1", "0"), list("2", "1")];
    add(
        &mut docx,
        "/word/document.xml",
        "rId1",
        (
            "/word/numbering.xml",
            numbering_definitions::CONTENT_TYPE,
            RelationshipType::NUMBERING,
        ),
        &numbering_bytes(numbering),
    );
    docx
}

/// A main document of one empty paragraph.
fn empty_document() -> Document {
    document_of(
        vec![BlockContent::Paragraph(Paragraph::default())],
        None,
        false,
        "",
    )
}

/// A header part of one empty paragraph.
fn empty_header() -> Vec<u8> {
    let mut header = Header::default();
    declare(&mut header.unknown, false, "");
    header.content = vec![BlockContent::Paragraph(Paragraph::default())];
    bytes(wordprocessing::write::header(
        &header,
        &mut Scope::default(),
    ))
}

/// A target with an image and a header that relates to it.
fn pictures_target() -> Docx {
    let mut target = docx(&empty_document());
    let image = (
        "/word/media/image1.png",
        "image/png",
        RelationshipType::IMAGE,
    );
    add(&mut target, "/word/document.xml", "rId1", image, PNG);
    add(
        &mut target,
        "/word/document.xml",
        "rId2",
        (
            "/word/header1.xml",
            HEADER_CONTENT,
            RelationshipType::HEADER,
        ),
        &empty_header(),
    );
    add(&mut target, "/word/header1.xml", "rId1", image, PNG);
    target
}

/// Section properties whose default header is the relationship `rId6`.
fn headed() -> DocumentFinalSectionProperties {
    let mut reference = HeaderReference::default();
    reference.id = Some("rId6".to_owned());
    reference.r#type = Some(HeaderOrFooterType::from("default".to_owned()));
    let mut section = DocumentFinalSectionProperties::default();
    section.content = vec![SectionPropertiesContent::HeaderReference(reference)];
    section
}

/// A source whose body names one image by two ids, a chart that relates to a workbook and to the
/// same image, and a header that relates to the image too.
fn pictures_source() -> Docx {
    let picture = |id: &str| {
        ParagraphContent::TextRun(run_of(vec![TextRunContent::DrawingMLObject(
            inline_picture(PictureSource::Embedded(id), 1, "p", None, (1, 1)),
        )]))
    };
    let mut chart_reference = ReferenceToChartPart::default();
    chart_reference.id = Some("rId5".to_owned());
    let chart_run = frame(
        "2",
        "c",
        "http://schemas.openxmlformats.org/drawingml/2006/chart",
        vec![AnyElementContent::ReferenceToChartPart(chart_reference)],
    );
    let mut properties = ParagraphProperties::default();
    properties.document_final_section_properties = Some(Box::new(headed()));
    let mut source = docx(&document_of(
        vec![
            BlockContent::Paragraph(paragraph(
                None,
                vec![
                    picture("rId3"),
                    picture("rId4"),
                    ParagraphContent::TextRun(chart_run),
                ],
            )),
            BlockContent::Paragraph(paragraph(Some(properties), Vec::new())),
        ],
        Some(headed()),
        false,
        "",
    ));
    let main = "/word/document.xml";
    let image = (
        "/word/media/image1.png",
        "image/png",
        RelationshipType::IMAGE,
    );
    add(&mut source, main, "rId3", image, PNG);
    add(&mut source, main, "rId4", image, PNG);
    add(
        &mut source,
        main,
        "rId5",
        (
            "/word/charts/chart1.xml",
            "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            RelationshipType::CHART,
        ),
        &bytes(chart::write::chart_space(
            &ChartSpace::default(),
            &mut Scope::default(),
        )),
    );
    add(
        &mut source,
        "/word/charts/chart1.xml",
        "rId7",
        (
            "/word/embeddings/Microsoft_Excel_Worksheet.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            RelationshipType::PACKAGE,
        ),
        b"PK not really",
    );
    add(&mut source, "/word/charts/chart1.xml", "rId9", image, PNG);
    add(
        &mut source,
        main,
        "rId6",
        (
            "/word/header1.xml",
            HEADER_CONTENT,
            RelationshipType::HEADER,
        ),
        &empty_header(),
    );
    add(&mut source, "/word/header1.xml", "rId1", image, PNG);
    source
}

/// The value of the `mc:Ignorable` attribute a root keeps.
fn ignorable(unknown: &Unknown) -> Option<&str> {
    unknown
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == "Ignorable")
        .map(|attribute| attribute.value.as_str())
}

#[test]
fn notes_come_in_after_the_separators_and_a_comment_into_a_new_part() {
    let (target, _, blocks) = notes_import();
    let ids = ids_of(&blocks);
    // The target's notes hold the separators -1 and 0, so the copies take 1.
    assert_eq!(ids.of("footnoteReference"), ["1"]);
    assert_eq!(ids.of("endnoteReference"), ["1"]);
    let footnotes_part = related_part(&target, &RelationshipType::FOOTNOTES);
    let notes = footnotes::load(target.package(), &footnotes_part, OpenOptions::default()).unwrap();
    let footnote = notes
        .footnote_content
        .iter()
        .find(|note| note.id.as_ref().map(decimal_text) == Some("1"))
        .unwrap();
    assert_eq!(texts_in(footnote), ["Imported footnote text."]);
    let endnotes_part = related_part(&target, &RelationshipType::ENDNOTES);
    let endnote = endnotes::load(target.package(), &endnotes_part, OpenOptions::default())
        .unwrap()
        .endnote_content
        .into_iter()
        .find(|note| note.id.as_ref().map(decimal_text) == Some("1"))
        .unwrap();
    assert_eq!(texts_in(&endnote), ["Imported endnote text."]);
    // The footnote's own hyperlink is related again from this file's footnotes part.
    let note_link = ids_in(&notes).of("hyperlink");
    assert_eq!(
        target_of(&target, footnotes_part.as_str(), &note_link[0]),
        Target::External("https://example.org/note".to_owned())
    );
    // No comments part here: one is made, related from the main part, declaring what the
    // source's declares, and the comment keeps id 0.
    assert_eq!(ids.of("commentRangeStart"), ["0"]);
    assert_eq!(ids.of("commentRangeEnd"), ["0"]);
    assert_eq!(ids.of("commentReference"), ["0"]);
    let comments_part = related_part(&target, &RelationshipType::COMMENTS);
    assert_eq!(comments_part.as_str(), "/word/comments1.xml");
    let comments =
        wordprocessing_comments::load(target.package(), &comments_part, OpenOptions::default())
            .unwrap();
    let entries: Vec<(Option<&str>, Option<&str>)> = comments
        .comment_content
        .iter()
        .map(|entry| (entry.id.as_ref().map(decimal_text), entry.author.as_deref()))
        .collect();
    assert_eq!(entries, [(Some("0"), Some("Reviewer"))]);
    assert!(
        comments
            .unknown
            .namespace_declarations
            .iter()
            .any(|declaration| declaration.prefix == "w15"
                && declaration.uri == "http://schemas.microsoft.com/office/word/2012/wordml")
    );
    assert_eq!(ignorable(&comments.unknown), Some("w14 wp14 w15"));
}

#[test]
fn a_hyperlink_names_a_new_external_relationship_of_the_main_part() {
    let (target, _, blocks) = notes_import();
    let hyperlinks = ids_of(&blocks).of("hyperlink");
    let fresh = notes_target();
    assert!(
        !fresh
            .package()
            .related(Some(fresh.main_document_part()))
            .unwrap()
            .iter()
            .any(|related| related.id == hyperlinks[0])
    );
    assert_eq!(
        target_of(&target, "/word/document.xml", &hyperlinks[0]),
        Target::External("https://example.org/imported".to_owned())
    );
}

#[test]
fn a_list_comes_in_as_list_1_over_abstract_numbering_0_in_a_new_part() {
    let (target, _, blocks) = notes_import();
    assert_eq!(ids_of(&blocks).of("numId"), ["1", "1"]);
    let numbering = numbering_of(
        &target,
        &related_part(&target, &RelationshipType::NUMBERING),
    );
    assert_eq!(
        numbering_ids(&numbering),
        (vec!["0".to_owned()], vec![("1".to_owned(), "0".to_owned())])
    );
    let identifier = numbering
        .abstract_numbering_definition
        .first()
        .and_then(|definition| definition.identifier.as_ref()?.val.clone());
    assert_eq!(identifier.map(|value| value.0).as_deref(), Some("0A1B2C3D"));
}

#[test]
fn missing_styles_come_with_the_styles_they_name_and_defined_ones_stay() {
    let (target, _, _) = notes_import();
    let styles = style_definitions::load(
        target.package(),
        &related_part(&target, &RelationshipType::STYLES),
        OpenOptions::default(),
    )
    .unwrap();
    let count = |id: &str| {
        styles
            .style_definition
            .iter()
            .filter(|definition| definition.style_id.as_deref() == Some(id))
            .count()
    };
    for (id, expected) in [
        ("ClausulaImportada", 1),
        ("BaseImportada", 1),
        ("ClausulaChar", 1),
        ("FootnoteText", 1),
        ("Normal", 1),
        ("Unused", 0),
    ] {
        assert_eq!(count(id), expected, "{id}");
    }
    let normal_size = styles
        .style_definition
        .iter()
        .find(|definition| definition.style_id.as_deref() == Some("Normal"))
        .and_then(|definition| definition.run_properties.as_ref())
        .and_then(|properties| {
            properties.content.iter().find_map(|item| {
                if let RunPropertiesContent::NonComplexScriptFontSize(size) = item {
                    size.val.clone()
                } else {
                    None
                }
            })
        });
    assert_eq!(normal_size.map(|size| size.0).as_deref(), Some("22"));
}

#[test]
fn the_root_gains_the_source_root_declarations_and_other_attributes_stay() {
    let (_, document, blocks) = notes_import();
    assert_eq!(ignorable(&document.unknown), Some("w14 wp14 w15"));
    for prefix in ["wp14", "w15"] {
        assert!(
            document
                .unknown
                .namespace_declarations
                .iter()
                .any(|declaration| declaration.prefix == prefix),
            "{prefix}"
        );
    }
    // Four paragraphs, without the final section properties.
    assert_eq!(blocks.len(), 4);
    assert!(
        blocks
            .iter()
            .all(|block| matches!(block, BlockContent::Paragraph(_paragraph)))
    );
    let ids = ids_of(&blocks);
    assert_eq!(ids.of("paraId"), ["1A2B3C4D"]);
    assert_eq!(ids.of("textId"), ["77777777"]);
    // The bookmark takes an id above the target's.
    assert_eq!(ids.of("bookmarkStart"), ["1"]);
    assert_eq!(ids.of("bookmarkEnd"), ["1"]);
}

#[test]
fn a_document_brought_into_itself_gets_a_new_list_and_ids_above_its_own() {
    let mut target = lists();
    let original = ids_in(&target.main_document().unwrap());
    let (_, blocks) = import(&mut target, &lists());
    let ids = ids_of(&blocks);
    // Lists 1 and 2 are over abstract numberings 0 and 1: the source's list 1 becomes list 3 over
    // a copy of its abstract numbering, 2, each after the last entry of its kind; 0 stays.
    assert_eq!(ids.of("numId"), ["3", "3", "0"]);
    let numbering = numbering_of(
        &target,
        &related_part(&target, &RelationshipType::NUMBERING),
    );
    let pairs = |entries: &[(&str, &str)]| -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(id, over)| ((*id).to_owned(), (*over).to_owned()))
            .collect()
    };
    assert_eq!(
        numbering_ids(&numbering),
        (
            ["0", "1", "2"].map(str::to_owned).to_vec(),
            pairs(&[("1", "0"), ("2", "1"), ("3", "2")])
        )
    );
    let copied = numbering
        .abstract_numbering_definition
        .get(2)
        .and_then(|definition| definition.identifier.as_ref()?.val.clone());
    assert_eq!(copied.map(|value| value.0).as_deref(), Some("11111111"));
    // Every bookmark and drawing id sits above every one the file uses; each bookmark pair keeps
    // one id between its start and its end, and each drawing takes its own.
    let highest = |element: &str| original.numbers(element).into_iter().max().unwrap();
    assert_eq!(ids.numbers("bookmarkStart"), ids.numbers("bookmarkEnd"));
    assert!(
        ids.numbers("bookmarkStart")
            .iter()
            .all(|id| *id > highest("bookmarkStart"))
    );
    assert_eq!(ids.numbers("bookmarkStart"), [8, 9]);
    assert!(ids.numbers("docPr").iter().all(|id| *id > highest("docPr")));
    assert_eq!(ids.numbers("docPr"), [7, 8]);
}

#[test]
fn range_ids_start_above_the_notes_parts_and_map_each_value_once() {
    let mut target = docx(&document_of(
        vec![BlockContent::Paragraph(paragraph(
            None,
            vec![bookmark_start("2", "own"), bookmark_end("2")],
        ))],
        None,
        false,
        "",
    ));
    let mut comment = CommentContent::default();
    comment.content = vec![BlockContent::Paragraph(paragraph(
        None,
        vec![bookmark_start("40", "note"), bookmark_end("40")],
    ))];
    comment.id = Some(number("0"));
    let mut comments = CommentsCollection::default();
    declare(&mut comments.unknown, false, "");
    comments.comment_content = vec![comment];
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        (
            "/word/comments.xml",
            wordprocessing_comments::CONTENT_TYPE,
            RelationshipType::COMMENTS,
        ),
        &bytes(wordprocessing::write::comments_collection(
            &comments,
            &mut Scope::default(),
        )),
    );
    let mut inserted = InsertedRunContent::default();
    inserted.author = Some("A".to_owned());
    inserted.content = vec![TrackedRunContent::TextRun(run_of(vec![
        TextRunContent::Text({
            let mut text = Text::default();
            text.value = "x".to_owned();
            text
        }),
    ]))];
    inserted.id = Some(number("5"));
    let mut removed = DeletedText::default();
    removed.value = "y".to_owned();
    let mut deleted = DeletedRunContent::default();
    deleted.author = Some("A".to_owned());
    deleted.content = vec![TrackedRunContent::TextRun(run_of(vec![
        TextRunContent::DeletedText(removed),
    ]))];
    deleted.id = Some(number("0"));
    let source = docx(&document_of(
        vec![BlockContent::Paragraph(paragraph(
            None,
            vec![
                bookmark_start("0", "a"),
                ParagraphContent::InsertedRunContent(inserted),
                bookmark_end("0"),
                ParagraphContent::DeletedRunContent(deleted),
            ],
        ))],
        None,
        false,
        "",
    ));
    let (_, blocks) = import(&mut target, &source);
    let ids = ids_of(&blocks);
    assert_eq!(ids.of("bookmarkStart"), ["41"]);
    assert_eq!(ids.of("bookmarkEnd"), ["41"]);
    assert_eq!(ids.of("ins"), ["42"]);
    assert_eq!(ids.of("del"), ["41"]);
}

#[test]
fn parts_come_under_free_names_with_their_relationships_and_each_once() {
    let mut target = pictures_target();
    let (_, blocks) = import(&mut target, &pictures_source());
    let ids = ids_of(&blocks);
    let image = Target::Part(name("/word/media/image2.png"));
    let embeds = ids.of("blip");
    assert_eq!(embeds.len(), 2);
    assert_ne!(embeds[0], embeds[1]);
    for id in &embeds {
        assert_eq!(target_of(&target, "/word/document.xml", id), image);
    }
    assert_eq!(
        target
            .package()
            .content_type(&name("/word/media/image2.png")),
        Some("image/png")
    );
    // The chart, its workbook, and the header come with their relationships under the same ids,
    // each reaching the one copy of the image.
    let chart_ids = ids.of("chart");
    assert_eq!(
        target_of(&target, "/word/document.xml", &chart_ids[0]),
        Target::Part(name("/word/charts/chart1.xml"))
    );
    assert_eq!(
        target
            .package()
            .content_type(&name("/word/charts/chart1.xml")),
        Some("application/vnd.openxmlformats-officedocument.drawingml.chart+xml")
    );
    assert_eq!(
        target_of(&target, "/word/charts/chart1.xml", "rId7"),
        Target::Part(name("/word/embeddings/Microsoft_Excel_Worksheet1.xlsx"))
    );
    assert_eq!(target_of(&target, "/word/charts/chart1.xml", "rId9"), image);
    let headers = ids.of("headerReference");
    assert_eq!(headers.len(), 1);
    assert_eq!(
        target_of(&target, "/word/document.xml", &headers[0]),
        Target::Part(name("/word/header2.xml"))
    );
    assert_eq!(target_of(&target, "/word/header2.xml", "rId1"), image);
    assert_eq!(
        target
            .package()
            .part_names()
            .unwrap()
            .iter()
            .filter(|part| part.directory() == "/word/media")
            .count(),
        2
    );
    // This file's own header keeps its one relationship.
    assert_eq!(
        target
            .package()
            .related(Some(&name("/word/header1.xml")))
            .unwrap()
            .len(),
        1
    );
    // Drawing ids start above the target's, which has none.
    assert_eq!(ids.numbers("docPr"), [1, 2, 3]);
}

/// The picture bullet `id`, a drawing of the image `embed`.
fn picture_bullet(id: &str, embed: &str) -> PictureNumberingSymbolDefinition {
    let mut definition = PictureNumberingSymbolDefinition::default();
    definition.content = Box::new(PictureNumberingSymbolDefinitionContent::DrawingMLObject(
        inline_picture(PictureSource::Embedded(embed), 9, "b", None, (1, 1)),
    ));
    definition.num_pic_bullet_id = Some(number(id));
    definition
}

/// The abstract numbering `abstract_id`, whose level 0 shows the picture bullet `bullet_id`.
fn picture_list(abstract_id: &str, bullet_id: &str) -> AbstractNumberingDefinition {
    let mut reference = PictureNumberingSymbolDefinitionReference::default();
    reference.val = Some(number(bullet_id));
    let mut level = NumberingLevelOverrideDefinition::default();
    level.ilvl = Some(number("0"));
    level.picture_numbering_symbol_definition_reference = Some(Box::new(reference));
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(number(abstract_id));
    definition.numbering_level_override_definition = vec![level];
    definition
}

#[test]
fn a_picture_bullet_and_its_image_come_with_the_list() {
    let image = (
        "/word/media/image1.png",
        "image/png",
        RelationshipType::IMAGE,
    );
    let numbering_part = (
        "/word/numbering.xml",
        numbering_definitions::CONTENT_TYPE,
        RelationshipType::NUMBERING,
    );
    let mut target = docx(&empty_document());
    let mut cleanup = LastReviewedAbstractNumberingDefinition::default();
    cleanup.val = Some(number("1"));
    let mut target_numbering = NumberingDefinitions::default();
    target_numbering.abstract_numbering_definition = vec![picture_list("0", "0")];
    target_numbering.last_reviewed_abstract_numbering_definition = Some(Box::new(cleanup));
    target_numbering.numbering_definition_instance = vec![list("1", "0")];
    target_numbering.picture_numbering_symbol_definition = vec![picture_bullet("0", "rId1")];
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        numbering_part,
        &numbering_bytes(target_numbering),
    );
    add(&mut target, "/word/numbering.xml", "rId1", image, PNG);
    let mut source = docx(&document_of(
        vec![BlockContent::Paragraph(paragraph(
            Some(numbered("4", true)),
            Vec::new(),
        ))],
        None,
        false,
        "",
    ));
    let mut source_numbering = NumberingDefinitions::default();
    source_numbering.abstract_numbering_definition = vec![picture_list("6", "5")];
    source_numbering.numbering_definition_instance = vec![list("4", "6")];
    source_numbering.picture_numbering_symbol_definition = vec![picture_bullet("5", "rId3")];
    add(
        &mut source,
        "/word/document.xml",
        "rId1",
        numbering_part,
        &numbering_bytes(source_numbering),
    );
    add(&mut source, "/word/numbering.xml", "rId3", image, PNG);
    let (_, blocks) = import(&mut target, &source);
    assert_eq!(ids_of(&blocks).of("numId"), ["2"]);
    let numbering_part_name = related_part(&target, &RelationshipType::NUMBERING);
    let numbering = numbering_of(&target, &numbering_part_name);
    let bullets: Vec<&str> = numbering
        .picture_numbering_symbol_definition
        .iter()
        .filter_map(|definition| definition.num_pic_bullet_id.as_ref().map(decimal_text))
        .collect();
    assert_eq!(bullets, ["0", "1"]);
    let (abstract_ids, lists_held) = numbering_ids(&numbering);
    assert_eq!(abstract_ids, ["0", "1"]);
    assert_eq!(
        lists_held,
        [
            ("1".to_owned(), "0".to_owned()),
            ("2".to_owned(), "1".to_owned())
        ]
    );
    assert_eq!(
        numbering
            .last_reviewed_abstract_numbering_definition
            .as_ref()
            .and_then(|reviewed| reviewed.val.as_ref().map(decimal_text)),
        Some("1")
    );
    let copied_bullet = numbering
        .abstract_numbering_definition
        .get(1)
        .and_then(|definition| definition.numbering_level_override_definition.first())
        .and_then(|level| level.picture_numbering_symbol_definition_reference.as_ref())
        .and_then(|reference| reference.val.as_ref().map(decimal_text));
    assert_eq!(copied_bullet, Some("1"));
    // The bullet's image is related from this file's numbering part.
    let embeds = ids_in(&numbering).of("blip");
    assert_eq!(embeds[0], "rId1");
    assert_eq!(
        target_of(&target, numbering_part_name.as_str(), &embeds[1]),
        Target::Part(name("/word/media/image2.png"))
    );
}

#[test]
fn a_prefix_bound_to_another_namespace_is_a_read_error_and_changes_nothing() {
    let bound = |uris: &[(&str, &str)]| {
        let mut document = empty_document();
        document
            .unknown
            .namespace_declarations
            .extend(uris.iter().map(|(prefix, uri)| {
                NamespaceDeclaration::new((*prefix).to_owned(), (*uri).to_owned())
            }));
        document
    };
    let mut target = docx(&bound(&[("x", "urn:example:one")]));
    let source = docx(&bound(&[
        ("x", "urn:example:two"),
        ("y", "urn:example:three"),
    ]));
    let mut document = target.main_document().unwrap();
    let declarations = document.unknown.namespace_declarations.clone();
    let error = target.import_body(&mut document, &source).unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part, reason } if part.as_str() == "/word/document.xml" && reason.contains("urn:example:two")),
        "{error:?}"
    );
    assert_eq!(document.unknown.namespace_declarations, declarations);
}

#[test]
fn a_styles_part_whose_root_binds_a_prefix_otherwise_is_a_read_error() {
    let styles = |uri: &str, definitions: Vec<StyleDefinition>| {
        let mut held = StyleDefinitions::default();
        declare(&mut held.unknown, false, "");
        held.unknown
            .namespace_declarations
            .push(NamespaceDeclaration::new("x".to_owned(), uri.to_owned()));
        held.style_definition = definitions;
        bytes(wordprocessing::write::style_definitions(
            &held,
            &mut Scope::default(),
        ))
    };
    let styles_part = (
        "/word/styles.xml",
        style_definitions::CONTENT_TYPE,
        RelationshipType::STYLES,
    );
    let mut target = docx(&empty_document());
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        styles_part,
        &styles("urn:example:one", Vec::new()),
    );
    let mut source = docx(&document_of(
        vec![BlockContent::Paragraph(paragraph(
            Some(styled("Other")),
            Vec::new(),
        ))],
        None,
        false,
        "",
    ));
    let mut other = StyleDefinition::default();
    other.style_id = Some("Other".to_owned());
    add(
        &mut source,
        "/word/document.xml",
        "rId1",
        styles_part,
        &styles("urn:example:two", vec![other]),
    );
    let mut document = target.main_document().unwrap();
    let error = target.import_body(&mut document, &source).unwrap_err();
    assert!(
        matches!(&error, PackageError::Read { part, reason } if part.as_str() == "/word/styles.xml" && reason.contains("urn:example:two")),
        "{error:?}"
    );
}

#[test]
fn a_target_without_the_entry_parts_gets_only_the_ones_entries_go_into() {
    let mut target = docx(&empty_document());
    let before = target.package().part_names().unwrap();
    let (_, blocks) = import(&mut target, &lists());
    assert_eq!(ids_of(&blocks).of("numId"), ["1", "1", "0"]);
    assert_eq!(
        target.package().part_names().unwrap().len(),
        before.len() + 1
    );
    let numbering_part = related_part(&target, &RelationshipType::NUMBERING);
    assert_eq!(numbering_part.as_str(), "/word/numbering1.xml");
    assert_eq!(
        target.package().content_type(&numbering_part),
        Some(numbering_definitions::CONTENT_TYPE)
    );
    assert_eq!(
        numbering_ids(&numbering_of(&target, &numbering_part)),
        (vec!["0".to_owned()], vec![("1".to_owned(), "0".to_owned())])
    );
}
