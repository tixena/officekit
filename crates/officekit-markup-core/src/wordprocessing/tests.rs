use officekit_markup_base::common_simple_types::OnOffValue;
use officekit_markup_base::generic;
use officekit_xml::element::Node;
use officekit_xml::name::OfficeVersion;
use officekit_xml::stream::reader::{PartEvent, PartReader};
use officekit_xml::values::{DateTime, Measure, MeasureUnit, ValueError};
use officekit_xml::write::Scope;

use super::{
    AnchorForImportedExternalContent, BlockContent, ContentBlockContent, HorizontalAlignmentType,
    HorizontalAlignmentTypeMember, MeasurementOrPercentageValue,
    MeasurementOrPercentageValueMember, Paragraph, ParagraphContent,
    StandardDateAndTimeStorageFormat, Text, TextRun, TextRunContent, read, write,
};
use crate::spreadsheet::ReferenceSequence;

#[test]
fn an_enumeration_reads_its_members_and_keeps_text_it_does_not_know() {
    assert_eq!(
        HorizontalAlignmentType("center".to_owned()).value(),
        Ok(HorizontalAlignmentTypeMember::Center)
    );
    let unknown = HorizontalAlignmentType("justifyLow".to_owned());
    assert_eq!(
        unknown.value(),
        Err(ValueError::new("ST_Jc", "justifyLow".to_owned()))
    );
    assert_eq!(unknown.0, "justifyLow");
    assert_eq!(
        HorizontalAlignmentType::from(HorizontalAlignmentTypeMember::ThaiDistribute).0,
        "thaiDistribute"
    );
}

#[test]
fn a_member_names_the_office_version_that_brought_it() {
    assert_eq!(
        HorizontalAlignmentTypeMember::Start.introduced(),
        Some(OfficeVersion::Office2010)
    );
    assert_eq!(HorizontalAlignmentTypeMember::Left.introduced(), None);
}

#[test]
fn on_off_reads_every_spelling_and_writes_the_canonical_one() {
    for (text, value) in [
        ("true", true),
        ("1", true),
        ("on", true),
        ("false", false),
        ("0", false),
        ("off", false),
    ] {
        assert_eq!(OnOffValue::from(text.to_owned()).value(), Ok(value));
    }
    assert_eq!(
        OnOffValue::from("yes".to_owned()).value(),
        Err(ValueError::new("ST_OnOff", "yes".to_owned()))
    );
    assert_eq!(OnOffValue::from(false).0, "false");
}

#[test]
fn a_union_gives_the_first_member_type_that_reads_the_text() {
    let value = |text: &str| MeasurementOrPercentageValue(text.to_owned()).value();
    assert_eq!(
        value("5000"),
        Ok(MeasurementOrPercentageValueMember::PercentageValueWithoutPercentSign(5000))
    );
    assert_eq!(
        value("-12.5%"),
        Ok(MeasurementOrPercentageValueMember::PercentageValueWithSign(
            -12.5
        ))
    );
    assert_eq!(
        value("2.5cm"),
        Ok(MeasurementOrPercentageValueMember::UniversalMeasurement(
            Measure::new(MeasureUnit::Centimeters, 2.5)
        ))
    );
    for refused in ["2.5", "5.%"] {
        assert_eq!(
            value(refused),
            Err(ValueError::new(
                "ST_MeasurementOrPercent",
                refused.to_owned()
            ))
        );
    }
}

#[test]
fn a_list_gives_each_item() {
    assert_eq!(
        ReferenceSequence("A1:B2  D4\tF6:F9".to_owned()).value(),
        Ok(vec![
            "A1:B2".to_owned(),
            "D4".to_owned(),
            "F6:F9".to_owned()
        ])
    );
    assert_eq!(ReferenceSequence(String::new()).value(), Ok(Vec::new()));
}

