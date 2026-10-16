//! The text of the part modules, of `parts.rs` declaring them, of the
//! document kinds, and of the relationship types' constants.

use core::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

use super::{Definition, Joined, Kind, PARTS_PATH, PartType, RELATIONSHIP_TYPE_PATH, Root};
use crate::generate_schema::emit::markup::render as markup_render;
use crate::generate_schema::emit::markup::render::layout::{self, Expr};
use crate::generate_schema::emit::markup::structs::Catalog;
use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::sdk_data::DataVersion;

/// rustfmt's widest array items on one line.
const ARRAY_WIDTH: usize = 60;

/// The `load` of a module without a typed root.
const BYTES_LOAD: &str = "
/// The part's bytes.
///
/// # Errors
///
/// The errors of [`Package::part_bytes`].
pub fn load(package: &Package, name: &PartName) -> Result<Vec<u8>, PackageError> {
    package.part_bytes(name)
}
";

/// The attribute of an item only a build with a class's feature uses.
const EITHER_CLASS: &str = "#[cfg(any(feature = \"strict\", feature = \"transitional\"))]\n";

/// The file types of the document kinds: their enum and the file family its
/// doc names, in the order of their enums.
const KIND_ENUMS: [(&str, &str, &str); 3] = [
    ("presentation", "PresentationKind", ".pptx"),
    ("spreadsheet", "SpreadsheetKind", ".xlsx"),
    ("wordprocessing", "WordprocessingKind", ".docx"),
];

/// `parts.rs`, after the modules it declares.
const PARTS_TAIL: &str = "
/// A generated part module, for code that works over every part type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PartModule {
    /// The module's name, `main_document`.
    pub name: &'static str,
    pub relationship_type: RelationshipType,
    /// The section of ECMA-376 Part 1 that defines the part, `11.3.10`;
    /// `None` for a part of Microsoft's extensions.
    pub section: Option<&'static str>,
}

/// `Ok` when `root` is the element `local` of the namespace `namespace`,
/// else what it is instead.
fn check_root(root: &Element, namespace: NamespaceId, local: &str) -> Result<(), String> {
    let expected = Namespace::Known(namespace);
    if root.name.namespace == expected && root.name.local == local {
        return Ok(());
    }
    Err(format!(
        \"the root is {{{}}}{}, not {{{}}}{local}\",
        root.name.namespace.uri(ConformanceClass::Transitional),
        root.name.local,
        expected.uri(ConformanceClass::Transitional),
    ))
}
";

/// The methods of `RelationshipType`, after its constants.
const RELATIONSHIP_METHODS: &str = "
    /// The spelling `class` writes.
    #[must_use]
    pub const fn in_class(&self, class: ConformanceClass) -> &'static str {
        match class {
            ConformanceClass::Strict => self.strict,
            ConformanceClass::Transitional => self.transitional,
        }
    }

    /// Whether `type_uri` is this type in either class's spelling.
    #[must_use]
    pub fn matches(&self, type_uri: &str) -> bool {
        type_uri == self.transitional || type_uri == self.strict
    }
}
";

/// What a part module writes for its root.
struct RootText {
    /// The function that reads and writes it, `document`.
    function: String,
    local: String,
    /// The namespace module of the typed tree that holds it, `wordprocessing`.
    module: String,
    /// The struct, `Document`.
    name: String,
    namespace: u8,
    /// Whether an earlier module implements `PartRoot` for it.
    shared: bool,
    /// The tag as a doc names it, `w:document`.
    tag: String,
}

/// The doc of a module's content type or content types.
fn content_type_doc(part: &PartType) -> &'static str {
    let several = part.content_types.len() > 1;
    let macro_enabled = part
        .content_types
        .iter()
        .any(|content_type| content_type.contains(".macroEnabled"));
    match (part.stated_in_part_1(), several, macro_enabled) {
        (true, false, _) => "The content type Part 1 lists.",
        (true, true, true) => {
            "The content types Part 1 lists, then the macro-enabled forms and add-ins of Microsoft's extensions."
        }
        (true, true, false) => "The content types Part 1 lists.",
        (false, _, _) => "The content type SDK 3.5.1's part data gives.",
    }
}

