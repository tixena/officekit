//! The format in force for a cell of a worksheet.
//!
//! The cell format its index names, with the font, the fill, the border and the number format
//! that entry names (ECMA-376 Part 1, sections 18.3.1.4, 18.3.1.13, 18.3.1.73, 18.8.45 and 18.8.30).
//!
//! ```
//! use officekit::parts::kinds::SpreadsheetKind;
//! use officekit::xlsx::Xlsx;
//! use officekit::xlsx::format::{CellFormats, FontProperties};
//! use officekit_markup::spreadsheet::{Bold, Cell, Row, RunProperties, RunPropertiesContent};
//!
//! # fn main() -> Result<(), Box<dyn core::error::Error>> {
//! let xlsx = Xlsx::new(SpreadsheetKind::Workbook)?;
//! let formats = xlsx.cell_formats()?;
//! let mut cell = Cell::default();
//! cell.s = Some("0".to_owned());
//! let format = formats.format(CellFormats::index(&[], &Row::default(), Some(&cell), 1));
//! assert_eq!(format.number_format, "General");
//! let font = format.font.or(formats.normal_font()).map(FontProperties::from).unwrap_or_default();
//! let mut run = RunProperties::default();
//! run.content.push(RunPropertiesContent::from(Bold::default()));
//! assert_eq!(FontProperties::from(&run).over(&font).bold, Some(true));
//! # Ok(())
//! # }
//! ```

mod builtin;
#[cfg(test)]
mod tests;

use officekit_markup::common_simple_types::VerticalPositioningLocationMember;
use officekit_markup::spreadsheet::{
    Alignment, Border, Cell, ColumnInformation, ColumnWidthFormatting, DataBarColor, Fill, Font,
    FontContent, FontSchemeStylesMember, Row, RunProperties, RunPropertiesContent, StyleSheet,
    UnderlineTypesMember,
};
use officekit_xml::values::parse::boolean;

use builtin::BUILTIN_FORMATS;

/// The properties `$members` state, each a variant of `$content`; `$family` and `$name` are the
/// variants that hold the family and the name. A member written twice takes the last.
macro_rules! font_properties {
    ($members:expr, $content:ident, $family:ident, $name:ident) => {{
        let mut properties = FontProperties::default();
        for member in $members {
            match member {
                $content::Bold(node) => properties.bold = toggle(node.val.as_deref()),
                $content::CharacterSet(node) => properties.character_set = node.val.as_deref(),
                $content::Condense(node) => properties.condense = toggle(node.val.as_deref()),
                $content::DataBarColor(node) => properties.color = Some(node),
                $content::Extend(node) => properties.extend = toggle(node.val.as_deref()),
                $content::$family(node) => {
                    properties.family = node.val.as_ref().map(AsRef::<str>::as_ref);
                }
                $content::$name(node) => properties.name = node.val.as_deref(),
                $content::FontSize(node) => properties.size = node.val.as_deref(),
                $content::Italic(node) => properties.italic = toggle(node.val.as_deref()),
                $content::Outline(node) => properties.outline = toggle(node.val.as_deref()),
                $content::Scheme(node) => {
                    properties.scheme = node.val.as_ref().and_then(|value| value.value().ok());
                }
                $content::Shadow(node) => properties.shadow = toggle(node.val.as_deref()),
                $content::StrikeThrough(node) => properties.strike = toggle(node.val.as_deref()),
                $content::Underline(node) => {
                    properties.underline = node
                        .val
                        .as_ref()
                        .map_or(Some(UnderlineTypesMember::Single), |value| {
                            value.value().ok()
                        });
                }
                $content::VerticalAlignment(node) => {
                    properties.vertical_alignment =
                        node.val.as_ref().and_then(|value| value.value().ok());
                }
                $content::Other(_) | _ => {}
            }
        }
        properties
    }};
}

/// The code of the general number format, built-in id 0 (section 18.8.30).
const GENERAL: &str = "General";

/// What one cell format names, each piece `None` when the styles part has no such entry.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct CellFormat<'styles> {
    /// The entry's own `alignment`.
    pub alignment: Option<&'styles Alignment>,
    /// The border its `borderId` names.
    pub border: Option<&'styles Border>,
    /// The fill its `fillId` names.
    pub fill: Option<&'styles Fill>,
    /// The font its `fontId` names.
    pub font: Option<&'styles Font>,
    /// The code of its number format: the workbook's own, a built-in id's, or `General`.
    pub number_format: &'styles str,
}

