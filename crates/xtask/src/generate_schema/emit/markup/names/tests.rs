use super::{
    ecma_module, extension_module, field_words, identifier, name_words, pascal, screaming, snake,
    title_words, variant,
};

fn words(text: &str) -> Vec<String> {
    text.split(' ').map(ToOwned::to_owned).collect()
}

#[test]
fn ecma_modules_follow_the_file_names() {
    let modules = [
        ("wml", "wordprocessing"),
        ("sml", "spreadsheet"),
        ("pml", "presentation"),
        ("dml-main", "drawing"),
        ("vml-main", "vml"),
        ("shared-math", "math"),
        ("dml-wordprocessingDrawing", "wordprocessing_drawing"),
        ("vml-officeDrawing", "vml_office_drawing"),
        ("dml-chart", "chart"),
        ("shared-commonSimpleTypes", "common_simple_types"),
    ];
    for (stem, module) in modules {
        assert_eq!(ecma_module(stem), module);
    }
}

#[test]
fn an_extension_module_is_its_uri_path_in_snake_case() {
    assert_eq!(
        extension_module("http://schemas.microsoft.com/office/word/2010/wordml").as_deref(),
        Some("office_word_2010_wordml")
    );
    assert_eq!(
        extension_module("http://schemas.openxmlformats.org/wordprocessingml/2006/main"),
        None
    );
}

#[test]
fn a_child_field_drops_its_parents_title() {
    assert_eq!(
        field_words(&words("Paragraph"), &words("Paragraph Properties")),
        words("Properties")
    );
    assert_eq!(
        field_words(&words("Text Run"), &words("Run Properties")),
        words("Properties")
    );
    assert_eq!(
        field_words(&words("Table Row"), &words("Table Row Properties")),
        words("Properties")
    );
    assert_eq!(
        field_words(&words("Paragraph"), &words("Text Run")),
        words("Text Run")
    );
}

#[test]
fn the_connecting_words_at_the_cut_go_with_the_parents_title() {
    let cases = [
        (
            "Picture",
            "Non Visual Properties for a Picture",
            "Non Visual Properties",
        ),
        (
            "Slide Layout",
            "Slide Transition for a Slide Layout",
            "Slide Transition",
        ),
        (
            "Style Definition",
            "Revision Identifier for Style Definition",
            "Revision Identifier",
        ),
        ("Comments", "List of Comments", "List"),
        ("Cell", "Margins for an Cell", "Margins"),
        ("Cell", "Border of a Cell", "Border"),
        ("Table", "Style of the Table", "Style"),
        ("Table", "Table of Authorities", "Authorities"),
        ("Text Run", "Run for a Paragraph", "Paragraph"),
        (
            "Paragraph",
            "Format of Paragraph Marks",
            "Format of Paragraph Marks",
        ),
    ];
    for (parent, child, field) in cases {
        assert_eq!(field_words(&words(parent), &words(child)), words(field));
    }
}

#[test]
fn names_change_case_as_the_rules_ask() {
    assert_eq!(snake("rsidRPr"), "rsid_r_pr");
    assert_eq!(snake("paraId"), "para_id");
    assert_eq!(snake("ID"), "id");
    assert_eq!(
        pascal(&title_words("Don't Use HTML Paragraph Auto Spacing")),
        "DontUseHTMLParagraphAutoSpacing"
    );
    assert_eq!(
        name_words("XMLMappingDataBinding"),
        words("XML Mapping Data Binding")
    );
    assert_eq!(screaming("BlockContent"), "BLOCK_CONTENT");
}

#[test]
fn enumeration_values_become_variants() {
    assert_eq!(variant("mediumKashida"), "MediumKashida");
    assert_eq!(variant("0000"), "Value0000");
    assert_eq!(variant("--"), "MinusMinus");
    assert_eq!(variant(""), "Empty");
    assert_eq!(variant("Self"), "SelfValue");
}

#[test]
fn a_keyword_becomes_a_raw_identifier() {
    assert_eq!(identifier("type").unwrap(), "r#type");
    assert_eq!(identifier("val").unwrap(), "val");
    assert_eq!(
        identifier("self").map_err(|error| error.to_string()),
        Err("`self` cannot be a Rust identifier".to_owned())
    );
}