/// Every file of the part modules, with its path and text.
///
/// # Errors
///
/// A root whose struct the catalog does not name or whose reader it cannot
/// give, a typed part with no content type, or a part with no constant.
pub fn files(
    header: &str,
    joined: &Joined,
    roots: &[Option<Root>],
    catalog: &Catalog,
    rows: &[Row],
    tests: bool,
) -> Result<Vec<(PathBuf, String)>> {
    let folder = PathBuf::from(PARTS_PATH);
    let mut files = vec![
        (
            PathBuf::from(RELATIONSHIP_TYPE_PATH),
            relationship_types(header, joined),
        ),
        (
            folder.with_extension("rs"),
            parts_text(header, joined, tests),
        ),
        (folder.join("kinds.rs"), kinds_text(header, &joined.kinds)),
    ];
    for (part, root) in joined.parts.iter().zip(roots) {
        let root_text = root
            .map(|known| root_text(known, catalog, rows))
            .transpose()?;
        let constant = joined
            .constants
            .iter()
            .find(|constant| constant.transitional == part.transitional)
            .map(|constant| constant.name.as_str())
            .with_context(|| format!("the part module {} has no relationship type", part.module))?;
        files.push((
            folder.join(format!("{}.rs", part.module)),
            module_text(header, part, root_text.as_ref(), constant)?,
        ));
    }
    Ok(files)
}

/// A doc comment of a module, `//!` lines.
fn inner_doc(text: &str) -> String {
    layout::doc(0, text)
        .lines()
        .fold(String::new(), |mut out, line| {
            let rest = line.strip_prefix("///").unwrap_or(line);
            let _ = writeln!(out, "//!{rest}");
            out
        })
}

/// One enum of `parts/kinds.rs` and its `impl`: its variants in alphabetical
/// order, with `ALL` and `main_content_type`.
fn kind_enum(kinds: &[Kind], file: &str, name: &str, family: &str) -> (String, String) {
    let mut rows: Vec<&Kind> = kinds.iter().filter(|kind| kind.file == file).collect();
    rows.sort_by(|left, right| left.name.cmp(&right.name));
    let mut definition = String::from("\n");
    definition.push_str(&layout::doc(
        0,
        &format!("The kinds of `{family}`-family file, by their main part's content type."),
    ));
    definition.push_str("#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n#[non_exhaustive]\n");
    let _ = writeln!(definition, "pub enum {name} {{");
    for row in &rows {
        let _ = writeln!(definition, "    {},", row.name);
    }
    definition.push_str("}\n");

    let mut implementation = format!("\nimpl {name} {{\n");
    implementation.push_str("    /// Every kind, in the order of the variants.\n");
    let variants: Vec<String> = rows
        .iter()
        .map(|row| format!("Self::{}", row.name))
        .collect();
    let head = format!("    pub const ALL: [Self; {}] = [", rows.len());
    let flat = variants.join(", ");
    if flat.len() <= ARRAY_WIDTH && head.len() + flat.len() + 2 <= layout::MAX_WIDTH {
        let _ = writeln!(implementation, "{head}{flat}];");
    } else {
        let _ = writeln!(implementation, "{head}");
        for variant in &variants {
            let _ = writeln!(implementation, "        {variant},");
        }
        implementation.push_str("    ];\n");
    }
    implementation.push_str(
        "\n    /// The content type of the main part of a file of this kind.\n    #[must_use]\n    pub const fn main_content_type(self) -> &'static str {\n        match self {\n",
    );
    for row in &rows {
        implementation.push_str(&layout::arm(
            12,
            &Expr::atom(&format!("Self::{}", row.name)),
            &Expr::atom(&layout::string(&row.content_type)),
            false,
        ));
    }
    implementation.push_str("        }\n    }\n}\n");
    (definition, implementation)
}

/// `parts/kinds.rs`: one enum per file type, then their `impl`s.
fn kinds_text(header: &str, kinds: &[Kind]) -> String {
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "The document kinds of each file type, by the content type of its main part.\n\nPart 1 lists the kinds of the Main Document, Workbook, and Presentation parts; the macro-enabled forms and the add-ins are Microsoft's extensions.",
    ));
    let mut implementations = String::new();
    for (file, name, family) in KIND_ENUMS {
        let (definition, implementation) = kind_enum(kinds, file, name, family);
        out.push_str(&definition);
        implementations.push_str(&implementation);
    }
    out.push_str(&implementations);
    out
}

