use alloc::collections::BTreeSet;

use officekit_xml::element::Element;
use officekit_xml::name::NamespaceId;
use officekit_xml::name::table::NAMESPACES;
use officekit_xml::parse::parse;

use super::{Counted, DRAWING, SPREADSHEET, WORDPROCESSING};

const A: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main""#;
const W: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const X: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

/// The declared properties that do not count, each with the section of
/// ECMA-376 Part 1 that shows it does not change how the text renders.
const NOT_COUNTED: &[NotCounted] = &[
    NotCounted {
        local: "noProof",
        place: Place::Child,
        prefix: "w",
        schema_type: "w:CT_RPr/w:rPr",
        section: "17.3.2.21: the run reports no spelling or grammar errors (proofing)",
    },
    NotCounted {
        local: "rPrChange",
        place: Place::Child,
        prefix: "w",
        schema_type: "w:CT_RPr/w:rPr",
        section: "17.13.5.31: the run properties before a tracked revision (revision tracking)",
    },
    NotCounted {
        local: "altLang",
        place: Place::Attribute,
        prefix: "",
        schema_type: "a:CT_TextCharacterProperties/a:rPr",
        section: "21.1.2.3.9: the language of the application's user interface controls \
                  (editing state)",
    },
    NotCounted {
        local: "dirty",
        place: Place::Attribute,
        prefix: "",
        schema_type: "a:CT_TextCharacterProperties/a:rPr",
        section: "21.1.2.3.9: the text changed since the proofing tools last ran (proofing)",
    },
    NotCounted {
        local: "err",
        place: Place::Attribute,
        prefix: "",
        schema_type: "a:CT_TextCharacterProperties/a:rPr",
        section: "21.1.2.3.9: the saved result of a spelling or grammar check (proofing, a \
                  cached value)",
    },
    NotCounted {
        local: "noProof",
        place: Place::Attribute,
        prefix: "",
        schema_type: "a:CT_TextCharacterProperties/a:rPr",
        section: "21.1.2.3.9: the run is not checked for mistakes (proofing)",
    },
    NotCounted {
        local: "smtClean",
        place: Place::Attribute,
        prefix: "",
        schema_type: "a:CT_TextCharacterProperties/a:rPr",
        section: "21.1.2.3.9: whether the run was checked for smart tags, as dirty is for \
                  proofing (editing state)",
    },
];

/// A declared property that does not count.
struct NotCounted {
    local: &'static str,
    place: Place,
    /// The namespace table's prefix, empty for no namespace.
    prefix: &'static str,
    schema_type: &'static str,
    /// Why it does not count.
    section: &'static str,
}

/// Where a property sits on its properties element.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Attribute,
    Child,
}

/// Every property `counted` lists, by place, prefix, and local name.
fn listed(counted: &Counted) -> Vec<(Place, &'static str, &'static str)> {
    counted
        .attributes
        .iter()
        .map(|&(prefix, local)| (Place::Attribute, prefix, local))
        .chain(
            counted
                .children
                .iter()
                .map(|&(prefix, local)| (Place::Child, prefix, local)),
        )
        .collect()
}

fn properties(xml: &str) -> Element {
    parse(xml.as_bytes()).unwrap().root
}

fn table_prefix(id: NamespaceId) -> &'static str {
    NAMESPACES[usize::from(id)].prefix
}

#[test]
fn every_declared_property_counts_or_is_listed_as_not_counting() {
    for counted in [&DRAWING, &SPREADSHEET, &WORDPROCESSING] {
        let row = counted.row().unwrap();
        let declared: BTreeSet<(Place, &str, &str)> =
            row.attributes
                .iter()
                .map(|constraint| {
                    (
                        Place::Attribute,
                        constraint.namespace.map_or("", table_prefix),
                        constraint.name,
                    )
                })
                .chain(row.children.iter().map(|&(namespace, local, _, _)| {
                    (Place::Child, table_prefix(namespace), local)
                }))
                .collect();
        let mut classified = listed(counted);
        classified.extend(
            NOT_COUNTED
                .iter()
                .filter(|property| property.schema_type == counted.schema_type)
                .map(|property| (property.place, property.prefix, property.local)),
        );
        let unique: BTreeSet<(Place, &str, &str)> = classified.iter().copied().collect();
        assert_eq!(
            unique.len(),
            classified.len(),
            "{} classifies a property twice",
            counted.schema_type
        );
        assert_eq!(unique, declared, "{}", counted.schema_type);
    }
}

