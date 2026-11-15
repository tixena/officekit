use officekit_markup::spreadsheet::{
    CellValue, ColorIndexes, Colors, HexUnsignedInteger, NumberFormat as NumFmt, RGBColor,
};

use super::{
    DateSystem, FormatColor, FormatError, FormatKind, NumberFormat, Piece, Shown, general,
};
use crate::drawing::color::Rgba;
use crate::xlsx::color::Palette;

/// The date system a code of numbers is given, which none of them reads.
const ANY: DateSystem = DateSystem::V1900Transitional;

/// The code read, or a failed test.
fn format(code: &str) -> NumberFormat {
    NumberFormat::parse(code).unwrap()
}

/// `number` under `code`, as one text.
fn plain(code: &str, number: &str) -> Option<String> {
    format(code).number(number, ANY).map(|shown| shown.plain())
}

/// `number` under `code`, as its pieces.
fn pieces(code: &str, number: &str) -> Option<Vec<Piece>> {
    format(code).number(number, ANY).map(|shown| shown.pieces)
}

fn text(written: &str) -> Piece {
    Piece::Text(written.to_owned())
}

#[test]
fn the_standards_digit_placeholders() {
    assert_eq!(plain("#.00", "8.9").as_deref(), Some("8.90"));
    assert_eq!(plain("#.##", "8.9").as_deref(), Some("8.9"));
    assert_eq!(plain("####.#", "1234.59").as_deref(), Some("1234.6"));
    assert_eq!(plain("#.000", "8.9").as_deref(), Some("8.900"));
    assert_eq!(plain("0.#", ".631").as_deref(), Some("0.6"));
    assert_eq!(plain("#.0#", "12").as_deref(), Some("12.0"));
    assert_eq!(plain("#.0#", "1234.568").as_deref(), Some("1234.57"));
    assert_eq!(plain("#.00", "0.5").as_deref(), Some(".50"));
    assert_eq!(plain(".00", "12.5").as_deref(), Some("12.50"));
}

#[test]
fn question_marks_align_the_decimal_points() {
    assert_eq!(plain("???.???", "44.398").as_deref(), Some(" 44.398"));
    assert_eq!(plain("???.???", "102.65").as_deref(), Some("102.65 "));
    assert_eq!(plain("???.???", "2.8").as_deref(), Some("  2.8  "));
}

#[test]
fn the_standards_thousands_separator_and_scaling() {
    assert_eq!(plain("#,###", "12000").as_deref(), Some("12,000"));
    assert_eq!(plain("#,", "12000").as_deref(), Some("12"));
    assert_eq!(plain("0.0,,", "12200000").as_deref(), Some("12.2"));
    assert_eq!(plain("#.0,,", "12200000").as_deref(), Some("12.2"));
    assert_eq!(plain("#,##0", "1234567").as_deref(), Some("1,234,567"));
    assert_eq!(plain("#,##0", "0").as_deref(), Some("0"));
    assert_eq!(plain("#,##0,", "1234567").as_deref(), Some("1,235"));
}

#[test]
fn the_standards_percent() {
    assert_eq!(plain("0%", ".08").as_deref(), Some("8%"));
    assert_eq!(plain("0%", "2.8").as_deref(), Some("280%"));
    assert_eq!(plain("0.0%%", "0.5").as_deref(), Some("5000.0%%"));
}

#[test]
fn the_standards_literals() {
    assert_eq!(plain("(000)", "12").as_deref(), Some("(012)"));
    assert_eq!(plain("0\\!", "3").as_deref(), Some("3!"));
    assert_eq!(
        plain("0.00 \"dollars\"", "1.23").as_deref(),
        Some("1.23 dollars")
    );
    assert_eq!(
        plain("000-00-0000", "123456789").as_deref(),
        Some("123-45-6789")
    );
}

#[test]
fn the_standards_surplus_and_shortage() {
    let code = "$0.00\" Surplus\";$-0.00\" Shortage\"";
    assert_eq!(plain(code, "125.74").as_deref(), Some("$125.74 Surplus"));
    assert_eq!(plain(code, "-125.74").as_deref(), Some("$-125.74 Shortage"));
}

