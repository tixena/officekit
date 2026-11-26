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

    /// The `LIVE` above which a rise is counted.
    static FLOOR: Cell<isize> = const { Cell::new(isize::MAX) };

    /// The rises of `LIVE` above `FLOOR` since it was set.
    static RISES: Cell<usize> = const { Cell::new(0) };

    /// The most `LIVE` reached in the latest rise above `FLOOR`.
    static RISE_PEAK: Cell<isize> = const { Cell::new(0) };
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
        let before = LIVE.get();
        let live = before.saturating_add(change);
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
        let floor = FLOOR.get();
        if live > floor {
            if before <= floor {
                RISES.set(RISES.get().saturating_add(1));
                RISE_PEAK.set(live);
            }
            RISE_PEAK.set(RISE_PEAK.get().max(live));
        }
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

/// Count each rise of this thread's bytes above `floor` from now on.
#[cfg(all(feature = "strict", feature = "transitional"))]
fn watch_rises_above(floor: usize) {
    FLOOR.set(isize::try_from(floor).unwrap());
    RISES.set(0);
    RISE_PEAK.set(0);
}

/// The rises above the floor so far, and the most bytes the latest one held.
#[cfg(all(feature = "strict", feature = "transitional"))]
fn rises() -> (usize, usize) {
    (RISES.get(), usize::try_from(RISE_PEAK.get()).unwrap())
}

/// Below the generic tree the Transitional spelling pass parses of a large main part and a copy
/// of it: the copy the namespace cleanup made of that tree. A save from Strict to Transitional
/// holds the part's trees twice, once to store the typed root and once to respell the stored
/// part, and frees them in between; the second rise is the spelling pass.
#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn the_transitional_spelling_pass_holds_less_than_its_tree_and_a_copy_of_it() {
    use officekit::docx::Docx;
    use officekit::parts::kinds::WordprocessingKind;
    use officekit_markup::wordprocessing::{
        HorizontalAlignmentType, ParagraphAlignment, ParagraphProperties,
    };
    use officekit_xml::packaging::package::OpenOptions;

    let mut typed = document(20_000);
    if let Some(BlockContent::Paragraph(first)) = typed
        .body
        .as_mut()
        .and_then(|body| body.content.first_mut())
    {
        let mut alignment = ParagraphAlignment::default();
        alignment.val = Some(HorizontalAlignmentType::from("left".to_owned()));
        let mut properties = ParagraphProperties::default();
        properties.paragraph_alignment = Some(Box::new(alignment));
        first.properties = Some(Box::new(properties));
    }
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    docx.set_main_document(&typed).unwrap();
    drop(typed);
    let strict = Docx::from_bytes(docx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    drop(docx);
    start();
    let (bytes, _) = strict.to_bytes_transitional().unwrap();
    let saved = Package::from_bytes(bytes).unwrap();
    let main = String::from_utf8(saved.part_bytes(&main_part()).unwrap()).unwrap();
    assert!(
        main.contains("<w:jc w:val=\"left\"/>"),
        "the part was respelled"
    );
    drop(main);
    let base = live();
    let tree = saved.tree_at(&main_part()).unwrap();
    let generic = live() - base;
    let copy = tree.clone();
    let copied = live() - base - generic;
    drop(copy);
    drop(tree);
    drop(saved);
    let before = live();
    watch_rises_above(before + generic / 4);
    strict.to_bytes_transitional().unwrap();
    let (count, rise_peak) = rises();
    stop();
    assert_eq!(
        count, 2,
        "one rise to store the typed root, one to respell the part"
    );
    let peak = rise_peak - before;
    assert!(
        peak < generic + copied,
        "peak {peak} bytes, generic tree {generic}, its copy {copied}"
    );
}
