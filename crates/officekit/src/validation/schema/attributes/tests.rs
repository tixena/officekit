use officekit_xml::element::Element;
use officekit_xml::name::{Namespace, OfficeVersion};
use officekit_xml::parse::parse;

use super::{Failure, check, run};
use crate::validation::error::ErrorCode;
use crate::validation::generated::{SCHEMA_TYPES, type_id};
use crate::validation::schema::types::{
    AttributeConstraint, Bounds, Check, SchemaType, StringKind, ValueCheck, ValueClass, Versions,
};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const TARGETS: [OfficeVersion; 6] = [
    OfficeVersion::Office2010,
    OfficeVersion::Office2013,
    OfficeVersion::Office2016,
    OfficeVersion::Office2019,
    OfficeVersion::Office2021,
    OfficeVersion::Microsoft365,
];

const PATTERN: Check = Check::String {
    class: None,
    kind: StringKind::Plain,
    length: None,
    max_length: None,
    min_length: None,
    pattern: Some("[0-9]+%"),
};

const WHOLE: Check = Check::Number {
    bounds: Bounds {
        max_exclusive: None,
        max_inclusive: None,
        min_exclusive: None,
        min_inclusive: None,
        non_negative: false,
        positive: false,
    },
    class: ValueClass::Int32,
    list: false,
};

const FROM_2013: [ValueCheck; 2] = [
    ValueCheck {
        check: Check::OfficeVersion,
        union: None,
        versions: Versions(0b11_1110),
    },
    ValueCheck {
        check: WHOLE,
        union: None,
        versions: Versions(0b11_1111),
    },
];

const PERCENT: [ValueCheck; 1] = [ValueCheck {
    check: PATTERN,
    union: None,
    versions: Versions(0b11_1111),
}];

const NEWER: [AttributeConstraint; 1] = [AttributeConstraint {
    checks: &FROM_2013,
    class: ValueClass::String,
    name: "val",
    namespace: None,
    strict_checks: Some(&PERCENT),
    transitional_only: false,
}];

const NEWER_TYPE: SchemaType = SchemaType {
    attributes: &NEWER,
    children: &[],
    name: "w:CT_Newer/w:newer",
    particles: &[],
    text_checks: &[],
    versions: Versions(0b11_1111),
};

/// The `w:w` attribute of a type.
fn width(type_name: &str) -> &'static AttributeConstraint {
    schema_type(type_name)
        .attributes
        .iter()
        .find(|constraint| constraint.name == "w")
        .unwrap()
}

fn schema_type(name: &str) -> &'static SchemaType {
    &SCHEMA_TYPES[usize::from(type_id(name).unwrap())]
}

fn element(xml: &str) -> Element {
    parse(xml.as_bytes()).unwrap().root
}

fn at_every_target(constraint: &AttributeConstraint, value: &str) -> Vec<Vec<Failure>> {
    TARGETS
        .iter()
        .map(|target| {
            run(
                constraint.checks,
                &constraint.class,
                Some(value),
                *target,
                false,
            )
        })
        .collect()
}

fn codes(element: &Element, type_name: &str, strict: bool) -> Vec<ErrorCode> {
    check(
        element,
        schema_type(type_name),
        OfficeVersion::Microsoft365,
        strict,
        |namespace| {
            *namespace
                == Namespace::from_uri("http://schemas.microsoft.com/office/word/2010/wordml")
        },
    )
    .into_iter()
    .map(|problem| problem.code)
    .collect()
}

#[test]
fn the_four_margin_values_on_the_left_margin() {
    let left = width("w:CT_TblWidthDxaNil/w:left");
    for value in ["100.0", "10%", "-5"] {
        for found in at_every_target(left, value) {
            assert_eq!(found.len(), 1, "{value}");
            assert!(matches!(found[0], Failure::Invalid(_)), "{value}");
        }
    }
    for found in at_every_target(left, "5000") {
        assert_eq!(found.len(), 0);
    }
}

#[test]
fn the_four_margin_values_on_the_top_margin() {
    let top = width("w:CT_TblWidth/w:top");
    for found in at_every_target(top, "100.0") {
        assert_eq!(found, [Failure::NotInUnion]);
    }
    for value in ["10%", "5000", "-5"] {
        for found in at_every_target(top, value) {
            assert_eq!(found.len(), 0, "{value}");
        }
    }
}

