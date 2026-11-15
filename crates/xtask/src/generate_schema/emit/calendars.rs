//! The calendars of a date's format code (section 18.8.31), from Unicode CLDR and Part 1.
//!
//! The eras of the Japanese and the Republic of China calendars and the tabular Hijri calendar,
//! read from CLDR, and the language ids section 18.8.30 gives ja-jp and zh-tw, written to
//! `crates/officekit/src/xlsx/number_format/date/calendars.rs`.

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
use crate::generate_schema::standard_text;

#[cfg(test)]
mod tests;

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/xlsx/number_format/date/calendars.rs";

/// The CLDR locale of section 18.8.31's ja-jp.
pub const JAPAN: &str = "ja_JP";

/// The CLDR locale of section 18.8.31's zh-tw.
pub const TAIWAN: &str = "zh_Hant_TW";

/// The Hijri calendar `b2` writes: of CLDR's arithmetic ones, the one that gives Part 1, section
/// L.1.18.5's 2007-12-17 as 8 Dhu al-Hijjah 1428.
pub const HIJRI: &str = "islamic-tbla";

/// The language of the Hijri months' names: section 18.8.31 writes the Gregorian ones in English.
pub const MONTH_LANGUAGE: &str = "en";

/// The most aliases one lookup follows.
const MOST_ALIASES: usize = 16;

/// The most characters rustfmt keeps an array on one line, `array_width`.
const ARRAY_WIDTH: usize = 60;

/// A day as CLDR writes it: the year, the month and the day.
pub type Day = (i64, i64, i64);

/// One step of a path in a CLDR file: an element's name and, when it has one, its `type`.
pub type Step = (String, Option<String>);

/// An era of a calendar.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Era {
    /// Whether the years count down from `year`, for an era with no first day.
    pub backward: bool,
    /// The first day, `None` for an era CLDR gives only an end.
    pub first: Option<Day>,
    /// What `g`, `gg` and `ggg` write.
    pub names: [String; 3],
    /// The year counted from: the first year, or the last of an era with no first day.
    pub year: i64,
}

/// An era as `supplementalData.xml` gives it.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EraDays {
    /// The calendar that names the era.
    pub calendar: String,
    pub end: Option<Day>,
    /// Its `type`.
    pub kind: String,
    pub start: Option<Day>,
}

/// Everything the table holds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Calendars {
    pub hijri_first: Day,
    pub hijri_leap_years: Vec<i64>,
    /// The narrow, abbreviated and wide name of each month.
    pub hijri_months: Vec<[String; 3]>,
    pub japanese: Vec<Era>,
    pub language_ids: Vec<(u16, String)>,
    pub roc: Vec<Era>,
}

enum Lookup {
    Alias(Vec<Step>),
    Found(String),
    Missing,
}

/// The days CLDR writes as `1868-10-23` or `0-12-31`.
///
/// # Errors
///
/// Text of another shape.
pub fn day(text: &str) -> Result<Day> {
    let mut parts = text.rsplitn(3, '-');
    let mut next = || -> Result<i64> {
        parts
            .next()
            .and_then(|part| part.parse().ok())
            .with_context(|| format!("{text} is not a day"))
    };
    let (day, month, year) = (next()?, next()?, next()?);
    Ok((year, month, day))
}

/// The eras `supplementalData.xml` gives `calendar`, those it inherits first.
///
/// # Errors
///
/// No `calendarData` entry for `calendar`, or an era with no `type` or with a malformed day.
pub fn era_days(supplemental: &Tree, calendar: &str) -> Result<Vec<EraDays>> {
    let entry = supplemental
        .child("calendarData")
        .and_then(|data| {
            data.named("calendar")
                .find(|found| found.attribute("type") == Some(calendar))
        })
        .with_context(|| format!("supplementalData.xml has no calendar {calendar}"))?;
    let mut found = match entry
        .child("inheritEras")
        .and_then(|inherit| inherit.attribute("calendar"))
    {
        Some(inherited) => era_days(supplemental, inherited)?,
        None => Vec::new(),
    };
    for era in entry
        .child("eras")
        .into_iter()
        .flat_map(|eras| eras.named("era"))
    {
        let kind = era
            .attribute("type")
            .with_context(|| format!("an era of {calendar} has no type"))?;
        found.push(EraDays {
            calendar: calendar.to_owned(),
            end: era.attribute("end").map(day).transpose()?,
            kind: kind.to_owned(),
            start: era.attribute("start").map(day).transpose()?,
        });
    }
    Ok(found)
}

