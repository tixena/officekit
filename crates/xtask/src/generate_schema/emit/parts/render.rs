//! The text of the part modules, of `lib.rs` declaring them, of the
//! document kinds, of the relationship types' constants, and of `archive.rs`,
//! the mirror `PartModel` archives through.

use alloc::collections::BTreeSet;
use core::fmt::Write as _;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

use super::{Definition, Joined, Kind, PARTS_PATH, PartType, RELATIONSHIP_TYPE_PATH, Root};
use crate::generate_schema::emit::markup::archive::{self, MirrorArm};
use crate::generate_schema::emit::markup::names;
use crate::generate_schema::emit::markup::render as markup_render;
use crate::generate_schema::emit::markup::render::layout::{self, Expr, MAX_WIDTH};
use crate::generate_schema::emit::markup::structs::Catalog;
use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::sdk_data::DataVersion;

/// rustfmt's widest array items on one line.
const ARRAY_WIDTH: usize = 60;

/// rustfmt's widest call arguments, and tuple items, on one line.
const CALL_WIDTH: usize = 60;

/// The head of each closure of the dispatch tables.
const CLOSURE_HEAD: &str = "|part| ";

/// What `lib.rs` adds to the name of a namespace module it imports when a
/// part module has that name.
const MARKUP_SUFFIX: &str = "_markup";

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

/// The attribute of an item only a build without both class features uses.
const SHARED_WRITER: &str = "#[cfg(not(all(feature = \"strict\", feature = \"transitional\")))]\n";
/// The attribute of an item only a Strict build uses.
const STRICT_ONLY: &str = "#[cfg(feature = \"strict\")]\n";
/// The attribute of an item only a Transitional build uses.
const TRANSITIONAL_ONLY: &str = "#[cfg(feature = \"transitional\")]\n";

/// The attribute of an item only a build with a class's feature uses.
const EITHER_CLASS: &str = "#[cfg(any(feature = \"strict\", feature = \"transitional\"))]\n";

/// The body of a typed root's `PartRoot::to_tree`: each class's writer when
/// its feature is on, else the shared one, and the end of the `impl`.
const TO_TREE_BODY: &str = "        #[cfg(feature = \"strict\")]
        let strict = Self::to_strict;
        #[cfg(not(feature = \"strict\"))]
        let strict = |root: &Self| Ok(shared_tree(root));
        #[cfg(feature = \"transitional\")]
        let transitional = Self::to_transitional;
        #[cfg(not(feature = \"transitional\"))]
        let transitional = |root: &Self| Ok(shared_tree(root));
        super::tree_in_class(self, class, strict, transitional)
    }
}

";

/// The helper every typed root's `to_tree` calls, the last item of `lib.rs`.
const TREE_IN_CLASS: &str = "
/// The tree of `root` in `class`, from `strict` or `transitional`, the writer of that class.
///
/// # Errors
///
/// The writer's.
fn tree_in_class<R>(
    root: &R,
    class: ConformanceClass,
    strict: fn(&R) -> Result<Element, WriteError>,
    transitional: fn(&R) -> Result<Element, WriteError>,
) -> Result<Element, WriteError> {
    if class == ConformanceClass::Strict {
        strict(root)
    } else {
        transitional(root)
    }
}
";

/// The file types of the document kinds: their enum and the file family its
/// doc names, in the order of their enums.
const KIND_ENUMS: [(&str, &str, &str); 3] = [
    ("presentation", "PresentationKind", ".pptx"),
    ("spreadsheet", "SpreadsheetKind", ".xlsx"),
    ("wordprocessing", "WordprocessingKind", ".docx"),
];

/// The struct of `lib.rs`'s `MODULES`.
const PART_MODULE: &str = "
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
";

/// The root check every typed module's `from_tree` calls.
const CHECK_ROOT: &str = "
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

