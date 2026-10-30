//! The tables the two classes' writes walk: what a Strict save does with each
//! element, attribute, and value Transitional adds, and the values only Strict
//! has, which a Transitional write refuses.
//!
//! The comparison of the two schema sets says what Transitional adds; the
//! correspondences say what each addition becomes. Rules are keyed by tag, so
//! each tag's types must agree on what a Strict save does with an attribute
//! or a child of that tag.

use alloc::collections::{BTreeMap, BTreeSet};
use core::mem;

use anyhow::{Context as _, Result, bail};

use super::correspondences::{Bits, Codes, Correspondences, Spelling};
use crate::generate_schema::compare::{self, Additions};
use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::model::{
    ComplexType, Content, Name, Particle, ParticleKind, SchemaSet, SimpleType, TypeRef,
};

#[cfg(test)]
mod tests;

/// A tag of the tables: its namespace's id, `None` when unqualified, and its
/// local name.
pub type Tag = (Option<u8>, String);

/// What a Strict save does with an attribute of an element.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AttributeRule {
    /// A code written as its name in the attribute `to`; `codes` names the
    /// code table.
    CharacterSet {
        codes: String,
        label: String,
        to: String,
    },
    /// The text written as a child element of this local name.
    Child(String),
    /// A serial date-time written in ISO 8601 in the attribute `to`.
    Iso { label: String, to: String },
    /// A bare number whose unit the attribute `unit` names.
    Measure {
        per_percent: u32,
        per_point: u32,
        percent: String,
        unit: String,
    },
    /// No Strict form; the label names it.
    Refuse(String),
    /// Strict's name for the attribute.
    Rename(String),
    /// A bitmask written as the attributes it packs.
    Unpack { bits: Bits, label: String },
    /// Values Strict spells otherwise.
    Values(Vec<Spelling>),
}

/// What a Strict save does with a child of an element.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ChildRule {
    /// No Strict form; the label names it.
    Refuse(String),
    /// Strict's name for the child, and the siblings it goes before.
    Rename { before: Vec<String>, to: String },
}

/// The tables, each sorted by its key.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Rules {
    /// By element and attribute.
    pub attributes: BTreeMap<(Tag, Tag), AttributeRule>,
    /// By parent and child.
    pub children: BTreeMap<(Tag, Tag), ChildRule>,
    /// The code tables the character-set rules name.
    pub codes: BTreeMap<String, Codes>,
    /// By parent, element, and attribute: the rules of an attribute whose
    /// element's types disagree, where the parent tells them apart.
    pub contextual: BTreeMap<(Tag, Tag, Tag), AttributeRule>,
    /// Elements of the Transitional schemas no Strict schema declares.
    pub elements: BTreeSet<Tag>,
    /// The namespaces with no Strict twin, each with the local names of its
    /// elements.
    pub namespaces: BTreeMap<u8, Vec<String>>,
    /// Each list of spellings with the name of the static holding it.
    pub spellings: BTreeMap<Vec<Spelling>, String>,
    /// By element and attribute: the values only Strict has, and the label
    /// of a Transitional write's refusal.
    pub strict_only: BTreeMap<(Tag, Tag), (Vec<String>, String)>,
}

/// What one type of a tag says of an attribute, with the tags of the
/// parents the tag has that type under.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Vote {
    parents: BTreeSet<Tag>,
    rule: Option<AttributeRule>,
}

/// What `build` works with and what it gathers.
struct Builder<'set> {
    additions: Additions,
    /// What each type of a tag says of an attribute, with the parents the
    /// tag has that type in; `None` keeps the attribute as it is.
    attribute_votes: BTreeMap<(Tag, Tag), Vec<Vote>>,
    /// What each tag's types say of a child.
    child_votes: BTreeMap<(Tag, Tag), BTreeSet<Option<ChildRule>>>,
    ids: BTreeMap<&'set str, u8>,
    /// Each pair of tag and complex type, with the tags of its parents.
    parents: BTreeMap<(Name, Name), BTreeSet<Name>>,
    problems: Vec<String>,
    rows: &'set [Row],
    rules: Rules,
    /// For each list of spellings, the types that give it.
    spelled_by: BTreeMap<Vec<Spelling>, BTreeSet<Name>>,
    strict: &'set SchemaSet,
    table: &'set Correspondences,
    /// Each Transitional namespace that has one, its Strict twin.
    to_strict: BTreeMap<String, String>,
    transitional: &'set SchemaSet,
    /// Each Strict namespace's Transitional twin.
    twins: BTreeMap<String, String>,
    /// The rows of the table a rule took.
    used: BTreeSet<(&'static str, Name, String)>,
}

