//! The sections of a sheet's header and footer (ECMA-376 Part 1, section 18.3.1.46).
//!
//! ```
//! use officekit::xlsx::header_footer::{Content, HeaderFooter, sections};
//! use officekit_markup::spreadsheet::{HeaderFooterSettings, OddHeader};
//!
//! let found = sections(r#"&L&A&C&"Arial,Bold"&14Orders&R&D"#);
//! assert_eq!(found.left[0].content, Content::SheetName);
//! assert_eq!(found.center[0].content, Content::Text("Orders".to_owned()));
//! let format = &found.center[0].format;
//! assert_eq!((format.font.as_deref(), format.style.bold(), format.size), (Some("Arial"), true, Some(14)));
//! assert_eq!(found.right[0].content, Content::Date);
//!
//! let mut header = OddHeader::default();
//! header.value = "&CPage &P of &N".to_owned();
//! let mut settings = HeaderFooterSettings::default();
//! settings.odd_header = Some(Box::new(header));
//! let page = HeaderFooter::for_page(&settings, true, false);
//! assert_eq!(page, HeaderFooter::new(Some("&CPage &P of &N"), None));
//! ```

#[cfg(test)]
mod tests;

use core::mem::take;

use officekit_markup::spreadsheet::HeaderFooterSettings;
use officekit_xml::values::parse::boolean;

/// What a piece of a header or a footer shows.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Content {
    /// The current date, `&D`.
    Date,
    /// The workbook's file name, `&F`.
    FileName,
    /// The workbook's file path, `&Z`.
    FilePath,
    /// The page's number plus this, `&P` with its optional `+n` or `-n`.
    PageNumber(i32),
    /// The count of pages, `&N`.
    Pages,
    /// A drawing object, `&G`.
    Picture,
    /// The sheet's name, `&A`.
    SheetName,
    Text(String),
    /// The current time, `&T`.
    Time,
}

/// The format in force where a piece stands.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Format {
    pub color: Option<HeaderColor>,
    /// The font's name; `None` when no code names one.
    pub font: Option<String>,
    pub outline: bool,
    pub script: Script,
    pub shadow: bool,
    /// The size in points.
    pub size: Option<u32>,
    pub strike: bool,
    pub style: FontStyle,
    pub underline: Underline,
}

/// Bold and italic, as a font type of `&"name,type"` writes them and `&B` and `&I` switch them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum FontStyle {
    Bold,
    BoldItalic,
    Italic,
    #[default]
    Regular,
}

impl FontStyle {
    /// Whether the style is bold.
    #[must_use = "this only returns the value"]
    pub const fn bold(self) -> bool {
        matches!(self, Self::Bold | Self::BoldItalic)
    }

    /// Whether the style is italic.
    #[must_use = "this only returns the value"]
    pub const fn italic(self) -> bool {
        matches!(self, Self::BoldItalic | Self::Italic)
    }

    /// The style that is `bold` and `italic`.
    #[must_use = "this only builds the value"]
    pub const fn new(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => Self::Regular,
            (false, true) => Self::Italic,
            (true, false) => Self::Bold,
            (true, true) => Self::BoldItalic,
        }
    }
}

/// The colour a `&K` code sets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum HeaderColor {
    /// Red, green and blue, from `RRGGBB`.
    Rgb([u8; 3]),
    /// A theme colour's index, and its tint: the three digits with their sign, as the file writes
    /// them in `TTSNNN`.
    Theme { index: u8, tint: i16 },
}

/// The header and the footer of one page.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct HeaderFooter<'sheet> {
    pub footer: Option<&'sheet str>,
    pub header: Option<&'sheet str>,
}

impl<'sheet> HeaderFooter<'sheet> {
    /// The texts a page takes (section 18.3.1.46): the first page's when `differentFirst` is on
    /// and `first`, the even ones when `differentOddEven` is on and `even`, else the odd ones.
    #[must_use = "this only returns the value"]
    pub fn for_page(settings: &'sheet HeaderFooterSettings, first: bool, even: bool) -> Self {
        let on =
            |value: Option<&String>| value.map(String::as_str).and_then(boolean).unwrap_or(false);
        if first && on(settings.different_first.as_ref()) {
            Self::new(
                settings
                    .first_page_header
                    .as_ref()
                    .map(|text| text.value.as_str()),
                settings
                    .first_page_footer
                    .as_ref()
                    .map(|text| text.value.as_str()),
            )
        } else if even && on(settings.different_odd_even.as_ref()) {
            Self::new(
                settings
                    .even_page_header
                    .as_ref()
                    .map(|text| text.value.as_str()),
                settings
                    .even_page_footer
                    .as_ref()
                    .map(|text| text.value.as_str()),
            )
        } else {
            Self::new(
                settings.odd_header.as_ref().map(|text| text.value.as_str()),
                settings
                    .odd_page_footer
                    .as_ref()
                    .map(|text| text.value.as_str()),
            )
        }
    }

