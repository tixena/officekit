//! The case mappings of a language, from the Unicode Character Database.
//!
//! The rows of `SpecialCasing.txt` that name a language, with the combining classes and the
//! `Soft_Dotted` property their contexts read, written to [`PATH`].

use core::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::markup::render::layout::{self, Expr};
use super::namespaces::{self, UNICODE_NOTICE};
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::unicode_data::{self, CasingContext, LanguageCasing};

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/docx/numbering/casing/special_casing.rs";

/// What the file holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tables {
    /// The language rows, sorted by language and character.
    pub casings: Vec<LanguageCasing>,
    /// The ranges of a combining class other than 0, with the class.
    pub classes: Vec<(char, char, u8)>,
    /// The ranges of `Soft_Dotted` characters.
    pub soft_dotted: Vec<(char, char)>,
}

/// `casings` sorted by language and character, keeping the text's order within each.
///
/// # Errors
///
/// No row at all, or two rows of one language, character and contexts.
pub fn sorted(mut casings: Vec<LanguageCasing>) -> Result<Vec<LanguageCasing>> {
    if casings.is_empty() {
        bail!("SpecialCasing.txt gives no language a mapping");
    }
    casings.sort_by(|left, right| {
        (&left.language, left.character).cmp(&(&right.language, right.character))
    });
    for pair in casings.windows(2) {
        if let [left, right] = pair
            && left.language == right.language
            && left.character == right.character
            && left.contexts == right.contexts
        {
            bail!(
                "SpecialCasing.txt gives {} U+{:04X} twice",
                left.language,
                u32::from(left.character)
            );
        }
    }
    Ok(casings)
}

/// Read the three files and write the table; the count of language rows written.
///
/// # Errors
///
/// An input that cannot be read or does not parse, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let read = |path: &Path| {
        ready_now(system.read_to_string(path))
            .with_context(|| format!("cannot read {}", path.display()))
    };
    let tables = Tables {
        casings: sorted(unicode_data::language_casings(&read(
            &inputs.unicode_special_casing,
        )?)?)?,
        classes: unicode_data::combining_classes(&read(&inputs.unicode_data)?)?,
        soft_dotted: unicode_data::property(&read(&inputs.unicode_prop_list)?, "Soft_Dotted")?,
    };
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &[
            inputs.unicode_prop_list.clone(),
            inputs.unicode_special_casing.clone(),
            inputs.unicode_data.clone(),
        ],
        &[],
    )?;
    let out = Path::new(PATH);
    ready_now(system.write(out, render(&header, &tables).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(tables.casings.len())
}

/// The table's source: `header`, then the three statics.
#[must_use = "this only returns the value"]
pub fn render(header: &str, tables: &Tables) -> String {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n//! The Unicode Character Database's case mappings of a \
         language, and the properties their contexts\n//! read.\n\n\
         use super::{{Casing, Context}};\n\n/// Each mapping \
         `SpecialCasing.txt` gives a language, sorted by language and character.\n\
         pub(super) static CASE_BY_LANGUAGE: [Casing; {}] = [\n",
        tables.casings.len()
    );
    for casing in &tables.casings {
        layout::line("    ", &casing_expr(casing), ",", &mut text);
    }
    let _: fmt::Result = write!(
        text,
        "];\n\n/// Each range of characters whose canonical combining class is not 0, and the \
         class, sorted.\npub(super) static COMBINING_CLASSES: [(char, char, u8); {}] = [\n",
        tables.classes.len()
    );
    for (first, last, class) in &tables.classes {
        let _: fmt::Result = writeln!(
            text,
            "    ({}, {}, {class}),",
            character(*first),
            character(*last)
        );
    }
    let _: fmt::Result = write!(
        text,
        "];\n\n/// Each range of characters with the property `Soft_Dotted`, sorted.\n\
         pub(super) static SOFT_DOTTED: [(char, char); {}] = [\n",
        tables.soft_dotted.len()
    );
    for (first, last) in &tables.soft_dotted {
        let _: fmt::Result = writeln!(text, "    ({}, {}),", character(*first), character(*last));
    }
    text.push_str("];\n");
    text
}

/// One entry of `CASE_BY_LANGUAGE`.
fn casing_expr(casing: &LanguageCasing) -> Expr {
    let contexts = casing
        .contexts
        .iter()
        .map(|(context, wanted)| {
            Expr::Tuple(vec![
                Expr::atom(&format!("Context::{}", variant(*context))),
                Expr::atom(if *wanted { "true" } else { "false" }),
            ])
        })
        .collect();
    Expr::Struct(
        "Casing".to_owned(),
        vec![
            (
                "character".to_owned(),
                Expr::atom(&character(casing.character)),
            ),
            ("contexts".to_owned(), Expr::Slice(contexts)),
            (
                "language".to_owned(),
                Expr::atom(&layout::string(&casing.language)),
            ),
            (
                "lower".to_owned(),
                Expr::atom(&layout::string(&casing.lower)),
            ),
            (
                "upper".to_owned(),
                Expr::atom(&layout::string(&casing.upper)),
            ),
        ],
    )
}

/// A character literal, the character's code point in hex.
fn character(found: char) -> String {
    format!("'\\u{{{:x}}}'", u32::from(found))
}

/// The name of the variant of `Context` officekit gives `context`.
const fn variant(context: CasingContext) -> &'static str {
    match context {
        CasingContext::AfterI => "AfterI",
        CasingContext::AfterSoftDotted => "AfterSoftDotted",
        CasingContext::BeforeDot => "BeforeDot",
        CasingContext::MoreAbove => "MoreAbove",
    }
}
