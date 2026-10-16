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

use regex::Regex;

#[cfg(test)]
mod tests;

/// Whether `value` matches `pattern` whole, as an XML Schema pattern facet
/// matches: the regular expression anchored at both ends. The pattern is
/// compiled on each call.
///
/// # Errors
///
/// A pattern the `regex` crate cannot compile.
pub fn matches_pattern(pattern: &str, value: &str) -> Result<bool, regex::Error> {
    Ok(Regex::new(&format!("^(?:{pattern})$"))?.is_match(value))
}
