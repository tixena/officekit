use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;

use officekit::parts::main_document;
use officekit::parts::part_root::{self, PartRoot as _};
use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentBody, Paragraph, ParagraphContent, Text, TextRun,
    TextRunContent,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::WriteOptions;

/// What a store over a large part may hold beyond its trees and the old part's bytes: the
/// reader's buffers and the new part's bytes.
const MARGIN: usize = 64 * 1024;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// Whether this thread's allocations are counted.
    static COUNTING: Cell<bool> = const { Cell::new(false) };

    /// The bytes this thread allocated and has not freed since counting began;
    /// frees of bytes allocated before it make the count negative.
    static LIVE: Cell<isize> = const { Cell::new(0) };

    /// The most `LIVE` reached since it was last reset.
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

/// The system allocator, counting the bytes it holds.
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

/// Adds `change` bytes to this thread's count while it counts.
fn count(change: isize) {
    if COUNTING.get() {
        let live = LIVE.get().saturating_add(change);
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
    }
}

/// Start counting this thread's allocations from zero.
fn start() {
    LIVE.set(0);
    PEAK.set(0);
    COUNTING.set(true);
}

/// The bytes this thread holds now that it did not hold when counting began.
fn live() -> usize {
    usize::try_from(LIVE.get()).unwrap()
}

/// The most bytes this thread held since `PEAK` was last reset, then stop counting.
fn stop() -> usize {
    COUNTING.set(false);
    usize::try_from(PEAK.get()).unwrap()
}

/// A main document of `count` paragraphs, each holding one run of text.
fn document(count: usize) -> Document {
    let mut body = DocumentBody::default();
    body.content = (0..count)
        .map(|index| {
            let mut text = Text::default();
            text.value = format!("paragraph {index:06}");
            let mut run = TextRun::default();
            run.content.push(TextRunContent::Text(text));
            let mut paragraph = Paragraph::default();
            paragraph.content.push(ParagraphContent::TextRun(run));
            BlockContent::Paragraph(paragraph)
        })
        .collect();
    let mut document = Document::default();
    document.body = Some(Box::new(body));
    document
}

/// The bytes the generic tree of `document` takes in `class`, and the bytes a copy of it takes.
fn generic_tree_bytes(document: &Document, class: ConformanceClass) -> (usize, usize) {
    let before = live();
    let tree = document.to_tree(class).unwrap();
    let built = live() - before;
    let copy = tree.clone();
    let copied = live() - before - built;
    drop(copy);
    drop(tree);
    (built, copied)
}

fn main_part() -> PartName {
    PartName::new("/word/document.xml").unwrap()
}

/// Store `document` into `package`'s main part in `class`; the most bytes this thread held while
/// it did, above what it held before.
fn peak_while_storing(
    package: &mut Package,
    document: &Document,
    class: ConformanceClass,
) -> usize {
    let before = live();
    PEAK.set(LIVE.get());
    part_root::store(
        package,
        &main_part(),
        main_document::CONTENT_TYPES[0],
        document,
        class,
        WriteOptions::default(),
    )
    .unwrap();
    stop() - before
}

/// Below the typed tree, the generic tree it writes as, and a copy of that: the copy the
/// namespace cleanup made of the generic tree.
#[test]
fn a_store_holds_less_than_its_trees_and_a_copy_of_the_generic_tree() {
    for class in [ConformanceClass::Strict, ConformanceClass::Transitional] {
        let mut package = Package::new();
        start();
        let typed = document(20_000);
        let typed_bytes = live();
        let (generic, copy) = generic_tree_bytes(&typed, class);
        let peak = typed_bytes + peak_while_storing(&mut package, &typed, class);
        assert!(
            peak < typed_bytes + generic + copy,
            "{class:?}: peak {peak} bytes, typed tree {typed_bytes}, generic tree {generic}, its copy {copy}"
        );
    }
}

/// Below the new part's typed and generic trees, the old part's bytes and a small margin: the old
/// part is read for its declaration, prolog and epilog, never parsed into a tree.
#[test]
fn a_store_over_a_large_part_holds_its_bytes_and_never_its_tree() {
    for class in [ConformanceClass::Strict, ConformanceClass::Transitional] {
        let mut package = Package::new();
        part_root::store(
            &mut package,
            &main_part(),
            main_document::CONTENT_TYPES[0],
            &document(20_000),
            class,
            WriteOptions::default(),
        )
        .unwrap();
        let old_bytes = package.part_bytes(&main_part()).unwrap().len();
        start();
        let typed = document(10);
        let typed_bytes = live();
        let (generic, _) = generic_tree_bytes(&typed, class);
        let peak = typed_bytes + peak_while_storing(&mut package, &typed, class);
        assert!(
            peak < typed_bytes + generic + old_bytes + MARGIN,
            "{class:?}: peak {peak} bytes, new trees {typed_bytes} and {generic}, old part {old_bytes}"
        );
    }
}