/// The types `restore_through_module` dispatches with.
const RESTORE_TYPES: &str = "
/// A part `restore_through_module` stores again: where it is, its content type, and the options
/// it is read and written with.
struct Restore<'part> {
    class: ConformanceClass,
    content_type: &'part str,
    name: &'part PartName,
    open: OpenOptions,
    package: &'part mut Package,
    write: WriteOptions,
}

/// The store again of a part through one module.
type Restorer = fn(Restore<'_>) -> Result<(), PackageError>;
";

/// `Restore::by_root`, the first item of its `impl` when a content type is
/// shared.
const BY_ROOT: &str =
    "    /// The part through the module whose root element it holds, when one does.
    fn by_root(self) -> Result<(), PackageError> {
        root_entry(self.package, self.name, ROOTS)?.map_or(Ok(()), |restorer| restorer(self))
    }

";

/// `Load::by_root`, the first item of its `impl` when a content type is
/// shared.
const LOAD_BY_ROOT: &str =
    "    /// The part's typed root through the module whose root element it holds, when one does.
    fn by_root(self) -> Result<Option<PartModel>, PackageError> {
        root_entry(self.package, self.name, MODEL_ROOTS)?.map_or(Ok(None), |modeler| modeler(self))
    }

";

/// `Load::through`, after `Load::by_root`.
const LOAD_THROUGH: &str =
    "    /// The part's typed root, read as the root `variant` holds, in `variant`.
    fn through<R>(self, variant: fn(R) -> PartModel) -> Result<Option<PartModel>, PackageError>
    where
        R: PartRoot,
    {
        part_root::load(self.package, self.name, self.open).map(|root| Some(variant(root)))
    }
";

/// `Load::through_boxed`, the last item of its `impl` when `PartModel` holds
/// a root boxed.
const LOAD_THROUGH_BOXED: &str = "
    /// The part's typed root, read as the root `variant` holds boxed, in `variant`.
    fn through_boxed<R>(
        self,
        variant: fn(Box<R>) -> PartModel,
    ) -> Result<Option<PartModel>, PackageError>
    where
        R: PartRoot,
    {
        part_root::load(self.package, self.name, self.open)
            .map(|root| Some(variant(Box::new(root))))
    }
";

/// The types `model_of` dispatches with.
const LOAD_TYPES: &str = "
/// A part `model_of` loads: where it is, and the options it is read with.
struct Load<'part> {
    name: &'part PartName,
    open: OpenOptions,
    package: &'part Package,
}

/// The typed root of a part through one module.
type Modeler = fn(Load<'_>) -> Result<Option<PartModel>, PackageError>;
";

/// The macro `bytes_of` writes a typed root through, the first item after
/// `lib.rs`'s imports.
const TREE_OF: &str = "
/// The match on the part model `$model` that writes the typed root of each of the variants
/// `$variant` in the class `$class`: a match of every variant is too long for one function.
macro_rules! tree_of {
    ($model:expr, $class:expr; $($variant:ident),+ $(,)?) => {
        match $model {
            $(PartModel::$variant(root) => root.to_tree($class),)+
        }
    };
}
";

/// `model_of`, after `check_root`.
const MODEL_OF: &str = "
/// The typed root of the part `name`, through the module its content type names, the same way
/// `restore_through_module` finds it; `None` for a part no module takes.
///
/// The module reads the part with `options`. A content type several modules list goes by the
/// part's root element.
///
/// # Errors
///
/// The errors of the module's `load`, and [`PackageError::Xml`] for a part of a content type
/// several modules list that is not well-formed.
pub fn model_of(
    package: &Package,
    name: &PartName,
    options: OpenOptions,
) -> Result<Option<PartModel>, PackageError> {
    let Some(content_type) = package.content_type(name) else {
        return Ok(None);
    };
    let Some((_, modeler)) = MODELS.iter().find(|(listed, _)| *listed == content_type) else {
        return Ok(None);
    };
    modeler(Load {
        name,
        open: options,
        package,
    })
}
";

/// `root_entry`, after `restore_through_module` when a content type is
/// shared.
const ROOT_ENTRY: &str = "
/// The entry of `roots` whose root element the part `name` holds, when one does.
///
/// # Errors
///
/// The errors of [`Package::part_bytes`], and [`PackageError::Xml`] for a part that is not
/// well-formed.
fn root_entry<'roots, T>(
    package: &Package,
    name: &PartName,
    roots: &'roots [(NamespaceId, &str, T)],
) -> Result<Option<&'roots T>, PackageError> {
    let root = parse(&package.part_bytes(name)?)
        .map_err(|error| PackageError::Xml {
            error,
            part: name.clone(),
        })?
        .root;
    Ok(roots
        .iter()
        .find(|(namespace, local, _)| {
            root.name.namespace == Namespace::Known(*namespace) && root.name.local == *local
        })
        .map(|(_, _, entry)| entry))
}
";

