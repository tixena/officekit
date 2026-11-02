use alloc::collections::BTreeSet;

use officekit_markup::Descend;
use officekit_markup::chart::{
    ChartSpace, ChartText, ChartTextContent, RichText, Title, read as chart_read,
    write as chart_write,
};
use officekit_markup::drawing::{
    self, TextParagraphs, TextParagraphsContent, TextRun as DrawingRun, TextString,
};
use officekit_markup::presentation::{
    CommonSlideData, GroupShapeContent, NotesSlide, Presentation, PresentationSlideSlide, Shape,
    ShapeTextBody, read as presentation_read, write as presentation_write,
};
use officekit_markup::spreadsheet::{
    self, Comment, CommentText, Comments as SpreadsheetComments, FontSize, Numeric,
    PivotCacheRecord, PivotCacheRecordContent, PivotCacheRecords, RichTextRun, SharedStringTable,
    StringItem, Text as SpreadsheetText, Workbook, read as spreadsheet_read,
    write as spreadsheet_write,
};
use officekit_markup::wordprocessing::{
    self, BlockContent, Bold, BookmarkStart, Break, BreakTypes, CarriageReturn,
    CommentAnchorRangeEnd, CommentAnchorRangeStart, CommentContentReferenceMark,
    DecimalNumberValue, DoNotCheckSpellingOrGrammar, Document, DocumentBody, DocumentFootnotes,
    EightDigitHexadecimalValue, Footer, FootnoteContent, Header, Italics,
    NonBreakingHyphenCharacter, OptionalHyphenCharacter, Paragraph, ParagraphContent,
    PositionOfLastCalculatedPageBreak, ProofingErrorAnchor, ProofingErrorType,
    ReferencedCharacterStyle, RunProperties, RunPropertiesContent, TabCharacter, Text, TextRun,
    TextRunContent, read, write,
};
use officekit_strict::traversal::Visit;
use officekit_xml::element::{Document as XmlDocument, Element};
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, Scope, WriteOptions};

use super::{Comments, Compaction, PartCompaction, compact_package, compact_tree};

const A: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const P: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const R: &str = r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const V: &str = r#"xmlns:v="urn:schemas-microsoft-com:vml""#;
pub(super) const W: &str =
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

const CHART: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
const FOOTER: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml";
const FOOTNOTES: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml";
pub(super) const HEADER: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
pub(super) const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const NOTES_SLIDE: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml";
const PRESENTATION: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml";
const SHARED_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
const SHEET_COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
const SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const STYLES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const WORKBOOK: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";

/// What a Word paragraph of [`fragmented`] holds once compacted.
const MERGED: &str = r#"[] "ab""#;

const KEEP: WriteOptions = WriteOptions::new(Layout::Compact, false);

/// What each paragraph it visits holds: a Word paragraph by its [`shape`], and a `DrawingML`
/// paragraph or a `SpreadsheetML` rich text by the texts of its runs.
struct Runs(Vec<String>);

impl officekit_markup::Visitor for Runs {
    fn drawing(&mut self) -> Option<&mut dyn drawing::Visitor> {
        Some(self)
    }

    fn spreadsheet(&mut self) -> Option<&mut dyn spreadsheet::Visitor> {
        Some(self)
    }

    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl drawing::Visitor for Runs {
    fn text_paragraphs(&mut self, node: &TextParagraphs) -> Descend {
        let texts: Vec<String> = node
            .content
            .iter()
            .filter_map(|item| {
                if let TextParagraphsContent::TextRun(found) = item {
                    Some(format!("{:?}", found.text_string.value))
                } else {
                    None
                }
            })
            .collect();
        self.0.push(texts.join(" "));
        Descend::Yes
    }
}

impl spreadsheet::Visitor for Runs {
    fn comment_text(&mut self, node: &CommentText) -> Descend {
        self.0.push(rich_text(&node.rich_text_run));
        Descend::Yes
    }

