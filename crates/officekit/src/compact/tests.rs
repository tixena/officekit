use alloc::collections::BTreeSet;

use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{Comments, Compaction, PartCompaction, compact_package, compact_tree};

const A: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
const C: &str = r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart""#;
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const P: &str = r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const R: &str = r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships""#;
const V: &str = r#"xmlns:v="urn:schemas-microsoft-com:vml""#;
pub(super) const W: &str =
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const X: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

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

const KEEP: WriteOptions = WriteOptions::new(Layout::Compact, false);

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

#[test]
fn the_fox_paragraph_becomes_one_run() {
    let xml = format!(
        r#"<w:p {W}>
<w:r w:rsidR="00A1"><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">the </w:t></w:r>
<w:r w:rsidR="00B2"><w:rPr><w:b/><w:noProof/></w:rPr><w:t xml:space="preserve">fox </w:t></w:r>
<w:proofErr w:type="spellStart"/><w:r w:rsidR="00C3"><w:rPr><w:b/></w:rPr><w:t>jumped</w:t></w:r><w:proofErr w:type="spellEnd"/>
<w:r w:rsidR="00C3"><w:rPr><w:b/></w:rPr><w:t xml:space="preserve"> over the lazy dog</w:t></w:r></w:p>"#
    );
    let mut document = parse(xml.as_bytes()).unwrap();
    let done = compact_tree(&mut document.root, Comments::Keep);
    assert_eq!(done.merged, 3);
    assert_eq!(done, merged(3));
    assert_eq!(document.root.child_elements().count(), 1);
    assert_eq!(
        String::from_utf8(document.to_xml(KEEP)).unwrap(),
        format!(
            r#"<w:p {W}><w:r w:rsidR="00A1"><w:rPr><w:b/></w:rPr><w:t>the fox jumped over the lazy dog</w:t></w:r></w:p>"#
        )
    );
}

#[test]
fn runs_with_different_formatting_stay_apart() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t>plain</w:t></w:r><w:proofErr w:type="gramStart"/><w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r></w:p>"#
    );
    assert_eq!(compacted(&xml, Comments::Keep), (xml, merged(0)));
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
    let xml = format!(
        r#"<w:p {W}><w:r><w:t>a</w:t><w:tab/></w:r><w:r><w:t>b</w:t></w:r><w:r><w:t>c</w:t><w:br w:type="page"/><w:cr/><w:noBreakHyphen/><w:softHyphen/><w:lastRenderedPageBreak/></w:r><w:r><w:t>d</w:t></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!(
                r#"<w:p {W}><w:r><w:t>a</w:t><w:tab/><w:t>bc</w:t><w:br w:type="page"/><w:cr/><w:noBreakHyphen/><w:softHyphen/><w:lastRenderedPageBreak/><w:t>d</w:t></w:r></w:p>"#
            ),
            merged(3)
        )
    );
}

#[test]
fn joined_text_preserves_its_space_exactly_when_it_starts_or_ends_with_white_space() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:t>one</w:t></w:r><w:r><w:t xml:space="preserve"> two </w:t></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep).0,
        format!(r#"<w:p {W}><w:r><w:t xml:space="preserve">one two </w:t></w:r></w:p>"#)
    );
    let leading = format!(
        r#"<w:p {W}><w:r><w:t xml:space="preserve">one </w:t></w:r><w:r><w:t>two</w:t></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&leading, Comments::Keep).0,
        format!("<w:p {W}><w:r><w:t>one two</w:t></w:r></w:p>")
    );
}

#[test]
fn text_whose_white_space_a_consumer_may_trim_is_not_joined() {
    let xml = format!("<w:p {W}><w:r><w:t>one</w:t></w:r><w:r><w:t> two</w:t></w:r></w:p>");
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!("<w:p {W}><w:r><w:t>one</w:t><w:t> two</w:t></w:r></w:p>"),
            merged(1)
        )
    );
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
    let xml = format!(
        r#"<w:p {W}><w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t>b</w:t></w:r><w:proofErr w:type="spellEnd"/><w:r><w:t>c</w:t></w:r></w:p>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!(
                r#"<w:p {W}><w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r><w:proofErr w:type="spellStart"/><w:r><w:t>bc</w:t></w:r></w:p>"#
            ),
            merged(1)
        )
    );
}

#[test]
fn a_kept_comment_keeps_its_runs_apart() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="7"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="7"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="7"/></w:r></w:p>"#
    );
    assert_eq!(compacted(&xml, Comments::Keep), (xml, merged(0)));
}

