//! The tests of the Schematron rules in SDK 3.5.1's data, each read into the
//! semantic constraint SDK 3.5.1 registers for it.
//!
//! A test is a path into the relationship part, a uniqueness count, a
//! reference into another part, or a boolean expression over the context
//! element's attributes, and each form gives one kind. Names keep the URI of
//! their prefix, with three exceptions SDK 3.5.1's registrations show: an
//! attribute prefixed `o` or `ovml` has no namespace, and neither has the
//! attribute of a test that is that attribute alone.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;
use core::slice;

use anyhow::{Context as _, Result, bail};

use super::Constraint;
use crate::generate_schema::model::Name;

#[cfg(test)]
mod tests;

/// The comparison operators, the two-character ones first.
const OPERATORS: [&str; 6] = ["!=", "<=", ">=", "=", "<", ">"];

/// How the comparisons of a list are joined.
#[derive(Clone, Copy)]
enum Choice {
    /// By `and`.
    All,
    /// By `or`.
    Any,
}

/// A boolean expression of a test.
#[derive(Clone, Debug, PartialEq)]
enum Expr {
    /// `@name`, the text after `@`.
    Attribute(String),
    /// `and` between the items.
    Both(Vec<Self>),
    /// `function(arguments)`.
    Call(String, Vec<Self>),
    /// `left operator right`.
    Compare(&'static str, Box<Self>, Box<Self>),
    /// `or` between the items.
    Either(Vec<Self>),
    /// A bare word or a quoted text.
    Value(String),
}

/// The namespace URIs a test's prefixes stand for.
#[non_exhaustive]
pub struct Names<'data> {
    /// Prefix to namespace URI, the data's `namespaces.json`.
    pub uris: &'data BTreeMap<String, String>,
}

/// A recursive-descent reader of a test's boolean expression.
struct Reader {
    at: usize,
    tokens: Vec<Token>,
}

/// A lexical item of a test.
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Close,
    Comma,
    Open,
    Operator(&'static str),
    /// A text between quotes, without them.
    Quoted(String),
    Word(String),
}

impl Reader {
    /// `comparison (and comparison)*`.
    fn both(&mut self) -> Result<Expr> {
        let mut items = vec![self.comparison()?];
        while self.next_is_word("and") {
            self.at += 1;
            items.push(self.comparison()?);
        }
        Ok(single_or(items, Expr::Both))
    }

    /// `operand (operator operand)?`.
    fn comparison(&mut self) -> Result<Expr> {
        let left = self.operand()?;
        if let Some(Token::Operator(operator)) = self.tokens.get(self.at) {
            let found = *operator;
            self.at += 1;
            let right = self.operand()?;
            return Ok(Expr::Compare(found, Box::new(left), Box::new(right)));
        }
        Ok(left)
    }

    /// `both (or both)*`.
    fn either(&mut self) -> Result<Expr> {
        let mut items = vec![self.both()?];
        while self.next_is_word("or") {
            self.at += 1;
            items.push(self.both()?);
        }
        Ok(single_or(items, Expr::Either))
    }

    fn expect(&mut self, wanted: &Token) -> Result<()> {
        if self.tokens.get(self.at) != Some(wanted) {
            bail!("expected {wanted:?} at token {}", self.at);
        }
        self.at += 1;
        Ok(())
    }

    fn next_is_word(&self, word: &str) -> bool {
        matches!(self.tokens.get(self.at), Some(Token::Word(found)) if found == word)
    }

