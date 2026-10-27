//! Peak heap while a template's row is repeated through `PartReader`, a write
//! hook, `PartWriter`, and `PackageWriter` into a sink that cannot seek,
//! measured by a counting allocator: one test in a binary of its own, so no
//! other test allocates while it measures. The allocator counts the measuring
//! thread only, because libtest's own thread allocates while a test runs.

#[cfg(test)]
#[path = "stream_generation/generation.rs"]
mod generation;
#[cfg(test)]
#[path = "stream_generation/tests.rs"]
mod tests;
