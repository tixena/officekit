use core::fmt::{self, Write as _};

use officekit_packaging::package::Package;

use crate::compact::tests::{HEADER, MAIN, W, name, package, text};
use crate::compact::{Comments, Compaction, compact_package};

const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const W16CEX: &str = r#"xmlns:w16cex="http://schemas.microsoft.com/office/word/2018/wordml/cex""#;
const W16CID: &str = r#"xmlns:w16cid="http://schemas.microsoft.com/office/word/2016/wordml/cid""#;

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
fn removable(id: &str) -> String {
    format!(
        r#"<w:p><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id="{id}"/><w:r><w:t>b</w:t></w:r><w:commentRangeEnd w:id="{id}"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{id}"/></w:r></w:p>"#
    )
}

/// A paragraph whose runs differ, so comment `id` stays.
fn kept(id: &str) -> String {
    format!(
        r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>c</w:t></w:r><w:commentRangeStart w:id="{id}"/><w:r><w:t>d</w:t></w:r><w:commentRangeEnd w:id="{id}"/><w:r><w:rPr><w:rStyle w:val="CommentReference"/></w:rPr><w:commentReference w:id="{id}"/></w:r></w:p>"#
    )
}

/// The comments part's entry of comment `id`, whose paragraphs have the
/// ids `paragraphs`.
fn comment(id: &str, paragraphs: &[&str]) -> String {
    let mut entry = format!(r#"<w:comment w:id="{id}" w:author="A">"#);
    for paragraph in paragraphs {
        let _: fmt::Result = write!(
            entry,
            r#"<w:p w14:paraId="{paragraph}"><w:r><w:t>note</w:t></w:r></w:p>"#
        );
    }
    entry.push_str("</w:comment>");
    entry
}

/// The four comments parts recording comment 7, whose last paragraph is
/// `00000002` and durable id `11111111`, and comment 8, with `00000003` and
/// `22222222`.
fn comments_parts() -> [(&'static str, &'static str, String); 4] {
    [
        (
            "/word/comments.xml",
            COMMENTS,
            format!(
                "<w:comments {W} {W14}>{}{}</w:comments>",
                comment("7", &["00000001", "00000002"]),
                comment("8", &["00000003"])
            ),
        ),
        (
            "/word/commentsExtended.xml",
            COMMENTS_EXTENDED,
            format!(
                r#"<w15:commentsEx {W15}><w15:commentEx w15:paraId="00000002" w15:done="0"/><w15:commentEx w15:paraId="00000003" w15:done="0"/></w15:commentsEx>"#
            ),
        ),
        (
            "/word/commentsIds.xml",
            COMMENTS_IDS,
            format!(
                r#"<w16cid:commentsIds {W16CID}><w16cid:commentId w16cid:paraId="00000002" w16cid:durableId="11111111"/><w16cid:commentId w16cid:paraId="00000003" w16cid:durableId="22222222"/></w16cid:commentsIds>"#
            ),
        ),
        (
            "/word/commentsExtensible.xml",
            COMMENTS_EXTENSIBLE,
            format!(
                r#"<w16cex:commentsExtensible {W16CEX}><w16cex:commentExtensible w16cex:durableId="11111111" w16cex:dateUtc="2026-10-16T00:00:00Z"/><w16cex:commentExtensible w16cex:durableId="22222222" w16cex:dateUtc="2026-10-16T00:00:00Z"/></w16cex:commentsExtensible>"#
            ),
        ),
    ]
}

/// A Word package whose main document holds `body` and relates the four
/// comments parts.
fn commented(body: &str) -> Package {
    let document = format!("<w:document {W}><w:body>{body}</w:body></w:document>");
    let mut word = package(&[("/word/document.xml", MAIN, &document)]);
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

/// The four comments parts once comment 7 is removed.
fn without_seven() -> [String; 4] {
    [
        format!(
            "<w:comments {W} {W14}>{}</w:comments>",
            comment("8", &["00000003"])
        ),
        format!(
            r#"<w15:commentsEx {W15}><w15:commentEx w15:paraId="00000003" w15:done="0"/></w15:commentsEx>"#
        ),
        format!(
            r#"<w16cid:commentsIds {W16CID}><w16cid:commentId w16cid:paraId="00000003" w16cid:durableId="22222222"/></w16cid:commentsIds>"#
        ),
        format!(
            r#"<w16cex:commentsExtensible {W16CEX}><w16cex:commentExtensible w16cex:durableId="22222222" w16cex:dateUtc="2026-10-16T00:00:00Z"/></w16cex:commentsExtensible>"#
        ),
    ]
}

/// The bytes of the four comments parts, as text.
fn comments_texts(word: &Package) -> [String; 4] {
    comments_parts().map(|(part, _, _)| text(word, part))
}

#[test]
fn a_removed_comment_leaves_every_comments_part_and_the_others_stay() {
    let mut word = commented(&format!("{}{}", removable("7"), kept("8")));
    assert_eq!(
        compact_package(&mut word, Comments::Remove).unwrap(),
        Compaction {
            merged: 1,
            parts: 5,
            removed_comments: 1,
            ..Compaction::default()
        }
    );
    assert_eq!(comments_texts(&word), without_seven());
    assert_eq!(
        text(&word, "/word/document.xml"),
        format!(
            "<w:document {W}><w:body><w:p><w:r><w:t>ab</w:t></w:r></w:p>{}</w:body></w:document>",
            kept("8")
        )
    );
}

#[test]
fn a_kept_comment_leaves_every_comments_part_as_it_was() {
    let mut word = commented(&format!("{}{}", removable("7"), kept("8")));
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
    let mut word = commented(&kept("8"));
    let header = name("/word/header1.xml");
    word.set_part_bytes(
        &header,
        HEADER,
        format!("<w:hdr {W}>{}</w:hdr>", removable("7")).as_bytes(),
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
    assert_eq!(comments_texts(&word), without_seven());
}

#[test]
fn a_comment_removed_from_the_glossary_leaves_the_comments_part_of_the_glossary() {
    let mut word = commented(&format!("{}{}", kept("7"), kept("8")));
    let main = name("/word/document.xml");
    let glossary = name("/word/glossary/document.xml");
    let glossary_comments = name("/word/glossary/comments.xml");
    word.set_part_bytes(
        &glossary,
        GLOSSARY,
        format!(
            "<w:glossaryDocument {W}><w:docParts><w:docPart><w:docPartBody>{}</w:docPartBody></w:docPart></w:docParts></w:glossaryDocument>",
            removable("7")
        )
        .as_bytes(),
    )
    .unwrap();
    word.relate(Some(&main), &glossary, GLOSSARY_RELATIONSHIP)
        .unwrap();
    word.set_part_bytes(
        &glossary_comments,
        COMMENTS,
        format!(
            "<w:comments {W} {W14}>{}</w:comments>",
            comment("7", &["00000009"])
        )
        .as_bytes(),
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
        text(&word, "/word/glossary/comments.xml"),
        format!("<w:comments {W} {W14}/>")
    );
}
