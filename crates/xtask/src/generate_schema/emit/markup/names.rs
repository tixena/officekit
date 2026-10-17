//! The names of the typed tree: module names, the standard's titles
//! for elements and simple types, the override table, and the words a name is
//! made of.

use alloc::collections::BTreeMap;
use core::cmp::Ordering;

use anyhow::{Context as _, Result, bail};

use crate::generate_schema::standard_text::{self, ElementHeading};

/// The connecting words a field name loses where the parent's name was cut
/// from it, the longest first.
const CONNECTORS: [&[&str]; 6] = [
    &["for", "an"],
    &["for", "a"],
    &["of", "the"],
    &["of", "a"],
    &["for"],
    &["of"],
];

/// The URI every Microsoft extension namespace starts with.
const MICROSOFT: &str = "http://schemas.microsoft.com/";

/// Rust's keywords, which a field takes as a raw identifier.
const KEYWORDS: [&str; 51] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "union", "unsafe", "unsized", "use", "virtual", "where", "while",
];

/// Keywords no raw identifier can spell.
const RESERVED: [&str; 4] = ["crate", "self", "super", "Self"];

/// Part 1's sections, by prefix, and the schema file whose namespace each describes.
const PART_1_SECTIONS: [(&str, &str); 22] = [
    ("17.", "wml"),
    ("18.", "sml"),
    ("19.", "pml"),
    ("20.1.", "dml-main"),
    ("20.2.", "dml-picture"),
    ("20.3.", "dml-lockedCanvas"),
    ("20.4.", "dml-wordprocessingDrawing"),
    ("20.5.", "dml-spreadsheetDrawing"),
    ("21.1.", "dml-main"),
    ("21.2.", "dml-chart"),
    ("21.3.", "dml-chartDrawing"),
    ("21.4.", "dml-diagram"),
    ("22.1.", "shared-math"),
    ("22.2.", "shared-documentPropertiesExtended"),
    ("22.3.", "shared-documentPropertiesCustom"),
    ("22.4.", "shared-documentPropertiesVariantTypes"),
    ("22.5.", "shared-customXmlDataProperties"),
    ("22.6.", "shared-bibliography"),
    ("22.7.", "shared-additionalCharacteristics"),
    ("22.8.", "shared-relationshipReference"),
    ("22.9.", "shared-commonSimpleTypes"),
    ("23.", "shared-customXmlSchemaProperties"),
];

/// Part 4's sections, by prefix, and the schema file whose namespace each describes.
const PART_4_SECTIONS: [(&str, &str); 14] = [
    ("14.", "wml"),
    ("15.", "sml"),
    ("16.", "pml"),
    ("17.2.", "dml-main"),
    ("17.3.", "dml-main"),
    ("18.1.", "dml-chart"),
    ("19.1.", "vml-main"),
    ("19.2.", "vml-officeDrawing"),
    ("19.3.", "vml-wordprocessingDrawing"),
    ("19.4.", "vml-spreadsheetDrawing"),
    ("19.5.", "vml-presentationDrawing"),
    ("20.1.", "shared-commonSimpleTypes"),
    ("20.2.", "shared-documentPropertiesExtended"),
    ("20.3.", "shared-documentPropertiesCustom"),
];

/// A heading of the standard: an element or a simple type, its title, and
/// where it stands.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Heading {
    /// The schema type the section's note names, `CT_P`.
    pub content_type: Option<String>,
    /// The schema file the section describes, `wml`.
    pub file: String,
    /// 1 for Part 1, 4 for Part 4.
    pub part: u8,
    pub section: String,
    pub title: String,
}

/// A row of the override table: the name a struct or a simple type takes in
/// place of its title.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Override {
    pub local: String,
    pub name: String,
    /// The namespace's prefix in the namespace table.
    pub prefix: String,
    /// The schema type, `CT_AxDataSource`; for a simple type its own name.
    pub type_local: String,
}

/// The overrides of `name_overrides.toml`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Overrides {
    pub elements: Vec<Override>,
    pub simple_types: Vec<Override>,
}