/// The text at `start` from the root element of `chain`'s first file that holds it, each alias
/// followed from the start of `chain` (UTS #35, Part 1, "Inheritance and Validity").
///
/// # Errors
///
/// An alias of another source or path, or aliases that do not end.
pub fn resolve(
    main: &BTreeMap<String, Tree>,
    chain: &[String],
    start: &[Step],
) -> Result<Option<String>> {
    let mut path = start.to_vec();
    for _ in 0..MOST_ALIASES {
        match lookup(main, chain, &path)? {
            Lookup::Alias(next) => path = next,
            Lookup::Found(text) => return Ok(Some(text)),
            Lookup::Missing => return Ok(None),
        }
    }
    bail!("the aliases of {} do not end", shown(&path))
}

fn lookup(main: &BTreeMap<String, Tree>, chain: &[String], path: &[Step]) -> Result<Lookup> {
    'locales: for locale in chain {
        let Some(mut at) = main.get(locale) else {
            continue;
        };
        for (depth, (name, kind)) in path.iter().enumerate() {
            if let Some(alias) = at.child("alias") {
                return Ok(Lookup::Alias(aliased(path, depth, alias)?));
            }
            let next = at.children.iter().find(|child| {
                child.name == *name
                    && child.counts()
                    && kind
                        .as_deref()
                        .is_none_or(|wanted| child.attribute("type") == Some(wanted))
            });
            match next {
                Some(child) => at = child,
                None => continue 'locales,
            }
        }
        return Ok(Lookup::Found(at.text.clone()));
    }
    Ok(Lookup::Missing)
}

/// `path` with its first `depth` steps moved by `alias`'s relative path.
fn aliased(path: &[Step], depth: usize, alias: &Tree) -> Result<Vec<Step>> {
    if alias.attribute("source") != Some("locale") {
        bail!("an alias at {} reads another source", shown(&path[..depth]));
    }
    let relative = alias
        .attribute("path")
        .with_context(|| format!("an alias at {} has no path", shown(&path[..depth])))?;
    let mut moved = path[..depth].to_vec();
    for segment in relative.split('/') {
        if segment == ".." {
            moved
                .pop()
                .with_context(|| format!("the alias {relative} leaves the file"))?;
        } else if let Some((name, rest)) = segment.split_once("[@type='") {
            let kind = rest
                .strip_suffix("']")
                .with_context(|| format!("the alias {relative} is not understood"))?;
            moved.push((name.to_owned(), Some(kind.to_owned())));
        } else if segment.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            moved.push((segment.to_owned(), None));
        } else {
            bail!("the alias {relative} is not understood");
        }
    }
    moved.extend_from_slice(&path[depth..]);
    Ok(moved)
}

fn shown(path: &[Step]) -> String {
    path.iter()
        .map(|(name, kind)| {
            kind.as_ref()
                .map_or_else(|| name.clone(), |own| format!("{name}[{own}]"))
        })
        .collect::<Vec<String>>()
        .join("/")
}

fn step(name: &str, kind: Option<&str>) -> Step {
    (name.to_owned(), kind.map(str::to_owned))
}

/// The path of the name of `era` of `calendar` in `width`: `eraNarrow`, `eraAbbr` or `eraNames`.
fn era_path(calendar: &str, width: &str, era: &str) -> Vec<Step> {
    vec![
        step("dates", None),
        step("calendars", None),
        step("calendar", Some(calendar)),
        step("eras", None),
        step(width, None),
        step("era", Some(era)),
    ]
}