impl<'set> Builder<'set> {
    /// The rules of an attribute whose tag's types disagree, by the parent
    /// each type stands in, where the types under one parent agree.
    fn by_parent(&mut self, owner: &Tag, attribute: &Tag, votes: &[Vote]) {
        let all: BTreeSet<&Tag> = votes.iter().flat_map(|vote| &vote.parents).collect();
        if votes
            .iter()
            .any(|vote| vote.parents.is_empty() && vote.rule.is_some())
        {
            self.problems.push(format!(
                "a root type of {owner:?} converts its attribute {attribute:?}, which another \
                 type of the tag keeps"
            ));
        }
        for parent in all {
            let rules: BTreeSet<&Option<AttributeRule>> = votes
                .iter()
                .filter(|vote| vote.parents.contains(parent))
                .map(|vote| &vote.rule)
                .collect();
            match rules.into_iter().collect::<Vec<_>>().as_slice() {
                [None] => {}
                [Some(rule)] => {
                    self.rules.contextual.insert(
                        (parent.clone(), owner.clone(), attribute.clone()),
                        rule.clone(),
                    );
                }
                _ => self.problems.push(format!(
                    "the types of {owner:?} under {parent:?} disagree on its attribute \
                     {attribute:?}"
                )),
            }
        }
    }