/// `Restore::through`, the last item of its `impl`.
const THROUGH: &str =
    "    /// Load the part with `load` and store its typed root again in the class.
    fn through<R>(
        self,
        load: fn(&Package, &PartName, OpenOptions) -> Result<R, PackageError>,
    ) -> Result<(), PackageError>
    where
        R: PartRoot,
    {
        let root = load(self.package, self.name, self.open)?;
        part_root::store(
            self.package,
            self.name,
            self.content_type,
            &root,
            self.class,
            self.write,
        )
    }
}
";

/// `restore_through_module`, the last item of `lib.rs`.
const RESTORE_THROUGH_MODULE: &str = "
/// Store the part `name` again in `class` through the module its content type names.
///
/// The module's `load` reads the part with `open`, and [`part_root::store`] writes its typed root
/// with `write`. A content type several modules list goes by the part's root element; a part no
/// module with a typed root takes is left as it is.
///
/// # Errors
///
/// The errors of the module's `load` and of [`part_root::store`], and [`PackageError::Xml`] for a
/// part of a content type several modules list that is not well-formed.
pub fn restore_through_module(
    package: &mut Package,
    name: &PartName,
    class: ConformanceClass,
    open: OpenOptions,
    write: WriteOptions,
) -> Result<(), PackageError> {
    let Some(content_type) = package.content_type(name).map(str::to_owned) else {
        return Ok(());
    };
    let Some((_, restorer)) = RESTORES.iter().find(|(listed, _)| *listed == content_type) else {
        return Ok(());
    };
    restorer(Restore {
        class,
        content_type: &content_type,
        name,
        open,
        package,
        write,
    })
}
";

/// The methods of `RelationshipType`, after its constants.
const RELATIONSHIP_METHODS: &str = "
    /// The type `type_uri` spells in either class.
    #[must_use]
    pub fn find(type_uri: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|known| known.matches(type_uri))
    }

    /// The spelling `class` writes.
    #[must_use]
    pub const fn in_class(&self, class: ConformanceClass) -> &'static str {
        if matches!(class, ConformanceClass::Strict) {
            self.strict
        } else {
            self.transitional
        }
    }

    /// Whether `type_uri` is this type in either class's spelling.
    #[must_use]
    pub fn matches(&self, type_uri: &str) -> bool {
        type_uri == self.transitional || type_uri == self.strict
    }
}
";

/// One dispatch from a part's content type to its module: the closure of a
/// module's entry, the type it has, what it gives, and the names of the two
/// tables.
struct Dispatch<'closure> {
    closure: &'closure dyn Fn(&PartType, &RootText) -> String,
    element: &'static str,
    /// What the closure gives, `the store again of a part`.
    gives: &'static str,
    roots: &'static str,
    table: &'static str,
}

