//! The colour a scheme colour's name stands for: the name goes through a mapping to one of the
//! theme's twelve colours (ECMA-376 Part 1, sections 20.1.6.2, 20.1.10.54 and 17.15.1.20).

#[cfg(test)]
mod tests;

use officekit_markup::drawing::{
    ColorContent, ColorScheme, OverrideColorMapping, StSchemeColorValMember,
    ThemeColorReference as ColorSchemeIndex, ThemeColorReferenceMember as ColorSchemeIndexMember,
};
use officekit_markup::presentation::ColorSchemeMap;
use officekit_markup::wordprocessing::{
    ThemeColorMappings, ThemeColorReference, ThemeColorReferenceMember,
};

/// One of the twelve colours of a theme's `a:clrScheme` (section 20.1.6.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ThemeSlot {
    /// The colour written `a:accent1`.
    Accent1,
    /// The colour written `a:accent2`.
    Accent2,
    /// The colour written `a:accent3`.
    Accent3,
    /// The colour written `a:accent4`.
    Accent4,
    /// The colour written `a:accent5`.
    Accent5,
    /// The colour written `a:accent6`.
    Accent6,
    /// The colour written `a:dk1`.
    Dark1,
    /// The colour written `a:dk2`.
    Dark2,
    /// The colour written `a:folHlink`.
    FollowedHyperlink,
    /// The colour written `a:hlink`.
    Hyperlink,
    /// The colour written `a:lt1`.
    Light1,
    /// The colour written `a:lt2`.
    Light2,
}

/// The theme colour each mapped name stands for.
///
/// The default is the mapping of a Word file that states none (section 17.15.1.20): each accent
/// and each hyperlink name its own colour, `bg1` the first light colour, `tx1` the first dark
/// one, `bg2` the second light colour and `tx2` the second dark one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SchemeMap {
    /// The colour of the name `accent1`, written `w:accent1` in a Word file.
    pub accent1: ThemeSlot,
    /// The colour of the name `accent2`, written `w:accent2` in a Word file.
    pub accent2: ThemeSlot,
    /// The colour of the name `accent3`, written `w:accent3` in a Word file.
    pub accent3: ThemeSlot,
    /// The colour of the name `accent4`, written `w:accent4` in a Word file.
    pub accent4: ThemeSlot,
    /// The colour of the name `accent5`, written `w:accent5` in a Word file.
    pub accent5: ThemeSlot,
    /// The colour of the name `accent6`, written `w:accent6` in a Word file.
    pub accent6: ThemeSlot,
    /// The colour of the name `bg1`, written `w:bg1` in a Word file.
    pub bg1: ThemeSlot,
    /// The colour of the name `bg2`, written `w:bg2` in a Word file.
    pub bg2: ThemeSlot,
    /// The colour of the name `folHlink`, written `w:followedHyperlink` in a Word file.
    pub fol_hlink: ThemeSlot,
    /// The colour of the name `hlink`, written `w:hyperlink` in a Word file.
    pub hlink: ThemeSlot,
    /// The colour of the name `tx1`, written `w:t1` in a Word file.
    pub tx1: ThemeSlot,
    /// The colour of the name `tx2`, written `w:t2` in a Word file.
    pub tx2: ThemeSlot,
}

impl Default for SchemeMap {
    fn default() -> Self {
        Self {
            accent1: ThemeSlot::Accent1,
            accent2: ThemeSlot::Accent2,
            accent3: ThemeSlot::Accent3,
            accent4: ThemeSlot::Accent4,
            accent5: ThemeSlot::Accent5,
            accent6: ThemeSlot::Accent6,
            bg1: ThemeSlot::Light1,
            bg2: ThemeSlot::Light2,
            fol_hlink: ThemeSlot::FollowedHyperlink,
            hlink: ThemeSlot::Hyperlink,
            tx1: ThemeSlot::Dark1,
            tx2: ThemeSlot::Dark2,
        }
    }
}