/// The cell formats of a workbook's styles part (section 18.8.10), ready to answer for an index.
#[derive(Clone, Debug, Default)]
pub struct CellFormats {
    styles: Option<StyleSheet>,
}

impl CellFormats {
    /// The entry of `cellXfs` at `index`, with the font, fill and border its ids name in `fonts`,
    /// `fills` and `borders` (section 18.8.45); its `apply` attributes and `xfId` are not read.
    #[must_use = "this only returns the value"]
    pub fn format(&self, index: u32) -> CellFormat<'_> {
        let Some((styles, entry)) = self.styles.as_ref().and_then(|styles| {
            let list = &styles.cell_formats.as_deref()?.format;
            Some((styles, at(list, Some(i64::from(index)))?))
        }) else {
            return CellFormat {
                alignment: None,
                border: None,
                fill: None,
                font: None,
                number_format: GENERAL,
            };
        };
        CellFormat {
            alignment: entry.alignment.as_deref(),
            border: styles
                .borders
                .as_deref()
                .and_then(|list| at(&list.border, entry.border_id.as_ref()?.value().ok())),
            fill: styles
                .fills
                .as_deref()
                .and_then(|list| at(&list.fill, entry.fill_id.as_ref()?.value().ok())),
            font: styles
                .fonts
                .as_deref()
                .and_then(|list| at(&list.font, entry.font_id.as_ref()?.value().ok())),
            number_format: entry
                .num_fmt_id
                .as_ref()
                .and_then(|id| self.number_format(id.value().ok()?))
                .unwrap_or(GENERAL),
        }
    }

    /// The cell formats of the styles part's root `styles`, taken as it is.
    pub(super) const fn from_styles(styles: Option<StyleSheet>) -> Self {
        Self { styles }
    }

    /// The index of the cell format in force for a cell (sections 18.3.1.4, 18.3.1.73 and
    /// 18.3.1.13): the cell's `s`, `0` when it states none; with no cell node, the row's `s` when
    /// the row states `customFormat`, else the `style` of the `col` holding `column`, else `0`.
    ///
    /// `column` is counted from 1.
    #[must_use = "this only returns the value"]
    pub fn index(
        columns: &[ColumnInformation],
        row: &Row,
        cell: Option<&Cell>,
        column: u32,
    ) -> u32 {
        if let Some(node) = cell {
            return number(node.s.as_deref()).unwrap_or(0);
        }
        if row.custom_format.as_deref().and_then(boolean) == Some(true) {
            return number(row.s.as_deref()).unwrap_or(0);
        }
        columns
            .iter()
            .flat_map(|information| &information.column_width_formatting)
            .find(|entry| holds(entry, column))
            .and_then(|entry| number(entry.style.as_deref()))
            .unwrap_or(0)
    }

    /// The cell formats of `styles`, the styles part's root; `None` for a workbook with none.
    #[must_use = "this only builds the value"]
    pub fn new(styles: Option<&StyleSheet>) -> Self {
        Self {
            styles: styles.cloned(),
        }
    }

    /// The font of the cell style Normal: the font of the `cellStyleXfs` entry that the
    /// `cellStyle` with `builtinId="0"` names (section 18.8.7), else the first font of `fonts`.
    #[must_use = "this only returns the value"]
    pub fn normal_font(&self) -> Option<&Font> {
        let styles = self.styles.as_ref()?;
        let fonts = &styles.fonts.as_deref()?.font;
        let normal = styles.cell_styles.as_deref().and_then(|list| {
            list.cell_style
                .iter()
                .find(|style| number(style.builtin_id.as_deref()) == Some(0))
        });
        normal
            .and_then(|style| {
                let records = &styles.formatting_records.as_deref()?.format;
                let entry = at(records, style.xf_id.as_ref()?.value().ok())?;
                at(fonts, entry.font_id.as_ref()?.value().ok())
            })
            .or_else(|| fonts.first())
    }

    /// The code of number format `id`: the `formatCode` of the workbook's `numFmt` with that id,
    /// else the code section 18.8.30 gives a built-in id.
    fn number_format(&self, id: i64) -> Option<&str> {
        let own = self
            .styles
            .as_ref()
            .and_then(|sheet| sheet.number_formats.as_deref())
            .and_then(|list| {
                list.number_format.iter().find(|format| {
                    format
                        .num_fmt_id
                        .as_ref()
                        .and_then(|stated| stated.value().ok())
                        == Some(id)
                })
            })
            .and_then(|format| format.format_code.as_deref());
        own.or_else(|| {
            let wanted = u32::try_from(id).ok()?;
            let found = BUILTIN_FORMATS
                .binary_search_by_key(&wanted, |&(builtin, _)| builtin)
                .ok()?;
            BUILTIN_FORMATS.get(found).map(|&(_, code)| code)
        })
    }
}

