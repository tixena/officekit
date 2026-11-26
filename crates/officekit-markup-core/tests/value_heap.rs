//! The size and the heap of a simple type that keeps its member, measured by a counting
//! allocator: one test in a binary of its own, so no other test allocates while it measures. The
//! allocator counts the measuring thread only, because libtest's own thread allocates while a
//! test runs.

#[cfg(test)]
#[path = "value_heap/tests.rs"]
mod tests;