    /// A page's `header` and `footer`.
    #[must_use = "this only builds the value"]
    pub const fn new(header: Option<&'sheet str>, footer: Option<&'sheet str>) -> Self {
        Self { footer, header }
    }
}

/// One piece of a section: what it shows and the format it shows it in.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Piece {
    pub content: Content,
    pub format: Format,
}

impl Piece {
    /// A piece that shows `content` in `format`.
    #[must_use = "this only builds the value"]
    pub const fn new(content: Content, format: Format) -> Self {
        Self { content, format }
    }
}

/// The three sections of a header or of a footer.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Sections {
    pub center: Vec<Piece>,
    pub left: Vec<Piece>,
    pub right: Vec<Piece>,
}

/// Superscript or subscript, which `&X` and `&Y` switch and which are never both on.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum Script {
    #[default]
    Baseline,
    Subscript,
    Superscript,
}

/// The underline in force.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum Underline {
    Double,
    #[default]
    None,
    Single,
}

/// Which section a piece goes to; text before any section code is in the centre.
#[derive(Clone, Copy, Default)]
enum Section {
    #[default]
    Center,
    Left,
    Right,
}

/// The pieces read so far, the section and the format in force, and the text not yet a piece.
#[derive(Default)]
struct Reader {
    format: Format,
    section: Section,
    sections: Sections,
    text: String,
}

impl Reader {
    /// Apply `change` to the format of what follows.
    fn change(&mut self, change: impl FnOnce(&mut Format)) {
        self.end_text();
        change(&mut self.format);
    }

    /// End the text gathered so far, joined to the piece before it when that is text in the same
    /// format.
    fn end_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = take(&mut self.text);
        let pieces = self.sections.of(self.section);
        if let Some(Piece {
            content: Content::Text(before),
            format,
        }) = pieces.last_mut()
            && *format == self.format
        {
            before.push_str(&text);
        } else {
            pieces.push(Piece::new(Content::Text(text), self.format.clone()));
        }
    }

    fn field(&mut self, content: Content) {
        self.end_text();
        let piece = Piece::new(content, self.format.clone());
        self.sections.of(self.section).push(piece);
    }

    /// Start a section specifier: the format goes back to its start.
    fn start(&mut self, section: Section) {
        self.end_text();
        self.section = section;
        self.format = Format::default();
    }
}

impl Sections {
    /// The pieces of `section`.
    const fn of(&mut self, section: Section) -> &mut Vec<Piece> {
        match section {
            Section::Center => &mut self.center,
            Section::Left => &mut self.left,
            Section::Right => &mut self.right,
        }
    }
}

