//! The rules of a CLDR `rbnfRules` element, in ICU's rule syntax (UTS #35, Part 3):
//! each rule set, and each of its rules that formats a whole number.

use core::mem;

use anyhow::{Context as _, Result, bail};

#[cfg(test)]
mod tests;

/// The digits of a decimal pattern a substitution names, such as `#,##0` (UTS #35, Part 3), for a
/// whole number.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Pattern {
    /// The zeros after the decimal separator.
    pub fraction: u8,
    /// The fewest digits of the whole part.
    pub minimum: u8,
    /// Whether the decimal separator ends the pattern, written though no fraction follows.
    pub point: bool,
    /// The digits of the group nearest the separator; 0 for no grouping.
    pub primary: u8,
    /// The digits of each group past the first.
    pub secondary: u8,
}

/// One piece of a rule's text.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Part {
    /// `[a|b]`: `a` when the number is not a multiple of the rule's divisor, else `b`.
    Optional(Vec<Self>, Vec<Self>),
    /// `$(kind,category{text}...)$`: the text of the plural category of the number divided by
    /// the rule's divisor; the kind is `cardinal` or `ordinal`.
    Plural(String, Vec<(String, String)>),
    /// `>>>`: the remainder, written by the rule before this one.
    Preceding,
    /// `<<`: the number divided by the divisor, rounded down.
    Quotient(Target),
    /// `>>`: the remainder of the number divided by the divisor; in the rule of a negative
    /// number, the number without its sign.
    Remainder(Target),
    Text(String),
    /// `==`: the number itself.
    Whole(Target),
}

/// One rule for whole numbers from `base`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Rule {
    pub base: u64,
    /// The radix raised to the highest power that does not pass `base`.
    pub divisor: u64,
    pub parts: Vec<Part>,
}

/// One rule set: its name as a substitution names it, `%name` or `%%name`.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RuleSet {
    pub name: String,
    /// The parts of the `-x` rule.
    pub negative: Option<Vec<Part>>,
    /// The rules of whole numbers, by base value.
    pub rules: Vec<Rule>,
}

/// How a substitution writes its number.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Target {
    /// In digits, as the pattern says.
    Digits(Pattern),
    /// With the rule set of the rule.
    Same,
    /// With the rule set named.
    Set(String),
}

/// The rule sets of `text`, the content of an `rbnfRules` element. The rules of fractions,
/// infinity and not-a-number are left out.
///
/// # Errors
///
/// A line that is neither a rule set's name nor a rule, a descriptor ICU does not define, a
/// bracket or a plural that is not closed, or a decimal pattern of other characters.
pub fn read(text: &str) -> Result<Vec<RuleSet>> {
    let mut sets: Vec<RuleSet> = Vec::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(name) = line.strip_suffix(':').filter(|name| name.starts_with('%')) {
            sets.push(RuleSet {
                name: name.to_owned(),
                negative: None,
                rules: Vec::new(),
            });
            continue;
        }
        let set = sets
            .last_mut()
            .with_context(|| format!("the rule `{line}` comes before any rule set"))?;
        let (descriptor, written) = line
            .strip_suffix(';')
            .and_then(|rule| rule.split_once(':'))
            .with_context(|| format!("`{line}` is not a rule"))?;
        let trimmed = written.trim_start();
        let body = trimmed.strip_prefix('\'').unwrap_or(trimmed);
        let parts = parts(body).with_context(|| format!("in `{line}`"))?;
        match descriptor {
            "-x" => set.negative = Some(parts),
            "x.x" | "0.x" | "x,x" | "x.0" | "Inf" | "NaN" => {}
            _ => {
                let (base_text, radix_text) =
                    descriptor.split_once('/').unwrap_or((descriptor, "10"));
                let base: u64 = base_text
                    .parse()
                    .with_context(|| format!("`{descriptor}` is not a descriptor"))?;
                let radix: u64 = radix_text
                    .parse()
                    .with_context(|| format!("`{descriptor}` has no radix"))?;
                set.rules.push(Rule {
                    base,
                    divisor: divisor(base, radix)?,
                    parts,
                });
            }
        }
    }
    Ok(sets)
}

/// The highest power of `radix` that does not pass `base`; 1 for a base of 0.
fn divisor(base: u64, radix: u64) -> Result<u64> {
    if radix < 2 {
        bail!("the radix {radix}");
    }
    let mut power = 1_u64;
    while let Some(next) = power.checked_mul(radix).filter(|next| *next <= base) {
        power = next;
    }
    Ok(power)
}

/// The pieces of a rule's text.
fn parts(body: &str) -> Result<Vec<Part>> {
    let characters: Vec<char> = body.chars().collect();
    let mut at = 0;
    let (found, stop) = sequence(&characters, &mut at, false)?;
    if stop.is_some() {
        bail!("a `|` or `]` outside brackets");
    }
    Ok(found)
}