#[test]
fn the_standards_conditions_and_colours() {
    let code = format("[Red][<=100];[Blue][>100]");
    let low = code.number("50", ANY).unwrap();
    assert_eq!(
        (low.plain().as_str(), low.color),
        ("50", Some(FormatColor::Red))
    );
    let high = code.number("150", ANY).unwrap();
    assert_eq!(
        (high.plain().as_str(), high.color),
        ("150", Some(FormatColor::Blue))
    );
}

#[test]
fn a_number_no_condition_serves_gives_none() {
    let code = format("[>=100]0;[<0]0");
    assert_eq!(code.number("50", ANY), None);
    assert_eq!(
        code.number("-5", ANY).map(|shown| shown.plain()).as_deref(),
        Some("-5")
    );
    assert_eq!(
        code.number("100", ANY)
            .map(|shown| shown.plain())
            .as_deref(),
        Some("100")
    );
    let fallback = format("[=1]\"one\";0.0");
    assert_eq!(
        fallback
            .number("1", ANY)
            .map(|shown| shown.plain())
            .as_deref(),
        Some("one")
    );
    assert_eq!(
        fallback
            .number("2", ANY)
            .map(|shown| shown.plain())
            .as_deref(),
        Some("2.0")
    );
}

#[test]
fn the_standards_skips() {
    assert_eq!(
        pieces("_(0.0_);(0.0)", "2.3"),
        Some(vec![Piece::Skip('('), text("2.3"), Piece::Skip(')')])
    );
    assert_eq!(plain("_(0.0_);(0.0)", "-4.5").as_deref(), Some("(4.5)"));
}

#[test]
fn the_standards_exponents() {
    assert_eq!(plain("0.00E+00", "12200000").as_deref(), Some("1.22E+07"));
    assert_eq!(plain("#0.0E+0", "12200000").as_deref(), Some("12.2E+6"));
    assert_eq!(plain("##0.0E+0", "12200000").as_deref(), Some("12.2E+6"));
    assert_eq!(plain("##0.0E+0", "1220000").as_deref(), Some("1.2E+6"));
    assert_eq!(plain("0.00E+00", "0.000122").as_deref(), Some("1.22E-04"));
    assert_eq!(plain("0.00E-00", "12200000").as_deref(), Some("1.22E07"));
    assert_eq!(plain("0.00e-00", "0.000122").as_deref(), Some("1.22e-04"));
    assert_eq!(plain("0.00E+00", "9.999").as_deref(), Some("1.00E+01"));
    assert_eq!(plain("0.00E+00", "0").as_deref(), Some("0.00E+00"));
    assert_eq!(plain("0.0E+0", "-12").as_deref(), Some("-1.2E+1"));
    assert_eq!(plain("0E+", "12").as_deref(), Some("12E+"));
}

#[test]
fn the_standards_fractions() {
    assert_eq!(plain("# ???/???", "5.25").as_deref(), Some("5   1/4  "));
    assert_eq!(plain("# ???/???", "5.3").as_deref(), Some("5   3/10 "));
}

#[test]
fn a_fraction_of_a_given_denominator_or_with_no_whole_part() {
    assert_eq!(plain("# ?/8", "1.3").as_deref(), Some("1 2/8"));
    assert_eq!(plain("# ?/4", "2.99").as_deref(), Some("3    "));
    assert_eq!(plain("?/?", "5.25").as_deref(), Some("21/4"));
    assert_eq!(plain("# ?/?", "0.25").as_deref(), Some(" 1/4"));
    assert_eq!(plain("# ?/?", "0").as_deref(), Some("0    "));
    assert_eq!(plain("# ??/??", "3.14159").as_deref(), Some("3 14/99"));
    assert_eq!(plain("# ?/?", "-1.5").as_deref(), Some("-1 1/2"));
    assert_eq!(plain("0/0", "0.5").as_deref(), Some("1/2"));
}

