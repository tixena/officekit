//! The text of the conformance classes' files: one module per namespace with
//! every node's impl, the file declaring those modules, and the tables of each
//! class's write.

use alloc::collections::BTreeSet;
use core::fmt::Write as _;

use super::back::{Back, BackRule};
use super::correspondences::{Bits, Codes, Spelling};
use super::rules::{AttributeRule, ChildRule, Rules, Tag};
use crate::generate_schema::emit::markup::render::layout::{self, Expr};
use crate::generate_schema::emit::markup::render::namespace_constant;

/// One conformance class, as its files name it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Class {
    Strict,
    Transitional,
}

/// One node's impl: its struct, its reader and writer function, its tag, and
/// the `conformance` attribute of a part's root.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Impl {
    pub conformance: Option<Tag>,
    pub function: String,
    pub name: String,
    pub tag: Tag,
}

impl Class {
    /// The module and trait names: `strict` and `Strict`.
    const fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Strict => ("strict", "Strict"),
            Self::Transitional => ("transitional", "Transitional"),
        }
    }

    /// The path, from a namespace module, of the module that holds the class's
    /// trait, and where the text says `include!` brings the namespace modules.
    const fn root(self) -> (&'static str, &'static str) {
        match self {
            Self::Strict => ("crate", "the crate root"),
            Self::Transitional => ("super", "the `transitional` module"),
        }
    }
}

/// `<module>.rs` of `officekit-strict` or of `officekit::transitional`: every node's impl.
#[must_use]
pub fn module(
    header: &str,
    class: Class,
    module: &str,
    prefixes: &[String],
    impls: &[Impl],
) -> String {
    let (_, upper) = class.names();
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "//! The `{upper}` impl of every node of `officekit_markup::{module}`.\n//!\n\
         //! Each node reads with the reader both classes share and writes with the\n\
         //! shared writer, whose element the {upper} write then {}.\n",
        if class == Class::Strict {
            "checks and converts"
        } else {
            "checks"
        }
    );
    let _ = writeln!(out, "use {}::{upper};", class.root().0);
    let flat = format!("use officekit_markup::{module}::{{self, read, write}};");
    // rustfmt breaks a braced import whose line, semicolon included, reaches the width.
    let own = if flat.len() >= layout::MAX_WIDTH {
        format!("use officekit_markup::{module}::{{\n    self, read, write,\n}};")
    } else {
        flat
    };
    let shared = "use officekit_markup::class_rules;".to_owned();
    let (first, second) = if module < "class_rules" {
        (own, shared)
    } else {
        (shared, own)
    };
    let _ = writeln!(out, "{first}");
    let _ = writeln!(out, "{second}");
    let _ = writeln!(out, "use officekit_xml::element::Element;");
    let _ = writeln!(out, "use officekit_xml::error::{{ReadError, WriteError}};");
    let _ = writeln!(out, "use officekit_xml::name::NamespaceId;");
    let _ = writeln!(out, "use officekit_xml::write::Scope;");
    let namespaces: BTreeSet<u8> = impls
        .iter()
        .flat_map(|found| {
            found
                .tag
                .0
                .into_iter()
                .chain(found.conformance.iter().filter_map(|tag| tag.0))
        })
        .collect();
    out.push('\n');
    out.push_str(&constants(prefixes, &namespaces));
    for found in impls {
        out.push('\n');
        out.push_str(&impl_text(class, module, prefixes, found));
    }
    out
}

/// `modules.rs`: the `pub mod` line of each module.
#[must_use]
pub fn modules(header: &str, class: Class, names: &[String]) -> String {
    let (lower, _) = class.names();
    let (_, place) = class.root();
    let path = if class == Class::Strict {
        format!("officekit_{lower}")
    } else {
        format!("officekit::{lower}")
    };
    let mut out = String::from(header);
    let _ = writeln!(
        out,
        "// The namespace modules, `{path}::<namespace>`, brought into\n\
         // {place} with `include!` ahead of its imports.\n"
    );
    for name in names {
        let _ = writeln!(out, "pub mod {name};");
    }
    out
}

