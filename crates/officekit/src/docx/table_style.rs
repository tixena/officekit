//! A table style applied to one table: the effective properties of the table, a row, a cell and
//! what is inside a cell (ECMA-376 Part 1, sections 17.7.6 and 17.4.55).

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

use officekit_markup::Read as _;
use officekit_markup::Shape;
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    ConditionalTableStyleFormattingTypesMember, DecimalNumberValue, ParagraphPropertiesPPrGeneral,
    PreviousTableProperties, RunProperties, StyleConditionalTableFormattingProperties,
    StyleDefinition, TABLE_CELL_PROPERTIES_SHAPE, TABLE_PROPERTIES_SHAPE,
    TABLE_ROW_PROPERTIES_SHAPE, TableCellProperties, TableProperties, TableRowProperties, write,
};
use officekit_xml::element::{Element, Node};
use officekit_xml::write::Scope;

use crate::docx::effective::{apply, empty, is_word};

/// The bits of `w:tblLook w:val` (ECMA-376 Part 4, section 14.4.12).
const FIRST_ROW: u16 = 0x0020;
const LAST_ROW: u16 = 0x0040;
const FIRST_COLUMN: u16 = 0x0080;
const LAST_COLUMN: u16 = 0x0100;
const NO_ROW_BANDS: u16 = 0x0200;
const NO_COLUMN_BANDS: u16 = 0x0400;

/// Where a cell stands in its table, for the conditional formats of section 17.7.6.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CellPosition {
    /// The cell's column, from 0, counted in cells of its row.
    pub column: usize,
    /// How many cells the row has.
    pub columns: usize,
    /// Whether the row holds `w:tblHeader`.
    pub header: bool,
    /// The cell's row, from 0.
    pub row: usize,
    /// How many rows the table has.
    pub rows: usize,
}

impl CellPosition {
    /// The cell at `row` of `rows` and `column` of `columns`; `header` when its row holds
    /// `w:tblHeader` (section 17.4.49).
    #[must_use = "this only builds the value"]
    pub const fn new(row: usize, rows: usize, column: usize, columns: usize, header: bool) -> Self {
        Self {
            column,
            columns,
            header,
            row,
            rows,
        }
    }
}

/// Which properties of a table style a layer holds.
#[derive(Clone, Copy)]
pub(super) enum Kind {
    /// `w:tcPr`.
    Cell,
    /// `w:pPr`.
    Paragraph,
    /// `w:trPr`.
    Row,
    /// `w:rPr`.
    Run,
    /// `w:tblPr`.
    Table,
}

/// The properties a table style, or one of its conditional formats, holds.
struct Sets<'style> {
    cell: Option<&'style TableCellProperties>,
    paragraph: Option<&'style ParagraphPropertiesPPrGeneral>,
    row: Option<&'style TableRowProperties>,
    run: Option<&'style RunProperties>,
    table: Option<&'style PreviousTableProperties>,
}

impl<'style> From<&'style StyleDefinition> for Sets<'style> {
    fn from(style: &'style StyleDefinition) -> Self {
        Self {
            cell: style.table_cell_properties.as_deref(),
            paragraph: style.paragraph_properties_p_pr_general.as_deref(),
            row: style.table_row_properties.as_deref(),
            run: style.run_properties.as_deref(),
            table: style.previous_table_properties.as_deref(),
        }
    }
}

impl<'style> From<&'style StyleConditionalTableFormattingProperties> for Sets<'style> {
    fn from(format: &'style StyleConditionalTableFormattingProperties) -> Self {
        Self {
            cell: format.table_cell_properties.as_deref(),
            paragraph: format.paragraph_properties_p_pr_general.as_deref(),
            row: format.table_row_properties.as_deref(),
            run: format.run_properties.as_deref(),
            table: format.previous_table_properties.as_deref(),
        }
    }
}

impl Sets<'_> {
    /// The element of `kind`, when it is stated.
    fn element(&self, kind: Kind) -> Option<Element> {
        let mut scope = Scope::default();
        match kind {
            Kind::Cell => self
                .cell
                .map(|found| write::table_cell_properties(found, &mut scope)),
            Kind::Paragraph => self
                .paragraph
                .map(|found| write::paragraph_properties_p_pr_general(found, &mut scope)),
            Kind::Row => self
                .row
                .map(|found| write::table_row_properties(found, &mut scope)),
            Kind::Run => self
                .run
                .map(|found| write::run_properties(found, &mut scope)),
            Kind::Table => self
                .table
                .map(|found| write::previous_table_properties(found, &mut scope)),
        }
    }
}

