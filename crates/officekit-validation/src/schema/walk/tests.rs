use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_xml::name::OfficeVersion;

use crate::error::{ErrorCode, ValidationError};
use crate::validator::Validator;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const STRICT_OFFICE_DOCUMENT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const STYLES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const STYLES_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const TARGETS: [OfficeVersion; 6] = [
    OfficeVersion::Office2010,
    OfficeVersion::Office2013,
    OfficeVersion::Office2016,
    OfficeVersion::Office2019,
    OfficeVersion::Office2021,
    OfficeVersion::Microsoft365,
];
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const BLOCK_PATH: &str =
    "/word/document.xml /w:document[1]/w:body[1]/w:p[1]/w:r[1]/mc:AlternateContent[1]";

/// A Word document whose main part holds `body`, with the `w` prefix bound
/// to `namespace`, related by `relationship`, and a styles part when given.
fn document(namespace: &str, relationship: &str, body: &str, styles: Option<&str>) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n\
         <w:document xmlns:w=\"{namespace}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" xmlns:wps=\"{WPS}\">\
         {body}</w:document>"
    );
    package.set_part_bytes(&main, MAIN, xml.as_bytes()).unwrap();
    package.relate(None, &main, relationship).unwrap();
    if let Some(content) = styles {
        let part = PartName::new("/word/styles.xml").unwrap();
        package
            .set_part_bytes(&part, STYLES, content.as_bytes())
            .unwrap();
        package
            .relate(Some(&main), &part, STYLES_RELATIONSHIP)
            .unwrap();
    }
    package
}

fn errors(package: &Package, target: OfficeVersion) -> Vec<ValidationError> {
    Validator::new(target).with_max_errors(0).validate(package)
}

/// Each error's code and where it is.
fn found(package: &Package, target: OfficeVersion) -> Vec<(ErrorCode, String)> {
    errors(package, target)
        .into_iter()
        .map(|error| {
            let part = error
                .part
                .as_ref()
                .map_or_else(String::new, |name| name.as_str().to_owned());
            (error.code, format!("{part} {}", error.path_text()))
        })
        .collect()
}

fn margins(width: &str) -> Package {
    let styles = format!(
        "<w:styles xmlns:w=\"{W}\"><w:style w:type=\"table\" w:default=\"1\" w:styleId=\"TableNormal\">\
         <w:name w:val=\"Normal Table\"/><w:tblPr><w:tblCellMar>\
         <w:top w:w=\"{width}\" w:type=\"dxa\"/><w:left w:w=\"{width}\" w:type=\"dxa\"/>\
         <w:bottom w:w=\"{width}\" w:type=\"dxa\"/><w:right w:w=\"{width}\" w:type=\"dxa\"/>\
         </w:tblCellMar></w:tblPr></w:style></w:styles>"
    );
    document(W, OFFICE_DOCUMENT, "<w:body/>", Some(&styles))
}

#[test]
fn the_table_cell_margins_of_the_third_party_sample_give_four_errors_at_every_target() {
    let package = margins("100.0");
    let at = |local: &str| {
        format!("/word/styles.xml /w:styles[1]/w:style[1]/w:tblPr[1]/w:tblCellMar[1]/w:{local}[1]")
    };
    for target in TARGETS {
        assert_eq!(
            found(&package, target),
            [
                (ErrorCode::ValueNotInUnion, at("top")),
                (ErrorCode::InvalidAttributeValue, at("left")),
                (ErrorCode::ValueNotInUnion, at("bottom")),
                (ErrorCode::InvalidAttributeValue, at("right")),
            ],
            "{target:?}"
        );
    }
    assert_eq!(found(&margins("5000"), OfficeVersion::Office2013).len(), 0);
}

#[test]
fn the_cap_keeps_the_error_that_reaches_it() {
    let package = margins("100.0");
    let validator = Validator::new(OfficeVersion::Microsoft365);
    assert_eq!(validator.with_max_errors(1).validate(&package).len(), 1);
    assert_eq!(validator.with_max_errors(3).validate(&package).len(), 3);
    assert_eq!(validator.with_max_errors(0).validate(&package).len(), 4);
    assert_eq!(validator.validate(&package).len(), 4);
}

#[test]
fn a_step_counts_the_siblings_before_it_of_the_same_name() {
    let package = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body><w:p/><w:bookmarkStart w:id=\"0\" w:name=\"a\"/><w:p w:bogus=\"1\"/></w:body>",
        None,
    );
    assert_eq!(
        found(&package, OfficeVersion::Office2016),
        [(
            ErrorCode::UndeclaredAttribute,
            "/word/document.xml /w:document[1]/w:body[1]/w:p[2]".to_owned()
        )]
    );
}

