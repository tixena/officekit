use core::alloc::{GlobalAlloc, Layout as Allocation};
use core::cell::Cell;
use std::alloc::System;

use officekit::parts::part_root::{self, PartRoot as _};
use officekit::parts::{main_document, worksheet};
use officekit_markup::spreadsheet;
use officekit_markup::wordprocessing::{
    BlockContent, Document, DocumentBody, Paragraph, ParagraphContent, Text, TextRun,
    TextRunContent,
};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::{Layout, WriteOptions};

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

    /// The rises of `LIVE` above `FLOOR` since it was set. A rise ends when `LIVE` falls to half
    /// of `FLOOR`, so the small frees inside one phase never split it in two.
    static RISES: Cell<usize> = const { Cell::new(0) };

    /// Whether a rise above `FLOOR` has begun and not yet ended.
    static RISING: Cell<bool> = const { Cell::new(false) };

    /// The most `LIVE` reached in the latest rise above `FLOOR`.
    static RISE_PEAK: Cell<isize> = const { Cell::new(0) };
}

/// The system allocator, counting the bytes it holds.
struct Counting;

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

/// Adds `change` bytes to this thread's count while it counts.
fn count(change: isize) {
    if COUNTING.get() {
        let live = LIVE.get().saturating_add(change);
        LIVE.set(live);
        PEAK.set(PEAK.get().max(live));
        let floor = FLOOR.get();
        if live <= floor / 2 {
            RISING.set(false);
        }
        if live > floor {
            if !RISING.get() {
                RISING.set(true);
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
            text.value = format!("paragraph {index:06}").into();
            let mut run = TextRun::default();
            run.content.push(TextRunContent::from(text));
            let mut paragraph = Paragraph::default();
            paragraph.content.push(ParagraphContent::from(run));
            BlockContent::from(paragraph)
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

/// Store `document` into `package`'s main part in `class` with `options`; the most bytes this
/// thread held while it did, above what it held before.
fn peak_while_storing(
    package: &mut Package,
    document: &Document,
    class: ConformanceClass,
    options: WriteOptions,
) -> usize {
    let before = live();
    PEAK.set(LIVE.get());
    part_root::store(
        package,
        &main_part(),
        main_document::CONTENT_TYPES[0],
        document,
        class,
        options,
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
        let peak =
            typed_bytes + peak_while_storing(&mut package, &typed, class, WriteOptions::default());
        assert!(
            peak < typed_bytes + generic + copy,
            "{class:?}: peak {peak} bytes, typed tree {typed_bytes}, generic tree {generic}, its copy {copy}"
        );
    }
}

/// Below the typed tree and the generic tree it writes as: a store in the pretty layout writes
/// from the typed tree too, and holds no generic tree of the part.
#[test]
fn a_pretty_store_holds_less_than_its_trees() {
    for class in [ConformanceClass::Strict, ConformanceClass::Transitional] {
        let mut package = Package::new();
        start();
        let typed = document(20_000);
        let typed_bytes = live();
        let (generic, _) = generic_tree_bytes(&typed, class);
        let options = WriteOptions::new(Layout::Pretty, true);
        let peak = typed_bytes + peak_while_storing(&mut package, &typed, class, options);
        assert!(
            peak < typed_bytes + generic,
            "{class:?}: peak {peak} bytes, typed tree {typed_bytes}, generic tree {generic}"
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
        let peak =
            typed_bytes + peak_while_storing(&mut package, &typed, class, WriteOptions::default());
        assert!(
            peak < typed_bytes + generic + old_bytes + MARGIN,
            "{class:?}: peak {peak} bytes, new trees {typed_bytes} and {generic}, old part {old_bytes}"
        );
    }
}

/// A worksheet of `count` rows, each holding one cell with a number.
fn worksheet(count: usize) -> spreadsheet::Worksheet {
    let mut sheet = spreadsheet::Worksheet::default();
    sheet.sheet_data.row = (0..count)
        .map(|index| {
            let mut value = spreadsheet::CellValue::default();
            value.value = index.to_string().into();
            let mut cell = spreadsheet::Cell::default();
            cell.r = Some(format!("A{}", index + 1).into());
            cell.value = Some(Box::new(value));
            let mut row = spreadsheet::Row::default();
            row.r = Some((index + 1).to_string().into());
            row.cell.push(cell);
            row
        })
        .collect();
    sheet
}

/// Below the typed tree and the generic tree it writes as: a Strict store of a worksheet writes
/// its rows tag by tag, though its root names children Strict refuses.
#[test]
fn a_strict_worksheet_store_holds_less_than_its_trees() {
    let name = PartName::new("/xl/worksheets/sheet1.xml").unwrap();
    for layout in [Layout::Compact, Layout::Pretty] {
        let mut package = Package::new();
        start();
        let typed = worksheet(20_000);
        let typed_bytes = live();
        let tree = typed.to_tree(ConformanceClass::Strict).unwrap();
        let generic = live() - typed_bytes;
        drop(tree);
        let before = live();
        PEAK.set(LIVE.get());
        part_root::store(
            &mut package,
            &name,
            worksheet::CONTENT_TYPE,
            &typed,
            ConformanceClass::Strict,
            WriteOptions::new(layout, true),
        )
        .unwrap();
        let peak = typed_bytes + stop() - before;
        assert!(
            peak < typed_bytes + generic,
            "{layout:?}: peak {peak} bytes, typed tree {typed_bytes}, generic tree {generic}"
        );
    }
}

/// Count each rise of this thread's bytes above `floor` from now on.
#[cfg(all(feature = "strict", feature = "transitional"))]
fn watch_rises_above(floor: usize) {
    FLOOR.set(isize::try_from(floor).unwrap());
    RISES.set(0);
    RISING.set(false);
    RISE_PEAK.set(0);
}

/// The rises above the floor so far, and the most bytes the latest one held.
#[cfg(all(feature = "strict", feature = "transitional"))]
fn rises() -> (usize, usize) {
    (RISES.get(), usize::try_from(RISE_PEAK.get()).unwrap())
}

/// Below the typed tree of a large main part and the generic tree it writes as, in one rise: a
/// save from Strict to Transitional writes each value in Transitional's spelling while it stores
/// the typed root, and parses no tree of the stored part to respell it.
#[cfg(all(feature = "strict", feature = "transitional"))]
#[test]
fn a_save_from_strict_to_transitional_respells_with_no_generic_tree_of_its_main_part() {
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
    let read = strict.main_document().unwrap();
    let typed_bytes = live() - base;
    drop(read);
    let tree = saved.tree_at(&main_part()).unwrap();
    let generic = live() - base;
    drop(tree);
    drop(saved);
    let before = live();
    watch_rises_above(before + generic / 4);
    strict.to_bytes_transitional().unwrap();
    let (count, rise_peak) = rises();
    stop();
    assert_eq!(count, 1, "one rise, to store the typed root");
    let peak = rise_peak - before;
    assert!(
        peak < typed_bytes + generic,
        "peak {peak} bytes, typed tree {typed_bytes}, generic tree {generic}"
    );
}