    fn string_item(&mut self, node: &StringItem) -> Descend {
        self.0.push(rich_text(&node.rich_text_run));
        Descend::Yes
    }
}

impl wordprocessing::Visitor for Runs {
    fn paragraph(&mut self, node: &Paragraph) -> Descend {
        self.0.push(shape(&node.content).join(", "));
        Descend::Yes
    }
}

/// Compact `xml` and write it back compact, with what the compaction did.
fn compacted(xml: &str, comments: Comments) -> (String, PartCompaction) {
    let mut document = parse(xml.as_bytes()).unwrap();
    let done = compact_tree(&mut document.root, comments);
    (String::from_utf8(document.to_xml(KEEP)).unwrap(), done)
}

fn merged(count: usize) -> PartCompaction {
    PartCompaction {
        merged: count,
        removed_comments: BTreeSet::new(),
    }
}

/// Text of `value` with no `xml:space`.
pub(super) fn plain_text(value: &str) -> TextRunContent {
    let mut text = Text::default();
    text.value = value.to_owned();
    TextRunContent::Text(text)
}

/// Text of `value` whose spaces are kept.
fn kept_text(value: &str) -> TextRunContent {
    let mut text = Text::default();
    text.space = Some("preserve".to_owned());
    text.value = value.to_owned();
    TextRunContent::Text(text)
}

/// A run with the run properties `marks`, when there are any, holding `content`.
pub(super) fn run(marks: Vec<RunPropertiesContent>, content: Vec<TextRunContent>) -> TextRun {
    let mut run = TextRun::default();
    run.content = content;
    if !marks.is_empty() {
        let mut properties = RunProperties::default();
        properties.content = marks;
        run.properties = Some(Box::new(properties));
    }
    run
}

/// A run of no properties holding the text `value`.
pub(super) fn plain_run(value: &str) -> ParagraphContent {
    ParagraphContent::TextRun(run(Vec::new(), vec![plain_text(value)]))
}

pub(super) fn bold() -> RunPropertiesContent {
    RunPropertiesContent::Bold(Bold::default())
}

fn italic() -> RunPropertiesContent {
    RunPropertiesContent::Italics(Italics::default())
}

fn number(value: &str) -> DecimalNumberValue {
    DecimalNumberValue::from(value.to_owned())
}

/// A proofing anchor of `kind`.
fn proofing(kind: &str) -> ParagraphContent {
    let mut anchor = ProofingErrorAnchor::default();
    anchor.r#type = Some(ProofingErrorType::from(kind.to_owned()));
    ParagraphContent::ProofingErrorAnchor(anchor)
}

/// The start of the range of comment `id`.
pub(super) fn comment_start(id: &str) -> ParagraphContent {
    let mut start = CommentAnchorRangeStart::default();
    start.id = Some(number(id));
    ParagraphContent::CommentAnchorRangeStart(start)
}

/// The end of the range of comment `id`.
pub(super) fn comment_end(id: &str) -> ParagraphContent {
    let mut end = CommentAnchorRangeEnd::default();
    end.id = Some(number(id));
    ParagraphContent::CommentAnchorRangeEnd(end)
}

/// The run Word writes for the reference mark of comment `id`, in the `CommentReference` style.
pub(super) fn comment_reference(id: &str) -> ParagraphContent {
    let mut style = ReferencedCharacterStyle::default();
    style.val = Some("CommentReference".to_owned());
    let mut mark = CommentContentReferenceMark::default();
    mark.id = Some(number(id));
    ParagraphContent::TextRun(run(
        vec![RunPropertiesContent::ReferencedCharacterStyle(style)],
        vec![TextRunContent::CommentContentReferenceMark(mark)],
    ))
}

/// A paragraph holding `content`.
pub(super) fn paragraph(content: Vec<ParagraphContent>) -> Paragraph {
    let mut paragraph = Paragraph::default();
    paragraph.content = content;
    paragraph
}

/// `paragraph` written, compacted with `comments` and read back, with what the compaction did.
fn compact_paragraph(paragraph: &Paragraph, comments: Comments) -> (Paragraph, PartCompaction) {
    let mut element = write::paragraph(paragraph, &mut Scope::default());
    let done = compact_tree(&mut element, comments);
    (read::paragraph(&element), done)
}

/// `body` written, compacted with `comments` and read back, with what the compaction did.
fn compact_body(body: &DocumentBody, comments: Comments) -> (DocumentBody, PartCompaction) {
    let mut element = write::document_body(body, &mut Scope::default());
    let done = compact_tree(&mut element, comments);
    (read::document_body(&element), done)
}

/// The paragraph `block` holds; `None` for any other block.
fn as_paragraph(block: &BlockContent) -> Option<&Paragraph> {
    if let BlockContent::Paragraph(found) = block {
        Some(found)
    } else {
        None
    }
}

/// What a run's mark is: `b`, `i`, `style` or `noProof`.
fn mark(property: &RunPropertiesContent) -> &'static str {
    if let RunPropertiesContent::Bold(_bold) = property {
        "b"
    } else if let RunPropertiesContent::Italics(_italics) = property {
        "i"
    } else if let RunPropertiesContent::ReferencedCharacterStyle(_style) = property {
        "style"
    } else if let RunPropertiesContent::DoNotCheckSpellingOrGrammar(_no_proof) = property {
        "noProof"
    } else {
        "other mark"
    }
}

/// What one item of a run is: a text by its value, `kept` when its spaces are, and anything
/// else by a short name.
fn item(content: &TextRunContent) -> String {
    if let TextRunContent::Text(text) = content {
        let kept = if text.space.is_some() { " kept" } else { "" };
        format!("{:?}{kept}", text.value)
    } else if let TextRunContent::TabCharacter(_tab) = content {
        "tab".to_owned()
    } else if let TextRunContent::Break(found) = content {
        format!(
            "br {}",
            found.r#type.as_ref().map_or("", |kind| kind.0.as_str())
        )
    } else if let TextRunContent::CarriageReturn(_cr) = content {
        "cr".to_owned()
    } else if let TextRunContent::NonBreakingHyphenCharacter(_hyphen) = content {
        "noBreakHyphen".to_owned()
    } else if let TextRunContent::OptionalHyphenCharacter(_hyphen) = content {
        "softHyphen".to_owned()
    } else if let TextRunContent::PositionOfLastCalculatedPageBreak(_page_break) = content {
        "lastRenderedPageBreak".to_owned()
    } else if let TextRunContent::CommentContentReferenceMark(found) = content {
        format!("reference {}", ids(found.id.as_ref()))
    } else {
        "other".to_owned()
    }
}

fn ids(id: Option<&DecimalNumberValue>) -> &str {
    id.map_or("", |found| found.0.as_str())
}

