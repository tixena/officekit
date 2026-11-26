//! What a cell of a worksheet holds: its stored value by its type, and the runs of a string
//! (ECMA-376 Part 1, sections 18.3.1.4, 18.18.11, 18.4.8 and 22.9.2.19).
//!
//! ```
//! use officekit::xlsx::value::{CellContent, SharedStrings, TextRun, content};
//! use officekit_markup::spreadsheet::{Cell, CellType, CellValue, SharedStringTable, StringItem, Text};
//!
//! let mut text = Text::default();
//! text.value = "Total_x0009_due".into();
//! let mut item = StringItem::default();
//! item.text = Some(Box::new(text));
//! let mut table = SharedStringTable::default();
//! table.string_item.push(item);
//! let strings = SharedStrings::new(Some(&table));
//!
//! let mut value = CellValue::default();
//! value.value = "0".into();
//! let mut cell = Cell::default();
//! cell.t = Some(CellType::from("s".to_owned()));
//! cell.value = Some(Box::new(value));
//! assert_eq!(
//!     content(&cell, &strings),
//!     CellContent::Text(vec![TextRun::new("Total\tdue", None)])
//! );
//! ```

#[cfg(test)]
mod tests;

use alloc::borrow::Cow;

use officekit_markup::spreadsheet::{
    Cell, CellTypeMember, RichTextRun, RunProperties, SharedStringTable, Text,
};
use officekit_xml::values::parse::boolean;

