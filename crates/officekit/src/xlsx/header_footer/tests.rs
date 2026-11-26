use officekit_markup::spreadsheet::{
    EvenPageFooter, EvenPageHeader, FirstPageFooter, FirstPageHeader, HeaderFooterSettings,
    OddHeader, OddPageFooter,
};

use super::{
    Content, FontStyle, Format, HeaderColor, HeaderFooter, Piece, Script, Sections, Underline,
    sections,
};

fn text(content: &str) -> Content {
    Content::Text(content.to_owned())
}

/// The contents of `pieces`, without their format.
fn contents(pieces: &[Piece]) -> Vec<Content> {
    pieces.iter().map(|piece| piece.content.clone()).collect()
}

/// Each text piece of the centre section of `code`, with its format.
fn texts(code: &str) -> Vec<(String, Format)> {
    sections(code)
        .center
        .into_iter()
        .filter_map(|piece| {
            let Content::Text(found) = piece.content else {
                return None;
            };
            Some((found, piece.format))
        })
        .collect()
}

#[test]
fn the_standard_s_sample_header() {
    let found = sections(r#"&L&A&C&"Arial,Bold"&14Orders&R&D"#);
    assert_eq!(contents(&found.left), [Content::SheetName]);
    assert_eq!(contents(&found.center), [text("Orders")]);
    let format = &found.center[0].format;
    assert_eq!(
        (format.font.as_deref(), format.style.bold(), format.size),
        (Some("Arial"), true, Some(1400))
    );
    assert_eq!(contents(&found.right), [Content::Date]);
    let footer = sections("&CPage &P of &N");
    assert_eq!(
        contents(&footer.center),
        [
            text("Page "),
            Content::PageNumber(0),
            text(" of "),
            Content::Pages
        ]
    );
}

#[test]
fn a_section_named_twice_is_joined() {
    let found = sections("&LA&CD&RG&LB&CE&RH");
    assert_eq!(contents(&found.left), [text("AB")]);
    assert_eq!(contents(&found.center), [text("DE")]);
    assert_eq!(contents(&found.right), [text("GH")]);
}

#[test]
fn text_before_a_section_code_is_in_the_centre() {
    let found = sections("Title&RRight");
    assert_eq!(contents(&found.center), [text("Title")]);
    assert_eq!(contents(&found.right), [text("Right")]);
    assert_eq!(found.left, []);
}

#[test]
fn each_field_is_a_piece() {
    assert_eq!(
        contents(&sections("&P&N&A&D&T&F&Z&G").center),
        [
            Content::PageNumber(0),
            Content::Pages,
            Content::SheetName,
            Content::Date,
            Content::Time,
            Content::FileName,
            Content::FilePath,
            Content::Picture,
        ]
    );
}

#[test]
fn a_number_added_to_the_page() {
    assert_eq!(
        contents(&sections("&P+100").center),
        [Content::PageNumber(100)]
    );
    assert_eq!(
        contents(&sections("&P-5").center),
        [Content::PageNumber(-5)]
    );
    assert_eq!(
        contents(&sections("&P+").center),
        [Content::PageNumber(0), text("+")]
    );
    assert_eq!(
        contents(&sections("&P + 1").center),
        [Content::PageNumber(0), text(" + 1")]
    );
    assert_eq!(
        contents(&sections("&P+99999999999").center),
        [Content::PageNumber(0), text("+99999999999")]
    );
}

#[test]
fn an_ampersand_and_a_code_that_is_none() {
    assert_eq!(
        contents(&sections("Smith && Sons").center),
        [text("Smith & Sons")]
    );
    assert_eq!(contents(&sections("A&").center), [text("A")]);
    assert_eq!(contents(&sections("A&?B").center), [text("AB")]);
    assert_eq!(contents(&sections("A&KZZB").center), [text("AZZB")]);
    assert_eq!(contents(&sections("A&\"Arial").center), [text("AArial")]);
    assert_eq!(sections(""), Sections::default());
}

#[test]
fn bold_is_switched_on_and_off() {
    let found = texts("ab&Bcd&Bef");
    let bold: Vec<(&str, bool)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.style.bold()))
        .collect();
    assert_eq!(bold, [("ab", false), ("cd", true), ("ef", false)]);
}