    /// The table's rows each take an addition the comparison names, and each
    /// addition of a value or a member has a row.
    fn check_rows(&mut self) {
        for (owner, value) in &self.additions.enumeration_values {
            let covered = self.table.values.iter().any(|row| {
                row.type_name == *owner && row.values.iter().any(|(from, _)| from == value)
            });
            if !covered {
                self.problems.push(format!(
                    "Transitional adds the value {value} to {owner}, which has no `value` row"
                ));
            }
        }
        for row in &self.table.values {
            for (from, _) in &row.values {
                if !self
                    .additions
                    .enumeration_values
                    .contains(&(row.type_name.clone(), from.clone()))
                {
                    self.problems.push(format!(
                        "the `value` row of {} names {from}, which Transitional does not add",
                        row.type_name
                    ));
                }
            }
        }
        for (owner, member) in &self.additions.union_members {
            if !self
                .table
                .members
                .iter()
                .any(|row| row.type_name == *owner && row.member == *member)
            {
                self.problems.push(format!(
                    "Transitional adds the member {member} to {owner}, which has no `member` row"
                ));
            }
        }
        for row in &self.table.members {
            if !self
                .additions
                .union_members
                .contains(&(row.type_name.clone(), row.member.clone()))
            {
                self.problems.push(format!(
                    "the `member` row of {} names {}, which Transitional does not add",
                    row.type_name, row.member
                ));
            }
        }
        let table = self.table;
        let mut expected: Vec<(&'static str, Name, String)> = Vec::new();
        for row in &table.attributes {
            for (from, _) in &row.renames {
                expected.push(("attribute", row.type_name.clone(), from.clone()));
            }
        }
        for row in &table.children {
            for (from, _) in &row.renames {
                expected.push(("child", row.type_name.clone(), from.clone()));
            }
        }
        for row in &table.child_attributes {
            expected.push((
                "child_attribute",
                row.type_name.clone(),
                row.attribute.clone(),
            ));
        }
        for row in &table.packed {
            expected.push(("packed", row.type_name.clone(), row.attribute.clone()));
        }
        for row in &table.character_sets {
            expected.push((
                "character_set",
                row.type_name.clone(),
                row.attribute.clone(),
            ));
        }
        for row in &table.iso_dates {
            expected.push(("iso_date", row.type_name.clone(), row.attribute.clone()));
        }
        for row in &table.measures {
            expected.push(("measure", row.type_name.clone(), row.attribute.clone()));
        }
        for key in expected {
            if !self.used.contains(&key) {
                self.problems.push(format!(
                    "the `{}` row of {} names {}, which no element of that type adds",
                    key.0, key.1, key.2
                ));
            }
        }
    }

    /// What a Strict save does with each attribute and child of the element
    /// `tag` of the complex type `name`.
    fn complex_type(&mut self, tag: &Name, name: &Name) -> Result<()> {
        let Some(declared) = self.transitional.complex_types.get(name) else {
            return Ok(());
        };
        let own = self.tag(tag)?;
        let effective = self.transitional.effective(declared);
        let parents: BTreeSet<Tag> = self
            .parents
            .get(&(tag.clone(), name.clone()))
            .map(|found| found.iter().map(|parent| self.tag(parent)).collect())
            .transpose()?
            .unwrap_or_default();
        let Some(twin) = self.twin(name) else {
            if !self.additions.complex_types.contains(name) {
                self.problems
                    .push(format!("{name} has no Strict twin the comparison names"));
            }
            return Ok(());
        };
        let strict_attributes: BTreeSet<Name> = twin
            .attributes
            .iter()
            .map(|attribute| compare::translate(&attribute.name, &self.twins))
            .collect();
        for attribute in &effective.attributes {
            let key = (own.clone(), self.tag(&attribute.name)?);
            let label = format!("{}/@{}", tag.local, attribute.name.local);
            let rule = if strict_attributes.contains(&attribute.name) {
                self.shared_attribute(name, &attribute.name.local, &attribute.type_ref)?
            } else {
                Some(self.transitional_only_attribute(name, &attribute.name.local, label))
            };
            self.attribute_votes.entry(key).or_default().push(Vote {
                parents: parents.clone(),
                rule,
            });
        }
        let ordered = children(self.transitional, &effective.content);
        let strict_children: BTreeSet<Name> = children(self.strict, &twin.content)
            .iter()
            .map(|child| compare::translate(child, &self.twins))
            .collect();
        for child in &ordered {
            let key = (own.clone(), self.tag(child)?);
            let rule = if strict_children.contains(child) {
                None
            } else {
                Some(self.transitional_only_child(name, child, &ordered))
            };
            self.child_votes.entry(key).or_default().insert(rule);
        }
        Ok(())
    }

    /// The elements of the Transitional schemas no Strict schema declares,
    /// in the namespaces both classes share; each Transitional-only complex
    /// type's elements must be among them.
    fn elements(&mut self) -> Result<()> {
        let strict_tags: BTreeSet<Name> = self
            .strict
            .element_tags()
            .into_iter()
            .map(|(name, _)| compare::translate(name, &self.twins))
            .collect();
        for (name, _) in self.transitional.element_tags() {
            if !self.to_strict.contains_key(&name.namespace) || strict_tags.contains(name) {
                continue;
            }
            let tag = self.tag(name)?;
            self.rules.elements.insert(tag);
        }
        for (name, type_ref) in self.transitional.element_tags() {
            let Some(type_name) = type_ref.name() else {
                continue;
            };
            if self.additions.complex_types.contains(&type_name)
                && !self.rules.elements.contains(&self.tag(name)?)
            {
                self.problems.push(format!(
                    "{name} has the Transitional-only type {type_name} but a Strict schema \
                     declares it"
                ));
            }
        }
        Ok(())
    }

    /// The namespace id of a URI the namespace table knows.
    fn id(&self, uri: &str) -> Result<u8> {
        self.ids
            .get(uri)
            .copied()
            .with_context(|| format!("the namespace table has no {uri}"))
    }

    /// The namespaces with no Strict twin, each with its elements' local names.
    fn namespaces(&mut self) -> Result<()> {
        let mut found: BTreeMap<u8, BTreeSet<String>> = BTreeMap::new();
        for namespace in self.transitional.namespaces.keys() {
            if !self.to_strict.contains_key(namespace) {
                let id = self.id(namespace)?;
                found.entry(id).or_default();
            }
        }
        for (name, _) in self.transitional.element_tags() {
            if let Some(locals) = self
                .ids
                .get(name.namespace.as_str())
                .and_then(|id| found.get_mut(id))
            {
                locals.insert(name.local.clone());
            }
        }
        self.rules.namespaces = found
            .into_iter()
            .map(|(id, locals)| (id, locals.into_iter().collect()))
            .collect();
        Ok(())
    }

    fn new(
        transitional: &'set SchemaSet,
        strict: &'set SchemaSet,
        rows: &'set [Row],
        table: &'set Correspondences,
    ) -> Result<Self> {
        let mut ids = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            ids.insert(row.uri.as_str(), u8::try_from(index)?);
        }
        let twins = compare::twins(transitional, strict);
        let to_strict = twins
            .iter()
            .map(|(strict_uri, uri)| (uri.clone(), strict_uri.clone()))
            .collect();
        Ok(Self {
            additions: compare::transitional_only(transitional, strict),
            attribute_votes: BTreeMap::new(),
            child_votes: BTreeMap::new(),
            ids,
            parents: parents(transitional),
            problems: Vec::new(),
            rows,
            rules: Rules::default(),
            spelled_by: BTreeMap::new(),
            strict,
            table,
            to_strict,
            transitional,
            twins,
            used: BTreeSet::new(),
        })
    }

