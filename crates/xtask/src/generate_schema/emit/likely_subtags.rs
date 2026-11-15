//! Unicode CLDR's likely subtags (UTS #35, Part 1, "Likely Subtags"), read from
//! `likelySubtags.xml` and written to `crates/officekit/src/docx/fonts/likely_subtags.rs`.

use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::markup::render::layout;
use super::namespaces::{self, UNICODE_NOTICE};
use crate::generate_schema::cldr::{self, Tree};
use crate::generate_schema::inputs::Inputs;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/docx/fonts/likely_subtags.rs";

/// Each id the file gives and the full id it gives it, sorted by id, when every full id is a
/// language, a script of four letters and a region, and no id is given twice.
///
/// # Errors
///
/// A file with no `likelySubtags`, a row with no `from` or `to`, an id given twice, or a full id
/// of another shape.
pub fn rows(file: &Tree) -> Result<BTreeMap<String, String>> {
    let list = file
        .child("likelySubtags")
        .context("likelySubtags.xml has no likelySubtags")?;
    let mut found = BTreeMap::new();
    for row in list.named("likelySubtag") {
        let from = row
            .attribute("from")
            .context("a likelySubtag with no from")?;
        let to = row
            .attribute("to")
            .with_context(|| format!("the likelySubtag of {from} has no to"))?;
        let subtags: Vec<&str> = to.split('_').collect();
        let shaped = matches!(subtags.as_slice(), [language, script, region]
            if !language.is_empty()
                && script.len() == 4
                && script.bytes().all(|byte| byte.is_ascii_alphabetic())
                && !region.is_empty());
        if !shaped {
            bail!("the likelySubtag of {from} gives {to}, not a language, a script and a region");
        }
        if found.insert(from.to_owned(), to.to_owned()).is_some() {
            bail!("likelySubtags.xml gives {from} twice");
        }
    }
    Ok(found)
}

/// Read `likelySubtags.xml`, check its rows, and write the table; the count of rows written.
///
/// # Errors
///
/// An input that cannot be read or that [`rows`] refuses, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let path = &inputs.cldr_likely_subtags;
    let text = ready_now(system.read_to_string(path))
        .with_context(|| format!("cannot read {}", path.display()))?;
    let found = rows(&cldr::read(&text).with_context(|| format!("in {}", path.display()))?)?;
    let license = ready_now(system.read_to_string(&inputs.cldr_license))
        .with_context(|| format!("cannot read {}", inputs.cldr_license.display()))?;
    let copyright = license
        .lines()
        .find(|line| line.starts_with("Copyright"))
        .context("the CLDR LICENSE holds no copyright line")?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        &[inputs.cldr_license.clone(), path.clone()],
        &[],
    )?;
    let out = Path::new(PATH);
    ready_now(system.write(
        out,
        render(&header, copyright, &found).as_bytes(),
        Condition::None,
    ))
    .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(found.len())
}

/// The table's source: `header`, the CLDR notice, then one row per id, in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, copyright: &str, rows: &BTreeMap<String, String>) -> String {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n//! Unicode CLDR {version}'s likely subtags: the most \
         likely language, script and region of an id\n//! that lacks some. {copyright} Unicode License v3.\n\n/// Each \
         id CLDR gives and its most likely full id, `language_Script_REGION`, sorted by id.\n\
         pub(super) static LIKELY_SUBTAGS: [(&str, &str); {}] = [\n",
        rows.len(),
        version = cldr::VERSION,
    );
    for (from, to) in rows {
        let _: fmt::Result = writeln!(
            text,
            "    ({}, {}),",
            layout::string(from),
            layout::string(to)
        );
    }
    text.push_str("];\n");
    text
}