/// `rules.rs` of `officekit-strict`: the tables of the Strict walk.
#[must_use]
pub fn strict_rules(header: &str, prefixes: &[String], rules: &Rules) -> String {
    let mut out = String::from(header);
    out.push_str(
        "// The tables of the Strict walk, generated from the comparison of the two\n\
         // schema sets and `strict_correspondences.toml`, brought into the crate root\n\
         // with `include!`.\n",
    );
    let mut namespaces: BTreeSet<u8> = BTreeSet::new();
    for (owner, item) in rules.attributes.keys().chain(rules.children.keys()) {
        namespaces.extend(owner.0.into_iter().chain(item.0));
    }
    for (parent, owner, item) in rules.contextual.keys() {
        namespaces.extend(parent.0.into_iter().chain(owner.0).chain(item.0));
    }
    namespaces.extend(rules.elements.iter().filter_map(|tag| tag.0));
    namespaces.extend(rules.texts.keys().filter_map(|tag| tag.0));
    out.push('\n');
    out.push_str(&constants(prefixes, &namespaces));
    attributes_static(prefixes, rules, &mut out);
    codes_statics(rules, &mut out);
    children_static(prefixes, rules, &mut out);
    contextual_static(prefixes, rules, &mut out);
    elements_static(prefixes, rules, &mut out);
    namespaces_static(rules, &mut out);
    spellings_statics(rules, &mut out);
    texts_static(prefixes, rules, &mut out);
    out
}