/// What a part module writes for its root.
struct RootText {
    /// Whether `PartModel` holds it boxed.
    boxed: bool,
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
    let nodes: Vec<usize> = roots.iter().flatten().map(|known| known.node).collect();
    let boxed = catalog.boxed_arms(&nodes);
    let root_texts = roots
        .iter()
        .map(|root| {
            root.map(|known| root_text(known, catalog, rows, &boxed))
                .transpose()
        })
        .collect::<Result<Vec<Option<RootText>>>>()?;
    let mut files = vec![
        (
            PathBuf::from(RELATIONSHIP_TYPE_PATH),
            relationship_types(header, joined),
        ),
        (
            folder.join("lib.rs"),
            parts_text(header, joined, &root_texts, tests),
        ),
        (folder.join("kinds.rs"), kinds_text(header, &joined.kinds)),
        (
            folder.join("archive.rs"),
            archive_text(header, &typed_parts(joined, &root_texts)),
        ),
    ];
    for (part, root_text) in joined.parts.iter().zip(&root_texts) {
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

/// One enum of `kinds.rs` and its `impl`: its variants in alphabetical
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

/// `kinds.rs`: one enum per file type, then their `impl`s.
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
        "\n/// The part's typed root, read from either class.\n///\n/// # Errors\n///\n/// The errors of [`part_root::load`].\n",
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
    out.push_str("    part_root::load(package, name, options)\n}\n");
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
        &["&self".to_owned(), "class: ConformanceClass".to_owned()],
        "Result<Element, WriteError>",
    ));
    out.push_str(TO_TREE_BODY);
    out.push_str(SHARED_WRITER);
    out.push_str(&layout::signature(
        0,
        "fn shared_tree",
        &[format!("root: &{path}")],
        "Element",
    ));
    layout::line(
        "    ",
        &Expr::call(
            &format!("{}::write::{}", root.module, root.function),
            vec![Expr::atom("root"), Expr::atom("&mut Scope::default()")],
        ),
        "",
        &mut out,
    );
    out.push_str("}\n");
    out
}

/// `lib.rs`: the module declarations, `MODULES`, the helpers the modules
/// share, and the dispatch from a part's content type to its module.
fn parts_text(header: &str, joined: &Joined, roots: &[Option<RootText>], tests: bool) -> String {
    let typed = typed_parts(joined, roots);
    let shared: BTreeSet<&str> = typed
        .iter()
        .flat_map(|(part, _)| part.content_types.iter().map(String::as_str))
        .filter(|content_type| {
            typed
                .iter()
                .filter(|(other, _)| {
                    other
                        .content_types
                        .iter()
                        .any(|found| found == content_type)
                })
                .count()
                > 1
        })
        .collect();
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "One module per part type: ECMA-376 Part 1's part definitions, then the parts Microsoft's extensions define.\n\nEach holds the part's content type, its relationship type in both classes' spelling, and the load and store of its root, or the load of its bytes; `kinds` holds the document kinds of each file type. `restore_through_module` stores a part again in a class through the module its content type names; `model_of` gives a part's typed root as a `PartModel`, and `bytes_of` the part's bytes from it.",
    ));
    out.push('\n');
    let taken = module_declarations(joined, tests, &mut out);
    lib_imports(&typed, &taken, !shared.is_empty(), &mut out);
    out.push_str(TREE_OF);
    out.push('\n');
    modules_constant(joined, &mut out);
    let restore = |part: &PartType, _: &RootText| {
        format!("{CLOSURE_HEAD}part.through({}::load)", part.module)
    };
    let model = |part: &PartType, root: &RootText| {
        let through = if root.boxed {
            "through_boxed"
        } else {
            "through"
        };
        format!(
            "{CLOSURE_HEAD}part.{through}(PartModel::{})",
            variant_name(part)
        )
    };
    dispatch_tables(
        &typed,
        &shared,
        &Dispatch {
            closure: &restore,
            element: "Restorer",
            gives: "the store again of a part",
            roots: "ROOTS",
            table: "RESTORES",
        },
        &mut out,
    );
    dispatch_tables(
        &typed,
        &shared,
        &Dispatch {
            closure: &model,
            element: "Modeler",
            gives: "the typed root of a part",
            roots: "MODEL_ROOTS",
            table: "MODELS",
        },
        &mut out,
    );
    out.push_str(&part_model(&typed, &taken));
    lib_items(&typed, !shared.is_empty(), &mut out);
    out
}

