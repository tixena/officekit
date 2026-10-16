//! The generator's reader of the standard's text: ECMA-376 Part 1 and Part 4 as `pdftotext -layout` writes them.
//!
//! `just inputs` runs `pdftotext` once per PDF into the inputs folder; the generator reads the text through
//! `os_shim::System`, so nothing in `crates/` starts a process.

use alloc::collections::{BTreeMap, BTreeSet};

const SECTION_SIGN: char = '\u{a7}';
const PART_1_REFERENCE: &str = " (Part 1, \u{a7}";
const PART_SECTIONS: [&str; 5] = ["11.3.", "12.3.", "13.3.", "14.2.", "15.2."];
const LABELS: [&str; 7] = [
    "Content Type:",
    "Content",
    "Type(s):",
    "Root",
    "Namespace:",
    "Source",
    "Relationship:",
];

/// `17.3.1.22 p (Paragraph)`: a section number of three to five levels, a local name, and a title starting upper case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct ElementHeading {
    pub local: String,
    pub section: String,
    pub title: String,
}

/// A part definition of Part 1 (sections 11.3, 12.3, 13.3, 14.2, 15.2), with the Strict URIs it states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct PartDefinition {
    /// The field's text when it names no MIME type: "Any supported image type.", "application-defined" content.
    pub content_type_text: Option<String>,
    /// Every MIME type the "Content Type(s):" field names, in order.
    pub content_types: Vec<String>,
    /// "This part is defined in §13.2.1 ... of ECMA-376-2": the part's definition is in Part 2.
    pub defined_in_part_2: bool,
    /// From "The root element for a part of this content type shall be document."
    pub root_element: Option<String>,
    /// The "Root Namespace:" field: a URI, or text such as "not applicable".
    pub root_namespace: Option<String>,
    pub section: String,
    /// The "Source Relationship:" field.
    pub source_relationship: Option<String>,
    pub title: String,
}

/// A part of Part 4's part summaries (sections 9.2 to 13.2), with the Transitional URIs that differ from Part 1's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct TransitionalPart {
    /// The Part 1 section the heading names: `11.3.10` in `9.2.10 Main Document Part (Part 1, §11.3.10)`.
    pub part_1_section: String,
    pub root_namespace: Option<String>,
    pub section: String,
    pub source_relationship: Option<String>,
    pub title: String,
}

/// A section of Part 1 outside the part definitions that states a relationship type: `15.3 Hyperlinks`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct RelationshipSection {
    pub section: String,
    /// The Strict relationship type of the "Source Relationship:" field.
    pub source_relationship: String,
    pub title: String,
}

/// The text's lines without page furniture: a line that holds only a page number, and a line starting with a form
/// feed, which holds the next page's running header.
#[must_use]
pub fn lines(text: &str) -> Vec<&str> {
    let page_number = |line: &str| {
        let trimmed = line.trim();
        !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit())
    };
    text.lines()
        .filter(|line| !line.starts_with('\u{c}') && !page_number(line))
        .collect()
}

fn section_number(token: &str, min_levels: usize, max_levels: usize) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    (min_levels..=max_levels).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Every element heading of Part 1's body, once each: lines of the table of contents, which carry a dot leader, are
/// left out.
#[must_use]
pub fn element_headings(text: &str) -> BTreeSet<ElementHeading> {
    headings(text, false)
}

/// The same for simple types: `17.18.44 ST_Jc (Horizontal Alignment Type)`, the local name starting with `ST_`.
#[must_use]
pub fn simple_type_headings(text: &str) -> BTreeSet<ElementHeading> {
    headings(text, true)
}

fn headings(text: &str, simple_types: bool) -> BTreeSet<ElementHeading> {
    text.lines()
        .filter_map(|line| heading(line, simple_types))
        .collect()
}