/// Words as a list: `a`, `a and b`, `a, b, and c`.
fn list(words: &[String]) -> String {
    match words {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// A typed module's `load`.
fn load_text(path: &str) -> String {
    let mut out = String::from(
        "\n/// The part's typed root, read from either class.\n///\n/// # Errors\n///\n/// The errors of [`Package::load`].\n",
    );
    out.push_str(&layout::signature(
        0,
        "pub fn load",
        &[
            "package: &Package".to_owned(),
            "name: &PartName".to_owned(),
            "options: OpenOptions".to_owned(),
        ],
        &format!("Result<{path}, PackageError>"),
    ));
    out.push_str("    package.load(name, options)\n}\n");
    out
}

/// A module's doc.
fn module_doc(part: &PartType, root: Option<&RootText>) -> String {
    let content = match part.content_types.len() {
        0 => "",
        1 => "its content type, ",
        _ => "its content types, ",
    };
    let held = root.map_or_else(
        || "the load of its bytes".to_owned(),
        |known| format!("the load and store of its root, `{}`", known.tag),
    );
    let what = if content.is_empty() {
        format!("its relationship type and {held}.")
    } else {
        format!("{content}its relationship type, and {held}.")
    };
    match &part.definition {
        Definition::Part1(definition) if definition.part_2 => format!(
            "ECMA-376 Part 1, {} \"{}\".\n\nPart 2 defines its fields, as SDK 3.5.1's part data gives them: {what}",
            definition.section, definition.title
        ),
        Definition::Part1(definition) => {
            let restated = definition
                .part_4
                .as_ref()
                .map_or_else(String::new, |section| format!(", and Part 4, {section}"));
            format!(
                "ECMA-376 Part 1, {} \"{}\"{restated}.\n\nThe module holds {what}",
                definition.section, definition.title
            )
        }
        Definition::Extension(extension) => {
            let since = extension
                .version
                .map_or_else(String::new, |known| format!(", from {}", office(known)));
            format!("A part of Microsoft's extensions{since}.\n\nThe module holds {what}")
        }
    }
}

/// A part module's file.
///
/// # Errors
///
/// A typed part with no content type.
fn module_text(
    header: &str,
    part: &PartType,
    root: Option<&RootText>,
    constant: &str,
) -> Result<String> {
    let mut out = String::from(header);
    out.push_str(&inner_doc(&module_doc(part, root)));
    out.push('\n');
    out.push_str(&uses(root));
    out.push('\n');
    match part.content_types.as_slice() {
        [] => {}
        [one] => {
            out.push_str(&layout::doc(0, content_type_doc(part)));
            layout::statement(
                "pub const CONTENT_TYPE: &str = ",
                &Expr::atom(&layout::string(one)),
                &mut out,
            );
            out.push('\n');
        }
        several => {
            out.push_str(&layout::doc(0, content_type_doc(part)));
            layout::statement(
                "pub const CONTENT_TYPES: &[&str] = ",
                &Expr::Slice(
                    several
                        .iter()
                        .map(|content_type| Expr::atom(&layout::string(content_type)))
                        .collect(),
                ),
                &mut out,
            );
            out.push('\n');
        }
    }
    out.push_str(&layout::doc(0, &relationship_doc(part)));
    layout::statement(
        "pub const RELATIONSHIP_TYPE: RelationshipType = ",
        &Expr::atom(&format!("RelationshipType::{constant}")),
        &mut out,
    );
    match root {
        Some(known) => {
            let fallback = match part.content_types.len() {
                0 => bail!(
                    "the part module {} has a typed root and no content type",
                    part.module
                ),
                1 => "CONTENT_TYPE",
                _ => "CONTENT_TYPES[0]",
            };
            let path = format!("{}::{}", known.module, known.name);
            if !known.shared {
                out.push_str(&part_root_impl(known, &path));
            }
            out.push_str(&load_text(&path));
            out.push_str(&store_text(&path, fallback));
        }
        None => out.push_str(BYTES_LOAD),
    }
    Ok(out)
}

/// An Office version as a doc names it.
const fn office(version: DataVersion) -> &'static str {
    match version {
        DataVersion::Microsoft365 => "Microsoft 365",
        DataVersion::Office2007 => "Office 2007",
        DataVersion::Office2010 => "Office 2010",
        DataVersion::Office2013 => "Office 2013",
        DataVersion::Office2016 => "Office 2016",
        DataVersion::Office2019 => "Office 2019",
        DataVersion::Office2021 => "Office 2021",
    }
}

/// A typed module's `impl PartRoot`.
fn part_root_impl(root: &RootText, path: &str) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::impl_header("PartRoot", path));
    let _ = writeln!(
        out,
        "    const LOCAL_NAME: &'static str = {};\n",
        layout::string(&root.local)
    );
    out.push_str(&layout::signature(
        4,
        "fn from_tree",
        &["root: &Element".to_owned()],
        "Result<Self, String>",
    ));
    let _ = writeln!(
        out,
        "        super::check_root(root, {}, Self::LOCAL_NAME)?;",
        root.namespace
    );
    layout::line(
        "        ",
        &Expr::call(
            "Ok",
            vec![Expr::call(
                &format!("{}::read::{}", root.module, root.function),
                vec![Expr::atom("root")],
            )],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n\n");
    out.push_str(&layout::signature(
        4,
        "fn to_tree",
        &["&self".to_owned(), "_class: ConformanceClass".to_owned()],
        "Result<Element, WriteError>",
    ));
    layout::line(
        "        ",
        &Expr::call(
            "Ok",
            vec![Expr::call(
                &format!("{}::write::{}", root.module, root.function),
                vec![Expr::atom("self"), Expr::atom("&mut Scope::default()")],
            )],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n}\n");
    out
}

/// `parts.rs`: the module declarations, `MODULES`, and the helpers the
/// modules share.
fn parts_text(header: &str, joined: &Joined, tests: bool) -> String {
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "One module per part type (D13): ECMA-376 Part 1's part definitions, then the parts Microsoft's extensions define.\n\nEach holds the part's content type, its relationship type in both classes' spelling, and the load and store of its root, or the load of its bytes; `kinds` holds the document kinds of each file type.",
    ));
    out.push('\n');
    let mut modules: Vec<&str> = joined
        .parts
        .iter()
        .map(|part| part.module.as_str())
        .collect();
    modules.push("kinds");
    modules.sort_unstable();
    for module in modules {
        let _ = writeln!(out, "pub mod {module};");
    }
    if tests {
        out.push_str("\n#[cfg(test)]\nmod tests;\n");
    }
    out.push_str(
        "\nuse crate::packaging::relationship::RelationshipType;\nuse crate::xml::conformance::ConformanceClass;\nuse crate::xml::element::Element;\nuse crate::xml::name::{Namespace, NamespaceId};\n\n",
    );
    out.push_str(&layout::doc(
        0,
        "Every part module: Part 1's definitions in the standard's order, then the parts of Microsoft's extensions by name.",
    ));
    out.push_str("pub const MODULES: &[PartModule] = &[\n");
    for part in &joined.parts {
        let constant = joined
            .constants
            .iter()
            .find(|constant| constant.transitional == part.transitional)
            .map_or("", |constant| constant.name.as_str());
        let section = part.part_1().map_or_else(
            || "None".to_owned(),
            |definition| format!("Some({})", layout::string(&definition.section)),
        );
        let _ = write!(
            out,
            "    PartModule {{\n        name: {},\n        relationship_type: RelationshipType::{constant},\n        section: {section},\n    }},\n",
            layout::string(&part.module)
        );
    }
    out.push_str("];\n");
    out.push_str(PARTS_TAIL);
    out
}

