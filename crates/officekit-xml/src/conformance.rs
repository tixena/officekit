//! The two conformance classes of ECMA-376.

/// A package's conformance class: Strict (ECMA-376 Part 1), or Transitional (Part 1 with the
/// migration features of Part 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ConformanceClass {
    /// Strict: the class without the features kept for older files. Its namespaces and
    /// relationship types have their own URIs.
    Strict,
    /// Transitional: Strict plus the features older Office files use.
    Transitional,
}
