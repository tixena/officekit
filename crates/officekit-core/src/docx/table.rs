//! Row operations on a table: the only inherent impl of [`Table`].

use crate::markup::wordprocessing::{
    BlockContent, ContentCellContent, ContentRowContent, Paragraph, ParagraphContent,
    RepeatTableRow, Table, TableCell, TableRow, TableRowProperties, TableRowPropertiesContent,
    Text, TextRun, TextRunContent,
};
use alloc::boxed::Box;
use alloc::vec::Vec;

impl Table {
    /// Appends a row after the last list item with one cell per text.
    ///
    /// # Panics
    /// Never: the row it returns was pushed by the call.
    pub fn append_row(&mut self, texts: &[&str]) -> &mut TableRow {
        let content = texts
            .iter()
            .map(|text| {
                ContentCellContent::TableCell(TableCell {
                    content: alloc::vec![BlockContent::Paragraph(paragraph_of(text))],
                    ..TableCell::default()
                })
            })
            .collect();
        self.content_row_content
            .push(ContentRowContent::TableRow(TableRow {
                content,
                ..TableRow::default()
            }));
        self.table_rows_mut().next_back().unwrap()
    }

    /// Inserts `row` so that it becomes row `index`; an index equal to the row count puts it right
    /// after the last row.
    ///
    /// # Errors
    /// The row, given back, when `index` is above the row count.
    pub fn insert_row(&mut self, index: usize, row: TableRow) -> Result<(), Box<TableRow>> {
        let positions = self.row_positions();
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

    /// Removes row `index` and returns it; `None` past the last row.
    pub fn remove_row(&mut self, index: usize) -> Option<TableRow> {
        let at = *self.row_positions().get(index)?;
        if let ContentRowContent::TableRow(row) = self.content_row_content.remove(at) {
            Some(row)
        } else {
            None
        }
    }

    /// Gives each of the first `count` rows a `w:tblHeader` and removes it from every later row.
    pub fn repeat_header_rows(&mut self, count: usize) {
        for (index, row) in self.table_rows_mut().enumerate() {
            if index < count {
                mark_header(row);
            } else {
                unmark_header(row);
            }
        }
    }

    fn row_positions(&self) -> Vec<usize> {
        self.content_row_content
            .iter()
            .enumerate()
            .filter_map(|(at, item)| matches!(item, ContentRowContent::TableRow(_)).then_some(at))
            .collect()
    }

    /// The table's `w:tr` children in order; a row inside a row-level content control or custom
    /// XML element is not one of them.
    pub fn table_rows(&self) -> impl Iterator<Item = &TableRow> {
        self.content_row_content.iter().filter_map(row_of)
    }

    fn table_rows_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut TableRow> {
        self.content_row_content.iter_mut().filter_map(row_of_mut)
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
    Paragraph {
        content: alloc::vec![ParagraphContent::TextRun(TextRun {
            content: alloc::vec![TextRunContent::Text(Text {
                space: Some("preserve".to_owned()),
                value: text.to_owned(),
                ..Text::default()
            })],
            ..TextRun::default()
        })],
        ..Paragraph::default()
    }
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

fn unmark_header(row: &mut TableRow) {
    if let Some(properties) = row.properties.as_mut() {
        properties.content.retain(|item| !is_header(item));
    }
}

#[cfg(test)]
mod tests;