/// The sections of `text`, a header's or a footer's (section 18.3.1.46).
///
/// Text before any of `&L`, `&C` and `&R` is in the centre section, and a section named more than
/// once is joined in order. A code the section does not list is dropped, its text kept.
#[must_use = "this only returns the value"]
pub fn sections(text: &str) -> Sections {
    let mut reader = Reader::default();
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        reader.text.push_str(&rest[..at]);
        let mut after = rest[at + 1..].chars();
        let Some(code) = after.next() else {
            rest = "";
            break;
        };
        rest = after.as_str();
        match code {
            '&' => reader.text.push('&'),
            'L' => reader.start(Section::Left),
            'C' => reader.start(Section::Center),
            'R' => reader.start(Section::Right),
            'P' => {
                let (offset, left) = page_offset(rest);
                reader.field(Content::PageNumber(offset));
                rest = left;
            }
            'N' => reader.field(Content::Pages),
            'A' => reader.field(Content::SheetName),
            'D' => reader.field(Content::Date),
            'T' => reader.field(Content::Time),
            'F' => reader.field(Content::FileName),
            'Z' => reader.field(Content::FilePath),
            'G' => reader.field(Content::Picture),
            'B' => reader.change(|format| {
                format.style = FontStyle::new(!format.style.bold(), format.style.italic());
            }),
            'I' => reader.change(|format| {
                format.style = FontStyle::new(format.style.bold(), !format.style.italic());
            }),
            'S' => reader.change(|format| format.strike = !format.strike),
            'H' => reader.change(|format| format.shadow = !format.shadow),
            'O' => reader.change(|format| format.outline = !format.outline),
            'U' => reader.change(|format| {
                format.underline = match format.underline {
                    Underline::None => Underline::Single,
                    Underline::Double | Underline::Single => Underline::None,
                };
            }),
            'E' => reader.change(|format| {
                format.underline = match format.underline {
                    Underline::Double => Underline::None,
                    Underline::None | Underline::Single => Underline::Double,
                };
            }),
            'X' => reader.change(|format| {
                format.script = match format.script {
                    Script::Superscript => Script::Baseline,
                    Script::Baseline | Script::Subscript => Script::Superscript,
                };
            }),
            'Y' => reader.change(|format| {
                format.script = match format.script {
                    Script::Subscript => Script::Baseline,
                    Script::Baseline | Script::Superscript => Script::Subscript,
                };
            }),
            'K' => {
                if let Some((color, left)) = color(rest) {
                    reader.change(|format| format.color = Some(color));
                    rest = left;
                }
            }
            '"' => {
                if let Some((font, left)) = rest.split_once('"') {
                    reader.change(|format| set_font(format, font));
                    rest = left;
                }
            }
            digit if digit.is_ascii_digit() => {
                let end = rest
                    .find(|character: char| !character.is_ascii_digit())
                    .unwrap_or(rest.len());
                let points = format!("{digit}{}", &rest[..end]).parse().ok();
                rest = &rest[end..];
                if points.is_some() {
                    reader.change(|format| format.size = points);
                }
            }
            _ => {}
        }
    }
    reader.text.push_str(rest);
    reader.end_text();
    reader.sections
}

/// The colour of `&K` and the text after it: `RRGGBB` in hexadecimal, or `TTSNNN`, a theme
/// colour's two digits, a sign and a tint's three digits.
fn color(text: &str) -> Option<(HeaderColor, &str)> {
    let code = text.get(..6)?;
    let rest = &text[6..];
    let (index, tint) = code.split_at_checked(2)?;
    if tint.starts_with(['+', '-']) {
        let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if !digits(index) || !digits(&tint[1..]) {
            return None;
        }
        let theme = HeaderColor::Theme {
            index: index.parse().ok()?,
            tint: tint.parse().ok()?,
        };
        return Some((theme, rest));
    }
    if !code.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&code[at..at + 2], 16).ok();
    Some((
        HeaderColor::Rgb([channel(0)?, channel(2)?, channel(4)?]),
        rest,
    ))
}

/// The number `&P` adds, `+n` or `-n` at the start of `text`, and the text after it; 0 and
/// `text` when it starts with neither.
fn page_offset(text: &str) -> (i32, &str) {
    let Some(unsigned) = text.strip_prefix(['+', '-']) else {
        return (0, text);
    };
    let end = unsigned
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(unsigned.len());
    if end == 0 {
        return (0, text);
    }
    let length = text.len() - unsigned.len() + end;
    text[..length]
        .parse()
        .map_or((0, text), |offset| (offset, &text[length..]))
}

/// Set the font of `&"name,type"`: the name unless it is `-`, and bold and italic from a type of
/// `Regular`, `Bold`, `Italic` or `Bold Italic`, read in English.
fn set_font(format: &mut Format, font: &str) {
    let (name, kind) = font.split_once(',').unwrap_or((font, ""));
    if name != "-" {
        format.font = Some(name.to_owned());
    }
    let styles = [
        ("Regular", FontStyle::Regular),
        ("Bold", FontStyle::Bold),
        ("Italic", FontStyle::Italic),
        ("Bold Italic", FontStyle::BoldItalic),
    ];
    if let Some(&(_, style)) = styles
        .iter()
        .find(|(written, _)| written.eq_ignore_ascii_case(kind))
    {
        format.style = style;
    }
}
