use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use core::mem::size_of;
use std::alloc::System;

use officekit_markup_core::wordprocessing::{
    ColorValue, ColorValueMember, MeasurementInHalfPoints, MeasurementInHalfPointsMember,
    NonComplexScriptFontSize, read,
};
use officekit_xml::parse::parse;
use officekit_xml::values::{Measure, MeasureUnit};

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

/// The allocations `run` makes on this thread, and what it returns.
fn counted<T>(run: impl FnOnce() -> T) -> (usize, T) {
    ALLOCATIONS.set(0);
    COUNTING.set(true);
    let value = run();
    COUNTING.set(false);
    (ALLOCATIONS.get(), value)
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

#[test]
fn a_hex_colour_read_by_its_unions_second_member_takes_no_heap() {
    let text = "FF0000".to_owned();
    let colour = ColorValueMember::HexadecimalColorValue(0x00FF_0000);
    let (parsed, member) = counted(|| ColorValue::parse(&text));
    assert_eq!(parsed, 0);
    assert_eq!(member, Ok(colour.clone()));
    let (from_text, value) = counted(|| ColorValue::from(text));
    assert_eq!(from_text, 0);
    assert_eq!(value, ColorValue::Member(colour));
}

#[test]
fn ten_thousand_measures_compile_their_pattern_once() {
    let texts: Vec<String> = (0_u32..10_000)
        .map(|number| format!("{number}.5pt"))
        .collect();
    let (first, member) = counted(|| MeasurementInHalfPoints::parse("1.5pt"));
    assert!(first > 0, "the first value compiles the pattern");
    assert_eq!(
        member,
        Ok(MeasurementInHalfPointsMember::PositiveUniversalMeasurement(
            Measure::new(MeasureUnit::Points, 1.5)
        ))
    );
    let (rest, matched) = counted(|| {
        texts
            .iter()
            .filter(|text| MeasurementInHalfPoints::parse(text).is_ok())
            .count()
    });
    assert_eq!(rest, 0);
    assert_eq!(matched, 10_000);
}
