//! Case mapping in the run's language: the mappings the Unicode Character Database's
//! `SpecialCasing.txt` gives a language, where their contexts hold (The Unicode Standard,
//! section 3.13).

mod special_casing;
#[cfg(test)]
mod tests;

use alloc::string::String;
use core::cmp::Ordering;

use self::special_casing::{CASE_BY_LANGUAGE, COMBINING_CLASSES, SOFT_DOTTED};

/// The canonical combining class `Above`.
const ABOVE: u8 = 230;

const COMBINING_DOT_ABOVE: char = '\u{307}';

/// A mapping of one character in one language, applying where each of its contexts holds as given.
#[derive(Debug)]
struct Casing {
    character: char,
    /// Each context and whether it must hold, `false` for `SpecialCasing.txt`'s `Not_`.
    contexts: &'static [(Context, bool)],
    /// The language subtag, lower case.
    language: &'static str,
    lower: &'static str,
    upper: &'static str,
}

/// A casing context (The Unicode Standard, section 3.13, table 3-17).
#[derive(Clone, Copy, Debug)]
enum Context {
    /// An `I` before the character, with no character of class 0 or 230 between.
    AfterI,
    /// A `Soft_Dotted` character before it, with no character of class 0 or 230 between.
    AfterSoftDotted,
    /// U+0307 after it, with no character of class 0 or 230 between.
    BeforeDot,
    /// A character of class 230 after it, with no character of class 0 between.
    MoreAbove,
}

impl Context {
    /// Whether the context holds for the character between `before` and `after`.
    fn holds(self, before: &str, after: &str) -> bool {
        match self {
            Self::AfterI => blocking(before.chars().rev()) == Some('I'),
            Self::AfterSoftDotted => blocking(before.chars().rev()).is_some_and(soft_dotted),
            Self::BeforeDot => blocking(after.chars()) == Some(COMBINING_DOT_ABOVE),
            Self::MoreAbove => {
                blocking(after.chars()).is_some_and(|found| combining_class(found) == ABOVE)
            }
        }
    }
}

/// The first of `characters` whose canonical combining class is 0 or 230.
fn blocking(mut characters: impl Iterator<Item = char>) -> Option<char> {
    characters.find(|found| matches!(combining_class(*found), 0 | ABOVE))
}

/// `text` with its first character in upper case in `language`.
pub(super) fn capitalized(text: &str, language: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        let rest = characters.as_str();
        let mut out = String::with_capacity(text.len());
        match mapping(first, language, "", rest) {
            Some(found) => out.push_str(found.upper),
            None => out.extend(first.to_uppercase()),
        }
        out.push_str(rest);
        out
    })
}

fn combining_class(character: char) -> u8 {
    COMBINING_CLASSES
        .binary_search_by(|(first, last, _)| {
            if *last < character {
                Ordering::Less
            } else if *first > character {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .and_then(|at| COMBINING_CLASSES.get(at))
        .map_or(0, |(_, _, class)| *class)
}

/// The language subtag of the tag `language`, such as `tr` of `tr-TR`.
fn language_subtag(language: &str) -> &str {
    language.split(['-', '_']).next().unwrap_or_default()
}

/// `text` in lower case in `language`.
pub(super) fn lowered(text: &str, language: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    let mut before = "";
    while let Some(character) = characters.next() {
        let after = characters.as_str();
        match mapping(character, language, before, after) {
            Some(found) => out.push_str(found.lower),
            None => out.extend(character.to_lowercase()),
        }
        before = &text[..text.len() - after.len()];
    }
    out
}

/// The mapping of `character`, between `before` and `after`, in `language`.
fn mapping(character: char, language: &str, before: &str, after: &str) -> Option<&'static Casing> {
    let subtag = language_subtag(language);
    CASE_BY_LANGUAGE.iter().find(|found| {
        found.character == character
            && found.language.eq_ignore_ascii_case(subtag)
            && found
                .contexts
                .iter()
                .all(|(context, wanted)| context.holds(before, after) == *wanted)
    })
}

fn soft_dotted(character: char) -> bool {
    SOFT_DOTTED
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&character))
}

/// Whether `SpecialCasing.txt` gives `language`, a tag such as `tr-TR`, mappings of its own.
pub(super) fn tailored(language: &str) -> bool {
    let subtag = language_subtag(language);
    CASE_BY_LANGUAGE
        .iter()
        .any(|found| found.language.eq_ignore_ascii_case(subtag))
}
