//! Streaming a part and a package (D31, D36, D37).
//!
//! A part read forward as events, a part written one element at a time, the
//! write-through form of `Transform`, whose result goes to a part writer as it
//! is made, and a package written part by part into a sink that cannot seek.

pub mod package_writer;
pub mod reader;
pub mod transform;
pub mod writer;
