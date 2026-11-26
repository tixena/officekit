use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::fmt::Write as _;
use std::alloc::System;

use officekit::parts::part_root::{self, PartRoot as _};
use officekit_markup::wordprocessing::Document;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// Whether this thread's allocations are counted.
    static COUNTING: Cell<bool> = const { Cell::new(false) };

    /// The bytes this thread allocated and has not freed since counting began.
    static LIVE: Cell<isize> = const { Cell::new(0) };
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
        LIVE.set(LIVE.get().saturating_add(change));
    }
}

/// A main document part of `paragraphs` paragraphs, each of one to nine runs with as many run
/// properties.
fn main_part(paragraphs: usize) -> Vec<u8> {
    let properties = [
        "<w:rStyle w:val=\"Strong\"/>",
        "<w:rFonts w:ascii=\"Arial\"/>",
        "<w:b/>",
        "<w:bCs/>",
        "<w:i/>",
        "<w:iCs/>",
        "<w:caps/>",
        "<w:smallCaps/>",
        "<w:strike/>",
    ];
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document \
         xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>",
    );
    for index in 0..paragraphs {
        let runs = index % 9 + 1;
        xml.push_str("<w:p><w:pPr><w:pStyle w:val=\"Normal\"/><w:jc w:val=\"both\"/></w:pPr>");
        for run in 0..runs {
            let _ = write!(
                xml,
                "<w:r><w:rPr>{}</w:rPr><w:t xml:space=\"preserve\">run {run} of paragraph \
                 {index:06}</w:t></w:r>",
                properties[..runs].concat()
            );
        }
        xml.push_str("</w:p>");
    }
    xml.push_str("<w:sectPr/></w:body></w:document>");
    xml.into_bytes()
}

/// What `make` gives, and the heap it holds above what this thread held before it.
fn measured<T, F>(make: F) -> (T, usize)
where
    F: FnOnce() -> T,
{
    LIVE.set(0);
    COUNTING.set(true);
    let value = make();
    COUNTING.set(false);
    (value, usize::try_from(LIVE.get()).unwrap())
}

#[test]
fn a_large_parts_typed_tree_holds_the_heap_of_an_exact_clone_of_it() {
    let name = PartName::new("/word/document.xml").unwrap();
    let bytes = main_part(9_000);
    let mut package = Package::new();
    package.set_part_bytes(&name, MAIN, &bytes).unwrap();
    let tree = package.load_tree(&name, OpenOptions::default()).unwrap();

    let (forward, forward_heap) =
        measured(|| part_root::load::<Document>(&package, &name, OpenOptions::default()).unwrap());
    let (from_tree, tree_heap) = measured(|| Document::from_tree(&tree.root).unwrap());
    assert_eq!(forward, from_tree);
    let (clone, clone_heap) = measured(|| forward.clone());
    assert_eq!(clone, forward);
    for (read, heap) in [("forward", forward_heap), ("from the tree", tree_heap)] {
        assert!(
            heap.abs_diff(clone_heap) * 100 <= clone_heap,
            "the read {read} holds {heap} bytes; an exact clone of it holds {clone_heap}"
        );
    }
}