impl SchemeMap {
    /// The mapping the twelve attributes of a `CT_ColorMapping` state, given in the order of this
    /// type's fields. An attribute that is absent, or whose text is no value of
    /// `ST_ColorSchemeIndex` (section 20.1.10.14), keeps the default.
    fn from_color_mapping(attributes: [Option<&ColorSchemeIndex>; 12]) -> Self {
        let defaults = Self::default();
        let stated = |attribute: Option<&ColorSchemeIndex>, omitted: ThemeSlot| {
            attribute
                .and_then(|reference| reference.value().ok())
                .and_then(scheme_index_slot)
                .unwrap_or(omitted)
        };
        let [
            accent1,
            accent2,
            accent3,
            accent4,
            accent5,
            accent6,
            bg1,
            bg2,
            fol_hlink,
            hlink,
            tx1,
            tx2,
        ] = attributes;
        Self {
            accent1: stated(accent1, defaults.accent1),
            accent2: stated(accent2, defaults.accent2),
            accent3: stated(accent3, defaults.accent3),
            accent4: stated(accent4, defaults.accent4),
            accent5: stated(accent5, defaults.accent5),
            accent6: stated(accent6, defaults.accent6),
            bg1: stated(bg1, defaults.bg1),
            bg2: stated(bg2, defaults.bg2),
            fol_hlink: stated(fol_hlink, defaults.fol_hlink),
            hlink: stated(hlink, defaults.hlink),
            tx1: stated(tx1, defaults.tx1),
            tx2: stated(tx2, defaults.tx2),
        }
    }

    /// The mapping a slide master's `p:clrMap` states (ECMA-376 Part 1, section 19.3.1.6). An
    /// attribute that is absent, or whose text is no value of `ST_ColorSchemeIndex`, keeps the
    /// default.
    #[must_use = "this only builds the value"]
    pub fn from_master(map: &ColorSchemeMap) -> Self {
        Self::from_color_mapping([
            map.accent1.as_ref(),
            map.accent2.as_ref(),
            map.accent3.as_ref(),
            map.accent4.as_ref(),
            map.accent5.as_ref(),
            map.accent6.as_ref(),
            map.bg1.as_ref(),
            map.bg2.as_ref(),
            map.fol_hlink.as_ref(),
            map.hlink.as_ref(),
            map.tx1.as_ref(),
            map.tx2.as_ref(),
        ])
    }

    /// The mapping an `a:overrideClrMapping` states (section 20.1.6.8), read as
    /// [`SchemeMap::from_master`] reads a master's.
    #[must_use = "this only builds the value"]
    pub fn from_override(map: &OverrideColorMapping) -> Self {
        Self::from_color_mapping([
            map.accent1.as_ref(),
            map.accent2.as_ref(),
            map.accent3.as_ref(),
            map.accent4.as_ref(),
            map.accent5.as_ref(),
            map.accent6.as_ref(),
            map.bg1.as_ref(),
            map.bg2.as_ref(),
            map.fol_hlink.as_ref(),
            map.hlink.as_ref(),
            map.tx1.as_ref(),
            map.tx2.as_ref(),
        ])
    }

