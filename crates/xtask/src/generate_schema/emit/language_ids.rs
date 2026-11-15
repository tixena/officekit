//! The language ids of [MS-LCID] section 2.2, checked against ECMA-376 Part 1, section 18.8.30,
//! and written to `crates/officekit/src/xlsx/number_format/language_ids.rs`.

use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::markup::render::layout;
use super::namespaces;
use crate::generate_schema::cldr::{self, Tree};
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::standard_text;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/number_format/language_ids.rs";

/// The first row of section 2.2's table of language ids.
const IDS: [&str; 2] = ["Language ID", "Language tag"];

/// The first row of the document's revision summary, whose last row is its version.
const REVISIONS: [&str; 4] = ["Date", "Revision History", "Revision Class", "Comments"];

/// Each element named `name` under `tree`, in document order.
fn descendants<'tree>(tree: &'tree Tree, name: &str, found: &mut Vec<&'tree Tree>) {
    for child in &tree.children {
        if child.name == name {
            found.push(child);
        } else {
            descendants(child, name, found);
        }
    }
}

/// The text of every `w:t` under `tree`, in document order.
fn text_of(tree: &Tree) -> String {
    let mut runs = Vec::new();
    descendants(tree, "w:t", &mut runs);
    runs.iter().map(|run| run.text.as_str()).collect()
}

/// The rows of a DOCX table, each the text of its cells, each cell's paragraphs joined by a
/// space.
fn rows(table: &Tree) -> Vec<Vec<String>> {
    table
        .named("w:tr")
        .map(|row| {
            row.named("w:tc")
                .map(|cell| {
                    cell.named("w:p")
                        .map(|paragraph| text_of(paragraph).trim().to_owned())
                        .collect::<Vec<String>>()
                        .join(" ")
                })
                .collect()
        })
        .collect()
}

/// The rows of the one table of `document` whose first row is `heading`.
fn table(document: &Tree, heading: &[&str]) -> Result<Vec<Vec<String>>> {
    let mut tables = Vec::new();
    descendants(document, "w:tbl", &mut tables);
    let mut found = tables
        .into_iter()
        .map(rows)
        .filter(|rows| rows.first().is_some_and(|first| first == heading));
    let one = found
        .next()
        .with_context(|| format!("[MS-LCID] has no table headed {heading:?}"))?;
    if found.next().is_some() {
        bail!("[MS-LCID] has two tables headed {heading:?}");
    }
    Ok(one)
}

/// The language tag of a cell, its text before any comma, as `fr-015` of `fr-015, reserved`;
/// `None` for a cell that describes an id with no language.
#[must_use = "this only returns the value"]
pub fn tag(cell: &str) -> Option<&str> {
    let first = cell.split(',').next()?.trim();
    let (tag, sort) = first.split_once('_').unwrap_or((first, ""));
    let mut subtags = tag.split('-');
    let language = subtags.next()?;
    let shaped = (2..=3).contains(&language.len())
        && language.bytes().all(|byte| byte.is_ascii_alphabetic())
        && subtags.all(|subtag| {
            !subtag.is_empty() && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        && sort.bytes().all(|byte| byte.is_ascii_alphabetic());
    shaped.then_some(first)
}

/// The language id and tag of each row of section 2.2's table that names a language, sorted by
/// id.
///
/// # Errors
///
/// No such table, a row of other than two cells, an id that is not `0x` and four hexadecimal
/// digits, or an id given twice.
pub fn language_ids(document: &Tree) -> Result<BTreeMap<u16, String>> {
    let mut found = BTreeMap::new();
    for row in table(document, &IDS)?.iter().skip(1) {
        let [id, cell] = row.as_slice() else {
            bail!("a row of [MS-LCID]'s language ids holds {row:?}");
        };
        let value = id
            .strip_prefix("0x")
            .filter(|hex| hex.len() == 4)
            .and_then(|hex| u16::from_str_radix(hex, 16).ok())
            .with_context(|| format!("[MS-LCID] gives {id} as a language id"))?;
        let Some(named) = tag(cell) else {
            continue;
        };
        if found.insert(value, named.to_owned()).is_some() {
            bail!("[MS-LCID] gives the language id {id} twice");
        }
    }
    Ok(found)
}

/// The version of the last row of the document's revision summary.
///
/// # Errors
///
/// No revision summary, or one with no version in its last row.
pub fn version(document: &Tree) -> Result<String> {
    table(document, &REVISIONS)?
        .last()
        .and_then(|row| row.get(1))
        .filter(|found| !found.is_empty())
        .cloned()
        .context("[MS-LCID]'s revision summary gives no version")
}

/// `ids` when each id section 18.8.30 gives a locale is the id [MS-LCID] gives that locale, letter
/// case aside, and ja-jp and zh-tw each have one.
///
/// # Errors
///
/// An id the two give different languages, or ja-jp or zh-tw with no id in section 18.8.30.
pub fn checked(
    ids: BTreeMap<u16, String>,
    standard: &[(u16, String)],
) -> Result<BTreeMap<u16, String>> {
    for (id, locale) in standard {
        let given = ids.get(id).map_or("no language", String::as_str);
        if !given.eq_ignore_ascii_case(locale) {
            bail!("section 18.8.30 gives the language id {id:x} to {locale}, [MS-LCID] to {given}");
        }
    }
    for wanted in ["ja-jp", "zh-tw"] {
        if !standard.iter().any(|(_, locale)| locale == wanted) {
            bail!("section 18.8.30 gives {wanted} no language id");
        }
    }
    Ok(ids)
}

/// Read [MS-LCID] and Part 1, check them, and write the table; the count of ids written.
///
/// # Errors
///
/// An input that cannot be read or that the readers refuse, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let read = |path: &Path| -> Result<String> {
        ready_now(system.read_to_string(path))
            .with_context(|| format!("cannot read {}", path.display()))
    };
    let document = cldr::read(&read(&inputs.ms_lcid)?)
        .with_context(|| format!("in {}", inputs.ms_lcid.display()))?;
    let standard = standard_text::language_ids(&read(&inputs.part1_text)?);
    let ids = checked(language_ids(&document)?, &standard)?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &[inputs.ms_lcid.clone(), inputs.part1_text.clone()],
        &[],
    )?;
    let out = Path::new(PATH);
    ready_now(system.write(
        out,
        render(&header, &version(&document)?, &ids).as_bytes(),
        Condition::None,
    ))
    .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(ids.len())
}

/// The table's source: `header`, then one row per id, in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, version: &str, ids: &BTreeMap<u16, String>) -> String {
    let mut text = format!(
        "{header}\n//! The language ids of [MS-LCID], \"Windows Language Code Identifier (LCID) \
         Reference\", version\n//! {version}, section 2.2: the bytes 0 and 1 of a number format's \
         `[$-<language info>]` (ECMA-376\n//! Part 1, section 18.8.31).\n\n/// Each language id \
         and the language tag [MS-LCID] gives it, sorted by id.\npub(super) static \
         LANGUAGE_IDS: [(u16, &str); {}] = [\n",
        ids.len(),
    );
    for (id, named) in ids {
        let _: fmt::Result = writeln!(text, "    (0x{id:04x}, {}),", layout::string(named));
    }
    text.push_str("];\n");
    text
}
