use alloc::collections::BTreeMap;
use std::path::PathBuf;

use os_shim::mock::MemorySystem;

use super::{Row, header, render, rows};
use crate::generate_schema::model::SchemaSet;
use crate::generate_schema::sdk_data::{DataNamespace, DataVersion, SdkData};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const S: &str = "http://schemas.openxmlformats.org/officeDocument/2006/sharedTypes";
const S_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/sharedTypes";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

fn xsd(namespace: &str, declarations: &str) -> String {
    format!(
        r#"<xsd:schema xmlns:xsd="http://www.w3.org/2001/XMLSchema" {declarations}
  targetNamespace="{namespace}"/>"#
    )
}

fn set(system: &MemorySystem, paths: &[&str]) -> SchemaSet {
    SchemaSet::read(system, paths).unwrap()
}

fn data(namespaces: &[(&str, &str, Option<DataVersion>)]) -> SdkData {
    SdkData {
        enumerations: BTreeMap::default(),
        namespaces: namespaces
            .iter()
            .map(|(prefix, uri, version)| DataNamespace {
                prefix: (*prefix).to_owned(),
                uri: (*uri).to_owned(),
                version: *version,
            })
            .collect(),
        parts: Vec::new(),
        schematrons: Vec::new(),
        typed_namespaces: BTreeMap::default(),
        types: BTreeMap::default(),
    }
}

fn system() -> MemorySystem {
    MemorySystem::new()
        .with_file("t/wml.xsd", xsd(W, "").as_bytes())
        .unwrap()
        .with_file(
            "t/shared-commonSimpleTypes.xsd",
            xsd(S, &format!(r#"xmlns:s="{S}""#)).as_bytes(),
        )
        .unwrap()
        .with_file("s/wml.xsd", xsd(W_STRICT, "").as_bytes())
        .unwrap()
        .with_file(
            "s/shared-commonSimpleTypes.xsd",
            xsd(S_STRICT, "").as_bytes(),
        )
        .unwrap()
        .with_file("o/w14.xsd", xsd(W14, "").as_bytes())
        .unwrap()
        .with_file("o/other.xsd", xsd("urn:other", "").as_bytes())
        .unwrap()
}

fn row(prefix: &str, uri: &str, strict: Option<&str>, version: Option<DataVersion>) -> Row {
    Row {
        introduced: version,
        prefix: prefix.to_owned(),
        strict_uri: strict.map(ToOwned::to_owned),
        uri: uri.to_owned(),
    }
}

fn table() -> Vec<Row> {
    let system = system();
    let data = data(&[
        ("", "", None),
        ("w14", W14, Some(DataVersion::Office2010)),
        ("w", W, Some(DataVersion::Office2007)),
    ]);
    rows(
        &data,
        &set(&system, &["t/wml.xsd", "t/shared-commonSimpleTypes.xsd"]),
        &set(&system, &["s/wml.xsd", "s/shared-commonSimpleTypes.xsd"]),
        &set(&system, &["o/w14.xsd"]),
    )
    .unwrap()
}

#[test]
fn rows_are_the_data_plus_the_transitional_namespaces_it_lacks_sorted_by_prefix() {
    assert_eq!(
        table(),
        vec![
            row("s", S, Some(S_STRICT), None),
            row("w", W, Some(W_STRICT), None),
            row("w14", W14, None, Some(DataVersion::Office2010)),
        ]
    );
}

#[test]
fn an_open_specifications_namespace_outside_the_data_is_an_error() {
    let system = system();
    let error = rows(
        &data(&[("w", W, None)]),
        &set(&system, &["t/wml.xsd"]),
        &set(&system, &["s/wml.xsd"]),
        &set(&system, &["o/other.xsd"]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("urn:other"), "{error}");
}

#[test]
fn a_strict_file_without_a_transitional_twin_is_an_error() {
    let system = system();
    let error = rows(
        &data(&[("w", W, None)]),
        &set(&system, &["t/wml.xsd"]),
        &set(&system, &["s/shared-commonSimpleTypes.xsd"]),
        &set(&system, &[]),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("shared-commonSimpleTypes.xsd"),
        "{error}"
    );
}

#[test]
fn a_transitional_namespace_with_no_declared_prefix_is_an_error() {
    let system = MemorySystem::new()
        .with_file("t/shared-commonSimpleTypes.xsd", xsd(S, "").as_bytes())
        .unwrap();
    let error = rows(
        &data(&[]),
        &set(&system, &["t/shared-commonSimpleTypes.xsd"]),
        &set(&system, &[]),
        &set(&system, &[]),
    )
    .unwrap_err();
    assert!(error.to_string().contains(S), "{error}");
}

#[test]
fn a_repeated_prefix_is_an_error() {
    let system = system();
    let error = rows(
        &data(&[("w", W, None), ("w", W14, None)]),
        &set(&system, &[]),
        &set(&system, &[]),
        &set(&system, &[]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("prefix w"), "{error}");
}

#[test]
fn a_table_past_the_range_of_a_namespace_id_is_an_error() {
    let system = system();
    let namespaces: Vec<(String, String)> = (0_u16..257_u16)
        .map(|index| (format!("p{index}"), format!("urn:{index}")))
        .collect();
    let listed: Vec<(&str, &str, Option<DataVersion>)> = namespaces
        .iter()
        .map(|(prefix, uri)| (prefix.as_str(), uri.as_str(), None))
        .collect();
    let error = rows(
        &data(&listed),
        &set(&system, &[]),
        &set(&system, &[]),
        &set(&system, &[]),
    )
    .unwrap_err();
    assert!(error.to_string().contains("u8"), "{error}");
}

#[test]
fn the_text_is_the_header_then_the_rows() {
    let text = render("// header\n", &table());
    assert!(text.starts_with("// header\n\nuse super::OfficeVersion;\n"));
    assert!(text.contains(
        "    NamespaceRow {\n        introduced: Some(OfficeVersion::Office2010),\n        prefix: \
         \"w14\",\n        strict_uri: None,\n        uri: \
         \"http://schemas.microsoft.com/office/word/2010/wordml\",\n    },\n"
    ));
    assert!(text.contains(&format!("strict_uri: Some({W_STRICT:?}),")));
    assert_eq!(text, render("// header\n", &table()));
}

#[test]
fn the_header_names_each_input_with_its_checksum_and_the_tools() {
    let checksums = "aaa  inputs/a.xsd\nbbb  inputs/b.xsd\n";
    let tools = "pdftotext version 1\nopen-xml-sdk abc (v3)\n";
    let text = header(checksums, tools, &[PathBuf::from("inputs/b.xsd")]).unwrap();
    assert_eq!(
        text,
        "// Generated by `cargo xtask generate-schema`; do not edit.\n// Inputs:\n//   bbb  \
         inputs/b.xsd\n// pdftotext version 1\n// open-xml-sdk abc (v3)\n"
    );
}

#[test]
fn an_input_without_a_checksum_is_an_error() {
    let error = header("aaa  inputs/a.xsd\n", "", &[PathBuf::from("inputs/b.xsd")]).unwrap_err();
    assert!(error.to_string().contains("inputs/b.xsd"), "{error}");
}