    /// The mapping a Word file's `w:clrSchemeMapping` states (section 17.15.1.20), where the two
    /// text names are written `t1` and `t2`. An attribute that is absent, or whose text is no
    /// value of `ST_WmlColorSchemeIndex`, keeps the default.
    #[must_use = "this only builds the value"]
    pub fn from_word(mapping: &ThemeColorMappings) -> Self {
        let defaults = Self::default();
        let stated = |attribute: Option<&ThemeColorReference>, omitted: ThemeSlot| {
            attribute
                .and_then(|reference| reference.value().ok())
                .and_then(word_slot)
                .unwrap_or(omitted)
        };
        Self {
            accent1: stated(mapping.accent1.as_ref(), defaults.accent1),
            accent2: stated(mapping.accent2.as_ref(), defaults.accent2),
            accent3: stated(mapping.accent3.as_ref(), defaults.accent3),
            accent4: stated(mapping.accent4.as_ref(), defaults.accent4),
            accent5: stated(mapping.accent5.as_ref(), defaults.accent5),
            accent6: stated(mapping.accent6.as_ref(), defaults.accent6),
            bg1: stated(mapping.bg1.as_ref(), defaults.bg1),
            bg2: stated(mapping.bg2.as_ref(), defaults.bg2),
            fol_hlink: stated(mapping.followed_hyperlink.as_ref(), defaults.fol_hlink),
            hlink: stated(mapping.hyperlink.as_ref(), defaults.hlink),
            tx1: stated(mapping.t1.as_ref(), defaults.tx1),
            tx2: stated(mapping.t2.as_ref(), defaults.tx2),
        }
    }

    /// The theme colour `name` stands for (section 20.1.10.54). `dk1`, `lt1`, `dk2` and `lt2`
    /// name a theme colour themselves; the twelve mapped names go through the map; `phClr` names
    /// none.
    #[must_use = "this only returns the value"]
    pub const fn slot(&self, name: StSchemeColorValMember) -> Option<ThemeSlot> {
        match name {
            StSchemeColorValMember::Accent1 => Some(self.accent1),
            StSchemeColorValMember::Accent2 => Some(self.accent2),
            StSchemeColorValMember::Accent3 => Some(self.accent3),
            StSchemeColorValMember::Accent4 => Some(self.accent4),
            StSchemeColorValMember::Accent5 => Some(self.accent5),
            StSchemeColorValMember::Accent6 => Some(self.accent6),
            StSchemeColorValMember::Bg1 => Some(self.bg1),
            StSchemeColorValMember::Bg2 => Some(self.bg2),
            StSchemeColorValMember::Dk1 => Some(ThemeSlot::Dark1),
            StSchemeColorValMember::Dk2 => Some(ThemeSlot::Dark2),
            StSchemeColorValMember::FolHlink => Some(self.fol_hlink),
            StSchemeColorValMember::Hlink => Some(self.hlink),
            StSchemeColorValMember::Lt1 => Some(ThemeSlot::Light1),
            StSchemeColorValMember::Lt2 => Some(ThemeSlot::Light2),
            StSchemeColorValMember::Tx1 => Some(self.tx1),
            StSchemeColorValMember::Tx2 => Some(self.tx2),
            StSchemeColorValMember::PhClr | _ => None,
        }
    }
}

/// A theme's twelve colours, its `a:clrScheme` (section 20.1.6.2), under a mapping; with no
/// theme, no name has a colour.
#[derive(Clone, Debug, Default)]
pub struct ThemeColors {
    map: SchemeMap,
    scheme: Option<ColorScheme>,
}

impl ThemeColors {
    /// The colour element the theme holds for `name`, after the mapping: the content of the
    /// `a:dk1`, `a:lt1`, `a:accent1` and so on that the name stands for. `None` with no theme
    /// and for `phClr`.
    #[must_use = "this only returns the value"]
    pub fn color(&self, name: StSchemeColorValMember) -> Option<&ColorContent> {
        self.slot(self.map.slot(name)?)
    }

    /// The mapping the names go through.
    #[must_use = "this only returns the value"]
    pub const fn map(&self) -> &SchemeMap {
        &self.map
    }

    /// `scheme` is the theme part's `a:clrScheme`, `None` for a file with no theme part.
    #[must_use = "this only builds the value"]
    pub fn new(scheme: Option<&ColorScheme>, map: SchemeMap) -> Self {
        Self {
            map,
            scheme: scheme.cloned(),
        }
    }

