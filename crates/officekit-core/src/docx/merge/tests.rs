#![cfg(feature = "transitional")]

use officekit_markup::wordprocessing::{self, Document, DocumentBody};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::OpenOptions;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};
use officekit_parts::kinds::WordprocessingKind;
use officekit_parts::{
    endnotes, footnotes, numbering_definitions, style_definitions, wordprocessing_comments,
};
use officekit_xml::element::Element;
use officekit_xml::write::{Layout, Scope, WriteOptions};

use super::super::Docx;

/// The declarations of every fixture root but the source's main document's.
const ROOT: &str = concat!(
    "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" ",
    "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" ",
    "xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" ",
    "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ",
    "xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" ",
    "xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
    "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" ",
    "xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\""
);

/// The bytes of every image part.
const PNG: &[u8] = b"\x89PNG not really";

/// The source root's declarations beyond [`ROOT`], as `LibreOffice` writes them, and the prefixes
/// it ignores.
const SOURCE_ROOT: &str = concat!(
    "xmlns:wp14=\"http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing\" ",
    "xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\" ",
    "mc:Ignorable=\"w14 wp14 w15\""
);

/// A `.docx` whose main document is `document`.
fn docx(document: &str) -> Docx {
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let main = docx.main_document_part().clone();
    docx.package_mut()
        .set_part_bytes(
            &main,
            WordprocessingKind::Document.main_content_type(),
            document.as_bytes(),
        )
        .unwrap();
    docx
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

/// Import `source`'s body into `target`; the blocks are written as a `w:body` in the scope of
/// `target`'s new root, and `target`'s main document is not stored.
fn import(target: &mut Docx, source: &Docx) -> (Document, Element) {
    let mut document = target.main_document().unwrap();
    let blocks = target.import_body(&mut document, source).unwrap();
    let mut body = DocumentBody::default();
    body.content = blocks;
    let mut scope = Scope::default();
    scope.start(&document.unknown.namespace_declarations);
    let element = wordprocessing::write::document_body(&body, &mut scope);
    (document, element)
}

/// Every value of an attribute, by local name, on the elements of a local name, in document
/// order.
fn values(element: &Element, element_local: &str, attribute_local: &str) -> Vec<String> {
    let mut found: Vec<String> = if element.name.local == element_local {
        element
            .attributes
            .iter()
            .filter(|attribute| attribute.name.local == attribute_local)
            .map(|attribute| attribute.value.clone())
            .collect()
    } else {
        Vec::new()
    };
    for child in element.child_elements() {
        found.extend(values(child, element_local, attribute_local));
    }
    found
}

fn numbers(element: &Element, element_local: &str, attribute_local: &str) -> Vec<i64> {
    values(element, element_local, attribute_local)
        .iter()
        .map(|value| value.parse().unwrap())
        .collect()
}

/// The part the main part relates to first by `relationship`, and its text.
fn related_text(docx: &Docx, relationship: &RelationshipType) -> (PartName, String) {
    let part = docx
        .package()
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
        .unwrap();
    let text = String::from_utf8(docx.package().part_bytes(&part).unwrap()).unwrap();
    (part, text)
}

fn name(text: &str) -> PartName {
    PartName::new(text).unwrap()
}

fn root_of(docx: &Docx, part: &PartName) -> Element {
    docx.package()
        .load_tree(part, OpenOptions::default())
        .unwrap()
        .root
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

fn notes(local: &str, entries: &str) -> String {
    format!("<w:{local}s {ROOT} {SOURCE_ROOT}>{entries}</w:{local}s>")
}

fn separators(local: &str, first: i64) -> String {
    format!(
        "<w:{local} w:type=\"separator\" w:id=\"{first}\"><w:p><w:r><w:separator/></w:r></w:p></w:{local}><w:{local} w:type=\"continuationSeparator\" w:id=\"{}\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:{local}>",
        first + 1
    )
}

/// A target shaped as `pagare` is for the notes import: footnotes and endnotes holding only their
/// separators, -1 and 0, a `Normal` style, a bookmark, and neither comments nor numbering.
fn notes_target() -> Docx {
    let mut target = docx(&format!(
        "<w:document {ROOT} mc:Ignorable=\"w14\"><w:body><w:p><w:bookmarkStart w:id=\"0\" w:name=\"own\"/><w:r><w:t>Own</w:t></w:r><w:bookmarkEnd w:id=\"0\"/></w:p><w:sectPr/></w:body></w:document>"
    ));
    let main = "/word/document.xml";
    add(
        &mut target,
        main,
        "rId1",
        ("/word/styles.xml", style_definitions::CONTENT_TYPE, RelationshipType::STYLES),
        format!("<w:styles {ROOT}><w:style w:type=\"paragraph\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:style></w:styles>").as_bytes(),
    );
    for (id, local, content_type, relationship) in [
        (
            "rId2",
            "footnote",
            footnotes::CONTENT_TYPE,
            RelationshipType::FOOTNOTES,
        ),
        (
            "rId3",
            "endnote",
            endnotes::CONTENT_TYPE,
            RelationshipType::ENDNOTES,
        ),
    ] {
        add(
            &mut target,
            main,
            id,
            (&format!("/word/{local}s.xml"), content_type, relationship),
            format!("<w:{local}s {ROOT}>{}</w:{local}s>", separators(local, -1)).as_bytes(),
        );
    }
    target
}

/// A source made as `LibreOffice` makes `notes_source`: a footnote, an endnote, a comment, a
/// bookmark, a hyperlink, a two-item list, and the paragraph style `ClausulaImportada`, with the
/// styles it names.
fn notes_source() -> Docx {
    let mut source = docx(&format!(
        "<w:document {ROOT} {SOURCE_ROOT}><w:body>\
<w:p w14:paraId=\"1A2B3C4D\" w14:textId=\"77777777\"><w:pPr><w:pStyle w:val=\"ClausulaImportada\"/></w:pPr><w:r><w:t>Imported clause</w:t></w:r><w:r><w:footnoteReference w:id=\"2\"/></w:r><w:r><w:endnoteReference w:id=\"2\"/></w:r></w:p>\
<w:p><w:pPr><w:pStyle w:val=\"Normal\"/></w:pPr><w:commentRangeStart w:id=\"0\"/><w:bookmarkStart w:id=\"0\" w:name=\"imported_mark\"/><w:r><w:t>a bookmark</w:t></w:r><w:bookmarkEnd w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r><w:hyperlink r:id=\"rId2\"><w:r><w:t>a hyperlink</w:t></w:r></w:hyperlink></w:p>\
<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>First</w:t></w:r></w:p>\
<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>Second</w:t></w:r></w:p>\
<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:body></w:document>"
    ));
    let main = "/word/document.xml";
    let style = |id: &str, more: &str| {
        format!(
            "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{id}\"/>{more}</w:style>"
        )
    };
    let styles = [
        style("Normal", "<w:rPr><w:sz w:val=\"24\"/></w:rPr>"),
        style("ClausulaImportada", "<w:basedOn w:val=\"BaseImportada\"/><w:next w:val=\"Normal\"/><w:link w:val=\"ClausulaChar\"/>"),
        style("BaseImportada", "<w:basedOn w:val=\"Normal\"/>"),
        "<w:style w:type=\"character\" w:styleId=\"ClausulaChar\"><w:name w:val=\"ClausulaChar\"/><w:link w:val=\"ClausulaImportada\"/></w:style>".to_owned(),
        style("FootnoteText", "<w:basedOn w:val=\"Normal\"/>"),
        style("Unused", ""),
    ]
    .concat();
    add(
        &mut source,
        main,
        "rId1",
        (
            "/word/styles.xml",
            style_definitions::CONTENT_TYPE,
            RelationshipType::STYLES,
        ),
        format!("<w:styles {ROOT} {SOURCE_ROOT}>{styles}</w:styles>").as_bytes(),
    );
    link(&mut source, main, "rId2", "https://example.org/imported");
    let footnote = "<w:footnote w:id=\"2\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr><w:r><w:footnoteRef/></w:r><w:hyperlink r:id=\"rId1\"><w:r><w:t>Imported footnote text.</w:t></w:r></w:hyperlink></w:p></w:footnote>";
    add(
        &mut source,
        main,
        "rId3",
        (
            "/word/footnotes.xml",
            footnotes::CONTENT_TYPE,
            RelationshipType::FOOTNOTES,
        ),
        notes(
            "footnote",
            &format!("{}{footnote}", separators("footnote", 0)),
        )
        .as_bytes(),
    );
    link(
        &mut source,
        "/word/footnotes.xml",
        "rId1",
        "https://example.org/note",
    );
    add(
        &mut source,
        main,
        "rId4",
        ("/word/endnotes.xml", endnotes::CONTENT_TYPE, RelationshipType::ENDNOTES),
        notes("endnote", &format!("{}<w:endnote w:id=\"2\"><w:p><w:r><w:t>Imported endnote text.</w:t></w:r></w:p></w:endnote>", separators("endnote", 0))).as_bytes(),
    );
    add(
        &mut source,
        main,
        "rId5",
        ("/word/comments.xml", wordprocessing_comments::CONTENT_TYPE, RelationshipType::COMMENTS),
        notes("comment", "<w:comment w:id=\"0\" w:author=\"Reviewer\"><w:p><w:r><w:t>Imported comment text.</w:t></w:r></w:p></w:comment>").as_bytes(),
    );
    add(
        &mut source,
        main,
        "rId6",
        ("/word/numbering.xml", numbering_definitions::CONTENT_TYPE, RelationshipType::NUMBERING),
        format!("<w:numbering {ROOT}><w:abstractNum w:abstractNumId=\"1\"><w:nsid w:val=\"0A1B2C3D\"/><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"bullet\"/></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>").as_bytes(),
    );
    source
}

/// The notes source imported into the notes target, every declaration kept on write.
fn notes_import() -> (Docx, Document, Element) {
    let mut target = notes_target();
    target.set_write_options(WriteOptions::new(Layout::Compact, false));
    let (document, body) = import(&mut target, &notes_source());
    (target, document, body)
}

#[test]
fn notes_come_in_after_the_separators_and_a_comment_into_a_new_part() {
    let (target, _, body) = notes_import();
    // The target's notes hold the separators -1 and 0, so the copies take 1.
    assert_eq!(values(&body, "footnoteReference", "id"), ["1"]);
    assert_eq!(values(&body, "endnoteReference", "id"), ["1"]);
    let (footnotes_part, footnotes) = related_text(&target, &RelationshipType::FOOTNOTES);
    assert!(footnotes.contains("<w:footnote w:id=\"1\">"), "{footnotes}");
    assert!(footnotes.contains("Imported footnote text."));
    let (_, endnotes) = related_text(&target, &RelationshipType::ENDNOTES);
    assert!(endnotes.contains("<w:endnote w:id=\"1\"><w:p><w:r><w:t>Imported endnote text."));
    // The footnote's own hyperlink is related again from this file's footnotes part.
    let note_link = values(&root_of(&target, &footnotes_part), "hyperlink", "id");
    assert_eq!(
        target_of(&target, footnotes_part.as_str(), &note_link[0]),
        Target::External("https://example.org/note".to_owned())
    );
    // No comments part here: one is made, related from the main part, declaring what the
    // source's declares, and the comment keeps id 0.
    assert_eq!(values(&body, "commentRangeStart", "id"), ["0"]);
    assert_eq!(values(&body, "commentRangeEnd", "id"), ["0"]);
    assert_eq!(values(&body, "commentReference", "id"), ["0"]);
    let (comments_part, comments) = related_text(&target, &RelationshipType::COMMENTS);
    assert_eq!(comments_part.as_str(), "/word/comments1.xml");
    assert!(comments.contains("<w:comment w:id=\"0\" w:author=\"Reviewer\">"));
    assert!(
        comments.contains("xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"")
    );
    assert!(comments.contains("mc:Ignorable=\"w14 wp14 w15\""));
}

#[test]
fn a_hyperlink_names_a_new_external_relationship_of_the_main_part() {
    let (target, _, body) = notes_import();
    let link = values(&body, "hyperlink", "id");
    let fresh = notes_target();
    assert!(
        !fresh
            .package()
            .related(Some(fresh.main_document_part()))
            .unwrap()
            .iter()
            .any(|related| related.id == link[0])
    );
    assert_eq!(
        target_of(&target, "/word/document.xml", &link[0]),
        Target::External("https://example.org/imported".to_owned())
    );
}

#[test]
fn a_list_comes_in_as_list_1_over_abstract_numbering_0_in_a_new_part() {
    let (target, _, body) = notes_import();
    assert_eq!(values(&body, "numId", "val"), ["1", "1"]);
    let (_, numbering) = related_text(&target, &RelationshipType::NUMBERING);
    assert!(
        numbering.contains("<w:abstractNum w:abstractNumId=\"0\"><w:nsid w:val=\"0A1B2C3D\"/>")
    );
    assert!(numbering.contains("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>"));
}

#[test]
fn missing_styles_come_with_the_styles_they_name_and_defined_ones_stay() {
    let (target, _, _) = notes_import();
    let (_, styles) = related_text(&target, &RelationshipType::STYLES);
    for (id, count) in [
        ("ClausulaImportada", 1),
        ("BaseImportada", 1),
        ("ClausulaChar", 1),
        ("FootnoteText", 1),
        ("Normal", 1),
        ("Unused", 0),
    ] {
        assert_eq!(
            styles.matches(&format!("w:styleId=\"{id}\"")).count(),
            count,
            "{id}"
        );
    }
    assert!(styles.contains("<w:sz w:val=\"22\"/>"));
    assert!(!styles.contains("<w:sz w:val=\"24\"/>"));
}

#[test]
fn the_root_gains_the_source_root_declarations_and_other_attributes_stay() {
    let (_, document, body) = notes_import();
    let ignorable = document
        .unknown
        .attributes
        .iter()
        .find(|attribute| attribute.name.local == "Ignorable")
        .unwrap();
    assert_eq!(ignorable.value, "w14 wp14 w15");
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
    // Four paragraphs, without the final `w:sectPr`.
    assert!(body.child_elements().all(|block| block.name.local == "p"));
    assert_eq!(body.child_elements().count(), 4);
    assert_eq!(values(&body, "p", "paraId"), ["1A2B3C4D"]);
    assert_eq!(values(&body, "p", "textId"), ["77777777"]);
    // The bookmark takes an id above the target's.
    assert_eq!(values(&body, "bookmarkStart", "id"), ["1"]);
    assert_eq!(values(&body, "bookmarkEnd", "id"), ["1"]);
}

/// A document shaped as `formulario_cumplimiento_fatca` is for the self import: lists 1 and 2 over
/// abstract numberings 0 and 1, the body using list 1, bookmarks, and drawings.
fn lists() -> Docx {
    let drawing = |id: &str| {
        format!(
            "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"{id}\" name=\"Picture {id}\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"/></a:graphic></wp:inline></w:drawing></w:r>"
        )
    };
    let item = |bookmark: &str, more: &str| {
        format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:bookmarkStart w:id=\"{bookmark}\" w:name=\"m{bookmark}\"/><w:r><w:t>Item</w:t></w:r><w:bookmarkEnd w:id=\"{bookmark}\"/>{more}</w:p>"
        )
    };
    let mut docx = docx(&format!(
        "<w:document {ROOT}><w:body>{}{}<w:p><w:pPr><w:numPr><w:numId w:val=\"0\"/></w:numPr></w:pPr></w:p><w:sectPr/></w:body></w:document>",
        item("3", &drawing("4")),
        item("7", &drawing("6")),
    ));
    let abstract_num = |id: &str, nsid: &str| {
        format!(
            "<w:abstractNum w:abstractNumId=\"{id}\"><w:nsid w:val=\"{nsid}\"/><w:lvl w:ilvl=\"0\"><w:numFmt w:val=\"decimal\"/></w:lvl></w:abstractNum>"
        )
    };
    add(
        &mut docx,
        "/word/document.xml",
        "rId1",
        ("/word/numbering.xml", numbering_definitions::CONTENT_TYPE, RelationshipType::NUMBERING),
        format!(
            "<w:numbering {ROOT}>{}{}<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>",
            abstract_num("0", "11111111"),
            abstract_num("1", "22222222"),
        )
        .as_bytes(),
    );
    docx
}

#[test]
fn a_document_brought_into_itself_gets_a_new_list_and_ids_above_its_own() {
    let mut target = lists();
    let original =
        wordprocessing::write::document(&target.main_document().unwrap(), &mut Scope::default());
    let (_, body) = import(&mut target, &lists());
    // Lists 1 and 2 are over abstract numberings 0 and 1: the source's list 1 becomes list 3 over
    // a copy of its abstract numbering, 2, each after the last entry of its kind; 0 stays.
    assert_eq!(values(&body, "numId", "val"), ["3", "3", "0"]);
    let (_, numbering) = related_text(&target, &RelationshipType::NUMBERING);
    let order: Vec<&str> = numbering
        .split("<w:")
        .skip(1)
        .filter_map(|tag| {
            ["abstractNum w:abstractNumId=\"", "num w:numId=\""]
                .iter()
                .find_map(|start| tag.strip_prefix(start))
                .map(|rest| rest.split('"').next().unwrap())
        })
        .collect();
    assert_eq!(order, ["0", "1", "2", "1", "2", "3"]);
    assert!(
        numbering.contains("<w:abstractNum w:abstractNumId=\"2\"><w:nsid w:val=\"11111111\"/>")
    );
    assert!(numbering.contains("<w:num w:numId=\"3\"><w:abstractNumId w:val=\"2\"/></w:num>"));
    // Every bookmark and drawing id sits above every one the file uses; each bookmark pair keeps
    // one id between its start and its end, and each drawing takes its own.
    let highest = |element: &str| numbers(&original, element, "id").into_iter().max().unwrap();
    assert_eq!(
        numbers(&body, "bookmarkStart", "id"),
        numbers(&body, "bookmarkEnd", "id")
    );
    assert!(
        numbers(&body, "bookmarkStart", "id")
            .iter()
            .all(|id| *id > highest("bookmarkStart"))
    );
    assert_eq!(numbers(&body, "bookmarkStart", "id"), [8, 9]);
    assert!(
        numbers(&body, "docPr", "id")
            .iter()
            .all(|id| *id > highest("docPr"))
    );
    assert_eq!(numbers(&body, "docPr", "id"), [7, 8]);
}

#[test]
fn range_ids_start_above_the_notes_parts_and_map_each_value_once() {
    let mut target = docx(&format!(
        "<w:document {ROOT}><w:body><w:p><w:bookmarkStart w:id=\"2\" w:name=\"own\"/><w:bookmarkEnd w:id=\"2\"/></w:p></w:body></w:document>"
    ));
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        ("/word/comments.xml", wordprocessing_comments::CONTENT_TYPE, RelationshipType::COMMENTS),
        format!("<w:comments {ROOT}><w:comment w:id=\"0\"><w:p><w:bookmarkStart w:id=\"40\" w:name=\"note\"/><w:bookmarkEnd w:id=\"40\"/></w:p></w:comment></w:comments>").as_bytes(),
    );
    let source = docx(&format!(
        "<w:document {ROOT}><w:body><w:p><w:bookmarkStart w:id=\"0\" w:name=\"a\"/><w:ins w:id=\"5\" w:author=\"A\"><w:r><w:t>x</w:t></w:r></w:ins><w:bookmarkEnd w:id=\"0\"/><w:del w:id=\"0\" w:author=\"A\"><w:r><w:delText>y</w:delText></w:r></w:del></w:p></w:body></w:document>"
    ));
    let (_, body) = import(&mut target, &source);
    assert_eq!(values(&body, "bookmarkStart", "id"), ["41"]);
    assert_eq!(values(&body, "bookmarkEnd", "id"), ["41"]);
    assert_eq!(values(&body, "ins", "id"), ["42"]);
    assert_eq!(values(&body, "del", "id"), ["41"]);
}

/// A target with an image and a header that relates to it.
fn pictures_target() -> Docx {
    let mut target = docx(&format!(
        "<w:document {ROOT}><w:body><w:p/></w:body></w:document>"
    ));
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        (
            "/word/media/image1.png",
            "image/png",
            RelationshipType::IMAGE,
        ),
        PNG,
    );
    add(
        &mut target,
        "/word/document.xml",
        "rId2",
        (
            "/word/header1.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
            RelationshipType::HEADER,
        ),
        format!("<w:hdr {ROOT}><w:p/></w:hdr>").as_bytes(),
    );
    add(
        &mut target,
        "/word/header1.xml",
        "rId1",
        (
            "/word/media/image1.png",
            "image/png",
            RelationshipType::IMAGE,
        ),
        PNG,
    );
    target
}

