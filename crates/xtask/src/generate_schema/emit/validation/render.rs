//! The Rust text of the validation tables, laid out as rustfmt lays it out.

use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};
use core::iter;

use anyhow::{Context as _, Result, bail};

use super::{
    AnyNamespace, AttributeRow, Bounds, Check, ChildPartRow, ChildRow, CompositeKind,
    EnumerationRow, PartRow, Particle, StringKind, Tables, TypeRow, ValueCheck, ValueClass,
    Versions,
};
use crate::generate_schema::sdk_data::DataChildKind;

/// The name `files` gives the module file, `generated.rs`.
pub(super) const MODULE: &str = "generated.rs";

/// rustfmt's widest array items on one line.
const ARRAY_WIDTH: usize = 60;
/// rustfmt's widest call or tuple arguments on one line.
const CALL_WIDTH: usize = 60;
/// rustfmt's widest array item that it still fills lines with.
const SHORT_ITEM_WIDTH: usize = 10;
const INDENT: usize = 4;
/// rustfmt's line width.
const MAX_WIDTH: usize = 100;
/// rustfmt's widest struct literal fields on one line.
const STRUCT_WIDTH: usize = 18;

/// The module's text after the header: its documentation, imports, and the
/// five files, with the semantic table `emit::semantic` writes.
const MODULE_BODY: &str = "\
//! The validation tables, converted from SDK 3.5.1's schema data.
//!
//! Every schema type's attributes, children, content models, and text checks,
//! the enumerations, the simple types the generator maps by hand, the part
//! rules, and the semantic constraints, written by `cargo xtask
//! generate-schema`.

use super::schema::types::{
    AnyNamespace, AttributeConstraint, Bounds, Check, ChildKind, ChildPart, CompositeKind,
    Enumeration, PartRule, Particle, SchemaType, StringKind, TypeId, ValueCheck, ValueClass,
    Versions,
};
use super::semantic::constraint::{
    Applications, AttributeName, Constraint, ElementName, Registration,
};
use crate::xml::name::NamespaceId;

