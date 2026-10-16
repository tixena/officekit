use super::{
    element_headings, part_definitions, relationship_sections, simple_type_headings,
    transitional_parts, transitional_relationships,
};

#[test]
fn a_body_heading_counts_and_its_table_of_contents_line_does_not() {
    let text = "     17.3.1.22 p (Paragraph) ........................ 235\n17.3.1.22        p (Paragraph)\n17.4.42       tblCellMar (Table Cell Margin Defaults)\n17.18.44 ST_Jc (Horizontal Alignment Type)\n17.3.1 Paragraphs\n";
    let headings: Vec<(String, String, String)> = element_headings(text)
        .into_iter()
        .map(|h| (h.section, h.local, h.title))
        .collect();
    let expected = [
        ("17.3.1.22", "p", "Paragraph"),
        ("17.4.42", "tblCellMar", "Table Cell Margin Defaults"),
    ];
    assert_eq!(
        headings,
        expected.map(|(s, l, t)| (s.to_owned(), l.to_owned(), t.to_owned()))
    );
    let simple: Vec<String> = simple_type_headings(text)
        .into_iter()
        .map(|h| h.title)
        .collect();
    assert_eq!(simple, ["Horizontal Alignment Type"]);
}

#[test]
fn a_part_definition_reads_wrapped_labels_across_a_page_break() {
    let text = concat!(
        "13.3.4         Notes Master Part\n",
        "Content Type:      application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\n",
        "\n\n\n110\n\u{c}                                    13. PresentationML\n\n\n",
        "Root             http://purl.oclc.org/ooxml/presentationml/main\n",
        "Namespace:\n",
        "Source           http://purl.oclc.org/ooxml/officeDocument/relationships/notesMaster\n",
        "Relationship:\n\n\n",
        "An instance of this part type contains information about the content and formatting of all notes pages.\n",
        "The root element for a part of this content type shall be notesMaster.\n",
        "11.3.10         Main Document Part\n",
        "Content           application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\n",
        "Type(s):          application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml\n",
        "Root              http://purl.oclc.org/ooxml/wordprocessingml/main\n",
        "Namespace:\n",
        "15.2.16         Thumbnail Part\n",
        "Content Type:    Any supported image type.\n",
    );
    let parts = part_definitions(text);
    assert_eq!(parts.len(), 3);
    assert_eq!(
        parts[0].content_types,
        ["application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"]
    );
    assert_eq!(
        parts[0].root_namespace.as_deref(),
        Some("http://purl.oclc.org/ooxml/presentationml/main")
    );
    assert_eq!(
        parts[0].source_relationship.as_deref(),
        Some("http://purl.oclc.org/ooxml/officeDocument/relationships/notesMaster")
    );
    assert_eq!(parts[0].root_element.as_deref(), Some("notesMaster"));
    assert_eq!(parts[1].content_types.len(), 2);
    assert_eq!(parts[1].source_relationship, None);
    assert_eq!(
        parts[2].content_type_text.as_deref(),
        Some("Any supported image type.")
    );
}

#[test]
fn a_definition_that_names_its_part_names_the_root_element_too() {
    let text = concat!(
        "11.3.9          Header Part\n",
        "Content Type:     application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\n",
        "Root              http://purl.oclc.org/ooxml/wordprocessingml/main\n",
        "Namespace:\n",
        "Source            http://purl.oclc.org/ooxml/officeDocument/relationships/header\n",
        "Relationship:\n\n\n",
        "An instance of this part type contains the information about a header.\n",
        "The root element for a Header part shall be hdr.\n",
        "11.3.11         Numbering Definitions Part\n",
        "Content Type:     application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\n",
        "The root element for a Numbering Definition part shall be numbering, with each numbering\n",
        "definition being\n",
        "15.2.1          Additional Characteristics Part\n",
        "Content Type:     application/xml\n",
        "A package is permitted to contain zero or one Additional Characteristics parts.\n",
    );
    let roots: Vec<Option<String>> = part_definitions(text)
        .into_iter()
        .map(|part| part.root_element)
        .collect();
    assert_eq!(
        roots,
        [Some("hdr".to_owned()), Some("numbering".to_owned()), None]
    );
}

#[test]
fn a_transitional_part_names_its_part_1_section() {
    let text = "9.2.10          Main Document Part (Part 1, \u{a7}11.3.10)\nRoot              http://schemas.openxmlformats.org/wordprocessingml/2006/main\nNamespace:\nSource            http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\nRelationship:\n";
    let parts = transitional_parts(text);
    assert_eq!(parts[0].part_1_section, "11.3.10");
    assert_eq!(
        parts[0].root_namespace.as_deref(),
        Some("http://schemas.openxmlformats.org/wordprocessingml/2006/main")
    );
}

