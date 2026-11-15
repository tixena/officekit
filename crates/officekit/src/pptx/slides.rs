//! The slides of a presentation in order, each with the parts it uses (ECMA-376 Part 1, sections
//! 13.3.6, 13.3.8, 13.3.9, 13.3.10 and 14.2.7).

#[cfg(test)]
mod tests;

use crate::parts::{slide_layout, slide_master, theme};
use officekit_markup::presentation::Presentation;
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};

/// A slide of a presentation and the parts it is drawn with.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SlideParts {
    /// The slide's layout part (section 13.3.9).
    pub layout: PartName,
    /// The layout's master part (section 13.3.10).
    pub master: PartName,
    /// The slide part (section 13.3.8).
    pub slide: PartName,
    /// The master's theme part; `None` for a master with none, which section 14.2.7 allows
    /// ("zero or one Theme part per ... Slide Master").
    pub theme: Option<PartName>,
}

/// The slides `presentation`, the root of the part `main` of `package`, lists in `p:sldIdLst`
/// (section 19.2.1.34), as [`Pptx::slides`](super::Pptx::slides) gives them.
///
/// # Errors
///
/// [`PackageError::Read`] naming a slide part that relates to no layout part or a layout part
/// that relates to no master part; the errors of [`Package::related`].
pub(super) fn of(
    package: &Package,
    main: &PartName,
    presentation: &Presentation,
) -> Result<Vec<SlideParts>, PackageError> {
    let Some(list) = presentation.list_of_slide_ids.as_deref() else {
        return Ok(Vec::new());
    };
    let related = package.related(Some(main))?;
    list.slide_id
        .iter()
        .filter_map(|entry| {
            let id = entry.r_id.as_deref()?;
            let relationship = related.iter().find(|candidate| candidate.id == id)?;
            let Target::Part(slide) = &relationship.target else {
                return None;
            };
            package.contains(slide).then_some(slide)
        })
        .map(|slide| parts_of(package, slide))
        .collect()
}

/// The first part `source` relates to by `relationship_type` that `package` holds.
fn first_related(
    package: &Package,
    source: &PartName,
    relationship_type: &RelationshipType,
) -> Result<Option<PartName>, PackageError> {
    Ok(package
        .related_by_type(Some(source), relationship_type)?
        .into_iter()
        .find_map(|related| {
            let Target::Part(part) = related.target else {
                return None;
            };
            package.contains(&part).then_some(part)
        }))
}

/// [`PackageError::Read`] naming `part`, for the reason `reason`.
fn missing(part: &PartName, reason: &str) -> PackageError {
    PackageError::Read {
        part: part.clone(),
        reason: reason.to_owned(),
    }
}

/// The slide part `slide` with its layout, the layout's master and the master's theme.
fn parts_of(package: &Package, slide: &PartName) -> Result<SlideParts, PackageError> {
    let layout = first_related(package, slide, &slide_layout::RELATIONSHIP_TYPE)?
        .ok_or_else(|| missing(slide, "the slide relates to no slide layout part"))?;
    let master = first_related(package, &layout, &slide_master::RELATIONSHIP_TYPE)?
        .ok_or_else(|| missing(&layout, "the slide layout relates to no slide master part"))?;
    let theme_part = first_related(package, &master, &theme::RELATIONSHIP_TYPE)?;
    Ok(SlideParts {
        layout,
        master,
        slide: slide.clone(),
        theme: theme_part,
    })
}