#[test]
fn a_child_out_of_order_is_reported_on_its_parent() {
    let package = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body><w:p><w:r/><w:pPr/></w:p></w:body>",
        None,
    );
    assert_eq!(
        found(&package, OfficeVersion::Microsoft365),
        [(
            ErrorCode::UnexpectedChild,
            "/word/document.xml /w:document[1]/w:body[1]/w:p[1]".to_owned()
        )]
    );
}

#[test]
fn an_undeclared_prefix_in_mc_ignorable_is_a_markup_compatibility_error() {
    let mut package = document(W, OFFICE_DOCUMENT, "<w:body/>", None);
    let main = PartName::new("/word/document.xml").unwrap();
    let xml = format!(
        "<w:document xmlns:w=\"{W}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" mc:Ignorable=\"w14 w15\"><w:body/></w:document>"
    );
    package.set_part_bytes(&main, MAIN, xml.as_bytes()).unwrap();
    assert_eq!(
        found(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::UndeclaredPrefixInIgnorable,
            "/word/document.xml /w:document[1]".to_owned()
        )]
    );
}

#[test]
fn an_element_of_an_ignorable_namespace_the_parent_does_not_declare_is_left_out() {
    let ignorable = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body mc:Ignorable=\"w14\"><w14:unknown/><w:p/></w:body>",
        None,
    );
    assert_eq!(found(&ignorable, OfficeVersion::Microsoft365).len(), 0);
    let not_ignorable = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body><w14:unknown/><w:p/></w:body>",
        None,
    );
    assert_eq!(
        found(&not_ignorable, OfficeVersion::Microsoft365),
        [(
            ErrorCode::InvalidChild,
            "/word/document.xml /w:document[1]/w:body[1]".to_owned()
        )]
    );
}

#[test]
fn the_fallback_is_checked_at_a_target_after_the_choices_namespace() {
    let package = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body><w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:t>a</w:t></mc:Choice>\
         <mc:Fallback><w:t w:bogus=\"1\">a</w:t></mc:Fallback></mc:AlternateContent></w:r></w:p></w:body>",
        None,
    );
    assert_eq!(found(&package, OfficeVersion::Office2010).len(), 0);
    assert_eq!(
        found(&package, OfficeVersion::Office2013),
        [(
            ErrorCode::UndeclaredAttribute,
            "/word/document.xml /w:document[1]/w:body[1]/w:p[1]/w:r[1]/mc:AlternateContent[1]/mc:Fallback[1]/w:t[1]"
                .to_owned()
        )]
    );
}

#[test]
fn an_alternate_content_needs_a_choice_with_requires() {
    let package = document(
        W,
        OFFICE_DOCUMENT,
        "<w:body><w:p><w:r><mc:AlternateContent><mc:Choice><w:t>a</w:t></mc:Choice>\
         </mc:AlternateContent><mc:AlternateContent/></w:r></w:p></w:body>",
        None,
    );
    let codes: Vec<ErrorCode> = found(&package, OfficeVersion::Office2016)
        .into_iter()
        .map(|(code, _)| code)
        .collect();
    assert_eq!(
        codes,
        [
            ErrorCode::ChoiceWithoutRequires,
            ErrorCode::AlternateContentWithoutChoice
        ]
    );
}

/// The codes a block gives, with the places they are at.
fn block_findings(block: &str) -> Vec<(ErrorCode, String)> {
    let body = format!("<w:body><w:p><w:r>{block}</w:r></w:p></w:body>");
    let package = document(W, OFFICE_DOCUMENT, &body, None);
    found(&package, OfficeVersion::Office2016)
        .into_iter()
        .filter(|(code, _)| {
            matches!(
                code,
                ErrorCode::AlternateContentWithoutChoice
                    | ErrorCode::UnexpectedChildInAlternateContent
                    | ErrorCode::NestedAlternateContent
            )
        })
        .collect()
}

#[test]
fn an_empty_alternate_content_is_one_block_without_a_choice() {
    assert_eq!(
        block_findings("<mc:AlternateContent/>"),
        [(
            ErrorCode::AlternateContentWithoutChoice,
            BLOCK_PATH.to_owned()
        )]
    );
}

#[test]
fn an_alternate_content_starting_with_a_fallback_is_one_block_without_a_choice() {
    assert_eq!(
        block_findings(
            "<mc:AlternateContent><mc:Fallback><w:t/></mc:Fallback></mc:AlternateContent>"
        ),
        [(
            ErrorCode::AlternateContentWithoutChoice,
            BLOCK_PATH.to_owned()
        )]
    );
}

