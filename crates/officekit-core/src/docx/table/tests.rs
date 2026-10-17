use officekit_markup::wordprocessing::{
    BookmarkStart, RowLevelStructuredDocumentTag, RowLevelStructuredDocumentTagContent,
};

use super::{
    BlockContent, ContentCellContent, ContentRowContent, ParagraphContent, Table, TableRow,
    TableRowPropertiesContent, TableRows as _, TextRunContent,
};
use alloc::boxed::Box;
use alloc::vec::Vec;

fn two_rows() -> Table {
    let mut table = Table::default();
    table.append_row(&["a", "b"]);
    table.append_row(&["c", "d"]);
    table
}

fn marked(row: &TableRow) -> bool {
    row.properties.as_ref().is_some_and(|properties| {
        properties
            .content
            .iter()
            .any(|item| matches!(item, TableRowPropertiesContent::RepeatTableRow(_)))
    })
}

fn row_with_cells(cells: usize) -> TableRow {
    let mut table = Table::default();
    let texts = alloc::vec!["x"; cells];
    table.append_row(&texts).clone()
}

fn sdt_row() -> ContentRowContent {
    let mut control = RowLevelStructuredDocumentTag::default();
    control.content = Some(Box::new(RowLevelStructuredDocumentTagContent::default()));
    ContentRowContent::RowLevelStructuredDocumentTag(control)
}

#[test]
fn append_row_holds_one_preserved_paragraph_per_cell() {
    let mut table = Table::default();
    let row = table.append_row(&[" a ", "b"]);
    assert_eq!(row.content.len(), 2);
    assert!(matches!(
        row.content.first(),
        Some(ContentCellContent::TableCell(cell))
            if matches!(
                cell.content.as_slice(),
                [BlockContent::Paragraph(paragraph)]
                    if matches!(
                        paragraph.content.as_slice(),
                        [ParagraphContent::TextRun(run)]
                            if matches!(
                                run.content.as_slice(),
                                [TextRunContent::Text(text)]
                                    if text.space.as_deref() == Some("preserve") && text.value == " a "
                            )
                    )
            )
    ));
    assert_eq!(table.table_rows().count(), 1);
}

#[test]
fn repeat_header_rows_marks_only_the_first_rows() {
    let mut table = two_rows();
    table.repeat_header_rows(1);
    let flags: Vec<bool> = table.table_rows().map(marked).collect();
    assert_eq!(flags, [true, false]);
    table.repeat_header_rows(2);
    table.repeat_header_rows(0);
    assert!(table.table_rows().all(|row| !marked(row)));
}

#[test]
fn insert_row_keeps_a_bookmark_before_the_new_row() {
    let mut table = two_rows();
    table.content_row_content.insert(
        1,
        ContentRowContent::BookmarkStart(BookmarkStart::default()),
    );
    assert_eq!(table.insert_row(1, row_with_cells(3)), Ok(()));
    assert!(matches!(
        table.content_row_content[1],
        ContentRowContent::BookmarkStart(_)
    ));
    assert!(matches!(
        &table.content_row_content[2],
        ContentRowContent::TableRow(inserted) if inserted.content.len() == 3
    ));
    assert_eq!(table.table_rows().count(), 3);
}

#[test]
fn insert_row_at_the_row_count_goes_after_the_last_row() {
    let mut table = two_rows();
    assert_eq!(table.insert_row(2, row_with_cells(1)), Ok(()));
    assert_eq!(table.table_rows().count(), 3);
    assert_eq!(
        table.table_rows().last().map(|row| row.content.len()),
        Some(1)
    );
}

#[test]
fn insert_row_above_the_row_count_gives_the_row_back() {
    let mut table = two_rows();
    let row = row_with_cells(4);
    let back = table.insert_row(3, row.clone());
    assert_eq!(back, Err(Box::new(row)));
    assert_eq!(table.table_rows().count(), 2);
}

#[test]
fn remove_row_skips_a_row_level_content_control() {
    let mut table = two_rows();
    table.content_row_content.insert(1, sdt_row());
    let removed = table.remove_row(1);
    assert_eq!(removed.map(|row| row.content.len()), Some(2));
    assert_eq!(table.table_rows().count(), 1);
    assert!(matches!(
        table.content_row_content[1],
        ContentRowContent::RowLevelStructuredDocumentTag(_)
    ));
    assert_eq!(table.remove_row(1), None);
}

#[test]
fn table_rows_skips_a_row_level_content_control() {
    let mut table = two_rows();
    table.content_row_content.insert(1, sdt_row());
    assert_eq!(table.table_rows().count(), 2);
}
