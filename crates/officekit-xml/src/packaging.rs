//! The container under every Office file: parts, content types, relationships and images.
//!
//! A `.docx`, `.xlsx` or `.pptx` file is a zip archive. Each entry of the archive is a part, such
//! as `/word/document.xml`. One part, `[Content_Types].xml`, gives every part its content type.
//! Relationship parts link a part to the parts and the web addresses it uses. This layout is the
//! Open Packaging Conventions (ECMA-376 Part 2). This module reads, edits and writes it, and
//! does not look inside a part beyond those two bookkeeping parts.
//!
//! Most programs start from the `officekit` crate and use this module for part-level work.
//!
//! The module is the layer under the typed parts of officekit: `officekit::parts` loads and stores
//! the typed content of a part through a [`Package`](package::Package), and `officekit` opens whole
//! files with it. Use it directly to list, read, replace, add or remove parts, to add an image, to
//! tell Strict from Transitional, or to write a large package to a sink that cannot seek.
//!
//! - [`package`][]: [`Package`](package::Package), the package held in memory, and
//!   [`OpenOptions`](package::OpenOptions).
//! - [`part_name`][]: [`PartName`](part_name::PartName), a checked part name.
//! - [`content_types`][]: the parsed `[Content_Types].xml`.
//! - [`relationship`][]: relationships, their targets, and the relationship types in both classes'
//!   spelling.
//! - [`image`][]: an image part as bytes, a format and a pixel size.
//! - [`package_writer`][]: [`PackageWriter`](package_writer::PackageWriter), a package written part
//!   by part into a sink that cannot seek.
//! - [`error`][]: [`PackageError`](error::PackageError), the failure of every package operation.
//!
//! A [`Package`](package::Package) is held in memory. Reading and writing a file goes through an
//! `os_shim::System` that the caller passes: `os_shim::real::RealSystem` for the disk,
//! `MemorySystem` for tests. [`Package::open`](package::Package::open) and
//! [`Package::save`](package::Package::save) are async, because only the caller knows what its
//! system's futures wait on; everything else on a package is synchronous.
//! [`ready_now`](crate::ready::ready_now) runs them from synchronous code.
//! [`Package::from_bytes`](package::Package::from_bytes) and
//! [`Package::to_bytes`](package::Package::to_bytes) need no system at all.
//!
//! With the crate's `binary-format` feature, the content types, part names, relationships and
//! open options also derive the `rkyv` archive, which officekit's binary form stores.
//!
//! # Build a package, save it, open it again
//!
//! ```
//! use std::path::Path;
//!
//! use officekit_xml::packaging::package::Package;
//! use officekit_xml::packaging::part_name::PartName;
//! use officekit_xml::packaging::relationship::RelationshipType;
//! use officekit_xml::conformance::ConformanceClass;
//! use officekit_xml::ready::ready_now;
//! use os_shim::mock::MemorySystem;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let content_type =
//!     "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
//! let main = PartName::new("/word/document.xml")?;
//! let mut package = Package::new();
//! package.set_part_bytes(&main, content_type, b"<w:document/>")?;
//! let id = package.relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)?;
//! assert_eq!(id, "rId1");
//!
//! let system = MemorySystem::new().with_dir("/out")?;
//! ready_now(package.save(&system, Path::new("/out/letter.docx")))?;
//!
//! let reopened = ready_now(Package::open(&system, Path::new("/out/letter.docx")))?;
//! assert_eq!(reopened.part_names()?, [main.clone()]);
//! assert_eq!(reopened.content_type(&main), Some(content_type));
//! let main_part = reopened.main_part()?;
//! assert_eq!(main_part.name, main);
//! assert_eq!(main_part.class, ConformanceClass::Transitional);
//! # Ok(())
//! # }
//! ```
//!
//! # Add an image and change the class
//!
//! ```
//! use officekit_xml::packaging::image::Image;
//! use officekit_xml::packaging::package::Package;
//! use officekit_xml::packaging::part_name::PartName;
//! use officekit_xml::packaging::relationship::RelationshipType;
//! use officekit_xml::conformance::ConformanceClass;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let main = PartName::new("/word/document.xml")?;
//! let mut package = Package::new();
//! package.set_part_bytes(&main, "application/xml", b"<w:document/>")?;
//! package.relate(None, &main, RelationshipType::OFFICE_DOCUMENT.transitional)?;
//!
//! let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
//! png.extend([0, 0, 1, 0x2C, 0, 0, 0, 100, 8, 6, 0, 0, 0]);
//! let image = Image::from_bytes(png)?;
//! assert_eq!(image.pixel_size().map(|size| (size.width, size.height)), Some((300, 100)));
//!
//! let (name, id) = package.add_image(&main, &image)?;
//! assert_eq!(name.as_str(), "/word/media/image1.png");
//! assert_eq!(id, "rId1");
//!
//! package.set_relationship_class(ConformanceClass::Strict)?;
//! assert_eq!(package.main_part()?.class, ConformanceClass::Strict);
//!
//! let again = Package::from_bytes(package.to_bytes()?)?;
//! assert_eq!(again.part_bytes(&name)?, image.bytes());
//! # Ok(())
//! # }
//! ```

extern crate alloc;

pub mod content_types;
pub mod error;
pub mod image;
pub mod package;
pub mod package_writer;
mod part_allocation;
pub mod part_name;
pub mod relationship;
mod relationship_type;