#[test]
fn the_standards_fill() {
    assert_eq!(pieces("0*x", "3"), Some(vec![text("3"), Piece::Fill('x')]));
    assert_eq!(
        pieces("*-0*x", "3"),
        Some(vec![text("3"), Piece::Fill('x')])
    );
}

#[test]
fn the_accounting_code() {
    assert_eq!(
        pieces("_(* #,##0.00_)", "12"),
        Some(vec![
            Piece::Skip('('),
            Piece::Fill(' '),
            text("12.00"),
            Piece::Skip(')')
        ])
    );
}

#[test]
fn a_code_that_cannot_be_read() {
    assert_eq!(
        NumberFormat::parse("\"unclosed"),
        Err(FormatError { at: 0 })
    );
    assert_eq!(NumberFormat::parse("0.0x"), Err(FormatError { at: 3 }));
    assert_eq!(NumberFormat::parse("[Red0"), Err(FormatError { at: 0 }));
    assert_eq!(NumberFormat::parse("0\\"), Err(FormatError { at: 1 }));
    assert_eq!(NumberFormat::parse("0_"), Err(FormatError { at: 1 }));
    assert_eq!(NumberFormat::parse("0*"), Err(FormatError { at: 1 }));
    assert_eq!(NumberFormat::parse("0;0;0;@;0"), Err(FormatError { at: 7 }));
    assert_eq!(NumberFormat::parse("0 5"), Err(FormatError { at: 2 }));
    assert_eq!(
        NumberFormat::parse("\"\u{20ac}\"0x"),
        Err(FormatError { at: 4 })
    );
}

#[test]
fn a_semicolon_that_is_text() {
    let code = format("0\";\"0;-0");
    assert_eq!(code.numbers.len(), 2);
    assert_eq!(
        code.number("12", ANY).map(|shown| shown.plain()).as_deref(),
        Some("1;2")
    );
    assert_eq!(
        code.number("-12", ANY)
            .map(|shown| shown.plain())
            .as_deref(),
        Some("-12")
    );
    assert_eq!(format("[$;-409]0").numbers.len(), 1);
    assert_eq!(format("0\\;0").numbers.len(), 1);
}

#[test]
fn the_kind() {
    let kind = |code: &str| format(code).kind();
    assert_eq!(kind("General"), FormatKind::General);
    assert_eq!(kind("general"), FormatKind::General);
    assert_eq!(kind("0.00"), FormatKind::Number);
    assert_eq!(kind("@"), FormatKind::Text);
    assert_eq!(kind("yyyy-mm-dd"), FormatKind::DateTime);
    assert_eq!(kind("h:mm"), FormatKind::DateTime);
    assert_eq!(kind("[h]:mm:ss"), FormatKind::DateTime);
    assert_eq!(kind("h:mm AM/PM"), FormatKind::DateTime);
    assert_eq!(kind("[Red]General"), FormatKind::Number);
    assert_eq!(kind("0;0;0;@"), FormatKind::Number);
}

#[test]
fn a_currency_in_brackets() {
    assert_eq!(
        plain("[$\u{20ac}-407]#,##0.00", "12").as_deref(),
        Some("\u{20ac}12.00")
    );
    assert_eq!(plain("[$-409]0.0", "12").as_deref(), Some("12.0"));
    assert_eq!(plain("[$USD]0", "12").as_deref(), Some("USD12"));
}

#[test]
fn a_section_of_dates_for_negative_numbers_writes_their_magnitude() {
    assert_eq!(plain("yyyy-mm-dd", "46000").as_deref(), Some("2025-12-09"));
    assert_eq!(plain("0;[h]:mm", "-1").as_deref(), Some("24:00"));
    assert_eq!(plain("0;[h]:mm", "1").as_deref(), Some("1"));
}

#[test]
fn what_is_no_number() {
    for written in ["INF", "-INF", "NaN", "abc", "", "1e400"] {
        assert_eq!(general(written, 11), None, "{written}");
        assert_eq!(plain("0.00", written), None, "{written}");
    }
}

