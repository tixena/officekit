//! The table of the number formats in the run's language, and of those spelled only to a limit,
//! from Unicode CLDR (ECMA-376 Part 1, section 17.18.59).

pub mod plurals;
pub mod rules;

use alloc::collections::{BTreeMap, BTreeSet};
use core::fmt::{self, Write as _};
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit_xml::ready::ready_now;
use os_shim::System;
use os_shim::bucket::Condition;

use self::plurals::{PluralRule, Relation};
use self::rules::{Part, Pattern, RuleSet, Target};
use super::markup::render::layout::{self, Expr};
use super::namespaces::{self, UNICODE_NOTICE};
use crate::generate_schema::cldr::{self, Tree};
use crate::generate_schema::inputs::Inputs;

#[cfg(test)]
mod tests;

/// The rule set of `cardinalText` and of `dollarText`'s words.
const CARDINAL: [&str; 1] = ["%spellout-numbering"];

/// The number formats section 17.18.59 spells only to a limit: each `StNumberFormatMember`
/// variant and the CLDR rule set, by locale, grouping and name, that writes it past that limit.
pub const COUNTING: [(&str, &str, &str, &str); 11] = [
    (
        "ChineseCountingThousand",
        "zh",
        SPELLOUT,
        "%spellout-cardinal",
    ),
    (
        "ChineseLegalSimplified",
        "zh",
        SPELLOUT,
        "%spellout-cardinal-financial",
    ),
    ("Hebrew1", "root", "NumberingSystemRules", "%hebrew"),
    ("HindiCounting", "hi", SPELLOUT, "%spellout-cardinal"),
    (
        "IdeographLegalTraditional",
        "zh_Hant",
        SPELLOUT,
        "%spellout-cardinal-financial",
    ),
    ("JapaneseCounting", "ja", SPELLOUT, "%spellout-cardinal"),
    (
        "JapaneseLegal",
        "ja",
        SPELLOUT,
        "%spellout-cardinal-financial",
    ),
    (
        "KoreanCounting",
        "ko",
        SPELLOUT,
        "%spellout-cardinal-sinokorean",
    ),
    (
        "TaiwaneseCountingThousand",
        "zh_Hant",
        SPELLOUT,
        "%spellout-cardinal",
    ),
    ("ThaiCounting", "th", SPELLOUT, "%spellout-cardinal"),
    ("VietnameseCounting", "vi", SPELLOUT, "%spellout-cardinal"),
];

/// The rule set of `ordinal`.
const ORDINAL: [&str; 1] = ["%digits-ordinal"];

/// The rule set of `ordinalText`: the first of these a locale has. Only Korean's rules hold
/// `%spellout-ordinal-native`, its native ordinals.
const ORDINAL_TEXT: [&str; 4] = [
    "%spellout-ordinal",
    "%spellout-ordinal-masculine",
    "%spellout-ordinal-common",
    "%spellout-ordinal-native",
];

const SPELLOUT: &str = "SpelloutRules";

/// The letters A to Z.
const UPPER_LETTERS: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

/// Where the table is written, from the workspace root.
pub const PATH: &str = "crates/officekit/src/docx/numbering/rule_based/locales.rs";

/// What one locale's main file states that the formats use, before inheritance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Main {
    /// The two-item pattern of the standard list, `{0} and {1}` in English.
    pub and: Option<String>,
    /// The index exemplar set, the alphabet's letters in order.
    pub index: Option<String>,
    pub numbering: Option<String>,
    /// For each numbering system, its decimal separator and its group separator.
    pub symbols: BTreeMap<String, Symbols>,
}

/// The `symbols` of one numbering system in a main file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Symbols {
    /// An alias to the symbols of the numbering system named, read from the requested locale.
    Alias(String),
    /// The decimal separator and the group separator, when stated.
    Stated(Option<String>, Option<String>),
}

/// Everything the generator reads from CLDR.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct Cldr {
    pub cardinal_plurals: BTreeMap<String, Vec<PluralRule>>,
    pub main: BTreeMap<String, Main>,
    /// The digits of each numeric numbering system, 0 to 9.
    pub numbering_systems: BTreeMap<String, String>,
    pub ordinal_plurals: BTreeMap<String, Vec<PluralRule>>,
    pub parents: BTreeMap<String, String>,
    /// For each locale with rules, the rule sets of each grouping.
    pub rules: BTreeMap<String, BTreeMap<String, Vec<RuleSet>>>,
}

/// What a locale resolves to.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Facts {
    /// The upper-case and lower-case letters of `upperLetter` and `lowerLetter`, when its
    /// alphabet is Latin and other than A to Z.
    pub letters: Option<(Vec<String>, Vec<String>)>,
    pub words: Option<Words>,
}

/// A rule set: the locale that holds it, its grouping and its name.
pub type SetKey = (String, String, String);

/// The formats in words and the facts they use, of a locale CLDR has rules for.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Words {
    pub and: String,
    pub cardinal: Option<SetKey>,
    pub cardinal_plurals: Vec<PluralRule>,
    pub decimal: String,
    pub digits: String,
    pub group: String,
    pub ordinal: Option<SetKey>,
    pub ordinal_plurals: Vec<PluralRule>,
    pub ordinal_text: Option<SetKey>,
}

