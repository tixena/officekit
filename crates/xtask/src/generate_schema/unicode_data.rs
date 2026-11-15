//! The Unicode Character Database's `UnicodeData.txt`, `SpecialCasing.txt` and `PropList.txt`:
//! one character or range per line, its fields separated by `;` (Unicode Standard Annex #44,
//! section 4.2).

use anyhow::{Context as _, Result, bail};

#[cfg(test)]
mod tests;

/// A character of general category `Nd`, a decimal digit.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DecimalDigit {
    /// The character, field 0.
    pub character: char,
    /// Its name, field 1.
    pub name: String,
    /// Its decimal digit value, field 6.
    pub value: u8,
}

/// A casing context a language's mapping in `SpecialCasing.txt` depends on (The Unicode Standard,
/// section 3.13, table 3-17).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CasingContext {
    AfterI,
    AfterSoftDotted,
    BeforeDot,
    MoreAbove,
}

impl CasingContext {
    /// The name `SpecialCasing.txt` writes.
    #[must_use = "this only returns the value"]
    pub const fn name(self) -> &'static str {
        match self {
            Self::AfterI => "After_I",
            Self::AfterSoftDotted => "After_Soft_Dotted",
            Self::BeforeDot => "Before_Dot",
            Self::MoreAbove => "More_Above",
        }
    }
}

/// A row of `SpecialCasing.txt` whose conditions name a language.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LanguageCasing {
    pub character: char,
    /// Each context and whether it must hold, `false` for one written `Not_`.
    pub contexts: Vec<(CasingContext, bool)>,
    /// The language subtag, lower case.
    pub language: String,
    pub lower: String,
    pub upper: String,
}

/// Every character of category `Nd` (field 2), in the text's order.
///
/// # Errors
///
/// A line of category `Nd` whose code point or decimal digit value does not parse.
pub fn decimal_digits(text: &str) -> Result<Vec<DecimalDigit>> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(';').collect();
            (fields.get(2) == Some(&"Nd")).then_some((line, fields))
        })
        .map(|(line, fields)| decimal_digit(&fields).with_context(|| format!("cannot read {line}")))
        .collect()
}

/// The digit of the fields of one line of category `Nd`.
fn decimal_digit(fields: &[&str]) -> Result<DecimalDigit> {
    let code = u32::from_str_radix(fields.first().context("no code point")?, 16)?;
    let character = char::from_u32(code).context("no character")?;
    let name = (*fields.get(1).context("no name")?).to_owned();
    let value = fields.get(6).context("no decimal digit value")?.parse()?;
    Ok(DecimalDigit {
        character,
        name,
        value,
    })
}

/// Each range of characters whose canonical combining class (field 3) is not 0, with that class,
/// merged where neighbours share it, in code point order.
///
/// # Errors
///
/// A line whose code point or combining class does not parse, or a class other than 0 on a code
/// point that is no character.
pub fn combining_classes(text: &str) -> Result<Vec<(char, char, u8)>> {
    let mut ranges: Vec<(char, char, u8)> = Vec::new();
    let mut first: Option<u32> = None;
    for line in text.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split(';').collect();
        let (code, class) =
            combining_class(&fields).with_context(|| format!("cannot read {line}"))?;
        if fields.get(1).is_some_and(|name| name.ends_with(", First>")) {
            first = Some(code);
            continue;
        }
        let start = first.take().unwrap_or(code);
        if class == 0 {
            continue;
        }
        let character = |value: u32| {
            char::from_u32(value).with_context(|| format!("cannot read {line}: no character"))
        };
        let (low, high) = (character(start)?, character(code)?);
        match ranges.last_mut() {
            Some((_, last, found)) if *found == class && u32::from(*last) + 1 == start => {
                *last = high;
            }
            _ => ranges.push((low, high, class)),
        }
    }
    Ok(ranges)
}

/// The code point and canonical combining class of the fields of one line.
fn combining_class(fields: &[&str]) -> Result<(u32, u8)> {
    let code = u32::from_str_radix(fields.first().context("no code point")?, 16)?;
    let class = fields
        .get(3)
        .context("no canonical combining class")?
        .parse()?;
    Ok((code, class))
}

