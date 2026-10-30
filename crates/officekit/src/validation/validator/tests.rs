use core::iter;

use officekit_xml::name::OfficeVersion;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;

use super::{Validator, document_parts, reachable_parts, rule};
use crate::validation::error::{ErrorCode, ValidationError};

const CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const CHART_CONTENT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const COMMENTS_EXTENDED: &str =
    "http://schemas.microsoft.com/office/2011/relationships/commentsExtended";
const COMMENTS_EXTENDED_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml";
const NA_AS_BLANK_CHART: &[u8] = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dataDisplayOptions16><c16r3:dispNaAsBlank val="1"/></c16r3:dataDisplayOptions16></c:ext></c:extLst></c:chart></c:chartSpace>"#;
const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const SMART_TAG: &str = r#"<w:smartTag w:uri="urn:schemas-microsoft-com:office:smarttags" w:element="PersonName"><w:smartTagPr><w:attr w:name="ProductID" w:val="LA VIVIENDA"/></w:smartTagPr><w:r><w:t>LA VIVIENDA</w:t></w:r></w:smartTag>"#;
const STRICT_OFFICE_DOCUMENT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";
const STYLES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles";
const STYLES_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
const NUMBERING: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering";
const NUMBERING_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml";
const SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
const SETTINGS_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
const WEB_SETTINGS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/webSettings";
const WEB_SETTINGS_CONTENT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.webSettings+xml";
const ON_OFF_ELEMENTS: [&str; 18] = [
    "cantSplit",
    "tblHeader",
    "hidden",
    "noWrap",
    "tcFitText",
    "hideMark",
    "noResizeAllowed",
    "linkedToFile",
    "noBorder",
    "flatBorders",
    "autoRedefine",
    "semiHidden",
    "unhideWhenUsed",
    "qFormat",
    "locked",
    "personal",
    "personalCompose",
    "personalReply",
];
const VERSIONS: [OfficeVersion; 6] = [
    OfficeVersion::Office2010,
    OfficeVersion::Office2013,
    OfficeVersion::Office2016,
    OfficeVersion::Office2019,
    OfficeVersion::Office2021,
    OfficeVersion::Microsoft365,
];
const WORD_NAMESPACES: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml""#;
const WNE: &str = "http://schemas.microsoft.com/office/word/2006/wordml";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const UNKNOWN: &str = "http://example.com/relationships/unknown";
const PARAGRAPH_STYLE: &str = r#"<w:style w:type="paragraph" w:styleId="S1"><w:name w:val="S1"/><w:pPr>PPR</w:pPr></w:style>"#;
const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const BAND_SIZES: &str = r#"<w:tblStyleRowBandSize w:val="1"/><w:tblStyleColBandSize w:val="1"/>"#;

/// A Word document related by `office_document` whose main part relates to
/// one part by each of `related`: its name, relationship type, and content
/// type.
fn word(office_document: &str, related: &[(&str, &str, &str)]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package.set_part_bytes(&main, MAIN, b"<a/>").unwrap();
    package.relate(None, &main, office_document).unwrap();
    for (name, relationship_type, content_type) in related {
        let part = PartName::new(name).unwrap();
        package
            .set_part_bytes(&part, content_type, b"<a/>")
            .unwrap();
        package
            .relate(Some(&main), &part, relationship_type)
            .unwrap();
    }
    package
}

fn chart_document(chart: &[u8]) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(
            &main,
            MAIN,
            br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#,
        )
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    let part = PartName::new("/word/charts/chart1.xml").unwrap();
    package.set_part_bytes(&part, CHART_CONTENT, chart).unwrap();
    package.relate(Some(&main), &part, CHART).unwrap();
    package
}

fn word_document(before_body: &str, body: &str) -> Package {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml").unwrap();
    let document =
        format!("<w:document {WORD_NAMESPACES}>{before_body}<w:body>{body}</w:body></w:document>");
    package
        .set_part_bytes(&main, MAIN, document.as_bytes())
        .unwrap();
    package.relate(None, &main, OFFICE_DOCUMENT).unwrap();
    package
}

