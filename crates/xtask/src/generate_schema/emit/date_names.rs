//! The month and weekday names of each locale of Unicode CLDR's main files (ECMA-376 Part 1,
//! section 18.8.31), written to `crates/officekit/src/xlsx/number_format/date/names.rs`.

use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use super::calendars::{HIJRI, Step, array, months, resolve};
use super::markup::render::layout;
use super::namespaces::{self, UNICODE_NOTICE};
use crate::generate_schema::cldr::{self, Tree};
use crate::generate_schema::inputs::Inputs;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/number_format/date/names.rs";

/// The weekdays as CLDR names them, from Sunday.
const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/// The locale whose Hijri months `b2` writes in a section with no language.
pub const ENGLISH: &str = "en";

/// The elements from a file's root to a calendar; below one only its months, its days and its
/// aliases are kept.
const KEPT: [&str; 3] = ["dates", "calendars", "calendar"];

/// Each distinct list of names, and the locales that give them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Names {
    /// Each locale, sorted, and its lists: the narrow, abbreviated and wide Gregorian months, the
    /// abbreviated and wide weekdays, and the narrow, abbreviated and wide Hijri months.
    pub locales: Vec<(String, [usize; 8])>,
    /// Each list of twelve month names.
    pub months: Vec<Vec<String>>,
    /// Each list of seven weekday names, from Sunday.
    pub weekdays: Vec<Vec<String>>,
}

impl Names {
    /// Add `locale` and its lists: `months` and `hijri` narrow, abbreviated and wide, `weekdays`
    /// abbreviated and wide.
    pub fn add(
        &mut self,
        locale: &str,
        months: [Vec<String>; 3],
        weekdays: [Vec<String>; 2],
        hijri: [Vec<String>; 3],
    ) {
        let mut lists = [0; 8];
        for (at, list) in months.into_iter().enumerate() {
            lists[at] = Self::index(&mut self.months, list);
        }
        for (at, list) in weekdays.into_iter().enumerate() {
            lists[3 + at] = Self::index(&mut self.weekdays, list);
        }
        for (at, list) in hijri.into_iter().enumerate() {
            lists[5 + at] = Self::index(&mut self.months, list);
        }
        self.locales.push((locale.to_owned(), lists));
    }

    /// The index of `list` in `lists`, added when it is new.
    fn index(lists: &mut Vec<Vec<String>>, list: Vec<String>) -> usize {
        lists
            .iter()
            .position(|found| *found == list)
            .unwrap_or_else(|| {
                lists.push(list);
                lists.len() - 1
            })
    }
}

/// `file` less everything but each calendar's months, days and aliases, and the aliases above.
#[must_use = "this only returns the value"]
pub fn pruned(mut file: Tree, depth: usize) -> Tree {
    file.children = file
        .children
        .into_iter()
        .filter_map(|child| match KEPT.get(depth) {
            _ if child.name == "alias" => Some(child),
            Some(name) if child.name == *name => Some(pruned(child, depth + 1)),
            None if matches!(child.name.as_str(), "months" | "days") => Some(child),
            _ => None,
        })
        .collect();
    file
}

/// The narrow, abbreviated and wide lists of the months of `calendar` in `chain`'s language.
///
/// # Errors
///
/// A month with no name in a width.
pub fn month_lists(
    main: &BTreeMap<String, Tree>,
    chain: &[String],
    calendar: &str,
) -> Result<[Vec<String>; 3]> {
    let mut lists: [Vec<String>; 3] = Default::default();
    for named in months(main, chain, calendar)? {
        for (list, name) in lists.iter_mut().zip(named) {
            list.push(name);
        }
    }
    Ok(lists)
}

/// The abbreviated and wide lists of the Gregorian weekdays in `chain`'s language, from Sunday.
///
/// # Errors
///
/// A weekday with no name in a width.
pub fn weekday_lists(main: &BTreeMap<String, Tree>, chain: &[String]) -> Result<[Vec<String>; 2]> {
    let mut lists: [Vec<String>; 2] = Default::default();
    for (list, width) in lists.iter_mut().zip(["abbreviated", "wide"]) {
        for day in DAYS {
            let path: Vec<Step> = [
                ("dates", None),
                ("calendars", None),
                ("calendar", Some("gregorian")),
                ("days", None),
                ("dayContext", Some("format")),
                ("dayWidth", Some(width)),
                ("day", Some(day)),
            ]
            .into_iter()
            .map(|(name, kind)| (name.to_owned(), kind.map(str::to_owned)))
            .collect();
            list.push(
                resolve(main, chain, &path)?
                    .with_context(|| format!("{} gives {day} no {width} name", chain.join(", ")))?,
            );
        }
    }
    Ok(lists)
}

