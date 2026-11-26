use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;

use officekit::parts::main_document;
use officekit::parts::part_root;
use officekit_markup::wordprocessing::{
    BlockContent, Bold, Document, DocumentBody, HorizontalAlignmentType, Paragraph,
    ParagraphAlignment, ParagraphContent, ParagraphProperties, RunProperties, RunPropertiesContent,
    Text, TextRun, TextRunContent,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::WriteOptions;

/// What a store may hold besides its typed tree and twice its bytes: the package's content
/// types, the chunk list of the buffer, and the part writer's scope.
const MARGIN: usize = 64 * 1024;

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

/// The system allocator, counting the bytes this thread holds.
struct Counting;

// SAFETY: every call goes to the system allocator unchanged; only the byte
// counts are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed on as it is.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            count(isize::try_from(layout.size()).unwrap_or(isize::MAX));
        }
        pointer
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-isize::try_from(layout.size()).unwrap_or(isize::MAX));
        // SAFETY: the caller's contract for `dealloc` is passed on as it is.
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn count(change: isize) {
    if COUNTING.get() {
        let live = LIVE.get().saturating_add(change);
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
    }
}

fn live() -> usize {
    usize::try_from(LIVE.get()).unwrap()
}

/// A document of `count` paragraphs, each aligned left and holding one bold run: the paragraph's
/// properties and the run go through the Strict rules as trees, the rest is written tag by tag.
fn document(count: usize) -> Document {
    let mut body = DocumentBody::default();
    body.content = (0..count)
        .map(|index| {
            let mut text = Text::default();
            text.value = format!("paragraph {index:06}");
            let mut run = TextRun::default();
            let mut properties = RunProperties::default();
            properties
                .content
                .push(RunPropertiesContent::Bold(Bold::default()));
            run.properties = Some(Box::new(properties));
            run.content.push(TextRunContent::Text(text));
            let mut alignment = ParagraphAlignment::default();
            alignment.val = Some(HorizontalAlignmentType::from("left".to_owned()));
            let mut paragraph_properties = ParagraphProperties::default();
            paragraph_properties.paragraph_alignment = Some(Box::new(alignment));
            let mut paragraph = Paragraph::default();
            paragraph.properties = Some(Box::new(paragraph_properties));
            paragraph.content.push(ParagraphContent::TextRun(run));
            BlockContent::Paragraph(paragraph)
        })
        .collect();
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document
}

#[test]
fn a_store_holds_its_typed_tree_and_twice_its_bytes() {
    let main = PartName::new("/word/document.xml").unwrap();
    for class in [ConformanceClass::Strict, ConformanceClass::Transitional] {
        let mut package = Package::new();
        LIVE.set(0);
        PEAK.set(0);
        COUNTING.set(true);
        let typed = document(20_000);
        let typed_bytes = live();
        part_root::store(
            &mut package,
            &main,
            main_document::CONTENT_TYPES[0],
            &typed,
            class,
            WriteOptions::default(),
        )
        .unwrap();
        COUNTING.set(false);
        let peak = usize::try_from(PEAK.get()).unwrap();
        let output = package.part_bytes(&main).unwrap().len();
        assert!(
            peak < typed_bytes + 2 * output + MARGIN,
            "{class:?}: peak {peak} bytes, typed tree {typed_bytes}, output {output}"
        );
    }
}
