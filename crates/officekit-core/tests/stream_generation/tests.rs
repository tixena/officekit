use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use std::alloc::System;
use std::io::{self, Write};

use officekit_packaging::package::Package;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};

use super::generation::{generate, template};

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The bytes the heap holds.
static LIVE: AtomicUsize = AtomicUsize::new(0);

/// The most bytes the heap held since the last reset.
static PEAK: AtomicUsize = AtomicUsize::new(0);

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
            let live = LIVE.fetch_add(layout.size(), Ordering::SeqCst) + layout.size();
            PEAK.fetch_max(live, Ordering::SeqCst);
        }
        pointer
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::SeqCst);
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

/// The most heap the generation uses above what was live before it, and the
/// bytes it wrote.
fn peak_heap_while_generating(template: &Package, count: usize) -> (usize, usize) {
    let before = LIVE.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    let sink = generate(template, count, CountingSink(0)).unwrap();
    (PEAK.load(Ordering::SeqCst) - before, sink.0)
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
