use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::fmt::Write as _;
use std::alloc::System;

use officekit::parts::part_root::{self, PartRoot as _};
use officekit_markup::wordprocessing::{BlockContent, Document};
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

/// What the read may hold past the typed tree and the part's bytes, besides the room the body's
/// list of blocks takes while it grows: the start tags and text of the element being read, and
/// the other lists' buffers growing.
const MARGIN: usize = 1 << 20;

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

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller's contract for `realloc` is passed on as it is.
        let pointer = unsafe { System.realloc(ptr, layout, new_size) };
        if !pointer.is_null() {
            count(-isize::try_from(layout.size()).unwrap_or(isize::MAX));
            count(isize::try_from(new_size).unwrap_or(isize::MAX));
        }
        pointer
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

/// A main document part of `paragraphs` paragraphs, each a styled bold run of text.
fn main_part(paragraphs: usize) -> Vec<u8> {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document \
         xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
    );
    for index in 0..paragraphs {
        let _ = write!(
            xml,
            "<w:p><w:pPr><w:pStyle w:val=\"Normal\"/><w:jc w:val=\"both\"/></w:pPr><w:r><w:rPr><w:b/>\
             <w:sz w:val=\"24\"/></w:rPr><w:t xml:space=\"preserve\">paragraph {index:06} of the \
             document</w:t></w:r></w:p>"
        );
    }
    xml.push_str("<w:sectPr/></w:body></w:document>");
    xml.into_bytes()
}

/// The typed root `read` gives, the most heap the read used above what this thread held before
/// it, and the heap the root holds.
fn measured<F>(read: F) -> (Document, usize, usize)
where
    F: FnOnce() -> Document,
{
    LIVE.set(0);
    PEAK.set(0);
    COUNTING.set(true);
    let document = read();
    COUNTING.set(false);
    let peak = usize::try_from(PEAK.get()).unwrap();
    let held = usize::try_from(LIVE.get()).unwrap();
    (document, peak, held)
}

#[test]
fn a_large_part_reads_into_its_typed_tree_in_the_heap_of_the_tree_and_the_parts_bytes() {
    let name = PartName::new("/word/document.xml").unwrap();
    let bytes = main_part(40_000);
    let mut package = Package::new();
    package.set_part_bytes(&name, MAIN, &bytes).unwrap();

    let (forward, peak, typed) =
        measured(|| part_root::load(&package, &name, OpenOptions::default()).unwrap());
    // A list grows by doubling, and a read shrinks it to its length once it is read.
    let growing = forward.body.as_deref().unwrap().content.len() * size_of::<BlockContent>();
    let bound = typed + bytes.len() + growing + MARGIN;
    assert!(
        peak <= bound,
        "peak heap {peak} bytes; the typed tree holds {typed}, the part is {} bytes",
        bytes.len()
    );

    let (from_tree, tree_peak, _) = measured(|| {
        let tree = package.load_tree(&name, OpenOptions::default()).unwrap();
        Document::from_tree(&tree.root).unwrap()
    });
    assert_eq!(forward, from_tree);
    assert!(
        tree_peak > bound,
        "the read through the generic tree peaks at {tree_peak} bytes, within {bound}"
    );
}
