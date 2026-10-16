use std::path::Path;

use os_shim::mock::MemorySystem;

use super::Inputs;

const MISSING_PREFIX: &str = "missing input: ";

fn layout() -> MemorySystem {
    let mut system = MemorySystem::new()
        .with_file("inputs/ecma-376/part1.txt", b"part 1")
        .unwrap()
        .with_file("inputs/ecma-376/part4.txt", b"part 4")
        .unwrap()
        .with_dir("inputs/open-xml-sdk/data")
        .unwrap()
        .with_file("inputs/open-specifications/SOURCES", b"sources")
        .unwrap()
        .with_file("inputs/open-specifications/aink.xsd", b"<xsd/>")
        .unwrap()
        .with_file("inputs/open-specifications/xr2.xsd", b"<xsd/>")
        .unwrap()
        .with_file("inputs/sdk-3.5.1-semantic.json", b"[]")
        .unwrap()
        .with_file("inputs/SHA256SUMS", b"abc  inputs/TOOLS\ndef  inputs/x\n")
        .unwrap()
        .with_file(
            "inputs/TOOLS",
            b"pdftotext version 26.01.0\nopen-xml-sdk 3139\n",
        )
        .unwrap();
    for index in 0_u8..21_u8 {
        system = system
            .with_file(format!("inputs/ecma-376/strict/s{index:02}.xsd"), b"<xsd/>")
            .unwrap();
    }
    for index in 0_u8..26_u8 {
        system = system
            .with_file(
                format!("inputs/ecma-376/transitional/t{index:02}.xsd"),
                b"<xsd/>",
            )
            .unwrap();
    }
    for name in ["contentTypes", "coreProperties", "digSig", "relationships"] {
        system = system
            .with_file(format!("inputs/ecma-376/opc/opc-{name}.xsd"), b"<xsd/>")
            .unwrap();
    }
    system
}

fn without(skipped: &str) -> MemorySystem {
    use os_shim::System as _;

    let system = layout();
    system.remove_file(Path::new(skipped)).unwrap();
    system
}

#[test]
fn finds_every_input() {
    let inputs = Inputs::open(&layout(), Path::new("inputs")).unwrap();
    assert_eq!(inputs.strict_schemas.len(), 21);
    assert_eq!(inputs.transitional_schemas.len(), 26);
    assert_eq!(inputs.opc_schemas.len(), 4);
    assert_eq!(inputs.part1_text, Path::new("inputs/ecma-376/part1.txt"));
    assert_eq!(inputs.part4_text, Path::new("inputs/ecma-376/part4.txt"));
    assert_eq!(inputs.sdk_data, Path::new("inputs/open-xml-sdk/data"));
    assert_eq!(
        inputs.semantic_dump,
        Path::new("inputs/sdk-3.5.1-semantic.json")
    );
    assert_eq!(
        inputs.open_specifications,
        [
            (
                "aink".to_owned(),
                Path::new("inputs/open-specifications/aink.xsd").to_path_buf()
            ),
            (
                "xr2".to_owned(),
                Path::new("inputs/open-specifications/xr2.xsd").to_path_buf()
            ),
        ]
    );
}

#[test]
fn returns_the_checksum_and_tool_lines() {
    let inputs = Inputs::open(&layout(), Path::new("inputs")).unwrap();
    assert_eq!(inputs.checksums(), "abc  inputs/TOOLS\ndef  inputs/x\n");
    assert_eq!(
        inputs.tools(),
        "pdftotext version 26.01.0\nopen-xml-sdk 3139\n"
    );
}

#[test]
fn names_the_missing_part4_text() {
    let err = Inputs::open(&without("inputs/ecma-376/part4.txt"), Path::new("inputs")).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{MISSING_PREFIX}inputs/ecma-376/part4.txt")
    );
}

#[test]
fn names_the_missing_checksums() {
    let err = Inputs::open(&without("inputs/SHA256SUMS"), Path::new("inputs")).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{MISSING_PREFIX}inputs/SHA256SUMS")
    );
}

#[test]
fn names_the_first_missing_input_of_an_empty_tree() {
    let err = Inputs::open(&MemorySystem::new(), Path::new("inputs")).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{MISSING_PREFIX}inputs/ecma-376/part1.txt")
    );
}

#[test]
fn names_the_missing_sdk_data() {
    use os_shim::System as _;

    let system = layout();
    system
        .remove_dir_all(Path::new("inputs/open-xml-sdk/data"))
        .unwrap();
    let err = Inputs::open(&system, Path::new("inputs")).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{MISSING_PREFIX}inputs/open-xml-sdk/data")
    );
}

#[test]
fn names_the_missing_semantic_dump() {
    let err = Inputs::open(
        &without("inputs/sdk-3.5.1-semantic.json"),
        Path::new("inputs"),
    )
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("{MISSING_PREFIX}inputs/sdk-3.5.1-semantic.json")
    );
}