/// The heading a line is, when it is one: a body line, not the table of contents.
fn heading(line: &str, simple_types: bool) -> Option<ElementHeading> {
    if line.contains("....") {
        return None;
    }
    let trimmed = line.trim();
    let (section, rest) = trimmed.split_once(char::is_whitespace)?;
    if !section_number(section, 3, 5) {
        return None;
    }
    let (local, raw_title) = rest.trim_start().split_once(char::is_whitespace)?;
    let title = raw_title.trim();
    let name = if simple_types {
        local.strip_prefix("ST_")
    } else {
        Some(local)
    };
    let valid_local = name.is_some_and(|n| {
        n.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            && n.chars().all(|c| c.is_ascii_alphanumeric())
    });
    let inner = title.strip_prefix('(').and_then(|t| t.strip_suffix(')'))?;
    if !valid_local || inner.contains(')') || !inner.starts_with(|c: char| c.is_ascii_uppercase()) {
        return None;
    }
    Some(ElementHeading {
        local: local.to_owned(),
        section: section.to_owned(),
        title: inner.split_whitespace().collect::<Vec<_>>().join(" "),
    })
}

/// Every element heading of the body, in document order, with the schema type
/// its section names.
///
/// The type is the one of the note "The W3C XML Schema definition of this
/// element's content model (`CT_P`) is located in ...", searched up to the
/// next heading; `None` when the section has no such note.
#[must_use]
pub fn element_heading_types(text: &str) -> Vec<(ElementHeading, Option<String>)> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<(usize, ElementHeading)> = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| heading(line, false).map(|found| (index, found)))
        .collect();
    let mut found = Vec::with_capacity(starts.len());
    for (position, (index, element)) in starts.iter().enumerate() {
        let end = starts
            .get(position + 1)
            .map_or(lines.len(), |(next, _)| *next);
        let body = lines[index + 1..end].join(" ");
        let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
        let content_type = collapsed
            .split("content model (")
            .nth(1)
            .and_then(|rest| rest.split(')').next())
            .map(str::to_owned);
        found.push((element.clone(), content_type));
    }
    found
}

/// The label fragment a field line starts with, and the value after it.
fn field_line(line: &str) -> Option<(Option<&'static str>, &str)> {
    if line.trim().is_empty() {
        return None;
    }
    if let Some(label) = LABELS.iter().find(|label| {
        line.starts_with(*label)
            && line[label.len()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace)
    }) {
        return Some((Some(label), line[label.len()..].trim()));
    }
    // A continuation: the value column, past the labels.
    line.starts_with("          ").then(|| (None, line.trim()))
}

/// The fields after a part heading, read until the first line that is neither blank, a label, nor a continuation.
/// A label can wrap: `Content` then `Type(s):`, `Root` then `Namespace:`, `Source` then `Relationship:`.
fn fields(lines: &[&str]) -> Vec<(String, Vec<String>)> {
    let mut fields: Vec<(String, Vec<String>)> = Vec::new();
    let mut wrapping = false;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let Some((label_fragment, value)) = field_line(line) else {
            break;
        };
        if let Some(fragment) = label_fragment {
            match fields.last_mut() {
                Some((label, _)) if wrapping => *label = format!("{label} {fragment}"),
                _ => fields.push((fragment.to_owned(), Vec::new())),
            }
            wrapping = !fragment.ends_with(':');
        }
        if let (false, Some((_, values))) = (value.is_empty(), fields.last_mut()) {
            values.push(value.to_owned());
        }
    }
    fields
}

fn field<'fields>(
    fields: &'fields [(String, Vec<String>)],
    name: &str,
) -> Option<&'fields Vec<String>> {
    fields
        .iter()
        .find(|(label, _)| label.split_whitespace().next() == Some(name))
        .map(|(_, values)| values)
}

/// A field value as the text states it: a URI is its first line's first word followed
/// directly by each continuation line's first word, a word ending with `/` taking the
/// next word of its line too (`.../relationships/ htmlPubSaveAs`); other text is joined
/// with spaces.
fn field_value(values: &[String]) -> String {
    let is_uri = values
        .first()
        .is_some_and(|first| first.starts_with("http://") || first.starts_with("https://"));
    if is_uri {
        let mut uri = String::new();
        for value in values {
            let mut words = value.split_whitespace();
            let first = words.next().unwrap_or_default();
            uri.push_str(first);
            if first.ends_with('/') {
                uri.push_str(words.next().unwrap_or_default());
            }
        }
        uri
    } else {
        values.join(" ")
    }
}