/// `lib.rs`'s module declarations, the private `archive` among them, and
/// the unit tests' when `tests`; returns the names the modules take.
fn module_declarations<'joined>(
    joined: &'joined Joined,
    tests: bool,
    out: &mut String,
) -> BTreeSet<&'joined str> {
    let mut modules: Vec<(&str, &str)> = joined
        .parts
        .iter()
        .map(|part| ("pub ", part.module.as_str()))
        .collect();
    modules.push(("", "archive"));
    modules.push(("pub ", "kinds"));
    modules.push(("pub ", "part_root"));
    modules.sort_unstable_by_key(|(_, module)| *module);
    for (visibility, module) in &modules {
        if *module == "archive" {
            out.push_str("#[cfg(feature = \"binary-format\")]\n");
        }
        let _ = writeln!(out, "{visibility}mod {module};");
    }
    if tests {
        out.push_str("\n#[cfg(test)]\nmod tests;\n");
    }
    modules.into_iter().map(|(_, module)| module).collect()
}

/// `lib.rs`'s imports: the namespace module of each typed root, one a module
/// of `taken` names imported under another name, and `parse` when a content
/// type is `shared`.
fn lib_imports(
    typed: &[(&PartType, &RootText)],
    taken: &BTreeSet<&str>,
    shared: bool,
    out: &mut String,
) {
    let namespaces: BTreeSet<&str> = typed.iter().map(|(_, root)| root.module.as_str()).collect();
    let mut markup_uses = String::new();
    for namespace in namespaces {
        if taken.contains(namespace) {
            let _ = writeln!(
                markup_uses,
                "use officekit_markup::{namespace} as {namespace}{MARKUP_SUFFIX};"
            );
        } else {
            let _ = writeln!(markup_uses, "use officekit_markup::{namespace};");
        }
    }
    let parse_use = if shared {
        "use officekit_xml::parse::parse;\n"
    } else {
        ""
    };
    let _ = write!(
        out,
        "\nuse crate::part_root::PartRoot;\n{markup_uses}use officekit_packaging::error::PackageError;\nuse officekit_packaging::package::{{DECLARATION, OpenOptions, Package}};\nuse officekit_packaging::part_name::PartName;\nuse officekit_packaging::relationship::RelationshipType;\nuse officekit_xml::conformance::ConformanceClass;\nuse officekit_xml::element::{{Document, Element}};\nuse officekit_xml::error::WriteError;\nuse officekit_xml::name::{{Namespace, NamespaceId}};\n{parse_use}use officekit_xml::write::WriteOptions;\n",
    );
}

/// `MODULES`, every part module in `joined`'s order.
fn modules_constant(joined: &Joined, out: &mut String) {
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
}

/// `lib.rs`'s types, their `impl`s and its functions, after `PartModel`;
/// the dispatch by root element when a content type is `shared`.
fn lib_items(typed: &[(&PartType, &RootText)], shared: bool, out: &mut String) {
    out.push_str(PART_MODULE);
    out.push_str(LOAD_TYPES);
    out.push_str(RESTORE_TYPES);
    out.push_str("\nimpl Load<'_> {\n");
    if shared {
        out.push_str(LOAD_BY_ROOT);
    }
    out.push_str(LOAD_THROUGH);
    if typed.iter().any(|(_, root)| root.boxed) {
        out.push_str(LOAD_THROUGH_BOXED);
    }
    out.push_str("}\n");
    out.push_str("\nimpl Restore<'_> {\n");
    if shared {
        out.push_str(BY_ROOT);
    }
    out.push_str(THROUGH);
    out.push_str(&bytes_of(typed));
    out.push_str(CHECK_ROOT);
    out.push_str(MODEL_OF);
    out.push_str(RESTORE_THROUGH_MODULE);
    if shared {
        out.push_str(ROOT_ENTRY);
    }
    out.push_str(TREE_IN_CLASS);
}

