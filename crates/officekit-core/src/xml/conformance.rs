//! The two conformance classes of ECMA-376.

#[cfg(test)]
mod tests;

/// The two conformance classes (D21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConformanceClass {
    Strict,
    Transitional,
}

impl ConformanceClass {
    /// Whether this build carries the class: its feature is on.
    #[must_use]
    pub const fn enabled(self) -> bool {
        match self {
            Self::Strict => cfg!(feature = "strict"),
            Self::Transitional => cfg!(feature = "transitional"),
        }
    }
}
