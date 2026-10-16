//! The types of the validation tables, and the pattern match their string
//! checks run.

pub mod types;

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