#[test]
fn a_missing_required_attribute_fails_only_its_required_check() {
    let left = width("w:CT_TblWidthDxaNil/w:left");
    assert_eq!(
        run(
            left.checks,
            &left.class,
            None,
            OfficeVersion::Office2013,
            false
        ),
        [Failure::Missing]
    );
    let top = width("w:CT_TblWidth/w:top");
    assert_eq!(
        run(
            top.checks,
            &top.class,
            None,
            OfficeVersion::Office2013,
            false
        )
        .len(),
        0
    );
    let missing = element(&format!("<w:left xmlns:w=\"{W}\" w:type=\"dxa\"/>"));
    assert_eq!(
        codes(&missing, "w:CT_TblWidthDxaNil/w:left", false),
        [ErrorCode::MissingRequiredAttribute]
    );
}

#[test]
fn a_union_passes_when_a_running_member_passes_or_none_runs() {
    let at_2010 = Versions(0b00_0001);
    let checks = [
        ValueCheck {
            check: PATTERN,
            union: Some(0),
            versions: at_2010,
        },
        ValueCheck {
            check: WHOLE,
            union: Some(0),
            versions: at_2010,
        },
    ];
    let class = ValueClass::String;
    let run_at =
        |value: &str, target: OfficeVersion| run(&checks, &class, Some(value), target, false);
    assert_eq!(run_at("50%", OfficeVersion::Office2010).len(), 0);
    assert_eq!(run_at("50", OfficeVersion::Office2010).len(), 0);
    assert_eq!(
        run_at("x", OfficeVersion::Office2010),
        [Failure::NotInUnion]
    );
    assert_eq!(run_at("x", OfficeVersion::Office2013).len(), 0);
}

#[test]
fn the_first_failing_check_stops_the_rest() {
    let checks = [
        ValueCheck {
            check: WHOLE,
            union: None,
            versions: Versions(0b11_1111),
        },
        ValueCheck {
            check: PATTERN,
            union: None,
            versions: Versions(0b11_1111),
        },
    ];
    let found = run(
        &checks,
        &ValueClass::String,
        Some("x"),
        OfficeVersion::Office2016,
        false,
    );
    assert_eq!(found.len(), 1);
}

#[test]
fn an_undeclared_attribute_is_an_error_unless_ignorable_or_xml() {
    let declared = element(&format!(
        "<w:left xmlns:w=\"{W}\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
         xmlns:v=\"urn:schemas-microsoft-com:vml\" w:w=\"0\" w:type=\"dxa\" w14:foo=\"1\" xml:space=\"preserve\"/>"
    ));
    assert_eq!(
        codes(&declared, "w:CT_TblWidthDxaNil/w:left", false).len(),
        0
    );
    let undeclared = element(&format!(
        "<w:left xmlns:w=\"{W}\" w:w=\"0\" w:type=\"dxa\" w:bogus=\"1\" ID=\"2\"/>"
    ));
    assert_eq!(
        codes(&undeclared, "w:CT_TblWidthDxaNil/w:left", false),
        [
            ErrorCode::UndeclaredAttribute,
            ErrorCode::UndeclaredAttribute
        ]
    );
}