#[test]
fn a_removed_comment_loses_every_node_of_it_in_the_tree() {
    let xml = format!(
        r#"<w:body {W}><w:p><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="7"/><w:r><w:t>b</w:t></w:r></w:p><w:p><w:r><w:t>c</w:t></w:r><w:commentRangeEnd w:id="7"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="7"/></w:r><w:bookmarkStart w:id="0" w:name="m"/><w:commentRangeStart w:id="8"/><w:r><w:rPr><w:i/></w:rPr><w:t>d</w:t></w:r><w:commentRangeEnd w:id="8"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="8"/></w:r></w:p><w:p><w:r><w:t>e</w:t><w:commentReference w:id="7"/></w:r></w:p></w:body>"#
    );
    let (written, done) = compacted(&xml, Comments::Remove);
    assert_eq!(
        written,
        format!(
            r#"<w:body {W}><w:p><w:r><w:t>ab</w:t></w:r></w:p><w:p><w:r><w:t>c</w:t></w:r><w:bookmarkStart w:id="0" w:name="m"/><w:commentRangeStart w:id="8"/><w:r><w:rPr><w:i/></w:rPr><w:t>d</w:t></w:r><w:commentRangeEnd w:id="8"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="8"/></w:r></w:p><w:p><w:r><w:t>e</w:t></w:r></w:p></w:body>"#
        )
    );
    assert_eq!(done.merged, 1);
    assert_eq!(done.removed_comments, BTreeSet::from(["7".to_owned()]));
}

#[test]
fn a_comment_between_runs_that_do_not_merge_stays_under_remove() {
    let xml = format!(
        r#"<w:p {W}><w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r><w:commentRangeStart w:id="7"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="7"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="7"/></w:r></w:p>"#
    );
    assert_eq!(compacted(&xml, Comments::Remove), (xml, merged(0)));
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
    let xml = format!(
        r#"<si {X}><r><rPr><b/><sz val="11"/></rPr><t>one</t></r><r><rPr><b/><sz val="11"/></rPr><t xml:space="preserve"> two</t></r></si>"#
    );
    assert_eq!(
        compacted(&xml, Comments::Keep),
        (
            format!(r#"<si {X}><r><rPr><b/><sz val="11"/></rPr><t>one two</t></r></si>"#),
            merged(1)
        )
    );
    let records = format!(
        r#"<pivotCacheRecords {X}><r/><r/><r><n v="1"/></r><r><n v="1"/></r></pivotCacheRecords>"#
    );
    assert_eq!(compacted(&records, Comments::Keep), (records, merged(0)));
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
fn fragmented() -> &'static str {
    "<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"
}

/// The paragraph of `fragmented` once compacted.
fn merged_paragraph() -> &'static str {
    "<w:p><w:r><w:t>ab</w:t></w:r></w:p>"
}

/// A Word document whose body holds the paragraph of `fragmented`.
fn fragmented_document() -> String {
    format!(
        "<w:document {W}><w:body>{}</w:body></w:document>",
        fragmented()
    )
}

