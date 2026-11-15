//! The sheets of a workbook in order, each with its part (ECMA-376 Part 1, sections 18.2.19,
//! 18.2.20, 12.3.23 and 12.3.24).

#[cfg(test)]
mod tests;

use crate::parts::{chartsheet, dialogsheet, international_macro_sheet, macro_sheet, worksheet};
use officekit_markup::spreadsheet::{SheetVisibilityTypesMember, Workbook};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};

/// What kind of sheet a part of a workbook is, from the type of the relationship that names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SheetKind {
    /// A chart sheet (section 12.3.2).
    Chartsheet,
    /// A dialog sheet (section 12.3.7).
    Dialogsheet,
    /// A macro sheet, international or not.
    Macrosheet,
    /// A part of another type.
    Other,
    /// A worksheet (section 12.3.24).
    Worksheet,
}

/// One sheet of a workbook's sheet list.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SheetEntry {
    /// What kind of sheet the part is.
    pub kind: SheetKind,
    /// The sheet's name, the attribute `name`; empty when the file states none.
    pub name: String,
    /// The sheet's part.
    pub part: PartName,
    /// The index of the sheet's element in the sheet list, from 0: the number `localSheetId` holds.
    pub position: u32,
    /// Whether the workbook shows the sheet (section 18.18.68); `Visible`, the default, when the
    /// attribute is absent or holds no value of `ST_SheetState`.
    pub state: SheetVisibilityTypesMember,
}

impl SheetKind {
    /// The kind of a part related by the relationship type `type_uri`, in either class's spelling.
    fn of(type_uri: &str) -> Self {
        let is = |relationship_type: RelationshipType| relationship_type.matches(type_uri);
        if is(worksheet::RELATIONSHIP_TYPE) {
            Self::Worksheet
        } else if is(chartsheet::RELATIONSHIP_TYPE) {
            Self::Chartsheet
        } else if is(dialogsheet::RELATIONSHIP_TYPE) {
            Self::Dialogsheet
        } else if is(macro_sheet::RELATIONSHIP_TYPE)
            || is(international_macro_sheet::RELATIONSHIP_TYPE)
        {
            Self::Macrosheet
        } else {
            Self::Other
        }
    }
}

/// The sheets `workbook`, the root of the part `main` of `package`, lists, as
/// [`Xlsx::sheets`](super::Xlsx::sheets) gives them.
///
/// # Errors
///
/// The errors of [`Package::related`].
pub(super) fn of(
    package: &Package,
    main: &PartName,
    workbook: &Workbook,
) -> Result<Vec<SheetEntry>, PackageError> {
    let related = package.related(Some(main))?;
    Ok((0_u32..)
        .zip(&workbook.sheets.sheet_information)
        .filter_map(|(position, sheet)| {
            let id = sheet.id.as_deref()?;
            let relationship = related.iter().find(|candidate| candidate.id == id)?;
            let Target::Part(part) = &relationship.target else {
                return None;
            };
            package.contains(part).then(|| SheetEntry {
                kind: SheetKind::of(&relationship.type_uri),
                name: sheet.name.clone().unwrap_or_default(),
                part: part.clone(),
                position,
                state: sheet
                    .state
                    .as_ref()
                    .and_then(|state| state.value().ok())
                    .unwrap_or(SheetVisibilityTypesMember::Visible),
            })
        })
        .collect())
}

/// The first part `source` relates to by `relationship_type` that `package` holds.
///
/// # Errors
///
/// The errors of [`Package::related_by_type`].
pub(super) fn first_related(
    package: &Package,
    source: &PartName,
    relationship_type: &RelationshipType,
) -> Result<Option<PartName>, PackageError> {
    Ok(package
        .related_by_type(Some(source), relationship_type)?
        .into_iter()
        .find_map(|related| {
            let Target::Part(part) = related.target else {
                return None;
            };
            package.contains(&part).then_some(part)
        }))
}
