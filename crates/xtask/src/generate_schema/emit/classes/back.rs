//! The tables a save from Strict to Transitional walks: each rule of the Strict walk reversed, so
//! that the Transitional copy holds every value in Transitional's own spelling.
//!
//! The way back of a row comes from the rule the forward tables settled for each tag, so the two
//! directions stay in step. A map whose two Transitional values share a Strict one has no way
//! back unless the table names it, and stops the build.

use alloc::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::correspondences::{Bits, Codes, Correspondences, Spelling};
use super::rules::{AttributeRule, ChildRule, Rules, Tag};

#[cfg(test)]
mod tests;

/// What a save from Strict to Transitional does with an attribute. The walk applies an
/// attribute's rules that rewrite its value before those that move or rename it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum BackRule {
    /// A name of the code table `codes` written as its code in the attribute `to`, in place of
    /// this one.
    Code { codes: String, to: String },
    /// A measure whose unit the attribute `unit` names: a percentage counted in `per_percent`
    /// of a percent when the unit is `percent`, else a universal measure in `per_point` of a
    /// point.
    Measure {
        per_percent: u32,
        per_point: u32,
        percent: String,
        unit: String,
    },
    /// The attributes `bits` names, packed into this attribute when it is absent.
    Pack(Bits),
    /// A percentage written as a bare number counting `per_percent` of a percent.
    Percent(u32),
    /// Transitional's name for the attribute.
    Rename(String),
    /// An ISO 8601 date-time written as a serial date-time in the attribute named, in place of
    /// this one.
    Serial(String),
    /// A universal measure written in twentieths of a point.
    Twips,
    /// `(Strict, Transitional)` values; sorted.
    Values(Vec<(String, String)>),
}

impl BackRule {
    /// Whether the rule moves or renames the attribute, which comes after its value's rules.
    const fn moves(&self) -> bool {
        match self {
            Self::Code {
                codes: _codes,
                to: _to,
            } => true,
            Self::Rename(_) | Self::Serial(_) => true,
            Self::Measure {
                per_percent: _per_percent,
                per_point: _per_point,
                percent: _percent,
                unit: _unit,
            } => false,
            Self::Pack(_) | Self::Percent(_) | Self::Twips | Self::Values(_) => false,
        }
    }
}

/// The tables, each sorted by its key.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Back {
    /// By element and attribute.
    pub attributes: BTreeMap<(Tag, Tag), Vec<BackRule>>,
    /// By element and child: the attribute the child's text goes back into.
    pub child_attributes: BTreeMap<(Tag, Tag), Tag>,
    /// By parent and child: Transitional's name for the child.
    pub children: BTreeMap<(Tag, Tag), String>,
    /// Each code table by name, as `(name, code)` pairs sorted by name.
    pub codes: BTreeMap<String, Vec<(String, String)>>,
    /// By parent, element, and attribute.
    pub contextual: BTreeMap<(Tag, Tag, Tag), Vec<BackRule>>,
    /// By element: the text of a percentage written in `per_percent` of a percent.
    pub texts: BTreeMap<Tag, u32>,
}

/// What `build` gathers.
struct Builder<'table> {
    back: Back,
    problems: Vec<String>,
    rules: &'table Rules,
    table: &'table Correspondences,
}