/// The decimal pattern `text`.
fn pattern(text: &str) -> Result<Pattern> {
    let (whole, fraction) = text
        .split_once('.')
        .map_or((text, None), |(whole, rest)| (whole, Some(rest)));
    if !whole
        .chars()
        .all(|character| matches!(character, '#' | ',' | '0'))
        || !fraction
            .unwrap_or_default()
            .chars()
            .all(|character| matches!(character, '#' | '0'))
    {
        bail!("`{text}` is not a decimal pattern");
    }
    let count = |piece: &str, wanted: char| {
        u8::try_from(
            piece
                .chars()
                .filter(|character| *character == wanted)
                .count(),
        )
        .unwrap_or(u8::MAX)
    };
    let groups: Vec<&str> = whole.split(',').collect();
    let width = |group: &&str| u8::try_from(group.len()).unwrap_or(u8::MAX);
    let (primary, secondary) = match groups.as_slice() {
        [_] => (0, 0),
        [_, last] => (width(last), width(last)),
        [.., before, last] => (width(last), width(before)),
        [] => bail!("`{text}` is empty"),
    };
    Ok(Pattern {
        fraction: count(fraction.unwrap_or_default(), '0'),
        minimum: count(whole, '0'),
        point: fraction == Some(""),
        primary,
        secondary,
    })
}

/// `$(kind,category{text}...)$` from just past its `$(`, `at` left past its `)$`.
fn plural(characters: &[char], at: &mut usize) -> Result<Part> {
    let mut kind = String::new();
    while let Some(&character) = characters.get(*at) {
        *at += 1;
        if character == ',' {
            break;
        }
        kind.push(character);
    }
    let mut choices = Vec::new();
    loop {
        match (characters.get(*at), characters.get(*at + 1)) {
            (Some(')'), Some('$')) => {
                *at += 2;
                return Ok(Part::Plural(kind, choices));
            }
            (None, _) => bail!("a plural is not closed"),
            _ => {}
        }
        let mut category = String::new();
        while let Some(&character) = characters.get(*at) {
            *at += 1;
            if character == '{' {
                break;
            }
            category.push(character);
        }
        let mut text = String::new();
        loop {
            let character = *characters
                .get(*at)
                .context("a plural choice is not closed")?;
            *at += 1;
            if character == '}' {
                break;
            }
            text.push(character);
        }
        choices.push((category, text));
    }
}

/// The parts from `at` to the end, or to the `|` or `]` that closes brackets when `inside`.
fn sequence(
    characters: &[char],
    at: &mut usize,
    inside: bool,
) -> Result<(Vec<Part>, Option<char>)> {
    let mut found = Vec::new();
    let mut text = String::new();
    let flush = |pending: &mut String, into: &mut Vec<Part>| {
        if !pending.is_empty() {
            into.push(Part::Text(mem::take(pending)));
        }
    };
    while let Some(&character) = characters.get(*at) {
        let next = characters.get(*at + 1).copied();
        match character {
            '|' | ']' if inside => {
                *at += 1;
                flush(&mut text, &mut found);
                return Ok((found, Some(character)));
            }
            '[' if !inside => {
                *at += 1;
                flush(&mut text, &mut found);
                let (first, stop) = sequence(characters, at, true)?;
                let second = match stop {
                    Some('|') => {
                        let (second, end) = sequence(characters, at, true)?;
                        if end != Some(']') {
                            bail!("brackets are not closed");
                        }
                        second
                    }
                    Some(']') => Vec::new(),
                    _ => bail!("brackets are not closed"),
                };
                found.push(Part::Optional(first, second));
            }
            '$' if next == Some('(') => {
                *at += 2;
                flush(&mut text, &mut found);
                found.push(plural(characters, at)?);
            }
            '<' | '>' | '='
                if next.is_some_and(|after| {
                    matches!(after, '%' | '#' | '0') || after == character
                }) =>
            {
                flush(&mut text, &mut found);
                found.push(substitution(characters, at, character)?);
            }
            _ => {
                *at += 1;
                text.push(character);
            }
        }
    }
    if inside {
        bail!("brackets are not closed");
    }
    flush(&mut text, &mut found);
    Ok((found, None))
}

/// The substitution that starts at `at` with `token`, `at` left past it.
fn substitution(characters: &[char], at: &mut usize, token: char) -> Result<Part> {
    let start = *at;
    if token == '>' && characters.get(start + 1..start + 3) == Some(&['>', '>']) {
        *at = start + 3;
        return Ok(Part::Preceding);
    }
    let target = if characters.get(start + 1) == Some(&token) {
        *at = start + 2;
        Target::Same
    } else {
        let end = (start + 1..characters.len())
            .find(|index| characters.get(*index) == Some(&token))
            .with_context(|| format!("a substitution `{token}` is not closed"))?;
        *at = end + 1;
        let named: String = characters
            .get(start + 1..end)
            .unwrap_or_default()
            .iter()
            .collect();
        if named.starts_with('%') {
            Target::Set(named)
        } else {
            Target::Digits(pattern(&named)?)
        }
    };
    Ok(match token {
        '<' => Part::Quotient(target),
        '>' => Part::Remainder(target),
        _ => Part::Whole(target),
    })
}