    /// `( either )`, `name(operand, ...)`, `@attribute`, or a value.
    fn operand(&mut self) -> Result<Expr> {
        let token = self
            .tokens
            .get(self.at)
            .cloned()
            .context("the test ends early")?;
        self.at += 1;
        match token {
            Token::Open => {
                let inner = self.either()?;
                self.expect(&Token::Close)?;
                Ok(inner)
            }
            Token::Word(word) if self.tokens.get(self.at) == Some(&Token::Open) => {
                self.at += 1;
                let mut arguments = vec![self.operand()?];
                while self.tokens.get(self.at) == Some(&Token::Comma) {
                    self.at += 1;
                    arguments.push(self.operand()?);
                }
                self.expect(&Token::Close)?;
                Ok(Expr::Call(word, arguments))
            }
            Token::Word(word) => Ok(word.strip_prefix('@').map_or_else(
                || Expr::Value(word.clone()),
                |name| Expr::Attribute(name.to_owned()),
            )),
            Token::Quoted(text) => Ok(Expr::Value(text)),
            Token::Close | Token::Comma | Token::Operator(_) => {
                bail!("unexpected {token:?} at token {}", self.at - 1)
            }
        }
    }
}

impl Names<'_> {
    /// An attribute's name: `o` and `ovml`, and a prefix the data lacks, are
    /// no namespace.
    fn attribute(&self, qualified: &str) -> Name {
        match qualified.rsplit_once(':') {
            Some(("o" | "ovml", local)) => Name::new("", local),
            Some((prefix, local)) => {
                Name::new(self.uris.get(prefix).map_or("", String::as_str), local)
            }
            None => Name::new("", qualified),
        }
    }

    /// An element's name: `ovml` is `o`.
    ///
    /// # Errors
    ///
    /// No prefix, or a prefix the data lacks.
    pub fn element(&self, qualified: &str) -> Result<Name> {
        let (prefix, local) = qualified
            .split_once(':')
            .with_context(|| format!("the element {qualified} has no prefix"))?;
        let key = if prefix == "ovml" { "o" } else { prefix };
        let uri = self
            .uris
            .get(key)
            .with_context(|| format!("no namespace has the prefix {prefix}"))?;
        Ok(Name::new(uri, local))
    }
}

/// The attribute of a test that is that attribute alone: `@w:type` is
/// required. Its prefix, `@` included, names no namespace.
fn alone(expr: &Expr) -> Option<Constraint> {
    let Expr::Attribute(name) = expr else {
        return None;
    };
    let local = name
        .rsplit_once(':')
        .map_or(name.as_str(), |(_, local)| local);
    Some(Constraint::CannotBeOmitted {
        attribute: Name::new("", local),
    })
}

/// `@a and @b = x`, `@a and (@b = x or @b = y)`, `@a and @b != x and @b != y`,
/// and `@a and @b`: an attribute absent on a condition, or exclusive ones.
fn and_present(expr: &Expr, names: &Names<'_>) -> Option<Constraint> {
    let Expr::Both(items) = expr else {
        return None;
    };
    let (first, rest) = items.split_first()?;
    let Expr::Attribute(attribute) = first else {
        return None;
    };
    if rest.iter().all(|item| matches!(item, Expr::Attribute(_))) {
        let mut attributes = Vec::new();
        for item in items {
            if let Expr::Attribute(name) = item {
                push_new(&mut attributes, names.attribute(name));
            }
        }
        return Some(Constraint::MutuallyExclusive { attributes });
    }
    if let [condition] = rest
        && let Some((other, values)) = comparisons(condition, "=", Choice::Any)
    {
        return Some(Constraint::AbsentWhenValue {
            attribute: names.attribute(attribute),
            other: names.attribute(&other),
            values,
        });
    }
    let (other, values) = comparisons(&Expr::Both(rest.to_vec()), "!=", Choice::All)?;
    Some(Constraint::AbsentUnlessValue {
        attribute: names.attribute(attribute),
        other: names.attribute(&other),
        values,
    })
}

