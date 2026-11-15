//! A table style applied to one table: the effective properties of the table, a row, a cell and
//! what is inside a cell (ECMA-376 Part 1, sections 17.7.6 and 17.4.55).

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::Range;

use officekit_markup::Read as _;
use officekit_markup::Shape;
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    ConditionalTableStyleFormattingTypesMember, DecimalNumberValue, MergedCellType,
    MergedCellTypeMember, ParagraphPropertiesPPrGeneral, PreviousTableProperties, RunProperties,
    StyleConditionalTableFormattingProperties, StyleDefinition, TABLE_CELL_PROPERTIES_SHAPE,
    TABLE_PROPERTIES_SHAPE, TABLE_ROW_PROPERTIES_SHAPE, Table, TableCellProperties,
    TableLevelPropertyExceptions, TableProperties, TableRow, TableRowProperties,
    TableRowPropertiesContent, TableStyleConditionalFormattingSettingsException, write,
};
use officekit_xml::element::{Element, Node};
use officekit_xml::write::Scope;

use crate::docx::effective::{apply, empty, is_word};
use crate::docx::table::{every_cell, every_row};

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

/// A cell of the row above, as [`TableFormat::vertical_merges`] read it.
struct Placed {
    column: usize,
    /// The grid columns it covers; `None` when the standard does not place it.
    columns: Option<Range<usize>>,
    merge: VerticalMerge,
    row: usize,
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

/// A cell's place in a horizontal merge (ECMA-376 Part 4, section 14.4.13, and Part 1, section
/// 17.18.57). It is read apart from [`VerticalMerge`]: the standard gives no rule for combining them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum HorizontalMerge {
    /// A `w:hMerge` of value `continue`, or of none, after a cell of a merge in the same row.
    Continues {
        /// The column of the cell that starts the merge, in cells of the row.
        column: usize,
    },
    /// No `w:hMerge`, which closes any merge before it; or a `continue` after a cell of no merge,
    /// or first in its row, which "shall be ignored" (section 17.18.57).
    NotMerged,
    /// `w:hMerge w:val="restart"`: the cell starts a new merged group of cells.
    Starts,
    /// A `w:val` that is no `ST_Merge` value, or a `continue` after such a cell.
    Unsettled,
}