include!(\"generated/attributes.rs\");
include!(\"generated/particles.rs\");
include!(\"generated/enums.rs\");
include!(\"generated/parts.rs\");
include!(\"generated/semantic.rs\");
include!(\"generated/types.rs\");
";

/// The documentation of `TRANSITIONAL_ONLY_NAMESPACES`.
const TRANSITIONAL_ONLY_NAMESPACES: &str = "
/// The namespace of every Transitional schema file with no Strict file of its
/// name: in a file that declares Strict, each element and attribute in one
/// exists only in Transitional.
";

/// The lookup by name that ends `types.rs`.
const TYPE_ID: &str = "
/// The id of the type the data names `name`, `w:CT_TblCellMar/w:tblCellMar`.
#[must_use]
pub fn type_id(name: &str) -> Option<TypeId> {
    let index = SCHEMA_TYPES
        .binary_search_by(|row| row.name.cmp(name))
        .ok()?;
    TypeId::try_from(index).ok()
}
";

/// A Rust expression of a table.
pub enum Expr {
    /// Written as is: a path or a literal.
    Atom(String),
    /// `name(arguments)`.
    Call(&'static str, Vec<Self>),
    /// `&inner`.
    Ref(Box<Self>),
    /// `&[items]`.
    Slice(Vec<Self>),
    /// `Path { field: value, ... }`.
    Struct(&'static str, Vec<(&'static str, Self)>),
    /// `(items)`.
    Tuple(Vec<Self>),
}

/// The lists the tables share, each written once under a name, a blank line between two.
#[derive(Default)]
struct Lists {
    attributes: BTreeMap<String, String>,
    attributes_text: String,
    checks: BTreeMap<String, String>,
    checks_text: String,
    particles: BTreeMap<String, String>,
    particles_text: String,
    strict_checks: BTreeMap<String, String>,
    strict_checks_text: String,
}

impl Expr {
    /// The expression on one line, when rustfmt's widths allow it there.
    fn flat(&self) -> Option<String> {
        match self {
            Self::Atom(text) => Some(text.clone()),
            Self::Call(name, arguments) => {
                Some(format!("{name}({})", joined(arguments, CALL_WIDTH)?))
            }
            Self::Ref(inner) => Some(format!("&{}", inner.flat()?)),
            Self::Slice(items) => Some(format!("&[{}]", joined(items, ARRAY_WIDTH)?)),
            Self::Struct(path, fields) => {
                let body = fields
                    .iter()
                    .map(|(name, value)| Some(format!("{name}: {}", value.flat()?)))
                    .collect::<Option<Vec<String>>>()?
                    .join(", ");
                (body.len() <= STRUCT_WIDTH).then(|| format!("{path} {{ {body} }}"))
            }
            Self::Tuple(items) => Some(format!("({})", joined(items, CALL_WIDTH)?)),
        }
    }

    /// Whether rustfmt counts the expression a short array item: a literal or
    /// a path of at most `SHORT_ITEM_WIDTH` characters.
    const fn is_short(&self) -> bool {
        matches!(self, Self::Atom(text) if text.len() <= SHORT_ITEM_WIDTH)
    }

    /// Whether rustfmt lets the expression, a list's only item, open on the list's line.
    fn overflows(&self) -> bool {
        match self {
            Self::Atom(_) => false,
            Self::Call(_, _) | Self::Slice(_) | Self::Struct(_, _) | Self::Tuple(_) => true,
            Self::Ref(inner) => inner.overflows(),
        }
    }

    /// Write the expression starting at column `start` of a line indented by
    /// `indent`, with `tail` columns following it on its last line.
    fn write(&self, indent: usize, start: usize, tail: usize, out: &mut String) {
        if let Some(text) = self.flat()
            && start + text.len() + tail <= MAX_WIDTH
        {
            out.push_str(&text);
            return;
        }
        match self {
            Self::Atom(text) => out.push_str(text),
            Self::Call(name, arguments) => {
                let open = format!("{name}(");
                list(&open, arguments, ")", [indent, start, tail], out);
            }
            Self::Ref(inner) => {
                out.push('&');
                inner.write(indent, start + 1, tail, out);
            }
            Self::Slice(items) if items.len() > 1 && items.iter().all(Self::is_short) => {
                filled(items, indent, out);
            }
            Self::Slice(items) => list("&[", items, "]", [indent, start, tail], out),
            Self::Struct(path, fields) => {
                out.push_str(path);
                out.push_str(" {\n");
                let inner = indent + INDENT;
                for (name, value) in fields {
                    pad(inner, out);
                    out.push_str(name);
                    out.push_str(": ");
                    value.write(inner, inner + name.len() + 2, 1, out);
                    out.push_str(",\n");
                }
                pad(indent, out);
                out.push('}');
            }
            Self::Tuple(items) => list("(", items, ")", [indent, start, tail], out),
        }
    }
}

impl Lists {
    /// The name of an attribute list, written on first use; `&[]` for none.
    fn attributes(&mut self, rows: &[AttributeRow], prefixes: &[&str]) -> Result<String> {
        if rows.is_empty() {
            return Ok("&[]".to_owned());
        }
        let key = format!("{rows:?}");
        if let Some(name) = self.attributes.get(&key) {
            return Ok(name.clone());
        }
        let items = rows
            .iter()
            .map(|row| {
                Ok(Expr::Struct(
                    "AttributeConstraint",
                    vec![
                        ("checks", Expr::Atom(self.checks(&row.checks))),
                        ("class", value_class(&row.class)),
                        ("name", string(&row.name)),
                        (
                            "namespace",
                            option(
                                (!row.prefix.is_empty())
                                    .then(|| namespace_id(prefixes, &row.prefix))
                                    .transpose()?,
                            ),
                        ),
                        (
                            "strict_checks",
                            option(
                                row.strict_checks
                                    .as_deref()
                                    .map(|checks| Expr::Atom(self.strict_checks(checks))),
                            ),
                        ),
                        ("transitional_only", boolean(row.transitional_only)),
                    ],
                ))
            })
            .collect::<Result<Vec<Expr>>>()?;
        let name = format!("ATTRIBUTES_{}", self.attributes.len() + 1);
        statement(
            &format!("const {name}: &[AttributeConstraint] = "),
            &Expr::Slice(items),
            separated(&mut self.attributes_text),
        );
        self.attributes.insert(key, name.clone());
        Ok(name)
    }

    /// The name of a check list, written on first use; `&[]` for none.
    fn checks(&mut self, checks: &[ValueCheck]) -> String {
        if checks.is_empty() {
            return "&[]".to_owned();
        }
        let key = format!("{checks:?}");
        if let Some(name) = self.checks.get(&key) {
            return name.clone();
        }
        let name = format!("CHECKS_{}", self.checks.len() + 1);
        statement(
            &format!("const {name}: &[ValueCheck] = "),
            &Expr::Slice(checks.iter().map(value_check).collect()),
            separated(&mut self.checks_text),
        );
        self.checks.insert(key, name.clone());
        name
    }

    /// The name of a type's content models, written on first use.
    fn particles(
        &mut self,
        particles: &[(Versions, Option<Particle>)],
        type_ids: &BTreeMap<&str, usize>,
    ) -> Result<String> {
        let key = format!("{particles:?}");
        if let Some(name) = self.particles.get(&key) {
            return Ok(name.clone());
        }
        let items = particles
            .iter()
            .map(|(versions, model)| {
                Ok(Expr::Tuple(vec![
                    versions_expr(*versions),
                    option(
                        model
                            .as_ref()
                            .map(|row| particle(row, type_ids))
                            .transpose()?,
                    ),
                ]))
            })
            .collect::<Result<Vec<Expr>>>()?;
        let name = format!("PARTICLES_{}", self.particles.len() + 1);
        statement(
            &format!("const {name}: &[(Versions, Option<Particle>)] = "),
            &Expr::Slice(items),
            separated(&mut self.particles_text),
        );
        self.particles.insert(key, name.clone());
        Ok(name)
    }

    /// The name of a list of Strict checks, written on first use; `&[]` for none.
    fn strict_checks(&mut self, checks: &[ValueCheck]) -> String {
        if checks.is_empty() {
            return "&[]".to_owned();
        }
        let key = format!("{checks:?}");
        if let Some(name) = self.strict_checks.get(&key) {
            return name.clone();
        }
        let name = format!("STRICT_CHECKS_{}", self.strict_checks.len() + 1);
        statement(
            &format!("const {name}: &[ValueCheck] = "),
            &Expr::Slice(checks.iter().map(value_check).collect()),
            separated(&mut self.strict_checks_text),
        );
        self.strict_checks.insert(key, name.clone());
        name
    }
}

fn any_namespace(namespace: &AnyNamespace) -> Expr {
    match namespace {
        AnyNamespace::Any => Expr::Atom("AnyNamespace::Any".to_owned()),
        AnyNamespace::Local => Expr::Atom("AnyNamespace::Local".to_owned()),
        AnyNamespace::Other => Expr::Atom("AnyNamespace::Other".to_owned()),
        AnyNamespace::TargetNamespace => Expr::Atom("AnyNamespace::TargetNamespace".to_owned()),
        AnyNamespace::Uri(uri) => Expr::Call("AnyNamespace::Uri", vec![string(uri)]),
    }
}

pub fn boolean(value: bool) -> Expr {
    Expr::Atom(value.to_string())
}

fn bounds(bounds: &Bounds) -> Expr {
    Expr::Struct(
        "Bounds",
        vec![
            ("max_exclusive", option(bounds.max_exclusive.map(float))),
            ("max_inclusive", option(bounds.max_inclusive.map(float))),
            ("min_exclusive", option(bounds.min_exclusive.map(float))),
            ("min_inclusive", option(bounds.min_inclusive.map(float))),
            ("non_negative", boolean(bounds.non_negative)),
            ("positive", boolean(bounds.positive)),
        ],
    )
}

fn check(row: &Check) -> Expr {
    match row {
        Check::Enum { class } => Expr::Struct("Check::Enum", vec![("class", value_class(class))]),
        Check::List { item } => Expr::Struct(
            "Check::List",
            vec![("item", Expr::Ref(Box::new(check(item))))],
        ),
        Check::Number {
            bounds: limits,
            class,
            list,
        } => Expr::Struct(
            "Check::Number",
            vec![
                ("bounds", bounds(limits)),
                ("class", value_class(class)),
                ("list", boolean(*list)),
            ],
        ),
        Check::OfficeVersion => Expr::Atom("Check::OfficeVersion".to_owned()),
        Check::Required => Expr::Atom("Check::Required".to_owned()),
        Check::String {
            class,
            kind,
            length,
            max_length,
            min_length,
            pattern,
        } => Expr::Struct(
            "Check::String",
            vec![
                ("class", option(class.as_ref().map(value_class))),
                (
                    "kind",
                    Expr::Atom(format!("StringKind::{}", string_kind(*kind))),
                ),
                ("length", option(length.map(integer))),
                ("max_length", option(max_length.map(integer))),
                ("min_length", option(min_length.map(integer))),
                ("pattern", option(pattern.as_deref().map(string))),
            ],
        ),
    }
}

fn child(row: &ChildRow, prefixes: &[&str], type_ids: &BTreeMap<&str, usize>) -> Result<Expr> {
    Ok(Expr::Tuple(vec![
        namespace_id(prefixes, &row.prefix)?,
        string(&row.local),
        type_id(type_ids, &row.type_name)?,
        boolean(row.transitional_only),
    ]))
}

fn child_part(row: &ChildPartRow, parts: &BTreeMap<&str, usize>) -> Expr {
    let kind = match row.kind {
        DataChildKind::DataPart => "ChildKind::DataPart",
        DataChildKind::Ordinary => "ChildKind::Ordinary",
        DataChildKind::SpecialEmbedded => "ChildKind::SpecialEmbedded",
    };
    Expr::Struct(
        "ChildPart",
        vec![
            ("fixed_content", boolean(row.fixed_content)),
            ("kind", Expr::Atom(kind.to_owned())),
            ("many", boolean(row.many)),
            ("name", string(&row.name)),
            (
                "part",
                option(
                    parts
                        .get(row.name.as_str())
                        .map(|index| integer(*index as u64)),
                ),
            ),
            ("required", boolean(row.required)),
        ],
    )
}

const fn composite_kind(kind: CompositeKind) -> &'static str {
    match kind {
        CompositeKind::All => "CompositeKind::All",
        CompositeKind::Choice => "CompositeKind::Choice",
        CompositeKind::Group => "CompositeKind::Group",
        CompositeKind::Sequence => "CompositeKind::Sequence",
    }
}

fn enumeration(row: &EnumerationRow) -> Expr {
    Expr::Struct(
        "Enumeration",
        vec![
            (
                "members",
                Expr::Slice(
                    row.members
                        .iter()
                        .map(|(value, versions, transitional_only)| {
                            Expr::Tuple(vec![
                                string(value),
                                versions_expr(*versions),
                                boolean(*transitional_only),
                            ])
                        })
                        .collect(),
                ),
            ),
            ("simple_type", string(&row.simple_type)),
        ],
    )
}

/// The six files: `generated.rs` and the five it includes, each starting with `header`.
pub(super) fn files(
    header: &str,
    tables: &Tables,
    prefixes: &[&str],
) -> Result<Vec<(&'static str, String)>> {
    let type_ids: BTreeMap<&str, usize> = tables
        .types
        .iter()
        .enumerate()
        .map(|(index, row)| (row.name.as_str(), index))
        .collect();
    if type_ids.len() > usize::from(u16::MAX) + 1 {
        bail!("{} types do not fit TypeId, a u16", type_ids.len());
    }
    let mut lists = Lists::default();
    let types = Expr::Slice(
        tables
            .types
            .iter()
            .map(|row| schema_type(row, &mut lists, prefixes, &type_ids))
            .collect::<Result<Vec<Expr>>>()?,
    );
    let mut types_text = format!(
        "{header}\n/// Every schema type of the data, sorted by name; a type's id is its index.\n"
    );
    statement(
        "pub static SCHEMA_TYPES: &[SchemaType] = ",
        &types,
        &mut types_text,
    );
    types_text.push_str(TRANSITIONAL_ONLY_NAMESPACES);
    statement(
        "pub static TRANSITIONAL_ONLY_NAMESPACES: &[NamespaceId] = ",
        &Expr::Slice(
            tables
                .transitional_only_namespaces
                .iter()
                .map(|(prefix, _)| namespace_id(prefixes, prefix))
                .collect::<Result<Vec<Expr>>>()?,
        ),
        &mut types_text,
    );
    types_text.push_str(TYPE_ID);

    if !lists.strict_checks_text.is_empty() {
        lists.strict_checks_text.push('\n');
    }
    let attributes_text = format!(
        "{header}\n{}\n{}{}",
        lists.checks_text, lists.strict_checks_text, lists.attributes_text
    );
    let particles_text = format!("{header}\n{}", lists.particles_text);

    let mut enums_text =
        format!("{header}\n/// Every enumeration of the data, sorted by simple type.\n");
    statement(
        "pub static ENUMERATIONS: &[Enumeration] = ",
        &Expr::Slice(tables.enumerations.iter().map(enumeration).collect()),
        &mut enums_text,
    );
    enums_text.push_str(
        "\n/// The class SDK 3.5.1's generator reads each of these simple types as,\n/// sorted by simple type.\n",
    );
    statement(
        "pub static SIMPLE_TYPES: &[(&str, ValueClass)] = ",
        &Expr::Slice(
            tables
                .simple_types
                .iter()
                .map(|(simple_type, class)| {
                    Expr::Tuple(vec![string(simple_type), value_class(class)])
                })
                .collect(),
        ),
        &mut enums_text,
    );

    let mut parts_text = format!(
        "{header}\n/// Every part type's rule, sorted by name; a rule's id is its index.\n"
    );
    statement(
        "pub static PART_RULES: &[PartRule] = ",
        &parts(&tables.parts, &type_ids)?,
        &mut parts_text,
    );

    Ok(vec![
        (MODULE, format!("{header}\n{MODULE_BODY}")),
        ("attributes.rs", attributes_text),
        ("enums.rs", enums_text),
        ("particles.rs", particles_text),
        ("parts.rs", parts_text),
        ("types.rs", types_text),
    ])
}

/// An array of short items that does not fit one line, laid out as rustfmt
/// lays it out: the items filled into lines one step past `indent`.
fn filled(items: &[Expr], indent: usize, out: &mut String) {
    out.push_str("&[\n");
    let inner = indent + INDENT;
    let width = MAX_WIDTH - inner;
    let mut line = 0;
    for item in items {
        let text = item.flat().unwrap_or_default();
        let used = text.len() + 1;
        if line > 0 && line + 1 + used > width {
            out.push('\n');
            line = 0;
        }
        if line == 0 {
            pad(inner, out);
        } else {
            out.push(' ');
            line += 1;
        }
        out.push_str(&text);
        out.push(',');
        line += used;
    }
    out.push('\n');
    pad(indent, out);
    out.push(']');
}

/// A whole number of the data as an `f64` literal, its digits grouped by three
/// past five as clippy's `unreadable_literal` asks.
fn float(value: i64) -> Expr {
    let digits = grouped(&value.unsigned_abs().to_string());
    let sign = if value < 0 { "-" } else { "" };
    Expr::Atom(format!("{sign}{digits}.0_f64"))
}

/// A number as an `f64` literal: `f64::INFINITY` or `f64::NEG_INFINITY`, else
/// Rust's shortest form of it, the whole part grouped as `grouped` does.
pub fn real(value: f64) -> Expr {
    if value.is_infinite() {
        let name = if value.is_sign_positive() {
            "f64::INFINITY"
        } else {
            "f64::NEG_INFINITY"
        };
        return Expr::Atom(name.to_owned());
    }
    let text = format!("{value:?}");
    if text.contains('e') {
        return Expr::Atom(format!("{text}_f64"));
    }
    let (sign, digits) = text
        .strip_prefix('-')
        .map_or(("", text.as_str()), |rest| ("-", rest));
    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, "0"));
    Expr::Atom(format!("{sign}{}.{fraction}_f64", grouped(whole)))
}

/// Decimal digits grouped by three from the right when there are more than five.
fn grouped(digits: &str) -> String {
    if digits.len() <= 5 {
        return digits.to_owned();
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push('_');
        }
        out.push(digit);
    }
    out
}

/// An integer literal as clippy accepts it: in hex, grouped by four, where
/// `decimal_literal_representation` finds hex the better form (from 16384,
/// its threshold), else in decimal.
pub fn integer(value: u64) -> Expr {
    if value >= 0x4000 && hex_is_better(&format!("{value:X}")) {
        let hex = format!("{value:X}");
        let mut out = String::from("0x");
        for (index, digit) in hex.chars().enumerate() {
            if index > 0 && (hex.len() - index).is_multiple_of(4) {
                out.push('_');
            }
            out.push(digit);
        }
        return Expr::Atom(out);
    }
    Expr::Atom(grouped(&value.to_string()))
}

/// clippy's `decimal_literal_representation` test on a number's hex digits:
/// a power of two, one less, within sixteen of one, or only `0` and `F` after
/// a leading `7` or `F`.
fn hex_is_better(hex: &str) -> bool {
    let first = &hex[..1];
    let rest = &hex[1..];
    if hex.len() < 4 {
        return (matches!(first, "1" | "2" | "4" | "8") && rest.chars().all(|digit| digit == '0'))
            || (matches!(first, "1" | "3" | "7" | "F") && rest.chars().all(|digit| digit == 'F'));
    }
    let middle = &hex[1..hex.len() - 1];
    (matches!(first, "1" | "2" | "4" | "8") && middle.chars().all(|digit| digit == '0'))
        || (matches!(first, "1" | "3" | "7" | "F") && middle.chars().all(|digit| digit == 'F'))
        || (matches!(first, "7" | "F") && rest.chars().all(|digit| digit == '0' || digit == 'F'))
}

/// The items on one line, when each is flat and together they fit `limit`;
/// a lone item needs only to fit its line.
fn joined(items: &[Expr], limit: usize) -> Option<String> {
    let text = items
        .iter()
        .map(Expr::flat)
        .collect::<Option<Vec<String>>>()?
        .join(", ");
    (items.len() == 1 || text.len() <= limit).then_some(text)
}

/// A list that does not fit one line: a lone item that rustfmt lets open on
/// the list's line written there, else each item on its own line.
/// `position` is the indent, the start column, and the columns after the list.
fn list(open: &str, items: &[Expr], close: &str, position: [usize; 3], out: &mut String) {
    let [indent, start, tail] = position;
    if let [only] = items
        && only.overflows()
    {
        out.push_str(open);
        only.write(indent, start + open.len(), tail + close.len(), out);
        out.push_str(close);
    } else {
        vertical(open, items, close, indent, out);
    }
}

fn namespace_id(prefixes: &[&str], prefix: &str) -> Result<Expr> {
    let index = prefixes
        .binary_search(&prefix)
        .ok()
        .with_context(|| format!("no namespace has the prefix {prefix}"))?;
    Ok(integer(index as u64))
}

pub fn option(value: Option<Expr>) -> Expr {
    value.map_or_else(
        || Expr::Atom("None".to_owned()),
        |inner| Expr::Call("Some", vec![inner]),
    )
}

fn pad(width: usize, out: &mut String) {
    out.extend(iter::repeat_n(' ', width));
}

fn particle(row: &Particle, type_ids: &BTreeMap<&str, usize>) -> Result<Expr> {
    Ok(match row {
        Particle::Any {
            max,
            min,
            namespace,
        } => Expr::Struct(
            "Particle::Any",
            vec![
                ("max", integer(u64::from(*max))),
                ("min", integer(u64::from(*min))),
                ("namespace", any_namespace(namespace)),
            ],
        ),
        Particle::Composite {
            items,
            kind,
            max,
            min,
        } => Expr::Struct(
            "Particle::Composite",
            vec![
                (
                    "items",
                    Expr::Slice(
                        items
                            .iter()
                            .map(|item| particle(item, type_ids))
                            .collect::<Result<Vec<Expr>>>()?,
                    ),
                ),
                ("kind", Expr::Atom(composite_kind(*kind).to_owned())),
                ("max", integer(u64::from(*max))),
                ("min", integer(u64::from(*min))),
            ],
        ),
        Particle::Element {
            max,
            min,
            type_name,
        } => Expr::Struct(
            "Particle::Element",
            vec![
                ("max", integer(u64::from(*max))),
                ("min", integer(u64::from(*min))),
                ("type_id", type_id(type_ids, type_name)?),
            ],
        ),
    })
}

/// The part rules; a child names a rule by its index, or none for a data part reference.
fn parts(rows: &[PartRow], type_ids: &BTreeMap<&str, usize>) -> Result<Expr> {
    let ids: BTreeMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| (row.name.as_str(), index))
        .collect();
    if ids.len() > usize::from(u8::MAX) + 1 {
        bail!("{} part rules do not fit PartRuleId, a u8", ids.len());
    }
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        for part_child in &row.children {
            if part_child.kind != DataChildKind::DataPart
                && !ids.contains_key(part_child.name.as_str())
            {
                bail!(
                    "the part {} relates to {}, which has no rule",
                    row.name,
                    part_child.name
                );
            }
        }
        items.push(Expr::Struct(
            "PartRule",
            vec![
                (
                    "children",
                    Expr::Slice(
                        row.children
                            .iter()
                            .map(|part_child| child_part(part_child, &ids))
                            .collect(),
                    ),
                ),
                (
                    "content_type",
                    option(row.content_type.as_deref().map(string)),
                ),
                ("name", string(&row.name)),
                (
                    "relationship_type",
                    option(row.relationship_type.as_deref().map(string)),
                ),
                (
                    "root",
                    option(
                        row.root
                            .as_deref()
                            .map(|root| type_id(type_ids, root))
                            .transpose()?,
                    ),
                ),
                ("versions", versions_expr(row.versions)),
            ],
        ));
    }
    Ok(Expr::Slice(items))
}