/// `(@a and @b) or (@a and @c) or ...`: attributes that exclude each other.
fn any_pair(expr: &Expr, names: &Names<'_>) -> Option<Constraint> {
    let Expr::Either(pairs) = expr else {
        return None;
    };
    let mut attributes = Vec::new();
    for pair in pairs {
        let Expr::Both(items) = pair else {
            return None;
        };
        for item in items {
            let Expr::Attribute(name) = item else {
                return None;
            };
            push_new(&mut attributes, names.attribute(name));
        }
    }
    Some(Constraint::MutuallyExclusive { attributes })
}

/// `(@a and @b = x) or @b != x`: an attribute required on a condition; and
/// `(@a = x and @b = y) or @b != y`, with `or` and `and` lists in place of the
/// single comparisons: a value conditioned on another attribute's.
fn conditional(expr: &Expr, names: &Names<'_>) -> Result<Option<Constraint>> {
    let Expr::Either(items) = expr else {
        return Ok(None);
    };
    let [Expr::Both(pair), condition] = items.as_slice() else {
        return Ok(None);
    };
    let [first, second] = pair.as_slice() else {
        return Ok(None);
    };
    let Some((other, other_values)) = comparisons(condition, "!=", Choice::All) else {
        return Ok(None);
    };
    if let Expr::Attribute(attribute) = first {
        let Some((condition_attribute, values)) = comparisons(second, "=", Choice::Any) else {
            return Ok(None);
        };
        let [value] = values.as_slice() else {
            bail!("an attribute required on {} values", values.len());
        };
        return Ok(Some(Constraint::RequiredWhenValue {
            attribute: names.attribute(attribute),
            other: names.attribute(&condition_attribute),
            value: value.clone(),
        }));
    }
    let Some((attribute, values)) = comparisons(first, "=", Choice::Any) else {
        return Ok(None);
    };
    Ok(Some(Constraint::ValueConditioned {
        attribute: names.attribute(&attribute),
        other: names.attribute(&other),
        other_values,
        values,
    }))
}

/// `@a op x`, or a list of them joined as `choice` says, all on one
/// attribute: the attribute and the values in order.
fn comparisons(expr: &Expr, operator: &str, choice: Choice) -> Option<(String, Vec<String>)> {
    let items = match (choice, expr) {
        (Choice::All, Expr::Both(items)) | (Choice::Any, Expr::Either(items)) => items.as_slice(),
        _ => slice::from_ref(expr),
    };
    let mut attribute: Option<&String> = None;
    let mut values = Vec::new();
    for item in items {
        let Expr::Compare(found, left, right) = item else {
            return None;
        };
        let (Expr::Attribute(name), Expr::Value(value)) = (left.as_ref(), right.as_ref()) else {
            return None;
        };
        if *found != operator || attribute.is_some_and(|known| known != name) {
            return None;
        }
        attribute = Some(name);
        values.push(value.clone());
    }
    Some((attribute?.clone(), values))
}

/// `count(distinct-values(path)) = count(path)`, the path optionally under
/// `lower-case(...)` and starting with `ancestor::e`: unique values.
fn count_distinct(test: &str, names: &Names<'_>) -> Result<Option<Constraint>> {
    let Some((left, _)) = test
        .strip_prefix("count(distinct-values(")
        .and_then(|rest| rest.split_once(")) = count("))
    else {
        return Ok(None);
    };
    let (case_sensitive, path) = left
        .strip_prefix("lower-case(")
        .and_then(|inner| inner.strip_suffix(')'))
        .map_or((true, left), |inner| (false, inner));
    let (parent, rest) = match path.strip_prefix("ancestor::") {
        Some(scoped) => {
            let (ancestor, inner) = scoped
                .split_once("//")
                .with_context(|| format!("no `//` after the ancestor in {test}"))?;
            (Some(names.element(ancestor)?), inner)
        }
        None => (
            None,
            path.strip_prefix("//")
                .with_context(|| format!("no `//` in {test}"))?,
        ),
    };
    let (_, attribute) = rest
        .rsplit_once("/@")
        .with_context(|| format!("no attribute in {test}"))?;
    Ok(Some(Constraint::UniqueValue {
        attribute: names.attribute(attribute),
        case_sensitive,
        parent,
    }))
}