/// What `content` holds, one entry per node: a run as its marks and items, and any other node by
/// a short name.
fn shape(content: &[ParagraphContent]) -> Vec<String> {
    content
        .iter()
        .map(|node| {
            if let ParagraphContent::TextRun(found) = node {
                let marks: Vec<&str> = found
                    .properties
                    .iter()
                    .flat_map(|properties| properties.content.iter().map(mark))
                    .collect();
                let items: Vec<String> = found.content.iter().map(item).collect();
                format!("[{}] {}", marks.join(" "), items.join(" "))
            } else if let ParagraphContent::ProofingErrorAnchor(anchor) = node {
                format!(
                    "proofErr {}",
                    anchor.r#type.as_ref().map_or("", |kind| kind.0.as_str())
                )
            } else if let ParagraphContent::CommentAnchorRangeStart(start) = node {
                format!("commentRangeStart {}", ids(start.id.as_ref()))
            } else if let ParagraphContent::CommentAnchorRangeEnd(end) = node {
                format!("commentRangeEnd {}", ids(end.id.as_ref()))
            } else if let ParagraphContent::BookmarkStart(_bookmark) = node {
                "bookmarkStart".to_owned()
            } else {
                "other".to_owned()
            }
        })
        .collect()
}

#[test]
fn the_fox_paragraph_becomes_one_run() {
    let mut first = run(vec![bold()], vec![kept_text("the ")]);
    first.rsid_r = Some(EightDigitHexadecimalValue::from("00A1".to_owned()));
    let no_proof =
        RunPropertiesContent::DoNotCheckSpellingOrGrammar(DoNotCheckSpellingOrGrammar::default());
    let source = paragraph(vec![
        ParagraphContent::TextRun(first),
        ParagraphContent::TextRun(run(vec![bold(), no_proof], vec![kept_text("fox ")])),
        proofing("spellStart"),
        ParagraphContent::TextRun(run(vec![bold()], vec![plain_text("jumped")])),
        proofing("spellEnd"),
        ParagraphContent::TextRun(run(vec![bold()], vec![kept_text(" over the lazy dog")])),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(3));
    assert_eq!(
        shape(&compacted.content),
        [r#"[b] "the fox jumped over the lazy dog""#]
    );
    let rsid = compacted.content.first().and_then(|node| {
        if let ParagraphContent::TextRun(found) = node {
            found.rsid_r.clone()
        } else {
            None
        }
    });
    assert_eq!(rsid.map(|value| value.0), Some("00A1".to_owned()));
}

#[test]
fn runs_with_different_formatting_stay_apart() {
    let source = paragraph(vec![
        ParagraphContent::TextRun(run(vec![bold()], vec![plain_text("bold")])),
        plain_run("plain"),
        proofing("gramStart"),
        ParagraphContent::TextRun(run(vec![italic()], vec![plain_text("italic")])),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(0));
    assert_eq!(
        shape(&compacted.content),
        [
            r#"[b] "bold""#,
            r#"[] "plain""#,
            "proofErr gramStart",
            r#"[i] "italic""#
        ]
    );
}

#[test]
fn every_other_node_between_two_runs_ends_the_stretch() {
    let barriers = [
        r#"<w:bookmarkStart w:id="0" w:name="mark"/>"#,
        r#"<w:bookmarkEnd w:id="0"/>"#,
        r#"<w:permStart w:id="1" w:edGrp="everyone"/>"#,
        r#"<w:permEnd w:id="1"/>"#,
        r#"<w:ins w:id="2" w:author="A"><w:r><w:t>in</w:t></w:r></w:ins>"#,
        r#"<w:del w:id="3" w:author="A"><w:r><w:delText>out</w:delText></w:r></w:del>"#,
        r#"<w:hyperlink r:id="rId1"><w:r><w:t>link</w:t></w:r></w:hyperlink>"#,
        "<w:sdt><w:sdtContent><w:r><w:t>control</w:t></w:r></w:sdtContent></w:sdt>",
        r#"<w:fldSimple w:instr="PAGE"><w:r><w:t>1</w:t></w:r></w:fldSimple>"#,
        r#"<w:smartTag w:element="place"><w:r><w:t>tag</w:t></w:r></w:smartTag>"#,
        r#"<w:customXml w:element="item"><w:r><w:t>custom</w:t></w:r></w:customXml>"#,
        r#"<w:moveFromRangeStart w:id="4" w:author="A" w:name="move"/>"#,
        r#"<w:moveToRangeEnd w:id="5"/>"#,
        r#"<m:oMath xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math"><m:r><m:t>x</m:t></m:r></m:oMath>"#,
        r#"<mc:AlternateContent><mc:Choice Requires="w"><w:r><w:t>choice</w:t></w:r></mc:Choice></mc:AlternateContent>"#,
        r#"<w:commentRangeStart w:id="6"/>"#,
        "<w:unknown/>",
        "<!-- a comment -->",
        "text",
    ];
    for barrier in barriers {
        let xml = format!(
            "<w:p {W} {R} {MC}><w:r><w:t>a</w:t></w:r>{barrier}<w:r><w:t>b</w:t></w:r></w:p>"
        );
        assert_eq!(
            compacted(&xml, Comments::Keep),
            (xml.clone(), merged(0)),
            "{barrier}"
        );
    }
}

#[test]
fn a_run_holding_more_than_text_is_left_as_it_is_and_ends_the_stretch() {
    let contents = [
        r#"<w:fldChar w:fldCharType="begin"/>"#,
        r#"<w:instrText xml:space="preserve"> PAGE </w:instrText>"#,
        "<w:drawing/>",
        "<w:pict/>",
        "<w:object/>",
        r#"<w:sym w:font="Wingdings" w:char="F0E0"/>"#,
        r#"<w:ptab w:relativeTo="margin" w:alignment="right" w:leader="none"/>"#,
        r#"<w:footnoteReference w:id="1"/>"#,
        r#"<w:endnoteReference w:id="1"/>"#,
        r#"<w:commentReference w:id="1"/>"#,
        r#"<mc:AlternateContent><mc:Choice Requires="w"><w:t>c</w:t></mc:Choice></mc:AlternateContent>"#,
        "<w:unknown/>",
        "<w:t>c</w:t><w:t>d</w:t><w:unknown/>",
    ];
    for content in contents {
        let xml = format!(
            "<w:p {W} {MC}><w:r><w:t>a</w:t></w:r><w:r>{content}</w:r><w:r><w:t>b</w:t></w:r></w:p>"
        );
        assert_eq!(
            compacted(&xml, Comments::Keep),
            (xml.clone(), merged(0)),
            "{content}"
        );
    }
}

#[test]
fn text_content_merges_in_order_and_neighbouring_text_joins() {
    let mut page = Break::default();
    page.r#type = Some(BreakTypes::from("page".to_owned()));
    let source = paragraph(vec![
        ParagraphContent::TextRun(run(
            Vec::new(),
            vec![
                plain_text("a"),
                TextRunContent::TabCharacter(TabCharacter::default()),
            ],
        )),
        plain_run("b"),
        ParagraphContent::TextRun(run(
            Vec::new(),
            vec![
                plain_text("c"),
                TextRunContent::Break(page),
                TextRunContent::CarriageReturn(CarriageReturn::default()),
                TextRunContent::NonBreakingHyphenCharacter(NonBreakingHyphenCharacter::default()),
                TextRunContent::OptionalHyphenCharacter(OptionalHyphenCharacter::default()),
                TextRunContent::PositionOfLastCalculatedPageBreak(
                    PositionOfLastCalculatedPageBreak::default(),
                ),
            ],
        )),
        plain_run("d"),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(3));
    assert_eq!(
        shape(&compacted.content),
        [r#"[] "a" tab "bc" br page cr noBreakHyphen softHyphen lastRenderedPageBreak "d""#]
    );
}

#[test]
fn joined_text_preserves_its_space_exactly_when_it_starts_or_ends_with_white_space() {
    let trailing = paragraph(vec![
        plain_run("one"),
        ParagraphContent::TextRun(run(Vec::new(), vec![kept_text(" two ")])),
    ]);
    assert_eq!(
        shape(&compact_paragraph(&trailing, Comments::Keep).0.content),
        [r#"[] "one two " kept"#]
    );
    let leading = paragraph(vec![
        ParagraphContent::TextRun(run(Vec::new(), vec![kept_text("one ")])),
        plain_run("two"),
    ]);
    assert_eq!(
        shape(&compact_paragraph(&leading, Comments::Keep).0.content),
        [r#"[] "one two""#]
    );
}

#[test]
fn text_whose_white_space_a_consumer_may_trim_is_not_joined() {
    let source = paragraph(vec![plain_run("one"), plain_run(" two")]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(1));
    assert_eq!(shape(&compacted.content), [r#"[] "one" " two""#]);
}

#[test]
fn moved_text_keeps_the_white_space_its_own_run_gave_it() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:t>one</w:t></w:r><w:r xml:space="preserve"><w:tab/><w:t> two</w:t></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep).0,
        format!(
            r#"<w:p {W}><w:r><w:t>one</w:t><w:tab/><w:t xml:space="preserve"> two</w:t></w:r></w:p>"#
        )
    );
}

#[test]
fn moved_content_keeps_the_declarations_of_its_own_run() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:t>one</w:t></w:r><w:r xmlns:e="urn:extra"><w:br e:kind="soft"/></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep).0,
        format!(
            r#"<w:p {W}><w:r><w:t>one</w:t><w:br xmlns:e="urn:extra" e:kind="soft"/></w:r></w:p>"#
        )
    );
}

#[test]
fn a_proofing_anchor_stays_unless_the_runs_around_it_merge() {
    let source = paragraph(vec![
        ParagraphContent::TextRun(run(vec![bold()], vec![plain_text("a")])),
        proofing("spellStart"),
        plain_run("b"),
        proofing("spellEnd"),
        plain_run("c"),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(1));
    assert_eq!(
        shape(&compacted.content),
        [r#"[b] "a""#, "proofErr spellStart", r#"[] "bc""#]
    );
}

#[test]
fn a_kept_comment_keeps_its_runs_apart() {
    let source = paragraph(vec![
        plain_run("a"),
        comment_start("7"),
        plain_run("b"),
        comment_end("7"),
        comment_reference("7"),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Keep);
    assert_eq!(done, merged(0));
    assert_eq!(
        shape(&compacted.content),
        [
            r#"[] "a""#,
            "commentRangeStart 7",
            r#"[] "b""#,
            "commentRangeEnd 7",
            "[style] reference 7"
        ]
    );
}

#[test]
fn a_removed_comment_loses_every_node_of_it_in_the_tree() {
    let mut bookmark = BookmarkStart::default();
    bookmark.id = Some(number("0"));
    bookmark.name = Some("m".to_owned());
    let mut referenced = run(Vec::new(), vec![plain_text("e")]);
    let mut mark = CommentContentReferenceMark::default();
    mark.id = Some(number("7"));
    referenced
        .content
        .push(TextRunContent::CommentContentReferenceMark(mark));
    let mut body = DocumentBody::default();
    body.content = [
        vec![plain_run("a"), comment_start("7"), plain_run("b")],
        vec![
            plain_run("c"),
            comment_end("7"),
            comment_reference("7"),
            ParagraphContent::BookmarkStart(bookmark),
            comment_start("8"),
            ParagraphContent::TextRun(run(vec![italic()], vec![plain_text("d")])),
            comment_end("8"),
            comment_reference("8"),
        ],
        vec![ParagraphContent::TextRun(referenced)],
    ]
    .into_iter()
    .map(|content| BlockContent::Paragraph(paragraph(content)))
    .collect();
    let (compacted, done) = compact_body(&body, Comments::Remove);
    let paragraphs: Vec<Vec<String>> = compacted
        .content
        .iter()
        .filter_map(as_paragraph)
        .map(|found| shape(&found.content))
        .collect();
    assert_eq!(
        paragraphs,
        [
            vec![r#"[] "ab""#.to_owned()],
            vec![
                r#"[] "c""#.to_owned(),
                "bookmarkStart".to_owned(),
                "commentRangeStart 8".to_owned(),
                r#"[i] "d""#.to_owned(),
                "commentRangeEnd 8".to_owned(),
                "[style] reference 8".to_owned(),
            ],
            vec![r#"[] "e""#.to_owned()],
        ]
    );
    assert_eq!(done.merged, 1);
    assert_eq!(done.removed_comments, BTreeSet::from(["7".to_owned()]));
}

#[test]
fn a_comment_between_runs_that_do_not_merge_stays_under_remove() {
    let source = paragraph(vec![
        ParagraphContent::TextRun(run(vec![bold()], vec![plain_text("a")])),
        comment_start("7"),
        plain_run("b"),
        comment_end("7"),
        comment_reference("7"),
    ]);
    let (compacted, done) = compact_paragraph(&source, Comments::Remove);
    assert_eq!(done, merged(0));
    assert_eq!(
        shape(&compacted.content),
        [
            r#"[b] "a""#,
            "commentRangeStart 7",
            r#"[] "b""#,
            "commentRangeEnd 7",
            "[style] reference 7"
        ]
    );
}

#[test]
fn runs_merge_inside_every_container_and_never_across_two() {
    let xml = format!(
        r#"<w:body {W} {R} {MC} {V}><w:p><w:r><w:t>a</w:t></w:r><w:hyperlink r:id="rId1"><w:r><w:t>b</w:t></w:r><w:r><w:t>c</w:t></w:r></w:hyperlink><w:r><w:t>d</w:t></w:r><w:ins w:id="1" w:author="A"><w:r><w:t>e</w:t></w:r><w:r><w:t>f</w:t></w:r></w:ins><w:sdt><w:sdtContent><w:r><w:t>g</w:t></w:r><w:r><w:t>h</w:t></w:r></w:sdtContent></w:sdt><w:r><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:r><w:t>i</w:t></w:r><w:r><w:t>j</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r><mc:AlternateContent><mc:Choice Requires="w"><w:r><w:t>k</w:t></w:r><w:r><w:t>l</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>k</w:t></w:r><w:r><w:t>l</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>m</w:t></w:r><w:r><w:t>n</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:del w:id="2" w:author="A"><w:r><w:delText>o</w:delText></w:r><w:r><w:delText xml:space="preserve"> p</w:delText></w:r></w:del></w:p></w:body>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!(
                r#"<w:body {W} {R} {MC} {V}><w:p><w:r><w:t>a</w:t></w:r><w:hyperlink r:id="rId1"><w:r><w:t>bc</w:t></w:r></w:hyperlink><w:r><w:t>d</w:t></w:r><w:ins w:id="1" w:author="A"><w:r><w:t>ef</w:t></w:r></w:ins><w:sdt><w:sdtContent><w:r><w:t>gh</w:t></w:r></w:sdtContent></w:sdt><w:r><w:pict><v:shape><v:textbox><w:txbxContent><w:p><w:r><w:t>ij</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r><mc:AlternateContent><mc:Choice Requires="w"><w:r><w:t>kl</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>kl</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>mn</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:del w:id="2" w:author="A"><w:r><w:delText>o p</w:delText></w:r></w:del></w:p></w:body>"#
            ),
            merged(8)
        )
    );
}

#[test]
fn drawingml_runs_with_equal_properties_become_one() {
    let xml = format!(
        r#"<p:txBody {P} {A}><a:bodyPr/><a:p><a:r><a:rPr lang="en-US" sz="1800" dirty="0"/><a:t>one </a:t></a:r><a:r><a:rPr sz="1800" lang="en-US" err="1"/><a:t>two </a:t></a:r><a:r><a:rPr lang="en-US" sz="1800"/><a:t>three</a:t></a:r><a:br><a:rPr lang="en-US" sz="1800"/></a:br><a:r><a:rPr lang="en-US" sz="1800"/><a:t>four</a:t></a:r></a:p></p:txBody>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!(
                r#"<p:txBody {P} {A}><a:bodyPr/><a:p><a:r><a:rPr lang="en-US" sz="1800" dirty="0"/><a:t>one two three</a:t></a:r><a:br><a:rPr lang="en-US" sz="1800"/></a:br><a:r><a:rPr lang="en-US" sz="1800"/><a:t>four</a:t></a:r></a:p></p:txBody>"#
            ),
            merged(2)
        )
    );
}

#[test]
fn spreadsheetml_rich_text_runs_merge_and_pivot_records_do_not() {
    let mut one = SpreadsheetText::default();
    one.value = "one".to_owned();
    let mut two = SpreadsheetText::default();
    two.value = " two".to_owned();
    two.space = Some("preserve".to_owned());
    let mut item = StringItem::default();
    item.rich_text_run = vec![styled_rich_text_run(one), styled_rich_text_run(two)];
    let mut shared = SharedStringTable::default();
    shared.string_item = vec![item];
    let written = xml_of(&spreadsheet_write::shared_string_table(
        &shared,
        &mut Scope::default(),
    ));
    let properties = r#"<x:rPr><x:b/><x:sz val="11"/></x:rPr>"#;
    assert!(written.contains(&format!(
        r#"<x:r>{properties}<x:t>one</x:t></x:r><x:r>{properties}<x:t xml:space="preserve"> two</x:t></x:r>"#
    )));
    let (compact, done) = compacted(&written, Comments::Keep);
    assert_eq!(done, merged(1));
    assert!(compact.contains(&format!("<x:r>{properties}<x:t>one two</x:t></x:r></x:si>")));
    assert_eq!(
        runs_in(&spreadsheet_read::shared_string_table(
            &parse(compact.as_bytes()).unwrap().root
        )),
        [r#""one two""#]
    );
    let mut numeric = Numeric::default();
    numeric.v = Some("1".to_owned());
    let mut valued = PivotCacheRecord::default();
    valued.content = vec![PivotCacheRecordContent::Numeric(numeric)];
    let mut records = PivotCacheRecords::default();
    records.pivotcache_record = vec![
        PivotCacheRecord::default(),
        PivotCacheRecord::default(),
        valued.clone(),
        valued,
    ];
    let records_xml = xml_of(&spreadsheet_write::pivot_cache_records(
        &records,
        &mut Scope::default(),
    ));
    assert_eq!(
        compacted(&records_xml, Comments::Keep),
        (records_xml, merged(0))
    );
}

pub(super) fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

/// A package holding `parts`, each a name, a content type, and its bytes;
/// the first is related from the package as its main part.
pub(super) fn package(parts: &[(&str, &str, &str)]) -> Package {
    let mut package = Package::new();
    for &(part, content_type, bytes) in parts {
        package
            .set_part_bytes(&name(part), content_type, bytes.as_bytes())
            .unwrap();
    }
    package
        .relate(
            None,
            &name(parts[0].0),
            RelationshipType::OFFICE_DOCUMENT.transitional,
        )
        .unwrap();
    package
}

/// The bytes of a part, as text.
pub(super) fn text(package: &Package, part: &str) -> String {
    String::from_utf8(package.part_bytes(&name(part)).unwrap()).unwrap()
}

/// A Word paragraph holding the runs of `a` and `b`, which merge.
fn fragmented_xml() -> &'static str {
    "<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"
}

/// A Word document whose body holds the paragraph of `fragmented_xml`.
fn fragmented_document_xml() -> String {
    format!(
        "<w:document {W}><w:body>{}</w:body></w:document>",
        fragmented_xml()
    )
}

/// The texts of `runs`.
fn rich_text(runs: &[RichTextRun]) -> String {
    let texts: Vec<String> = runs
        .iter()
        .map(|found| format!("{:?}", found.text.value))
        .collect();
    texts.join(" ")
}

/// What the paragraphs under `node` hold, as [`Runs`] gives it.
pub(super) fn runs_in<N>(node: &N) -> Vec<String>
where
    N: Visit,
{
    let mut runs = Runs(Vec::new());
    node.visit(&mut runs);
    runs.0
}

/// The root element of the part `name` of `package`.
pub(super) fn root_of(package: &Package, part: &str) -> Element {
    parse(&package.part_bytes(&name(part)).unwrap())
        .unwrap()
        .root
}

/// The part text of `root`.
pub(super) fn xml_of(root: &Element) -> String {
    let mut part = XmlDocument::new("", root.clone());
    part.declaration = None;
    part.prolog.clear();
    String::from_utf8(part.to_xml(KEEP)).unwrap()
}

/// A Word paragraph holding the runs of `a` and `b`, which merge.
fn fragmented() -> BlockContent {
    BlockContent::Paragraph(paragraph(vec![plain_run("a"), plain_run("b")]))
}

/// The text of a Word document whose body holds `blocks`.
pub(super) fn document_of(blocks: Vec<BlockContent>) -> String {
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    xml_of(&write::document(&document, &mut Scope::default()))
}

/// A Word document whose body holds the paragraph of [`fragmented`].
fn fragmented_document() -> String {
    document_of(vec![fragmented()])
}

/// A `DrawingML` paragraph holding the runs of `a` and `b`, which merge.
fn fragmented_drawing() -> TextParagraphs {
    let run_of = |value: &str| {
        let mut text = TextString::default();
        text.value = value.to_owned();
        let mut found = DrawingRun::default();
        found.text_string = Box::new(text);
        TextParagraphsContent::TextRun(found)
    };
    let mut paragraph = TextParagraphs::default();
    paragraph.content = vec![run_of("a"), run_of("b")];
    paragraph
}

/// `SpreadsheetML` rich text runs of `a` and `b`, which merge.
fn fragmented_rich_text() -> Vec<RichTextRun> {
    ["a", "b"]
        .map(|value| {
            let mut text = SpreadsheetText::default();
            text.value = value.to_owned();
            let mut found = RichTextRun::default();
            found.text = Box::new(text);
            found
        })
        .into()
}

/// A `SpreadsheetML` rich text run of `text`, bold and of size 11.
fn styled_rich_text_run(text: SpreadsheetText) -> RichTextRun {
    let mut size = FontSize::default();
    size.val = Some("11".to_owned());
    let mut properties = spreadsheet::RunProperties::default();
    properties.content = vec![
        spreadsheet::RunPropertiesContent::Bold(spreadsheet::Bold::default()),
        spreadsheet::RunPropertiesContent::FontSize(size),
    ];
    let mut found = RichTextRun::default();
    found.properties = Some(Box::new(properties));
    found.text = Box::new(text);
    found
}

/// A slide's common data, its one shape holding the paragraph of [`fragmented_drawing`].
fn fragmented_slide_data() -> CommonSlideData {
    let mut body = ShapeTextBody::default();
    body.text_paragraphs = vec![fragmented_drawing()];
    let mut shape = Shape::default();
    shape.text_body = Some(Box::new(body));
    let mut data = CommonSlideData::default();
    data.shape_tree.content = vec![GroupShapeContent::Shape(shape)];
    data
}

#[test]
fn every_word_part_that_holds_runs_is_compacted_and_stored_back() {
    let document = fragmented_document();
    let mut header = Header::default();
    header.content = vec![fragmented()];
    let mut footer = Footer::default();
    footer.content = vec![fragmented()];
    let mut footnote = FootnoteContent::default();
    footnote.content = vec![fragmented()];
    footnote.id = Some(number("1"));
    let mut footnotes = DocumentFootnotes::default();
    footnotes.footnote_content = vec![footnote];
    let mut rich = RichText::default();
    rich.paragraphs = vec![fragmented_drawing()];
    let mut chart_text = ChartText::default();
    chart_text.content = Box::new(ChartTextContent::RichText(rich));
    let mut title = Title::default();
    title.chart_text = Some(Box::new(chart_text));
    let mut chart = ChartSpace::default();
    chart.chart.title = Some(Box::new(title));
    let scope = || Scope::default();
    let mut word = package(&[
        ("/word/document.xml", MAIN, &document),
        (
            "/word/header1.xml",
            HEADER,
            &xml_of(&write::header(&header, &mut scope())),
        ),
        (
            "/word/footer1.xml",
            FOOTER,
            &xml_of(&write::footer(&footer, &mut scope())),
        ),
        (
            "/word/footnotes.xml",
            FOOTNOTES,
            &xml_of(&write::document_footnotes(&footnotes, &mut scope())),
        ),
        (
            "/word/charts/chart1.xml",
            CHART,
            &xml_of(&chart_write::chart_space(&chart, &mut scope())),
        ),
    ]);
    assert_eq!(
        compact_package(&mut word, Comments::Keep).unwrap(),
        Compaction {
            merged: 5,
            parts: 5,
            ..Compaction::default()
        }
    );
    assert_eq!(
        runs_in(&read::document(&root_of(&word, "/word/document.xml"))),
        [MERGED]
    );
    assert_eq!(
        runs_in(&read::header(&root_of(&word, "/word/header1.xml"))),
        [MERGED]
    );
    assert_eq!(
        runs_in(&read::footer(&root_of(&word, "/word/footer1.xml"))),
        [MERGED]
    );
    assert_eq!(
        runs_in(&read::document_footnotes(&root_of(
            &word,
            "/word/footnotes.xml"
        ))),
        [MERGED]
    );
    assert_eq!(
        runs_in(&chart_read::chart_space(&root_of(
            &word,
            "/word/charts/chart1.xml"
        ))),
        [r#""ab""#]
    );
}

#[test]
fn a_part_with_nothing_to_merge_and_a_part_that_is_not_xml_keep_their_bytes() {
    let document = fragmented_document_xml();
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:styles {W}>\n  <w:style w:type=\"paragraph\" w:styleId=\"Normal\"/>\n</w:styles>"
    );
    let data = fragmented_document_xml();
    let mut word = package(&[
        ("/word/document.xml", MAIN, &document),
        ("/word/styles.xml", STYLES, &styles),
        ("/word/data.bin", "application/octet-stream", &data),
    ]);
    assert_eq!(
        compact_package(&mut word, Comments::Keep).unwrap(),
        Compaction {
            merged: 1,
            parts: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(text(&word, "/word/styles.xml"), styles);
    assert_eq!(text(&word, "/word/data.bin"), data);
}

#[test]
fn an_embedded_document_is_compacted_inside_its_own_package() {
    let document = fragmented_document();
    let inner = package(&[("/word/document.xml", MAIN, &document)])
        .to_bytes()
        .unwrap();
    let mut outer = package(&[("/word/document.xml", MAIN, &document)]);
    let embedded = name("/word/embeddings/Document.docx");
    outer.set_part_bytes(&embedded, DOCX, &inner).unwrap();
    assert_eq!(
        compact_package(&mut outer, Comments::Keep).unwrap(),
        Compaction {
            embedded: 1,
            merged: 2,
            parts: 2,
            ..Compaction::default()
        }
    );
    assert_eq!(outer.content_type(&embedded), Some(DOCX));
    let compacted = Package::from_bytes(outer.part_bytes(&embedded).unwrap()).unwrap();
    assert_eq!(
        runs_in(&read::document(&root_of(&compacted, "/word/document.xml"))),
        [MERGED]
    );
}

#[test]
fn every_office_package_type_is_compacted_inside_its_own_package() {
    let types = [
        "application/vnd.ms-excel.sheet.macroEnabled.12",
        "application/vnd.ms-excel.template.macroEnabled.12",
        "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
        "application/vnd.ms-powerpoint.slideshow.macroEnabled.12",
        "application/vnd.ms-powerpoint.template.macroEnabled.12",
        "application/vnd.ms-word.document.macroEnabled.12",
        "application/vnd.ms-word.template.macroEnabled.12",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
        "application/vnd.openxmlformats-officedocument.presentationml.template",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
        DOCX,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
    ];
    let inner = package(&[("/word/document.xml", MAIN, &fragmented_document())])
        .to_bytes()
        .unwrap();
    let mut outer = package(&[("/word/document.xml", MAIN, &document_of(Vec::new()))]);
    for (number, content_type) in types.iter().enumerate() {
        outer
            .set_part_bytes(
                &name(&format!("/word/embeddings/package{number}.bin")),
                content_type,
                &inner,
            )
            .unwrap();
    }
    assert_eq!(
        compact_package(&mut outer, Comments::Keep).unwrap(),
        Compaction {
            embedded: types.len(),
            merged: types.len(),
            parts: types.len(),
            ..Compaction::default()
        }
    );
}

#[test]
fn an_embedded_document_with_nothing_to_merge_keeps_its_bytes() {
    let document = document_of(Vec::new());
    let inner = package(&[("/word/document.xml", MAIN, &document)])
        .to_bytes()
        .unwrap();
    let mut outer = package(&[("/word/document.xml", MAIN, &document)]);
    let embedded = name("/word/embeddings/Document.docx");
    outer.set_part_bytes(&embedded, DOCX, &inner).unwrap();
    assert_eq!(
        compact_package(&mut outer, Comments::Keep).unwrap(),
        Compaction {
            embedded: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(outer.part_bytes(&embedded).unwrap(), inner);
}

#[test]
fn an_embedded_part_that_is_no_package_is_left_as_it_is_and_skipped() {
    let mut outer = package(&[("/word/document.xml", MAIN, &fragmented_document())]);
    let embedded = name("/word/embeddings/Broken.docx");
    outer.set_part_bytes(&embedded, DOCX, b"not a zip").unwrap();
    assert_eq!(
        compact_package(&mut outer, Comments::Keep).unwrap(),
        Compaction {
            merged: 1,
            parts: 1,
            skipped: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(outer.part_bytes(&embedded).unwrap(), b"not a zip");
}

#[test]
fn a_picture_whose_type_ends_in_xml_is_left_as_it_is() {
    let svg = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" ",
        "\"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>"
    );
    let mut word = package(&[
        ("/word/document.xml", MAIN, &fragmented_document()),
        ("/word/media/image1.svg", "image/svg+xml", svg),
    ]);
    assert_eq!(
        compact_package(&mut word, Comments::Keep).unwrap(),
        Compaction {
            merged: 1,
            parts: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(text(&word, "/word/media/image1.svg"), svg);
}

#[test]
fn a_presentation_has_its_slides_and_notes_compacted() {
    let presentation = xml_of(&presentation_write::presentation(
        &Presentation::default(),
        &mut Scope::default(),
    ));
    let mut slide = PresentationSlideSlide::default();
    *slide.common_slide_data = fragmented_slide_data();
    let mut notes = NotesSlide::default();
    *notes.common_slide_data = fragmented_slide_data();
    let mut deck = package(&[
        ("/ppt/presentation.xml", PRESENTATION, &presentation),
        (
            "/ppt/slides/slide1.xml",
            SLIDE,
            &xml_of(&presentation_write::presentation_slide_slide(
                &slide,
                &mut Scope::default(),
            )),
        ),
        (
            "/ppt/notesSlides/notesSlide1.xml",
            NOTES_SLIDE,
            &xml_of(&presentation_write::notes_slide(
                &notes,
                &mut Scope::default(),
            )),
        ),
    ]);
    assert_eq!(
        compact_package(&mut deck, Comments::Keep).unwrap(),
        Compaction {
            merged: 2,
            parts: 2,
            ..Compaction::default()
        }
    );
    assert_eq!(
        runs_in(&presentation_read::presentation_slide_slide(&root_of(
            &deck,
            "/ppt/slides/slide1.xml"
        ))),
        [r#""ab""#]
    );
    assert_eq!(
        runs_in(&presentation_read::notes_slide(&root_of(
            &deck,
            "/ppt/notesSlides/notesSlide1.xml"
        ))),
        [r#""ab""#]
    );
    assert_eq!(text(&deck, "/ppt/presentation.xml"), presentation);
}

#[test]
fn a_workbook_has_its_shared_strings_and_comments_compacted() {
    let mut item = StringItem::default();
    item.rich_text_run = fragmented_rich_text();
    let mut shared = SharedStringTable::default();
    shared.string_item = vec![item];
    let mut comment = Comment::default();
    comment.author_id = Some("0".to_owned());
    comment.r#ref = Some("A1".to_owned());
    comment.text.rich_text_run = fragmented_rich_text();
    let mut comments = SpreadsheetComments::default();
    comments.list.comment = vec![comment];
    let scope = || Scope::default();
    let mut book = package(&[
        (
            "/xl/workbook.xml",
            WORKBOOK,
            &xml_of(&spreadsheet_write::workbook(
                &Workbook::default(),
                &mut scope(),
            )),
        ),
        (
            "/xl/sharedStrings.xml",
            SHARED_STRINGS,
            &xml_of(&spreadsheet_write::shared_string_table(
                &shared,
                &mut scope(),
            )),
        ),
        (
            "/xl/comments1.xml",
            SHEET_COMMENTS,
            &xml_of(&spreadsheet_write::comments(&comments, &mut scope())),
        ),
    ]);
    assert_eq!(
        compact_package(&mut book, Comments::Keep).unwrap(),
        Compaction {
            merged: 2,
            parts: 2,
            ..Compaction::default()
        }
    );
    assert_eq!(
        runs_in(&spreadsheet_read::shared_string_table(&root_of(
            &book,
            "/xl/sharedStrings.xml"
        ))),
        [r#""ab""#]
    );
    assert_eq!(
        runs_in(&spreadsheet_read::comments(&root_of(
            &book,
            "/xl/comments1.xml"
        ))),
        [r#""ab""#]
    );
}