/// What a cell holds (sections 18.3.1.4 and 18.18.11).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CellContent<'cell> {
    /// No stored value, or one its type cannot read.
    Blank,
    /// A logical value (`t="b"`).
    Boolean(bool),
    /// A date and time in ISO 8601, as the file writes it (`t="d"`, section 18.17.4).
    Date(&'cell str),
    /// The text of an error, such as `#DIV/0!` (`t="e"`).
    Error(&'cell str),
    /// A number, as the file writes it with no white space at its ends (`t="n"`, or no type).
    Number(&'cell str),
    /// The runs of a shared string, of the cell's own string, or of a formula's string.
    Text(Vec<TextRun<'cell>>),
}

/// The string items of a workbook's shared string table (section 18.4.9), ready to answer for an
/// index.
#[derive(Clone, Debug, Default)]
pub struct SharedStrings {
    table: Option<SharedStringTable>,
}

impl SharedStrings {
    /// The string items of the table's root `table`, taken as it is.
    pub(super) const fn from_table(table: Option<SharedStringTable>) -> Self {
        Self { table }
    }

    /// The runs of the string item at `index` (section 18.4.8); `None` past the table.
    #[must_use = "this only returns the value"]
    pub fn get(&self, index: u32) -> Option<Vec<TextRun<'_>>> {
        let item = self
            .table
            .as_ref()?
            .string_item
            .get(usize::try_from(index).ok()?)?;
        Some(runs(item.text.as_deref(), &item.rich_text_run))
    }

    /// Whether the table has no string item.
    #[must_use = "this only returns the value"]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The number of string items.
    #[must_use = "this only returns the value"]
    pub fn len(&self) -> usize {
        self.table
            .as_ref()
            .map_or(0, |table| table.string_item.len())
    }

    /// The string items of `table`, the shared string table's root; none for a workbook with none.
    #[must_use = "this only builds the value"]
    pub fn new(table: Option<&SharedStringTable>) -> Self {
        Self {
            table: table.cloned(),
        }
    }
}

/// One run of a string: its text, and its own properties when it states any (section 18.4.4).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct TextRun<'cell> {
    /// The run's `rPr`; `None` for a string's plain text and for a formula's string.
    pub properties: Option<&'cell RunProperties>,
    /// The run's text, with its escaped characters decoded.
    pub text: Cow<'cell, str>,
}

impl<'cell> TextRun<'cell> {
    /// The run of `text` with `properties`.
    #[must_use = "this only builds the value"]
    pub fn new<T>(text: T, properties: Option<&'cell RunProperties>) -> Self
    where
        T: Into<Cow<'cell, str>>,
    {
        Self {
            properties,
            text: text.into(),
        }
    }
}

/// What `cell` holds, with a shared string taken from `strings` (sections 18.3.1.4 and 18.18.11).
/// A type that is absent or no member of `ST_CellType` is `n`; the formula is not read.
#[must_use = "this only returns the value"]
pub fn content<'cell>(cell: &'cell Cell, strings: &'cell SharedStrings) -> CellContent<'cell> {
    let kind = cell
        .t
        .as_ref()
        .and_then(|stated| stated.value().ok())
        .unwrap_or(CellTypeMember::N);
    if kind == CellTypeMember::InlineStr {
        return cell
            .rich_text_inline
            .as_deref()
            .map_or(CellContent::Blank, |inline| {
                CellContent::Text(runs(inline.text.as_deref(), &inline.rich_text_run))
            });
    }
    let Some(value) = cell.value.as_deref().map(|stored| &*stored.value) else {
        return CellContent::Blank;
    };
    match kind {
        CellTypeMember::B => boolean(value).map_or(CellContent::Blank, CellContent::Boolean),
        CellTypeMember::D => CellContent::Date(value),
        CellTypeMember::E => CellContent::Error(value),
        CellTypeMember::S => trimmed(value)
            .parse()
            .ok()
            .and_then(|index| strings.get(index))
            .map_or(CellContent::Blank, CellContent::Text),
        CellTypeMember::Str => CellContent::Text(run(value, None).into_iter().collect()),
        CellTypeMember::N | _ => CellContent::Number(trimmed(value)),
    }
}

/// `text` with each `_xHHHH_` replaced by the character it numbers (section 22.9.2.19); an escape
/// that numbers no character, such as half of a surrogate pair, stays. Borrows when none is replaced.
#[must_use = "this only returns the value"]
pub fn unescape(text: &str) -> Cow<'_, str> {
    let mut decoded = String::new();
    let mut copied = 0;
    let mut from = 0;
    while let Some(found) = text[from..].find("_x") {
        let at = from + found;
        let number = text.get(at..at + 7).and_then(escaped_number);
        match number.map(char::from_u32) {
            Some(Some(character)) => {
                decoded.push_str(&text[copied..at]);
                decoded.push(character);
                copied = at + 7;
                from = copied;
            }
            Some(None) => from = at + 7,
            None => from = at + 1,
        }
    }
    if copied == 0 {
        Cow::Borrowed(text)
    } else {
        decoded.push_str(&text[copied..]);
        Cow::Owned(decoded)
    }
}

/// The number the seven bytes `candidate` write when they are an `_xHHHH_`.
fn escaped_number(candidate: &str) -> Option<u32> {
    let digits = candidate.strip_prefix("_x")?.strip_suffix('_')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// The run of `text`, decoded, with `properties`; none for a text of no characters.
fn run<'cell>(
    text: &'cell str,
    properties: Option<&'cell RunProperties>,
) -> Option<TextRun<'cell>> {
    (!text.is_empty()).then(|| TextRun::new(unescape(text), properties))
}

/// The runs of a string item or of a cell's own string (section 18.4.8): its `t` with no
/// properties, then each `r` with its `rPr`. The phonetic runs and properties are not read.
fn runs<'cell>(text: Option<&'cell Text>, rich: &'cell [RichTextRun]) -> Vec<TextRun<'cell>> {
    text.and_then(|plain| run(&plain.value, None))
        .into_iter()
        .chain(
            rich.iter()
                .filter_map(|each| run(&each.text.value, each.properties.as_deref())),
        )
        .collect()
}

/// `text` with the XML white space at its ends taken off.
fn trimmed(text: &str) -> &str {
    text.trim_matches([' ', '\t', '\r', '\n'])
}