fn with_part(
    mut package: Package,
    name: &str,
    relationship: &str,
    content_type: &str,
    xml: &str,
) -> Package {
    let main = PartName::new("/word/document.xml").unwrap();
    let part = PartName::new(name).unwrap();
    package
        .set_part_bytes(&part, content_type, xml.as_bytes())
        .unwrap();
    package.relate(Some(&main), &part, relationship).unwrap();
    package
}

fn numbering_document(level_justification: &str) -> Package {
    let numbering = format!(
        "<w:numbering {WORD_NAMESPACES}><w:abstractNum w:abstractNumId=\"0\"><w:lvl w:ilvl=\"0\">{level_justification}</w:lvl></w:abstractNum></w:numbering>"
    );
    with_part(
        word_document("", ""),
        "/word/numbering.xml",
        NUMBERING,
        NUMBERING_CONTENT,
        &numbering,
    )
}

fn settings_document(settings: &str) -> Package {
    let part = format!("<w:settings {WORD_NAMESPACES}>{settings}</w:settings>");
    with_part(
        word_document("", ""),
        "/word/settings.xml",
        SETTINGS,
        SETTINGS_CONTENT,
        &part,
    )
}

fn web_settings_document(web_settings: &str) -> Package {
    let part = format!("<w:webSettings {WORD_NAMESPACES}>{web_settings}</w:webSettings>");
    with_part(
        word_document("", ""),
        "/word/webSettings.xml",
        WEB_SETTINGS,
        WEB_SETTINGS_CONTENT,
        &part,
    )
}

fn on_off_document(value: &str) -> Package {
    let on = |name: &str| format!(r#"<w:{name} w:val="{value}"/>"#);
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:trPr>{}{}{}</w:trPr><w:tc><w:tcPr>{}{}{}</w:tcPr><w:p/></w:tc></w:tr></w:tbl>",
        on("cantSplit"),
        on("tblHeader"),
        on("hidden"),
        on("noWrap"),
        on("tcFitText"),
        on("hideMark"),
    );
    let style: String = [
        "autoRedefine",
        "hidden",
        "semiHidden",
        "unhideWhenUsed",
        "qFormat",
        "locked",
        "personal",
        "personalCompose",
        "personalReply",
    ]
    .map(on)
    .concat();
    let styles = format!(
        r#"<w:styles {WORD_NAMESPACES}><w:style w:type="paragraph" w:styleId="Body"><w:name w:val="Body"/>{style}</w:style></w:styles>"#
    );
    let web = format!(
        "<w:webSettings {WORD_NAMESPACES}><w:frameset><w:framesetSplitbar>{}{}</w:framesetSplitbar><w:frame>{}{}</w:frame></w:frameset></w:webSettings>",
        on("noBorder"),
        on("flatBorders"),
        on("noResizeAllowed"),
        on("linkedToFile"),
    );
    let package = with_part(
        word_document("", &table),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &styles,
    );
    with_part(
        package,
        "/word/webSettings.xml",
        WEB_SETTINGS,
        WEB_SETTINGS_CONTENT,
        &web,
    )
}

fn table_document(table_properties: &str) -> Package {
    word_document(
        "",
        &format!("<w:tbl><w:tblPr>{table_properties}</w:tblPr><w:tblGrid/></w:tbl>"),
    )
}