#[test]
fn a_wrapped_uri_is_joined_without_a_space_and_a_residue_after_it_is_dropped() {
    let text = concat!(
        "10.2.22         Volatile Dependencies Part (Part 1, §12.3.22)\n",
        "Root              http://schemas.openxmlformats.org/spreadsheetml/2006/main\n",
        "Namespace:\n",
        "Source            http://schemas.openxmlformats.org/officeDocument/2006/relationships/volatileDependencie\n",
        "Relationship:     s\n\n",
        "13.2.10         Core File Properties Part (Part 1, §15.2.12.1)\n",
        "Source            http://schemas.openxmlformats.org/officedocument/2006/relationships/metadata/core-\n",
        "Relationship:     properties\n\n",
    );
    let parts = transitional_parts(text);
    let relationships: Vec<_> = parts
        .iter()
        .map(|part| part.source_relationship.as_deref())
        .collect();
    assert_eq!(
        relationships[0],
        Some(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/volatileDependencies"
        )
    );
    assert_eq!(
        relationships[1],
        Some(
            "http://schemas.openxmlformats.org/officedocument/2006/relationships/metadata/core-properties"
        )
    );
    let notes_slide = concat!(
        "13.3.5          Notes Slide Part\n",
        "Content Type:     application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\n",
        "Root              http://purl.oclc.org/ooxml/presentationml/main ain\n",
        "Namespace:\n",
        "Source           http://purl.oclc.org/ooxml/officeDocument/relationships/notesSlide\n",
    );
    let definitions = part_definitions(notes_slide);
    assert_eq!(
        definitions[0].root_namespace.as_deref(),
        Some("http://purl.oclc.org/ooxml/presentationml/main")
    );
}

#[test]
fn a_prose_value_is_joined_with_spaces() {
    let text = concat!(
        "9.2.12          VML Drawing Part (Part 1, \u{a7}11.3.12)\n",
        "Root             not applicable\n",
        "Namespace:\n",
        "Source           http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing\n",
    );
    let parts = transitional_parts(text);
    assert_eq!(parts[0].root_namespace.as_deref(), Some("not applicable"));
}

#[test]
fn a_relationship_section_outside_the_part_definitions_states_its_type() {
    let text = concat!(
        "11.3.1          Alternative Format Import Part\n",
        "Source           http://purl.oclc.org/ooxml/officeDocument/relationships/aFChunk\n",
        "Relationship:\n\n",
        "11.4        Document Template\n",
        "Source           http://purl.oclc.org/ooxml/officeDocument/relationships/attachedTemplate\n",
        "Relationship:\n\n\n",
        "A document template can be represented by an instance of a WordprocessingML package.\n",
        "15.3       Hyperlinks\n",
        "Source           http://purl.oclc.org/ooxml/officeDocument/relationships/hyperlink\n",
        "Relationship:\n\n",
        "15.4       Custom Properties\n",
        "A section with no field.\n",
    );
    let sections: Vec<_> = relationship_sections(text)
        .into_iter()
        .map(|found| (found.section, found.title, found.source_relationship))
        .collect();
    assert_eq!(
        sections,
        [
            (
                "11.4".to_owned(),
                "Document Template".to_owned(),
                "http://purl.oclc.org/ooxml/officeDocument/relationships/attachedTemplate"
                    .to_owned()
            ),
            (
                "15.3".to_owned(),
                "Hyperlinks".to_owned(),
                "http://purl.oclc.org/ooxml/officeDocument/relationships/hyperlink".to_owned()
            ),
        ]
    );
}

#[test]
fn a_uri_split_after_a_slash_is_joined_to_the_next_word_of_its_line() {
    let text = concat!(
        "11.3      HTML Publish Location (Part 1, \u{a7}13.4)\n",
        "Source            http://schemas.openxmlformats.org/officeDocument/2006/relationships/ htmlPubSaveAs\n",
        "Relationship:\n\n",
        "11.4      Slide Synchronization Server Location (Part 1, \u{a7}13.5)\n",
        "Source            http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideUpdateUrl\n",
        "Relationship:\n",
    );
    let restated = transitional_relationships(text);
    assert_eq!(restated[0].part_1_section, "13.4");
    assert_eq!(
        restated[0].source_relationship.as_deref(),
        Some("http://schemas.openxmlformats.org/officeDocument/2006/relationships/htmlPubSaveAs")
    );
    assert_eq!(restated[1].part_1_section, "13.5");
}
