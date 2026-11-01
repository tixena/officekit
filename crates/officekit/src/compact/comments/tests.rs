use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::office_word_2012_wordml::{
    CommentEx, CommentsEx, read as extended_read, write as extended_write,
};
use officekit_markup::office_word_2016_wordml_cid::{
    CommentId, CommentsIds, read as ids_read, write as ids_write,
};
use officekit_markup::office_word_2018_wordml_cex::{
    CommentExtensible, CommentsExtensible, read as extensible_read, write as extensible_write,
};
use officekit_markup::wordprocessing::{
    BlockContent, CommentContent, CommentsCollection, ContentsOfGlossaryDocumentEntry,
    DecimalNumberValue, GlossaryDocumentEntry, GlossaryDocumentRootElement, Header,
    ListOfGlossaryDocumentEntries, ParagraphContent, read, write,
};
use officekit_xml::packaging::package::Package;
use officekit_xml::write::Scope;

use crate::compact::tests::{
    HEADER, MAIN, bold, comment_end, comment_reference, comment_start, document_of, name, package,
    paragraph, plain_run, plain_text, root_of, run, runs_in, text, xml_of,
};
use crate::compact::{Comments, Compaction, compact_package};

const COMMENTS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
const COMMENTS_EXTENDED: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml";
const COMMENTS_EXTENSIBLE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtensible+xml";
const COMMENTS_IDS: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsIds+xml";
const GLOSSARY: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.glossary+xml";

const COMMENTS_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const COMMENTS_EXTENDED_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const COMMENTS_EXTENSIBLE_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2018/08/relationships/commentsExtensible";
const COMMENTS_IDS_RELATIONSHIP: &str =
    "http://schemas.microsoft.com/office/2016/09/relationships/commentsIds";
const GLOSSARY_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/glossaryDocument";
const HEADER_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";

/// A paragraph whose two runs merge around the range start of comment `id`,
/// followed by the range end and the reference.
fn removable(id: &str) -> BlockContent {
    BlockContent::Paragraph(paragraph(vec![
        plain_run("a"),
        comment_start(id),
        plain_run("b"),
        comment_end(id),
        comment_reference(id),
    ]))
}

/// A paragraph whose runs differ, so comment `id` stays.
fn kept(id: &str) -> BlockContent {
    BlockContent::Paragraph(paragraph(vec![
        ParagraphContent::TextRun(run(vec![bold()], vec![plain_text("c")])),
        comment_start(id),
        plain_run("d"),
        comment_end(id),
        comment_reference(id),
    ]))
}

/// The comments part's entry of comment `id`, whose paragraphs have the
/// ids `paragraphs`.
fn comment(id: &str, paragraphs: &[&str]) -> CommentContent {
    let mut entry = CommentContent::default();
    entry.author = Some("A".to_owned());
    entry.content = paragraphs
        .iter()
        .map(|paragraph_id| {
            let mut note = paragraph(vec![plain_run("note")]);
            note.para_id = Some((*paragraph_id).to_owned());
            BlockContent::Paragraph(note)
        })
        .collect();
    entry.id = Some(DecimalNumberValue::from(id.to_owned()));
    entry
}

/// A comments part of `entries`.
fn comments_of(entries: Vec<CommentContent>) -> String {
    let mut comments = CommentsCollection::default();
    comments.comment_content = entries;
    xml_of(&write::comments_collection(
        &comments,
        &mut Scope::default(),
    ))
}

