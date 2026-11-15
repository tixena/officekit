//! Numbers in words and the alphabets of the run's language, and the formats ECMA-376 spells only
//! to a limit past it, from Unicode CLDR's rule-based number formats, plural rules, list patterns
//! and index exemplars (UTS #35, Parts 2 and 3).

mod locales;
#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;
use core::iter;

use officekit_markup::wordprocessing::StNumberFormatMember;

use self::locales::{ALPHABETS, FORMATS, LOCALES, PARENTS, PATTERNS, PLURALS, RULE_SETS, WORDS};
use crate::docx::fonts::likely_subtags;

/// Past this many rule sets within one another a number is left unwritten.
const DEEPEST: u8 = 64;

/// A plural category (UTS #35, Part 3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Category {
    Few,
    Many,
    One,
    Other,
    Two,
    Zero,
}

/// The letters of `upperLetter` and `lowerLetter` in a language's alphabet.
#[derive(Debug)]
struct Letters {
    lower: &'static [&'static str],
    upper: &'static [&'static str],
}

/// A locale whose facts differ from its parent's, each an index of its table.
#[derive(Debug)]
struct Locale {
    alphabet: Option<usize>,
    /// The CLDR id, such as `es_419`.
    id: &'static str,
    words: Option<usize>,
}

/// One piece of a rule's text.
#[derive(Debug)]
enum Part {
    /// `[a|b]`: `a` when the number is not a multiple of the rule's divisor, else `b`.
    Optional(&'static [Self], &'static [Self]),
    /// The text of the plural category of the number divided by the rule's divisor.
    Plural(PluralKind, &'static [(Category, &'static str)]),
    /// `<<`: the number divided by the divisor, rounded down.
    Quotient(Target),
    /// `>>`: the remainder; in the rule of a negative number, the number without its sign.
    Remainder(Target),
    Text(&'static str),
    /// `==`: the number itself.
    Whole(Target),
}

/// A decimal pattern such as `#,##0`, for a whole number.
#[derive(Debug)]
struct Pattern {
    /// The zeros after the decimal separator.
    fraction: u8,
    minimum: u8,
    /// Whether the decimal separator ends the number though no fraction follows.
    point: bool,
    /// The digits of the group nearest the separator; 0 for no grouping.
    primary: u8,
    secondary: u8,
}

#[derive(Clone, Copy, Debug)]
enum PluralKind {
    Cardinal,
    Ordinal,
}

/// A category's rule: it applies when each relation of one of its conditions holds.
#[derive(Debug)]
struct PluralRule {
    category: Category,
    conditions: &'static [&'static [Relation]],
}

/// Where a rule's parts are written: its rule set, the number and the divisor.
#[derive(Clone, Copy, Debug)]
struct Place {
    divisor: u64,
    /// Whether the rule is that of a negative number.
    negative: bool,
    set: usize,
    value: u64,
}

/// `n % modulus` in `ranges`, or not in them when not `equal`.
#[derive(Debug)]
struct Relation {
    equal: bool,
    modulus: Option<u64>,
    ranges: &'static [(u64, u64)],
}

impl Relation {
    fn holds(&self, number: u64) -> bool {
        let operand = self
            .modulus
            .and_then(|modulus| number.checked_rem(modulus))
            .unwrap_or(number);
        self.ranges
            .iter()
            .any(|(first, last)| (*first..=*last).contains(&operand))
            == self.equal
    }
}

/// One rule for whole numbers from `base`.
#[derive(Debug)]
struct Rule {
    base: u64,
    /// The radix raised to the highest power that does not pass `base`.
    divisor: u64,
    parts: &'static [Part],
}

impl Rule {
    const fn new(base: u64, divisor: u64, parts: &'static [Part]) -> Self {
        Self {
            base,
            divisor,
            parts,
        }
    }

    /// ICU's rollback: a rule with a remainder, in brackets or not, whose base is not a multiple
    /// of its divisor gives way to the rule before it for a number that is.
    fn rolls_back(&self, value: u64) -> bool {
        holds_remainder(self.parts)
            && !self.base.is_multiple_of(self.divisor)
            && value.is_multiple_of(self.divisor)
    }
}

#[derive(Debug)]
struct RuleSet {
    /// The parts of the rule of a negative number.
    negative: Option<&'static [Part]>,
    /// The rules of whole numbers, by base.
    rules: &'static [Rule],
}

/// Writes numbers by the rule sets, in the digits and plural categories of `words`; without
/// words, a number a rule writes in digits or by a plural is left unwritten.
#[derive(Debug)]
struct Speller {
    words: Option<&'static Words>,
}

impl Speller {
    /// The text of the rule at `at` of the rule set `set` for `value`.
    fn rule(&self, set: usize, at: usize, value: u64, depth: u8) -> Option<String> {
        let rule = RULE_SETS.get(set)?.rules.get(at)?;
        let place = Place {
            divisor: rule.divisor,
            negative: false,
            set,
            value,
        };
        let mut text = String::new();
        self.write(rule.parts, place, depth, &mut text)?;
        Some(text)
    }

