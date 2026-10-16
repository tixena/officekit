//! `strict_correspondences.toml`: what a Strict save writes for each
//! Transitional form the comparison of the two schema sets finds.

use anyhow::{Context as _, Result, bail};

use crate::generate_schema::emit::namespaces::Row;
use crate::generate_schema::model::{Name, XSD_NAMESPACE};

/// The attributes of a type Strict names otherwise.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct AttributeRenames {
    /// `(Transitional, Strict)` local names.
    pub renames: Vec<(String, String)>,
    pub type_name: Name,
}

/// How a packed attribute's bits are laid out.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Bits {
    /// A string of `0` and `1`: the attribute of each character, first first.
    Characters(Vec<String>),
    /// A hexadecimal number: each attribute with its bit, by bit.
    Hex(Vec<(u16, String)>),
}

/// An attribute holding a character-set code.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CharacterSet {
    pub attribute: String,
    pub codes: Codes,
    pub to: String,
    pub type_name: Name,
}

/// An attribute Strict writes as a child element.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildAttribute {
    pub attribute: String,
    pub child: String,
    pub type_name: Name,
}

/// The children of a type Strict names otherwise.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ChildRenames {
    /// `(Transitional, Strict)` local names.
    pub renames: Vec<(String, String)>,
    pub type_name: Name,
}

/// A table of codes with the names Strict writes for them.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Codes {
    /// One-byte codes, written in hexadecimal.
    Byte(Vec<(u8, String)>),
    /// Numbers, written in decimal.
    Number(Vec<(u32, String)>),
}

/// The whole table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Correspondences {
    pub attributes: Vec<AttributeRenames>,
    pub character_sets: Vec<CharacterSet>,
    pub child_attributes: Vec<ChildAttribute>,
    pub children: Vec<ChildRenames>,
    pub iso_dates: Vec<IsoDate>,
    pub measures: Vec<Measure>,
    pub members: Vec<Member>,
    pub packed: Vec<Packed>,
    pub values: Vec<Values>,
}

/// A serial date-time attribute Strict writes in ISO 8601.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct IsoDate {
    pub attribute: String,
    pub to: String,
    pub type_name: Name,
}

/// A measure whose unit another attribute names.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Measure {
    pub attribute: String,
    pub per_percent: u32,
    pub per_point: u32,
    /// The unit's value that makes the measure a percentage.
    pub percent: String,
    pub type_name: Name,
    /// The attribute naming the unit.
    pub unit: String,
}

/// A union member Transitional adds, with what Strict writes for its values.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Member {
    pub member: Name,
    pub spelling: Spelling,
    pub type_name: Name,
}

/// A packed attribute.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Packed {
    pub attribute: String,
    pub bits: Bits,
    pub type_name: Name,
}

/// What Strict writes for a value.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum Spelling {
    /// `(Transitional, Strict)` spellings.
    Map(Vec<(String, String)>),
    /// A bare number counting `per_percent` of a percent, written as a
    /// percentage with at most `decimals` decimal places.
    Percent {
        decimals: Option<u32>,
        per_percent: u32,
    },
}

/// Enumeration values Transitional adds, with Strict's for each.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Values {
    pub type_name: Name,
    /// `(Transitional, Strict)` values.
    pub values: Vec<(String, String)>,
}