fn text_box(content: &str) -> Package {
    word_document(
        "",
        &format!(
            r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="100" cy="100"/><wp:docPr id="1" name="Box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="{WPS}"><wps:wsp xmlns:wps="{WPS}"><wps:cNvSpPr txBox="1"/><wps:spPr/><wps:txbx>{content}</wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        ),
    )
}

fn errors_at(package: &Package, version: OfficeVersion, element: &str) -> Vec<String> {
    Validator::new(version)
        .validate(package)
        .iter()
        .map(ValidationError::line)
        .filter(|line| line.contains(element))
        .collect()
}

fn accepted_at_every_version(package: &Package, element: &str) {
    for version in VERSIONS {
        assert_eq!(
            errors_at(package, version, element),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

fn refused_with(package: &Package, element: &str, error: &str) {
    let errors = errors_at(package, OfficeVersion::Microsoft365, element);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains(error), "{errors:?}");
}

fn errors_in_the_extension_list(package: &Package, version: OfficeVersion) -> Vec<String> {
    Validator::new(version)
        .validate(package)
        .iter()
        .filter(|error| error.path_text().contains("c:extLst"))
        .map(ValidationError::line)
        .collect()
}

fn reached(package: &Package, target: OfficeVersion) -> Vec<(String, &'static str)> {
    reachable_parts(package, target)
        .into_iter()
        .map(|(part, part_rule)| (part.as_str().to_owned(), part_rule.name))
        .collect()
}

fn errors(count: usize) -> Vec<ValidationError> {
    iter::repeat_with(|| ValidationError::new(ErrorCode::ChoiceWithoutRequires, None, None, &[]))
        .take(count)
        .collect()
}

#[test]
fn a_new_validator_stops_at_1000_errors() {
    let validator = Validator::new(OfficeVersion::Office2016);
    assert_eq!(Validator::DEFAULT_MAX_ERRORS, 1000);
    assert_eq!(validator.max_errors(), 1000);
    assert_eq!(validator.version(), OfficeVersion::Office2016);
}

#[test]
fn the_error_that_reaches_the_cap_is_kept() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(3);
    assert_eq!(validator.limit(errors(2)).len(), 2);
    assert_eq!(validator.limit(errors(3)).len(), 3);
    assert_eq!(validator.limit(errors(4)).len(), 3);
}

#[test]
fn zero_means_no_limit() {
    let validator = Validator::new(OfficeVersion::Microsoft365).with_max_errors(0);
    assert_eq!(validator.limit(errors(2500)).len(), 2500);
}

#[test]
fn a_package_with_no_document_has_no_errors() {
    let validator = Validator::new(OfficeVersion::Microsoft365);
    assert_eq!(validator.validate(&Package::new()).len(), 0);
}

#[test]
fn each_relationship_type_names_one_part_type_in_a_document() {
    for document in [
        "PresentationDocument",
        "SpreadsheetDocument",
        "WordprocessingDocument",
    ] {
        let parts = document_parts(rule(document).unwrap());
        assert!(!parts.is_empty(), "{document}");
        for part_rule in &parts {
            let same = parts
                .iter()
                .filter(|other| other.relationship_type == part_rule.relationship_type)
                .count();
            assert_eq!(same, 1, "{document}: {}", part_rule.name);
        }
    }
}

#[test]
fn the_parts_reached_are_typed_by_relationship_and_kept_from_their_version_on() {
    let package = word(
        OFFICE_DOCUMENT,
        &[
            ("/word/styles.xml", STYLES, STYLES_CONTENT),
            (
                "/word/commentsExtended.xml",
                COMMENTS_EXTENDED,
                COMMENTS_EXTENDED_CONTENT,
            ),
            ("/word/other.xml", UNKNOWN, "application/xml"),
        ],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
            (
                "/word/commentsExtended.xml".to_owned(),
                "WordprocessingCommentsExPart"
            ),
        ]
    );
    assert_eq!(
        reached(&package, OfficeVersion::Office2010),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_part_whose_content_type_contradicts_its_part_type_is_not_reached() {
    let package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, "application/xml")],
    );
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
    );
}

#[test]
fn a_strict_relationship_type_reaches_the_same_parts() {
    let package = word(STRICT_OFFICE_DOCUMENT, &[]);
    assert_eq!(
        reached(&package, OfficeVersion::Microsoft365),
        [("/word/document.xml".to_owned(), "MainDocumentPart")]
    );
}

#[test]
fn a_strict_extended_properties_type_reaches_the_extended_properties_part() {
    let mut package = word(OFFICE_DOCUMENT, &[]);
    let app = PartName::new("/docProps/app.xml").unwrap();
    package
        .set_part_bytes(
            &app,
            "application/vnd.openxmlformats-officedocument.extended-properties+xml",
            b"<a/>",
        )
        .unwrap();
    package
        .relate(None, &app, RelationshipType::EXTENDED_PROPERTIES.strict)
        .unwrap();
    assert!(
        reached(&package, OfficeVersion::Microsoft365)
            .contains(&("/docProps/app.xml".to_owned(), "ExtendedFilePropertiesPart"))
    );
}