impl Builder<'_> {
    /// The way back of the forward rule of an attribute, keyed as the forward rule is.
    fn attribute(&mut self, rule: &AttributeRule) -> Vec<BackRule> {
        match rule {
            AttributeRule::Measure {
                per_percent,
                per_point,
                percent,
                unit,
            } => vec![BackRule::Measure {
                per_percent: *per_percent,
                per_point: *per_point,
                percent: percent.clone(),
                unit: unit.clone(),
            }],
            AttributeRule::Unpack {
                bits,
                label: _label,
            } => vec![BackRule::Pack(bits.clone())],
            AttributeRule::Values(spellings) => self.spellings(spellings),
            AttributeRule::CharacterSet {
                codes: _codes,
                label: _label,
                to: _to,
            } => Vec::new(),
            AttributeRule::Iso {
                label: _label,
                to: _to,
            } => Vec::new(),
            AttributeRule::Child(_) | AttributeRule::Refuse(_) | AttributeRule::Rename(_) => {
                Vec::new()
            }
        }
    }

    /// Transitional's name of each child Strict names otherwise.
    fn children(&mut self) {
        for ((parent, child), rule) in &self.rules.children {
            if let ChildRule::Rename {
                before: _before,
                to,
            } = rule
                && self
                    .back
                    .children
                    .insert((parent.clone(), (child.0, to.clone())), child.1.clone())
                    .is_some()
            {
                self.problems.push(format!(
                    "two children of {parent:?} are renamed {to} in Strict, so it has no way back"
                ));
            }
        }
    }

    /// The reversed code table `name`, each name two codes share taking the code the table's
    /// `back` rows name.
    fn codes(&mut self, name: &str) {
        if self.back.codes.contains_key(name) {
            return;
        }
        let Some(table) = self.rules.codes.get(name) else {
            self.problems.push(format!("no code table {name}"));
            return;
        };
        let rows: Vec<(String, String)> = match table {
            Codes::Byte(rows) => rows
                .iter()
                .map(|(code, text)| (text.clone(), format!("{code:02X}")))
                .collect(),
            Codes::Number(rows) => rows
                .iter()
                .map(|(code, text)| (text.clone(), code.to_string()))
                .collect(),
        };
        let chosen: BTreeMap<&str, &str> = self
            .table
            .character_sets
            .iter()
            .filter(|row| super::rules::codes_name(&row.codes) == name)
            .flat_map(|row| &row.back)
            .map(|(text, code)| (text.as_str(), code.as_str()))
            .collect();
        let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (text, code) in rows {
            by_name.entry(text).or_default().insert(code);
        }
        let mut reversed = Vec::new();
        for (text, codes) in &by_name {
            let code = match (codes.len(), chosen.get(text.as_str())) {
                (1, None) => codes.first().cloned(),
                (_, Some(code)) if codes.len() > 1 && codes.contains(*code) => {
                    Some((*code).to_owned())
                }
                (1, Some(_)) => {
                    self.problems.push(format!(
                        "the way back of {name} names {text}, which only one code has"
                    ));
                    None
                }
                _ => {
                    self.problems.push(format!(
                        "{text} is the name of the codes {codes:?} of {name}, and no `back` row \
                         names the one a Transitional save writes"
                    ));
                    None
                }
            };
            if let Some(found) = code {
                reversed.push((text.clone(), found));
            }
        }
        self.back.codes.insert(name.to_owned(), reversed);
    }

    /// The way back of each forward rule keyed as it is, and of an attribute written as a child.
    fn forward(&mut self) {
        let rules = self.rules;
        for ((owner, attribute), rule) in &rules.attributes {
            if let AttributeRule::Child(local) = rule {
                let child = (owner.0, local.clone());
                if self
                    .back
                    .child_attributes
                    .insert((owner.clone(), child), attribute.clone())
                    .is_some()
                {
                    self.problems.push(format!(
                        "two attributes of {owner:?} go back from the child {local}"
                    ));
                }
                continue;
            }
            for back in self.attribute(rule) {
                insert(
                    &mut self.back.attributes,
                    (owner.clone(), attribute.clone()),
                    back,
                );
            }
        }
        for ((parent, owner, attribute), rule) in &rules.contextual {
            for back in self.attribute(rule) {
                insert(
                    &mut self.back.contextual,
                    (parent.clone(), owner.clone(), attribute.clone()),
                    back,
                );
            }
        }
    }

    /// The way back of a rule that moved `attribute` to the attribute it is keyed by.
    fn moved(&mut self, attribute: &Tag, rule: &AttributeRule) -> Option<BackRule> {
        match rule {
            AttributeRule::CharacterSet {
                codes,
                label: _label,
                to: _to,
            } => {
                self.codes(codes);
                Some(BackRule::Code {
                    codes: codes.clone(),
                    to: attribute.1.clone(),
                })
            }
            AttributeRule::Iso {
                label: _label,
                to: _to,
            } => Some(BackRule::Serial(attribute.1.clone())),
            AttributeRule::Rename(_) => Some(BackRule::Rename(attribute.1.clone())),
            AttributeRule::Measure {
                per_percent: _per_percent,
                per_point: _per_point,
                percent: _percent,
                unit: _unit,
            } => None,
            AttributeRule::Unpack {
                bits: _bits,
                label: _label,
            } => None,
            AttributeRule::Child(_) | AttributeRule::Refuse(_) | AttributeRule::Values(_) => None,
        }
    }

    /// The way back of each rule that moved an attribute, keyed by the attribute it moved to.
    fn moves(&mut self) {
        let rules = self.rules;
        for ((owner, target), (attribute, rule)) in &rules.moved {
            if let Some(back) = self.moved(attribute, rule) {
                insert(
                    &mut self.back.attributes,
                    (owner.clone(), target.clone()),
                    back,
                );
            }
        }
        for ((parent, owner, target), (attribute, rule)) in &rules.moved_contextual {
            if let Some(back) = self.moved(attribute, rule) {
                insert(
                    &mut self.back.contextual,
                    (parent.clone(), owner.clone(), target.clone()),
                    back,
                );
            }
        }
    }

    /// The way back of a list of spellings: a value row's map reversed, and a percentage. A
    /// member's map stays, as Transitional takes Strict's values too.
    fn spellings(&mut self, spellings: &[Spelling]) -> Vec<BackRule> {
        let mut found = Vec::new();
        for spelling in spellings {
            match spelling {
                Spelling::Map(pairs) => {
                    if !self.table.values.iter().any(|row| row.values == *pairs) {
                        continue;
                    }
                    let mut reversed: Vec<(String, String)> = pairs
                        .iter()
                        .map(|(transitional, strict)| (strict.clone(), transitional.clone()))
                        .collect();
                    reversed.sort();
                    let targets: BTreeSet<&String> =
                        pairs.iter().map(|(_, strict)| strict).collect();
                    if targets.len() == pairs.len() {
                        found.push(BackRule::Values(reversed));
                    } else {
                        self.problems.push(format!(
                            "two Transitional values of {pairs:?} share a Strict one, so it has \
                             no way back"
                        ));
                    }
                }
                Spelling::Percent {
                    decimals: _decimals,
                    per_percent,
                } => found.push(BackRule::Percent(*per_percent)),
            }
        }
        let percents: BTreeSet<&BackRule> = found
            .iter()
            .filter(|rule| matches!(rule, BackRule::Percent(_)))
            .collect();
        if percents.len() > 1 {
            self.problems.push(format!(
                "the spellings {spellings:?} count a percentage in two units, so it has no way \
                 back"
            ));
        }
        found
    }

    /// The way back of the text of each extension element Strict writes as a percentage.
    fn texts(&mut self) {
        for (element, spellings) in &self.rules.texts {
            let back = self.spellings(spellings);
            if let [BackRule::Percent(per_percent)] = back.as_slice() {
                self.back.texts.insert(element.clone(), *per_percent);
            } else {
                self.problems.push(format!(
                    "the text of {element:?} has no way back from {spellings:?}"
                ));
            }
        }
    }

    /// The universal measure of each attribute that counts twips, written as twips.
    fn twips(&mut self) {
        for key in &self.rules.twips {
            let measured = self.back.attributes.get(key).is_some_and(|found| {
                found.iter().any(|rule| {
                    matches!(
                        rule,
                        BackRule::Measure {
                            per_percent: _per_percent,
                            per_point: _per_point,
                            percent: _percent,
                            unit: _unit,
                        }
                    )
                })
            });
            if measured {
                self.problems.push(format!(
                    "{key:?} counts twips and has a measure row, so it has two ways back"
                ));
            }
            insert(&mut self.back.attributes, key.clone(), BackRule::Twips);
        }
        for (parent, owner, attribute) in &self.rules.twips_contextual {
            insert(
                &mut self.back.contextual,
                (parent.clone(), owner.clone(), attribute.clone()),
                BackRule::Twips,
            );
        }
    }
}

/// The way back of every rule of `rules`, with the code tables' choices `table` names.
///
/// # Errors
///
/// A map whose two values share one Strict value, a name two codes share that no `back` row
/// settles, two rules writing one attribute's percentage in two units, or two rules of one
/// target.
pub fn build(rules: &Rules, table: &Correspondences) -> Result<Back> {
    let mut builder = Builder {
        back: Back::default(),
        problems: Vec::new(),
        rules,
        table,
    };
    builder.forward();
    builder.moves();
    builder.twips();
    builder.children();
    builder.texts();
    if !builder.problems.is_empty() {
        bail!(
            "the way back from Strict:\n  {}",
            builder.problems.join("\n  ")
        );
    }
    Ok(builder.back)
}

/// Add `rule` to the rules of `key` in `table`, unless it is there, the rules that rewrite a
/// value before those that move the attribute.
fn insert<K>(table: &mut BTreeMap<K, Vec<BackRule>>, key: K, rule: BackRule)
where
    K: Ord,
{
    let rules = table.entry(key).or_default();
    if !rules.contains(&rule) {
        rules.push(rule);
        rules.sort_by(|left, right| {
            left.moves()
                .cmp(&right.moves())
                .then_with(|| left.cmp(right))
        });
    }
}