/// The doc of a module's relationship type.
fn relationship_doc(part: &PartType) -> String {
    match &part.definition {
        Definition::Part1(definition) if definition.part_2 => {
            "Part 2's, as SDK 3.5.1's part data gives it: one URI in both classes.".to_owned()
        }
        Definition::Part1(definition) => definition.part_4.as_ref().map_or_else(
            || format!("Part 1, {}; one URI in both classes.", definition.section),
            |restated| format!("Part 1, {}, and Part 4, {restated}.", definition.section),
        ),
        Definition::Extension(_) => "One URI in both classes.".to_owned(),
    }
}

/// `packaging/relationship_type.rs`: the one inherent `impl` of
/// `RelationshipType`, a constant per type, then its methods.
fn relationship_types(header: &str, joined: &Joined) -> String {
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "The relationship types of every part module, in both classes' spelling (D21).\n\nPart 1 states the Strict type of each part and Part 4 the Transitional one; the types of the parts Part 2 defines and of Microsoft's extensions are one URI in both classes.",
    ));
    out.push_str(
        "\nuse crate::packaging::relationship::RelationshipType;\nuse crate::xml::conformance::ConformanceClass;\n\nimpl RelationshipType {\n",
    );
    for constant in &joined.constants {
        let modules: Vec<String> = constant
            .modules
            .iter()
            .map(|module| format!("`{module}`"))
            .collect();
        let both = if constant.strict == constant.transitional {
            "; one URI in both classes"
        } else {
            ""
        };
        out.push_str(&layout::doc(
            4,
            &format!("The relationship type of {}{both}.", list(&modules)),
        ));
        layout::statement(
            &format!("    pub const {}: Self = ", constant.name),
            &Expr::Struct(
                "Self".to_owned(),
                vec![
                    (
                        "strict".to_owned(),
                        Expr::atom(&layout::string(&constant.strict)),
                    ),
                    (
                        "transitional".to_owned(),
                        Expr::atom(&layout::string(&constant.transitional)),
                    ),
                ],
            ),
            &mut out,
        );
    }
    out.push_str(RELATIONSHIP_METHODS);
    out
}

