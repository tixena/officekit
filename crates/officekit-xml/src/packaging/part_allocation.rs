//! New part names: a template's `{}` takes the first number from 1 that gives
//! a name no part has, compared case-folded.

use alloc::collections::BTreeSet;

use crate::packaging::error::PackageError;
use crate::packaging::part_name::PartName;

#[cfg(test)]
mod tests;

/// Where a template takes the number.
const PLACEHOLDER: &str = "{}";

/// `template` with its `{}` replaced by `number`.
pub fn fill(template: &str, number: usize) -> String {
    template.replace(PLACEHOLDER, &number.to_string())
}

/// The first free name `template` gives; `taken` holds the folded names of
/// every part.
pub fn first_free(template: &str, taken: &BTreeSet<String>) -> Result<PartName, PackageError> {
    let number = first_free_number(&[template], taken)?;
    PartName::new(&fill(template, number))
}

/// The first number from 1 for which every template gives a name no part has;
/// `taken` holds the folded names of every part.
pub fn first_free_number(
    templates: &[&str],
    taken: &BTreeSet<String>,
) -> Result<usize, PackageError> {
    if let Some(template) = templates
        .iter()
        .find(|template| !template.contains(PLACEHOLDER))
    {
        return Err(PackageError::PartName {
            name: (*template).to_owned(),
            reason: "a part-name template must hold `{}`",
        });
    }
    let mut number = 1;
    // Each number gives other names, and `taken` is finite, so this ends.
    while templates
        .iter()
        .any(|template| taken.contains(&fill(template, number).to_ascii_lowercase()))
    {
        number += 1;
    }
    Ok(number)
}
