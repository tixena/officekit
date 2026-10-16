//! The semantic pass.
//!
//! The constraints SDK 3.5.1 registers on element types (`constraint`, with
//! the generated table in `validation::generated`), and their checks, run on
//! each element the schema walk visits (`registry`).

pub mod constraint;
pub mod registry;

#[cfg(test)]
mod tests;