#[test]
fn an_alternate_content_starting_with_a_text_is_one_block_without_a_choice() {
    assert_eq!(
        block_findings("<mc:AlternateContent><w:t/></mc:AlternateContent>"),
        [(
            ErrorCode::AlternateContentWithoutChoice,
            BLOCK_PATH.to_owned()
        )]
    );
}

#[test]
fn a_child_after_the_fallback_is_an_unexpected_child() {
    assert_eq!(
        block_findings(
            "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:t/></mc:Choice>\
             <mc:Fallback><w:t/></mc:Fallback><mc:Fallback><w:t/></mc:Fallback>\
             <mc:Choice Requires=\"wps\"><w:t/></mc:Choice></mc:AlternateContent>"
        ),
        [
            (
                ErrorCode::UnexpectedChildInAlternateContent,
                BLOCK_PATH.to_owned()
            ),
            (
                ErrorCode::UnexpectedChildInAlternateContent,
                BLOCK_PATH.to_owned()
            )
        ]
    );
}

#[test]
fn a_child_that_is_neither_choice_nor_fallback_is_an_unexpected_child() {
    assert_eq!(
        block_findings(
            "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:t/></mc:Choice><w:t/></mc:AlternateContent>"
        ),
        [(
            ErrorCode::UnexpectedChildInAlternateContent,
            BLOCK_PATH.to_owned()
        )]
    );
}

#[test]
fn a_nested_alternate_content_is_reported_once_at_the_outer_block() {
    assert_eq!(
        block_findings(
            "<mc:AlternateContent><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:t/></mc:Choice>\
             </mc:AlternateContent><mc:Choice Requires=\"wps\"><w:t/></mc:Choice></mc:AlternateContent>"
        ),
        [(ErrorCode::NestedAlternateContent, BLOCK_PATH.to_owned())]
    );
}

#[test]
fn a_strict_file_with_a_transitional_only_justification_gets_transitional_only_value() {
    let body = "<w:body><w:p><w:pPr><w:jc w:val=\"left\"/></w:pPr></w:p></w:body>";
    let strict = document(STRICT_W, STRICT_OFFICE_DOCUMENT, body, None);
    let found_errors = errors(&strict, OfficeVersion::Microsoft365);
    assert_eq!(found_errors.len(), 1);
    assert_eq!(found_errors[0].code, ErrorCode::TransitionalOnlyValue);
    assert_eq!(
        found_errors[0].path_text(),
        "/w:document[1]/w:body[1]/w:p[1]/w:pPr[1]/w:jc[1]"
    );
    assert_eq!(
        found_errors[0].message,
        "the value \"left\" of w:val exists only in Transitional, and the file declares Strict"
    );
    let path = found_errors[0].path.as_ref().unwrap();
    assert!(path.iter().all(|step| step.namespace == W));
    let transitional = document(W, OFFICE_DOCUMENT, body, None);
    assert_eq!(errors(&transitional, OfficeVersion::Microsoft365).len(), 0);
}

#[test]
fn an_empty_xml_part_has_no_root_element() {
    let package = document(W, OFFICE_DOCUMENT, "<w:body/>", Some(""));
    let found_errors = errors(&package, OfficeVersion::Office2019);
    assert_eq!(found_errors.len(), 1);
    assert_eq!(found_errors[0].code, ErrorCode::MissingRootElement);
    assert_eq!(found_errors[0].path, None);
    assert_eq!(
        found_errors[0].part.as_ref().map(PartName::as_str),
        Some("/word/styles.xml")
    );
}

#[test]
fn a_part_whose_root_is_another_element_is_not_checked() {
    let styles = format!("<w:document xmlns:w=\"{W}\" w:bogus=\"1\"/>");
    let package = document(W, OFFICE_DOCUMENT, "<w:body/>", Some(&styles));
    assert_eq!(errors(&package, OfficeVersion::Office2019).len(), 0);
}