    /// Each tag's rules, where its types agree.
    fn settle(&mut self) {
        let attribute_votes = mem::take(&mut self.attribute_votes);
        for ((owner, attribute), votes) in &attribute_votes {
            let rules: BTreeSet<&Option<AttributeRule>> =
                votes.iter().map(|vote| &vote.rule).collect();
            match rules.into_iter().collect::<Vec<_>>().as_slice() {
                [None] => {}
                [Some(rule)] => {
                    self.rules
                        .attributes
                        .insert((owner.clone(), attribute.clone()), rule.clone());
                }
                _ => self.by_parent(owner, attribute, votes),
            }
        }
        for ((owner, child), votes) in &self.child_votes {
            match votes.iter().collect::<Vec<_>>().as_slice() {
                [None] => {}
                [Some(rule)] => {
                    self.rules
                        .children
                        .insert((owner.clone(), child.clone()), rule.clone());
                }
                _ => self.problems.push(format!(
                    "the types of {owner:?} disagree on their child {child:?}: {votes:?}"
                )),
            }
        }
        for rule in self
            .rules
            .attributes
            .values()
            .chain(self.rules.contextual.values())
        {
            if let AttributeRule::Values(spellings) = rule
                && !self.rules.spellings.contains_key(spellings)
            {
                let typed = self
                    .spelled_by
                    .get(spellings)
                    .and_then(|names| names.first())
                    .map_or_else(String::new, |type_name| {
                        let prefix = self
                            .ids
                            .get(type_name.namespace.as_str())
                            .and_then(|id| self.rows.get(usize::from(*id)))
                            .map_or("", |row| row.prefix.as_str());
                        static_name(prefix, &type_name.local)
                    });
                let name = spellings_name(spellings).unwrap_or(typed);
                self.rules.spellings.insert(spellings.clone(), name);
            }
        }
    }

    /// The rule of an attribute both classes declare: the measure its type's
    /// row names, else the spellings of its type, else none.
    fn shared_attribute(
        &mut self,
        owner: &Name,
        local: &str,
        type_ref: &TypeRef,
    ) -> Result<Option<AttributeRule>> {
        if let Some(measure) = self
            .table
            .measures
            .iter()
            .find(|row| row.type_name == *owner && row.attribute == local)
        {
            self.used
                .insert(("measure", owner.clone(), local.to_owned()));
            return Ok(Some(AttributeRule::Measure {
                per_percent: measure.per_percent,
                per_point: measure.per_point,
                percent: measure.percent.clone(),
                unit: measure.unit.clone(),
            }));
        }
        let mut spellings = Vec::new();
        self.spell(type_ref, &mut BTreeSet::new(), &mut spellings)?;
        if spellings.is_empty() {
            return Ok(None);
        }
        if let Some(name) = type_ref.name() {
            self.spelled_by
                .entry(spellings.clone())
                .or_default()
                .insert(name);
        }
        Ok(Some(AttributeRule::Values(spellings)))
    }