    /// `value`, negative when `negative`, by the rule set `set`.
    fn set(&self, set: usize, negative: bool, value: u64, depth: u8) -> Option<String> {
        if depth > DEEPEST {
            return None;
        }
        let rules = RULE_SETS.get(set)?;
        if negative {
            let place = Place {
                divisor: 1,
                negative: true,
                set,
                value,
            };
            let mut text = String::new();
            self.write(rules.negative?, place, depth, &mut text)?;
            return Some(text);
        }
        let mut at = rules
            .rules
            .partition_point(|rule| rule.base <= value)
            .checked_sub(1)?;
        if at > 0 && rules.rules.get(at)?.rolls_back(value) {
            at -= 1;
        }
        self.rule(set, at, value, depth)
    }

    /// `number` written as `target` says, from the rule set `set`.
    fn target(&self, target: Target, set: usize, number: u64, depth: u8) -> Option<String> {
        match target {
            Target::Digits(pattern) => Some(self.words?.digits(number, PATTERNS.get(pattern)?)),
            Target::Same => self.set(set, false, number, depth + 1),
            Target::Set(other) => self.set(other, false, number, depth + 1),
        }
    }

    fn write(&self, parts: &[Part], place: Place, depth: u8, text: &mut String) -> Option<()> {
        let quotient = place.value / place.divisor;
        let remainder = place.value % place.divisor;
        for part in parts {
            match part {
                Part::Optional(first, second) => {
                    let chosen = if remainder == 0 { second } else { first };
                    self.write(chosen, place, depth, text)?;
                }
                Part::Plural(kind, choices) => {
                    let category = self.words?.category(*kind, quotient);
                    let (_, chosen) = choices
                        .iter()
                        .find(|(found, _)| *found == category)
                        .or_else(|| choices.iter().find(|(found, _)| *found == Category::Other))?;
                    text.push_str(chosen);
                }
                Part::Quotient(target) => {
                    text.push_str(&self.target(*target, place.set, quotient, depth)?);
                }
                Part::Remainder(target) => {
                    let number = if place.negative {
                        place.value
                    } else {
                        remainder
                    };
                    text.push_str(&self.target(*target, place.set, number, depth)?);
                }
                Part::Text(found) => text.push_str(found),
                Part::Whole(target) => {
                    text.push_str(&self.target(*target, place.set, place.value, depth)?);
                }
            }
        }
        Some(())
    }
}

/// The format a number is spelled in.
#[derive(Clone, Copy, Debug)]
pub(super) enum Spelled {
    /// `cardinalText`.
    Cardinal,
    /// `ordinal`.
    Ordinal,
    /// `ordinalText`.
    OrdinalText,
}

/// How a substitution writes its number.
#[derive(Clone, Copy, Debug)]
enum Target {
    /// In digits, by the pattern of that index.
    Digits(usize),
    /// With the rule set of the rule.
    Same,
    /// With the rule set of that index.
    Set(usize),
}

/// The formats in words and the facts they use, of a locale CLDR has rules for.
#[derive(Debug)]
struct Words {
    /// The pattern of a list of two items, `{0}` and `{1}` its places.
    and: &'static str,
    cardinal: Option<usize>,
    cardinal_plurals: usize,
    decimal: &'static str,
    /// The digits 0 to 9.
    digits: &'static str,
    group: &'static str,
    ordinal: Option<usize>,
    ordinal_plurals: usize,
    ordinal_text: Option<usize>,
}

impl Words {
    fn category(&self, kind: PluralKind, number: u64) -> Category {
        let index = match kind {
            PluralKind::Cardinal => self.cardinal_plurals,
            PluralKind::Ordinal => self.ordinal_plurals,
        };
        PLURALS
            .get(index)
            .and_then(|rules| {
                rules.iter().find(|rule| {
                    rule.conditions
                        .iter()
                        .any(|relations| relations.iter().all(|relation| relation.holds(number)))
                })
            })
            .map_or(Category::Other, |rule| rule.category)
    }

    /// `number` in the locale's digits, as `pattern` groups it.
    fn digits(&self, number: u64, pattern: &Pattern) -> String {
        let digits: Vec<char> = self.digits.chars().collect();
        let mut plain = number.to_string();
        let minimum = usize::from(pattern.minimum);
        if plain.len() < minimum {
            plain.insert_str(0, &"0".repeat(minimum - plain.len()));
        }
        let primary = usize::from(pattern.primary);
        let secondary = usize::from(pattern.secondary);
        let mut text = String::new();
        for (index, digit) in plain.bytes().enumerate() {
            let remaining = plain.len() - index;
            let boundary = primary > 0
                && index > 0
                && (remaining == primary
                    || (remaining > primary
                        && secondary > 0
                        && (remaining - primary).is_multiple_of(secondary)));
            if boundary {
                text.push_str(self.group);
            }
            text.push(
                digits
                    .get(usize::from(digit - b'0'))
                    .copied()
                    .unwrap_or_else(|| char::from(digit)),
            );
        }
        if pattern.fraction > 0 || pattern.point {
            text.push_str(self.decimal);
        }
        let zero = digits.first().copied().unwrap_or('0');
        text.extend(iter::repeat_n(zero, usize::from(pattern.fraction)));
        text
    }
}

