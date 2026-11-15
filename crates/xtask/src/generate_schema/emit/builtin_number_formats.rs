//! The built-in number formats: the format code of each id of the list "All Languages", read from
//! section 18.8.30 of Part 1 and written to `crates/officekit/src/xlsx/format/builtin.rs`.

use core::fmt::{self, Write as _};
use core::slice;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::namespaces;
use crate::generate_schema::inputs::Inputs;
use crate::generate_schema::standard_text;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/format/builtin.rs";

/// The count of ids the list "All Languages" gives.
pub const COUNT: usize = 28;

/// The formats, sorted by id, when the list gives [`COUNT`] of them, each id once.
///
/// # Errors
///
/// Another count, or an id given twice.
pub fn checked(mut formats: Vec<(u32, String)>) -> Result<Vec<(u32, String)>> {
    if formats.len() != COUNT {
        bail!(
            "section 18.8.30 gives {} built-in number formats for all languages, not {COUNT}",
            formats.len()
        );
    }
    formats.sort();
    if let Some(((id, _), _)) = formats
        .iter()
        .zip(formats.iter().skip(1))
        .find(|((left, _), (right, _))| left == right)
    {
        bail!("section 18.8.30 gives the number format id {id} twice");
    }
    Ok(formats)
}

/// Read section 18.8.30 of Part 1 and write the table; the count of formats written.
///
/// # Errors
///
/// An input that cannot be read, a list that gives no table, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<usize>
where
    S: System + ?Sized,
{
    let text = ready_now(system.read_to_string(&inputs.part1_text))
        .with_context(|| format!("cannot read {}", inputs.part1_text.display()))?;
    let formats = checked(standard_text::builtin_number_formats(&text))?;
    let header = namespaces::header(
        inputs.checksums(),
        inputs.tools(),
        slice::from_ref(&inputs.part1_text),
        &[],
    )?;
    let out = Path::new(PATH);
    if let Some(parent) = out.parent() {
        ready_now(system.create_dir_all(parent))
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    ready_now(system.write(out, render(&header, &formats).as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(formats.len())
}

/// `text` as a Rust string literal, its quotes and backslashes escaped.
fn literal(text: &str) -> String {
    let mut out = String::from('"');
    for character in text.chars() {
        if matches!(character, '"' | '\\') {
            out.push('\\');
        }
        out.push(character);
    }
    out.push('"');
    out
}

/// The table's source: `header`, then one row per format in the order given.
#[must_use = "this only returns the value"]
pub fn render(header: &str, formats: &[(u32, String)]) -> String {
    let mut text = format!(
        "{header}\n/// The format code of each built-in number format id of the list \"All Languages\" \
         (ECMA-376\n/// Part 1, section 18.8.30), sorted by id.\npub(super) static BUILTIN_FORMATS: \
         [(u32, &str); {}] = [\n",
        formats.len()
    );
    for (id, code) in formats {
        let _: fmt::Result = writeln!(text, "    ({id}, {}),", literal(code));
    }
    text.push_str("];\n");
    text
}
