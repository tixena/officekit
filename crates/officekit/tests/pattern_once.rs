//! The allocations of the validator's pattern facets, measured by a counting allocator in a binary
//! of its own. The allocator counts the measuring thread only, because libtest's own thread and the
//! other tests allocate while a test runs.

#[cfg(test)]
#[path = "pattern_once/tests.rs"]
mod tests;
