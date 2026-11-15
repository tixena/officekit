//! The date names and formats of each locale of Unicode CLDR's main files (ECMA-376 Part 1,
//! section 18.8.31), written to `crates/officekit/src/xlsx/number_format/date/names.rs`.
//!
//! A locale's names are its months and weekdays; its formats are its long date and medium time
//! patterns and the names of the halves of the day.

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

/// The elements from a file's root to a calendar; below one only [`KEPT_IN_CALENDAR`] and its
/// aliases are kept.
const KEPT: [&str; 3] = ["dates", "calendars", "calendar"];

const KEPT_IN_CALENDAR: [&str; 5] = ["months", "days", "dateFormats", "timeFormats", "dayPeriods"];

/// The formats a locale writes for section 18.8.31's system long date and system time.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Formats {
    /// The Gregorian `long` date pattern, in UTS #35's pattern letters.
    pub date: String,
    /// The abbreviated `am` and `pm` names, in the format context.
    pub halves: Vec<String>,
    /// The Gregorian `medium` time pattern.
    pub time: String,
}

/// Each distinct list of names, and the locales that give them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Names {
    /// Each pair of names of the halves of the day.
    pub halves: Vec<Vec<String>>,
    /// Each locale, sorted, and its lists: the narrow, abbreviated and wide Gregorian months, the
    /// abbreviated and wide weekdays, and the narrow, abbreviated and wide Hijri months.
    pub locales: Vec<(String, [usize; 8])>,
    /// Each list of twelve month names.
    pub months: Vec<Vec<String>>,
    /// Each long date and medium time pattern.
    pub patterns: Vec<String>,
    /// Each locale's index in [`patterns`](Self::patterns) of its long date and its medium time
    /// pattern, and in [`halves`](Self::halves) of its names, in the order of `locales`.
    pub system: Vec<[usize; 3]>,
    /// Each list of seven weekday names, from Sunday.
    pub weekdays: Vec<Vec<String>>,
}

impl Names {
    /// Add `locale` and its lists: `months` and `hijri` narrow, abbreviated and wide, `weekdays`
    /// abbreviated and wide; and its `formats`.
    pub fn add(
        &mut self,
        locale: &str,
        months: [Vec<String>; 3],
        weekdays: [Vec<String>; 2],
        hijri: [Vec<String>; 3],
        formats: Formats,
    ) {
        self.system.push([
            Self::index(&mut self.patterns, formats.date),
            Self::index(&mut self.patterns, formats.time),
            Self::index(&mut self.halves, formats.halves),
        ]);
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
    fn index<T>(lists: &mut Vec<T>, list: T) -> usize
    where
        T: PartialEq,
    {
        lists
            .iter()
            .position(|found| *found == list)
            .unwrap_or_else(|| {
                lists.push(list);
                lists.len() - 1
            })
    }
}

/// `file` less everything but each calendar's [`KEPT_IN_CALENDAR`] and aliases, and the aliases
/// above.
#[must_use = "this only returns the value"]
pub fn pruned(mut file: Tree, depth: usize) -> Tree {
    file.children = file
        .children
        .into_iter()
        .filter_map(|child| match KEPT.get(depth) {
            _ if child.name == "alias" => Some(child),
            Some(name) if child.name == *name => Some(pruned(child, depth + 1)),
            None if KEPT_IN_CALENDAR.contains(&child.name.as_str()) => Some(child),
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

/// The Gregorian long date and medium time patterns, and the abbreviated `am` and `pm` names, of
/// `chain`'s language (UTS #35, Part 4, "Dates").
///
/// # Errors
///
/// A pattern or a name that is missing, or an `am` name holding the `/` that parts `AM/PM`.
pub fn formats(main: &BTreeMap<String, Tree>, chain: &[String]) -> Result<Formats> {
    let found = |steps: &[(&str, Option<&str>)]| -> Result<String> {
        let mut path: Vec<Step> = vec![
            ("dates".to_owned(), None),
            ("calendars".to_owned(), None),
            ("calendar".to_owned(), Some("gregorian".to_owned())),
        ];
        path.extend(
            steps
                .iter()
                .map(|(name, kind)| ((*name).to_owned(), kind.map(str::to_owned))),
        );
        resolve(main, chain, &path)?.with_context(|| {
            let last = steps.iter().rev().find_map(|(_, kind)| *kind).unwrap_or("");
            format!("{} gives no {last} {}", chain.join(", "), steps[0].0)
        })
    };
    let pattern = |formats: &str, length: &str, format: &str| {
        found(&[
            (formats, None),
            (&format!("{format}Length"), Some(length)),
            (format, None),
            ("pattern", None),
        ])
    };
    let half = |kind: &str| {
        found(&[
            ("dayPeriods", None),
            ("dayPeriodContext", Some("format")),
            ("dayPeriodWidth", Some("abbreviated")),
            ("dayPeriod", Some(kind)),
        ])
    };
    let am = half("am")?;
    if am.contains('/') {
        bail!("{} names am {am}, which holds a /", chain.join(", "));
    }
    Ok(Formats {
        date: pattern("dateFormats", "long", "dateFormat")?,
        halves: vec![am, half("pm")?],
        time: pattern("timeFormats", "medium", "timeFormat")?,
    })
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
            formats(main, &chain)?,
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

/// `indexes` as the items of a static array of `[u16; length]`, one to a line.
fn render_indexes(text: &mut String, locale: &str, indexes: &[usize]) -> Result<()> {
    let mut shown = Vec::new();
    for list in indexes {
        let index =
            u16::try_from(*list).with_context(|| format!("the lists of {locale} pass a u16"))?;
        shown.push(index.to_string());
    }
    let _: fmt::Result = writeln!(text, "    {},", array(&shown, 4));
    Ok(())
}

/// The table's source: `header`, the notices, the lists of month and of weekday names, the
/// patterns and the names of the halves of the day, the locales, and each locale's lists.
///
/// # Errors
///
/// More lists than a `u16` counts.
pub fn render(header: &str, copyright: &str, names: &Names) -> Result<String> {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n//! Unicode CLDR {version}'s month and weekday names of each \
         locale, in the format context of the\n//! Gregorian calendar and of `{HIJRI}`; its \
         Gregorian long date and medium time patterns, in\n//! UTS #35's pattern letters; and its \
         abbreviated names of the halves of the day.\n//! {copyright} Unicode License v3.\n\n/// Each list of twelve month names, from January or \
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
    let patterns: Vec<String> = names
        .patterns
        .iter()
        .map(|pattern| layout::string(pattern))
        .collect();
    let _: fmt::Result = write!(
        text,
        "\n/// Each long date and medium time pattern.\npub(super) static PATTERNS: [&str; {}] = \
         {};\n\n/// Each pair of names of the morning and the afternoon.\npub(super) static \
         HALVES: [[&str; 2]; {}] = [\n",
        patterns.len(),
        array(&patterns, 0),
        names.halves.len(),
    );
    render_lists(&mut text, &names.halves);
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
        render_indexes(&mut text, locale, lists)?;
    }
    let _: fmt::Result = write!(
        text,
        "];\n\n/// The formats of each of [`LOCALES`]: the index in [`PATTERNS`] of its long date \
         and its medium\n/// time pattern, and in [`HALVES`] of its names of the halves of the \
         day.\npub(super) static SYSTEM: [[u16; 3]; {}] = [\n",
        names.system.len(),
    );
    for ((locale, _), indexes) in names.locales.iter().zip(&names.system) {
        render_indexes(&mut text, locale, indexes)?;
    }
    text.push_str("];\n");
    Ok(text)
}