/// The counts of what the table holds.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Summary {
    pub alphabets: usize,
    pub locales: usize,
    pub rule_sets: usize,
    pub with_words: usize,
}

/// Everything the table holds, each list deduplicated and indexed.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Table {
    /// The alphabets, upper case then lower case.
    pub alphabets: Vec<(Vec<String>, Vec<String>)>,
    /// Each format of [`COUNTING`]'s kind: its variant and the index of its rule set.
    pub formats: Vec<(String, usize)>,
    /// Each locale whose facts differ from its parent's, with its alphabet and words.
    pub locales: Vec<(String, Option<usize>, Option<usize>)>,
    /// Each locale a parent locale rule names, and that parent.
    pub parents: Vec<(String, String)>,
    pub plurals: Vec<Vec<PluralRule>>,
    /// The rule sets the words reach, each rule's references made indexes.
    pub rule_sets: Vec<RuleSet>,
    /// The words, each rule set an index of `rule_sets` and each plural list one of `plurals`.
    pub words: Vec<(Words, [Option<usize>; 3], [usize; 2])>,
}

impl Cldr {
    /// What `locale` resolves to through its parents.
    ///
    /// # Errors
    ///
    /// A locale with rules whose list pattern, digits, separators or plural rules no locale of
    /// its chain states, or a numbering system that is not numeric.
    pub fn facts(&self, locale: &str) -> Result<Facts> {
        if locale == "root" {
            return Ok(Facts::default());
        }
        let chain = cldr::chain(locale, &self.parents);
        // The chain ends in root, whose index set is no language's alphabet.
        let letters = self
            .first(&chain[..chain.len() - 1], |main| main.index.as_deref())
            .map(alphabet)
            .transpose()?
            .flatten();
        let covered = chain
            .iter()
            .any(|id| id != "root" && self.rules.contains_key(id));
        let words = if covered {
            Some(self.words(locale, &chain)?)
        } else {
            None
        };
        Ok(Facts { letters, words })
    }

    /// The first value `take` gives along `chain`.
    fn first<'cldr>(
        &'cldr self,
        chain: &[String],
        take: impl Fn(&'cldr Main) -> Option<&'cldr str>,
    ) -> Option<&'cldr str> {
        chain
            .iter()
            .filter_map(|id| self.main.get(id))
            .find_map(take)
    }

    /// The rule set of the first of `names` the grouping `grouping` of `chain` holds.
    fn set(&self, chain: &[String], grouping: &str, names: &[&str]) -> Option<SetKey> {
        let (owner, sets) = chain.iter().find_map(|id| {
            self.rules
                .get(id)
                .and_then(|groupings| groupings.get(grouping))
                .map(|sets| (id, sets))
        })?;
        names
            .iter()
            .find(|name| sets.iter().any(|set| set.name == **name))
            .map(|name| (owner.clone(), grouping.to_owned(), (*name).to_owned()))
    }

    /// The decimal separator, else the group separator, of `system` along `chain`, an alias
    /// followed from the start of the chain.
    fn symbol(&self, chain: &[String], system: &str, decimal: bool, depth: u8) -> Option<&str> {
        for id in chain {
            match self.main.get(id).and_then(|main| main.symbols.get(system)) {
                Some(Symbols::Alias(target)) if depth < 8 => {
                    return self.symbol(chain, target, decimal, depth + 1);
                }
                Some(Symbols::Stated(point, group)) => {
                    if let Some(found) = if decimal { point } else { group } {
                        return Some(found);
                    }
                }
                Some(Symbols::Alias(_)) | None => {}
            }
        }
        None
    }

    fn words(&self, locale: &str, chain: &[String]) -> Result<Words> {
        let missing = |what: &str| format!("no locale of {locale}'s chain states {what}");
        let and = self
            .first(chain, |main| main.and.as_deref())
            .with_context(|| missing("a list pattern"))?;
        let numbering = self
            .first(chain, |main| main.numbering.as_deref())
            .with_context(|| missing("a numbering system"))?;
        let digits = self
            .numbering_systems
            .get(numbering)
            .with_context(|| format!("the numbering system {numbering} is not numeric"))?;
        let symbol = |decimal: bool| self.symbol(chain, numbering, decimal, 0);
        let plain = cldr::chain(locale, &BTreeMap::new());
        let plural = |rules: &BTreeMap<String, Vec<PluralRule>>| {
            plain
                .iter()
                .find_map(|id| rules.get(id))
                .cloned()
                .with_context(|| missing("plural rules"))
        };
        Ok(Words {
            and: and.to_owned(),
            cardinal: self.set(chain, "SpelloutRules", &CARDINAL),
            cardinal_plurals: plural(&self.cardinal_plurals)?,
            decimal: symbol(true)
                .with_context(|| missing("a decimal separator"))?
                .to_owned(),
            digits: digits.clone(),
            group: symbol(false)
                .with_context(|| missing("a group separator"))?
                .to_owned(),
            ordinal: self.set(chain, "OrdinalRules", &ORDINAL),
            ordinal_plurals: plural(&self.ordinal_plurals)?,
            ordinal_text: self.set(chain, "SpelloutRules", &ORDINAL_TEXT),
        })
    }
}