/// The four comments parts recording comment 7, whose last paragraph is
/// `00000002` and durable id `11111111`, and comment 8, with `00000003` and
/// `22222222`.
fn comments_parts() -> [(&'static str, &'static str, String); 4] {
    let pairs = [("00000002", "11111111"), ("00000003", "22222222")];
    let mut extended = CommentsEx::default();
    extended.comment_ex = pairs
        .iter()
        .map(|(paragraph_id, _durable)| {
            let mut entry = CommentEx::default();
            entry.done = Some(OnOffValue::from("0".to_owned()));
            entry.para_id = Some((*paragraph_id).to_owned());
            entry
        })
        .collect();
    let mut ids = CommentsIds::default();
    ids.comment_id = pairs
        .iter()
        .map(|(paragraph_id, durable)| {
            let mut entry = CommentId::default();
            entry.durable_id = Some((*durable).to_owned());
            entry.para_id = Some((*paragraph_id).to_owned());
            entry
        })
        .collect();
    let mut extensible = CommentsExtensible::default();
    extensible.comment_extensible = pairs
        .iter()
        .map(|(_paragraph_id, durable)| {
            let mut entry = CommentExtensible::default();
            entry.date_utc = Some("2026-10-16T00:00:00Z".to_owned());
            entry.durable_id = Some((*durable).to_owned());
            entry
        })
        .collect();
    let scope = || Scope::default();
    [
        (
            "/word/comments.xml",
            COMMENTS,
            comments_of(vec![
                comment("7", &["00000001", "00000002"]),
                comment("8", &["00000003"]),
            ]),
        ),
        (
            "/word/commentsExtended.xml",
            COMMENTS_EXTENDED,
            xml_of(&extended_write::comments_ex(&extended, &mut scope())),
        ),
        (
            "/word/commentsIds.xml",
            COMMENTS_IDS,
            xml_of(&ids_write::comments_ids(&ids, &mut scope())),
        ),
        (
            "/word/commentsExtensible.xml",
            COMMENTS_EXTENSIBLE,
            xml_of(&extensible_write::comments_extensible(
                &extensible,
                &mut scope(),
            )),
        ),
    ]
}

/// A Word package whose main document holds `blocks` and relates the four
/// comments parts.
fn commented(blocks: Vec<BlockContent>) -> Package {
    let mut word = package(&[("/word/document.xml", MAIN, &document_of(blocks))]);
    let main = name("/word/document.xml");
    let relationships = [
        COMMENTS_RELATIONSHIP,
        COMMENTS_EXTENDED_RELATIONSHIP,
        COMMENTS_IDS_RELATIONSHIP,
        COMMENTS_EXTENSIBLE_RELATIONSHIP,
    ];
    for ((part, content_type, xml), relationship) in comments_parts().iter().zip(relationships) {
        word.set_part_bytes(&name(part), content_type, xml.as_bytes())
            .unwrap();
        word.relate(Some(&main), &name(part), relationship).unwrap();
    }
    word
}

/// The ids of the comments of the comments part `part`.
fn comment_ids(word: &Package, part: &str) -> Vec<String> {
    read::comments_collection(&root_of(word, part))
        .comment_content
        .iter()
        .filter_map(|entry| entry.id.as_ref().map(|id| id.0.clone()))
        .collect()
}

/// What the four comments parts record, read back through their typed roots: the comments'
/// ids, the extended entries' paragraph ids, and the durable ids of the other two.
fn recorded(word: &Package) -> [Vec<String>; 4] {
    [
        comment_ids(word, "/word/comments.xml"),
        extended_read::comments_ex(&root_of(word, "/word/commentsExtended.xml"))
            .comment_ex
            .iter()
            .filter_map(|entry| entry.para_id.clone())
            .collect(),
        ids_read::comments_ids(&root_of(word, "/word/commentsIds.xml"))
            .comment_id
            .iter()
            .filter_map(|entry| entry.durable_id.clone())
            .collect(),
        extensible_read::comments_extensible(&root_of(word, "/word/commentsExtensible.xml"))
            .comment_extensible
            .iter()
            .filter_map(|entry| entry.durable_id.clone())
            .collect(),
    ]
}

/// What the four comments parts record once comment 7 is removed.
fn without_seven() -> [Vec<String>; 4] {
    [
        vec!["8".to_owned()],
        vec!["00000003".to_owned()],
        vec!["22222222".to_owned()],
        vec!["22222222".to_owned()],
    ]
}

