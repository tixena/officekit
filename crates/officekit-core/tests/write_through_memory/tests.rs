use core::alloc::{GlobalAlloc, Layout as Allocation};
use core::cell::Cell;
use std::alloc::System;
use std::io::{self, Write};

use officekit_markup::wordprocessing::{
    self, BlockContent, BlockLevelStructuredDocumentTag, BlockLevelStructuredDocumentTagContent,
    ContentBlockContent, ContentCellContent, ContentRowContent, FriendlyName, Paragraph,
    ParagraphContent, StructuredDocumentTagProperties, Table, TableCell, TableRow, Text, TextRun,
    TextRunContent,
};
use officekit_markup::{Descend, Written};
use officekit_traversal::{Visit as _, WriteThrough as _};
use officekit_xml::stream::writer::PartWriter;
use officekit_xml::write::Layout;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// Whether this thread's allocations are counted.
    static COUNTING: Cell<bool> = const { Cell::new(false) };

    /// The bytes this thread allocated and has not freed since counting began;
    /// frees of bytes allocated before it make the count negative.
    static LIVE: Cell<isize> = const { Cell::new(0) };

    /// The most `LIVE` reached since counting began.
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

/// The system allocator, counting the bytes it holds.
struct Counting;

/// Keeps nothing but the number of bytes it is given, as a zip entry streamed
/// into an upload keeps nothing.
struct CountingSink(usize);

/// Replaces the content control aliased "repeat" by a paragraph holding
/// `value`, in both block lists.
struct FillControl {
    value: String,
}

/// Finds the content control aliased "repeat".
struct FindRepeat {
    found: bool,
}

/// Writes the row holding a "repeat" control `count` times, one at a time.
struct StreamRepeatRows {
    count: usize,
}

// SAFETY: every call goes to the system allocator unchanged; only the byte
// counts are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Allocation) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed on as it is.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            count(isize::try_from(layout.size()).unwrap_or(isize::MAX));
        }
        pointer
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Allocation) {
        count(-isize::try_from(layout.size()).unwrap_or(isize::MAX));
        // SAFETY: the caller's contract for `dealloc` is passed on as it is.
        unsafe { System.dealloc(ptr, layout) }
    }
}

impl Write for CountingSink {
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
}

impl officekit_markup::Context for FillControl {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for FillControl {
    fn block(&mut self, node: &BlockContent) -> Option<BlockContent> {
        let BlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        is_repeat(sdt).then(|| BlockContent::Paragraph(paragraph(&self.value)))
    }

    fn content_block(&mut self, node: &ContentBlockContent) -> Option<ContentBlockContent> {
        let ContentBlockContent::BlockLevelStructuredDocumentTag(sdt) = node else {
            return None;
        };
        is_repeat(sdt).then(|| ContentBlockContent::Paragraph(paragraph(&self.value)))
    }
}

impl officekit_markup::Visitor for FindRepeat {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Visitor> {
        Some(self)
    }
}

impl wordprocessing::Visitor for FindRepeat {
    fn block_level_structured_document_tag(
        &mut self,
        node: &BlockLevelStructuredDocumentTag,
    ) -> Descend {
        self.found |= is_repeat(node);
        Descend::Yes
    }
}

impl officekit_markup::Context for StreamRepeatRows {
    fn wordprocessing(&mut self) -> Option<&mut dyn wordprocessing::Context> {
        Some(self)
    }
}

impl wordprocessing::Context for StreamRepeatRows {
    fn write_content_row_items(
        &mut self,
        node: &ContentRowContent,
        out: &mut PartWriter<'_>,
    ) -> io::Result<Written> {
        let mut find = FindRepeat { found: false };
        node.visit(&mut find);
        if !find.found {
            return Ok(Written::No);
        }
        for index in 0..self.count {
            node.write_through(
                &mut FillControl {
                    value: format!("item {index:06}"),
                },
                out,
            )?;
        }
        Ok(Written::Yes)
    }
}

/// Adds `change` bytes to this thread's count while it counts.
fn count(change: isize) {
    if COUNTING.get() {
        let live = LIVE.get().saturating_add(change);
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
    }
}

/// Whether a content control is aliased "repeat".
fn is_repeat(sdt: &BlockLevelStructuredDocumentTag) -> bool {
    sdt.properties
        .as_ref()
        .and_then(|properties| properties.friendly_name.as_ref())
        .and_then(|name| name.val.as_deref())
        == Some("repeat")
}

fn paragraph(value: &str) -> Paragraph {
    let mut text = Text::default();
    text.value = value.to_owned();
    let mut run = TextRun::default();
    run.content.push(TextRunContent::Text(text));
    let mut paragraph = Paragraph::default();
    paragraph.content.push(ParagraphContent::TextRun(run));
    paragraph
}

/// The most heap the write uses above what this thread held before it, and the
/// bytes it wrote.
fn peak_heap_while_writing(table: &Table, count: usize) -> (usize, usize) {
    let mut sink = CountingSink(0);
    LIVE.set(0);
    PEAK.set(0);
    COUNTING.set(true);
    let written = table.write_through(
        &mut StreamRepeatRows { count },
        &mut PartWriter::new(&mut sink, Layout::Compact),
    );
    COUNTING.set(false);
    written.unwrap();
    (usize::try_from(PEAK.get()).unwrap(), sink.0)
}

fn row(block: BlockContent) -> ContentRowContent {
    let mut cell = TableCell::default();
    cell.content.push(block);
    let mut row = TableRow::default();
    row.content.push(ContentCellContent::TableCell(cell));
    ContentRowContent::TableRow(row)
}

/// A header row, a row whose cell holds a "repeat" control, and a total row.
fn template() -> Table {
    let mut name = FriendlyName::default();
    name.val = Some("repeat".to_owned());
    let mut properties = StructuredDocumentTagProperties::default();
    properties.friendly_name = Some(Box::new(name));
    let mut content = BlockLevelStructuredDocumentTagContent::default();
    content
        .content
        .push(ContentBlockContent::Paragraph(paragraph("placeholder")));
    let mut control = BlockLevelStructuredDocumentTag::default();
    control.properties = Some(Box::new(properties));
    control.content = Some(Box::new(content));
    let mut table = Table::default();
    table.content_row_content.extend([
        row(BlockContent::Paragraph(paragraph("Name"))),
        row(BlockContent::BlockLevelStructuredDocumentTag(control)),
        row(BlockContent::Paragraph(paragraph("Total"))),
    ]);
    table
}

#[test]
fn a_row_repeated_100_000_times_writes_through_in_the_peak_heap_of_10() {
    let table = template();
    let (peak_10, bytes_10) = peak_heap_while_writing(&table, 10);
    let (_, bytes_11) = peak_heap_while_writing(&table, 11);
    let (peak_100_000, bytes_100_000) = peak_heap_while_writing(&table, 100_000);
    let row_bytes = bytes_11 - bytes_10;
    assert_eq!(
        bytes_100_000,
        bytes_10 + 99_990 * row_bytes,
        "every repeated row is written, {row_bytes} bytes each"
    );
    assert_eq!(
        peak_100_000, peak_10,
        "peak heap: {peak_10} bytes for 10 rows, {peak_100_000} for 100,000"
    );
}
