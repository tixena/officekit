use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::mem::size_of;
use std::alloc::System;

use officekit_markup_core::wordprocessing::{
    MeasurementInHalfPoints, MeasurementInHalfPointsMember, NonComplexScriptFontSize, read,
};
use officekit_xml::parse::parse;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

thread_local! {
    /// Whether this thread's allocations are counted.
    static COUNTING: Cell<bool> = const { Cell::new(false) };

    /// The allocations this thread made since counting began.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

/// The system allocator, counting the allocations it makes.
struct Counting;

// SAFETY: every call goes to the system allocator unchanged; only the count is added.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if COUNTING.get() {
            ALLOCATIONS.set(ALLOCATIONS.get() + 1);
        }
        // SAFETY: the caller's contract for `alloc` is passed on as it is.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller's contract for `dealloc` is passed on as it is.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[test]
fn a_font_size_of_20_keeps_its_number_on_no_heap() {
    assert_eq!(size_of::<MeasurementInHalfPoints>(), 24);
    assert_eq!(size_of::<NonComplexScriptFontSize>(), 32);
    let text = "20".to_owned();
    ALLOCATIONS.set(0);
    COUNTING.set(true);
    let value = MeasurementInHalfPoints::from(text);
    COUNTING.set(false);
    assert_eq!(ALLOCATIONS.get(), 0);
    let twenty = MeasurementInHalfPoints::Member(
        MeasurementInHalfPointsMember::UnsignedDecimalNumberValue(20),
    );
    assert_eq!(value, twenty);
    let element = parse(
        b"<w:sz xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" w:val=\"20\"/>",
    )
    .unwrap()
    .root;
    assert_eq!(
        read::non_complex_script_font_size(&element).val,
        Some(twenty)
    );
}