/// `out`, after a blank line when it already holds a statement.
fn separated(out: &mut String) -> &mut String {
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn schema_type(
    row: &TypeRow,
    lists: &mut Lists,
    prefixes: &[&str],
    type_ids: &BTreeMap<&str, usize>,
) -> Result<Expr> {
    Ok(Expr::Struct(
        "SchemaType",
        vec![
            (
                "attributes",
                Expr::Atom(lists.attributes(&row.attributes, prefixes)?),
            ),
            (
                "children",
                Expr::Slice(
                    row.children
                        .iter()
                        .map(|declared| child(declared, prefixes, type_ids))
                        .collect::<Result<Vec<Expr>>>()?,
                ),
            ),
            ("name", string(&row.name)),
            (
                "particles",
                Expr::Atom(lists.particles(&row.particles, type_ids)?),
            ),
            ("text_checks", Expr::Atom(lists.checks(&row.text_checks))),
            ("versions", versions_expr(row.versions)),
        ],
    ))
}

/// `start` and the expression, laid out from column `start.len()`, and `;`.
pub fn statement(start: &str, expr: &Expr, out: &mut String) {
    out.push_str(start);
    expr.write(0, start.len(), 1, out);
    out.push_str(";\n");
}

/// A string literal, every character outside printable ASCII escaped.
pub fn string(text: &str) -> Expr {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ' '..='~' => out.push(character),
            _ => {
                let _: fmt::Result = write!(out, "\\u{{{:x}}}", u32::from(character));
            }
        }
    }
    out.push('"');
    Expr::Atom(out)
}