/// The bytes of the four comments parts, as text.
fn comments_texts(word: &Package) -> [String; 4] {
    comments_parts().map(|(part, _, _)| text(word, part))
}

#[test]
fn a_removed_comment_leaves_every_comments_part_and_the_others_stay() {
    let mut word = commented(vec![removable("7"), kept("8")]);
    assert_eq!(
        compact_package(&mut word, Comments::Remove).unwrap(),
        Compaction {
            merged: 1,
            parts: 5,
            removed_comments: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(recorded(&word), without_seven());
    assert_eq!(
        runs_in(&read::document(&root_of(&word, "/word/document.xml"))),
        [
            r#"[] "ab""#,
            r#"[b] "c", commentRangeStart 8, [] "d", commentRangeEnd 8, [style] reference 8"#
        ]
    );
}

#[test]
fn a_kept_comment_leaves_every_comments_part_as_it_was() {
    let mut word = commented(vec![removable("7"), kept("8")]);
    let before = comments_texts(&word);
    let document = text(&word, "/word/document.xml");
    assert_eq!(
        compact_package(&mut word, Comments::Keep).unwrap(),
        Compaction::default()
    );
    assert_eq!(comments_texts(&word), before);
    assert_eq!(text(&word, "/word/document.xml"), document);
}

#[test]
fn a_comment_removed_from_a_header_leaves_the_comments_parts_of_the_main_document() {
    let mut word = commented(vec![kept("8")]);
    let mut content = Header::default();
    content.content = vec![removable("7")];
    let header = name("/word/header1.xml");
    word.set_part_bytes(
        &header,
        HEADER,
        xml_of(&write::header(&content, &mut Scope::default())).as_bytes(),
    )
    .unwrap();
    word.relate(
        Some(&name("/word/document.xml")),
        &header,
        HEADER_RELATIONSHIP,
    )
    .unwrap();
    assert_eq!(
        compact_package(&mut word, Comments::Remove).unwrap(),
        Compaction {
            merged: 1,
            parts: 5,
            removed_comments: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(recorded(&word), without_seven());
}

#[test]
fn a_comment_removed_from_the_glossary_leaves_the_comments_part_of_the_glossary() {
    let mut word = commented(vec![kept("7"), kept("8")]);
    let main = name("/word/document.xml");
    let glossary = name("/word/glossary/document.xml");
    let glossary_comments = name("/word/glossary/comments.xml");
    let mut body = ContentsOfGlossaryDocumentEntry::default();
    body.content = vec![removable("7")];
    let mut entry = GlossaryDocumentEntry::default();
    entry.contents = Some(Box::new(body));
    let mut entries = ListOfGlossaryDocumentEntries::default();
    entries.glossary_document_entry = vec![entry];
    let mut glossary_document = GlossaryDocumentRootElement::default();
    glossary_document.list_of_glossary_document_entries = Some(Box::new(entries));
    word.set_part_bytes(
        &glossary,
        GLOSSARY,
        xml_of(&write::glossary_document_root_element(
            &glossary_document,
            &mut Scope::default(),
        ))
        .as_bytes(),
    )
    .unwrap();
    word.relate(Some(&main), &glossary, GLOSSARY_RELATIONSHIP)
        .unwrap();
    word.set_part_bytes(
        &glossary_comments,
        COMMENTS,
        comments_of(vec![comment("7", &["00000009"])]).as_bytes(),
    )
    .unwrap();
    word.relate(Some(&glossary), &glossary_comments, COMMENTS_RELATIONSHIP)
        .unwrap();
    let before = comments_texts(&word);
    assert_eq!(
        compact_package(&mut word, Comments::Remove).unwrap(),
        Compaction {
            merged: 1,
            parts: 2,
            removed_comments: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(comments_texts(&word), before);
    assert_eq!(
        comment_ids(&word, "/word/glossary/comments.xml"),
        Vec::<String>::new()
    );
}