/// `@a < count(document('Part:p')//e) + n`, `e` optionally `parent/element`:
/// an index into another part.
fn count_reference(test: &str, names: &Names<'_>) -> Result<Option<Constraint>> {
    let Some((attribute, counted)) = test
        .strip_prefix('@')
        .and_then(|after| after.split_once(" < count(document('Part:"))
    else {
        return Ok(None);
    };
    let (part, after_part) = counted
        .split_once("')//")
        .with_context(|| format!("no part path in {test}"))?;
    let (path, base) = after_part
        .split_once(") + ")
        .with_context(|| format!("no index base in {test}"))?;
    let (parent, element) = match path.split_once('/') {
        Some((parent, element)) => (Some(names.element(parent)?), element),
        None => (None, path),
    };
    Ok(Some(Constraint::IndexReference {
        attribute: names.attribute(attribute),
        element: names.element(element)?,
        index_base: base
            .parse()
            .with_context(|| format!("the index base of {test}"))?,
        parent,
        part: part.to_owned(),
    }))
}

/// `Index-of(document('Part:p')//path/@b, @a)`: a reference to an element of
/// another part, the last element of the path.
fn index_of(test: &str, names: &Names<'_>) -> Result<Option<Constraint>> {
    let Some(arguments) = test.strip_prefix("Index-of(document('Part:") else {
        return Ok(None);
    };
    let (part, after_part) = arguments
        .split_once("')//")
        .with_context(|| format!("no part path in {test}"))?;
    let (path, attributes) = after_part
        .split_once("/@")
        .with_context(|| format!("no element attribute in {test}"))?;
    let (element_attribute, attribute) = attributes
        .strip_suffix(')')
        .and_then(|inner| inner.split_once(", @"))
        .with_context(|| format!("no attribute in {test}"))?;
    let element_name = path.rsplit('/').next().unwrap_or(path);
    Ok(Some(Constraint::ReferenceExists {
        attribute: names.attribute(attribute),
        element: names.element(element_name)?,
        element_attribute: names.attribute(element_attribute),
        element_name: element_name.to_owned(),
        part: part.to_owned(),
    }))
}

/// `string-length(@a) op n`, or `>= n and <= m` of it: a length bound.
fn length(expr: &Expr, names: &Names<'_>) -> Result<Option<Constraint>> {
    let bounds = match expr {
        Expr::Both(items) => items.iter().collect::<Vec<&Expr>>(),
        Expr::Compare(..) => vec![expr],
        Expr::Attribute(_) | Expr::Call(..) | Expr::Either(_) | Expr::Value(_) => {
            return Ok(None);
        }
    };
    let mut attribute = None;
    let mut min = 0;
    let mut max = i32::MAX.unsigned_abs();
    for bound in bounds {
        let Expr::Compare(operator, left, right) = bound else {
            return Ok(None);
        };
        let (Expr::Call(function, arguments), Expr::Value(number)) =
            (left.as_ref(), right.as_ref())
        else {
            return Ok(None);
        };
        let ([Expr::Attribute(name)], "string-length") = (arguments.as_slice(), function.as_str())
        else {
            return Ok(None);
        };
        let value: u32 = number
            .parse()
            .with_context(|| format!("the length {number}"))?;
        match *operator {
            ">=" => min = value,
            "<=" => max = value,
            _ => bail!("a length compared with {operator}"),
        }
        attribute = Some(names.attribute(name));
    }
    Ok(attribute.map(|found| Constraint::ValueLength {
        attribute: found,
        max,
        min,
    }))
}

