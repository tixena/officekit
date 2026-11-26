//! The allocations a typed read of a main document part makes for the names of the elements the
//! schemas do not declare, counted by a counting allocator: one test in a binary of its own, so
//! no other test allocates while it counts. The allocator counts the counting thread only,
//! because libtest's own thread allocates while a test runs.

extern crate alloc;

#[cfg(test)]
#[path = "unknown_names/tests.rs"]
mod tests;
