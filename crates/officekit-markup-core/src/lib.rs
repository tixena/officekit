//! `officekit-markup-core` is the typed tree's namespace modules that reference each other in a
//! cycle, on the runtime of `officekit-markup-base`; `officekit-markup` gathers them under its
//! paths.

#![recursion_limit = "256"]
#![allow(
    clippy::exhaustive_enums,
    reason = "ext matches the enums its `From` impls convert arm by arm, which `#[non_exhaustive]` forbids across crates"
)]

// The namespace modules, generated in a file of their own.
include!("modules.rs");