#[test]
fn a_target_that_names_no_part_does_not_hide_the_other_parts_of_its_source() {
    let main = PartName::new("/word/document.xml").unwrap();
    let mut package = word(
        OFFICE_DOCUMENT,
        &[("/word/styles.xml", STYLES, STYLES_CONTENT)],
    );
    package
        .set_part_bytes(
            &PartName::new(&main.relationships_part()).unwrap(),
            "application/vnd.openxmlformats-package.relationships+xml",
            format!(
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{UNKNOWN}" Target="media/"/><Relationship Id="rId2" Type="{STYLES}" Target="styles.xml"/></Relationships>"#
            )
            .as_bytes(),
        )
        .unwrap();
    let reread = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
    assert_eq!(
        reached(&reread, OfficeVersion::Office2013),
        [
            ("/word/document.xml".to_owned(), "MainDocumentPart"),
            ("/word/styles.xml".to_owned(), "StyleDefinitionsPart"),
        ]
    );
}

#[test]
fn a_chart_extension_list_accepts_the_extension_word_writes() {
    let package = chart_document(NA_AS_BLANK_CHART);
    for version in [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ] {
        assert_eq!(
            errors_in_the_extension_list(&package, version),
            Vec::<String>::new(),
            "{version:?}"
        );
    }
}

#[test]
fn a_chart_extension_without_its_content_element_is_refused() {
    let misnested = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea><c:extLst><c:ext xmlns:c16r3="http://schemas.microsoft.com/office/drawing/2017/03/chart" uri="{56B9EC1D-385E-4148-901F-78D8002777C0}"><c16r3:dispNaAsBlank val="1"/></c:ext></c:extLst></c:chart></c:chartSpace>"#;
    let errors =
        errors_in_the_extension_list(&chart_document(misnested), OfficeVersion::Microsoft365);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].contains("InvalidChild"), "{errors:?}");
}