#[test]
fn a_text_section() {
    assert_eq!(
        format("\"gross receipts for \"@").text("June").plain(),
        "gross receipts for June"
    );
    assert_eq!(format("0.00").text("June").plain(), "June");
    assert_eq!(format("0;0;0;").text("June").pieces, Vec::<Piece>::new());
    assert_eq!(
        format("0;0;0;[Blue]_(@*.").text("x"),
        Shown {
            color: Some(FormatColor::Blue),
            pieces: vec![Piece::Skip('('), text("x"), Piece::Fill('.')],
        }
    );
    assert_eq!(format("0;-0;\"zero\";\"text\"").text("x").plain(), "text");
}

#[test]
fn a_half_is_rounded_away_from_zero() {
    assert_eq!(plain("0", "0.5").as_deref(), Some("1"));
    assert_eq!(plain("0", "1.5").as_deref(), Some("2"));
    assert_eq!(plain("0", "2.5").as_deref(), Some("3"));
    assert_eq!(plain("0", "-2.5").as_deref(), Some("-3"));
    assert_eq!(plain("0.00", "1.005").as_deref(), Some("1.01"));
    assert_eq!(plain("0.0", "9.96").as_deref(), Some("10.0"));
}

#[test]
fn a_negative_number_rounded_to_zero() {
    assert_eq!(plain("0.0;(0.0)", "-0.01").as_deref(), Some("(0.0)"));
}

#[test]
fn a_palette_colour() {
    let color = |code: &str| format(code).number("1", ANY).unwrap().color;
    assert_eq!(color("[Color3]0"), Some(FormatColor::Palette(3)));
    assert_eq!(color("[color56]0"), Some(FormatColor::Palette(56)));
    assert_eq!(color("[Color57]0"), None);
}

#[test]
fn the_eight_colours() {
    let names = [
        ("[Black]", FormatColor::Black),
        ("[Blue]", FormatColor::Blue),
        ("[Cyan]", FormatColor::Cyan),
        ("[Green]", FormatColor::Green),
        ("[Magenta]", FormatColor::Magenta),
        ("[Red]", FormatColor::Red),
        ("[White]", FormatColor::White),
        ("[YELLOW]", FormatColor::Yellow),
    ];
    for (bracket, color) in names {
        let shown = format(&format!("{bracket}0")).number("1", ANY).unwrap();
        assert_eq!(shown.color, Some(color), "{bracket}");
    }
}

fn opaque(red: u8, green: u8, blue: u8) -> Rgba {
    Rgba::new(red, green, blue, 100_000)
}

#[test]
fn the_red_green_and_blue_of_the_eight_names() {
    let names = [
        ("[Black]", Some(opaque(0, 0, 0))),
        ("[Blue]", None),
        ("[Cyan]", None),
        ("[Green]", None),
        ("[Magenta]", None),
        ("[Red]", Some(opaque(255, 0, 0))),
        ("[White]", Some(opaque(255, 255, 255))),
        ("[Yellow]", None),
    ];
    for (bracket, rgba) in names {
        let stated = format(&format!("{bracket}0"))
            .number("1", ANY)
            .unwrap()
            .color;
        let found = stated.and_then(|color| color.rgba(&Palette::default()));
        assert_eq!(found, rgba, "{bracket}");
    }
}

#[test]
fn color_1_to_56_are_the_standard_s_entries_8_to_63() {
    let rgba = |code: &str| {
        let named = format(code).number("1", ANY).unwrap().color.unwrap();
        named.rgba(&Palette::default())
    };
    assert_eq!(rgba("[Color1]0"), Some(opaque(0, 0, 0)));
    assert_eq!(rgba("[Color2]0"), Some(opaque(255, 255, 255)));
    assert_eq!(rgba("[Color3]0"), Some(opaque(255, 0, 0)));
    assert_eq!(rgba("[Color56]0"), Some(opaque(0x33, 0x33, 0x33)));
    for outside in [0, 57, 58, u8::MAX] {
        assert_eq!(
            FormatColor::Palette(outside).rgba(&Palette::default()),
            None,
            "{outside}"
        );
    }
}