/// The lists of every locale of `main` but `root`, each resolved through its parents.
///
/// # Errors
///
/// A name that is missing, or no locale [`ENGLISH`].
pub fn names(main: &BTreeMap<String, Tree>, parents: &BTreeMap<String, String>) -> Result<Names> {
    let mut found = Names::default();
    for locale in main.keys().filter(|locale| *locale != "root") {
        let chain = cldr::chain(locale, parents);
        found.add(
            locale,
            month_lists(main, &chain, "gregorian")?,
            weekday_lists(main, &chain)?,
            month_lists(main, &chain, HIJRI)?,
        );
    }
    if !main.contains_key(ENGLISH) {
        bail!("CLDR has no locale {ENGLISH}");
    }
    Ok(found)
}

/// Read every main file, resolve each locale's names, and write the table; the counts of locales
/// and of lists.
///
/// # Errors
///
/// An input that cannot be read or that the readers refuse, or an output that cannot be written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<(usize, usize)>
where
    S: System + ?Sized,
{
    let read = |path: &Path| -> Result<String> {
        ready_now(system.read_to_string(path))
            .with_context(|| format!("cannot read {}", path.display()))
    };
    let tree = |path: &Path| -> Result<Tree> {
        cldr::read(&read(path)?).with_context(|| format!("in {}", path.display()))
    };
    let parents = cldr::parent_locales(&tree(&inputs.cldr_supplemental)?)?;
    let mut main = BTreeMap::new();
    for path in &inputs.cldr_main {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .with_context(|| format!("{} has no name", path.display()))?;
        main.insert(stem.to_owned(), pruned(tree(path)?, 0));
    }
    let found = names(&main, &parents)?;
    let license = read(&inputs.cldr_license)?;
    let copyright = license
        .lines()
        .find(|line| line.starts_with("Copyright"))
        .context("the CLDR LICENSE holds no copyright line")?;
    let mut sources = vec![inputs.cldr_license.clone()];
    sources.extend(inputs.cldr_main.iter().cloned());
    sources.push(inputs.cldr_supplemental.clone());
    let header = namespaces::header(inputs.checksums(), inputs.tools(), &sources, &[])?;
    let out = Path::new(PATH);
    ready_now(system.write(
        out,
        render(&header, copyright, &found)?.as_bytes(),
        Condition::None,
    ))
    .with_context(|| format!("cannot write {}", out.display()))?;
    Ok((
        found.locales.len(),
        found.months.len() + found.weekdays.len(),
    ))
}

/// `lists` as the items of a static array of `[&str; length]`, one to a line.
fn render_lists(text: &mut String, lists: &[Vec<String>]) {
    for list in lists {
        let names: Vec<String> = list.iter().map(|name| layout::string(name)).collect();
        let _: fmt::Result = writeln!(text, "    {},", array(&names, 4));
    }
    text.push_str("];\n");
}

/// The table's source: `header`, the notices, the lists of month and of weekday names, the
/// locales, and each locale's lists.
///
/// # Errors
///
/// More lists than a `u16` counts.
pub fn render(header: &str, copyright: &str, names: &Names) -> Result<String> {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n//! Unicode CLDR {version}'s month and weekday names of each \
         locale, in the format context of the\n//! Gregorian calendar and of `{HIJRI}`. \
         {copyright} Unicode License v3.\n\n/// Each list of twelve month names, from January or \
         from Muharram.\npub(super) static MONTHS: [[&str; 12]; {}] = [\n",
        names.months.len(),
        version = cldr::VERSION,
    );
    render_lists(&mut text, &names.months);
    let _: fmt::Result = write!(
        text,
        "\n/// Each list of seven weekday names, from Sunday.\npub(super) static WEEKDAYS: [[&str; \
         7]; {}] = [\n",
        names.weekdays.len()
    );
    render_lists(&mut text, &names.weekdays);
    let locales: Vec<String> = names
        .locales
        .iter()
        .map(|(locale, _)| layout::string(locale))
        .collect();
    let _: fmt::Result = write!(
        text,
        "\n/// Each locale of CLDR's main files but `root`, sorted.\npub(super) static LOCALES: \
         [&str; {}] = {};\n\n/// The lists of each of [`LOCALES`]: the index in [`MONTHS`] of its \
         narrow, abbreviated and wide\n/// Gregorian months, in [`WEEKDAYS`] of its abbreviated \
         and wide weekdays, and in [`MONTHS`] of its\n/// narrow, abbreviated and wide \
         `{HIJRI}` months.\npub(super) static NAMES: [[u16; 8]; {}] = [\n",
        locales.len(),
        array(&locales, 0),
        names.locales.len(),
    );
    for (locale, lists) in &names.locales {
        let mut indexes = Vec::new();
        for list in lists {
            let index = u16::try_from(*list)
                .with_context(|| format!("the lists of {locale} pass a u16"))?;
            indexes.push(index.to_string());
        }
        let _: fmt::Result = writeln!(text, "    {},", array(&indexes, 4));
    }
    text.push_str("];\n");
    Ok(text)
}