/// Every part definition of Part 1, in document order.
#[must_use]
pub fn part_definitions(text: &str) -> Vec<PartDefinition> {
    let lines = lines(text);
    let mut parts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let Some((section, raw_title)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let title = raw_title.trim();
        if !PART_SECTIONS
            .iter()
            .any(|prefix| section.starts_with(prefix))
            || !section_number(section, 3, 4)
            || !title.ends_with("Part")
            || !title.starts_with(|c: char| c.is_ascii_uppercase())
            || !title.chars().all(|c| c.is_ascii_alphabetic() || c == ' ')
        {
            continue;
        }
        let after = &lines[index + 1..];
        let found = fields(after);
        let mut part = PartDefinition {
            section: section.to_owned(),
            title: title.to_owned(),
            ..PartDefinition::default()
        };
        // A field that starts with a MIME type lists one per line, each line's first word; one that starts with prose
        // ("Any supported image type.") names none, and its examples in a note are not a list.
        if let Some(values) = field(&found, "Content") {
            let mime = |token: &str| {
                token.contains('/')
                    && !token.contains("://")
                    && token.starts_with(|c: char| c.is_ascii_lowercase())
            };
            if values
                .first()
                .and_then(|v| v.split_whitespace().next())
                .is_some_and(mime)
            {
                part.content_types = values
                    .iter()
                    .filter_map(|v| v.split_whitespace().next())
                    .filter(|t| mime(t))
                    .map(str::to_owned)
                    .collect();
            } else {
                let prose = values.join(" ");
                part.content_type_text = Some(
                    prose
                        .split("[Note:")
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_owned(),
                );
            }
        }
        part.root_namespace = field(&found, "Root").map(|values| field_value(values));
        part.source_relationship = field(&found, "Source").map(|values| field_value(values));
        let next_heading = after
            .iter()
            .position(|l| {
                let t = l.trim();
                t.split_once(char::is_whitespace).is_some_and(|(s, rest)| {
                    section_number(s, 2, 4)
                        && rest.trim().starts_with(|c: char| c.is_ascii_uppercase())
                        && !l.contains(SECTION_SIGN)
                })
            })
            .unwrap_or(after.len());
        let body = after[..next_heading].join(" ");
        part.defined_in_part_2 = body.contains("of ECMA-376-2");
        part.root_element = root_element(&body);
        parts.push(part);
    }
    parts
}

/// The root element a definition's text names: the word after "shall be" in
/// "The root element for a part of this content type shall be document." or
/// "The root element for a Header part shall be hdr.", within one sentence.
fn root_element(body: &str) -> Option<String> {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let (_, rest) = collapsed.split_once("The root element for a")?;
    let (subject, value) = rest.split_once(" shall be ")?;
    if subject.contains('.') {
        return None;
    }
    value
        .split(|c: char| c == '.' || c == ',' || c.is_whitespace())
        .next()
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
}

/// Every part of Part 4's part summaries, in document order.
#[must_use]
pub fn transitional_parts(text: &str) -> Vec<TransitionalPart> {
    restatements(text, |section| {
        section_number(section, 3, 3)
            && ["9.2.", "10.2.", "11.2.", "12.2.", "13.2."]
                .iter()
                .any(|p| section.starts_with(p))
    })
}

/// Every relationship section of Part 4 (`9.3 Document Template (Part 1, §11.4)`), in document order, as the
/// part it restates: the Part 1 section it names and the Transitional relationship type it states.
#[must_use]
pub fn transitional_relationships(text: &str) -> Vec<TransitionalPart> {
    restatements(text, |section| {
        section_number(section, 2, 2)
            && ["9.", "10.", "11.", "12.", "13."]
                .iter()
                .any(|p| section.starts_with(p))
    })
    .into_iter()
    .filter(|restated| restated.source_relationship.is_some())
    .collect()
}