/// The letters of `upperLetter`, else of `lowerLetter`, in `language`'s alphabet; `None` when
/// it is not Latin or is A to Z.
pub(super) fn alphabet(language: &str, upper: bool) -> Option<&'static [&'static str]> {
    let letters = ALPHABETS.get(locale(language)?.alphabet?)?;
    Some(if upper { letters.upper } else { letters.lower })
}

/// `value` in `format` past the values section 17.18.59 spells out, by the CLDR rule set
/// [`FORMATS`] names; `None` for another format, and for a value the set writes in digits.
pub(super) fn counted(value: u64, format: StNumberFormatMember) -> Option<String> {
    let (_, set) = FORMATS.iter().find(|(found, _)| *found == format)?;
    Speller { words: None }.set(*set, false, value, 0)
}

/// `dollarText`: the words of `value`, then "and 00/100" by the language's list pattern.
pub(super) fn dollars(value: i64, language: &str) -> Option<String> {
    let found = words(language)?;
    let cardinal = spelled(value, Spelled::Cardinal, language)?;
    Some(found.and.replace("{1}", "00/100").replace("{0}", &cardinal))
}

/// Whether `parts`, those in brackets included, write the remainder.
fn holds_remainder(parts: &[Part]) -> bool {
    parts.iter().any(|part| match part {
        Part::Remainder(_) => true,
        Part::Optional(first, second) => holds_remainder(first) || holds_remainder(second),
        Part::Plural(..) | Part::Quotient(_) | Part::Text(_) | Part::Whole(_) => false,
    })
}

/// The script of the full id likely subtags give the CLDR id `id`.
fn likely_script(id: &str) -> Option<&'static str> {
    likely_subtags(id)?.split('_').nth(1)
}

/// The nearest locale of `language`'s chain that the table holds: the tag, with the script
/// [`with_likely_script`] gives it, then each parent.
fn locale(language: &str) -> Option<&'static Locale> {
    let mut id = with_likely_script(normalized(language)?);
    loop {
        if let Ok(at) = LOCALES.binary_search_by(|found| found.id.cmp(id.as_str())) {
            return LOCALES.get(at);
        }
        id = match PARENTS.binary_search_by(|(child, _)| (*child).cmp(id.as_str())) {
            Ok(at) => PARENTS.get(at)?.1.to_owned(),
            Err(_) => id.rsplit_once('_')?.0.to_owned(),
        };
        if id == "root" {
            return None;
        }
    }
}

/// A language tag such as `en-US` as a CLDR id, `en_US`: its language, script and region.
fn normalized(tag: &str) -> Option<String> {
    let mut subtags = tag.split(['-', '_']);
    let language = subtags.next()?;
    if !(2..=8).contains(&language.len())
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return None;
    }
    let mut id = language.to_ascii_lowercase();
    for subtag in subtags {
        let alphabetic = subtag.bytes().all(|byte| byte.is_ascii_alphabetic());
        if subtag.len() == 4 && alphabetic {
            id.push('_');
            let (first, rest) = subtag.split_at(1);
            id.push_str(&first.to_ascii_uppercase());
            id.push_str(&rest.to_ascii_lowercase());
        } else if (subtag.len() == 2 && alphabetic)
            || (subtag.len() == 3 && subtag.bytes().all(|byte| byte.is_ascii_digit()))
        {
            id.push('_');
            id.push_str(&subtag.to_ascii_uppercase());
        } else {
            break;
        }
    }
    Some(id)
}

/// `value` in `kind` in `language`, as CLDR spells it; `None` for a language it has no rules
/// of that kind for.
pub(super) fn spelled(value: i64, kind: Spelled, language: &str) -> Option<String> {
    let found = words(language)?;
    let set = match kind {
        Spelled::Cardinal => found.cardinal,
        Spelled::Ordinal => found.ordinal,
        Spelled::OrdinalText => found.ordinal_text,
    }?;
    Speller { words: Some(found) }.set(set, value < 0, value.unsigned_abs(), 0)
}

fn words(language: &str) -> Option<&'static Words> {
    WORDS.get(locale(language)?.words?)
}

/// The CLDR id `id` with the script Unicode CLDR's likely subtags give its language and region
/// (UTS #35, Part 1, "Likely Subtags"), when it names none and that is not its language's script.
fn with_likely_script(id: String) -> String {
    let mut subtags = id.split('_');
    let (Some(language), Some(region)) = (subtags.next(), subtags.next()) else {
        return id;
    };
    if region.len() == 4 {
        return id;
    }
    let likely = likely_script(&format!("{language}_{region}"));
    match likely {
        Some(found) if likely != likely_script(language) => {
            let rest = &id[language.len()..];
            format!("{language}_{found}{rest}")
        }
        _ => id,
    }
}