/// A table's style as that table uses it: the style's chain and the conditional formats the
/// table's `w:tblLook` turns on, with the table's own properties (ECMA-376 Part 1, sections 17.7.6
/// and 17.4.55).
#[derive(Clone, Debug)]
pub struct TableFormat {
    /// The style, then the style it is based on, and so on.
    chain: Vec<StyleDefinition>,
    /// How many columns a column band holds, at least 1.
    column_band: usize,
    /// What `w:tblLook` turns on, as the bits of its `w:val`.
    look: u16,
    /// The table's own `w:tblPr`.
    own: TableProperties,
    /// How many rows a row band holds, at least 1.
    row_band: usize,
}

impl TableFormat {
    /// The effective properties of a cell at `position` whose own properties are `own`: the
    /// table style's `w:tcPr` and those of the conditional formats that apply there, then `own`
    /// (sections 17.7.6.8 and 17.7.6.9).
    #[must_use = "this only returns the value"]
    pub fn cell(
        &self,
        position: CellPosition,
        own: Option<&TableCellProperties>,
    ) -> TableCellProperties {
        let mut layers: Vec<Element> = self
            .layers(&self.formats(position), Kind::Cell)
            .into_iter()
            .flatten()
            .collect();
        layers.extend(own.map(|found| write::table_cell_properties(found, &mut Scope::default())));
        TableCellProperties::read(&laid("tcPr", &TABLE_CELL_PROPERTIES_SHAPE, &layers))
    }

    /// Whether a style of the chain holds a conditional format of type `format`.
    fn defines(&self, format: ConditionalTableStyleFormattingTypesMember) -> bool {
        self.chain.iter().any(|style| {
            style
                .style_conditional_table_formatting_properties
                .iter()
                .any(|conditional| type_of(conditional) == Some(format))
        })
    }

    /// The conditional formats of the table's style that apply at `position`, in the order
    /// section 17.7.6 applies them; `wholeTable` is always the first.
    #[must_use = "this only returns the value"]
    pub fn formats(
        &self,
        position: CellPosition,
    ) -> Vec<ConditionalTableStyleFormattingTypesMember> {
        use ConditionalTableStyleFormattingTypesMember::{
            Band1Horz, Band1Vert, Band2Horz, Band2Vert, FirstCol, FirstRow, LastCol, LastRow,
            NeCell, NwCell, SeCell, SwCell, WholeTable,
        };
        let on = |bit: u16| self.look & bit != 0;
        let top = position.row == 0;
        let bottom = position.rows.checked_sub(1) == Some(position.row);
        let left = position.column == 0;
        let right = position.columns.checked_sub(1) == Some(position.column);
        let column_band = if (position.column / self.column_band).is_multiple_of(2) {
            Band1Vert
        } else {
            Band2Vert
        };
        let row_band = if (position.row / self.row_band).is_multiple_of(2) {
            Band1Horz
        } else {
            Band2Horz
        };
        let candidates = [
            (column_band, !on(NO_COLUMN_BANDS)),
            (row_band, !on(NO_ROW_BANDS)),
            (FirstRow, on(FIRST_ROW) && (top || position.header)),
            (LastRow, on(LAST_ROW) && bottom),
            (FirstCol, on(FIRST_COLUMN) && left),
            (LastCol, on(LAST_COLUMN) && right),
            (NwCell, on(FIRST_ROW) && on(FIRST_COLUMN) && top && left),
            (NeCell, on(FIRST_ROW) && on(LAST_COLUMN) && top && right),
            (SwCell, on(LAST_ROW) && on(FIRST_COLUMN) && bottom && left),
            (SeCell, on(LAST_ROW) && on(LAST_COLUMN) && bottom && right),
        ];
        let mut found = alloc::vec![WholeTable];
        found.extend(
            candidates
                .into_iter()
                .filter(|(format, applies)| *applies && self.defines(*format))
                .map(|(format, _)| format),
        );
        found
    }

    /// The elements of `kind` that `formats` lay down, one list for each format in order, each
    /// from the chain's last ancestor down; for `wholeTable` a style's own properties come before
    /// those of its `wholeTable` format (sections 17.7.2 and 17.7.6).
    pub(super) fn layers(
        &self,
        formats: &[ConditionalTableStyleFormattingTypesMember],
        kind: Kind,
    ) -> Vec<Vec<Element>> {
        formats
            .iter()
            .map(|format| {
                let whole = matches!(
                    format,
                    ConditionalTableStyleFormattingTypesMember::WholeTable
                );
                self.chain
                    .iter()
                    .rev()
                    .flat_map(|style| {
                        let own = whole.then(|| Sets::from(style).element(kind)).flatten();
                        let conditional = style
                            .style_conditional_table_formatting_properties
                            .iter()
                            .filter(|found| type_of(found) == Some(*format))
                            .filter_map(|found| Sets::from(found).element(kind));
                        own.into_iter().chain(conditional)
                    })
                    .collect()
            })
            .collect()
    }