/// What a module writes for its root, from the catalog.
///
/// # Errors
///
/// A struct whose reader the catalog cannot name, or a root in no namespace.
fn root_text(root: Root, catalog: &Catalog, rows: &[Row]) -> Result<RootText> {
    let node = catalog
        .nodes
        .get(root.node)
        .context("a root names no struct of the catalog")?;
    let module = catalog
        .modules
        .get(node.module)
        .context("a root's struct names no module")?;
    let functions = markup_render::functions(catalog, node.module)?;
    let function = functions
        .node_functions
        .get(&root.node)
        .cloned()
        .with_context(|| format!("the struct {} has no reader", node.name))?;
    let namespace = node
        .namespace
        .with_context(|| format!("the root {} is in no namespace", node.name))?;
    let prefix = rows
        .get(usize::from(namespace))
        .map_or("", |row| row.prefix.as_str());
    Ok(RootText {
        function,
        local: node.key.tag.local.clone(),
        module: module.name.clone(),
        name: node.name.clone(),
        namespace,
        shared: root.shared,
        tag: format!("{prefix}:{}", node.key.tag.local),
    })
}

/// A typed module's `store_strict` and `store_transitional`; a new part takes
/// the content type `fallback` names.
fn store_text(path: &str, fallback: &str) -> String {
    let mut out = String::new();
    for (feature, class, function) in [
        ("strict", "Strict", "store_strict"),
        ("transitional", "Transitional", "store_transitional"),
    ] {
        let _ = write!(
            out,
            "\n/// Store `root` into the part in the {class} class; the part keeps its\n/// content type, or takes this module's.\n///\n/// # Errors\n///\n/// The errors of [`Package::store`].\n#[cfg(feature = \"{feature}\")]\n"
        );
        out.push_str(&layout::signature(
            0,
            &format!("pub fn {function}"),
            &[
                "package: &mut Package".to_owned(),
                "name: &PartName".to_owned(),
                format!("root: &{path}"),
                "options: WriteOptions".to_owned(),
            ],
            "Result<(), PackageError>",
        ));
        out.push_str(&layout::chain(
            4,
            "let content_type = package",
            &[
                ".content_type(name)".to_owned(),
                format!(".unwrap_or({fallback})"),
                ".to_owned();".to_owned(),
            ],
        ));
        layout::line(
            "    ",
            &Expr::Method(
                "package.store".to_owned(),
                vec![
                    Expr::atom("name"),
                    Expr::atom("&content_type"),
                    Expr::atom("root"),
                    Expr::atom(&format!("ConformanceClass::{class}")),
                    Expr::atom("options"),
                ],
            ),
            "",
            &mut out,
        );
        out.push_str("}\n");
    }
    out
}

/// A module's `use` lines.
fn uses(root: Option<&RootText>) -> String {
    match root {
        None => "use crate::error::PackageError;\nuse crate::packaging::package::Package;\nuse crate::packaging::part_name::PartName;\nuse crate::packaging::relationship::RelationshipType;\n".to_owned(),
        Some(known) if known.shared => format!(
            "use crate::error::PackageError;\nuse crate::markup::{};\nuse crate::packaging::package::{{OpenOptions, Package}};\nuse crate::packaging::part_name::PartName;\nuse crate::packaging::relationship::RelationshipType;\n{EITHER_CLASS}use crate::xml::conformance::ConformanceClass;\n{EITHER_CLASS}use crate::xml::write::WriteOptions;\n",
            known.module
        ),
        Some(known) => format!(
            "use crate::error::{{PackageError, WriteError}};\nuse crate::markup::{{Scope, {}}};\nuse crate::packaging::package::{{OpenOptions, Package, PartRoot}};\nuse crate::packaging::part_name::PartName;\nuse crate::packaging::relationship::RelationshipType;\nuse crate::xml::conformance::ConformanceClass;\nuse crate::xml::element::Element;\n{EITHER_CLASS}use crate::xml::write::WriteOptions;\n",
            known.module
        ),
    }
}