#[test]
fn the_other_switches() {
    let found = texts("a&Ib&Sc&Hd&Oe&I&S&H&Of");
    let switched: Vec<(&str, bool, bool, bool, bool)> = found
        .iter()
        .map(|(content, format)| {
            (
                content.as_str(),
                format.style.italic(),
                format.strike,
                format.shadow,
                format.outline,
            )
        })
        .collect();
    assert_eq!(
        switched,
        [
            ("a", false, false, false, false),
            ("b", true, false, false, false),
            ("c", true, true, false, false),
            ("d", true, true, true, false),
            ("e", true, true, true, true),
            ("f", false, false, false, false),
        ]
    );
}

#[test]
fn the_underlines() {
    let found = texts("&Uaa&Ebb&Ucc");
    let lines: Vec<(&str, Underline)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.underline))
        .collect();
    assert_eq!(
        lines,
        [
            ("aa", Underline::Single),
            ("bb", Underline::Double),
            ("cc", Underline::None)
        ]
    );
}

#[test]
fn superscript_and_subscript_switch_each_other_off() {
    let found = texts("aa&Xbb&Xcc&Ydd&Yee&Xff&Ygg");
    let scripts: Vec<(&str, Script)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.script))
        .collect();
    assert_eq!(
        scripts,
        [
            ("aa", Script::Baseline),
            ("bb", Script::Superscript),
            ("cc", Script::Baseline),
            ("dd", Script::Subscript),
            ("ee", Script::Baseline),
            ("ff", Script::Superscript),
            ("gg", Script::Subscript),
        ]
    );
}

#[test]
fn a_font_and_its_type() {
    let found = texts(
        r#"&"Arial,Bold"a&"-,Regular"b&"Lucida Sans Typewriter,Italic"c&"-,Bold Italic"d&"Courier,Book"e"#,
    );
    let fonts: Vec<(&str, Option<&str>, FontStyle)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.font.as_deref(), format.style))
        .collect();
    assert_eq!(
        fonts,
        [
            ("a", Some("Arial"), FontStyle::Bold),
            ("b", Some("Arial"), FontStyle::Regular),
            ("c", Some("Lucida Sans Typewriter"), FontStyle::Italic),
            ("d", Some("Lucida Sans Typewriter"), FontStyle::BoldItalic),
            ("e", Some("Courier"), FontStyle::BoldItalic),
        ]
    );
}

#[test]
fn the_first_comma_separates_the_name_from_the_type() {
    let found = texts(r#"&"A,B,Bold"x"#);
    assert_eq!(found[0].1.font.as_deref(), Some("A"));
    assert_eq!(found[0].1.style, FontStyle::Regular);
}

#[test]
fn a_size() {
    let found = texts("&18A&36B");
    let sizes: Vec<(&str, Option<u32>)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.size))
        .collect();
    assert_eq!(sizes, [("A", Some(1800)), ("B", Some(3600))]);
}

#[test]
fn a_size_with_a_fraction() {
    let size = |code: &str| {
        texts(code)
            .into_iter()
            .map(|(content, format)| (content, format.size))
            .collect::<Vec<_>>()
    };
    let one = |content: &str, hundredths: Option<u32>| vec![(content.to_owned(), hundredths)];
    assert_eq!(size("&18A"), one("A", Some(1800)));
    assert_eq!(size("&10.5Text"), one("Text", Some(1050)));
    assert_eq!(size("&10.25Text"), one("Text", Some(1025)));
    assert_eq!(size("&10.125X"), one("X", Some(1013)));
    assert_eq!(size("&10.124X"), one("X", Some(1012)));
    assert_eq!(size("&10.Text"), one(".Text", Some(1000)));
    assert_eq!(size("&99999999999A"), one("A", None));
    assert_eq!(size("&42949673A"), one("A", None));
    assert_eq!(size("&42949672.95A"), one("A", Some(u32::MAX)));
    assert_eq!(size("&42949672.955A"), one("A", None));
}