#[test]
fn a_strict_file_reports_a_value_only_transitional_allows() {
    let left = element(&format!("<w:jc xmlns:w=\"{W}\" w:val=\"left\"/>"));
    assert_eq!(
        codes(&left, "w:CT_Jc/w:jc", true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(codes(&left, "w:CT_Jc/w:jc", false).len(), 0);
    let start = element(&format!("<w:jc xmlns:w=\"{W}\" w:val=\"start\"/>"));
    assert_eq!(codes(&start, "w:CT_Jc/w:jc", true).len(), 0);
}

#[test]
fn a_strict_file_reports_an_attribute_undeclared_at_the_target() {
    let at = |value: &str, target: OfficeVersion, strict: bool| -> Vec<ErrorCode> {
        check(
            &element(&format!(r#"<newer val="{value}"/>"#)),
            &NEWER_TYPE,
            target,
            strict,
            |_| false,
        )
        .into_iter()
        .map(|problem| problem.code)
        .collect()
    };
    assert_eq!(
        at("50%", OfficeVersion::Office2010, true),
        [ErrorCode::UndeclaredAttribute]
    );
    assert_eq!(
        at("50", OfficeVersion::Office2010, true),
        [ErrorCode::UndeclaredAttribute]
    );
    assert_eq!(
        at("50", OfficeVersion::Office2010, false),
        [ErrorCode::UndeclaredAttribute]
    );
    assert_eq!(at("50%", OfficeVersion::Office2013, true).len(), 0);
    assert_eq!(
        at("50", OfficeVersion::Office2013, true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(
        at("x", OfficeVersion::Office2013, true),
        [ErrorCode::InvalidAttributeValue]
    );
}

#[test]
fn a_strict_file_checks_a_value_against_its_strict_type() {
    let alpha = |value: &str| {
        element(&format!(
            "<a:alpha xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" val=\"{value}\"/>"
        ))
    };
    let opacity = "a:CT_PositiveFixedPercentage/a:alpha";
    assert_eq!(
        codes(&alpha("50000"), opacity, true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(codes(&alpha("50000"), opacity, false).len(), 0);
    assert_eq!(
        codes(&alpha("50%"), opacity, true).len(),
        0,
        "Strict's form, in a Strict file"
    );
    assert_eq!(
        codes(&alpha("50%"), opacity, false),
        [ErrorCode::InvalidAttributeValue],
        "Strict's form, in a Transitional file"
    );
    assert_eq!(
        codes(&alpha("x"), opacity, true),
        [ErrorCode::InvalidAttributeValue],
        "a value both refuse"
    );
    let luminance = |value: &str| {
        element(&format!(
            "<a:lumMod xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" val=\"{value}\"/>"
        ))
    };
    let modulation = "a:CT_Percentage/a:lumMod";
    assert_eq!(codes(&luminance("110%"), modulation, true).len(), 0);
    assert_eq!(
        codes(&luminance("110%"), modulation, false),
        [ErrorCode::InvalidAttributeValue]
    );
    assert_eq!(
        codes(&luminance("110000"), modulation, true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(codes(&luminance("110000"), modulation, false).len(), 0);
    assert_eq!(
        codes(&luminance("x"), modulation, true),
        [ErrorCode::InvalidAttributeValue]
    );
    let indent = |value: &str| {
        element(&format!(
            "<w:tblInd xmlns:w=\"{W}\" w:w=\"{value}\" w:type=\"dxa\"/>"
        ))
    };
    let table_indent = "w:CT_TblWidthShort/w:tblInd";
    assert_eq!(codes(&indent("0pt"), table_indent, true).len(), 0);
    assert_eq!(
        codes(&indent("0pt"), table_indent, false),
        [ErrorCode::InvalidAttributeValue]
    );
    assert_eq!(
        codes(&indent("0"), table_indent, true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(codes(&indent("0"), table_indent, false).len(), 0);
    assert_eq!(
        codes(&indent("x"), table_indent, true),
        [ErrorCode::ValueNotInUnion]
    );
    let bold = |value: &str| element(&format!("<w:b xmlns:w=\"{W}\" w:val=\"{value}\"/>"));
    assert_eq!(
        codes(&bold("on"), "w:CT_OnOff/w:b", true),
        [ErrorCode::TransitionalOnlyValue]
    );
    assert_eq!(codes(&bold("true"), "w:CT_OnOff/w:b", true).len(), 0);
    assert_eq!(codes(&bold("on"), "w:CT_OnOff/w:b", false).len(), 0);
}

#[test]
fn a_strict_file_reports_a_marked_value_once() {
    let left = element(&format!("<w:jc xmlns:w=\"{W}\" w:val=\"left\"/>"));
    let constraint = schema_type("w:CT_Jc/w:jc")
        .attributes
        .iter()
        .find(|constraint| constraint.name == "val")
        .unwrap();
    assert!(constraint.strict_checks.is_some());
    assert_eq!(
        codes(&left, "w:CT_Jc/w:jc", true),
        [ErrorCode::TransitionalOnlyValue],
        "a marked member that the Strict checks refuse too"
    );
}

#[test]
fn a_strict_file_reports_each_attribute_of_a_namespace_only_transitional_has() {
    let shape = element(
        "<v:shape xmlns:v=\"urn:schemas-microsoft-com:vml\" \
         xmlns:o=\"urn:schemas-microsoft-com:office:office\" id=\"a\" o:spid=\"b\" o:bogus=\"c\"/>",
    );
    assert_eq!(
        codes(&shape, "v:CT_Shape/v:shape", true),
        [
            ErrorCode::TransitionalOnlyAttribute,
            ErrorCode::UndeclaredAttribute,
            ErrorCode::TransitionalOnlyAttribute,
        ]
    );
    assert_eq!(
        codes(&shape, "v:CT_Shape/v:shape", false),
        [ErrorCode::UndeclaredAttribute]
    );
}
