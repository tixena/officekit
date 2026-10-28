//! The validator of officekit: it checks an Office Open XML package, a `.docx`, `.xlsx` or
//! `.pptx`, and lists what the Office release you name would not accept.
//!
//! Start with [`validator::Validator`]. Give it the release to check against, an
//! [`OfficeVersion`](officekit_xml::name::OfficeVersion) from Office 2010 to Microsoft 365, and a
//! package from `officekit_xml::packaging`. It returns a list of [`error::ValidationError`] values,
//! empty when the package is clean. A package comes from `Package::from_bytes`, or from
//! `Package::open` with an `os_shim::System`.
//!
//! A validation runs in passes.
//!
//! 1. The package pass walks the relationships from the package down. It reports a relationship
//!    to a part the source may not relate to, a part type that must appear once and appears
//!    twice, and a required part that is missing. See [`package`].
//! 2. The schema pass reads every part the relationships reach and checks its elements,
//!    attributes, values and child order against the schema of the chosen release, and the
//!    rules of markup compatibility (`mc:AlternateContent` and its attributes). See [`schema`].
//! 3. The semantic checks run in the same walk. They look at rules the schema cannot say, such
//!    as an attribute that must be absent when another has a given value, or a reference to a
//!    relationship that does not exist. See [`semantic`].
//!
//! A package whose main part relates with the Strict spelling is also checked for content that
//! exists only in Transitional.
//!
//! Each error has an [`error::ErrorCode`], the [`error::Finding`] that produced it (package,
//! schema, semantic or markup compatibility), the part, the path of elements to it, and a
//! message. The `officekit validate` command prints [`error::ValidationError::line`] for each
//! error, or [`error::ValidationError::record`] with `--json`. It exits with 0 when no error is
//! found, 1 when some are, and 2 when the check could not run.
//!
//! The tables behind the checks are generated from the schema data of the Open XML SDK 3.5.1 and
//! are public in [`generated`]. The types of the tables are in [`schema::types`] and
//! [`semantic::constraint`].
//!
//! # Example
//!
//! A Word document whose main part relates to a slide. A document may not hold a slide, so the
//! package pass reports it.
//!
//! ```
//! use officekit_xml::packaging::package::Package;
//! use officekit_xml::packaging::part_name::PartName;
//! use officekit::validation::error::ErrorCode;
//! use officekit::validation::validator::Validator;
//! use officekit_xml::name::OfficeVersion;
//!
//! const MAIN: &str =
//!     "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
//! const SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
//! const OFFICE_DOCUMENT: &str =
//!     "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
//! const SLIDE_RELATIONSHIP: &str =
//!     "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
//!
//! let mut package = Package::new();
//! let letter = PartName::new("/word/document.xml").unwrap();
//! let slide = PartName::new("/ppt/slides/slide1.xml").unwrap();
//! package
//!     .set_part_bytes(
//!         &letter,
//!         MAIN,
//!         br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body/></w:document>"#,
//!     )
//!     .unwrap();
//! package
//!     .set_part_bytes(
//!         &slide,
//!         SLIDE,
//!         br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#,
//!     )
//!     .unwrap();
//! package.relate(None, &letter, OFFICE_DOCUMENT).unwrap();
//! package
//!     .relate(Some(&letter), &slide, SLIDE_RELATIONSHIP)
//!     .unwrap();
//! let package = Package::from_bytes(package.to_bytes().unwrap()).unwrap();
//!
//! let errors = Validator::new(OfficeVersion::Microsoft365)
//!     .with_max_errors(1)
//!     .validate(&package);
//! assert_eq!(errors.len(), 1);
//! assert_eq!(errors[0].code, ErrorCode::PartNotAllowed);
//! assert_eq!(
//!     errors[0].line(),
//!     "/word/document.xml: package PartNotAllowed: the part /word/document.xml cannot relate to the part /ppt/slides/slide1.xml"
//! );
//! ```

pub mod error;
pub mod generated;
pub mod package;
pub mod schema;
pub mod semantic;
pub mod validator;

extern crate alloc;