    /// What Strict writes for the values of `type_ref`: its own enumeration
    /// values and union members Transitional adds, and its members' and
    /// base's, in that order.
    fn spell(
        &mut self,
        type_ref: &TypeRef,
        seen: &mut BTreeSet<Name>,
        out: &mut Vec<Spelling>,
    ) -> Result<()> {
        let TypeRef::Simple {
            anonymous: _anonymous,
            name,
        } = type_ref
        else {
            return Ok(());
        };
        if !seen.insert(name.clone()) {
            return Ok(());
        }
        if let Some(row) = self.table.values.iter().find(|row| row.type_name == *name) {
            out.push(Spelling::Map(row.values.clone()));
        }
        match self.transitional.simple_types.get(name) {
            Some(SimpleType::Union {
                members,
                name: _union,
            }) => {
                for member in members {
                    let row = member.name().and_then(|member_name| {
                        self.table
                            .members
                            .iter()
                            .find(|row| row.type_name == *name && row.member == member_name)
                    });
                    if let Some(found) = row {
                        out.push(found.spelling.clone());
                    }
                    self.spell(member, seen, out)?;
                }
            }
            Some(SimpleType::Restriction {
                base,
                enumeration: _enumeration,
                facets: _facets,
                name: _restricted,
            }) => self.spell(base, seen, out)?,
            Some(SimpleType::List { item, name: _list }) => {
                let mut inner = Vec::new();
                self.spell(item, seen, &mut inner)?;
                if !inner.is_empty() {
                    bail!("{name} is a list of a type Strict spells otherwise");
                }
            }
            None => {}
        }
        Ok(())
    }

    /// The values only Strict has, by the element and attribute that may
    /// hold them.
    fn strict_only(&mut self) -> Result<()> {
        let only = compare::strict_only(self.transitional, self.strict);
        let values_only = Additions {
            enumeration_values: only.enumeration_values.clone(),
            ..Additions::default()
        };
        if only != values_only {
            bail!("Strict declares more than enumeration values Transitional lacks: {only:?}");
        }
        for (tag, type_ref) in self.transitional.element_tags() {
            let TypeRef::Complex {
                anonymous: _anonymous,
                name,
            } = type_ref
            else {
                continue;
            };
            let Some(declared) = self.transitional.complex_types.get(name) else {
                continue;
            };
            for attribute in self.transitional.effective(declared).attributes {
                let values: Vec<String> = only
                    .enumeration_values
                    .iter()
                    .filter(|(owner, _)| holds(self.transitional, &attribute.type_ref, owner))
                    .map(|(_, value)| value.clone())
                    .collect();
                if values.is_empty() {
                    continue;
                }
                let key = (self.tag(tag)?, self.tag(&attribute.name)?);
                let label = format!("{}/@{}", tag.local, attribute.name.local);
                self.rules.strict_only.insert(key, (values, label));
            }
        }
        Ok(())
    }

    /// The table's tag of a name.
    fn tag(&self, name: &Name) -> Result<Tag> {
        let id = if name.namespace.is_empty() {
            None
        } else {
            Some(self.id(&name.namespace)?)
        };
        Ok((id, name.local.clone()))
    }

    /// The rule of an attribute Transitional adds: the conversion its type's
    /// row gives, else a refusal.
    fn transitional_only_attribute(
        &mut self,
        owner: &Name,
        local: &str,
        label: String,
    ) -> AttributeRule {
        let table = self.table;
        let rule = table
            .attributes
            .iter()
            .filter(|row| row.type_name == *owner)
            .find_map(|row| {
                row.renames
                    .iter()
                    .find(|(from, _)| from == local)
                    .map(|(_, to)| ("attribute", AttributeRule::Rename(to.clone())))
            })
            .or_else(|| {
                table
                    .child_attributes
                    .iter()
                    .find(|row| row.type_name == *owner && row.attribute == local)
                    .map(|row| ("child_attribute", AttributeRule::Child(row.child.clone())))
            })
            .or_else(|| {
                table
                    .packed
                    .iter()
                    .find(|row| row.type_name == *owner && row.attribute == local)
                    .map(|row| {
                        (
                            "packed",
                            AttributeRule::Unpack {
                                bits: row.bits.clone(),
                                label: label.clone(),
                            },
                        )
                    })
            })
            .or_else(|| {
                table
                    .character_sets
                    .iter()
                    .find(|row| row.type_name == *owner && row.attribute == local)
                    .map(|row| {
                        (
                            "character_set",
                            AttributeRule::CharacterSet {
                                codes: codes_name(&row.codes),
                                label: label.clone(),
                                to: row.to.clone(),
                            },
                        )
                    })
            })
            .or_else(|| {
                table
                    .iso_dates
                    .iter()
                    .find(|row| row.type_name == *owner && row.attribute == local)
                    .map(|row| {
                        (
                            "iso_date",
                            AttributeRule::Iso {
                                label: label.clone(),
                                to: row.to.clone(),
                            },
                        )
                    })
            });
        match rule {
            Some((kind, found)) => {
                if let AttributeRule::CharacterSet {
                    codes,
                    label: _label,
                    to: _to,
                } = &found
                    && let Some(row) = table
                        .character_sets
                        .iter()
                        .find(|row| row.type_name == *owner && row.attribute == local)
                {
                    self.rules.codes.insert(codes.clone(), row.codes.clone());
                }
                self.used.insert((kind, owner.clone(), local.to_owned()));
                found
            }
            None => AttributeRule::Refuse(label),
        }
    }