/// The `PartModel` enum: one variant per module with a typed root, by name,
/// each holding its root; a namespace module named as one of `taken` is
/// imported under another name.
fn part_model(typed: &[(&PartType, &RootText)], taken: &BTreeSet<&str>) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::doc(
        0,
        "The typed root of a part, by the module that loads it. With `binary-format`, rkyv archives it through its mirror in `archive`.",
    ));
    out.push_str("#[derive(Clone, Debug, PartialEq)]\n#[non_exhaustive]\npub enum PartModel {\n");
    for arm in model_arms(typed, |namespace| {
        if taken.contains(namespace) {
            format!("{namespace}{MARKUP_SUFFIX}")
        } else {
            namespace.to_owned()
        }
    }) {
        out.push_str(&archive::variant(&arm));
    }
    out.push_str("}\n");
    out
}

/// One arm per module with a typed root, by variant: the variant and its
/// root, its namespace module named as `module` gives it.
fn model_arms<F>(typed: &[(&PartType, &RootText)], module: F) -> Vec<MirrorArm>
where
    F: Fn(&str) -> String,
{
    let mut arms: Vec<MirrorArm> = typed
        .iter()
        .map(|(part, root)| {
            let path = format!("{}::{}", module(&root.module), root.name);
            MirrorArm {
                recursive: false,
                shown: if root.boxed {
                    format!("Box<{path}>")
                } else {
                    path
                },
                variant: variant_name(part),
            }
        })
        .collect();
    arms.sort_by(|left, right| left.variant.cmp(&right.variant));
    arms
}

/// `bytes_of`, its match on the model `tree_of!` called with every variant.
fn bytes_of(typed: &[(&PartType, &RootText)]) -> String {
    let mut out = String::from("\n");
    out.push_str(&layout::doc(
        0,
        "The bytes of the part `name` from its typed root `model`, in `class`, every namespace declared with that class's URI, written with `write`.\n\n# Errors\n\n[`PackageError::Write`] naming `name` when the root holds content `class` has no form for, as [`part_root::store`] does.",
    ));
    out.push_str(&layout::signature(
        0,
        "pub fn bytes_of",
        &[
            "name: &PartName".to_owned(),
            "model: &PartModel".to_owned(),
            "class: ConformanceClass".to_owned(),
            "write: WriteOptions".to_owned(),
        ],
        "Result<Vec<u8>, PackageError>",
    ));
    out.push_str("    let written = tree_of!(model, class;\n");
    let mut row = String::new();
    for arm in model_arms(typed, str::to_owned) {
        let item = format!("{},", arm.variant);
        if !row.is_empty() && 8 + row.len() + 1 + item.len() > MAX_WIDTH {
            let _ = writeln!(out, "        {row}");
            row.clear();
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(&item);
    }
    let _ = writeln!(out, "        {row}\n    );");
    out.push_str(
        "    let mut tree = written.map_err(|error| PackageError::Write {\n        error,\n        part: name.clone(),\n    })?;\n    tree.set_class(class);\n    Ok(Document::new(DECLARATION, tree).to_xml(write))\n}\n",
    );
    out
}

/// `archive.rs`: the mirror rkyv archives `PartModel` through, its `From`,
/// and `PartModel`'s hand-off impls.
fn archive_text(header: &str, typed: &[(&PartType, &RootText)]) -> String {
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "The mirror rkyv archives `crate::PartModel` through: the archived enum and resolver it writes stay in this private module.",
    ));
    out.push('\n');
    let namespaces: BTreeSet<&str> = typed.iter().map(|(_, root)| root.module.as_str()).collect();
    for namespace in namespaces {
        let _ = writeln!(out, "use officekit_markup::{namespace};");
    }
    out.push_str(
        "use rkyv::de::Pooling;\nuse rkyv::rancor::{Fallible, Source};\nuse rkyv::ser::{Allocator, Sharing, Writer};\nuse rkyv::with::{ArchiveWith as _, DeserializeWith as _, SerializeWith as _};\n\n",
    );
    let arms = model_arms(typed, str::to_owned);
    out.push_str(&archive::mirror("PartModel", &arms, false));
    out.push_str(&archive::from("officekit_markup", "PartModel", &arms));
    out.push_str(&archive::handoffs("PartModel"));
    out
}

