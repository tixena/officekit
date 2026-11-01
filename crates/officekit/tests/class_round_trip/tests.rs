use officekit::docx::Docx;
use officekit::parts::font_table;
use officekit::parts::kinds::WordprocessingKind;
use officekit_markup::wordprocessing;
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use officekit_xml::parse::parse;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// One value of each kind of row a Word file holds, as Word writes it in Transitional.
const VALUES: [&str; 9] = [
    "<w:jc w:val=\"left\"/>",
    "<w:w w:val=\"89\"/>",
    "<w:tblW w:w=\"4680\" w:type=\"dxa\"/>",
    "<w:tblInd w:w=\"4804\" w:type=\"pct\"/>",
    "<w:tblBorders><w:left w:val=\"single\"/></w:tblBorders>",
    "w:val=\"04A0\"",
    "<w:ind w:left=\"720\" w:hanging=\"360\"/>",
    "<w:pgSz w:w=\"12240\" w:h=\"15840\"/>",
    "<w:charset w:val=\"00\"/>",
];

/// The text of the part `name` of the file `bytes`.
fn part_text(bytes: Vec<u8>, name: &str) -> String {
    let package = Package::from_bytes(bytes).unwrap();
    String::from_utf8(package.part_bytes(&PartName::new(name).unwrap()).unwrap()).unwrap()
}

#[test]
fn a_save_to_strict_and_back_writes_each_value_as_the_first_transitional_file_held_it() {
    let body = concat!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"4680\" w:type=\"dxa\"/><w:tblInd w:w=\"4804\" w:type=\"pct\"/>",
        "<w:tblBorders><w:left w:val=\"single\"/></w:tblBorders>",
        "<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" ",
        "w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/></w:tblPr></w:tbl>",
        "<w:p><w:pPr><w:ind w:left=\"720\" w:hanging=\"360\"/><w:jc w:val=\"left\"/></w:pPr>",
        "<w:r><w:rPr><w:w w:val=\"89\"/></w:rPr><w:t>x</w:t></w:r></w:p>",
        "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>",
    );
    let mut docx = Docx::new(WordprocessingKind::Document).unwrap();
    let text = format!("<w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>");
    let document = wordprocessing::read::document(&parse(text.as_bytes()).unwrap().root);
    docx.set_main_document(&document).unwrap();
    let main = docx.main_document_part().clone();
    docx.package_mut()
        .add_part(
            Some(&main),
            RelationshipType::FONT_TABLE.transitional,
            font_table::CONTENT_TYPE,
            "/word/fontTable{}.xml",
            format!(
                "<w:fonts xmlns:w=\"{W}\"><w:font w:name=\"Calibri\"><w:charset w:val=\"00\"/></w:font></w:fonts>"
            )
            .as_bytes(),
        )
        .unwrap();
    let strict = Docx::from_bytes(docx.to_bytes_strict().unwrap(), OpenOptions::default()).unwrap();
    assert_eq!(strict.conformance_class(), ConformanceClass::Strict);
    let (bytes, warnings) = strict.to_bytes_transitional().unwrap();
    assert_eq!(warnings, []);
    let written = format!(
        "{}{}",
        part_text(bytes.clone(), "/word/document.xml"),
        part_text(bytes, "/word/fontTable1.xml")
    );
    for value in VALUES {
        assert!(written.contains(value), "{value} in {written}");
    }
}