impl Correspondences {
    /// Read the table; names take their namespace from `rows`.
    ///
    /// # Errors
    ///
    /// Text that is not TOML, a row missing a field or with a field of the
    /// wrong kind, a prefix the namespace table lacks, or a code table a row
    /// names that is missing.
    pub fn parse(text: &str, rows: &[Row]) -> Result<Self> {
        let table: toml::Table = text
            .parse()
            .context("the table of Strict correspondences is not TOML")?;
        Ok(Self {
            attributes: read_rows(&table, "attribute", rows, |row| {
                Ok(AttributeRenames {
                    renames: row.pairs("renames")?,
                    type_name: row.name("type")?,
                })
            })?,
            character_sets: read_rows(&table, "character_set", rows, |row| {
                Ok(CharacterSet {
                    attribute: row.text("attribute")?,
                    codes: codes(&table, &row.text("codes")?, &row.text("written")?)?,
                    to: row.text("to")?,
                    type_name: row.name("type")?,
                })
            })?,
            child_attributes: read_rows(&table, "child_attribute", rows, |row| {
                Ok(ChildAttribute {
                    attribute: row.text("attribute")?,
                    child: row.text("child")?,
                    type_name: row.name("type")?,
                })
            })?,
            children: read_rows(&table, "child", rows, |row| {
                Ok(ChildRenames {
                    renames: row.pairs("renames")?,
                    type_name: row.name("type")?,
                })
            })?,
            iso_dates: read_rows(&table, "iso_date", rows, |row| {
                Ok(IsoDate {
                    attribute: row.text("attribute")?,
                    to: row.text("to")?,
                    type_name: row.name("type")?,
                })
            })?,
            measures: read_rows(&table, "measure", rows, |row| {
                Ok(Measure {
                    attribute: row.text("attribute")?,
                    per_percent: row.number("per_percent")?,
                    per_point: row.number("per_point")?,
                    percent: row.text("percent")?,
                    type_name: row.name("type")?,
                    unit: row.text("unit")?,
                })
            })?,
            members: read_rows(&table, "member", rows, |row| {
                Ok(Member {
                    member: row.name("member")?,
                    spelling: row.spelling()?,
                    type_name: row.name("type")?,
                })
            })?,
            packed: read_rows(&table, "packed", rows, |row| {
                Ok(Packed {
                    attribute: row.text("attribute")?,
                    bits: row.bits()?,
                    type_name: row.name("type")?,
                })
            })?,
            values: read_rows(&table, "value", rows, |row| {
                Ok(Values {
                    type_name: row.name("type")?,
                    values: row.pairs("values")?,
                })
            })?,
        })
    }
}

/// One row of a list of rows.
struct TableRow<'row> {
    key: &'static str,
    row: &'row toml::Value,
    rows: &'row [Row],
}

