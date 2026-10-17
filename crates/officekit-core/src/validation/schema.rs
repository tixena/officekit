//! The schema pass.
//!
//! The types of the validation tables, a value's checks (`simple_types`), an
//! element's attributes (`attributes`), its content model (`particle`), and
//! the walk over a part's tree that runs them (`walk`).

pub mod attributes;
pub mod particle;
pub mod simple_types;
pub mod types;
pub mod walk;

#[cfg(test)]
mod tests;
