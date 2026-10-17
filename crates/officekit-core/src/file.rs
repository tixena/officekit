//! What the three file types share: the main part, its class, and its kind on open; the store of a
//! typed root in the file's class; and the bytes of the file in a class.

use officekit_packaging::error::PackageError;
#[cfg(any(feature = "strict", feature = "transitional"))]
use officekit_packaging::package::OpenOptions;
use officekit_packaging::package::{MainPart, Package};
use officekit_packaging::part_name::PartName;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::write::WriteOptions;

use crate::part_root::{self, PartRoot};
#[cfg(any(feature = "strict", feature = "transitional"))]
use crate::parts;

#[cfg(test)]
mod tests;

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
/// [`PackageError::ClassNotEnabled`] when the class's feature is off; the errors of
/// [`Package::main_part`].
pub fn main_part(package: &Package) -> Result<MainPart, PackageError> {
    let main = package.main_part()?;
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
/// The errors of [`part_root::store`].
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
    part_root::store(package, name, &content_type, root, class, options)
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
