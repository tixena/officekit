//! The validator: the target version and the cap on errors it reports.

use alloc::vec::Vec;

use super::error::ValidationError;
use crate::packaging::package::Package;
use crate::xml::name::OfficeVersion;

#[cfg(test)]
mod tests;

/// Checks a package against one target version, stopping at a number of errors.
#[derive(Debug, Clone, Copy)]
pub struct Validator {
    max_errors: usize,
    version: OfficeVersion,
}

impl Validator {
    pub const DEFAULT_MAX_ERRORS: usize = 1000;

    /// Keeps the errors up to the cap; the one that reaches it is kept.
    #[must_use]
    pub fn limit(&self, mut errors: Vec<ValidationError>) -> Vec<ValidationError> {
        if self.max_errors != 0 {
            errors.truncate(self.max_errors);
        }
        errors
    }

    #[must_use]
    pub const fn max_errors(&self) -> usize {
        self.max_errors
    }

    #[must_use]
    pub const fn new(version: OfficeVersion) -> Self {
        Self {
            max_errors: Self::DEFAULT_MAX_ERRORS,
            version,
        }
    }

    /// Runs no pass yet, so no package has errors.
    #[must_use]
    pub fn validate(&self, _package: &Package) -> Vec<ValidationError> {
        self.limit(Vec::new())
    }

    #[must_use]
    pub const fn version(&self) -> OfficeVersion {
        self.version
    }

    /// `0` means no limit.
    #[must_use]
    pub const fn with_max_errors(mut self, max_errors: usize) -> Self {
        self.max_errors = max_errors;
        self
    }
}