#[test]
fn every_property_that_does_not_count_names_its_element_and_a_section() {
    let types = [
        DRAWING.schema_type,
        SPREADSHEET.schema_type,
        WORDPROCESSING.schema_type,
    ];
    for property in NOT_COUNTED {
        assert!(types.contains(&property.schema_type), "{}", property.local);
        assert!(
            property
                .section
                .starts_with(|character: char| character.is_ascii_digit()),
            "{}",
            property.local
        );
    }
}

#[test]
fn a_declared_property_that_does_not_count_is_ignored() {
    let proofed = properties(&format!("<w:rPr {W}><w:b/></w:rPr>"));
    let unproofed = properties(&format!(
        r#"<w:rPr {W}><w:b/><w:noProof/><w:rPrChange w:id="1" w:author="A"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>"#
    ));
    assert!(WORDPROCESSING.equal(Some(&proofed), Some(&unproofed)));
    let only_unproofed = properties(&format!("<w:rPr {W}><w:noProof/></w:rPr>"));
    assert!(WORDPROCESSING.equal(None, Some(&only_unproofed)));
    let checked = properties(&format!(r#"<a:rPr {A} lang="es-ES" sz="1800" dirty="0"/>"#));
    let flagged = properties(&format!(
        r#"<a:rPr {A} sz="1800" lang="es-ES" err="1" smtClean="0" noProof="1" altLang="en-US"/>"#
    ));
    assert!(DRAWING.equal(Some(&checked), Some(&flagged)));
}

#[test]
fn a_property_that_counts_must_be_written_the_same() {
    let bold = properties(&format!("<w:rPr {W}><w:b/></w:rPr>"));
    let not_bold = properties(&format!(r#"<w:rPr {W}><w:b w:val="0"/></w:rPr>"#));
    assert!(!WORDPROCESSING.equal(Some(&bold), Some(&not_bold)));
    assert!(!WORDPROCESSING.equal(Some(&bold), None));
    let spanish = properties(&format!(r#"<a:rPr {A} lang="es-ES"/>"#));
    let english = properties(&format!(r#"<a:rPr {A} lang="en-US"/>"#));
    assert!(!DRAWING.equal(Some(&spanish), Some(&english)));
    let red = properties(&format!(
        r#"<a:rPr {A}><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr>"#
    ));
    let green = properties(&format!(
        r#"<a:rPr {A}><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:rPr>"#
    ));
    assert!(!DRAWING.equal(Some(&red), Some(&green)));
    let small = properties(&format!(r#"<rPr {X}><b/><sz val="11"/></rPr>"#));
    let large = properties(&format!(r#"<rPr {X}><b/><sz val="12"/></rPr>"#));
    assert!(!SPREADSHEET.equal(Some(&small), Some(&large)));
}

#[test]
fn properties_compare_in_any_order_and_under_any_prefix() {
    let one = properties(&format!(
        r#"<w:rPr {W}><w:b/><w:sz w:val="24" w:foo="1"/></w:rPr>"#
    ));
    let other = properties(
        "<o:rPr xmlns:o=\"http://purl.oclc.org/ooxml/wordprocessingml/main\">\n  \
         <o:sz o:foo=\"1\" o:val=\"24\"/>\n  <o:b/>\n</o:rPr>",
    );
    assert!(WORDPROCESSING.equal(Some(&one), Some(&other)));
}

#[test]
fn a_property_the_tables_do_not_declare_counts() {
    let plain = properties(&format!("<w:rPr {W}><w:b/></w:rPr>"));
    let extended = properties(&format!("<w:rPr {W} {W15}><w:b/><w15:unknown/></w:rPr>"));
    assert!(!WORDPROCESSING.equal(Some(&plain), Some(&extended)));
    let attributed = properties(&format!(r#"<w:rPr {W} w:unknown="1"><w:b/></w:rPr>"#));
    assert!(!WORDPROCESSING.equal(Some(&plain), Some(&attributed)));
    let sized = properties(&format!(r#"<a:rPr {A} sz="1800"/>"#));
    let marked = properties(&format!(r#"<a:rPr {A} sz="1800" unknown="1"/>"#));
    assert!(!DRAWING.equal(Some(&sized), Some(&marked)));
}