    /// The format of a table whose own properties are `own` and whose style's chain is `chain`.
    pub(super) fn new(chain: Vec<StyleDefinition>, own: &TableProperties) -> Self {
        let mut format = Self {
            chain,
            column_band: 1,
            look: look_of(own),
            own: own.clone(),
            row_band: 1,
        };
        let properties = format.properties();
        format.column_band = band_size(
            properties
                .number_of_columns_in_column_band
                .as_deref()
                .and_then(|band| band.val.as_ref()),
        );
        format.row_band = band_size(
            properties
                .number_of_rows_in_row_band
                .as_deref()
                .and_then(|band| band.val.as_ref()),
        );
        format
    }

    /// The effective properties of the table: the style chain's `w:tblPr`, from the last
    /// ancestor down, then the table's own (sections 17.7.6.4 and 17.4.59).
    #[must_use = "this only returns the value"]
    pub fn properties(&self) -> TableProperties {
        use ConditionalTableStyleFormattingTypesMember::WholeTable;
        let mut layers: Vec<Element> = self
            .layers(&[WholeTable], Kind::Table)
            .into_iter()
            .flatten()
            .collect();
        let mut own = write::table_properties(&self.own, &mut Scope::default());
        own.children
            .retain(|node| !matches!(node, Node::Element(child) if is_word(child, "tblPrChange")));
        layers.push(own);
        TableProperties::read(&laid("tblPr", &TABLE_PROPERTIES_SHAPE, &layers))
    }

    /// The effective properties of the row of `position` whose own properties are `own`: the
    /// table style's `w:trPr` and those of the row's conditional formats, then `own`
    /// (sections 17.7.6.10 and 17.7.6.11).
    #[must_use = "this only returns the value"]
    pub fn row(
        &self,
        position: CellPosition,
        own: Option<&TableRowProperties>,
    ) -> TableRowProperties {
        use ConditionalTableStyleFormattingTypesMember::{
            Band1Horz, Band2Horz, FirstRow, LastRow, WholeTable,
        };
        let formats: Vec<ConditionalTableStyleFormattingTypesMember> = self
            .formats(position)
            .into_iter()
            .filter(|format| {
                matches!(
                    format,
                    WholeTable | Band1Horz | Band2Horz | FirstRow | LastRow
                )
            })
            .collect();
        let mut layers: Vec<Element> = self
            .layers(&formats, Kind::Row)
            .into_iter()
            .flatten()
            .collect();
        layers.extend(own.map(|found| write::table_row_properties(found, &mut Scope::default())));
        TableRowProperties::read(&laid("trPr", &TABLE_ROW_PROPERTIES_SHAPE, &layers))
    }
}

/// A band's size: `value`, or 1 when it is absent or below 1 (sections 17.7.6.5 and 17.7.6.7).
fn band_size(value: Option<&DecimalNumberValue>) -> usize {
    value
        .and_then(|found| found.value().ok())
        .and_then(|found| usize::try_from(found).ok())
        .filter(|found| *found >= 1)
        .unwrap_or(1)
}

/// An empty element `local` of `shape` with `layers` laid over it in order.
fn laid(local: &str, shape: &Shape, layers: &[Element]) -> Element {
    let mut merged = empty(local);
    for layer in layers {
        apply(&mut merged, layer, Some(shape));
    }
    merged
}

/// What the table's `w:tblLook` turns on, as the bits of `w:val` (ECMA-376 Part 1, section
/// 17.4.55, and Part 4, section 14.4.12): an attribute states its own setting, `w:val` the ones
/// whose attribute is absent; no `w:tblLook` turns on the bands only.
fn look_of(own: &TableProperties) -> u16 {
    let Some(look) = own
        .table_style_conditional_formatting_settings_exception
        .as_deref()
    else {
        return 0;
    };
    let mut bits = look
        .val
        .as_ref()
        .and_then(|value| value.value().ok())
        .unwrap_or(0);
    let stated = [
        (look.first_row.as_ref(), FIRST_ROW),
        (look.last_row.as_ref(), LAST_ROW),
        (look.first_column.as_ref(), FIRST_COLUMN),
        (look.last_column.as_ref(), LAST_COLUMN),
        (look.no_h_band.as_ref(), NO_ROW_BANDS),
        (look.no_v_band.as_ref(), NO_COLUMN_BANDS),
    ];
    for (attribute, bit) in stated {
        match attribute.and_then(|value: &OnOffValue| value.value().ok()) {
            Some(true) => bits |= bit,
            Some(false) => bits &= !bit,
            None => {}
        }
    }
    bits
}

/// The type of a conditional format, when it has a valid one.
fn type_of(
    format: &StyleConditionalTableFormattingProperties,
) -> Option<ConditionalTableStyleFormattingTypesMember> {
    format.r#type.as_ref()?.value().ok()
}
