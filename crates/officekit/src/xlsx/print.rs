//! A sheet's print area and its repeated rows and columns, from the workbook's defined names
//! (ECMA-376 Part 1, section 18.2.5).
//!
//! ```
//! use officekit::xlsx::print::{PrintTitles, print_areas, print_titles};
//! use officekit::xlsx::reference::Area;
//! use officekit_markup::spreadsheet::{DefinedName, DefinedNames, Workbook};
//!
//! let mut names = DefinedNames::default();
//! for (name, sheet, text) in [
//!     ("_xlnm.Print_Titles", "1", "Big!$A:$A,Big!$1:$1"),
//!     ("_xlnm.Print_Area", "2", "Fit!$A$1:$T$100"),
//! ] {
//!     let mut defined = DefinedName::default();
//!     defined.name = Some(name.into());
//!     defined.local_sheet_id = Some(sheet.into());
//!     defined.value = text.into();
//!     names.defined_name.push(defined);
//! }
//! let mut workbook = Workbook::default();
//! workbook.defined_names = Some(Box::new(names));
//! assert_eq!(print_titles(&workbook, 1), PrintTitles::new(Some((1, 1)), Some((1, 1))));
//! assert_eq!(print_areas(&workbook, 2), [Area::new(1, 1, 20, 100)]);
//! ```

#[cfg(test)]
mod tests;

use officekit_markup::spreadsheet::Workbook;

use crate::xlsx::reference::{Area, area};

const PRINT_AREA: &str = "_xlnm.Print_Area";
const PRINT_TITLES: &str = "_xlnm.Print_Titles";

/// The rows and the columns every page of a sheet repeats, each from its first to its last.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct PrintTitles {
    pub columns: Option<(u32, u32)>,
    pub rows: Option<(u32, u32)>,
}

impl PrintTitles {
    /// The titles that repeat `columns` and `rows`.
    #[must_use = "this only builds the value"]
    pub const fn new(columns: Option<(u32, u32)>, rows: Option<(u32, u32)>) -> Self {
        Self { columns, rows }
    }
}

/// The areas of the print area of the sheet at `position` in the workbook's sheet list, in the
/// name's order; empty when the workbook names none for it (section 18.2.5, `_xlnm.Print_Area`).
#[must_use = "this only returns the value"]
pub fn print_areas(workbook: &Workbook, position: u32) -> Vec<Area> {
    areas(workbook, PRINT_AREA, position).collect()
}

/// The rows and the columns the sheet at `position` repeats on every page: the first area of
/// whole rows and the first of whole columns (section 18.2.5, `_xlnm.Print_Titles`).
#[must_use = "this only returns the value"]
pub fn print_titles(workbook: &Workbook, position: u32) -> PrintTitles {
    let found: Vec<Area> = areas(workbook, PRINT_TITLES, position).collect();
    let columns = found
        .iter()
        .find(|one| one.whole_columns() && !one.whole_rows())
        .map(|one| (one.first_column, one.last_column));
    let rows = found
        .iter()
        .find(|one| one.whole_rows())
        .map(|one| (one.first_row, one.last_row));
    PrintTitles::new(columns, rows)
}

/// The areas of the built-in name `name` whose `localSheetId` is `position`, the name compared
/// in any case; parts that are no reference to an area are passed over.
fn areas<'workbook>(
    workbook: &'workbook Workbook,
    name: &str,
    position: u32,
) -> impl Iterator<Item = Area> + 'workbook {
    let text = workbook.defined_names.as_ref().and_then(|names| {
        names.defined_name.iter().find_map(|defined| {
            let named = defined
                .name
                .as_deref()
                .is_some_and(|found| found.eq_ignore_ascii_case(name));
            let sheet = defined
                .local_sheet_id
                .as_deref()
                .and_then(|id| id.parse::<u32>().ok());
            (named && sheet == Some(position)).then_some(&*defined.value)
        })
    });
    text.into_iter().flat_map(parts).filter_map(sheet_area)
}

/// `text` cut at each comma outside quotation marks.
fn parts(text: &str) -> impl Iterator<Item = &str> {
    let mut quoted = false;
    text.split(move |character| {
        if character == '\'' {
            quoted = !quoted;
        }
        character == ',' && !quoted
    })
}

/// The area of `part`, a sheet's name, bare or in quotation marks, then `!` and a reference;
/// `None` for a reference to two sheets or another workbook, or what is no reference.
fn sheet_area(part: &str) -> Option<Area> {
    let (sheet, reference) = part.rsplit_once('!')?;
    let one_sheet = sheet
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .map_or_else(
            || !sheet.is_empty() && !sheet.contains(['\'', '"', '!', ':', '(', ')', '[', ']', ' ']),
            |quoted| {
                !quoted.is_empty() && !quoted.replace("\'\'", "").contains(['\'', ':', '[', ']'])
            },
        );
    if one_sheet { area(reference) } else { None }
}
