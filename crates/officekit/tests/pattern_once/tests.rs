use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use std::alloc::System;

use officekit::validation::generated::{SCHEMA_TYPES, type_id};
use officekit::validation::schema::simple_types::failures;
use officekit::validation::schema::types::{Check, ValueClass};
use officekit_xml::name::OfficeVersion;

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
fn ten_thousand_guids_compile_their_pattern_once() {
    let row =
        &SCHEMA_TYPES[usize::from(type_id("a15:CT_SignatureLine/a15:signatureLine").unwrap())];
    let id = row
        .attributes
        .iter()
        .find(|attribute| attribute.name == "id")
        .unwrap();
    let check = id
        .checks
        .iter()
        .map(|check| &check.check)
        .find(|check| {
            matches!(
                check,
                Check::String {
                    class: _class,
                    kind: _kind,
                    length: _length,
                    max_length: _max_length,
                    min_length: _min_length,
                    pattern: Some(_),
                }
            )
        })
        .unwrap();
    let target = OfficeVersion::Microsoft365;
    let texts: Vec<String> = (0_u32..10_000)
        .map(|number| format!("{{{number:08X}-0000-0000-0000-{number:012X}}}"))
        .collect();
    let (first, found) = counted(|| {
        failures(
            check,
            &ValueClass::String,
            "{00000000-0000-0000-0000-00000000000A}",
            target,
        )
    });
    assert!(first > 0, "the first value compiles the pattern");
    assert_eq!(found, Vec::<String>::new());
    let (rest, passed) = counted(|| {
        texts
            .iter()
            .filter(|text| failures(check, &ValueClass::String, text, target).is_empty())
            .count()
    });
    assert_eq!(rest, 0);
    assert_eq!(passed, 10_000);
    assert_eq!(
        failures(check, &ValueClass::String, "{0000}", target),
        [
            "does not match the pattern \\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\\}"
        ]
    );
}