/// A source whose body names one image by two ids, a chart that relates to a workbook and to the
/// same image, and a header that relates to the image too.
fn pictures_source() -> Docx {
    let picture = |id: &str| {
        format!(
            "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"1\" name=\"p\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:blipFill><a:blip r:embed=\"{id}\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )
    };
    let chart = "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"2\" name=\"c\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"rId5\"/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>";
    let mut source = docx(&format!(
        "<w:document {ROOT}><w:body><w:p>{}{}{chart}</w:p><w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId6\"/></w:sectPr></w:pPr></w:p><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId6\"/></w:sectPr></w:body></w:document>",
        picture("rId3"),
        picture("rId4"),
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
        b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/>",
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
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
            RelationshipType::HEADER,
        ),
        format!("<w:hdr {ROOT}><w:p/></w:hdr>").as_bytes(),
    );
    add(&mut source, "/word/header1.xml", "rId1", image, PNG);
    source
}

#[test]
fn parts_come_under_free_names_with_their_relationships_and_each_once() {
    let mut target = pictures_target();
    let (_, body) = import(&mut target, &pictures_source());
    let image = Target::Part(name("/word/media/image2.png"));
    let embeds = values(&body, "blip", "embed");
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
    let chart = values(&body, "chart", "id");
    assert_eq!(
        target_of(&target, "/word/document.xml", &chart[0]),
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
    let headers = values(&body, "headerReference", "id");
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
    assert_eq!(numbers(&body, "docPr", "id"), [1, 2, 3]);
}

#[test]
fn a_picture_bullet_and_its_image_come_with_the_list() {
    let pict = |id: &str, embed: &str| {
        format!(
            "<w:numPicBullet w:numPicBulletId=\"{id}\"><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"9\" name=\"b\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:blipFill><a:blip r:embed=\"{embed}\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:numPicBullet>"
        )
    };
    let list = |abstract_id: &str, bullet: &str| {
        format!(
            "<w:abstractNum w:abstractNumId=\"{abstract_id}\"><w:lvl w:ilvl=\"0\"><w:lvlPicBulletId w:val=\"{bullet}\"/></w:lvl></w:abstractNum>"
        )
    };
    let mut target = docx(&format!(
        "<w:document {ROOT}><w:body><w:p/></w:body></w:document>"
    ));
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        ("/word/numbering.xml", numbering_definitions::CONTENT_TYPE, RelationshipType::NUMBERING),
        format!("<w:numbering {ROOT}>{}{}<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:numIdMacAtCleanup w:val=\"1\"/></w:numbering>", pict("0", "rId1"), list("0", "0")).as_bytes(),
    );
    add(
        &mut target,
        "/word/numbering.xml",
        "rId1",
        (
            "/word/media/image1.png",
            "image/png",
            RelationshipType::IMAGE,
        ),
        PNG,
    );
    let mut source = docx(&format!(
        "<w:document {ROOT}><w:body><w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"4\"/></w:numPr></w:pPr></w:p></w:body></w:document>"
    ));
    add(
        &mut source,
        "/word/document.xml",
        "rId1",
        ("/word/numbering.xml", numbering_definitions::CONTENT_TYPE, RelationshipType::NUMBERING),
        format!("<w:numbering {ROOT}>{}{}<w:num w:numId=\"4\"><w:abstractNumId w:val=\"6\"/></w:num></w:numbering>", pict("5", "rId3"), list("6", "5")).as_bytes(),
    );
    add(
        &mut source,
        "/word/numbering.xml",
        "rId3",
        (
            "/word/media/image1.png",
            "image/png",
            RelationshipType::IMAGE,
        ),
        PNG,
    );
    let (_, body) = import(&mut target, &source);
    assert_eq!(values(&body, "numId", "val"), ["2"]);
    let (numbering_part, numbering) = related_text(&target, &RelationshipType::NUMBERING);
    let tree = target
        .package()
        .load_tree(&numbering_part, OpenOptions::default())
        .unwrap();
    let entries: Vec<(String, String)> = tree
        .root
        .child_elements()
        .map(|entry| {
            let id = entry
                .attributes
                .first()
                .map_or_else(String::new, |attribute| attribute.value.clone());
            (entry.name.local.clone(), id)
        })
        .collect();
    let expected = [
        ("numPicBullet", "0"),
        ("numPicBullet", "1"),
        ("abstractNum", "0"),
        ("abstractNum", "1"),
        ("num", "1"),
        ("num", "2"),
        ("numIdMacAtCleanup", "1"),
    ];
    assert_eq!(
        entries,
        expected.map(|(local, id)| (local.to_owned(), id.to_owned()))
    );
    assert!(numbering.contains(
        "<w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:lvlPicBulletId w:val=\"1\"/>"
    ));
    // The bullet's image is related from this file's numbering part.
    let embeds = values(&tree.root, "blip", "embed");
    assert_eq!(embeds[0], "rId1");
    assert_eq!(
        target_of(&target, numbering_part.as_str(), &embeds[1]),
        Target::Part(name("/word/media/image2.png"))
    );
}

#[test]
fn a_prefix_bound_to_another_namespace_is_a_read_error_and_changes_nothing() {
    let mut target = docx(&format!(
        "<w:document {ROOT} xmlns:x=\"urn:example:one\"><w:body><w:p/></w:body></w:document>"
    ));
    let source = docx(&format!(
        "<w:document {ROOT} xmlns:x=\"urn:example:two\" xmlns:y=\"urn:example:three\"><w:body><w:p/></w:body></w:document>"
    ));
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
    let styles =
        |uri: &str, ids: &str| format!("<w:styles {ROOT} xmlns:x=\"{uri}\">{ids}</w:styles>");
    let mut target = docx(&format!(
        "<w:document {ROOT}><w:body><w:p/></w:body></w:document>"
    ));
    add(
        &mut target,
        "/word/document.xml",
        "rId1",
        (
            "/word/styles.xml",
            style_definitions::CONTENT_TYPE,
            RelationshipType::STYLES,
        ),
        styles("urn:example:one", "").as_bytes(),
    );
    let mut source = docx(&format!(
        "<w:document {ROOT}><w:body><w:p><w:pPr><w:pStyle w:val=\"Other\"/></w:pPr></w:p></w:body></w:document>"
    ));
    add(
        &mut source,
        "/word/document.xml",
        "rId1",
        (
            "/word/styles.xml",
            style_definitions::CONTENT_TYPE,
            RelationshipType::STYLES,
        ),
        styles("urn:example:two", "<w:style w:styleId=\"Other\"/>").as_bytes(),
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
    let mut target = docx(&format!(
        "<w:document {ROOT}><w:body><w:p/></w:body></w:document>"
    ));
    let before = target.package().part_names().unwrap();
    let (_, body) = import(&mut target, &lists());
    assert_eq!(values(&body, "numId", "val"), ["1", "1", "0"]);
    assert_eq!(
        target.package().part_names().unwrap().len(),
        before.len() + 1
    );
    let (numbering_part, numbering) = related_text(&target, &RelationshipType::NUMBERING);
    assert_eq!(numbering_part.as_str(), "/word/numbering1.xml");
    assert_eq!(
        target.package().content_type(&numbering_part),
        Some(numbering_definitions::CONTENT_TYPE)
    );
    assert_eq!(numbering.matches("<w:abstractNum ").count(), 1);
    assert!(numbering.contains("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>"));
}