/// What a font states, from a font of the styles part or from a run of a string (sections 18.8.22
/// and 18.4.7). Each value is `None` when the font does not state it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct FontProperties<'font> {
    /// `b`.
    pub bold: Option<bool>,
    /// `charset`'s value.
    pub character_set: Option<&'font str>,
    /// `color`.
    pub color: Option<&'font DataBarColor>,
    /// `condense`.
    pub condense: Option<bool>,
    /// `extend`.
    pub extend: Option<bool>,
    /// `family`'s value.
    pub family: Option<&'font str>,
    /// `i`.
    pub italic: Option<bool>,
    /// `name`'s value in a font, `rFont`'s in a run.
    pub name: Option<&'font str>,
    /// `outline`.
    pub outline: Option<bool>,
    /// `scheme`'s value.
    pub scheme: Option<FontSchemeStylesMember>,
    /// `shadow`.
    pub shadow: Option<bool>,
    /// `sz`'s value: a decimal number of points, as the file writes it.
    pub size: Option<&'font str>,
    /// `strike`.
    pub strike: Option<bool>,
    /// `u`'s value; `single` when it states none.
    pub underline: Option<UnderlineTypesMember>,
    /// `vertAlign`'s value.
    pub vertical_alignment: Option<VerticalPositioningLocationMember>,
}

impl FontProperties<'_> {
    /// Each value of `self` that is stated, and `lower`'s for the others.
    #[must_use = "this only returns the value"]
    pub fn over(&self, lower: &Self) -> Self {
        Self {
            bold: self.bold.or(lower.bold),
            character_set: self.character_set.or(lower.character_set),
            color: self.color.or(lower.color),
            condense: self.condense.or(lower.condense),
            extend: self.extend.or(lower.extend),
            family: self.family.or(lower.family),
            italic: self.italic.or(lower.italic),
            name: self.name.or(lower.name),
            outline: self.outline.or(lower.outline),
            scheme: self.scheme.or(lower.scheme),
            shadow: self.shadow.or(lower.shadow),
            size: self.size.or(lower.size),
            strike: self.strike.or(lower.strike),
            underline: self.underline.or(lower.underline),
            vertical_alignment: self.vertical_alignment.or(lower.vertical_alignment),
        }
    }
}

impl<'font> From<&'font Font> for FontProperties<'font> {
    fn from(font: &'font Font) -> Self {
        font_properties!(&font.content, FontContent, FontFamily, FontName)
    }
}

impl<'font> From<&'font RunProperties> for FontProperties<'font> {
    fn from(run: &'font RunProperties) -> Self {
        font_properties!(
            &run.content,
            RunPropertiesContent,
            FontFamilyIntProperty,
            FontRFont
        )
    }
}

/// The item of `list` at `id`; `None` for no id and for an id past the list.
fn at<T>(list: &[T], id: Option<i64>) -> Option<&T> {
    list.get(usize::try_from(id?).ok()?)
}

/// Whether the `col` entry `entry` holds `column`, from its `min` to its `max` (section 18.3.1.13).
fn holds(entry: &ColumnWidthFormatting, column: u32) -> bool {
    let first = number(entry.min.as_deref());
    let last = number(entry.max.as_deref());
    first
        .zip(last)
        .is_some_and(|(min, max)| (min..=max).contains(&column))
}

/// The `xsd:unsignedInt` `text` states; `None` when it is absent or no such number.
fn number(text: Option<&str>) -> Option<u32> {
    text?.parse().ok()
}

/// A boolean property's value: `true` with no `val` (section 18.8.2), `None` for a `val` that is
/// no boolean.
fn toggle(val: Option<&str>) -> Option<bool> {
    val.map_or(Some(true), boolean)
}