/// A cell's place in a vertical merge (ECMA-376 Part 1, sections 17.4.84 and 17.18.57).
///
/// Of a continuing cell's content the standard says nothing: such cells "shall be merged into
/// this merged cell group", and the content stays in the cell as the file holds it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum VerticalMerge {
    /// A `w:vMerge` of value `continue`, or of none ("its value shall be assumed to be
    /// continue", section 17.4.84), under a cell of a merge over the same grid columns.
    Continues {
        /// The column of the cell that starts the merge, in cells of its row.
        column: usize,
        /// The row of the cell that starts the merge.
        row: usize,
    },
    /// No `w:vMerge`: the cell "shall not be part of any vertically merged grouping of cells"; or
    /// a `continue` under no cell of a merge, which "shall be ignored" (sections 17.4.84, 17.18.57).
    NotMerged,
    /// `w:vMerge w:val="restart"`: the cell "starts (or restarts) a group of merged cells in the
    /// parent table" (section 17.18.57).
    Starts,
    /// A `continue` under a merge over other grid columns, which section 17.4.84 calls
    /// "non-conformant", or one whose `w:val`, `w:gridSpan` or `w:gridBefore` the standard gives
    /// no meaning.
    Unsettled,
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
    /// The `w:tblPrEx` of the row this format is for, from [`TableFormat::for_row`].
    exceptions: Option<TableLevelPropertyExceptions>,
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

    /// This format as the row whose `w:tblPrEx` is `exceptions` sees it (ECMA-376 Part 1, section
    /// 17.4.60): its properties, and the look its conditional formats take; `None` gives the table's.
    #[must_use = "this only returns the value"]
    pub fn for_row(&self, exceptions: Option<&TableLevelPropertyExceptions>) -> Self {
        let own_look = self
            .own
            .table_style_conditional_formatting_settings_exception
            .as_deref();
        let look = exceptions
            .and_then(|found| {
                found
                    .table_style_conditional_formatting_settings_exception
                    .as_deref()
            })
            .or(own_look);
        Self {
            exceptions: exceptions.cloned(),
            look: look_of(look),
            ..self.clone()
        }
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

    /// The place in a horizontal merge of cell `column` of row `row` of `table`, the table of
    /// this format, counted as [`TableFormat::horizontal_merges`] counts; `None` when there is none.
    #[must_use = "this only returns the value"]
    pub fn horizontal_merge(
        &self,
        table: &Table,
        row: usize,
        column: usize,
    ) -> Option<HorizontalMerge> {
        let rows = every_row(table);
        self.horizontal_merges_of(rows.get(row)?, row, rows.len())
            .get(column)
            .copied()
    }

    /// Each cell's place in a horizontal merge, by row of [`every_row`] and cell of
    /// [`every_cell`], from the cells' effective `w:hMerge` (Part 4, section 14.4.13).
    #[must_use = "this only returns the value"]
    pub fn horizontal_merges(&self, table: &Table) -> Vec<Vec<HorizontalMerge>> {
        let rows = every_row(table);
        rows.iter()
            .enumerate()
            .map(|(index, row)| self.horizontal_merges_of(row, index, rows.len()))
            .collect()
    }

    /// Each cell's place in a horizontal merge in `row`, row `index` of a table of `rows` rows.
    fn horizontal_merges_of(
        &self,
        row: &TableRow,
        index: usize,
        rows: usize,
    ) -> Vec<HorizontalMerge> {
        let cells = every_cell(row);
        let format = self.for_row(row.table_level_property_exceptions.as_deref());
        let header = holds_header(row);
        let mut merges: Vec<HorizontalMerge> = Vec::with_capacity(cells.len());
        for (column, cell) in cells.iter().enumerate() {
            let position = CellPosition::new(index, rows, column, cells.len(), header);
            let properties = format.cell(position, cell.properties.as_deref());
            let previous = column.checked_sub(1).zip(merges.last().copied());
            merges.push(horizontal_of(&properties, previous));
        }
        merges
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

    /// Each cell's place in a vertical merge, for the first `count` rows of `table`.
    fn merges_through(&self, table: &Table, count: usize) -> Vec<Vec<VerticalMerge>> {
        let rows = every_row(table);
        let grid = table.grid.column_definition.len();
        let mut merges = Vec::new();
        let mut above: Vec<Placed> = Vec::new();
        for (row_index, row) in rows.iter().take(count).enumerate() {
            let cells = every_cell(row);
            let format = self.for_row(row.table_level_property_exceptions.as_deref());
            let header = holds_header(row);
            let at = |column| CellPosition::new(row_index, rows.len(), column, cells.len(), header);
            let mut next = grid_before(&format.row(at(0), row.properties.as_deref()), grid);
            let mut placed = Vec::with_capacity(cells.len());
            for (column, cell) in cells.iter().enumerate() {
                let properties = format.cell(at(column), cell.properties.as_deref());
                let columns = next
                    .zip(grid_span(&properties))
                    .and_then(|(start, span)| Some(start..start.checked_add(span)?));
                next = columns.as_ref().map(|found| found.end);
                let merge = merge_of(&properties, columns.as_ref(), &above);
                placed.push(Placed {
                    column,
                    columns,
                    merge,
                    row: row_index,
                });
            }
            merges.push(placed.iter().map(|cell| cell.merge).collect());
            above = placed;
        }
        merges
    }

    /// The format of a table whose own properties are `own` and whose style's chain is `chain`.
    pub(super) fn new(chain: Vec<StyleDefinition>, own: &TableProperties) -> Self {
        let mut format = Self {
            chain,
            column_band: 1,
            exceptions: None,
            look: look_of(
                own.table_style_conditional_formatting_settings_exception
                    .as_deref(),
            ),
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
    /// ancestor down, then the table's own (sections 17.7.6.4 and 17.4.59), then those of the
    /// row's `w:tblPrEx` for a format from [`TableFormat::for_row`].
    #[must_use = "this only returns the value"]
    pub fn properties(&self) -> TableProperties {
        self.properties_of(&[ConditionalTableStyleFormattingTypesMember::WholeTable])
    }

    /// The table properties in force at `position` (sections 17.7.6 and 17.7.6.3): the style
    /// chain's `w:tblPr`, then those of the conditional formats that apply there, then the
    /// table's own, then the row's `w:tblPrEx` for a format from [`TableFormat::for_row`].
    #[must_use = "this only returns the value"]
    pub fn properties_at(&self, position: CellPosition) -> TableProperties {
        self.properties_of(&self.formats(position))
    }

    /// The table properties in effect for the row whose `w:tblPrEx` is `exceptions`: those of
    /// [`TableFormat::properties`] with each one the exceptions state in its place (section
    /// 17.4.60); `None` gives [`TableFormat::properties`].
    #[must_use = "this only returns the value"]
    pub fn properties_for_row(
        &self,
        exceptions: Option<&TableLevelPropertyExceptions>,
    ) -> TableProperties {
        self.for_row(exceptions).properties()
    }

    /// The `w:tblPr` that `formats` lay down, then the table's own, then the row's exceptions.
    fn properties_of(
        &self,
        formats: &[ConditionalTableStyleFormattingTypesMember],
    ) -> TableProperties {
        let mut layers: Vec<Element> = self
            .layers(formats, Kind::Table)
            .into_iter()
            .flatten()
            .collect();
        let mut own = write::table_properties(&self.own, &mut Scope::default());
        own.children
            .retain(|node| !matches!(node, Node::Element(child) if is_word(child, "tblPrChange")));
        layers.push(own);
        let table = TableProperties::read(&laid("tblPr", &TABLE_PROPERTIES_SHAPE, &layers));
        match &self.exceptions {
            Some(exceptions) => excepted(table, exceptions),
            None => table,
        }
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

    /// The place in a vertical merge of cell `column` of row `row` of `table`, the table of this
    /// format, counted as [`TableFormat::vertical_merges`] counts; `None` when there is no such cell.
    #[must_use = "this only returns the value"]
    pub fn vertical_merge(
        &self,
        table: &Table,
        row: usize,
        column: usize,
    ) -> Option<VerticalMerge> {
        self.merges_through(table, row.saturating_add(1))
            .get(row)?
            .get(column)
            .copied()
    }

    /// Each cell's place in a vertical merge, by row of [`every_row`] and cell of [`every_cell`],
    /// from the cells' effective `w:vMerge`, `w:gridSpan` and `w:gridBefore` (sections 17.4.84,
    /// 17.4.17, 17.4.15 and 17.7.6.9).
    #[must_use = "this only returns the value"]
    pub fn vertical_merges(&self, table: &Table) -> Vec<Vec<VerticalMerge>> {
        self.merges_through(table, usize::MAX)
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

/// A `continue` over the grid columns `own` under the cells `above`: it joins the merge of the one
/// cell over the same columns, is ignored under cells of no merge (section 17.18.57), and is
/// unsettled under a merge over other columns, which section 17.4.84 calls non-conformant.
fn continued(own: &Range<usize>, above: &[Placed]) -> VerticalMerge {
    if above.iter().any(|cell| cell.columns.is_none()) {
        return VerticalMerge::Unsettled;
    }
    let over: Vec<&Placed> = above
        .iter()
        .filter(|cell| {
            cell.columns
                .as_ref()
                .is_some_and(|theirs| theirs.start < own.end && own.start < theirs.end)
        })
        .collect();
    if let [cell] = over.as_slice()
        && cell.columns.as_ref() == Some(own)
    {
        return if cell.merge == VerticalMerge::Starts {
            VerticalMerge::Continues {
                column: cell.column,
                row: cell.row,
            }
        } else {
            cell.merge
        };
    }
    if over
        .iter()
        .all(|cell| cell.merge == VerticalMerge::NotMerged)
    {
        VerticalMerge::NotMerged
    } else {
        VerticalMerge::Unsettled
    }
}

/// `table` with each property `exceptions` states in its place: they "shall be applied to the
/// contents of this row in place of the table properties specified in the tblPr element" (section
/// 17.4.60). A border or a margin is a property of its own, so a side the exceptions omit stays.
fn excepted(
    mut table: TableProperties,
    exceptions: &TableLevelPropertyExceptions,
) -> TableProperties {
    replace(
        &mut table.paragraph_shading,
        exceptions.paragraph_shading.as_ref(),
    );
    replace(
        &mut table.preferred_table_width,
        exceptions.preferred_table_width.as_ref(),
    );
    replace(
        &mut table.table_alignment_exception,
        exceptions.table_alignment_exception.as_ref(),
    );
    replace(
        &mut table.table_indent_from_leading_margin,
        exceptions.table_indent_from_leading_margin.as_ref(),
    );
    replace(&mut table.table_layout, exceptions.table_layout.as_ref());
    replace(
        &mut table.table_row_cell_spacing,
        exceptions.table_row_cell_spacing.as_ref(),
    );
    replace(
        &mut table.table_style_conditional_formatting_settings_exception,
        exceptions
            .table_style_conditional_formatting_settings_exception
            .as_ref(),
    );
    if let Some(over) = exceptions.table_borders.as_deref() {
        let held = table.table_borders.get_or_insert_with(Box::default);
        replace(
            &mut held.left_paragraph_border,
            over.left_paragraph_border.as_ref(),
        );
        replace(
            &mut held.paragraph_border_above_identical_paragraphs,
            over.paragraph_border_above_identical_paragraphs.as_ref(),
        );
        replace(
            &mut held.paragraph_border_below_identical_paragraphs,
            over.paragraph_border_below_identical_paragraphs.as_ref(),
        );
        replace(
            &mut held.right_paragraph_border,
            over.right_paragraph_border.as_ref(),
        );
        replace(
            &mut held.table_cell_leading_edge_border,
            over.table_cell_leading_edge_border.as_ref(),
        );
        replace(
            &mut held.table_cell_trailing_margin_exception,
            over.table_cell_trailing_margin_exception.as_ref(),
        );
        replace(
            &mut held.table_inside_horizontal_edges_border,
            over.table_inside_horizontal_edges_border.as_ref(),
        );
        replace(
            &mut held.table_inside_vertical_edges_border,
            over.table_inside_vertical_edges_border.as_ref(),
        );
    }
    if let Some(over) = exceptions.table_cell_margin_exceptions.as_deref() {
        let held = table
            .table_cell_margin_exceptions
            .get_or_insert_with(Box::default);
        replace(
            &mut held.left_paragraph_border_tbl_width,
            over.left_paragraph_border_tbl_width.as_ref(),
        );
        replace(
            &mut held.paragraph_border_above_identical_paragraphs_tbl_width,
            over.paragraph_border_above_identical_paragraphs_tbl_width
                .as_ref(),
        );
        replace(
            &mut held.paragraph_border_below_identical_paragraphs_tbl_width,
            over.paragraph_border_below_identical_paragraphs_tbl_width
                .as_ref(),
        );
        replace(
            &mut held.right_paragraph_border_tbl_width,
            over.right_paragraph_border_tbl_width.as_ref(),
        );
        replace(
            &mut held.table_cell_leading_edge_border_tbl_width,
            over.table_cell_leading_edge_border_tbl_width.as_ref(),
        );
        replace(
            &mut held.table_cell_trailing_margin_exception_tbl_width,
            over.table_cell_trailing_margin_exception_tbl_width.as_ref(),
        );
    }
    table
}

/// The grid columns skipped before a row's first cell (section 17.4.15): 0 when `w:gridBefore` is
/// absent or larger than the `grid` columns; `None` when its value is no count.
fn grid_before(row: &TableRowProperties, grid: usize) -> Option<usize> {
    let Some(before) = row.content.iter().find_map(|item| {
        if let TableRowPropertiesContent::GridColumnsBeforeFirstCell(found) = item {
            Some(found)
        } else {
            None
        }
    }) else {
        return Some(0);
    };
    let count = usize::try_from(before.val.as_ref()?.value().ok()?).ok()?;
    Some(if count > grid { 0 } else { count })
}

/// The grid columns a cell spans (section 17.4.17): 1 when `w:gridSpan` is absent; `None` when its
/// value is below 1 or no number, which the standard gives no meaning.
fn grid_span(cell: &TableCellProperties) -> Option<usize> {
    let Some(span) = cell.grid_columns_spanned.as_deref() else {
        return Some(1);
    };
    usize::try_from(span.val.as_ref()?.value().ok()?)
        .ok()
        .filter(|count| *count >= 1)
}

/// Whether `row` holds `w:tblHeader` turned on (section 17.4.49).
fn holds_header(row: &TableRow) -> bool {
    row.properties.as_deref().is_some_and(|properties| {
        properties.content.iter().any(|item| {
            matches!(item, TableRowPropertiesContent::RepeatTableRow(found)
                if found.val.as_ref().is_none_or(|on| !matches!(on.value(), Ok(false))))
        })
    })
}

/// The place in a horizontal merge of a cell whose effective properties are `cell`, after the
/// cell of the same row at `previous`, its column and place (Part 4, section 14.4.13; 17.18.57).
fn horizontal_of(
    cell: &TableCellProperties,
    previous: Option<(usize, HorizontalMerge)>,
) -> HorizontalMerge {
    let Some(merge) = cell.horizontally_merged_cell.as_deref() else {
        return HorizontalMerge::NotMerged;
    };
    let value = merge.val.as_ref().map(MergedCellType::value);
    if matches!(value, Some(Ok(MergedCellTypeMember::Restart))) {
        return HorizontalMerge::Starts;
    }
    if !matches!(value, None | Some(Ok(MergedCellTypeMember::Continue))) {
        return HorizontalMerge::Unsettled;
    }
    match previous {
        Some((column, HorizontalMerge::Starts)) => HorizontalMerge::Continues { column },
        Some((_, HorizontalMerge::NotMerged)) | None => HorizontalMerge::NotMerged,
        Some((_, found)) => found,
    }
}

/// An empty element `local` of `shape` with `layers` laid over it in order.
fn laid(local: &str, shape: &Shape, layers: &[Element]) -> Element {
    let mut merged = empty(local);
    for layer in layers {
        apply(&mut merged, layer, Some(shape));
    }
    merged
}

/// What a `w:tblLook` turns on, as the bits of `w:val` (ECMA-376 Part 1, sections 17.4.54 and
/// 17.4.55, and Part 4, section 14.4.12): an attribute states its own setting, `w:val` the ones
/// whose attribute is absent; no `w:tblLook` turns on the bands only.
fn look_of(element: Option<&TableStyleConditionalFormattingSettingsException>) -> u16 {
    let Some(look) = element else {
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

/// The place in a vertical merge of a cell whose effective properties are `cell`, over the grid
/// columns `columns`, under the cells `above` (sections 17.4.84 and 17.18.57).
fn merge_of(
    cell: &TableCellProperties,
    columns: Option<&Range<usize>>,
    above: &[Placed],
) -> VerticalMerge {
    let Some(merge) = cell.vertically_merged_cell.as_deref() else {
        return VerticalMerge::NotMerged;
    };
    let value = merge.val.as_ref().map(MergedCellType::value);
    if matches!(value, Some(Ok(MergedCellTypeMember::Restart))) {
        VerticalMerge::Starts
    } else if matches!(value, None | Some(Ok(MergedCellTypeMember::Continue))) {
        columns.map_or(VerticalMerge::Unsettled, |own| continued(own, above))
    } else {
        VerticalMerge::Unsettled
    }
}

/// `held` set to a clone of `over` when `over` is stated.
fn replace<T>(held: &mut Option<T>, over: Option<&T>)
where
    T: Clone,
{
    if let Some(found) = over {
        *held = Some(found.clone());
    }
}

/// The type of a conditional format, when it has a valid one.
fn type_of(
    format: &StyleConditionalTableFormattingProperties,
) -> Option<ConditionalTableStyleFormattingTypesMember> {
    format.r#type.as_ref()?.value().ok()
}
