//! The size and the heap of simple types read from text, measured by a counting allocator in a
//! binary of its own. The allocator counts the measuring thread only, because libtest's own thread
//! and the other tests allocate while a test runs.

#[cfg(test)]
#[path = "value_heap/tests.rs"]
mod tests;