/// A Word document of the class `strict` declares whose body holds a run
/// property `w:b` of each value, a VML shape in `w:pict`, an opacity of each
/// form, and a text box of a `wps` shape, the `a`, `wp`, and `pic` prefixes
/// bound to the class's URIs.
fn transitional_content(strict: bool) -> Package {
    let (relationship, w, a, wp, pic) = if strict {
        (
            STRICT_OFFICE_DOCUMENT,
            STRICT_W,
            "http://purl.oclc.org/ooxml/drawingml/main",
            "http://purl.oclc.org/ooxml/drawingml/wordprocessingDrawing",
            "http://purl.oclc.org/ooxml/drawingml/picture",
        )
    } else {
        (
            OFFICE_DOCUMENT,
            W,
            "http://schemas.openxmlformats.org/drawingml/2006/main",
            "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
            "http://schemas.openxmlformats.org/drawingml/2006/picture",
        )
    };
    let opacity = |value: &str| {
        format!(
            "<w:p><w:r><w:drawing><wp:inline><a:graphic><a:graphicData uri=\"{pic}\"><pic:pic>\
             <pic:spPr><a:solidFill><a:srgbClr val=\"FF0000\"><a:alpha val=\"{value}\"/></a:srgbClr>\
             </a:solidFill></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
        )
    };
    let body = format!(
        "<w:body xmlns:a=\"{a}\" xmlns:wp=\"{wp}\" xmlns:pic=\"{pic}\" \
         xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\">\
         <w:p><w:r><w:rPr><w:b w:val=\"on\"/></w:rPr></w:r></w:p>\
         <w:p><w:r><w:rPr><w:b w:val=\"true\"/></w:rPr></w:r></w:p>\
         <w:p><w:r><w:pict><v:shape o:spid=\"_x0000_s1025\"><v:textbox/></v:shape></w:pict></w:r></w:p>\
         {}{}\
         <w:p><w:r><w:drawing><wp:anchor><a:graphic><a:graphicData uri=\"{WPS}\"><wps:wsp><wps:txbx>\
         <w:txbxContent><w:p/></w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor>\
         </w:drawing></w:r></w:p></w:body>",
        opacity("50000"),
        opacity("50%"),
    );
    document(w, relationship, &body, None)
}

/// The errors a file gets of the three codes of what only Transitional allows.
fn transitional_only(package: &Package) -> Vec<(ErrorCode, String)> {
    found(package, OfficeVersion::Microsoft365)
        .into_iter()
        .filter(|(code, _)| {
            matches!(
                code,
                ErrorCode::TransitionalOnlyAttribute
                    | ErrorCode::TransitionalOnlyElement
                    | ErrorCode::TransitionalOnlyValue
            )
        })
        .collect()
}

#[test]
fn a_strict_file_gets_an_error_for_each_thing_only_transitional_allows() {
    let at = |path: &str| format!("/word/document.xml /w:document[1]/w:body[1]/{path}");
    let drawing = "w:r[1]/w:drawing[1]";
    let shape = "w:p[3]/w:r[1]/w:pict[1]/v:shape[1]";
    assert_eq!(
        transitional_only(&transitional_content(true)),
        [
            (
                ErrorCode::TransitionalOnlyValue,
                at("w:p[1]/w:r[1]/w:rPr[1]/w:b[1]")
            ),
            (
                ErrorCode::TransitionalOnlyElement,
                at("w:p[3]/w:r[1]/w:pict[1]")
            ),
            (ErrorCode::TransitionalOnlyElement, at(shape)),
            (ErrorCode::TransitionalOnlyAttribute, at(shape)),
            (
                ErrorCode::TransitionalOnlyValue,
                at(&format!(
                    "w:p[4]/{drawing}/wp:inline[1]/a:graphic[1]/a:graphicData[1]/pic:pic[1]/\
                     pic:spPr[1]/a:solidFill[1]/a:srgbClr[1]/a:alpha[1]"
                ))
            ),
            (
                ErrorCode::TransitionalOnlyElement,
                at(&format!(
                    "w:p[6]/{drawing}/wp:anchor[1]/a:graphic[1]/a:graphicData[1]/wps:wsp[1]/\
                     wps:txbx[1]/w:txbxContent[1]"
                ))
            ),
        ],
        "v:textbox, in v:shape's namespace, is not reported again"
    );
    let opacity_of_strict_form = at(&format!(
        "w:p[5]/{drawing}/wp:inline[1]/a:graphic[1]/a:graphicData[1]/pic:pic[1]/pic:spPr[1]/\
         a:solidFill[1]/a:srgbClr[1]/a:alpha[1]"
    ));
    assert!(
        found(&transitional_content(true), OfficeVersion::Microsoft365)
            .contains(&(ErrorCode::InvalidAttributeValue, opacity_of_strict_form)),
        "the own checks' error of 50% stays"
    );
}

#[test]
fn a_transitional_file_gets_none_of_the_strict_errors() {
    assert_eq!(transitional_only(&transitional_content(false)), []);
}