/// The eras of `calendar` in `locale`, from the earliest, each named by `names` from its
/// narrow, abbreviated and wide names.
///
/// # Errors
///
/// An era with neither a first nor a last day, with no first day after the first era, or with
/// no name in a width.
pub fn eras(
    supplemental: &Tree,
    main: &BTreeMap<String, Tree>,
    chain: &[String],
    calendar: &str,
    names: fn([String; 3]) -> [String; 3],
) -> Result<Vec<Era>> {
    let mut found = Vec::new();
    for EraDays {
        calendar: of,
        end,
        kind,
        start,
    } in era_days(supplemental, calendar)?
    {
        let (year, backward) = match (start, end) {
            (Some((year, _, _)), _) => (year, false),
            (None, Some((year, _, _))) => (year, true),
            (None, None) => bail!("the era {kind} of {of} has neither a start nor an end"),
        };
        let name = |width: &str| -> Result<String> {
            resolve(main, chain, &era_path(&of, width, &kind))?.with_context(|| {
                format!(
                    "{} gives the era {kind} of {of} no {width}",
                    chain.join(", ")
                )
            })
        };
        let widths = [name("eraNarrow")?, name("eraAbbr")?, name("eraNames")?];
        found.push(Era {
            backward,
            first: start,
            names: names(widths),
            year,
        });
    }
    found.sort_by_key(|era| era.first);
    if found.iter().skip(1).any(|era| era.first.is_none()) {
        bail!("an era of {calendar} after the first has no start");
    }
    Ok(found)
}

/// Section 18.8.31's ja-jp: `g` the narrow name, Roman; `gg` the first character of the
/// abbreviated name, the single Kanji character; `ggg` the wide name.
fn japanese_names([narrow, abbreviated, wide]: [String; 3]) -> [String; 3] {
    let single = abbreviated
        .chars()
        .next()
        .map_or_else(String::new, String::from);
    [narrow, single, wide]
}

/// Section 18.8.31's zh-tw: `g` as `gg`, the short name; `ggg` the long name.
fn roc_names([_, abbreviated, wide]: [String; 3]) -> [String; 3] {
    [abbreviated.clone(), abbreviated, wide]
}

/// The intercalary years of `calendar`'s tabular cycle, which `bcp47/calendar.xml` writes in
/// brackets in its description.
///
/// # Errors
///
/// No description of `calendar`, or one with no list of years.
pub fn leap_years(keywords: &Tree, calendar: &str) -> Result<Vec<i64>> {
    let description = keywords
        .named("keyword")
        .flat_map(|keyword| keyword.named("key"))
        .flat_map(|key| key.named("type"))
        .find(|kind| kind.attribute("name") == Some(calendar))
        .and_then(|kind| kind.attribute("description"))
        .with_context(|| format!("calendar.xml does not describe {calendar}"))?;
    let list = description
        .split_once('[')
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list)
        .with_context(|| format!("the description of {calendar} lists no years"))?;
    list.split(',')
        .map(|year| {
            year.trim()
                .parse()
                .with_context(|| format!("the description of {calendar} lists {year}"))
        })
        .collect()
}

/// The narrow, abbreviated and wide name of each month of `calendar` in `chain`'s language.
///
/// # Errors
///
/// A month with no name in a width.
pub fn months(
    main: &BTreeMap<String, Tree>,
    chain: &[String],
    calendar: &str,
) -> Result<Vec<[String; 3]>> {
    let mut found = Vec::new();
    for month in 1..=12_u8 {
        let name = |width: &str| -> Result<String> {
            let path = [
                step("dates", None),
                step("calendars", None),
                step("calendar", Some(calendar)),
                step("months", None),
                step("monthContext", Some("format")),
                step("monthWidth", Some(width)),
                step("month", Some(&month.to_string())),
            ];
            resolve(main, chain, &path)?.with_context(|| {
                format!(
                    "{} gives month {month} of {calendar} no {width} name",
                    chain.join(", ")
                )
            })
        };
        found.push([name("narrow")?, name("abbreviated")?, name("wide")?]);
    }
    Ok(found)
}

