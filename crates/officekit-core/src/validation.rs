//! Schema validation: what SDK 3.5.1's schema data says each schema type,
//! attribute, value, content model, and part allows, as static tables
//! (`generated`), and the types of those tables (`schema`).
//!
//! Will hold the validator and the errors it reports.

pub mod generated;
pub mod schema;
