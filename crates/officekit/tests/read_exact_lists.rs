//! The heap a large main document part's typed tree holds after a read, against an exact clone
//! of it, measured by a counting allocator: one test in a binary of its own, so no other test
//! allocates while it measures. The allocator counts the measuring thread only, because
//! libtest's own thread allocates while a test runs.

#[cfg(test)]
#[path = "read_exact_lists/tests.rs"]
mod tests;
