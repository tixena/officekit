//! Row operations on a table: the trait [`TableRows`], which [`Table`] implements.

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
