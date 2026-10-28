use officekit::parts::main_document;
use officekit_markup::wordprocessing::Document;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::RelationshipType;
use rkyv::rancor::Error;

const MAIN: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

/// A main document as Word writes it: styled paragraphs and runs, a
/// hyperlink, a bookmark, a field, a table with a table nested in a cell,
/// the section's properties, and an element of a namespace no schema knows.
const DOCUMENT: &str = concat!(
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
    "\r\n",
    r#"<w:document xmlns:wpc="http://schemas.microsoft.com/office/word/2010/wordprocessingCanvas" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:o="urn:schemas-microsoft-com:office:office" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" "#,
    r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" "#,
    r#"xmlns:x="urn:example:extra" mc:Ignorable="w14 w15">"#,
    "<w:body>",
    r#"<w:p w14:paraId="1A2B3C4D" w14:textId="77777777" w:rsidR="00A1B2C3" w:rsidRDefault="00A1B2C3">"#,
    r#"<w:pPr><w:pStyle w:val="Title"/><w:jc w:val="center"/></w:pPr>"#,
    r#"<w:r><w:t>Quarterly report</w:t></w:r></w:p>"#,
    r#"<w:p w14:paraId="2B3C4D5E" w14:textId="77777777">"#,
    r#"<w:pPr><w:spacing w:before="120" w:after="240" w:line="276" w:lineRule="auto"/></w:pPr>"#,
    r#"<w:bookmarkStart w:id="0" w:name="results"/>"#,
    r#"<w:r><w:rPr><w:b/><w:bCs/><w:sz w:val="28"/><w:lang w:val="en-US"/></w:rPr><w:t xml:space="preserve">Revenue grew </w:t></w:r>"#,
    r#"<w:r><w:rPr><w:i/></w:rPr><w:t>12%</w:t></w:r>"#,
    r#"<w:bookmarkEnd w:id="0"/>"#,
    r#"<w:r><w:tab/><w:t xml:space="preserve"> see </w:t></w:r>"#,
    r#"<w:hyperlink r:id="rId5" w:history="1"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>the dashboard</w:t></w:r></w:hyperlink>"#,
    r#"<w:proofErr w:type="spellStart"/><w:r><w:t>Q3</w:t></w:r><w:proofErr w:type="spellEnd"/>"#,
    r#"<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>"#,
    r#"<w:r><w:br w:type="page"/></w:r>"#,
    "<x:note x:kind=\"kept\">unknown content</x:note>",
    "</w:p>",
    r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="0" w:type="auto"/><w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr>"#,
    r#"<w:tblGrid><w:gridCol w:w="4675"/><w:gridCol w:w="4675"/></w:tblGrid>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="4675" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Region</w:t></w:r></w:p></w:tc>"#,
    r#"<w:tc><w:tcPr><w:tcW w:w="4675" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>Revenue</w:t></w:r></w:p></w:tc></w:tr>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="4675" w:type="dxa"/></w:tcPr>"#,
    r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>"#,
    r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>North</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    r#"<w:p/></w:tc>"#,
    r#"<w:tc><w:tcPr><w:tcW w:w="4675" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>1,200</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
    r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Notes</w:t></w:r></w:p>"#,
    r#"<w:sectPr w:rsidR="00A1B2C3"><w:pgSz w:w="12240" w:h="15840"/>"#,
    r#"<w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/>"#,
    r#"<w:cols w:space="720"/><w:docGrid w:linePitch="360"/></w:sectPr>"#,
    "</w:body></w:document>"
);

#[test]
fn a_main_document_round_trips_through_rkyv() {
    let mut package = Package::new();
    let part = PartName::new("/word/document.xml").unwrap();
    package
        .set_part_bytes(&part, MAIN, DOCUMENT.as_bytes())
        .unwrap();
    package
        .relate(None, &part, RelationshipType::OFFICE_DOCUMENT.transitional)
        .unwrap();
    let document = main_document::load(&package, &part, OpenOptions::default()).unwrap();
    let bytes = rkyv::to_bytes::<Error>(&document).unwrap();
    let back = rkyv::from_bytes::<Document, Error>(&bytes).unwrap();
    assert_eq!(back, document);
    eprintln!(
        "main document: {} bytes of XML, {} bytes of archive",
        DOCUMENT.len(),
        bytes.len()
    );
}
