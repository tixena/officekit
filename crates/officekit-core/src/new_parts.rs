//! The parts a new file of each type starts with: the fewest a Word, Excel, or `PowerPoint` file
//! needs to open, written Transitional.

use officekit_packaging::error::PackageError;
use officekit_packaging::package::{DECLARATION, Package};
use officekit_packaging::part_name::PartName;
use officekit_parts::{
    main_document, presentation, slide_layout, slide_master, theme, workbook, worksheet,
};
use officekit_xml::element::Document;
use officekit_xml::parse::parse;
use officekit_xml::write::WriteOptions;

/// The namespace declarations of every `PresentationML` part a new presentation holds.
macro_rules! presentation_namespaces {
    () => {
        r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#
    };
}

/// The shape tree of a new slide master and layout: its group alone.
macro_rules! empty_shape_tree {
    () => {
        r#"<p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld>"#
    };
}

/// A new workbook: one sheet, `Sheet1`, its relationship `rId1`.
const EXCEL_WORKBOOK: &str = r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#;

/// The empty sheet of a new workbook.
const EXCEL_WORKSHEET: &str = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData/></worksheet>"#;

/// A new presentation: its slide master, relationship `rId1`, a 16:9 slide, and no slides.
const POWERPOINT_PRESENTATION: &str = concat!(
    "<p:presentation ",
    presentation_namespaces!(),
    r#"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#,
);

/// The slide master of a new presentation: its blank layout, relationship `rId1`.
const POWERPOINT_SLIDE_MASTER: &str = concat!(
    "<p:sldMaster ",
    presentation_namespaces!(),
    ">",
    empty_shape_tree!(),
    r#"<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst></p:sldMaster>"#,
);

/// The blank layout of a new presentation.
const POWERPOINT_SLIDE_LAYOUT: &str = concat!(
    "<p:sldLayout ",
    presentation_namespaces!(),
    r#" type="blank">"#,
    empty_shape_tree!(),
    "<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
);

/// The theme of a new presentation, with every required list at its minimum: twelve colors, three
/// fonts per scheme, three of each style.
const THEME: &str = concat!(
    r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements>"#,
    r#"<a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>"#,
    r#"<a:dk2><a:srgbClr val="0E2841"/></a:dk2><a:lt2><a:srgbClr val="E8E8E8"/></a:lt2><a:accent1><a:srgbClr val="156082"/></a:accent1>"#,
    r#"<a:accent2><a:srgbClr val="E97132"/></a:accent2><a:accent3><a:srgbClr val="196B24"/></a:accent3><a:accent4><a:srgbClr val="0F9ED5"/></a:accent4>"#,
    r#"<a:accent5><a:srgbClr val="A02B93"/></a:accent5><a:accent6><a:srgbClr val="4EA72E"/></a:accent6><a:hlink><a:srgbClr val="467886"/></a:hlink>"#,
    r#"<a:folHlink><a:srgbClr val="96607D"/></a:folHlink></a:clrScheme>"#,
    r#"<a:fontScheme name="Office"><a:majorFont><a:latin typeface="Aptos Display"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>"#,
    r#"<a:minorFont><a:latin typeface="Aptos"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>"#,
    r#"<a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#,
    r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>"#,
    r#"<a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="25400"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>"#,
    r#"<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>"#,
    r#"<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#,
    r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"#,
);

/// The main document of a new `.docx`: one empty paragraph.
const WORD_DOCUMENT: &str = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p/></w:body></w:document>"#;

/// A new workbook's package, its main part of `content_type`; returns the package and that part.
///
/// # Errors
///
/// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
pub fn excel(content_type: &str) -> Result<(Package, PartName), PackageError> {
    let mut package = Package::new();
    let main = PartName::new("/xl/workbook.xml")?;
    let sheet = PartName::new("/xl/worksheets/sheet1.xml")?;
    put(&mut package, &main, content_type, EXCEL_WORKBOOK)?;
    put(
        &mut package,
        &sheet,
        worksheet::CONTENT_TYPE,
        EXCEL_WORKSHEET,
    )?;
    package.relate(None, &main, workbook::RELATIONSHIP_TYPE.transitional)?;
    package.relate(
        Some(&main),
        &sheet,
        worksheet::RELATIONSHIP_TYPE.transitional,
    )?;
    Ok((package, main))
}

/// A new presentation's package, its main part of `content_type`: the presentation, one slide
/// master with one blank layout, and the theme the presentation and the master relate to; returns
/// the package and its main part.
///
/// # Errors
///
/// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
pub fn powerpoint(content_type: &str) -> Result<(Package, PartName), PackageError> {
    let mut package = Package::new();
    let main = PartName::new("/ppt/presentation.xml")?;
    let master = PartName::new("/ppt/slideMasters/slideMaster1.xml")?;
    let layout = PartName::new("/ppt/slideLayouts/slideLayout1.xml")?;
    let theme_part = PartName::new("/ppt/theme/theme1.xml")?;
    put(&mut package, &main, content_type, POWERPOINT_PRESENTATION)?;
    put(
        &mut package,
        &master,
        slide_master::CONTENT_TYPE,
        POWERPOINT_SLIDE_MASTER,
    )?;
    put(
        &mut package,
        &layout,
        slide_layout::CONTENT_TYPE,
        POWERPOINT_SLIDE_LAYOUT,
    )?;
    put(&mut package, &theme_part, theme::CONTENT_TYPE, THEME)?;
    package.relate(None, &main, presentation::RELATIONSHIP_TYPE.transitional)?;
    package.relate(
        Some(&main),
        &master,
        slide_master::RELATIONSHIP_TYPE.transitional,
    )?;
    package.relate(
        Some(&main),
        &theme_part,
        theme::RELATIONSHIP_TYPE.transitional,
    )?;
    package.relate(
        Some(&master),
        &layout,
        slide_layout::RELATIONSHIP_TYPE.transitional,
    )?;
    package.relate(
        Some(&master),
        &theme_part,
        theme::RELATIONSHIP_TYPE.transitional,
    )?;
    package.relate(
        Some(&layout),
        &master,
        slide_master::RELATIONSHIP_TYPE.transitional,
    )?;
    Ok((package, main))
}

/// Write `xml` into the part `name` with the declaration of every part the package writes.
fn put(
    package: &mut Package,
    name: &PartName,
    content_type: &str,
    xml: &str,
) -> Result<(), PackageError> {
    let document = parse(xml.as_bytes()).map_err(|error| PackageError::Xml {
        error,
        part: name.clone(),
    })?;
    let bytes = Document::new(DECLARATION, document.root).to_xml(WriteOptions::default());
    package.set_part_bytes(name, content_type, &bytes)
}

/// A new document's package, its main part of `content_type`; returns the package and that part.
///
/// # Errors
///
/// The errors of [`Package::set_part_bytes`] and [`Package::relate`].
pub fn word(content_type: &str) -> Result<(Package, PartName), PackageError> {
    let mut package = Package::new();
    let main = PartName::new("/word/document.xml")?;
    put(&mut package, &main, content_type, WORD_DOCUMENT)?;
    package.relate(None, &main, main_document::RELATIONSHIP_TYPE.transitional)?;
    Ok((package, main))
}
