use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;
use std::io::{self, Write};

use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};

use super::generation::{generate, template};

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

/// Keeps nothing but the number of bytes it is given, as an upload keeps
/// nothing; it implements `Write` and not `Seek`.
struct CountingSink(usize);

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

impl Write for CountingSink {
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
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

/// The most heap the generation uses above what this thread held before it, and
/// the bytes it wrote.
fn peak_heap_while_generating(template: &Package, count: usize) -> (usize, usize) {
    LIVE.set(0);
    PEAK.set(0);
    COUNTING.set(true);
    let generated = generate(template, count, CountingSink(0));
    COUNTING.set(false);
    let sink = generated.unwrap();
    (usize::try_from(PEAK.get()).unwrap(), sink.0)
}

#[test]
fn a_row_repeated_100_000_times_streams_into_a_sink_that_cannot_seek_in_the_peak_heap_of_10() {
    let template = template().unwrap();
    let (peak_10, bytes_10) = peak_heap_while_generating(&template, 10);
    let (peak_100_000, bytes_100_000) = peak_heap_while_generating(&template, 100_000);
    eprintln!(
        "peak heap: {peak_10} bytes for 10 rows, {peak_100_000} for 100,000; \
         package: {bytes_10} bytes for 10 rows, {bytes_100_000} for 100,000"
    );
    assert!(bytes_100_000 > bytes_10);
    assert_eq!(
        peak_100_000, peak_10,
        "peak heap: {peak_10} bytes for 10 rows, {peak_100_000} for 100,000"
    );
    let package = Package::from_bytes(generate(&template, 10, Vec::new()).unwrap()).unwrap();
    let main = PartName::new("/word/document.xml").unwrap();
    assert!(package.part_bytes(&main).unwrap().starts_with(
        b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<w:document"
    ));
    let hyperlinks: Vec<(String, Target)> = package
        .related_by_type(Some(&main), &RelationshipType::HYPERLINK)
        .unwrap()
        .into_iter()
        .map(|related| (related.id, related.target))
        .collect();
    assert_eq!(
        hyperlinks,
        [(
            "rId2".to_owned(),
            Target::External("https://example.com/".to_owned())
        )]
    );
}
