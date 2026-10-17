//! Streaming a package.
//!
//! The write-through form of `Transform`, whose result goes to a part writer as
//! it is made, and a package written part by part into a sink that cannot seek.

pub mod package_writer;
pub mod transform;