/// The titles of Part 1 and Part 4, by schema file and local name.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Titles {
    /// `(file, local name)` to its element headings, Part 1's first, each in document order.
    pub elements: BTreeMap<(String, String), Vec<Heading>>,
    /// `(file, ST_ name)` to its simple-type headings, in the same order.
    pub simple_types: BTreeMap<(String, String), Vec<Heading>>,
}

impl Overrides {
    /// Read `name_overrides.toml`: `[[element]]` rows of `namespace`, `local`,
    /// `type`, and `name`, and `[[simple_type]]` rows of `namespace`, `type`,
    /// and `name`.
    ///
    /// # Errors
    ///
    /// Text that is not TOML, or a row missing a field.
    pub fn parse(text: &str) -> Result<Self> {
        let table: toml::Table = text
            .parse()
            .context("the table of name overrides is not TOML")?;
        let rows = |key: &str, element: bool| -> Result<Vec<Override>> {
            let Some(list) = table.get(key) else {
                return Ok(Vec::new());
            };
            list.as_array()
                .with_context(|| format!("`{key}` is not a list of rows"))?
                .iter()
                .map(|row| {
                    let field = |name: &str| -> Result<String> {
                        Ok(row
                            .get(name)
                            .and_then(toml::Value::as_str)
                            .with_context(|| format!("a `{key}` row has no `{name}`"))?
                            .to_owned())
                    };
                    let type_local = field("type")?;
                    Ok(Override {
                        local: if element {
                            field("local")?
                        } else {
                            type_local.clone()
                        },
                        name: field("name")?,
                        prefix: field("namespace")?,
                        type_local,
                    })
                })
                .collect()
        };
        Ok(Self {
            elements: rows("element", true)?,
            simple_types: rows("simple_type", false)?,
        })
    }
}

impl Titles {
    /// Read the element and simple-type headings of both texts.
    #[must_use]
    pub fn read(part_1: &str, part_4: &str) -> Self {
        let mut titles = Self::default();
        for (part, text, sections) in [
            (1, part_1, PART_1_SECTIONS.as_slice()),
            (4, part_4, PART_4_SECTIONS.as_slice()),
        ] {
            for (element, content_type) in standard_text::element_heading_types(text) {
                if let Some(heading) = describe(part, sections, &element, content_type) {
                    titles
                        .elements
                        .entry((heading.file.clone(), element.local.clone()))
                        .or_default()
                        .push(heading);
                }
            }
            let mut simple: Vec<ElementHeading> = standard_text::simple_type_headings(text)
                .into_iter()
                .collect();
            simple.sort_by(|left, right| section_order(&left.section, &right.section));
            for element in simple {
                if let Some(heading) = describe(part, sections, &element, None) {
                    titles
                        .simple_types
                        .entry((heading.file.clone(), element.local.clone()))
                        .or_default()
                        .push(heading);
                }
            }
        }
        titles
    }
}

/// The schema file a section describes, from the section's prefix.
fn describe(
    part: u8,
    sections: &[(&str, &str)],
    element: &ElementHeading,
    content_type: Option<String>,
) -> Option<Heading> {
    let (_, file) = sections
        .iter()
        .filter(|(prefix, _)| element.section.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())?;
    (!file.is_empty()).then(|| Heading {
        content_type,
        file: (*file).to_owned(),
        part,
        section: element.section.clone(),
        title: element.title.clone(),
    })
}

/// The order of two section numbers, level by level.
#[must_use]
pub fn section_order(left: &str, right: &str) -> Ordering {
    let levels = |section: &str| -> Vec<u32> {
        section
            .split('.')
            .map(|level| level.parse().unwrap_or(u32::MAX))
            .collect()
    };
    levels(left).cmp(&levels(right))
}