/// The modules with a typed root, each with its root.
fn typed_parts<'joined>(
    joined: &'joined Joined,
    roots: &'joined [Option<RootText>],
) -> Vec<(&'joined PartType, &'joined RootText)> {
    joined
        .parts
        .iter()
        .zip(roots)
        .filter_map(|(part, root)| root.as_ref().map(|known| (part, known)))
        .collect()
}

/// The variant of `PartModel` that holds the typed root of `part`'s module:
/// its name in `PascalCase`.
fn variant_name(part: &PartType) -> String {
    let words: Vec<String> = part.module.split('_').map(str::to_owned).collect();
    names::pascal(&words)
}

/// One entry of a table at indent 4, a tuple of `items` and, last, `last`,
/// as rustfmt lays it out: on one line when its items take at most
/// `CALL_WIDTH` columns; a closure's body in a block after the closure's
/// head when the items before it and that head do; else one item per line.
fn table_entry(items: &[String], last: &str) -> String {
    let before = items.iter().fold(String::new(), |mut out, item| {
        let _ = write!(out, "{item}, ");
        out
    });
    if before.len() + last.len() <= CALL_WIDTH {
        return format!("    ({before}{last}),\n");
    }
    if let Some(body) = last.strip_prefix(CLOSURE_HEAD)
        && before.len() + CLOSURE_HEAD.len() + "{".len() <= CALL_WIDTH
    {
        return format!("    ({before}{CLOSURE_HEAD}{{\n        {body}\n    }}),\n");
    }
    let mut out = String::from("    (\n");
    for item in items {
        let _ = writeln!(out, "        {item},");
    }
    let _ = writeln!(out, "        {last},\n    ),");
    out
}

