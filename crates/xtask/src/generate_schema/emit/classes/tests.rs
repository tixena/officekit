use super::correspondences::{Bits, Codes, Correspondences, Spelling};
use super::text::{self, Class, Impl};
use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::model::{Name, XSD_NAMESPACE};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn rows() -> Vec<Row> {
    vec![Row {
        introduced: None,
        prefix: "w".to_owned(),
        strict_uri: Some("http://purl.oclc.org/ooxml/wordprocessingml/main".to_owned()),
        uri: W.to_owned(),
    }]
}

#[test]
fn the_table_reads_every_kind_of_row() {
    let table = Correspondences::parse(
        r#"
[[member]]
type = "w:ST_Number"
member = "xsd:int"
per_percent = 1000
decimals = 2

[[packed]]
type = "w:CT_Look"
attribute = "val"
bits = { second = 0x0040, first = 0x0020 }

[[character_set]]
type = "w:CT_Charset"
attribute = "val"
to = "characterSet"
codes = "charset"
written = "hexadecimal"

[[character_set]]
type = "w:CT_TextPr"
attribute = "codePage"
to = "characterSet"
codes = "code_page"
written = "decimal"

[charset]
"4D" = "macintosh"
"00" = "iso-8859-1"

[code_page]
1252 = "windows-1252"
37 = "IBM037"
"#,
        &rows(),
    )
    .unwrap();
    assert_eq!(table.members.len(), 1);
    assert_eq!(table.members[0].member, Name::new(XSD_NAMESPACE, "int"));
    assert_eq!(table.members[0].type_name, Name::new(W, "ST_Number"));
    assert_eq!(
        table.members[0].spelling,
        Spelling::Percent {
            decimals: Some(2),
            per_percent: 1000,
        }
    );
    assert_eq!(
        table.packed[0].bits,
        Bits::Hex(vec![
            (0x0020, "first".to_owned()),
            (0x0040, "second".to_owned())
        ])
    );
    assert_eq!(
        table.character_sets[0].codes,
        Codes::Byte(vec![
            (0x00, "iso-8859-1".to_owned()),
            (0x4D, "macintosh".to_owned())
        ])
    );
    assert_eq!(
        table.character_sets[1].codes,
        Codes::Number(vec![
            (37, "IBM037".to_owned()),
            (1252, "windows-1252".to_owned())
        ])
    );
}

#[test]
fn a_prefix_the_namespace_table_lacks_is_an_error() {
    let error = Correspondences::parse("[[value]]\ntype = \"q:ST_Jc\"\nvalues = {}\n", &rows())
        .unwrap_err()
        .to_string();
    assert!(error.contains("`q`"), "{error}");
}

#[test]
fn a_module_sorting_before_class_rules_imports_its_own_first() {
    let module = text::module("// header\n", Class::Strict, "chart", &[], &[]);
    assert!(
        module.contains(
            "use crate::Strict;\n\
             use officekit_markup::chart::{self, read, write};\n\
             use officekit_markup::class_rules;\n"
        ),
        "{module}"
    );
}

#[test]
fn a_module_line_past_the_width_is_braced_across_three_lines() {
    let name = "office_spreadsheetml_2024_workbook_compatibility_version";
    let module = text::module("// header\n", Class::Strict, name, &[], &[]);
    let expected = format!(
        "use crate::Strict;\n\
         use officekit_markup::class_rules;\n\
         use officekit_markup::{name}::{{\n    self, read, write,\n}};\n"
    );
    assert!(module.contains(&expected), "{module}");
}

#[test]
fn a_module_holds_each_node_impl_and_a_root_sets_conformance() {
    let impls = [
        Impl {
            conformance: Some((Some(0), "conformance".to_owned())),
            function: "document".to_owned(),
            name: "Document".to_owned(),
            tag: (Some(0), "document".to_owned()),
        },
        Impl {
            conformance: None,
            function: "paragraph".to_owned(),
            name: "Paragraph".to_owned(),
            tag: (Some(0), "p".to_owned()),
        },
    ];
    let module = text::module(
        "// header\n",
        Class::Strict,
        "wordprocessing",
        &["w".to_owned()],
        &impls,
    );
    assert_eq!(
        module,
        "// header\n\
         //! The `Strict` impl of every node of `officekit_markup::wordprocessing`.\n\
         //!\n\
         //! Each node reads with the reader both classes share and writes with the\n\
         //! shared writer, whose element the Strict write then checks and converts.\n\
         \n\
         use crate::Strict;\n\
         use officekit_markup::class_rules;\n\
         use officekit_markup::wordprocessing::{self, read, write};\n\
         use officekit_xml::element::Element;\n\
         use officekit_xml::error::{ReadError, WriteError};\n\
         use officekit_xml::name::NamespaceId;\n\
         use officekit_xml::write::Scope;\n\
         \n\
         const NS_W: Option<NamespaceId> = Some(0);\n\
         \n\
         impl Strict for wordprocessing::Document {\n    \
             fn from_strict(element: &Element) -> Result<Self, ReadError> {\n        \
                 class_rules::read(element, (NS_W, \"document\"), read::document)\n    \
             }\n\
         \n    \
             fn to_strict(&self) -> Result<Element, WriteError> {\n        \
                 crate::write_root(\n            \
                     write::document(self, &mut Scope::default()),\n            \
                     (NS_W, \"conformance\"),\n        \
                 )\n    \
             }\n\
         }\n\
         \n\
         impl Strict for wordprocessing::Paragraph {\n    \
             fn from_strict(element: &Element) -> Result<Self, ReadError> {\n        \
                 class_rules::read(element, (NS_W, \"p\"), read::paragraph)\n    \
             }\n\
         \n    \
             fn to_strict(&self) -> Result<Element, WriteError> {\n        \
                 crate::write(write::paragraph(self, &mut Scope::default()))\n    \
             }\n\
         }\n"
    );
}

#[test]
fn the_module_list_names_each_module() {
    assert_eq!(
        text::modules(
            "// header\n",
            Class::Transitional,
            &["chart".to_owned(), "wordprocessing".to_owned()]
        ),
        "// header\n\
         // The namespace modules, `officekit::transitional::<namespace>`, brought into\n\
         // the `transitional` module with `include!` ahead of its imports.\n\
         \n\
         pub mod chart;\n\
         pub mod wordprocessing;\n"
    );
}