/// The module of an ECMA-376 schema file: `wml` is `wordprocessing`,
/// `vml-officeDrawing` is `vml_office_drawing`, `dml-chart` is `chart`.
#[must_use]
pub fn ecma_module(file_stem: &str) -> String {
    match file_stem {
        "wml" => "wordprocessing".to_owned(),
        "sml" => "spreadsheet".to_owned(),
        "pml" => "presentation".to_owned(),
        "dml-main" => "drawing".to_owned(),
        "vml-main" => "vml".to_owned(),
        "shared-math" => "math".to_owned(),
        _ => {
            let kept = file_stem
                .strip_prefix("dml-")
                .or_else(|| file_stem.strip_prefix("shared-"))
                .map_or_else(|| file_stem.replacen('-', "_", 1), ToOwned::to_owned);
            snake(&kept)
        }
    }
}

/// The module of a Microsoft extension namespace: its URI's path after
/// `http://schemas.microsoft.com/` in snake case.
#[must_use]
pub fn extension_module(uri: &str) -> Option<String> {
    let path = uri.strip_prefix(MICROSOFT)?;
    let parts: Vec<String> = path
        .split('/')
        .filter(|part| !part.is_empty())
        .map(snake)
        .collect();
    (!parts.is_empty()).then(|| parts.join("_"))
}

/// A camel-case or hyphenated name in snake case: `rsidRPr` is `rsid_r_pr`,
/// `wordprocessingDrawing` is `wordprocessing_drawing`, `ID` is `id`.
#[must_use]
pub fn snake(name: &str) -> String {
    let characters: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().enumerate() {
        if !character.is_ascii_alphanumeric() {
            if !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if character.is_ascii_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let next = characters.get(index + 1).copied();
            let after_lower = previous.is_ascii_lowercase() || previous.is_ascii_digit();
            let acronym_end = previous.is_ascii_uppercase()
                && next.is_some_and(|following| following.is_ascii_lowercase());
            if (after_lower || acronym_end) && !out.is_empty() && !out.ends_with('_') {
                out.push('_');
            }
        }
        out.push(character.to_ascii_lowercase());
    }
    while out.ends_with('_') {
        out.pop();
    }
    out
}