/// The dispatch's table, each content type of a typed module with the
/// dispatch's closure through that module, and its table of roots, the root
/// of each module whose content type is `shared`, when one is.
fn dispatch_tables(
    typed: &[(&PartType, &RootText)],
    shared: &BTreeSet<&str>,
    dispatch: &Dispatch<'_>,
    out: &mut String,
) {
    let entry = dispatch.closure;
    let element = dispatch.element;
    out.push('\n');
    out.push_str(&layout::doc(
        0,
        &format!(
            "Each content type a module with a typed root lists, and {} of it through that module; a content type several modules list goes by the part's root element.",
            dispatch.gives
        ),
    ));
    let _ = writeln!(out, "const {}: &[(&str, {element})] = &[", dispatch.table);
    for (part, root) in typed {
        for content_type in &part.content_types {
            if !shared.contains(content_type.as_str()) {
                out.push_str(&table_entry(
                    &[layout::string(content_type)],
                    &entry(part, root),
                ));
            }
        }
    }
    for content_type in shared {
        out.push_str(&table_entry(
            &[layout::string(content_type)],
            &format!("{CLOSURE_HEAD}part.by_root()"),
        ));
    }
    out.push_str("];\n");
    if shared.is_empty() {
        return;
    }
    out.push('\n');
    out.push_str(&layout::doc(
        0,
        &format!(
            "The root element of each module whose content type another module lists too: its namespace and local name, and {} through that module.",
            dispatch.gives
        ),
    ));
    let _ = writeln!(
        out,
        "const {}: &[(NamespaceId, &str, {element})] = &[",
        dispatch.roots
    );
    for (part, root) in typed {
        if part
            .content_types
            .iter()
            .any(|content_type| shared.contains(content_type.as_str()))
        {
            out.push_str(&table_entry(
                &[root.namespace.to_string(), layout::string(&root.local)],
                &entry(part, root),
            ));
        }
    }
    out.push_str("];\n");
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

/// The constant `ALL`, which lists every constant of `joined`.
fn all_constant(joined: &Joined, out: &mut String) {
    out.push_str(&layout::doc(
        4,
        "Every relationship type of the standard and the extensions.",
    ));
    layout::statement(
        "    pub const ALL: &'static [Self] = ",
        &Expr::Slice(
            joined
                .constants
                .iter()
                .map(|constant| Expr::atom(&format!("Self::{}", constant.name)))
                .collect(),
        ),
        out,
    );
}

/// `officekit-packaging`'s `relationship_type.rs`: the one inherent `impl` of
/// `RelationshipType`, a constant per type, then its methods.
fn relationship_types(header: &str, joined: &Joined) -> String {
    let mut out = String::from(header);
    out.push_str(&inner_doc(
        "The relationship types of every part module and of the sections of Part 1 that define no part, in both classes' spelling.\n\nPart 1 states the Strict type of each part and Part 4 the Transitional one; the types of the parts Part 2 defines and of Microsoft's extensions are one URI in both classes.",
    ));
    out.push_str(
        "\nuse crate::relationship::RelationshipType;\nuse officekit_xml::conformance::ConformanceClass;\n\nimpl RelationshipType {\n",
    );
    let mut all_written = false;
    for constant in &joined.constants {
        if !all_written && constant.name.as_str() > "ALL" {
            all_constant(joined, &mut out);
            all_written = true;
        }
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
        let doc = constant.source.as_ref().map_or_else(
            || format!("The relationship type of {}{both}.", list(&modules)),
            |source| format!("The relationship type of {source}{both}; no part module."),
        );
        out.push_str(&layout::doc(4, &doc));
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
    if !all_written {
        all_constant(joined, &mut out);
    }
    out.push_str(RELATIONSHIP_METHODS);
    out
}

/// What a module writes for its root, from the catalog.
///
/// # Errors
///
/// A struct whose reader the catalog cannot name, or a root in no namespace.
fn root_text(
    root: Root,
    catalog: &Catalog,
    rows: &[Row],
    boxed: &BTreeSet<usize>,
) -> Result<RootText> {
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
        boxed: boxed.contains(&root.node),
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
            "\n/// Store `root` into the part in the {class} class; the part keeps its\n/// content type, or takes this module's.\n///\n/// # Errors\n///\n/// The errors of [`part_root::store`].\n#[cfg(feature = \"{feature}\")]\n"
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
            &Expr::call(
                "part_root::store",
                vec![
                    Expr::atom("package"),
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
        None => "use officekit_packaging::error::PackageError;\nuse officekit_packaging::package::Package;\nuse officekit_packaging::part_name::PartName;\nuse officekit_packaging::relationship::RelationshipType;\n".to_owned(),
        Some(known) if known.shared => format!(
            "use crate::part_root;\nuse officekit_markup::{};\nuse officekit_packaging::error::PackageError;\nuse officekit_packaging::package::{{OpenOptions, Package}};\nuse officekit_packaging::part_name::PartName;\nuse officekit_packaging::relationship::RelationshipType;\n{EITHER_CLASS}use officekit_xml::conformance::ConformanceClass;\n{EITHER_CLASS}use officekit_xml::write::WriteOptions;\n",
            known.module
        ),
        Some(known) => format!(
            "use crate::part_root::{{self, PartRoot}};\nuse officekit_markup::{};\nuse officekit_packaging::error::PackageError;\nuse officekit_packaging::package::{{OpenOptions, Package}};\nuse officekit_packaging::part_name::PartName;\nuse officekit_packaging::relationship::RelationshipType;\n{STRICT_ONLY}use officekit_strict::Strict as _;\n{TRANSITIONAL_ONLY}use officekit_transitional::Transitional as _;\nuse officekit_xml::conformance::ConformanceClass;\nuse officekit_xml::element::Element;\nuse officekit_xml::error::WriteError;\n{SHARED_WRITER}use officekit_xml::write::Scope;\n{EITHER_CLASS}use officekit_xml::write::WriteOptions;\n",
            known.module
        ),
    }
}