const fn string_kind(kind: StringKind) -> &'static str {
    match kind {
        StringKind::Id => "Id",
        StringKind::NcName => "NcName",
        StringKind::Plain => "Plain",
        StringKind::QName => "QName",
        StringKind::Token => "Token",
        StringKind::Uri => "Uri",
    }
}

fn type_id(type_ids: &BTreeMap<&str, usize>, name: &str) -> Result<Expr> {
    let index = type_ids
        .get(name)
        .with_context(|| format!("no type is {name}"))?;
    Ok(integer(*index as u64))
}

fn value_check(row: &ValueCheck) -> Expr {
    Expr::Struct(
        "ValueCheck",
        vec![
            ("check", check(&row.check)),
            (
                "union",
                option(row.union.map(|union| integer(u64::from(union)))),
            ),
            ("versions", versions_expr(row.versions)),
        ],
    )
}

fn value_class(class: &ValueClass) -> Expr {
    let name = match class {
        ValueClass::Enum(simple_type) => {
            return Expr::Call("ValueClass::Enum", vec![string(simple_type)]);
        }
        ValueClass::List(item) => {
            return Expr::Call(
                "ValueClass::List",
                vec![Expr::Ref(Box::new(value_class(item)))],
            );
        }
        ValueClass::Base64Binary => "Base64Binary",
        ValueClass::Boolean => "Boolean",
        ValueClass::Byte => "Byte",
        ValueClass::DateTime => "DateTime",
        ValueClass::Decimal => "Decimal",
        ValueClass::Double => "Double",
        ValueClass::HexBinary => "HexBinary",
        ValueClass::Int16 => "Int16",
        ValueClass::Int32 => "Int32",
        ValueClass::Int64 => "Int64",
        ValueClass::Integer => "Integer",
        ValueClass::OnOff => "OnOff",
        ValueClass::SByte => "SByte",
        ValueClass::Single => "Single",
        ValueClass::String => "String",
        ValueClass::TrueFalse => "TrueFalse",
        ValueClass::TrueFalseBlank => "TrueFalseBlank",
        ValueClass::UInt16 => "UInt16",
        ValueClass::UInt32 => "UInt32",
        ValueClass::UInt64 => "UInt64",
    };
    Expr::Atom(format!("ValueClass::{name}"))
}

/// `Versions(0b11_1111)`: one bit per target, Office 2010 the lowest.
pub fn versions_expr(versions: Versions) -> Expr {
    Expr::Call(
        "Versions",
        vec![Expr::Atom(format!(
            "0b{:02b}_{:04b}",
            versions.0 >> 4,
            versions.0 & 0b1111
        ))],
    )
}

/// Each item on its own line, indented one step past `indent`, between `open` and `close`.
fn vertical(open: &str, items: &[Expr], close: &str, indent: usize, out: &mut String) {
    out.push_str(open);
    out.push('\n');
    let inner = indent + INDENT;
    for item in items {
        pad(inner, out);
        item.write(inner, inner, 1, out);
        out.push_str(",\n");
    }
    pad(indent, out);
    out.push_str(close);
}