/// Every heading of Part 4 that names a Part 1 section and whose number `keep` accepts.
fn restatements(text: &str, keep: impl Fn(&str) -> bool) -> Vec<TransitionalPart> {
    let lines = lines(text);
    let mut parts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.contains("....") {
            continue;
        }
        let trimmed = line.trim();
        let Some((section, rest)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let Some((title, reference)) = rest.trim().split_once(PART_1_REFERENCE) else {
            continue;
        };
        let Some(part_1_section) = reference.strip_suffix(')') else {
            continue;
        };
        if !keep(section) {
            continue;
        }
        let found = fields(&lines[index + 1..]);
        parts.push(TransitionalPart {
            part_1_section: part_1_section.to_owned(),
            root_namespace: field(&found, "Root").map(|values| field_value(values)),
            section: section.to_owned(),
            source_relationship: field(&found, "Source").map(|values| field_value(values)),
            title: title.trim().to_owned(),
        });
    }
    parts
}

/// Every section of clauses 11 to 15 of Part 1 that is not a part definition and states a "Source Relationship:",
/// in document order.
#[must_use]
pub fn relationship_sections(text: &str) -> Vec<RelationshipSection> {
    let lines = lines(text);
    let mut sections = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if line.contains("....") {
            continue;
        }
        let Some((section, raw_title)) = line.trim().split_once(char::is_whitespace) else {
            continue;
        };
        let title = raw_title.trim();
        let clause = ["11.", "12.", "13.", "14.", "15."]
            .iter()
            .any(|prefix| section.starts_with(prefix));
        let part_definition = PART_SECTIONS
            .iter()
            .any(|prefix| section.starts_with(prefix))
            && title.ends_with("Part");
        if !clause
            || !section_number(section, 2, 4)
            || part_definition
            || !title.starts_with(|c: char| c.is_ascii_uppercase())
        {
            continue;
        }
        let found = fields(&lines[index + 1..]);
        if let Some(values) = field(&found, "Source") {
            sections.push(RelationshipSection {
                section: section.to_owned(),
                source_relationship: field_value(values),
                title: title.to_owned(),
            });
        }
    }
    sections
}

/// The counts the generator reports over the two texts, on one line.
#[must_use]
pub fn summary(part_1: &str, part_4: &str) -> String {
    let elements = element_headings(part_1);
    let mut per_name: BTreeMap<&str, usize> = BTreeMap::new();
    for heading in &elements {
        *per_name.entry(heading.local.as_str()).or_default() += 1;
    }
    let several = per_name.values().filter(|&&count| count > 1).count();
    let words = |heading: &ElementHeading| heading.title.split_whitespace().count();
    let shortest = elements.iter().map(words).min().unwrap_or(0);
    let longest = elements.iter().map(words).max().unwrap_or(0);
    let simple_types = simple_type_headings(part_1);
    let simple_names: BTreeSet<&str> = simple_types.iter().map(|h| h.local.as_str()).collect();
    let definitions = part_definitions(part_1);
    let defined =
        |test: fn(&PartDefinition) -> bool| definitions.iter().filter(|p| test(p)).count();
    let transitional = transitional_parts(part_4);
    let joined = transitional
        .iter()
        .filter(|part| definitions.iter().any(|d| d.section == part.part_1_section))
        .count();
    format!(
        "{} element headings over {} local names, {several} under several headings, titles of \
         {shortest} to {longest} words; {} simple-type headings over {} names; {} part \
         definitions: {} list MIME types, {} state prose, {} defined in Part 2, {} state a root \
         namespace, {} a source relationship, {} name a root element; {} Transitional parts: {} \
         joined to a Part 1 definition, {} with a root namespace, {} with a source relationship",
        elements.len(),
        per_name.len(),
        simple_types.len(),
        simple_names.len(),
        definitions.len(),
        defined(|p| !p.content_types.is_empty()),
        defined(|p| p.content_type_text.is_some()),
        defined(|p| p.defined_in_part_2),
        defined(|p| p.root_namespace.is_some()),
        defined(|p| p.source_relationship.is_some()),
        defined(|p| p.root_element.is_some()),
        transitional.len(),
        joined,
        transitional
            .iter()
            .filter(|p| p.root_namespace.is_some())
            .count(),
        transitional
            .iter()
            .filter(|p| p.source_relationship.is_some())
            .count(),
    )
}

#[cfg(test)]
mod tests;