/// Read the inputs, check them, and write the table; the counts of eras and of language ids.
///
/// # Errors
///
/// An input that cannot be read or that the readers refuse, a language the standard pairs with
/// no id or with two, or an output that cannot be written.
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
    let supplemental = tree(&inputs.cldr_supplemental)?;
    let parents = cldr::parent_locales(&supplemental)?;
    let japan = cldr::chain(JAPAN, &parents);
    let taiwan = cldr::chain(TAIWAN, &parents);
    let english = cldr::chain(MONTH_LANGUAGE, &parents);
    let mut main = BTreeMap::new();
    let mut sources = vec![inputs.cldr_license.clone()];
    for path in &inputs.cldr_main {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .with_context(|| format!("{} has no name", path.display()))?;
        if japan
            .iter()
            .chain(&taiwan)
            .chain(&english)
            .any(|id| id == stem)
        {
            main.insert(stem.to_owned(), tree(path)?);
            sources.push(path.clone());
        }
    }
    let first = era_days(&supplemental, HIJRI)?
        .into_iter()
        .find_map(|era| era.start)
        .with_context(|| format!("supplementalData.xml gives {HIJRI} no first day"))?;
    let calendars = Calendars {
        hijri_first: first,
        hijri_leap_years: leap_years(&tree(&inputs.cldr_calendars)?, HIJRI)?,
        hijri_months: months(&main, &english, HIJRI)?,
        japanese: eras(&supplemental, &main, &japan, "japanese", japanese_names)?,
        language_ids: checked_ids(standard_text::language_ids(&read(&inputs.part1_text)?))?,
        roc: eras(&supplemental, &main, &taiwan, "roc", roc_names)?,
    };
    let license = read(&inputs.cldr_license)?;
    let copyright = license
        .lines()
        .find(|line| line.starts_with("Copyright"))
        .context("the CLDR LICENSE holds no copyright line")?;
    sources.extend([
        inputs.cldr_calendars.clone(),
        inputs.cldr_supplemental.clone(),
        inputs.part1_text.clone(),
    ]);
    sources.sort();
    let header = namespaces::header(inputs.checksums(), inputs.tools(), &sources, &[])?;
    let out = Path::new(PATH);
    ready_now(system.write(
        out,
        render(&header, copyright, &calendars).as_bytes(),
        Condition::None,
    ))
    .with_context(|| format!("cannot write {}", out.display()))?;
    Ok((
        calendars.japanese.len() + calendars.roc.len(),
        calendars.language_ids.len(),
    ))
}

/// The ids of `found`, each once and sorted, when ja-jp and zh-tw each have one and no id names
/// two locales.
///
/// # Errors
///
/// An id given two locales, or ja-jp or zh-tw with no id.
pub fn checked_ids(found: Vec<(u16, String)>) -> Result<Vec<(u16, String)>> {
    let mut ids: BTreeMap<u16, String> = BTreeMap::new();
    for (id, locale) in found {
        if let Some(other) = ids.insert(id, locale.clone())
            && other != locale
        {
            bail!("section 18.8.30 gives the language id {id:x} to {other} and to {locale}");
        }
    }
    for wanted in ["ja-jp", "zh-tw"] {
        if !ids.values().any(|locale| locale == wanted) {
            bail!("section 18.8.30 gives {wanted} no language id");
        }
    }
    Ok(ids.into_iter().collect())
}

/// A day as a call to `days_from_civil`, or `i64::MIN` for none.
fn first_day(first: Option<Day>) -> String {
    first.map_or_else(
        || "i64::MIN".to_owned(),
        |(year, month, day)| format!("days_from_civil({year}, {month}, {day})"),
    )
}