impl Table {
    /// The table of every locale `data` names, and of each format of `formats`, laid out as
    /// [`COUNTING`] is.
    ///
    /// # Errors
    ///
    /// What [`Cldr::facts`] refuses, or a rule set the evaluator cannot follow.
    pub fn build(data: &Cldr, formats: &[(&str, &str, &str, &str)]) -> Result<Self> {
        let ids: BTreeSet<&String> = data
            .main
            .keys()
            .chain(data.rules.keys())
            .filter(|id| *id != "root")
            .collect();
        let mut resolved = BTreeMap::new();
        for id in &ids {
            let parent = cldr::parent(id, &data.parents).unwrap_or_default();
            let own = data.facts(id)?;
            if own != data.facts(&parent)? {
                resolved.insert((*id).clone(), own);
            }
        }

        let mut keys = BTreeSet::new();
        for words in resolved.values().filter_map(|facts| facts.words.as_ref()) {
            keys.extend(
                [&words.cardinal, &words.ordinal, &words.ordinal_text]
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }
        let format_keys: Vec<(&str, SetKey)> = formats
            .iter()
            .map(|(variant, locale, grouping, name)| {
                (
                    *variant,
                    (
                        (*locale).to_owned(),
                        (*grouping).to_owned(),
                        (*name).to_owned(),
                    ),
                )
            })
            .collect();
        keys.extend(format_keys.iter().map(|(_, key)| key.clone()));
        let reached = reach(data, keys)?;
        let index_of: BTreeMap<&SetKey, usize> = reached
            .iter()
            .enumerate()
            .map(|(index, (key, _))| (key, index))
            .collect();
        let mut rule_sets = Vec::new();
        for (key, set) in &reached {
            rule_sets.push(indexed(set, key, &index_of)?);
        }

        let mut table = Self {
            formats: format_keys
                .iter()
                .map(|(variant, key)| {
                    index_of
                        .get(key)
                        .map(|index| ((*variant).to_owned(), *index))
                        .context("a rule set was not reached")
                })
                .collect::<Result<_>>()?,
            parents: data
                .parents
                .iter()
                .map(|(child, parent)| (child.clone(), parent.clone()))
                .collect(),
            rule_sets,
            ..Self::default()
        };
        for (id, facts) in resolved {
            let alphabet = facts
                .letters
                .map(|letters| position(&mut table.alphabets, letters));
            let words = facts
                .words
                .map(|words| {
                    let set = |key: &Option<SetKey>| -> Result<Option<usize>> {
                        key.as_ref()
                            .map(|found| {
                                index_of
                                    .get(found)
                                    .copied()
                                    .context("a rule set was not reached")
                            })
                            .transpose()
                    };
                    let sets = [
                        set(&words.cardinal)?,
                        set(&words.ordinal)?,
                        set(&words.ordinal_text)?,
                    ];
                    let plurals = [
                        position(&mut table.plurals, words.cardinal_plurals.clone()),
                        position(&mut table.plurals, words.ordinal_plurals.clone()),
                    ];
                    Ok::<_, anyhow::Error>(position(&mut table.words, (words, sets, plurals)))
                })
                .transpose()?;
            table.locales.push((id, alphabet, words));
        }
        Ok(table)
    }

    fn summary(&self) -> Summary {
        Summary {
            alphabets: self.alphabets.len(),
            locales: self.locales.len(),
            rule_sets: self.rule_sets.len(),
            with_words: self
                .locales
                .iter()
                .filter(|(_, _, words)| words.is_some())
                .count(),
        }
    }
}

/// The letters of an index exemplar set, upper case and lower case, when each is Latin and
/// they are other than A to Z; `None` for another script.
///
/// # Errors
///
/// A set that is not written `[...]`, a brace not closed, or a range.
pub fn alphabet(set: &str) -> Result<Option<(Vec<String>, Vec<String>)>> {
    let items = set_items(set)?;
    if items.is_empty() || !items.iter().all(|item| latin(item)) {
        return Ok(None);
    }
    let upper: Vec<String> = items.iter().map(|item| item.to_uppercase()).collect();
    if upper.iter().map(String::as_str).eq(UPPER_LETTERS) {
        return Ok(None);
    }
    let lower = items.iter().map(|item| item.to_lowercase()).collect();
    Ok(Some((upper, lower)))
}

/// Read every CLDR input and write the table; the counts it holds.
///
/// # Errors
///
/// An input that cannot be read or that a reader refuses, a rule set a rule names that its
/// grouping does not hold, a rule the evaluator cannot follow, or an output that cannot be
/// written.
pub fn emit<S>(system: &S, inputs: &Inputs) -> Result<Summary>
where
    S: System + ?Sized,
{
    let tree = |path: &Path| -> Result<Tree> {
        let text = ready_now(system.read_to_string(path))
            .with_context(|| format!("cannot read {}", path.display()))?;
        cldr::read(&text).with_context(|| format!("in {}", path.display()))
    };
    let mut data = Cldr {
        cardinal_plurals: plurals::read(&tree(&inputs.cldr_plurals)?)?,
        ordinal_plurals: plurals::read(&tree(&inputs.cldr_ordinals)?)?,
        parents: cldr::parent_locales(&tree(&inputs.cldr_supplemental)?)?,
        numbering_systems: numbering_systems(&tree(&inputs.cldr_numbering_systems)?),
        ..Cldr::default()
    };
    for path in &inputs.cldr_rbnf {
        let groupings =
            groupings(&tree(path)?).with_context(|| format!("in {}", path.display()))?;
        data.rules.insert(stem(path)?, groupings);
    }
    for path in &inputs.cldr_main {
        data.main.insert(stem(path)?, main(&tree(path)?));
    }
    let license = ready_now(system.read_to_string(&inputs.cldr_license))
        .with_context(|| format!("cannot read {}", inputs.cldr_license.display()))?;
    let copyright = license
        .lines()
        .find(|line| line.starts_with("Copyright"))
        .context("the CLDR LICENSE holds no copyright line")?;

    let table = Table::build(&data, &COUNTING)?;
    let mut read = vec![inputs.cldr_license.clone()];
    read.extend(inputs.cldr_main.iter().cloned());
    read.extend(inputs.cldr_rbnf.iter().cloned());
    read.extend([
        inputs.cldr_numbering_systems.clone(),
        inputs.cldr_ordinals.clone(),
        inputs.cldr_plurals.clone(),
        inputs.cldr_supplemental.clone(),
    ]);
    let header = namespaces::header(inputs.checksums(), inputs.tools(), &read, &[])?;
    let out = Path::new(PATH);
    if let Some(parent) = out.parent() {
        ready_now(system.create_dir_all(parent))
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    let text = render(&header, copyright, &table)?;
    ready_now(system.write(out, text.as_bytes(), Condition::None))
        .with_context(|| format!("cannot write {}", out.display()))?;
    Ok(table.summary())
}

/// The rule sets of each grouping of an `rbnf` file.
///
/// # Errors
///
/// A grouping with no type, or rules [`rules::read`] refuses.
pub fn groupings(file: &Tree) -> Result<BTreeMap<String, Vec<RuleSet>>> {
    let mut found = BTreeMap::new();
    let Some(rbnf) = file.child("rbnf") else {
        return Ok(found);
    };
    for grouping in rbnf.named("rulesetGrouping") {
        let kind = grouping
            .attribute("type")
            .context("a rulesetGrouping with no type")?;
        let text = grouping
            .child("rbnfRules")
            .map_or("", |rules| rules.text.as_str());
        found.insert(
            kind.to_owned(),
            rules::read(text).with_context(|| format!("in the grouping {kind}"))?,
        );
    }
    Ok(found)
}

/// Whether `item` is a letter of the Latin script with its marks.
fn latin(item: &str) -> bool {
    let mut characters = item.chars();
    characters.next().is_some_and(|first| {
        latin_letter(first)
            && characters.all(|next| latin_letter(next) || matches!(next, '\u{2b0}'..='\u{36f}'))
    })
}

/// Whether `character` is in a block of Latin letters: Basic Latin, Latin-1 Supplement, Latin
/// Extended-A to -E, IPA Extensions or Latin Extended Additional.
const fn latin_letter(character: char) -> bool {
    matches!(
        character,
        'A'..='Z'
            | 'a'..='z'
            | '\u{c0}'..='\u{d6}'
            | '\u{d8}'..='\u{f6}'
            | '\u{f8}'..='\u{2af}'
            | '\u{1e00}'..='\u{1eff}'
            | '\u{2c60}'..='\u{2c7f}'
            | '\u{a720}'..='\u{a7ff}'
            | '\u{ab30}'..='\u{ab6f}'
    )
}

/// What a main file states that the formats use.
#[must_use = "this only returns the value"]
pub fn main(file: &Tree) -> Main {
    let and = file
        .child("listPatterns")
        .and_then(|patterns| {
            patterns
                .named("listPattern")
                .find(|pattern| pattern.attribute("type").is_none() && pattern.counts())
        })
        .and_then(|pattern| {
            pattern
                .named("listPatternPart")
                .find(|part| part.attribute("type") == Some("2") && part.counts())
        })
        .map(|part| part.text.clone());
    let index = file.child("characters").and_then(|characters| {
        characters
            .named("exemplarCharacters")
            .find(|set| set.attribute("type") == Some("index") && set.counts())
            .map(|set| set.text.clone())
    });
    let numbers = file.child("numbers");
    let numbering = numbers
        .and_then(|found| found.child("defaultNumberingSystem"))
        .map(|system| system.text.clone());
    let mut symbols = BTreeMap::new();
    for found in numbers
        .into_iter()
        .flat_map(|list| list.named("symbols"))
        .filter(|found| found.counts())
    {
        let system = found.attribute("numberSystem").unwrap_or("latn").to_owned();
        let text = |name: &str| found.child(name).map(|symbol| symbol.text.clone());
        let aliased = found
            .child("alias")
            .and_then(|alias| alias.attribute("path"))
            .and_then(|path| path.strip_prefix("../symbols[@numberSystem='"))
            .and_then(|rest| rest.strip_suffix("']"));
        let stated = aliased.map_or_else(
            || Symbols::Stated(text("decimal"), text("group")),
            |target| Symbols::Alias(target.to_owned()),
        );
        symbols.insert(system, stated);
    }
    Main {
        and,
        index,
        numbering,
        symbols,
    }
}

/// The digits of each numeric numbering system of `numberingSystems.xml`.
#[must_use = "this only returns the value"]
pub fn numbering_systems(file: &Tree) -> BTreeMap<String, String> {
    file.child("numberingSystems")
        .into_iter()
        .flat_map(|systems| systems.named("numberingSystem"))
        .filter(|system| system.attribute("type") == Some("numeric"))
        .filter_map(|system| {
            Some((
                system.attribute("id")?.to_owned(),
                system.attribute("digits")?.to_owned(),
            ))
        })
        .collect()
}

/// The items of a set written `[a b {ch} \u200D]`: each character, or each string in braces.
fn set_items(set: &str) -> Result<Vec<String>> {
    let inner = set
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .with_context(|| format!("`{set}` is not a set"))?;
    let mut items = Vec::new();
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            '{' => {
                let mut item = String::new();
                loop {
                    match characters.next() {
                        Some('}') => break,
                        Some(found) => item.push(found),
                        None => bail!("a brace of `{set}` is not closed"),
                    }
                }
                items.push(item);
            }
            '\\' => {
                let escaped = characters.next().context("a set ends in `\\`")?;
                if escaped == 'u' {
                    let hex: String = characters.by_ref().take(4).collect();
                    let code = u32::from_str_radix(&hex, 16)
                        .with_context(|| format!("`\\u{hex}` in `{set}`"))?;
                    items.push(char::from_u32(code).context("not a character")?.to_string());
                } else {
                    items.push(escaped.to_string());
                }
            }
            '-' => bail!("`{set}` holds a range"),
            _ if character.is_whitespace() => {}
            _ => items.push(character.to_string()),
        }
    }
    Ok(items)
}

