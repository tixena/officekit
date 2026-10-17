//! What the three file types share: the main part, its class, and its kind on open; the store of a
//! typed root in the file's class; and the bytes of the file in a class.

use officekit_xml::conformance::ConformanceClass;
use officekit_xml::write::WriteOptions;

use crate::error::PackageError;
#[cfg(any(feature = "strict", feature = "transitional"))]
use crate::packaging::package::OpenOptions;
use crate::packaging::package::{PACKAGE_RELATIONSHIPS_PART, Package, PartRoot};
use crate::packaging::part_name::PartName;
use crate::packaging::relationship::{Target, class_of_office_document};
#[cfg(any(feature = "strict", feature = "transitional"))]
use crate::parts;

#[cfg(test)]
mod tests;

/// The part a package's `officeDocument` relationship names, that part's content type, and the
/// class the relationship's type names.
pub struct MainPart {
    pub class: ConformanceClass,
    pub content_type: Option<String>,
    pub name: PartName,
}

/// Whether this build reads and writes `class`: its feature is on.
const fn class_enabled(class: ConformanceClass) -> bool {
    (cfg!(feature = "strict") && matches!(class, ConformanceClass::Strict))
        || (cfg!(feature = "transitional") && matches!(class, ConformanceClass::Transitional))
}

/// The kind among `kinds` whose main part has the main part's content type.
///
/// # Errors
///
/// [`PackageError::ContentType`] naming `expected` when no kind has it: a main part of another
/// file type.
pub fn kind<K>(
    main: &MainPart,
    kinds: &[K],
    content_type: fn(K) -> &'static str,
    expected: &'static str,
) -> Result<K, PackageError>
where
    K: Copy,
{
    kinds
        .iter()
        .copied()
        .find(|kind| Some(content_type(*kind)) == main.content_type.as_deref())
        .ok_or_else(|| PackageError::ContentType {
            expected,
            found: main.content_type.clone(),
            part: main.name.clone(),
        })
}

/// The main part of `package`, by its `officeDocument` relationship in either class's spelling.
///
/// # Errors
///
/// [`PackageError::MissingPart`] naming the package's relationship part when no relationship of
/// that type names a part of the package; [`PackageError::ClassNotEnabled`] when the class's
/// feature is off; the errors of [`Package::related`].
pub fn main_part(package: &Package) -> Result<MainPart, PackageError> {
    let relationships = PartName::new(PACKAGE_RELATIONSHIPS_PART)?;
    let found = package.related(None)?.into_iter().find_map(|related| {
        let class = class_of_office_document(&related.type_uri)?;
        match related.target {
            Target::Part(name) => Some(MainPart {
                class,
                content_type: related.content_type,
                name,
            }),
            Target::External(_) => None,
        }
    });
    let main = found.ok_or(PackageError::MissingPart(relationships))?;
    if !class_enabled(main.class) {
        return Err(PackageError::ClassNotEnabled(main.class));
    }
    Ok(main)
}

/// Store `root` into the part `name` in `class`; the part keeps its content type, or takes
/// `fallback`.
///
/// # Errors
///
/// The errors of [`Package::store`].
pub fn store<R>(
    package: &mut Package,
    name: &PartName,
    root: &R,
    fallback: &str,
    class: ConformanceClass,
    options: WriteOptions,
) -> Result<(), PackageError>
where
    R: PartRoot,
{
    let content_type = package.content_type(name).unwrap_or(fallback).to_owned();
    package.store(name, &content_type, root, class, options)
}

/// The package's bytes in `to`, read in `from`.
///
/// In `from`, the package as it is. In the other class, a copy: `own` is given each part first
/// and answers whether it stored the part itself; every other part goes through the module its
/// content type names, loaded with `open` and stored with `write` in `to`, a part no module knows
/// staying as it is; then every relationship type takes `to`'s spelling.
///
/// # Errors
///
/// The errors of `own`, of [`parts::restore_through_module`] (a [`PackageError::Write`] for the
/// first part holding content `to` has no form for), and of building the archive.
#[cfg(any(feature = "strict", feature = "transitional"))]
pub fn to_bytes_in_class(
    package: &Package,
    from: ConformanceClass,
    to: ConformanceClass,
    open: OpenOptions,
    write: WriteOptions,
    own: &mut dyn FnMut(&mut Package, &PartName) -> Result<bool, PackageError>,
) -> Result<Vec<u8>, PackageError> {
    if from == to {
        return package.to_bytes();
    }
    let mut copy = package.duplicate()?;
    for name in copy.part_names()? {
        if !own(&mut copy, &name)? {
            parts::restore_through_module(&mut copy, &name, to, open, write)?;
        }
    }
    copy.set_relationship_class(to)?;
    copy.to_bytes()
}
