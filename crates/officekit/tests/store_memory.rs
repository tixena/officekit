//! Peak heap while a typed root is stored into a part, and while a Strict file is saved in
//! Transitional, measured by a counting allocator. The allocator counts the measuring thread
//! only, so the tests of this binary and libtest's own thread do not count toward each other.

#[cfg(test)]
#[path = "store_memory/tests.rs"]
mod tests;
