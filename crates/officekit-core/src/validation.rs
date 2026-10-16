//! Validation of a package.
//!
//! What SDK 3.5.1's schema data says each schema type, attribute, value,
//! content model, and part allows, as static tables (`generated`), and the
//! types of those tables (`schema`); the errors the validator reports
//! (`error`) and the validator itself (`validator`).

pub mod error;
pub mod generated;
pub mod schema;
pub mod validator;
