use alloc::sync::Arc;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::fmt::Write as _;
use std::alloc::System;

use officekit::parts::part_root;
use officekit_markup::wordprocessing::Document;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

/// How many times each unknown element repeats in the part.
const REPEATS: usize = 200;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// The layout counted, `(size, align)`; `(0, 0)` counts nothing.
    static COUNTED: Cell<(usize, usize)> = const { Cell::new((0, 0)) };

    /// The allocations of the counted layout this thread made since counting began.
    static FOUND: Cell<usize> = const { Cell::new(0) };

    /// The layout of this thread's last allocation.
    static LAST: Cell<(usize, usize)> = const { Cell::new((0, 0)) };
}

/// The system allocator, counting the allocations of one layout.
struct Counting;

// SAFETY: every call goes to the system allocator unchanged; only the counts are added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's contract for `alloc` is passed on as it is.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let seen = (layout.size(), layout.align());
            LAST.set(seen);
            if COUNTED.get() == seen {
                FOUND.set(FOUND.get().saturating_add(1));
            }
        }
        pointer
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc` is passed on as it is.
        unsafe { System.dealloc(ptr, layout) }
    }
}

/// The layout the allocation of an `Arc<str>` holding `text` has.
fn layout_of(text: &str) -> (usize, usize) {
    let made = Arc::<str>::from(text);
    let layout = LAST.get();
    drop(made);
    layout
}

/// How many allocations `read` makes of the layout an `Arc<str>` holding `text` has.
fn allocations_like<T, F>(text: &str, read: F) -> usize
where
    F: FnOnce() -> T,
{
    let layout = layout_of(text);
    FOUND.set(0);
    COUNTED.set(layout);
    let value = read();
    COUNTED.set((0, 0));
    drop(value);
    FOUND.get()
}

/// A name of `length` characters ending in `tag`, a length no other allocation of the read has.
fn long_name(tag: char, length: usize) -> String {
    let mut name = "n".repeat(length - 1);
    name.push(tag);
    name
}

#[test]
fn a_read_allocates_each_distinct_unknown_name_once() {
    let locals = [
        long_name('a', 4093),
        long_name('b', 4093),
        long_name('c', 4093),
    ];
    let attribute = long_name('d', 3001);
    let prefix = long_name('e', 2003);
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document \
         xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:{prefix}=\"urn:vendor\"><w:body>"
    );
    for _ in 0..REPEATS {
        for local in &locals {
            write!(xml, "<{prefix}:{local} {prefix}:{attribute}=\"1\"/>").unwrap();
        }
    }
    xml.push_str("<w:sectPr/></w:body></w:document>");
    let name = PartName::new("/word/document.xml").unwrap();
    let mut package = Package::new();
    package.set_part_bytes(&name, MAIN, xml.as_bytes()).unwrap();
    let read = || part_root::load::<Document>(&package, &name, OpenOptions::default()).unwrap();
    let unknown = format!("{:?}", read());
    assert_eq!(unknown.matches(locals[0].as_str()).count(), REPEATS);

    assert_eq!(allocations_like(&locals[0], read), locals.len());
    assert_eq!(allocations_like(&attribute, read), 1);
    assert_eq!(allocations_like(&prefix, read), 1);
}