    /// The colour element of one of the theme's twelve colours; `None` with no theme.
    #[must_use = "this only returns the value"]
    pub fn slot(&self, slot: ThemeSlot) -> Option<&ColorContent> {
        let scheme = self.scheme.as_ref()?;
        let content: &ColorContent = match slot {
            ThemeSlot::Accent1 => &scheme.accent_1.content,
            ThemeSlot::Accent2 => &scheme.accent_2.content,
            ThemeSlot::Accent3 => &scheme.accent_3.content,
            ThemeSlot::Accent4 => &scheme.accent_4.content,
            ThemeSlot::Accent5 => &scheme.accent_5.content,
            ThemeSlot::Accent6 => &scheme.accent_6.content,
            ThemeSlot::Dark1 => &scheme.dark_1.content,
            ThemeSlot::Dark2 => &scheme.dark_2.content,
            ThemeSlot::FollowedHyperlink => &scheme.followed_hyperlink.content,
            ThemeSlot::Hyperlink => &scheme.hyperlink.content,
            ThemeSlot::Light1 => &scheme.light_1.content,
            ThemeSlot::Light2 => &scheme.light_2.content,
        };
        Some(content)
    }
}

/// The theme colour a value of `ST_ColorSchemeIndex` names (section 20.1.10.14); `None` for a
/// member this code does not know.
const fn scheme_index_slot(member: ColorSchemeIndexMember) -> Option<ThemeSlot> {
    match member {
        ColorSchemeIndexMember::Accent1 => Some(ThemeSlot::Accent1),
        ColorSchemeIndexMember::Accent2 => Some(ThemeSlot::Accent2),
        ColorSchemeIndexMember::Accent3 => Some(ThemeSlot::Accent3),
        ColorSchemeIndexMember::Accent4 => Some(ThemeSlot::Accent4),
        ColorSchemeIndexMember::Accent5 => Some(ThemeSlot::Accent5),
        ColorSchemeIndexMember::Accent6 => Some(ThemeSlot::Accent6),
        ColorSchemeIndexMember::Dk1 => Some(ThemeSlot::Dark1),
        ColorSchemeIndexMember::Dk2 => Some(ThemeSlot::Dark2),
        ColorSchemeIndexMember::FolHlink => Some(ThemeSlot::FollowedHyperlink),
        ColorSchemeIndexMember::Hlink => Some(ThemeSlot::Hyperlink),
        ColorSchemeIndexMember::Lt1 => Some(ThemeSlot::Light1),
        ColorSchemeIndexMember::Lt2 => Some(ThemeSlot::Light2),
        _ => None,
    }
}

/// The theme colour a value of `ST_WmlColorSchemeIndex` names (section 17.18.103); `None` for a
/// member this code does not know.
const fn word_slot(member: ThemeColorReferenceMember) -> Option<ThemeSlot> {
    match member {
        ThemeColorReferenceMember::Accent1 => Some(ThemeSlot::Accent1),
        ThemeColorReferenceMember::Accent2 => Some(ThemeSlot::Accent2),
        ThemeColorReferenceMember::Accent3 => Some(ThemeSlot::Accent3),
        ThemeColorReferenceMember::Accent4 => Some(ThemeSlot::Accent4),
        ThemeColorReferenceMember::Accent5 => Some(ThemeSlot::Accent5),
        ThemeColorReferenceMember::Accent6 => Some(ThemeSlot::Accent6),
        ThemeColorReferenceMember::Dark1 => Some(ThemeSlot::Dark1),
        ThemeColorReferenceMember::Dark2 => Some(ThemeSlot::Dark2),
        ThemeColorReferenceMember::FollowedHyperlink => Some(ThemeSlot::FollowedHyperlink),
        ThemeColorReferenceMember::Hyperlink => Some(ThemeSlot::Hyperlink),
        ThemeColorReferenceMember::Light1 => Some(ThemeSlot::Light1),
        ThemeColorReferenceMember::Light2 => Some(ThemeSlot::Light2),
        _ => None,
    }
}