#[test]
fn a_workbook_s_own_palette_changes_color_n_and_not_a_name() {
    let mut indexes = ColorIndexes::default();
    for at in 0_u8..64 {
        let mut entry = RGBColor::default();
        entry.rgb = Some(HexUnsignedInteger::from(format!("FF{at:02X}0000")));
        indexes.rgb_color.push(entry);
    }
    let mut colors = Colors::default();
    colors.color_indexes = Some(Box::new(indexes));
    let own = Palette::new(Some(&colors));
    assert_eq!(FormatColor::Palette(1).rgba(&own), Some(opaque(8, 0, 0)));
    assert_eq!(FormatColor::Palette(3).rgba(&own), Some(opaque(10, 0, 0)));
    assert_eq!(FormatColor::Palette(56).rgba(&own), Some(opaque(63, 0, 0)));
    assert_eq!(FormatColor::Red.rgba(&own), Some(opaque(255, 0, 0)));
    assert_eq!(FormatColor::Black.rgba(&own), Some(opaque(0, 0, 0)));
    assert_eq!(FormatColor::White.rgba(&own), Some(opaque(255, 255, 255)));
    assert_eq!(FormatColor::Blue.rgba(&own), None);
    let short = {
        let mut few = Colors::default();
        few.color_indexes = Some(Box::default());
        Palette::new(Some(&few))
    };
    assert_eq!(FormatColor::Palette(3).rgba(&short), None);
}

#[test]
fn sections_by_sign() {
    let code = format("0.0;(0.0);\"zero\"");
    let shown = |number: &str| code.number(number, ANY).map(|found| found.plain());
    assert_eq!(shown("1").as_deref(), Some("1.0"));
    assert_eq!(shown("-1").as_deref(), Some("(1.0)"));
    assert_eq!(shown("0").as_deref(), Some("zero"));
    assert_eq!(shown("-0").as_deref(), Some("zero"));
    assert_eq!(plain("\"$\"0", "-5").as_deref(), Some("-$5"));
    assert_eq!(plain("0;;0", "-5").as_deref(), Some(""));
    assert_eq!(plain("\"none\"", "-5").as_deref(), Some("none"));
}

#[test]
fn general_as_a_section() {
    assert_eq!(plain("General", "1234.5678").as_deref(), Some("1234.5678"));
    assert_eq!(plain("General", "-42").as_deref(), Some("-42"));
    assert_eq!(
        plain("\"Total \"General", "3.5").as_deref(),
        Some("Total 3.5")
    );
    assert_eq!(plain("General;-General", "-3").as_deref(), Some("-3"));
    let colored = format("[Red]General").number("123456789012", ANY).unwrap();
    assert_eq!(
        (colored.plain().as_str(), colored.color),
        ("1.23457E+11", Some(FormatColor::Red))
    );
    assert_eq!(plain("@", "12").as_deref(), Some("12"));
}

#[test]
fn a_cells_number_under_a_number_format_element() {
    let mut element = NumFmt::default();
    element.format_code = Some("#,##0.00;[Red]-#,##0.00".to_owned());
    let mut cell = CellValue::default();
    cell.value = "-1234.5".to_owned();
    let code = NumberFormat::parse(element.format_code.as_deref().unwrap_or("General")).unwrap();
    let shown = code.number(&cell.value, ANY).unwrap();
    assert_eq!(
        (shown.plain().as_str(), shown.color),
        ("-1,234.50", Some(FormatColor::Red))
    );
}

#[test]
fn the_sample() {
    let code = format("#,##0.00;[Red]-#,##0.00");
    assert_eq!(
        code.number("1234.5", ANY).map(|shown| shown.plain()),
        Some("1,234.50".to_owned())
    );
    let negative = code.number("-5", ANY).unwrap();
    assert_eq!(
        (negative.plain().as_str(), negative.color),
        ("-5.00", Some(FormatColor::Red))
    );
    assert_eq!(general("3.14159", 11).as_deref(), Some("3.14159"));
    assert_eq!(general("123456789012", 11).as_deref(), Some("1.23457E+11"));
    assert_eq!(general("123456", 4), None);
}