#[test]
fn a_colour() {
    let found = texts("A&KFF0000B&K01+000C&K07+037D&K03-050E");
    let colors: Vec<(&str, Option<HeaderColor>)> = found
        .iter()
        .map(|(content, format)| (content.as_str(), format.color))
        .collect();
    assert_eq!(
        colors,
        [
            ("A", None),
            ("B", Some(HeaderColor::Rgb([255, 0, 0]))),
            ("C", Some(HeaderColor::Theme { index: 1, tint: 0 })),
            ("D", Some(HeaderColor::Theme { index: 7, tint: 37 })),
            (
                "E",
                Some(HeaderColor::Theme {
                    index: 3,
                    tint: -50
                })
            ),
        ]
    );
}

#[test]
fn a_section_puts_the_format_back() {
    let found = sections("&L&B&I&14&KFF0000&\"Arial,Regular\"&U&Xa&Cb");
    let left = Format {
        color: Some(HeaderColor::Rgb([255, 0, 0])),
        font: Some("Arial".to_owned()),
        script: Script::Superscript,
        size: Some(1400),
        underline: Underline::Single,
        ..Format::default()
    };
    assert_eq!(found.left, [Piece::new(text("a"), left)]);
    assert_eq!(found.center, [Piece::new(text("b"), Format::default())]);
    let bold = sections("&L&Ba&Cb");
    assert_eq!(bold.left[0].format.style, FontStyle::Bold);
    assert_eq!(bold.center[0].format.style, FontStyle::Regular);
}

#[test]
fn a_line_break_stays_in_the_text() {
    assert_eq!(
        contents(&sections("&CCenter\nHeader").center),
        [text("Center\nHeader")]
    );
}

/// A `headerFooter` element with all six texts and the two attributes given.
fn settings(
    different_first: Option<&str>,
    different_odd_even: Option<&str>,
) -> HeaderFooterSettings {
    let mut element = HeaderFooterSettings::default();
    element.different_first = different_first.map(Box::from);
    element.different_odd_even = different_odd_even.map(Box::from);
    let mut odd_header = OddHeader::default();
    odd_header.value = Box::from("odd header");
    element.odd_header = Some(Box::new(odd_header));
    let mut odd_footer = OddPageFooter::default();
    odd_footer.value = Box::from("odd footer");
    element.odd_page_footer = Some(Box::new(odd_footer));
    let mut even_header = EvenPageHeader::default();
    even_header.value = Box::from("even header");
    element.even_page_header = Some(Box::new(even_header));
    let mut even_footer = EvenPageFooter::default();
    even_footer.value = Box::from("even footer");
    element.even_page_footer = Some(Box::new(even_footer));
    let mut first_header = FirstPageHeader::default();
    first_header.value = Box::from("first header");
    element.first_page_header = Some(Box::new(first_header));
    let mut first_footer = FirstPageFooter::default();
    first_footer.value = Box::from("first footer");
    element.first_page_footer = Some(Box::new(first_footer));
    element
}

#[test]
fn which_texts_a_page_takes() {
    let odd = HeaderFooter::new(Some("odd header"), Some("odd footer"));
    let first = HeaderFooter::new(Some("first header"), Some("first footer"));
    let even = HeaderFooter::new(Some("even header"), Some("even footer"));
    let neither = settings(None, None);
    assert_eq!(HeaderFooter::for_page(&neither, true, false), odd);
    assert_eq!(HeaderFooter::for_page(&neither, false, true), odd);
    let first_on = settings(Some("1"), None);
    assert_eq!(HeaderFooter::for_page(&first_on, true, false), first);
    assert_eq!(HeaderFooter::for_page(&first_on, false, true), odd);
    let even_on = settings(Some("false"), Some("true"));
    assert_eq!(HeaderFooter::for_page(&even_on, false, true), even);
    assert_eq!(HeaderFooter::for_page(&even_on, true, false), odd);
    assert_eq!(HeaderFooter::for_page(&even_on, false, false), odd);
}

#[test]
fn a_text_that_is_absent_is_none() {
    let mut element = HeaderFooterSettings::default();
    element.different_first = Some(Box::from("1"));
    assert_eq!(
        HeaderFooter::for_page(&element, true, false),
        HeaderFooter::default()
    );
}