/// `matches(@a, "pattern")`: the pattern anchored at both ends, a backslash
/// before an apostrophe dropped.
fn matches(expr: &Expr, names: &Names<'_>) -> Option<Constraint> {
    let Expr::Call(function, arguments) = expr else {
        return None;
    };
    let ("matches", [Expr::Attribute(name), Expr::Value(pattern)]) =
        (function.as_str(), arguments.as_slice())
    else {
        return None;
    };
    let unescaped = pattern.replace("\\'", "'");
    let anchored = if unescaped.starts_with('^') && unescaped.ends_with('$') {
        unescaped
    } else {
        format!("^{unescaped}$")
    };
    Some(Constraint::ValuePattern {
        attribute: names.attribute(name),
        pattern: anchored,
    })
}

/// A number of a range as the data writes it: `0x` hexadecimal, a `f` suffix,
/// or a decimal number with an optional exponent.
fn number(text: &str) -> Result<f64> {
    if let Some(hex) = text.strip_prefix("0x") {
        let value = u32::from_str_radix(hex, 16).with_context(|| format!("the number {text}"))?;
        return Ok(f64::from(value));
    }
    text.strip_suffix('f')
        .unwrap_or(text)
        .parse()
        .with_context(|| format!("the number {text}"))
}

fn push_new(names: &mut Vec<Name>, name: Name) {
    if !names.contains(&name) {
        names.push(name);
    }
}

/// `@a op n`, `@a op n and @a op m`, and `@a op @b`: a range, or a bound by
/// another attribute.
fn range(expr: &Expr, names: &Names<'_>) -> Result<Option<Constraint>> {
    let bounds = match expr {
        Expr::Both(items) if items.len() == 2 => items.iter().collect::<Vec<&Expr>>(),
        Expr::Compare(..) => vec![expr],
        Expr::Attribute(_) | Expr::Both(_) | Expr::Call(..) | Expr::Either(_) | Expr::Value(_) => {
            return Ok(None);
        }
    };
    if let [Expr::Compare(operator @ ("<" | "<="), left, right)] = bounds.as_slice()
        && let (Expr::Attribute(attribute), Expr::Attribute(other)) =
            (left.as_ref(), right.as_ref())
    {
        return Ok(Some(Constraint::LessOrEqual {
            attribute: names.attribute(attribute),
            can_equal: *operator == "<=",
            other: names.attribute(other),
        }));
    }
    let mut parsed = Vec::new();
    for bound in &bounds {
        let Expr::Compare(operator @ ("<" | "<=" | ">" | ">="), left, right) = bound else {
            return Ok(None);
        };
        let (Expr::Attribute(name), Expr::Value(text)) = (left.as_ref(), right.as_ref()) else {
            return Ok(None);
        };
        parsed.push((*operator, name, number(text)?));
    }
    Ok(match parsed.as_slice() {
        [(operator @ (">" | ">="), name, min)] => Some(Constraint::ValueRange {
            attribute: names.attribute(name),
            max: f64::INFINITY,
            max_inclusive: true,
            min: *min,
            min_inclusive: *operator == ">=",
        }),
        [(operator, name, max)] => Some(Constraint::ValueRange {
            attribute: names.attribute(name),
            max: *max,
            max_inclusive: *operator == "<=",
            min: f64::NEG_INFINITY,
            min_inclusive: true,
        }),
        [(lower, name, min), (upper, _, max)] => Some(Constraint::ValueRange {
            attribute: names.attribute(name),
            max: *max,
            max_inclusive: *upper == "<=" || *lower == "<=",
            min: *min,
            min_inclusive: *lower == ">=" || *upper == ">=",
        }),
        _ => None,
    })
}

