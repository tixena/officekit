//! The system long date and the system time of a `[$-<language info>]` (ECMA-376 Part 1, section
//! 18.8.31, "International Considerations"), written by Unicode CLDR's long date and medium time
//! patterns of the caller's language.

use core::mem;

use super::super::Section;
use super::super::parse::Token;
use super::names::{HALVES, PATTERNS, SYSTEM};

#[cfg(test)]
mod tests;

/// The language info section 18.8.31 gives the system long date.
const LONG_DATE: u16 = 0xF800;

/// The language info section 18.8.31 gives the system time.
const TIME: u16 = 0xF400;

/// Whether the language id `id` is section 18.8.31's system long date or system time.
pub(in super::super) const fn is_system_format(id: u16) -> bool {
    matches!(id, LONG_DATE | TIME)
}

/// The section that writes `section`'s system long date or time, by the patterns of CLDR's
/// locale at `locale` in [`SYSTEM`], in `section`'s colour.
pub(super) fn section(section: &Section, locale: usize) -> Option<Section> {
    let [date, time, halves] = SYSTEM.get(locale)?;
    let at = if section.language? == LONG_DATE {
        date
    } else {
        time
    };
    let pattern = PATTERNS.get(usize::from(*at))?;
    let [am, pm] = HALVES.get(usize::from(*halves))?;
    Some(Section {
        color: section.color,
        dates: true,
        tokens: tokens(pattern, &format!("{am}/{pm}")),
        ..Section::default()
    })
}

/// The section's own codes for a run of `length` of the pattern letter `letter` (UTS #35, Part 4,
/// "Date Field Symbol Table"); `None` for a letter they cannot write: an era, a quarter, a week,
/// a day of the year or of the week in the month, a fraction of a second, a zone.
fn code(letter: char, length: usize, halves: &str) -> Option<Token> {
    let token = match letter {
        'y' => Token::Date('y', if length == 2 { 2 } else { 4 }),
        'M' | 'L' => Token::Date('m', length.min(5)),
        'd' => Token::Date('d', length.min(2)),
        'E' => Token::Date('d', if length == 4 { 4 } else { 3 }),
        'a' | 'b' | 'B' => Token::AmPm(halves.to_owned()),
        'h' | 'H' | 'K' | 'k' => Token::Date('h', length.min(2)),
        'm' => Token::Date('m', length.min(2)),
        's' => Token::Date('s', length.min(2)),
        _ => return None,
    };
    Some(token)
}

/// `pattern`'s letters as [`code`] writes them, its quoted text and every other character as
/// literals, `''` as one apostrophe; `halves` is the `AM/PM` of `a`, `b` and `B`.
fn tokens(pattern: &str, halves: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut literal = String::new();
    let mut quoted = false;
    let mut at = 0_usize;
    while let Some(&found) = chars.get(at) {
        if found == '\'' {
            if chars.get(at + 1) == Some(&'\'') {
                literal.push('\'');
                at += 2;
            } else {
                quoted = !quoted;
                at += 1;
            }
        } else if quoted || !found.is_ascii_alphabetic() {
            literal.push(found);
            at += 1;
        } else {
            let length = chars[at..]
                .iter()
                .take_while(|other| **other == found)
                .count();
            if let Some(written) = code(found, length, halves) {
                if !literal.is_empty() {
                    tokens.push(Token::Literal(mem::take(&mut literal)));
                }
                tokens.push(written);
            }
            at += length;
        }
    }
    if !literal.is_empty() {
        tokens.push(Token::Literal(literal));
    }
    tokens
}
