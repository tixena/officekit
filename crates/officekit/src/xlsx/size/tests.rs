use officekit_markup::common_simple_types::PositiveUniversalMeasurement;
use officekit_markup::spreadsheet::PageSetupSettings;

use super::{
    PaperSize, column_characters, column_pixels, column_width, page_setup_size, paper_size,
};

/// A paper `width` by `height` inches, in EMU.
const fn inches(width: i64, height: i64) -> PaperSize {
    PaperSize::new(width * 914_400 / 100, height * 914_400 / 100)
}

/// A paper `width` by `height` millimetres, in EMU.
const fn millimetres(width: i64, height: i64) -> PaperSize {
    PaperSize::new(width * 36_000, height * 36_000)
}

fn setup(code: Option<&str>, width: Option<&str>, height: Option<&str>) -> PageSetupSettings {
    let measure = |text: &str| PositiveUniversalMeasurement::from(text.to_owned());
    let mut settings = PageSetupSettings::default();
    settings.paper_size = code.map(str::to_owned);
    settings.paper_width = width.map(measure);
    settings.paper_height = height.map(measure);
    settings
}

#[test]
fn the_standard_s_example_is_61_pixels() {
    assert_eq!(column_pixels("8.7109375", 7), Some(61));
}

#[test]
fn widths_files_hold_give_their_pixels() {
    let pixels: Vec<Option<u32>> = ["12", "20", "10.453125", "40.85", "9.36328125"]
        .into_iter()
        .map(|width| column_pixels(width, 7))
        .collect();
    assert_eq!(pixels, [Some(84), Some(140), Some(73), Some(286), Some(66)]);
}

#[test]
fn another_digit_gives_another_width() {
    assert_eq!(column_pixels("8.7109375", 8), Some(70));
}

#[test]
fn the_other_two_formulas_give_the_standard_s_examples() {
    assert_eq!(column_width(8, 7).as_deref(), Some("8.7109375"));
    assert_eq!(column_characters(61, 7).as_deref(), Some("8"));
    assert_eq!(column_characters(60, 7).as_deref(), Some("7.86"));
}

#[test]
fn no_number_a_negative_width_and_a_digit_of_0_give_nothing() {
    assert_eq!(column_pixels("abc", 7), None);
    assert_eq!(column_pixels("-1", 7), None);
    assert_eq!(column_pixels("", 7), None);
    assert_eq!(column_pixels("8.7109375", 0), None);
    assert_eq!(column_width(8, 0), None);
    assert_eq!(column_characters(61, 0), None);
    assert_eq!(column_characters(0, 7), None);
}

#[test]
fn papers_in_inches_are_their_size_in_emu() {
    assert_eq!(paper_size(1), Some(inches(850, 1100)));
    assert_eq!(paper_size(5), Some(inches(850, 1400)));
    assert_eq!(paper_size(3), Some(inches(1100, 1700)));
}

#[test]
fn papers_in_millimetres_are_their_size_in_emu() {
    assert_eq!(paper_size(9), Some(millimetres(210, 297)));
    assert_eq!(paper_size(8), Some(millimetres(297, 420)));
    assert_eq!(paper_size(70), Some(millimetres(105, 148)));
}

#[test]
fn a_code_with_no_size_and_a_code_not_listed_give_nothing() {
    assert_eq!(paper_size(71), None);
    assert_eq!(paper_size(0), None);
    assert_eq!(paper_size(9999), None);
}

#[test]
fn a_page_setup_gives_its_stated_paper_or_the_paper_of_its_code() {
    assert_eq!(
        page_setup_size(&setup(Some("9"), None, None)),
        Some(millimetres(210, 297))
    );
    assert_eq!(
        page_setup_size(&setup(None, None, None)),
        Some(inches(850, 1100))
    );
    assert_eq!(
        page_setup_size(&setup(Some("9"), Some("100mm"), Some("200mm"))),
        Some(millimetres(100, 200))
    );
    assert_eq!(
        page_setup_size(&setup(Some("9"), Some("100mm"), None)),
        Some(millimetres(210, 297))
    );
    assert_eq!(
        page_setup_size(&setup(None, Some("8.5in"), Some("11in"))),
        Some(inches(850, 1100))
    );
    assert_eq!(page_setup_size(&setup(Some("71"), None, None)), None);
}