/// The constraint the test of a rule gives, its names resolved through `names`.
///
/// # Errors
///
/// A test of no known form, or a form whose parts do not read.
pub fn read(test: &str, names: &Names<'_>) -> Result<Constraint> {
    if let Some(found) = relationship(test, names) {
        return Ok(found);
    }
    if let Some(found) = count_distinct(test, names)? {
        return Ok(found);
    }
    if let Some(found) = index_of(test, names)? {
        return Ok(found);
    }
    if let Some(found) = count_reference(test, names)? {
        return Ok(found);
    }
    let mut reader = Reader {
        at: 0,
        tokens: tokens(test)?,
    };
    let expr = reader.either()?;
    if reader.at != reader.tokens.len() {
        bail!("the test {test} goes on after its expression");
    }
    if let Some(found) = alone(&expr).or_else(|| matches(&expr, names)) {
        return Ok(found);
    }
    if let Some(found) = length(&expr, names)? {
        return Ok(found);
    }
    if let Some(found) = range(&expr, names)? {
        return Ok(found);
    }
    if let Some(found) = value_set(&expr, names) {
        return Ok(found);
    }
    if let Some(found) = and_present(&expr, names).or_else(|| any_pair(&expr, names)) {
        return Ok(found);
    }
    conditional(&expr, names)?.with_context(|| format!("the test {test} has no known form"))
}

/// `document(rels)//r:Relationship[@Id = current()/@a]`, and the same with
/// `/@Type = 'type'`: a relationship, of a type when given.
fn relationship(test: &str, names: &Names<'_>) -> Option<Constraint> {
    let rest = test.strip_prefix("document(rels)//r:Relationship[@Id = current()/@")?;
    let (attribute, after) = rest.split_once(']')?;
    if after.is_empty() {
        return Some(Constraint::RelationshipExists {
            attribute: names.attribute(attribute),
        });
    }
    let relationship_type = after
        .strip_prefix("/@Type = '")
        .and_then(|inner| inner.strip_suffix('\''))?;
    Some(Constraint::RelationshipType {
        attribute: names.attribute(attribute),
        relationship_type: relationship_type.to_owned(),
    })
}

fn single_or(mut items: Vec<Expr>, many: fn(Vec<Expr>) -> Expr) -> Expr {
    if items.len() == 1
        && let Some(only) = items.pop()
    {
        return only;
    }
    many(items)
}

/// The lexical items of a test.
fn tokens(test: &str) -> Result<Vec<Token>> {
    let mut found = Vec::new();
    let mut rest = test;
    while let Some(character) = rest.chars().next() {
        if character.is_whitespace() {
            rest = &rest[character.len_utf8()..];
            continue;
        }
        let (token, used) = match character {
            '(' => (Token::Open, 1),
            ')' => (Token::Close, 1),
            ',' => (Token::Comma, 1),
            '\'' | '"' => {
                let end = rest[1..]
                    .find(character)
                    .with_context(|| format!("an unclosed quote in {test}"))?;
                (Token::Quoted(rest[1..=end].to_owned()), end + 2)
            }
            _ => {
                if let Some(operator) = OPERATORS
                    .iter()
                    .find(|operator| rest.starts_with(**operator))
                {
                    (Token::Operator(operator), operator.len())
                } else {
                    let end = rest
                        .find(|stop: char| stop.is_whitespace() || "()=<>!,".contains(stop))
                        .unwrap_or(rest.len());
                    if end == 0 {
                        bail!("an unexpected {character} in {test}");
                    }
                    (Token::Word(rest[..end].to_string()), end)
                }
            }
        };
        found.push(token);
        rest = &rest[used..];
    }
    Ok(found)
}

/// `@a = x or @a = y ...`, or `@a != x and @a != y ...`: the values allowed,
/// or refused.
fn value_set(expr: &Expr, names: &Names<'_>) -> Option<Constraint> {
    if let Some((attribute, values)) = comparisons(expr, "=", Choice::Any) {
        return Some(Constraint::ValueSet {
            attribute: names.attribute(&attribute),
            valid: true,
            values,
        });
    }
    let (attribute, values) = comparisons(expr, "!=", Choice::All)?;
    Some(Constraint::ValueSet {
        attribute: names.attribute(&attribute),
        valid: false,
        values,
    })
}
