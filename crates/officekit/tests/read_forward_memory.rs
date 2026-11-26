//! Peak heap while a large main document part is read into its typed tree,
//! measured by a counting allocator: one test in a binary of its own, so no
//! other test allocates while it measures. The allocator counts the measuring
//! thread only, because libtest's own thread allocates while a test runs.

#[cfg(test)]
#[path = "read_forward_memory/tests.rs"]
mod tests;
