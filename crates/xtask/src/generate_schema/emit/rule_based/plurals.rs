//! CLDR's plural rules (UTS #35, Part 3), as they apply to a whole number: its
//! operands `n` and `i` are the number, and `v`, `w`, `f`, `t`, `c` and `e` are 0.

use alloc::collections::BTreeMap;

use anyhow::{Context as _, Result, bail};

use crate::generate_schema::cldr::Tree;

#[cfg(test)]
mod tests;

/// The rule of one category but `other`: it applies when one of its conditions holds, a
/// condition holding when each of its relations does.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct PluralRule {
    pub category: String,
    pub conditions: Vec<Vec<Relation>>,
}

/// `n % modulus = ranges`, or `!=` when not `equal`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct Relation {
    pub equal: bool,
    pub modulus: Option<u64>,
    /// Each range from its first value to its last, both in it.
    pub ranges: Vec<(u64, u64)>,
}

/// The conditions of a rule's text, without the relations that hold for every whole number;
/// `None` when it holds for none.
///
/// # Errors
///
/// An operand UTS #35 does not define, or a relation in another syntax.
pub fn conditions(text: &str) -> Result<Option<Vec<Vec<Relation>>>> {
    let rule = text.split('@').next().unwrap_or_default().trim();
    if rule.is_empty() {
        return Ok(Some(vec![Vec::new()]));
    }
    let mut kept = Vec::new();
    'or: for alternative in rule.split(" or ") {
        let mut relations = Vec::new();
        for relation in alternative.split(" and ") {
            match self::relation(relation.trim())
                .with_context(|| format!("in the plural rule `{rule}`"))?
            {
                Ok(found) => relations.push(found),
                Err(true) => {}
                Err(false) => continue 'or,
            }
        }
        kept.push(relations);
    }
    Ok((!kept.is_empty()).then_some(kept))
}

/// The plural rules of each locale a `plurals.xml` or `ordinals.xml` names.
///
/// # Errors
///
/// A rule [`conditions`] refuses, or a `pluralRules` with no `locales`.
pub fn read(file: &Tree) -> Result<BTreeMap<String, Vec<PluralRule>>> {
    let mut found = BTreeMap::new();
    for list in file.named("plurals") {
        for rules in list.named("pluralRules") {
            let mut kept = Vec::new();
            for rule in rules.named("pluralRule") {
                let category = rule
                    .attribute("count")
                    .context("a pluralRule with no count")?;
                if category == "other" {
                    continue;
                }
                if let Some(conditions) = conditions(&rule.text)? {
                    kept.push(PluralRule {
                        category: category.to_owned(),
                        conditions,
                    });
                }
            }
            let locales = rules
                .attribute("locales")
                .context("a pluralRules with no locales")?;
            for locale in locales.split_whitespace() {
                found.insert(locale.to_owned(), kept.clone());
            }
        }
    }
    Ok(found)
}

/// One relation: `Ok` for one on the number, `Err` with its truth for one on an operand that is
/// 0 for every whole number.
fn relation(text: &str) -> Result<Result<Relation, bool>> {
    let (expression, listed, equal) = if let Some((left, right)) = text.split_once("!=") {
        (left, right, false)
    } else if let Some((left, right)) = text.split_once('=') {
        (left, right, true)
    } else {
        bail!("`{text}` is not a relation");
    };
    let (operand, divided_by) = expression.split_once('%').map_or_else(
        || (expression.trim(), None),
        |(left, right)| (left.trim(), Some(right.trim())),
    );
    let modulus = divided_by
        .map(str::parse::<u64>)
        .transpose()
        .with_context(|| format!("`{text}` has no whole modulus"))?;
    let ranges = listed
        .split(',')
        .map(|range| {
            let (first, last) = range.split_once("..").unwrap_or((range, range));
            Ok((first.trim().parse()?, last.trim().parse()?))
        })
        .collect::<Result<Vec<(u64, u64)>>>()
        .with_context(|| format!("`{text}` has a range that is not whole numbers"))?;
    match operand {
        "n" | "i" => Ok(Ok(Relation {
            equal,
            modulus,
            ranges,
        })),
        "v" | "w" | "f" | "t" | "c" | "e" => {
            Ok(Err(ranges.iter().any(|(first, _)| *first == 0) == equal))
        }
        other => bail!("the operand `{other}`"),
    }
}