#[test]
fn every_word_part_that_holds_runs_is_compacted_and_stored_back() {
    let paragraph = fragmented();
    let document = fragmented_document();
    let header = format!("<w:hdr {W}>{paragraph}</w:hdr>");
    let footer = format!("<w:ftr {W}>{paragraph}</w:ftr>");
    let footnotes =
        format!(r#"<w:footnotes {W}><w:footnote w:id="1">{paragraph}</w:footnote></w:footnotes>"#);
    let chart = format!(
        "<c:chartSpace {C} {A}><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>a</a:t></a:r><a:r><a:t>b</a:t></a:r></a:p></c:rich></c:tx></c:title></c:chart></c:chartSpace>"
    );
    let mut word = package(&[
        ("/word/document.xml", MAIN, &document),
        ("/word/header1.xml", HEADER, &header),
        ("/word/footer1.xml", FOOTER, &footer),
        ("/word/footnotes.xml", FOOTNOTES, &footnotes),
        ("/word/charts/chart1.xml", CHART, &chart),
    ]);
    assert_eq!(
        compact_package(&mut word, Comments::Keep).unwrap(),
        Compaction {
            merged: 5,
            parts: 5,
            ..Compaction::default()
        }
    );
    let one = merged_paragraph();
    assert_eq!(
        text(&word, "/word/document.xml"),
        format!("<w:document {W}><w:body>{one}</w:body></w:document>")
    );
    assert_eq!(
        text(&word, "/word/header1.xml"),
        format!("<w:hdr {W}>{one}</w:hdr>")
    );
    assert_eq!(
        text(&word, "/word/footer1.xml"),
        format!("<w:ftr {W}>{one}</w:ftr>")
    );
    assert_eq!(
        text(&word, "/word/footnotes.xml"),
        format!(r#"<w:footnotes {W}><w:footnote w:id="1">{one}</w:footnote></w:footnotes>"#)
    );
    assert_eq!(
        text(&word, "/word/charts/chart1.xml"),
        format!(
            "<c:chartSpace {C} {A}><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>ab</a:t></a:r></a:p></c:rich></c:tx></c:title></c:chart></c:chartSpace>"
        )
    );
}

#[test]
fn a_part_with_nothing_to_merge_and_a_part_that_is_not_xml_keep_their_bytes() {
    let document = fragmented_document();
    let styles = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:styles {W}>\n  <w:style w:type=\"paragraph\" w:styleId=\"Normal\"/>\n</w:styles>"
    );
    let data = fragmented_document();
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
        text(&compacted, "/word/document.xml"),
        format!(
            "<w:document {W}><w:body>{}</w:body></w:document>",
            merged_paragraph()
        )
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
    let document = fragmented_document();
    let inner = package(&[("/word/document.xml", MAIN, &document)])
        .to_bytes()
        .unwrap();
    let empty = format!("<w:document {W}><w:body/></w:document>");
    let mut outer = package(&[("/word/document.xml", MAIN, &empty)]);
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
    let document = format!("<w:document {W}><w:body/></w:document>");
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
    let document = fragmented_document();
    let mut outer = package(&[("/word/document.xml", MAIN, &document)]);
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
    let document = fragmented_document();
    let svg = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" ",
        "\"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"/>"
    );
    let mut word = package(&[
        ("/word/document.xml", MAIN, &document),
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
    let presentation = format!("<p:presentation {P}/>");
    let body =
        "<p:txBody><a:bodyPr/><a:p><a:r><a:t>a</a:t></a:r><a:r><a:t>b</a:t></a:r></a:p></p:txBody>";
    let slide =
        format!("<p:sld {P} {A}><p:cSld><p:spTree><p:sp>{body}</p:sp></p:spTree></p:cSld></p:sld>");
    let notes = format!(
        "<p:notes {P} {A}><p:cSld><p:spTree><p:sp>{body}</p:sp></p:spTree></p:cSld></p:notes>"
    );
    let mut deck = package(&[
        ("/ppt/presentation.xml", PRESENTATION, &presentation),
        ("/ppt/slides/slide1.xml", SLIDE, &slide),
        ("/ppt/notesSlides/notesSlide1.xml", NOTES_SLIDE, &notes),
    ]);
    assert_eq!(
        compact_package(&mut deck, Comments::Keep).unwrap(),
        Compaction {
            merged: 2,
            parts: 2,
            ..Compaction::default()
        }
    );
    let one = "<p:txBody><a:bodyPr/><a:p><a:r><a:t>ab</a:t></a:r></a:p></p:txBody>";
    assert_eq!(
        text(&deck, "/ppt/slides/slide1.xml"),
        format!("<p:sld {P} {A}><p:cSld><p:spTree><p:sp>{one}</p:sp></p:spTree></p:cSld></p:sld>")
    );
    assert_eq!(
        text(&deck, "/ppt/notesSlides/notesSlide1.xml"),
        format!(
            "<p:notes {P} {A}><p:cSld><p:spTree><p:sp>{one}</p:sp></p:spTree></p:cSld></p:notes>"
        )
    );
    assert_eq!(text(&deck, "/ppt/presentation.xml"), presentation);
}

#[test]
fn a_workbook_has_its_shared_strings_and_comments_compacted() {
    let workbook = format!("<workbook {X}/>");
    let shared = format!("<sst {X}><si><r><t>a</t></r><r><t>b</t></r></si></sst>");
    let comments = format!(
        r#"<comments {X}><commentList><comment ref="A1" authorId="0"><text><r><t>a</t></r><r><t>b</t></r></text></comment></commentList></comments>"#
    );
    let mut book = package(&[
        ("/xl/workbook.xml", WORKBOOK, &workbook),
        ("/xl/sharedStrings.xml", SHARED_STRINGS, &shared),
        ("/xl/comments1.xml", SHEET_COMMENTS, &comments),
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
        text(&book, "/xl/sharedStrings.xml"),
        format!("<sst {X}><si><r><t>ab</t></r></si></sst>")
    );
    assert_eq!(
        text(&book, "/xl/comments1.xml"),
        format!(
            r#"<comments {X}><commentList><comment ref="A1" authorId="0"><text><r><t>ab</t></r></text></comment></commentList></comments>"#
        )
    );
}
