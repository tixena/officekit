//! Row operations on a table: the trait [`TableRows`], which [`Table`] implements; and the walks
//! [`every_row`] and [`every_cell`] over the rows and cells the document has.

use alloc::boxed::Box;
use alloc::vec::Vec;

use officekit_markup::wordprocessing::{
    BlockContent, ContentCellContent, ContentRowContent, Paragraph, ParagraphContent,
    RepeatTableRow, Table, TableCell, TableRow, TableRowProperties, TableRowPropertiesContent,
    Text, TextRun, TextRunContent,
};

/// Row operations on a [`Table`].
pub trait TableRows {
    /// Appends a row after the last row, with one cell per text, and returns it.
    fn append_row(&mut self, texts: &[&str]) -> &mut TableRow;

    /// Inserts `row` so that it becomes row `index`; an index equal to the row count puts it right
    /// after the last row.
    ///
    /// # Errors
    /// The row, given back, when `index` is above the row count.
    fn insert_row(&mut self, index: usize, row: TableRow) -> Result<(), Box<TableRow>>;

    /// Removes row `index` and returns it; `None` past the last row.
    fn remove_row(&mut self, index: usize) -> Option<TableRow>;

    /// Gives each of the first `count` rows a `w:tblHeader` and removes it from every later row.
    fn repeat_header_rows(&mut self, count: usize);

    /// The table's `w:tr` children in order; a row inside a row-level content control or custom
    /// XML element is not one of them.
    fn table_rows(&self) -> impl Iterator<Item = &TableRow>;
}

impl TableRows for Table {
    fn append_row(&mut self, texts: &[&str]) -> &mut TableRow {
        let content = texts
            .iter()
            .map(|text| {
                let mut cell = TableCell::default();
                cell.content = alloc::vec![BlockContent::Paragraph(paragraph_of(text))];
                ContentCellContent::TableCell(cell)
            })
            .collect();
        let mut row = TableRow::default();
        row.content = content;
        self.content_row_content
            .push(ContentRowContent::TableRow(row));
        table_rows_mut(self).next_back().unwrap()
    }

    fn insert_row(&mut self, index: usize, row: TableRow) -> Result<(), Box<TableRow>> {
        let positions = row_positions(self);
        let at = match positions.get(index) {
            Some(at) => *at,
            None if index == positions.len() => positions
                .last()
                .map_or(self.content_row_content.len(), |last| last + 1),
            None => return Err(Box::new(row)),
        };
        self.content_row_content
            .insert(at, ContentRowContent::TableRow(row));
        Ok(())
    }

    fn remove_row(&mut self, index: usize) -> Option<TableRow> {
        let at = *row_positions(self).get(index)?;
        if let ContentRowContent::TableRow(row) = self.content_row_content.remove(at) {
            Some(row)
        } else {
            None
        }
    }

    fn repeat_header_rows(&mut self, count: usize) {
        for (index, row) in table_rows_mut(self).enumerate() {
            if index < count {
                mark_header(row);
            } else {
                unmark_header(row);
            }
        }
    }

    fn table_rows(&self) -> impl Iterator<Item = &TableRow> {
        self.content_row_content.iter().filter_map(row_of)
    }
}

/// Every `w:tc` of `row` in order, those inside cell-level custom XML elements and content
/// controls too (ECMA-376 Part 1, sections 17.4.65, 17.5.1.4 and 17.5.2.32).
#[must_use = "this only returns the value"]
pub fn every_cell(row: &TableRow) -> Vec<&TableCell> {
    let mut cells = Vec::new();
    push_cells(&row.content, &mut cells);
    cells
}

/// Every `w:tr` of `table` in order, those inside row-level custom XML elements and content
/// controls too (ECMA-376 Part 1, sections 17.4.78, 17.5.1.5 and 17.5.2.30).
#[must_use = "this only returns the value"]
pub fn every_row(table: &Table) -> Vec<&TableRow> {
    let mut rows = Vec::new();
    push_rows(&table.content_row_content, &mut rows);
    rows
}

const fn is_header(item: &TableRowPropertiesContent) -> bool {
    matches!(item, TableRowPropertiesContent::RepeatTableRow(_))
}

fn mark_header(row: &mut TableRow) {
    let properties = row
        .properties
        .get_or_insert_with(|| Box::new(TableRowProperties::default()));
    if !properties.content.iter().any(is_header) {
        properties
            .content
            .push(TableRowPropertiesContent::RepeatTableRow(
                RepeatTableRow::default(),
            ));
    }
}

fn paragraph_of(text: &str) -> Paragraph {
    let mut characters = Text::default();
    characters.space = Some("preserve".to_owned());
    text.clone_into(&mut characters.value);
    let mut run = TextRun::default();
    run.content = alloc::vec![TextRunContent::Text(characters)];
    let mut paragraph = Paragraph::default();
    paragraph.content = alloc::vec![ParagraphContent::TextRun(run)];
    paragraph
}

/// What a cell-level custom XML element or content control holds; nothing for any other item.
fn cell_children(item: &ContentCellContent) -> &[ContentCellContent] {
    if let ContentCellContent::CellLevelCustomXMLElement(element) = item {
        &element.content
    } else if let ContentCellContent::CellLevelStructuredDocumentTag(tag) = item {
        tag.content.as_deref().map_or(&[], |inner| &inner.content)
    } else {
        &[]
    }
}

fn push_cells<'row>(content: &'row [ContentCellContent], cells: &mut Vec<&'row TableCell>) {
    for item in content {
        if let ContentCellContent::TableCell(cell) = item {
            cells.push(cell);
        } else {
            push_cells(cell_children(item), cells);
        }
    }
}

fn push_rows<'table>(content: &'table [ContentRowContent], rows: &mut Vec<&'table TableRow>) {
    for item in content {
        if let ContentRowContent::TableRow(row) = item {
            rows.push(row);
        } else {
            push_rows(row_children(item), rows);
        }
    }
}

/// What a row-level custom XML element or content control holds; nothing for any other item.
fn row_children(item: &ContentRowContent) -> &[ContentRowContent] {
    if let ContentRowContent::RowLevelCustomXMLElement(element) = item {
        &element.content
    } else if let ContentRowContent::RowLevelStructuredDocumentTag(tag) = item {
        tag.content.as_deref().map_or(&[], |inner| &inner.content)
    } else {
        &[]
    }
}

fn row_positions(table: &Table) -> Vec<usize> {
    table
        .content_row_content
        .iter()
        .enumerate()
        .filter_map(|(at, item)| matches!(item, ContentRowContent::TableRow(_)).then_some(at))
        .collect()
}

const fn row_of(item: &ContentRowContent) -> Option<&TableRow> {
    if let ContentRowContent::TableRow(row) = item {
        Some(row)
    } else {
        None
    }
}

const fn row_of_mut(item: &mut ContentRowContent) -> Option<&mut TableRow> {
    if let ContentRowContent::TableRow(row) = item {
        Some(row)
    } else {
        None
    }
}

fn table_rows_mut(table: &mut Table) -> impl DoubleEndedIterator<Item = &mut TableRow> {
    table.content_row_content.iter_mut().filter_map(row_of_mut)
}

fn unmark_header(row: &mut TableRow) {
    if let Some(properties) = row.properties.as_mut() {
        properties.content.retain(|item| !is_header(item));
    }
}

#[cfg(test)]
mod tests;