/// Each row of `SpecialCasing.txt` whose condition list names a language, in the text's order.
///
/// # Errors
///
/// A row that does not parse, that names two languages or a language with more than its
/// language subtag, or a context the table cannot evaluate, such as `Final_Sigma`.
pub fn language_casings(text: &str) -> Result<Vec<LanguageCasing>> {
    let mut found = Vec::new();
    for line in text.lines() {
        let data = line
            .split_once('#')
            .map_or(line, |(before, _)| before)
            .trim();
        if data.is_empty() {
            continue;
        }
        if let Some(row) = language_casing(data).with_context(|| format!("cannot read {line}"))? {
            found.push(row);
        }
    }
    Ok(found)
}

/// The row of one line's data, `None` when its conditions name no language.
fn language_casing(data: &str) -> Result<Option<LanguageCasing>> {
    let fields: Vec<&str> = data.split(';').map(str::trim).collect();
    let conditions = fields.get(4).copied().unwrap_or_default();
    let mut named = None;
    let mut contexts = Vec::new();
    let mut unknown = None;
    for condition in conditions.split_whitespace() {
        let (wanted, name) = condition
            .get(..4)
            .filter(|prefix| prefix.eq_ignore_ascii_case("not_"))
            .map_or((true, condition), |_| (false, &condition[4..]));
        let context = [
            CasingContext::AfterI,
            CasingContext::AfterSoftDotted,
            CasingContext::BeforeDot,
            CasingContext::MoreAbove,
        ]
        .into_iter()
        .find(|context| context.name().eq_ignore_ascii_case(name));
        if let Some(known) = context {
            contexts.push((known, wanted));
        } else if wanted
            && (2..=8).contains(&name.len())
            && name.bytes().all(|byte| byte.is_ascii_alphabetic())
        {
            if named.replace(name.to_ascii_lowercase()).is_some() {
                bail!("two languages");
            }
        } else {
            unknown = Some(condition);
        }
    }
    let Some(language) = named else {
        return Ok(None);
    };
    if let Some(condition) = unknown {
        bail!("the condition {condition}, which the table cannot evaluate");
    }
    let code = u32::from_str_radix(fields.first().context("no code point")?, 16)?;
    Ok(Some(LanguageCasing {
        character: char::from_u32(code).context("no character")?,
        contexts,
        language,
        lower: characters(fields.get(1).context("no lower case mapping")?)?,
        upper: characters(fields.get(3).context("no upper case mapping")?)?,
    }))
}

/// The characters of a mapping, code points in hex separated by spaces.
fn characters(field: &str) -> Result<String> {
    field
        .split_whitespace()
        .map(|code| {
            let value = u32::from_str_radix(code, 16)?;
            char::from_u32(value).context("no character")
        })
        .collect()
}

/// Each range of characters `PropList.txt` gives the property `property`, in code point order.
///
/// # Errors
///
/// A line of that property whose code point or range does not parse.
pub fn property(text: &str, property: &str) -> Result<Vec<(char, char)>> {
    let mut ranges: Vec<(char, char)> = text
        .lines()
        .filter_map(|line| {
            let data = line.split_once('#').map_or(line, |(before, _)| before);
            let (codes, name) = data.split_once(';')?;
            (name.trim() == property).then_some((line, codes.trim()))
        })
        .map(|(line, codes)| range(codes).with_context(|| format!("cannot read {line}")))
        .collect::<Result<_>>()?;
    ranges.sort_unstable();
    Ok(ranges)
}

/// The range `first..last`, or the one character `first`.
fn range(codes: &str) -> Result<(char, char)> {
    let (first, last) = codes.split_once("..").unwrap_or((codes, codes));
    let character = |code: &str| -> Result<char> {
        char::from_u32(u32::from_str_radix(code, 16)?).context("no character")
    };
    Ok((character(first)?, character(last)?))
}
