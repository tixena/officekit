//! Peak heap while a row is repeated through the write-through form,
//! measured by a counting allocator: one test in a binary of its own, so no
//! other test allocates while it measures. The allocator counts the measuring
//! thread only, because libtest's own thread allocates while a test runs.

#[cfg(test)]
#[path = "write_through_memory/tests.rs"]
mod tests;