/// The words of a title: split on spaces, hyphens, and slashes, every
/// character other than a letter or a digit dropped (`Don't` is `Dont`).
#[must_use]
pub fn title_words(title: &str) -> Vec<String> {
    title
        .split(|c: char| c.is_whitespace() || c == '-' || c == '/')
        .map(|word| {
            word.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// The words of a `PascalCase` name: `XMLMappingDataBinding` is `XML`,
/// `Mapping`, `Data`, `Binding`; a digit stays with the word before it.
#[must_use]
pub fn name_words(name: &str) -> Vec<String> {
    let characters: Vec<char> = name.chars().collect();
    let mut words: Vec<String> = Vec::new();
    for (index, character) in characters.iter().enumerate() {
        let starts = index == 0
            || (character.is_ascii_uppercase()
                && (characters[index - 1].is_ascii_lowercase()
                    || (characters[index - 1].is_ascii_uppercase()
                        && characters
                            .get(index + 1)
                            .is_some_and(char::is_ascii_lowercase))));
        if starts || words.is_empty() {
            words.push(String::new());
        }
        if let Some(word) = words.last_mut() {
            word.push(*character);
        }
    }
    words
}

/// Words in `PascalCase`: each word's first letter upper-cased, the rest kept.
#[must_use]
pub fn pascal(words: &[String]) -> String {
    words
        .iter()
        .map(|word| {
            let mut characters = word.chars();
            characters.next().map_or_else(String::new, |first| {
                let mut out = first.to_ascii_uppercase().to_string();
                out.push_str(characters.as_str());
                out
            })
        })
        .collect()
}

/// Words in snake case.
#[must_use]
pub fn snake_words(words: &[String]) -> String {
    words
        .iter()
        .map(|word| word.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("_")
}

/// A snake-case name as a Rust identifier: a keyword becomes a raw identifier.
///
/// # Errors
///
/// A keyword no raw identifier can spell, or a name that does not start with a letter.
pub fn identifier(name: &str) -> Result<String> {
    if RESERVED.contains(&name) {
        bail!("`{name}` cannot be a Rust identifier");
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        bail!("`{name}` does not start with a letter");
    }
    Ok(if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    })
}

/// A child field's words: the child's name without the parent's name where
/// the child's starts or ends with it; otherwise without the words that end
/// the parent's and start the child's (`Text Run` and `Run Properties` give
/// `Properties`). The connecting words the cut leaves go with it (`Non-Visual
/// Properties for a Picture` under `Picture` gives `Non Visual Properties`).
#[must_use]
pub fn field_words(parent: &[String], child: &[String]) -> Vec<String> {
    if parent.is_empty() {
        return child.to_vec();
    }
    if child.len() >= parent.len() && same_words(&child[..parent.len()], parent) {
        return without_leading_connector(&child[parent.len()..]).to_vec();
    }
    if child.len() >= parent.len() && same_words(&child[child.len() - parent.len()..], parent) {
        return without_trailing_connector(&child[..child.len() - parent.len()]).to_vec();
    }
    let longest = (1..parent.len().min(child.len()))
        .rev()
        .find(|&count| same_words(&parent[parent.len() - count..], &child[..count]));
    longest.map_or_else(
        || child.to_vec(),
        |count| without_leading_connector(&child[count..]).to_vec(),
    )
}

/// Whether two word lists are the same, case aside.
fn same_words<L, R>(left: &[L], right: &[R]) -> bool
where
    L: AsRef<str>,
    R: AsRef<str>,
{
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(one, other)| one.as_ref().eq_ignore_ascii_case(other.as_ref()))
}

/// The words without the connecting words of `CONNECTORS` that start them.
fn without_leading_connector(words: &[String]) -> &[String] {
    CONNECTORS
        .iter()
        .find(|connector| {
            words.len() >= connector.len() && same_words(&words[..connector.len()], connector)
        })
        .map_or(words, |connector| &words[connector.len()..])
}

/// The words without the connecting words of `CONNECTORS` that end them.
fn without_trailing_connector(words: &[String]) -> &[String] {
    CONNECTORS
        .iter()
        .find(|connector| {
            words.len() >= connector.len()
                && same_words(&words[words.len() - connector.len()..], connector)
        })
        .map_or(words, |connector| &words[..words.len() - connector.len()])
}

/// An enumeration value as a `PascalCase` variant: `mediumKashida` is
/// `MediumKashida`, `0000` is `Value0000`, `--` is `MinusMinus`, the empty
/// text is `Empty`; a word written all in capitals keeps only its first.
#[must_use]
pub fn variant(value: &str) -> String {
    let spelled: String = value
        .chars()
        .map(|c| match c {
            '-' if !value.chars().any(|other| other.is_ascii_alphanumeric()) => {
                " Minus ".to_owned()
            }
            '+' => " Plus ".to_owned(),
            other => other.to_string(),
        })
        .collect();
    let words: Vec<String> = spelled
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| {
            if word.len() > 1 && word.chars().all(|c| !c.is_ascii_lowercase()) {
                let mut characters = word.chars();
                characters.next().map_or_else(String::new, |first| {
                    let mut out = first.to_string();
                    out.push_str(&characters.as_str().to_ascii_lowercase());
                    out
                })
            } else {
                word.to_owned()
            }
        })
        .collect();
    let name = pascal(&words);
    if name.is_empty() {
        "Empty".to_owned()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("Value{name}")
    } else if name == "Self" {
        "SelfValue".to_owned()
    } else {
        name
    }
}

/// The words of a local name: its parts apart by any character other than a
/// letter or a digit, each split where its case changes (`openxmlsdk_49BECFFA`
/// is `Openxmlsdk`, `49BECFFA`; `cNvPr` is `C`, `Nv`, `Pr`).
#[must_use]
pub fn local_words(local: &str) -> Vec<String> {
    local
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .flat_map(|part| name_words(&pascal(&[part.to_owned()])))
        .collect()
}

/// `SCREAMING_SNAKE_CASE` of a `PascalCase` name.
#[must_use]
pub fn screaming(name: &str) -> String {
    snake_words(&name_words(name)).to_ascii_uppercase()
}

#[cfg(test)]
mod tests;
