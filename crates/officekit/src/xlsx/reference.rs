//! The row and the column of a cell reference, and the rows and columns of an area (ECMA-376
//! Part 1, sections 18.18.7 and 18.18.62, in the A1 style of section 18.17.2.3.1).
//!
//! ```
//! use officekit::xlsx::reference::{Area, CellReference, area, cell, column_name};
//!
//! assert_eq!(cell("AD58"), Some(CellReference::new(30, 58)));
//! assert_eq!(area("$C$3:$A$1"), Some(Area::new(1, 1, 3, 3)));
//! assert_eq!(column_name(30).as_deref(), Some("AD"));
//! ```

#[cfg(test)]
mod tests;

/// The last column a sheet has, `XFD`: 16,384.
pub const LAST_COLUMN: u32 = 0x4000;
/// The last row a sheet has: 1,048,576.
pub const LAST_ROW: u32 = 0x0010_0000;

/// A rectangle of cells, each edge counted from 1 and the last ones included.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Area {
    pub first_column: u32,
    pub first_row: u32,
    pub last_column: u32,
    pub last_row: u32,
}

impl Area {
    /// The area from `first_column` and `first_row` to `last_column` and `last_row`.
    #[must_use = "this only builds the value"]
    pub const fn new(first_column: u32, first_row: u32, last_column: u32, last_row: u32) -> Self {
        Self {
            first_column,
            first_row,
            last_column,
            last_row,
        }
    }

    /// Whether the area is whole columns: every row of the sheet.
    #[must_use = "this only returns the value"]
    pub const fn whole_columns(&self) -> bool {
        self.first_row == 1 && self.last_row == LAST_ROW
    }

    /// Whether the area is whole rows: every column of the sheet.
    #[must_use = "this only returns the value"]
    pub const fn whole_rows(&self) -> bool {
        self.first_column == 1 && self.last_column == LAST_COLUMN
    }
}

/// A cell: its column and its row, each from 1.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CellReference {
    pub column: u32,
    pub row: u32,
}

impl CellReference {
    /// The cell at `column` and `row`.
    #[must_use = "this only builds the value"]
    pub const fn new(column: u32, row: u32) -> Self {
        Self { column, row }
    }
}

/// The area `reference` names: a cell, two cells, whole rows or whole columns, each with or
/// without `$`, two edges put in order (section 18.17.2.3.1).
#[must_use = "this only returns the value"]
pub fn area(reference: &str) -> Option<Area> {
    let Some((first, last)) = reference.split_once(':') else {
        let one = cell(reference)?;
        return Some(Area::new(one.column, one.row, one.column, one.row));
    };
    if let Some((start, end)) = cell(first).zip(cell(last)) {
        return Some(Area::new(
            start.column.min(end.column),
            start.row.min(end.row),
            start.column.max(end.column),
            start.row.max(end.row),
        ));
    }
    if let Some((start, end)) = row(first).zip(row(last)) {
        return Some(Area::new(1, start.min(end), LAST_COLUMN, start.max(end)));
    }
    let (start, end) = (whole_column(first)?, whole_column(last)?);
    Some(Area::new(start.min(end), 1, start.max(end), LAST_ROW))
}

/// The cell `reference` names, such as `AD58` or `$B$12`: its letters in either case, then its
/// row, each after an optional `$` (sections 18.18.7 and 18.17.2.3.1).
#[must_use = "this only returns the value"]
pub fn cell(reference: &str) -> Option<CellReference> {
    let (column, rest) = column(reference)?;
    Some(CellReference::new(column, row(rest)?))
}

/// The letters of column `column`, from `A` for 1 to `XFD` for [`LAST_COLUMN`] (section
/// 18.17.2.3.1); `None` outside them.
#[must_use = "this only returns the value"]
pub fn column_name(column: u32) -> Option<String> {
    if !(1..=LAST_COLUMN).contains(&column) {
        return None;
    }
    let mut letters = Vec::new();
    let mut rest = column;
    while rest > 0 {
        letters.push(char::from(b'A' + u8::try_from((rest - 1) % 26).ok()?));
        rest = (rest - 1) / 26;
    }
    Some(letters.iter().rev().collect())
}

/// The column of the letters `text` starts with after an optional `$`, and the text after them.
fn column(text: &str) -> Option<(u32, &str)> {
    let unsigned = text.strip_prefix('$').unwrap_or(text);
    let end = unsigned
        .find(|character: char| !character.is_ascii_alphabetic())
        .unwrap_or(unsigned.len());
    let (letters, rest) = unsigned.split_at(end);
    if letters.is_empty() {
        return None;
    }
    let number = letters.bytes().try_fold(0_u32, |number, letter| {
        number
            .checked_mul(26)?
            .checked_add(u32::from(letter.to_ascii_uppercase() - b'A' + 1))
    })?;
    (number <= LAST_COLUMN).then_some((number, rest))
}

/// The row `text` is, digits after an optional `$`, from 1 to [`LAST_ROW`].
fn row(text: &str) -> Option<u32> {
    let digits = text.strip_prefix('$').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let number: u32 = digits.parse().ok()?;
    (1..=LAST_ROW).contains(&number).then_some(number)
}

/// The column `text` is, letters after an optional `$` and nothing else.
fn whole_column(text: &str) -> Option<u32> {
    let (number, rest) = column(text)?;
    rest.is_empty().then_some(number)
}