/// `items` as an array literal laid out as rustfmt lays it out at `indent` spaces: on one line
/// when it fits, else one item to a line.
fn array(items: &[String], indent: usize) -> String {
    let flat = format!("[{}]", items.join(", "));
    if flat.chars().count() <= ARRAY_WIDTH {
        return flat;
    }
    let inner = " ".repeat(indent + 4);
    let mut text = String::from("[\n");
    for item in items {
        let _: fmt::Result = writeln!(text, "{inner}{item},");
    }
    let _: fmt::Result = write!(text, "{}]", " ".repeat(indent));
    text
}

fn render_eras(text: &mut String, table: &str, doc: &str, eras: &[Era]) {
    let _: fmt::Result = writeln!(
        text,
        "\n{doc}\npub(super) static {table}: [Era; {}] = [",
        eras.len()
    );
    for era in eras {
        let names: Vec<String> = era.names.iter().map(|name| layout::string(name)).collect();
        let _: fmt::Result = writeln!(
            text,
            "    Era {{\n        backward: {},\n        first: {},\n        names: {},\n        \
             year: {},\n    }},",
            era.backward,
            first_day(era.first),
            array(&names, 8),
            era.year,
        );
    }
    text.push_str("];\n");
}

/// The table's source: `header`, the notices, then each table.
#[must_use = "this only returns the value"]
pub fn render(header: &str, copyright: &str, calendars: &Calendars) -> String {
    let mut text = format!(
        "{header}{UNICODE_NOTICE}\n\n//! Unicode CLDR {version}'s eras of the Japanese and the \
         Republic of China calendars, its tabular\n//! Hijri calendar `{HIJRI}`, and the language ids section \
         18.8.30 gives ja-jp and zh-tw.\n//! {copyright} Unicode License v3.\n\nuse \
         super::{{Era, days_from_civil}};\n",
        version = cldr::VERSION,
    );
    render_eras(
        &mut text,
        "JAPANESE",
        &format!(
            "/// The eras of `{JAPAN}`: CLDR's `japanese` calendar and the Gregorian eras it \
             inherits,\n/// from the earliest."
        ),
        &calendars.japanese,
    );
    render_eras(
        &mut text,
        "ROC",
        &format!("/// The eras of `{TAIWAN}`: CLDR's `roc` calendar, from the earliest."),
        &calendars.roc,
    );
    let (year, month, day) = calendars.hijri_first;
    let _: fmt::Result = write!(
        text,
        "\n/// The first day of `{HIJRI}`, 1 Muharram of year 1.\npub(super) const HIJRI_FIRST: \
         i64 = days_from_civil({year}, {month}, {day});\n\n/// The intercalary years of each \
         cycle of `{HIJRI}`.\npub(super) static HIJRI_LEAP_YEARS: [i64; {}] = [{}];\n\n/// The \
         narrow, abbreviated and wide name of each month of `{HIJRI}` in `{MONTH_LANGUAGE}`.\n\
         pub(super) static HIJRI_MONTHS: [[&str; 3]; {}] = [\n",
        calendars.hijri_leap_years.len(),
        calendars
            .hijri_leap_years
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>()
            .join(", "),
        calendars.hijri_months.len(),
    );
    for named in &calendars.hijri_months {
        let names: Vec<String> = named.iter().map(|name| layout::string(name)).collect();
        let _: fmt::Result = writeln!(text, "    {},", array(&names, 4));
    }
    let ids: Vec<String> = calendars
        .language_ids
        .iter()
        .map(|(id, locale)| format!("(0x{id:04x}, {})", layout::string(locale)))
        .collect();
    let _: fmt::Result = write!(
        text,
        "];\n\n/// Each language id a `[$-<language info>]` of section 18.8.30's tables carries and \
         the locale of\n/// its column, sorted by id.\npub(super) static LANGUAGE_IDS: [(u16, \
         &str); {}] = {};\n",
        ids.len(),
        array(&ids, 0),
    );
    text
}