#[test]
fn a_numbering_level_accepts_start_and_end() {
    for value in ["start", "end"] {
        let package = numbering_document(&format!(r#"<w:lvlJc w:val="{value}"/>"#));
        accepted_at_every_version(&package, "w:lvlJc");
    }
}

#[test]
fn a_numbering_level_refuses_a_value_outside_st_jc() {
    let package = numbering_document(r#"<w:lvlJc w:val="middle"/>"#);
    refused_with(&package, "w:lvlJc", "InvalidAttributeValue");
}

#[test]
fn a_table_accepts_start_and_end_as_its_alignment() {
    for value in ["start", "end"] {
        let package = table_document(&format!(r#"<w:jc w:val="{value}"/>"#));
        accepted_at_every_version(&package, "w:jc");
    }
}

#[test]
fn a_table_refuses_an_alignment_outside_st_jc_table() {
    let package = table_document(r#"<w:jc w:val="both"/>"#);
    refused_with(&package, "w:jc", "InvalidAttributeValue");
}

#[test]
fn a_table_accepts_every_on_off_value_for_bidi_visual() {
    for value in ["0", "1", "true", "false", "on", "off"] {
        let package = table_document(&format!(r#"<w:bidiVisual w:val="{value}"/>"#));
        accepted_at_every_version(&package, "w:bidiVisual");
    }
}

#[test]
fn a_table_refuses_a_value_outside_st_on_off_for_bidi_visual() {
    let package = table_document(r#"<w:bidiVisual w:val="maybe"/>"#);
    refused_with(&package, "w:bidiVisual", "InvalidAttributeValue");
}

#[test]
fn the_on_off_elements_accept_every_on_off_value() {
    for value in ["0", "1", "true", "false", "on", "off"] {
        let package = on_off_document(value);
        for element in ON_OFF_ELEMENTS {
            accepted_at_every_version(&package, &format!("w:{element}"));
        }
    }
}

#[test]
fn an_on_off_element_refuses_a_value_outside_st_on_off() {
    let package = word_document(
        "",
        r#"<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:trPr><w:cantSplit w:val="maybe"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#,
    );
    refused_with(&package, "w:cantSplit", "InvalidAttributeValue");
}

#[test]
fn a_document_background_accepts_filled() {
    let package = word_document(
        r#"<w:background><v:background filled="t"/></w:background>"#,
        "",
    );
    accepted_at_every_version(&package, "v:background");
}

#[test]
fn a_document_background_refuses_fill() {
    let package = word_document(
        r#"<w:background><v:background fill="t"/></w:background>"#,
        "",
    );
    refused_with(&package, "v:background", "UndeclaredAttribute");
}

#[test]
fn a_paragraph_and_the_settings_accept_smart_tags() {
    let paragraph = word_document("", &format!("<w:p>{SMART_TAG}</w:p>"));
    accepted_at_every_version(&paragraph, "w:smartTag");
    let settings = settings_document(
        r#"<w:smartTagType w:namespaceuri="urn:schemas-microsoft-com:office:smarttags" w:name="PersonName" w:url="http://example.com/"/>"#,
    );
    accepted_at_every_version(&settings, "w:smartTagType");
}

#[test]
fn the_settings_accept_do_not_embed_smart_tags_after_the_shape_defaults() {
    let package = settings_document(
        r#"<w:shapeDefaults/><w:doNotEmbedSmartTags/><w:decimalSymbol w:val="."/>"#,
    );
    accepted_at_every_version(&package, "w:doNotEmbedSmartTags");
}

#[test]
fn the_web_settings_accept_save_smart_tags_as_xml_after_the_target_screen_size() {
    let package =
        web_settings_document(r#"<w:targetScreenSz w:val="800x600"/><w:saveSmartTagsAsXml/>"#);
    accepted_at_every_version(&package, "w:saveSmartTagsAsXml");
}

#[test]
fn the_settings_refuse_do_not_embed_smart_tags_before_the_shape_defaults() {
    let package = settings_document("<w:doNotEmbedSmartTags/><w:shapeDefaults/>");
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:settings[1]");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn the_settings_refuse_a_second_do_not_embed_smart_tags() {
    let package = settings_document("<w:doNotEmbedSmartTags/><w:doNotEmbedSmartTags/>");
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:settings[1]");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn the_body_refuses_a_smart_tag() {
    let package = word_document("", SMART_TAG);
    refused_with(&package, "w:smartTag", "InvalidChild");
}

fn styles_document(styles: &str) -> Package {
    let part = format!("<w:styles {WORD_NAMESPACES}>{styles}</w:styles>");
    with_part(
        word_document("", ""),
        "/word/styles.xml",
        STYLES,
        STYLES_CONTENT,
        &part,
    )
}

fn paragraph_style(properties: &str) -> Package {
    styles_document(&PARAGRAPH_STYLE.replace("PPR", properties))
}

fn table_with_band_sizes(sizes: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr><w:bidiVisual/>{sizes}<w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid/></w:tbl>"#
    )
}

fn content_control(shape: &str) -> Package {
    let properties = r#"<w:sdtPr><w:tag w:val="a"/><w:label w:val="1"/><w:tabIndex w:val="1"/><w:text/></w:sdtPr>"#;
    let sdt = |content: &str| {
        format!("<w:sdt>{properties}<w:sdtContent>{content}</w:sdtContent></w:sdt>")
    };
    let body = match shape {
        "block" => sdt("<w:p/>"),
        "run" => format!("<w:p>{}</w:p>", sdt("<w:r><w:t>a</w:t></w:r>")),
        "cell" => format!(
            "<w:tbl><w:tblPr/><w:tblGrid/><w:tr>{}</w:tr></w:tbl>",
            sdt("<w:tc><w:p/></w:tc>")
        ),
        _ => format!(
            "<w:tbl><w:tblPr/><w:tblGrid/>{}</w:tbl>",
            sdt("<w:tr><w:tc><w:p/></w:tc></w:tr>")
        ),
    };
    word_document("", &body)
}

#[test]
fn a_numbering_level_accepts_div_id_and_cnf_style_after_the_outline_level() {
    let div_id =
        numbering_document(r#"<w:pPr><w:outlineLvl w:val="1"/><w:divId w:val="1"/></w:pPr>"#);
    accepted_at_every_version(&div_id, "holds w:divId");
    let cnf_style = numbering_document(
        r#"<w:pPr><w:divId w:val="1"/><w:cnfStyle w:val="000000000000"/></w:pPr>"#,
    );
    accepted_at_every_version(&cnf_style, "holds w:cnfStyle");
}

#[test]
fn a_style_accepts_div_id_and_cnf_style_after_the_outline_level() {
    let package = paragraph_style(
        r#"<w:outlineLvl w:val="1"/><w:divId w:val="1"/><w:cnfStyle w:val="000000000000"/>"#,
    );
    accepted_at_every_version(&package, "holds w:divId");
    accepted_at_every_version(&package, "holds w:cnfStyle");
}

#[test]
fn the_document_defaults_and_a_table_style_condition_accept_div_id_and_cnf_style() {
    let properties = r#"<w:divId w:val="1"/><w:cnfStyle w:val="000000000000"/>"#;
    let defaults = styles_document(&format!(
        "<w:docDefaults><w:pPrDefault><w:pPr>{properties}</w:pPr></w:pPrDefault></w:docDefaults>"
    ));
    let condition = styles_document(&format!(
        r#"<w:style w:type="table" w:styleId="T1"><w:name w:val="T1"/><w:tblStylePr w:type="firstRow"><w:pPr>{properties}</w:pPr></w:tblStylePr></w:style>"#
    ));
    for package in [defaults, condition] {
        accepted_at_every_version(&package, "holds w:divId");
        accepted_at_every_version(&package, "holds w:cnfStyle");
    }
}

#[test]
fn a_table_and_its_property_change_accept_the_band_sizes_after_bidi_visual() {
    let table = word_document("", &table_with_band_sizes(BAND_SIZES));
    let change = word_document(
        "",
        &table_with_band_sizes("").replace(
            "<w:bidiVisual/>",
            &format!(
                r#"<w:bidiVisual/><w:tblPrChange w:id="1" w:author="a"><w:tblPr><w:bidiVisual/>{BAND_SIZES}<w:tblW w:w="0" w:type="auto"/></w:tblPr></w:tblPrChange>"#
            ),
        ),
    );
    for package in [table, change] {
        accepted_at_every_version(&package, "holds w:tblStyleRowBandSize");
        accepted_at_every_version(&package, "holds w:tblStyleColBandSize");
    }
}

#[test]
fn a_table_style_condition_accepts_the_band_sizes_before_the_justification() {
    let package = styles_document(&format!(
        r#"<w:style w:type="table" w:styleId="T1"><w:name w:val="T1"/><w:tblStylePr w:type="firstRow"><w:tblPr>{BAND_SIZES}<w:jc w:val="center"/></w:tblPr></w:tblStylePr></w:style>"#
    ));
    accepted_at_every_version(&package, "holds w:tblStyleRowBandSize");
    accepted_at_every_version(&package, "holds w:tblStyleColBandSize");
}

#[test]
fn a_background_accepts_a_drawing() {
    let package = word_document(
        r#"<w:background w:color="FFFFFF"><w:drawing/></w:background>"#,
        "",
    );
    accepted_at_every_version(&package, "holds w:drawing");
}

#[test]
fn an_object_accepts_a_movie() {
    let package = word_document(
        "",
        &format!(
            r#"<w:p><w:r><w:object><w:movie xmlns:r="{RELATIONSHIPS}" r:id="rId9"/></w:object></w:r></w:p>"#
        ),
    );
    accepted_at_every_version(&package, "holds w:movie");
}

#[test]
fn a_form_field_accepts_a_label_and_a_tab_index() {
    let package = word_document(
        "",
        r#"<w:p><w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="a"/><w:label w:val="1"/><w:tabIndex w:val="1"/><w:enabled/></w:ffData></w:fldChar></w:r></w:p>"#,
    );
    accepted_at_every_version(&package, "holds w:label");
    accepted_at_every_version(&package, "holds w:tabIndex");
}

#[test]
fn a_content_control_accepts_a_label_and_a_tab_index() {
    for shape in ["block", "run", "cell", "row"] {
        let package = content_control(shape);
        accepted_at_every_version(&package, "holds w:label");
        accepted_at_every_version(&package, "holds w:tabIndex");
    }
}

#[test]
fn a_frame_accepts_a_title_and_a_long_description_and_a_frameset_a_title() {
    let frame = web_settings_document(&format!(
        r#"<w:frameset><w:frame xmlns:r="{RELATIONSHIPS}"><w:sz w:val="x"/><w:name w:val="n"/><w:title w:val="t"/><w:longDesc r:id="rId9"/><w:marW w:val="1"/></w:frame></w:frameset>"#
    ));
    accepted_at_every_version(&frame, "holds w:title");
    accepted_at_every_version(&frame, "holds w:longDesc");
    let frameset = web_settings_document(
        r#"<w:frameset><w:sz w:val="x"/><w:title w:val="t"/><w:frame/></w:frameset>"#,
    );
    accepted_at_every_version(&frameset, "holds w:title");
}

#[test]
fn a_style_refuses_div_id_before_the_outline_level() {
    let package = paragraph_style(r#"<w:divId w:val="1"/><w:outlineLvl w:val="1"/>"#);
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:style[1]/w:pPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_table_refuses_a_band_size_after_the_table_width() {
    let package =
        table_document(r#"<w:tblW w:w="0" w:type="auto"/><w:tblStyleRowBandSize w:val="1"/>"#);
    let errors = errors_at(
        &package,
        OfficeVersion::Microsoft365,
        "/w:tbl[1]/w:tblPr[1]:",
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_paragraph_refuses_a_label() {
    let package = word_document("", r#"<w:p><w:pPr><w:label w:val="1"/></w:pPr></w:p>"#);
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:p[1]/w:pPr[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_frame_refuses_a_long_description_before_the_title() {
    let package = web_settings_document(&format!(
        r#"<w:frameset><w:frame xmlns:r="{RELATIONSHIPS}"><w:longDesc r:id="rId9"/><w:title w:val="t"/></w:frame></w:frameset>"#
    ));
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:frame[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_frameset_refuses_a_title_after_a_frame() {
    let package =
        web_settings_document(r#"<w:frameset><w:frame/><w:title w:val="t"/></w:frameset>"#);
    let errors = errors_at(&package, OfficeVersion::Microsoft365, "/w:frameset[1]:");
    assert_eq!(errors.len(), 1, "{errors:?}");
}

#[test]
fn a_compatibility_setting_accepts_a_name_newer_than_the_target() {
    let package = settings_document(
        r#"<w:compat><w:compatSetting w:name="useWord2013TrackBottomHyphenation" w:uri="http://schemas.microsoft.com/office/word" w:val="1"/></w:compat>"#,
    );
    accepted_at_every_version(&package, "w:compatSetting");
}

#[test]
fn a_text_box_accepts_wne_txbx_content_and_w_txbx_content() {
    accepted_at_every_version(
        &text_box(&format!(
            r#"<wne:txbxContent xmlns:wne="{WNE}"><w:p/></wne:txbxContent>"#
        )),
        "txbxContent",
    );
    accepted_at_every_version(
        &text_box("<w:txbxContent><w:p/></w:txbxContent>"),
        "txbxContent",
    );
}

#[test]
fn wne_txbx_content_outside_a_text_box_is_refused() {
    let package = word_document(
        "",
        &format!(r#"<wne:txbxContent xmlns:wne="{WNE}"><w:p/></wne:txbxContent>"#),
    );
    assert_eq!(
        errors_at(&package, OfficeVersion::Microsoft365, "wne:txbxContent").len(),
        1
    );
}
