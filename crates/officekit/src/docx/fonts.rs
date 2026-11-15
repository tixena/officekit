//! The font family a run's `w:rFonts` names, with the theme's fonts (ECMA-376 Part 1, sections
//! 17.3.2.26, 17.18.96 and 17.15.1.88).

#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::drawing::{ComplexScriptFont, EastAsianFont, Font, LatinFont, Theme};
use officekit_markup::wordprocessing::{RunFonts, ThemeFont, ThemeFontLanguages, ThemeFontMember};

/// One of a theme's two collections of fonts, `a:majorFont` or `a:minorFont` (sections
/// 20.1.4.1.24 and 20.1.4.1.25).
#[derive(Clone, Debug, Default)]
struct Collection {
    complex_script: Option<String>,
    east_asian: Option<String>,
    latin: Option<String>,
    supplemental: Vec<Font>,
}

/// The family each of the four kinds of character of a run uses (section 17.3.2.26).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct RunFontFamilies {
    /// The family of the characters U+0000 to U+007F.
    pub ascii: Option<String>,
    /// The family of complex script characters.
    pub cs: Option<String>,
    /// The family of East Asian characters.
    pub east_asia: Option<String>,
    /// The family of the other characters, High ANSI.
    pub h_ansi: Option<String>,
}

/// The major and the minor fonts of a theme (sections 20.1.4.1.24 and 20.1.4.1.25), for the eight
/// values of `ST_Theme` (section 17.18.96).
#[derive(Clone, Debug, Default)]
pub struct ThemeFonts {
    bidi: Option<String>,
    east_asia: Option<String>,
    major: Collection,
    minor: Collection,
    val: Option<String>,
}

impl Collection {
    /// The typeface of the `a:font` for the language `tag` (sections 17.15.1.88 and 20.1.4.1.16):
    /// the one whose script is the tag itself, else the one whose script is its script subtag.
    fn for_language(&self, tag: &str) -> Option<&str> {
        if let Some(found) = self.for_script(tag) {
            return Some(found);
        }
        self.for_script(script_subtag(tag)?)
    }

    /// The typeface of the first `a:font` whose script is `script`, compared without case, and
    /// whose typeface is not empty.
    fn for_script(&self, script: &str) -> Option<&str> {
        self.supplemental.iter().find_map(|font| {
            font.script
                .as_deref()
                .filter(|found| found.eq_ignore_ascii_case(script))?;
            named(font.typeface.as_deref())
        })
    }

    fn new(
        latin: &LatinFont,
        east_asian: &EastAsianFont,
        complex_script: &ComplexScriptFont,
        supplemental: &[Font],
    ) -> Self {
        Self {
            complex_script: complex_script.typeface.clone(),
            east_asian: east_asian.typeface.clone(),
            latin: latin.typeface.clone(),
            supplemental: supplemental.to_vec(),
        }
    }
}

impl ThemeFonts {
    /// The family each kind of character uses under `fonts`, a run's effective `w:rFonts`: the
    /// theme attribute of a kind when it is there, as section 17.3.2.26 says, else the plain one.
    #[must_use = "this only returns the value"]
    pub fn families(&self, fonts: Option<&RunFonts>) -> RunFontFamilies {
        fonts.map_or_else(RunFontFamilies::default, |found| RunFontFamilies {
            ascii: self.kind(found.ascii_theme.as_ref(), found.ascii.as_deref()),
            cs: self.kind(found.cstheme.as_ref(), found.cs.as_deref()),
            east_asia: self.kind(found.east_asia_theme.as_ref(), found.east_asia.as_deref()),
            h_ansi: self.kind(found.h_ansi_theme.as_ref(), found.h_ansi.as_deref()),
        })
    }

    /// The family the theme gives `value` (sections 17.18.96, 17.15.1.88 and 20.1.4.1.16): the
    /// `a:font` for the region's language, else `a:latin`, `a:ea` or `a:cs`; `None` with no theme
    /// and for a typeface that is empty.
    #[must_use = "this only returns the value"]
    pub fn family(&self, value: ThemeFontMember) -> Option<&str> {
        let (collection, language, own) = match value {
            ThemeFontMember::MajorAscii | ThemeFontMember::MajorHAnsi => {
                (&self.major, &self.val, &self.major.latin)
            }
            ThemeFontMember::MajorBidi => (&self.major, &self.bidi, &self.major.complex_script),
            ThemeFontMember::MajorEastAsia => {
                (&self.major, &self.east_asia, &self.major.east_asian)
            }
            ThemeFontMember::MinorAscii | ThemeFontMember::MinorHAnsi => {
                (&self.minor, &self.val, &self.minor.latin)
            }
            ThemeFontMember::MinorBidi => (&self.minor, &self.bidi, &self.minor.complex_script),
            ThemeFontMember::MinorEastAsia => {
                (&self.minor, &self.east_asia, &self.minor.east_asian)
            }
            _ => return None,
        };
        language
            .as_deref()
            .and_then(|tag| collection.for_language(tag))
            .or_else(|| named(own.as_deref()))
    }

    /// The family of one kind of character: that of its theme attribute when the attribute's text
    /// is a value of `ST_Theme`, else its plain attribute.
    fn kind(&self, theme: Option<&ThemeFont>, plain: Option<&str>) -> Option<String> {
        theme
            .and_then(|value| value.value().ok())
            .map_or(plain, |member| self.family(member))
            .map(str::to_owned)
    }

    /// `theme` is the theme part's root and `languages` the settings part's `w:themeFontLang`
    /// (section 17.15.1.88).
    #[must_use = "this only builds the value"]
    pub fn new(theme: Option<&Theme>, languages: Option<&ThemeFontLanguages>) -> Self {
        let scheme = theme.map(|root| &*root.elements.font_scheme);
        Self {
            bidi: languages.and_then(|found| found.bidi.clone()),
            east_asia: languages.and_then(|found| found.east_asia.clone()),
            major: scheme.map_or_else(Collection::default, |found| {
                let fonts = &found.major_font;
                Collection::new(
                    &fonts.latin_font,
                    &fonts.east_asian_font,
                    &fonts.complex_script_font,
                    &fonts.font,
                )
            }),
            minor: scheme.map_or_else(Collection::default, |found| {
                let fonts = &found.minor_fonts;
                Collection::new(
                    &fonts.latin_font,
                    &fonts.east_asian_font,
                    &fonts.complex_script_font,
                    &fonts.font,
                )
            }),
            val: languages.and_then(|found| found.val.clone()),
        }
    }
}

/// `typeface` when it names a font: an empty one names none.
fn named(typeface: Option<&str>) -> Option<&str> {
    typeface.filter(|name| !name.is_empty())
}

/// The script subtag of the language tag `tag`: a subtag of four letters after the first, as
/// `Hant` in `zh-Hant-TW`.
fn script_subtag(tag: &str) -> Option<&str> {
    tag.split('-')
        .skip(1)
        .find(|subtag| subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()))
}