    /// The rule of a child Transitional adds: the Strict name its type's row
    /// gives it, with the siblings between the two names in the Transitional
    /// order, else a refusal.
    fn transitional_only_child(
        &mut self,
        owner: &Name,
        child: &Name,
        ordered: &[Name],
    ) -> ChildRule {
        let renamed = self
            .table
            .children
            .iter()
            .filter(|row| row.type_name == *owner)
            .find_map(|row| {
                row.renames
                    .iter()
                    .find(|(from, _)| *from == child.local)
                    .map(|(_, to)| to.clone())
            });
        let Some(to) = renamed else {
            return ChildRule::Refuse(child.local.clone());
        };
        self.used
            .insert(("child", owner.clone(), child.local.clone()));
        let target = Name::new(&child.namespace, &to);
        let from = ordered.iter().position(|name| name == child);
        let before = ordered
            .iter()
            .position(|name| *name == target)
            .zip(from)
            .filter(|(start, end)| start < end)
            .map_or_default(|(start, end)| {
                ordered[start + 1..end]
                    .iter()
                    .map(|name| name.local.clone())
                    .collect()
            });
        ChildRule::Rename { before, to }
    }

    /// The Strict twin of a Transitional complex type, its base folded in.
    fn twin(&self, name: &Name) -> Option<ComplexType> {
        let strict_namespace = self.to_strict.get(&name.namespace)?;
        let declared = self
            .strict
            .complex_types
            .get(&Name::new(strict_namespace, &name.local))?;
        Some(self.strict.effective(declared))
    }
}

/// Every table the walks read, from the two schema sets, the namespace
/// table's rows, and the correspondences.
///
/// # Errors
///
/// A namespace the table lacks, an addition with no row or a row with no
/// addition, types of one tag that disagree on a rule, or a type Strict
/// spells otherwise inside a list.
pub fn build(
    transitional: &SchemaSet,
    strict: &SchemaSet,
    rows: &[Row],
    table: &Correspondences,
) -> Result<Rules> {
    let mut builder = Builder::new(transitional, strict, rows, table)?;
    for (tag, type_ref) in transitional.element_tags() {
        if !builder.to_strict.contains_key(&tag.namespace) {
            continue;
        }
        if let TypeRef::Complex {
            anonymous: _anonymous,
            name,
        } = type_ref
        {
            builder.complex_type(tag, name)?;
        }
    }
    builder.elements()?;
    builder.namespaces()?;
    builder.strict_only()?;
    builder.check_rows();
    builder.settle();
    if !builder.problems.is_empty() {
        bail!(
            "the Strict correspondences:\n  {}",
            builder.problems.join("\n  ")
        );
    }
    let mut rules = builder.rules;
    for (spellings, name) in &mut rules.spellings {
        if name.is_empty() {
            bail!("no type gives the spellings {spellings:?}");
        }
    }
    Ok(rules)
}

/// Each pair of tag and complex type, with the tags of the elements whose
/// content declares it.
fn parents(set: &SchemaSet) -> BTreeMap<(Name, Name), BTreeSet<Name>> {
    let mut found: BTreeMap<(Name, Name), BTreeSet<Name>> = BTreeMap::new();
    for (parent, type_ref) in set.element_tags() {
        let TypeRef::Complex {
            anonymous: _anonymous,
            name,
        } = type_ref
        else {
            continue;
        };
        let Some(declared) = set.complex_types.get(name) else {
            continue;
        };
        if let Content::Particle(particle) = set.effective(declared).content {
            let mut typed = Vec::new();
            typed_children(set, &particle, &mut BTreeSet::new(), &mut typed);
            for (child, child_type) in typed {
                found
                    .entry((child, child_type))
                    .or_default()
                    .insert(parent.clone());
            }
        }
    }
    found
}