impl TableRow<'_> {
    /// A packed attribute's bits: `characters`, else `bits` sorted by bit.
    fn bits(&self) -> Result<Bits> {
        if self.row.get("characters").is_some() {
            return Ok(Bits::Characters(self.texts("characters")?));
        }
        let mut bits: Vec<(u16, String)> = self
            .pairs_of("bits", |value| {
                let bit = value.as_integer()?;
                u16::try_from(bit).ok()
            })?
            .into_iter()
            .map(|(attribute, bit)| (bit, attribute))
            .collect();
        bits.sort();
        Ok(Bits::Hex(bits))
    }

    /// A `prefix:local` name, its namespace from the namespace table; `xsd`
    /// names XML Schema's built-ins.
    fn name(&self, field: &str) -> Result<Name> {
        let text = self.text(field)?;
        let (prefix, local) = text
            .split_once(':')
            .with_context(|| format!("`{text}` in a `{}` row has no prefix", self.key))?;
        if prefix == "xsd" {
            return Ok(Name::new(XSD_NAMESPACE, local));
        }
        let row = self
            .rows
            .iter()
            .find(|row| row.prefix == prefix)
            .with_context(|| format!("no namespace has the prefix `{prefix}` of `{text}`"))?;
        Ok(Name::new(&row.uri, local))
    }

    /// A whole number.
    fn number(&self, field: &str) -> Result<u32> {
        self.optional_number(field)?
            .with_context(|| format!("a `{}` row has no `{field}`", self.key))
    }

    /// A whole number, when the row has one.
    fn optional_number(&self, field: &str) -> Result<Option<u32>> {
        self.row
            .get(field)
            .map(|value| {
                value
                    .as_integer()
                    .and_then(|number| u32::try_from(number).ok())
                    .with_context(|| {
                        format!("`{field}` of a `{}` row is no whole number", self.key)
                    })
            })
            .transpose()
    }

    /// An inline table of text values, sorted by key.
    fn pairs(&self, field: &str) -> Result<Vec<(String, String)>> {
        self.pairs_of(field, |value| value.as_str().map(str::to_owned))
    }

    /// An inline table, each value read by `value`, sorted by key.
    fn pairs_of<T, F>(&self, field: &str, value: F) -> Result<Vec<(String, T)>>
    where
        F: Fn(&toml::Value) -> Option<T>,
    {
        self.row
            .get(field)
            .and_then(toml::Value::as_table)
            .with_context(|| format!("a `{}` row has no table `{field}`", self.key))?
            .iter()
            .map(|(key, found)| {
                let read = value(found).with_context(|| {
                    format!(
                        "`{key}` of `{field}` in a `{}` row has the wrong kind",
                        self.key
                    )
                })?;
                Ok((key.clone(), read))
            })
            .collect()
    }

    /// What a `member` row writes: its `values`, else a percentage.
    fn spelling(&self) -> Result<Spelling> {
        if self.row.get("values").is_some() {
            return Ok(Spelling::Map(self.pairs("values")?));
        }
        Ok(Spelling::Percent {
            decimals: self.optional_number("decimals")?,
            per_percent: self.number("per_percent")?,
        })
    }

    /// A text.
    fn text(&self, field: &str) -> Result<String> {
        Ok(self
            .row
            .get(field)
            .and_then(toml::Value::as_str)
            .with_context(|| format!("a `{}` row has no `{field}`", self.key))?
            .to_owned())
    }

    /// A list of texts.
    fn texts(&self, field: &str) -> Result<Vec<String>> {
        self.row
            .get(field)
            .and_then(toml::Value::as_array)
            .with_context(|| format!("a `{}` row has no list `{field}`", self.key))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .with_context(|| format!("`{field}` of a `{}` row holds no text", self.key))
            })
            .collect()
    }
}

/// Each row of the list `key`, read by `read`.
fn read_rows<T, F>(table: &toml::Table, key: &'static str, rows: &[Row], read: F) -> Result<Vec<T>>
where
    F: Fn(&TableRow<'_>) -> Result<T>,
{
    let Some(found) = table.get(key) else {
        return Ok(Vec::new());
    };
    found
        .as_array()
        .with_context(|| format!("`{key}` is not a list of rows"))?
        .iter()
        .map(|row| read(&TableRow { key, row, rows }))
        .collect()
}

/// The code table `key`, its codes `written` in `hexadecimal` (one byte
/// each) or in `decimal`.
fn codes(table: &toml::Table, key: &str, written: &str) -> Result<Codes> {
    let found = table
        .get(key)
        .and_then(toml::Value::as_table)
        .with_context(|| format!("no code table `{key}`"))?;
    if found.is_empty() {
        bail!("the code table `{key}` is empty");
    }
    let name = |code: &str, value: &toml::Value| -> Result<String> {
        Ok(value
            .as_str()
            .with_context(|| format!("the code {code} of `{key}` has no name"))?
            .to_owned())
    };
    match written {
        "hexadecimal" => {
            let mut rows = Vec::new();
            for (code, value) in found {
                let byte = u8::from_str_radix(code, 16)
                    .ok()
                    .filter(|_| code.len() == 2)
                    .with_context(|| format!("the code {code} of `{key}` is no byte"))?;
                rows.push((byte, name(code, value)?));
            }
            rows.sort();
            Ok(Codes::Byte(rows))
        }
        "decimal" => {
            let mut rows = Vec::new();
            for (code, value) in found {
                let number: u32 = code
                    .parse()
                    .with_context(|| format!("the code {code} of `{key}` is no number"))?;
                rows.push((number, name(code, value)?));
            }
            rows.sort();
            Ok(Codes::Number(rows))
        }
        other => {
            bail!("the codes of `{key}` are written in `{other}`, neither hexadecimal nor decimal")
        }
    }
}