/// A file's name without its extension: its locale.
fn stem(path: &Path) -> Result<String> {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::to_owned)
        .with_context(|| format!("{} has no name", path.display()))
}

/// The rule set `set` of `key`, each rule set it names written `%index`, after checking that the
/// evaluator can follow each rule.
fn indexed(set: &RuleSet, key: &SetKey, index_of: &BTreeMap<&SetKey, usize>) -> Result<RuleSet> {
    let context = || format!("in {} of {}'s {}", key.2, key.0, key.1);
    let rename = |parts: &[Part]| -> Result<Vec<Part>> {
        map_parts(parts, &|name| {
            let named = (key.0.clone(), key.1.clone(), name.to_owned());
            index_of
                .get(&named)
                .map(|index| format!("%{index}"))
                .with_context(|| format!("{name} was not reached"))
        })
    };
    if let Some(negative) = &set.negative {
        for part in negative {
            if !matches!(part, Part::Text(_) | Part::Remainder(_)) {
                bail!(
                    "{}: the rule of a negative number holds {part:?}",
                    context()
                );
            }
        }
    }
    for rule in &set.rules {
        let mut flat = Vec::new();
        flatten(&rule.parts, &mut flat);
        for part in flat {
            match part {
                Part::Whole(Target::Same) => {
                    bail!("{}: `==` formats a number with its own rule", context())
                }
                Part::Preceding => bail!("{}: the evaluator does not follow `>>>`", context()),
                Part::Plural(kind, choices) => {
                    if !matches!(kind.as_str(), "cardinal" | "ordinal") {
                        bail!("{}: the plural kind {kind}", context());
                    }
                    for (category, _) in choices {
                        category_name(category)?;
                    }
                }
                Part::Optional(_, _)
                | Part::Quotient(_)
                | Part::Remainder(_)
                | Part::Text(_)
                | Part::Whole(_) => {}
            }
        }
    }
    Ok(RuleSet {
        name: set.name.clone(),
        negative: set.negative.as_deref().map(rename).transpose()?,
        rules: set
            .rules
            .iter()
            .map(|rule| {
                Ok(rules::Rule {
                    base: rule.base,
                    divisor: rule.divisor,
                    parts: rename(&rule.parts)?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
    })
}

/// The name of the `Category` variant of a plural category.
fn category_name(category: &str) -> Result<&'static str> {
    Ok(match category {
        "zero" => "Category::Zero",
        "one" => "Category::One",
        "two" => "Category::Two",
        "few" => "Category::Few",
        "many" => "Category::Many",
        "other" => "Category::Other",
        other => bail!("the plural category {other}"),
    })
}

/// Every part of `parts`, those in brackets included.
fn flatten<'parts>(parts: &'parts [Part], out: &mut Vec<&'parts Part>) {
    for part in parts {
        out.push(part);
        if let Part::Optional(first, second) = part {
            flatten(first, out);
            flatten(second, out);
        }
    }
}

/// `parts` with each rule set name made what `rename` gives.
fn map_parts(parts: &[Part], rename: &dyn Fn(&str) -> Result<String>) -> Result<Vec<Part>> {
    let target = |target: &Target| -> Result<Target> {
        Ok(match target {
            Target::Set(name) => Target::Set(rename(name)?),
            Target::Digits(_) | Target::Same => target.clone(),
        })
    };
    parts
        .iter()
        .map(|part| {
            Ok(match part {
                Part::Optional(first, second) => {
                    Part::Optional(map_parts(first, rename)?, map_parts(second, rename)?)
                }
                Part::Quotient(found) => Part::Quotient(target(found)?),
                Part::Remainder(found) => Part::Remainder(target(found)?),
                Part::Whole(found) => Part::Whole(target(found)?),
                Part::Plural(_, _) | Part::Preceding | Part::Text(_) => part.clone(),
            })
        })
        .collect()
}

/// The index of `value` in `list`, pushed when it is not there.
fn position<T>(list: &mut Vec<T>, value: T) -> usize
where
    T: PartialEq,
{
    list.iter()
        .position(|found| *found == value)
        .unwrap_or_else(|| {
            list.push(value);
            list.len() - 1
        })
}

/// Every rule set `keys` name and those their rules name, sorted by key.
fn reach(data: &Cldr, keys: BTreeSet<SetKey>) -> Result<BTreeMap<SetKey, RuleSet>> {
    let mut reached = BTreeMap::new();
    let mut pending: Vec<SetKey> = keys.into_iter().collect();
    while let Some(key) = pending.pop() {
        if reached.contains_key(&key) {
            continue;
        }
        let set = data
            .rules
            .get(&key.0)
            .and_then(|groupings| groupings.get(&key.1))
            .and_then(|sets| sets.iter().find(|set| set.name == key.2))
            .with_context(|| format!("{}'s {} holds no rule set {}", key.0, key.1, key.2))?;
        let mut flat = Vec::new();
        for rule in &set.rules {
            flatten(&rule.parts, &mut flat);
        }
        if let Some(negative) = &set.negative {
            flatten(negative, &mut flat);
        }
        for part in flat {
            if let Part::Quotient(Target::Set(name))
            | Part::Remainder(Target::Set(name))
            | Part::Whole(Target::Set(name)) = part
            {
                pending.push((key.0.clone(), key.1.clone(), name.clone()));
            }
        }
        reached.insert(key, set.clone());
    }
    Ok(reached)
}

/// The table's text: `header`, the CLDR notice, then each static.
///
/// # Errors
///
/// A plural category or a pattern the table cannot name.
pub fn render(header: &str, copyright: &str, table: &Table) -> Result<String> {
    let mut out = format!(
        "{header}{UNICODE_NOTICE}\n\n//! Unicode CLDR {version}'s rule-based number formats, \
         plural rules, list patterns, digits,\n//! separators and alphabets. {copyright} Unicode License v3.\n\n\
         use officekit_markup::wordprocessing::StNumberFormatMember;\n\n\
         use super::{{\n    Category, Letters, Locale, Part, Pattern, PluralKind, PluralRule, \
         Relation, Rule, RuleSet,\n    Target, Words,\n}};\n",
        version = cldr::VERSION,
    );
    let alphabets: Vec<Expr> = table.alphabets.iter().map(alphabet_expr).collect();
    static_table(
        &mut out,
        "The alphabets of `upperLetter` and `lowerLetter` other than A to Z.",
        "ALPHABETS: [Letters",
        &alphabets,
    );
    let formats: Vec<Expr> = table
        .formats
        .iter()
        .map(|(variant, set)| {
            Expr::Tuple(vec![
                Expr::atom(&format!("StNumberFormatMember::{variant}")),
                Expr::atom(&set.to_string()),
            ])
        })
        .collect();
    static_table(
        &mut out,
        "Each format section 17.18.59 spells only to a limit, and the rule set past it.",
        "FORMATS: [(StNumberFormatMember, usize)",
        &formats,
    );
    let locales: Vec<Expr> = table.locales.iter().map(locale_expr).collect();
    static_table(
        &mut out,
        "Each locale whose facts differ from its parent's, sorted by id.",
        "LOCALES: [Locale",
        &locales,
    );
    let parents: Vec<Expr> = table
        .parents
        .iter()
        .map(|(child, parent)| {
            Expr::Tuple(vec![
                Expr::atom(&layout::string(child)),
                Expr::atom(&layout::string(parent)),
            ])
        })
        .collect();
    static_table(
        &mut out,
        "Each locale a parent locale rule names, and the parent it names, sorted by locale.",
        "PARENTS: [(&str, &str)",
        &parents,
    );
    let patterns = patterns(&table.rule_sets);
    let shown: Vec<Expr> = patterns.iter().map(|found| pattern_expr(*found)).collect();
    static_table(
        &mut out,
        "The decimal patterns the rules name.",
        "PATTERNS: [Pattern",
        &shown,
    );
    let plurals = table
        .plurals
        .iter()
        .map(|list| {
            Ok(Expr::Slice(
                list.iter()
                    .map(plural_expr)
                    .collect::<Result<Vec<Expr>>>()?,
            ))
        })
        .collect::<Result<Vec<Expr>>>()?;
    static_table(
        &mut out,
        "The plural rules, each list in the order its locale gives them.",
        "PLURALS: [&[PluralRule]",
        &plurals,
    );
    let sets = table
        .rule_sets
        .iter()
        .map(|set| set_expr(set, &patterns))
        .collect::<Result<Vec<Expr>>>()?;
    static_table(
        &mut out,
        "The rule sets the formats reach.",
        "RULE_SETS: [RuleSet",
        &sets,
    );
    let words: Vec<Expr> = table.words.iter().map(words_expr).collect();
    static_table(
        &mut out,
        "The words of each locale CLDR has rules for.",
        "WORDS: [Words",
        &words,
    );
    Ok(out)
}

/// One entry of `ALPHABETS`.
fn alphabet_expr((upper, lower): &(Vec<String>, Vec<String>)) -> Expr {
    Expr::Struct(
        "Letters".to_owned(),
        vec![
            ("lower".to_owned(), strings(lower)),
            ("upper".to_owned(), strings(upper)),
        ],
    )
}

/// One entry of `LOCALES`.
fn locale_expr((id, alphabet, words): &(String, Option<usize>, Option<usize>)) -> Expr {
    Expr::Struct(
        "Locale".to_owned(),
        vec![
            ("alphabet".to_owned(), index(*alphabet)),
            ("id".to_owned(), Expr::atom(&layout::string(id))),
            ("words".to_owned(), index(*words)),
        ],
    )
}

/// `Some(index)`, or `None`.
fn index(found: Option<usize>) -> Expr {
    found.map_or_else(
        || Expr::atom("None"),
        |at| Expr::call("Some", vec![Expr::atom(&at.to_string())]),
    )
}

/// A static array named and typed by `head`, `NAME: [Type`, holding `items`.
fn static_table(out: &mut String, doc: &str, head: &str, items: &[Expr]) {
    let _: fmt::Result = write!(
        out,
        "\n/// {doc}\npub(super) static {head}; {}] = [\n",
        items.len()
    );
    for item in items {
        layout::line("    ", item, ",", out);
    }
    out.push_str("];\n");
}

/// A slice of string literals.
fn strings(list: &[String]) -> Expr {
    Expr::Slice(
        list.iter()
            .map(|text| Expr::atom(&layout::string(text)))
            .collect(),
    )
}

/// One entry of `WORDS`.
fn words_expr((words, sets, plurals): &(Words, [Option<usize>; 3], [usize; 2])) -> Expr {
    let [cardinal, ordinal, ordinal_text] = *sets;
    let [cardinal_plurals, ordinal_plurals] = *plurals;
    let text = |value: &str| Expr::atom(&layout::string(value));
    Expr::Struct(
        "Words".to_owned(),
        vec![
            ("and".to_owned(), text(&words.and)),
            ("cardinal".to_owned(), index(cardinal)),
            (
                "cardinal_plurals".to_owned(),
                Expr::atom(&cardinal_plurals.to_string()),
            ),
            ("decimal".to_owned(), text(&words.decimal)),
            ("digits".to_owned(), text(&words.digits)),
            ("group".to_owned(), text(&words.group)),
            ("ordinal".to_owned(), index(ordinal)),
            (
                "ordinal_plurals".to_owned(),
                Expr::atom(&ordinal_plurals.to_string()),
            ),
            ("ordinal_text".to_owned(), index(ordinal_text)),
        ],
    )
}

/// The expression of a list of parts, each pattern an index of `patterns`.
fn parts_expr(parts: &[Part], patterns: &[Pattern]) -> Result<Expr> {
    Ok(Expr::Slice(
        parts
            .iter()
            .map(|part| part_expr(part, patterns))
            .collect::<Result<Vec<Expr>>>()?,
    ))
}

fn part_expr(part: &Part, patterns: &[Pattern]) -> Result<Expr> {
    let target = |found: &Target| target_expr(found, patterns);
    Ok(match part {
        Part::Optional(first, second) => Expr::call(
            "Part::Optional",
            vec![parts_expr(first, patterns)?, parts_expr(second, patterns)?],
        ),
        Part::Plural(kind, choices) => {
            let shown_kind = if kind == "ordinal" {
                "PluralKind::Ordinal"
            } else {
                "PluralKind::Cardinal"
            };
            let shown = choices
                .iter()
                .map(|(category, text)| {
                    Ok(Expr::Tuple(vec![
                        Expr::atom(category_name(category)?),
                        Expr::atom(&layout::string(text)),
                    ]))
                })
                .collect::<Result<Vec<Expr>>>()?;
            Expr::call(
                "Part::Plural",
                vec![Expr::atom(shown_kind), Expr::Slice(shown)],
            )
        }
        Part::Preceding => bail!("`>>>` has no part in the table"),
        Part::Quotient(found) => Expr::call("Part::Quotient", vec![target(found)?]),
        Part::Remainder(found) => Expr::call("Part::Remainder", vec![target(found)?]),
        Part::Text(text) => Expr::call("Part::Text", vec![Expr::atom(&layout::string(text))]),
        Part::Whole(found) => Expr::call("Part::Whole", vec![target(found)?]),
    })
}

/// The distinct decimal patterns of `sets`, sorted.
fn patterns(sets: &[RuleSet]) -> Vec<Pattern> {
    let mut found = BTreeSet::new();
    for set in sets {
        let mut flat = Vec::new();
        for rule in &set.rules {
            flatten(&rule.parts, &mut flat);
        }
        if let Some(negative) = &set.negative {
            flatten(negative, &mut flat);
        }
        for part in flat {
            if let Part::Quotient(Target::Digits(pattern))
            | Part::Remainder(Target::Digits(pattern))
            | Part::Whole(Target::Digits(pattern)) = part
            {
                found.insert(*pattern);
            }
        }
    }
    found.into_iter().collect()
}

fn pattern_expr(pattern: Pattern) -> Expr {
    Expr::Struct(
        "Pattern".to_owned(),
        vec![
            (
                "fraction".to_owned(),
                Expr::atom(&pattern.fraction.to_string()),
            ),
            (
                "minimum".to_owned(),
                Expr::atom(&pattern.minimum.to_string()),
            ),
            ("point".to_owned(), Expr::atom(&pattern.point.to_string())),
            (
                "primary".to_owned(),
                Expr::atom(&pattern.primary.to_string()),
            ),
            (
                "secondary".to_owned(),
                Expr::atom(&pattern.secondary.to_string()),
            ),
        ],
    )
}

fn plural_expr(rule: &PluralRule) -> Result<Expr> {
    let relation = |found: &Relation| {
        let modulus = found.modulus.map_or_else(
            || Expr::atom("None"),
            |value| Expr::call("Some", vec![Expr::atom(&layout::integer(value.into()))]),
        );
        let ranges = found
            .ranges
            .iter()
            .map(|(first, last)| {
                Expr::Tuple(vec![
                    Expr::atom(&layout::integer((*first).into())),
                    Expr::atom(&layout::integer((*last).into())),
                ])
            })
            .collect();
        Expr::Struct(
            "Relation".to_owned(),
            vec![
                ("equal".to_owned(), Expr::atom(&found.equal.to_string())),
                ("modulus".to_owned(), modulus),
                ("ranges".to_owned(), Expr::Slice(ranges)),
            ],
        )
    };
    Ok(Expr::Struct(
        "PluralRule".to_owned(),
        vec![
            (
                "category".to_owned(),
                Expr::atom(category_name(&rule.category)?),
            ),
            (
                "conditions".to_owned(),
                Expr::Slice(
                    rule.conditions
                        .iter()
                        .map(|relations| Expr::Slice(relations.iter().map(relation).collect()))
                        .collect(),
                ),
            ),
        ],
    ))
}

fn set_expr(set: &RuleSet, patterns: &[Pattern]) -> Result<Expr> {
    let negative = set
        .negative
        .as_deref()
        .map(|parts| parts_expr(parts, patterns))
        .transpose()?
        .map_or_else(
            || Expr::atom("None"),
            |parts| Expr::call("Some", vec![parts]),
        );
    let rules = set
        .rules
        .iter()
        .map(|rule| {
            Ok(Expr::call(
                "Rule::new",
                vec![
                    Expr::atom(&layout::integer(rule.base.into())),
                    Expr::atom(&layout::integer(rule.divisor.into())),
                    parts_expr(&rule.parts, patterns)?,
                ],
            ))
        })
        .collect::<Result<Vec<Expr>>>()?;
    Ok(Expr::Struct(
        "RuleSet".to_owned(),
        vec![
            ("negative".to_owned(), negative),
            ("rules".to_owned(), Expr::Slice(rules)),
        ],
    ))
}

fn target_expr(target: &Target, patterns: &[Pattern]) -> Result<Expr> {
    Ok(match target {
        Target::Digits(pattern) => {
            let index = patterns
                .iter()
                .position(|found| found == pattern)
                .context("a pattern was not listed")?;
            Expr::call("Target::Digits", vec![Expr::atom(&index.to_string())])
        }
        Target::Same => Expr::atom("Target::Same"),
        Target::Set(name) => {
            let index: usize = name
                .strip_prefix('%')
                .and_then(|index| index.parse().ok())
                .with_context(|| format!("the rule set {name} has no index"))?;
            Expr::call("Target::Set", vec![Expr::atom(&index.to_string())])
        }
    })
}