#[test]
fn a_date_reads_its_zone_and_fraction() {
    let date = |text: &str| StandardDateAndTimeStorageFormat(text.to_owned()).value();
    assert_eq!(
        date("2026-10-15T09:30:05Z"),
        Ok(DateTime::new((2026_i32, 10, 15), (9, 30, 5, 0), Some(0)))
    );
    assert_eq!(
        date("2024-02-29T23:59:59.25-04:30").map(|found| (found.nanosecond, found.offset_minutes)),
        Ok((250_000_000, Some(-270)))
    );
    assert_eq!(
        date("2026-10-15T09:30:00").map(|found| found.offset_minutes),
        Ok(None)
    );
    for refused in ["2025-02-29T00:00:00", "2026-10-15"] {
        assert_eq!(
            date(refused),
            Err(ValueError::new("ST_DateTime", refused.to_owned()))
        );
    }
}

#[test]
fn the_block_enums_convert_and_refuse_an_alt_chunk() {
    let paragraph = ContentBlockContent::Paragraph(Paragraph::default());
    let block = BlockContent::from(paragraph.clone());
    assert_eq!(block, BlockContent::Paragraph(Paragraph::default()));
    assert_eq!(ContentBlockContent::try_from(block), Ok(paragraph));
    let chunk =
        BlockContent::AnchorForImportedExternalContent(AnchorForImportedExternalContent::default());
    assert_eq!(ContentBlockContent::try_from(chunk.clone()), Err(chunk));
}

#[test]
fn a_paragraph_built_from_default_writes_and_reads_back() {
    let text = Text {
        value: "Hola".to_owned(),
        ..Text::default()
    };
    let run = TextRun {
        content: vec![TextRunContent::Text(text)],
        ..TextRun::default()
    };
    let paragraph = Paragraph {
        content: vec![ParagraphContent::TextRun(run)],
        para_id: Some("1A2B3C4D".to_owned()),
        ..Paragraph::default()
    };
    let element = write::paragraph(&paragraph, &mut Scope::default());
    // The writer declares the prefixes its element uses, and the reader keeps
    // an element's own declarations.
    let mut read_back = read::paragraph(&element);
    let declared: Vec<&str> = read_back
        .unknown
        .namespace_declarations
        .iter()
        .map(|declaration| declaration.prefix.as_str())
        .collect();
    assert_eq!(declared, ["w", "w14"]);
    read_back.unknown.namespace_declarations.clear();
    assert_eq!(read_back, paragraph);
}

#[test]
fn an_item_in_its_generic_form_is_its_element_or_its_other_node() {
    let item = BlockContent::Paragraph(Paragraph::default());
    let node = generic(&item);
    let name = if let Node::Element(element) = &node {
        Some((element.name.prefix.as_str(), element.name.local.as_str()))
    } else {
        None
    };
    assert_eq!(name, Some(("w", "p")));
    let other = BlockContent::Other(Node::Comment("kept".to_owned()));
    assert_eq!(generic(&other), Node::Comment("kept".to_owned()));
}

#[test]
fn an_element_a_part_reader_loads_reads_as_a_typed_paragraph() {
    const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let xml = format!(
        "<?xml version=\"1.0\"?><w:document xmlns:w=\"{W}\"><w:body><w:p><w:r><w:t>a &amp; b</w:t></w:r></w:p>\
         <!--c--><w:sectPr/></w:body></w:document>"
    );
    let mut reader = PartReader::new(xml.as_bytes());
    let mut element = None;
    while let Some(event) = reader.read_event().unwrap() {
        if let PartEvent::Start(start) = event
            && start.name.local == "p"
        {
            element = Some(reader.read_element(start).unwrap());
            break;
        }
    }
    let paragraph = read::paragraph(&element.unwrap());
    assert!(matches!(
        paragraph.content.as_slice(),
        [ParagraphContent::TextRun(run)]
            if matches!(run.content.as_slice(), [TextRunContent::Text(text)] if text.value == "a & b")
    ));
}