/// The child elements of complex types a particle declares, with their types.
fn typed_children(
    set: &SchemaSet,
    particle: &Particle,
    groups: &mut BTreeSet<Name>,
    found: &mut Vec<(Name, Name)>,
) {
    match &particle.kind {
        ParticleKind::All(items) | ParticleKind::Choice(items) | ParticleKind::Sequence(items) => {
            for item in items {
                typed_children(set, item, groups, found);
            }
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => {}
        ParticleKind::Element { name, type_ref } => {
            if let TypeRef::Complex {
                anonymous: _anonymous,
                name: type_name,
            } = type_ref
            {
                found.push((name.clone(), type_name.clone()));
            }
        }
        ParticleKind::GroupRef(name) => {
            if groups.insert(name.clone())
                && let Some(group) = set.groups.get(name)
            {
                typed_children(set, &group.particle, groups, found);
                groups.remove(name);
            }
        }
    }
}

/// The child elements a content model declares, in its order, each once.
fn children(set: &SchemaSet, content: &Content) -> Vec<Name> {
    let mut found = Vec::new();
    if let Content::Particle(particle) = content {
        gather(set, particle, &mut BTreeSet::new(), &mut found);
    }
    found
}

/// The name of the static holding a code table: the table's key, upper case.
fn codes_name(codes: &Codes) -> String {
    match codes {
        Codes::Byte(_) => "CHARSET".to_owned(),
        Codes::Number(_) => "CODE_PAGE".to_owned(),
    }
}

fn gather(
    set: &SchemaSet,
    particle: &Particle,
    groups: &mut BTreeSet<Name>,
    found: &mut Vec<Name>,
) {
    match &particle.kind {
        ParticleKind::All(items) | ParticleKind::Choice(items) | ParticleKind::Sequence(items) => {
            for item in items {
                gather(set, item, groups, found);
            }
        }
        ParticleKind::Any {
            namespace: _namespace,
            process_contents: _process_contents,
        } => {}
        ParticleKind::Element {
            name,
            type_ref: _type_ref,
        } => {
            if !found.contains(name) {
                found.push(name.clone());
            }
        }
        ParticleKind::GroupRef(name) => {
            if groups.insert(name.clone())
                && let Some(group) = set.groups.get(name)
            {
                gather(set, &group.particle, groups, found);
                groups.remove(name);
            }
        }
    }
}

/// Whether the values of `type_ref` include those of the simple type `owner`.
fn holds(set: &SchemaSet, type_ref: &TypeRef, owner: &Name) -> bool {
    let TypeRef::Simple {
        anonymous: _anonymous,
        name,
    } = type_ref
    else {
        return false;
    };
    if name == owner {
        return true;
    }
    match set.simple_types.get(name) {
        Some(SimpleType::Union {
            members,
            name: _union,
        }) => members.iter().any(|member| holds(set, member, owner)),
        Some(SimpleType::Restriction {
            base,
            enumeration: _enumeration,
            facets: _facets,
            name: _restricted,
        }) => holds(set, base, owner),
        Some(SimpleType::List {
            item: _item,
            name: _list,
        }) => false,
        None => false,
    }
}

/// The name of a list of spellings that holds no map, after its units:
/// `PERCENT_IN_1000THS_TO_2_PLACES`; `None` for a list with a map, which is
/// named after its type.
fn spellings_name(spellings: &[Spelling]) -> Option<String> {
    let mut parts = Vec::new();
    for spelling in spellings {
        let Spelling::Percent {
            decimals,
            per_percent,
        } = spelling
        else {
            return None;
        };
        let unit = if *per_percent == 1 {
            "WHOLE_PERCENT".to_owned()
        } else {
            format!("PERCENT_IN_{per_percent}THS")
        };
        parts.push(decimals.map_or_else(
            || unit.clone(),
            |places| format!("{unit}_TO_{places}_PLACES"),
        ));
    }
    Some(parts.join("_OR_"))
}

/// `S_ST_ON_OFF`: a type's prefix and local name in upper snake case.
#[must_use]
pub fn static_name(prefix: &str, local: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<char> = None;
    for character in format!("{prefix}_{local}").chars() {
        let separator = !character.is_ascii_alphanumeric();
        let boundary = character.is_ascii_uppercase()
            && previous
                .is_some_and(|before| before.is_ascii_lowercase() || before.is_ascii_digit());
        if (separator || boundary) && !out.ends_with('_') {
            out.push('_');
        }
        if !separator {
            out.push(character.to_ascii_uppercase());
        }
        previous = Some(character);
    }
    out
}
