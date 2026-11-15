//! The colour map in force for a slide (ECMA-376 Part 1, section 19.3.1.7 and annex L.3.2.5).

#[cfg(test)]
mod tests;

use crate::drawing::scheme::{SchemeMap, ThemeColors};
use officekit_markup::drawing::{ColorSchemeMapOverrideContent, Theme};
use officekit_markup::presentation::{
    ColorSchemeMapOverride, PresentationSlideSlide, SlideLayout, SlideMaster,
};

/// The colour map in force for `slide` (section 19.3.1.7 and annex L.3.2.5).
///
/// Its own `a:overrideClrMapping`, else its layout's, else the master's `p:clrMap`.
#[must_use = "this only returns the value"]
pub fn in_force(
    slide: &PresentationSlideSlide,
    layout: &SlideLayout,
    master: &SlideMaster,
) -> SchemeMap {
    override_of(slide.color_scheme_map_override.as_deref())
        .or_else(|| override_of(layout.color_scheme_map_override.as_deref()))
        .unwrap_or_else(|| SchemeMap::from_master(&master.color_scheme_map))
}

/// The mapping a `p:clrMapOvr` states when it holds an `a:overrideClrMapping` (section 20.1.6.8).
fn override_of(map: Option<&ColorSchemeMapOverride>) -> Option<SchemeMap> {
    let ColorSchemeMapOverrideContent::OverrideColorMapping(mapping) = &*map?.content else {
        return None;
    };
    Some(SchemeMap::from_override(mapping))
}

/// The colours of `theme` under the map in force for `slide` (section 19.3.1.7).
///
/// With no theme, which section 14.2.7 allows a master, no name has a colour.
#[must_use = "this only returns the value"]
pub fn theme_colors(
    slide: &PresentationSlideSlide,
    layout: &SlideLayout,
    master: &SlideMaster,
    theme: Option<&Theme>,
) -> ThemeColors {
    ThemeColors::new(
        theme.map(|root| &*root.elements.color_scheme),
        in_force(slide, layout, master),
    )
}