/// `spellings.rs` of `officekit::transitional`: the tables of a save from Strict to
/// Transitional, a module of its own.
#[must_use]
pub fn transitional_spellings(header: &str, prefixes: &[String], back: &Back) -> String {
    let mut out = String::from(header);
    out.push_str(
        "//! The tables of a save from Strict to Transitional, generated from the Strict walk's\n\
         //! tables reversed: what each Strict spelling is in Transitional.\n",
    );
    let mut namespaces: BTreeSet<u8> = BTreeSet::new();
    for (owner, item) in back
        .attributes
        .keys()
        .chain(back.children.keys())
        .chain(back.child_attributes.keys())
    {
        namespaces.extend(owner.0.into_iter().chain(item.0));
    }
    for attribute in back.child_attributes.values() {
        namespaces.extend(attribute.0);
    }
    for (parent, owner, item) in back.contextual.keys() {
        namespaces.extend(parent.0.into_iter().chain(owner.0).chain(item.0));
    }
    namespaces.extend(back.texts.keys().filter_map(|found| found.0));
    out.push_str(
        "\nuse officekit_markup::Tag;\nuse officekit_xml::name::NamespaceId;\n\nuse super::{BackRule, Bits};\n\n",
    );
    out.push_str(&constants(prefixes, &namespaces));
    back_attributes_static(prefixes, back, &mut out);
    back_children_statics(prefixes, back, &mut out);
    for (name, pairs) in &back.codes {
        out.push('\n');
        out.push_str("/// Each name with the code Transitional writes for it; sorted.\n");
        layout::statement(
            &format!("pub static {}: &[(&str, &str)] = ", codes_static(name)),
            &pairs_expr(pairs),
            &mut out,
        );
    }
    back_contextual_static(prefixes, back, &mut out);
    let texts: Vec<Expr> = back
        .texts
        .iter()
        .map(|(owner, per_percent)| {
            Expr::Tuple(vec![
                tag(prefixes, owner),
                Expr::Atom(layout::integer(i128::from(*per_percent))),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// The elements whose text is a percentage, each with how many of the bare number make\n\
         /// one percent; sorted.\n",
    );
    layout::statement(
        "pub static TEXTS: &[(Tag, u32)] = ",
        &Expr::Slice(texts),
        &mut out,
    );
    out
}

/// `ATTRIBUTES` of the way back: each attribute's rules, in the order they apply.
fn back_attributes_static(prefixes: &[String], back: &Back, out: &mut String) {
    let rows: Vec<Expr> = back
        .attributes
        .iter()
        .flat_map(|((owner, attribute), rules)| {
            rules.iter().map(|rule| {
                Expr::Tuple(vec![
                    tag(prefixes, owner),
                    tag(prefixes, attribute),
                    back_rule(rule),
                ])
            })
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// What the save does with an attribute, by element and attribute, an attribute's rules\n\
         /// in the order they apply; sorted.\n",
    );
    layout::statement(
        "pub static ATTRIBUTES: &[(Tag, Tag, BackRule)] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// `CHILD_ATTRIBUTES` and `CHILDREN` of the way back.
fn back_children_statics(prefixes: &[String], back: &Back, out: &mut String) {
    let moved: Vec<Expr> = back
        .child_attributes
        .iter()
        .map(|((owner, child), attribute)| {
            Expr::Tuple(vec![
                tag(prefixes, owner),
                tag(prefixes, child),
                tag(prefixes, attribute),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// The children whose text goes back into an attribute, by element and child; sorted.\n",
    );
    layout::statement(
        "pub static CHILD_ATTRIBUTES: &[(Tag, Tag, Tag)] = ",
        &Expr::Slice(moved),
        out,
    );
    let renamed: Vec<Expr> = back
        .children
        .iter()
        .map(|((parent, child), to)| {
            Expr::Tuple(vec![
                tag(prefixes, parent),
                tag(prefixes, child),
                Expr::Atom(layout::string(to)),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str("/// Transitional's name for a child, by parent and child; sorted.\n");
    layout::statement(
        "pub static CHILDREN: &[(Tag, Tag, &str)] = ",
        &Expr::Slice(renamed),
        out,
    );
}

/// `CONTEXTUAL` of the way back: the rules that depend on the element's parent.
fn back_contextual_static(prefixes: &[String], back: &Back, out: &mut String) {
    let rows: Vec<Expr> = back
        .contextual
        .iter()
        .flat_map(|((parent, owner, attribute), rules)| {
            rules.iter().map(|rule| {
                Expr::Tuple(vec![
                    tag(prefixes, parent),
                    tag(prefixes, owner),
                    tag(prefixes, attribute),
                    back_rule(rule),
                ])
            })
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// What the save does with an attribute of an element whose types disagree, by\n\
         /// parent, element, and attribute; sorted.\n",
    );
    layout::statement(
        "pub static CONTEXTUAL: &[(Tag, Tag, Tag, BackRule)] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// `rules.rs` of `officekit::transitional`: the values a Transitional write refuses.
#[must_use]
pub fn transitional_rules(header: &str, prefixes: &[String], rules: &Rules) -> String {
    let mut out = String::from(header);
    out.push_str(
        "// The table of the Transitional check, generated from the comparison of the\n\
         // two schema sets, brought into the `transitional` module with `include!`.\n",
    );
    let namespaces: BTreeSet<u8> = rules
        .strict_only
        .keys()
        .flat_map(|(owner, attribute)| owner.0.into_iter().chain(attribute.0))
        .collect();
    out.push('\n');
    out.push_str(&constants(prefixes, &namespaces));
    let rows: Vec<Expr> = rules
        .strict_only
        .iter()
        .map(|((owner, attribute), (values, label))| {
            Expr::Tuple(vec![
                tag(prefixes, owner),
                tag(prefixes, attribute),
                strings(values),
                Expr::Atom(layout::string(label)),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// The values only Strict has, by element and attribute, with the name\n\
         /// a refusal gives; sorted.\n",
    );
    layout::statement(
        "static STRICT_ONLY: &[(Tag, Tag, &[&str], &str)] = ",
        &Expr::Slice(rows),
        &mut out,
    );
    out
}

/// `ATTRIBUTES`: each attribute rule by element and attribute.
fn attributes_static(prefixes: &[String], rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .attributes
        .iter()
        .map(|((owner, attribute), rule)| {
            Expr::Tuple(vec![
                tag(prefixes, owner),
                tag(prefixes, attribute),
                attribute_rule(rules, rule),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// What a Strict write does with an attribute, by element and attribute;\n\
         /// sorted.\n",
    );
    layout::statement(
        "static ATTRIBUTES: &[(Tag, Tag, AttributeRule)] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// `CHILDREN`: each child rule by parent and child.
fn children_static(prefixes: &[String], rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .children
        .iter()
        .map(|((owner, child), rule)| {
            let rule_expr = match rule {
                ChildRule::Refuse(label) => {
                    Expr::call("ChildRule::Refuse", vec![Expr::Atom(layout::string(label))])
                }
                ChildRule::Rename { before, to } => Expr::Struct(
                    "ChildRule::Rename".to_owned(),
                    vec![
                        ("before".to_owned(), strings(before)),
                        ("to".to_owned(), Expr::Atom(layout::string(to))),
                    ],
                ),
            };
            Expr::Tuple(vec![tag(prefixes, owner), tag(prefixes, child), rule_expr])
        })
        .collect();
    out.push('\n');
    out.push_str("/// What a Strict write does with a child, by parent and child; sorted.\n");
    layout::statement(
        "static CHILDREN: &[(Tag, Tag, ChildRule)] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// `CONTEXTUAL`: the attribute rules that depend on the element's parent.
fn contextual_static(prefixes: &[String], rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .contextual
        .iter()
        .map(|((parent, owner, attribute), rule)| {
            Expr::Tuple(vec![
                tag(prefixes, parent),
                tag(prefixes, owner),
                tag(prefixes, attribute),
                attribute_rule(rules, rule),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// What a Strict write does with an attribute of an element whose types\n\
         /// disagree, by parent, element, and attribute; sorted.\n",
    );
    layout::statement(
        "static CONTEXTUAL: &[(Tag, Tag, Tag, AttributeRule)] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// One static per code table.
fn codes_statics(rules: &Rules, out: &mut String) {
    for (name, codes) in &rules.codes {
        let (kind, rows): (&str, Vec<Expr>) = match codes {
            Codes::Byte(rows) => (
                "u8",
                rows.iter()
                    .map(|(code, text)| {
                        Expr::Tuple(vec![
                            Expr::Atom(format!("0x{code:02X}")),
                            Expr::Atom(layout::string(text)),
                        ])
                    })
                    .collect(),
            ),
            Codes::Number(rows) => (
                "u32",
                rows.iter()
                    .map(|(code, text)| {
                        Expr::Tuple(vec![
                            Expr::Atom(layout::integer(i128::from(*code))),
                            Expr::Atom(layout::string(text)),
                        ])
                    })
                    .collect(),
            ),
        };
        out.push('\n');
        out.push_str("/// Each code with the name Strict writes for it; sorted.\n");
        layout::statement(
            &format!("static {}: &[({kind}, &str)] = ", codes_static(name)),
            &Expr::Slice(rows),
            out,
        );
    }
}

/// `ELEMENTS`: the tags a Strict write refuses wherever they stand.
fn elements_static(prefixes: &[String], rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .elements
        .iter()
        .map(|element| tag(prefixes, element))
        .collect();
    out.push('\n');
    out.push_str(
        "/// The elements of the Transitional schemas no Strict schema declares;\n\
         /// sorted.\n",
    );
    layout::statement("static ELEMENTS: &[Tag] = ", &Expr::Slice(rows), out);
}

/// `REFUSED_NAMESPACES`: the namespaces with no Strict twin.
fn namespaces_static(rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .namespaces
        .iter()
        .map(|(id, locals)| {
            Expr::Tuple(vec![
                Expr::Atom(layout::integer(i128::from(*id))),
                strings(locals),
            ])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// The namespaces with no Strict twin, each with its elements' local\n\
         /// names; sorted.\n",
    );
    layout::statement(
        "static REFUSED_NAMESPACES: &[(NamespaceId, &[&str])] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// One static per list of spellings, by its name.
fn spellings_statics(rules: &Rules, out: &mut String) {
    let mut spellings: Vec<(&String, &Vec<Spelling>)> = rules
        .spellings
        .iter()
        .map(|(list, name)| (name, list))
        .collect();
    spellings.sort();
    for (name, list) in spellings {
        let items: Vec<Expr> = list.iter().map(spelling).collect();
        out.push('\n');
        layout::statement(
            &format!("static {name}: &[Spelling] = "),
            &Expr::Slice(items),
            out,
        );
    }
}

/// `TEXTS`: what Strict writes for the text of each extension element it names.
fn texts_static(prefixes: &[String], rules: &Rules, out: &mut String) {
    let rows: Vec<Expr> = rules
        .texts
        .iter()
        .map(|(owner, spellings)| {
            let name = rules.spellings.get(spellings).cloned().unwrap_or_default();
            Expr::Tuple(vec![tag(prefixes, owner), Expr::Atom(name)])
        })
        .collect();
    out.push('\n');
    out.push_str(
        "/// What a Strict write does with the text of an element, by element;\n\
         /// sorted.\n",
    );
    layout::statement(
        "static TEXTS: &[(Tag, &[Spelling])] = ",
        &Expr::Slice(rows),
        out,
    );
}

/// An attribute rule's expression.
fn attribute_rule(rules: &Rules, rule: &AttributeRule) -> Expr {
    let string = |text: &str| Expr::Atom(layout::string(text));
    match rule {
        AttributeRule::CharacterSet { codes, label, to } => {
            let kind = match rules.codes.get(codes) {
                Some(Codes::Byte(_)) | None => "Codes::Byte",
                Some(Codes::Number(_)) => "Codes::Number",
            };
            Expr::Struct(
                "AttributeRule::CharacterSet".to_owned(),
                vec![
                    (
                        "codes".to_owned(),
                        Expr::call(kind, vec![Expr::Atom(codes_static(codes))]),
                    ),
                    ("label".to_owned(), string(label)),
                    ("to".to_owned(), string(to)),
                ],
            )
        }
        AttributeRule::Child(child) => Expr::call("AttributeRule::Child", vec![string(child)]),
        AttributeRule::Iso { label, to } => Expr::Struct(
            "AttributeRule::Iso".to_owned(),
            vec![
                ("label".to_owned(), string(label)),
                ("to".to_owned(), string(to)),
            ],
        ),
        AttributeRule::Measure {
            per_percent,
            per_point,
            percent,
            unit,
        } => Expr::Struct(
            "AttributeRule::Measure".to_owned(),
            vec![
                (
                    "per_percent".to_owned(),
                    Expr::Atom(layout::integer(i128::from(*per_percent))),
                ),
                (
                    "per_point".to_owned(),
                    Expr::Atom(layout::integer(i128::from(*per_point))),
                ),
                ("percent".to_owned(), string(percent)),
                ("unit".to_owned(), string(unit)),
            ],
        ),
        AttributeRule::Refuse(label) => Expr::call("AttributeRule::Refuse", vec![string(label)]),
        AttributeRule::Rename(to) => Expr::call("AttributeRule::Rename", vec![string(to)]),
        AttributeRule::Unpack { bits, label } => Expr::Struct(
            "AttributeRule::Unpack".to_owned(),
            vec![
                ("bits".to_owned(), bits_expr(bits)),
                ("label".to_owned(), string(label)),
            ],
        ),
        AttributeRule::Values(list) => Expr::call(
            "AttributeRule::Values",
            vec![Expr::Atom(
                rules.spellings.get(list).cloned().unwrap_or_default(),
            )],
        ),
    }
}

/// A way-back rule's expression.
fn back_rule(rule: &BackRule) -> Expr {
    let string = |text: &str| Expr::Atom(layout::string(text));
    let number = |value: u32| Expr::Atom(layout::integer(i128::from(value)));
    match rule {
        BackRule::Code { codes, to } => Expr::Struct(
            "BackRule::Code".to_owned(),
            vec![
                ("codes".to_owned(), Expr::Atom(codes_static(codes))),
                ("to".to_owned(), string(to)),
            ],
        ),
        BackRule::Measure {
            per_percent,
            per_point,
            percent,
            unit,
        } => Expr::Struct(
            "BackRule::Measure".to_owned(),
            vec![
                ("per_percent".to_owned(), number(*per_percent)),
                ("per_point".to_owned(), number(*per_point)),
                ("percent".to_owned(), string(percent)),
                ("unit".to_owned(), string(unit)),
            ],
        ),
        BackRule::Pack(bits) => Expr::call("BackRule::Pack", vec![bits_expr(bits)]),
        BackRule::Percent(per_percent) => {
            Expr::call("BackRule::Percent", vec![number(*per_percent)])
        }
        BackRule::Rename(to) => Expr::call("BackRule::Rename", vec![string(to)]),
        BackRule::Serial(to) => Expr::call("BackRule::Serial", vec![string(to)]),
        BackRule::Twips => Expr::atom("BackRule::Twips"),
        BackRule::Values(pairs) => Expr::call("BackRule::Values", vec![pairs_expr(pairs)]),
    }
}

/// A packed attribute's bits' expression.
fn bits_expr(bits: &Bits) -> Expr {
    match bits {
        Bits::Characters(names) => Expr::call("Bits::Characters", vec![strings(names)]),
        Bits::Hex(names) => Expr::call(
            "Bits::Hex",
            vec![Expr::Slice(
                names
                    .iter()
                    .map(|(bit, name)| {
                        Expr::Tuple(vec![
                            Expr::Atom(format!("0x{bit:04X}")),
                            Expr::Atom(layout::string(name)),
                        ])
                    })
                    .collect(),
            )],
        ),
    }
}

/// The name of a code table's static.
fn codes_static(name: &str) -> String {
    format!("{name}S")
}

/// The `NS_` constants of `namespaces`.
fn constants(prefixes: &[String], namespaces: &BTreeSet<u8>) -> String {
    let mut out = String::new();
    for id in namespaces {
        let prefix = prefixes.get(usize::from(*id)).map_or("", String::as_str);
        let _ = writeln!(
            out,
            "const {}: Option<NamespaceId> = Some({id});",
            namespace_constant(prefix)
        );
    }
    out
}

/// One node's impl of the class's trait.
fn impl_text(class: Class, module: &str, prefixes: &[String], found: &Impl) -> String {
    let (lower, upper) = class.names();
    let (root, _) = class.root();
    let mut out = layout::impl_header(upper, &format!("{module}::{}", found.name));
    let _ = writeln!(
        out,
        "    fn from_{lower}(element: &Element) -> Result<Self, ReadError> {{"
    );
    layout::line(
        "        ",
        &Expr::call(
            "class_rules::read",
            vec![
                Expr::atom("element"),
                tag(prefixes, &found.tag),
                Expr::Atom(format!("read::{}", found.function)),
            ],
        ),
        "",
        &mut out,
    );
    out.push_str("    }\n\n");
    let _ = writeln!(
        out,
        "    fn to_{lower}(&self) -> Result<Element, WriteError> {{"
    );
    let written = Expr::call(
        &format!("write::{}", found.function),
        vec![Expr::atom("self"), Expr::atom("&mut Scope::default()")],
    );
    let call = found.conformance.as_ref().map_or_else(
        || Expr::call(&format!("{root}::write"), vec![written.clone()]),
        |conformance| {
            Expr::call(
                &format!("{root}::write_root"),
                vec![written.clone(), tag(prefixes, conformance)],
            )
        },
    );
    layout::line("        ", &call, "", &mut out);
    out.push_str("    }\n}\n");
    out
}

/// A spelling's expression.
fn spelling(item: &Spelling) -> Expr {
    match item {
        Spelling::Map(pairs) => Expr::call(
            "Spelling::Map",
            vec![Expr::Slice(
                pairs
                    .iter()
                    .map(|(from, to)| {
                        Expr::Tuple(vec![
                            Expr::Atom(layout::string(from)),
                            Expr::Atom(layout::string(to)),
                        ])
                    })
                    .collect(),
            )],
        ),
        Spelling::Percent {
            decimals,
            per_percent,
        } => Expr::Struct(
            "Spelling::Percent".to_owned(),
            vec![
                (
                    "decimals".to_owned(),
                    decimals.map_or_else(
                        || Expr::atom("None"),
                        |places| {
                            Expr::call(
                                "Some",
                                vec![Expr::Atom(layout::integer(i128::from(places)))],
                            )
                        },
                    ),
                ),
                (
                    "per_percent".to_owned(),
                    Expr::Atom(layout::integer(i128::from(*per_percent))),
                ),
            ],
        ),
    }
}

/// A slice of pairs of string literals.
fn pairs_expr(pairs: &[(String, String)]) -> Expr {
    Expr::Slice(
        pairs
            .iter()
            .map(|(from, to)| {
                Expr::Tuple(vec![
                    Expr::Atom(layout::string(from)),
                    Expr::Atom(layout::string(to)),
                ])
            })
            .collect(),
    )
}

/// A slice of string literals.
fn strings(items: &[String]) -> Expr {
    Expr::Slice(
        items
            .iter()
            .map(|item| Expr::Atom(layout::string(item)))
            .collect(),
    )
}

/// A tag literal, `(NS_W, "p")`.
fn tag(prefixes: &[String], found: &Tag) -> Expr {
    let namespace = found.0.map_or_else(
        || "None".to_owned(),
        |id| namespace_constant(prefixes.get(usize::from(id)).map_or("", String::as_str)),
    );
    Expr::Tuple(vec![
        Expr::Atom(namespace),
        Expr::Atom(layout::string(&found.1)),
    ])
}
