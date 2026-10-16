//! Validation of a package.
//!
//! What SDK 3.5.1's schema data says each schema type, attribute, value,
//! content model, and part allows, and the semantic constraints it registers,
//! as static tables (`generated`), and the types of those tables (`schema`,
//! `semantic`); the errors the validator reports (`error`), the package pass
//! over the relationship graph (`package`), the semantic checks
//! (`semantic`), and the validator itself (`validator`).

pub mod error;
pub mod generated;
pub mod package;
pub mod schema;
pub mod semantic;
pub mod validator;
