//! The font family a run's `w:rFonts` names, with the theme's fonts (ECMA-376 Part 1, sections
//! 17.3.2.26, 17.18.96 and 17.15.1.88).

mod likely_subtags;
#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::drawing::{ComplexScriptFont, EastAsianFont, Font, LatinFont, Theme};
use officekit_markup::wordprocessing::{RunFonts, ThemeFont, ThemeFontLanguages, ThemeFontMember};

use self::likely_subtags::LIKELY_SUBTAGS;

/// The script a theme lists a language under when it lacks the language's likely script: ISO
/// 15924's Kore is Hangul with Han, and the example themes of sections 20.1.4.1.24 and
/// 20.1.4.1.25 list Korean under Hang.
const SCRIPT_FALLBACKS: [(&str, &str); 1] = [("Kore", "Hang")];

/// One of a theme's two collections of fonts, `a:majorFont` or `a:minorFont` (sections
/// 20.1.4.1.24 and 20.1.4.1.25).
#[derive(Clone, Debug, Default)]
struct Collection {
    complex_script: Option<String>,
    east_asian: Option<String>,
    latin: Option<String>,
    supplemental: Vec<Font>,
}

/// A language tag of `w:themeFontLang` and its script.
#[derive(Clone, Debug)]
struct Language {
    script: Option<String>,
    tag: String,
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
    bidi: Option<Language>,
    east_asia: Option<Language>,
    major: Collection,
    minor: Collection,
    val: Option<Language>,
}

impl Collection {
    /// The typeface of the `a:font` for `language` (sections 17.15.1.88 and 20.1.4.1.16): the one
    /// whose script is the tag itself, else the one whose script is the tag's script, else the one
    /// whose script is that script's fallback.
    fn for_language(&self, language: &Language) -> Option<&str> {
        if let Some(found) = self.for_script(&language.tag) {
            return Some(found);
        }
        let script = language.script.as_deref()?;
        self.for_script(script).or_else(|| {
            SCRIPT_FALLBACKS
                .iter()
                .filter(|(from, _)| from.eq_ignore_ascii_case(script))
                .find_map(|(_, to)| self.for_script(to))
        })
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
            .as_ref()
            .and_then(|found| collection.for_language(found))
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
            bidi: languages.and_then(|found| found.bidi.as_deref().map(Language::new)),
            east_asia: languages.and_then(|found| found.east_asia.as_deref().map(Language::new)),
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
            val: languages.and_then(|found| found.val.as_deref().map(Language::new)),
        }
    }
}

impl Language {
    fn new(tag: &str) -> Self {
        Self {
            script: script_of(tag).map(str::to_owned),
            tag: tag.to_owned(),
        }
    }
}

/// The most likely full id, `language_Script_REGION`, of the CLDR id `id` (Unicode CLDR's likely
/// subtags), as `zh_Hant_TW` for `zh_TW`.
pub(crate) fn likely_subtags(id: &str) -> Option<&'static str> {
    let at = LIKELY_SUBTAGS
        .binary_search_by(|(from, _)| (*from).cmp(id))
        .ok()?;
    LIKELY_SUBTAGS.get(at).map(|(_, to)| *to)
}

/// `typeface` when it names a font: an empty one names none.
fn named(typeface: Option<&str>) -> Option<&str> {
    typeface.filter(|name| !name.is_empty())
}

/// The region subtag of the language tag `tag`, upper case: two letters or three digits after the
/// first subtag and before any singleton, as `TW` in `zh-TW`.
fn region_subtag(tag: &str) -> Option<String> {
    tag.split('-')
        .skip(1)
        .take_while(|subtag| subtag.len() > 1)
        .find(|subtag| {
            (subtag.len() == 2 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()))
                || (subtag.len() == 3 && subtag.bytes().all(|byte| byte.is_ascii_digit()))
        })
        .map(str::to_ascii_uppercase)
}

/// The script of the language tag `tag`: its own script subtag, else the likely script of its
/// language and region, else of its language alone.
fn script_of(tag: &str) -> Option<&str> {
    if let Some(own) = script_subtag(tag) {
        return Some(own);
    }
    let language = tag.split('-').next()?.to_ascii_lowercase();
    let full = region_subtag(tag)
        .and_then(|region| likely_subtags(&format!("{language}_{region}")))
        .or_else(|| likely_subtags(&language))?;
    full.split('_').nth(1)
}

/// The script subtag of the language tag `tag`: a subtag of four letters after the first and
/// before any singleton, as `Hant` in `zh-Hant-TW`.
fn script_subtag(tag: &str) -> Option<&str> {
    tag.split('-')
        .skip(1)
        .take_while(|subtag| subtag.len() > 1)
        .find(|subtag| subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()))
}
