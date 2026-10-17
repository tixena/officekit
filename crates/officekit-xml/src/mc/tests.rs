use super::MarkupCompatibility;
use crate::element::XmlError;
use crate::name::OfficeVersion;
use crate::parse::parse;
use crate::write::{Layout, WriteOptions};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const W16SE: &str = r#"xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex""#;
const WPS: &str =
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
const OTHER: &str = r#"xmlns:x="urn:not-in-the-table""#;

const KEEP: WriteOptions = WriteOptions {
    layout: Layout::Compact,
    namespace_cleanup: false,
};

/// A symbol in an Office 2016 namespace, with a fallback every version reads.
const SYMBOL: &str = r#"<mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx w16se:font="A"/></mc:Choice><mc:Fallback><w:sym w:font="A"/></mc:Fallback></mc:AlternateContent>"#;

fn processed(xml: &str, target: OfficeVersion) -> Result<String, XmlError> {
    let mut document = parse(xml.as_bytes())?;
    document.process_markup_compatibility(MarkupCompatibility::Process { target })?;
    Ok(String::from_utf8_lossy(&document.to_xml(KEEP)).into_owned())
}

#[test]
fn keep_is_the_default_and_changes_nothing() {
    assert_eq!(MarkupCompatibility::default(), MarkupCompatibility::Keep);
    let xml = format!(r#"<w:r xmlns:w="{W}" {MC} {W16SE}>{SYMBOL}</w:r>"#);
    let mut document = parse(xml.as_bytes()).unwrap();
    let before = document.clone();
    document
        .process_markup_compatibility(MarkupCompatibility::Keep)
        .unwrap();
    assert_eq!(document, before);
}

#[test]
fn a_choice_the_target_does_not_understand_gives_way_to_the_fallback() {
    let xml = format!(r#"<w:r xmlns:w="{W}" {MC} {W16SE}>{SYMBOL}</w:r>"#);
    assert_eq!(
        processed(&xml, OfficeVersion::Office2013),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC} {W16SE}><w:sym w:font="A"/></w:r>"#
        ))
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2016),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC} {W16SE}><w16se:symEx w16se:font="A"/></w:r>"#
        ))
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Microsoft365),
        processed(&xml, OfficeVersion::Office2016)
    );
}

#[test]
fn the_first_choice_the_target_understands_is_taken() {
    let xml = format!(
        r#"<w:r xmlns:w="{W}" {MC} {W14} {W15}><mc:AlternateContent><mc:Choice Requires="w15"><w:a/></mc:Choice><mc:Choice Requires="w14"><w:b/></mc:Choice><mc:Choice Requires="w14 w15"><w:c/></mc:Choice><mc:Fallback><w:d/></mc:Fallback></mc:AlternateContent></w:r>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2010),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC} {W14} {W15}><w:b/></w:r>"#
        ))
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2013),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC} {W14} {W15}><w:a/></w:r>"#
        ))
    );
}

#[test]
fn a_block_with_no_choice_understood_and_no_fallback_goes() {
    let xml = format!(
        r#"<w:r xmlns:w="{W}" {MC} {W16SE}><w:t/><mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx/></mc:Choice></mc:AlternateContent><w:t/></w:r>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2010),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC} {W16SE}><w:t/><w:t/></w:r>"#
        ))
    );
}

#[test]
fn the_block_and_branch_declarations_move_onto_the_elements_that_replace_it() {
    let xml = format!(
        r#"<w:r xmlns:w="{W}" {MC}><mc:AlternateContent {WPS}><mc:Choice Requires="wps" {W14}><wps:wsp w14:a="1"/>text<wps:txbx {WPS}/></mc:Choice></mc:AlternateContent></w:r>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2010),
        Ok(format!(
            r#"<w:r xmlns:w="{W}" {MC}><wps:wsp {WPS} {W14} w14:a="1"/>text<wps:txbx {W14} {WPS}/></w:r>"#
        ))
    );
}

#[test]
fn ignorable_content_the_target_does_not_understand_is_dropped_or_unwrapped() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W14} {W15} mc:Ignorable="w14 w15" mc:ProcessContent="w15:wrap"><w:p w14:paraId="1" w15:x="2"/><w15:gone><w:p/></w15:gone><w15:wrap><w:p/><w:p/></w15:wrap></w:body>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2010),
        Ok(format!(
            r#"<w:body xmlns:w="{W}" {MC} {W14} {W15} mc:Ignorable="w14 w15" mc:ProcessContent="w15:wrap"><w:p w14:paraId="1"/><w:p/><w:p/></w:body>"#
        ))
    );
    assert_eq!(processed(&xml, OfficeVersion::Office2013), Ok(xml));
}

#[test]
fn an_unwrapped_element_hands_its_declarations_to_its_children() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15" mc:ProcessContent="w15:wrap"><w15:wrap {W14}><w:p w14:paraId="1"/>text</w15:wrap></w:body>"#
    );
    let written = processed(&xml, OfficeVersion::Office2010).unwrap();
    assert_eq!(
        written,
        format!(
            r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15" mc:ProcessContent="w15:wrap"><w:p {W14} w14:paraId="1"/>text</w:body>"#
        )
    );
    parse(written.as_bytes()).unwrap();
}

#[test]
fn a_namespace_outside_the_table_is_never_understood() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {OTHER} mc:Ignorable="x"><w:p x:a="1"/><x:b/></w:body>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Microsoft365),
        Ok(format!(
            r#"<w:body xmlns:w="{W}" {MC} {OTHER} mc:Ignorable="x"><w:p/></w:body>"#
        ))
    );
}

#[test]
fn the_mc_attributes_apply_to_their_element_and_its_descendants_only() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15}><w:p mc:Ignorable="w15" w15:a="1"><w:r w15:b="2"/></w:p><w:p w15:c="3"/><w15:d mc:Ignorable="w15"/></w:body>"#
    );
    assert_eq!(
        processed(&xml, OfficeVersion::Office2010),
        Ok(format!(
            r#"<w:body xmlns:w="{W}" {MC} {W15}><w:p mc:Ignorable="w15"><w:r/></w:p><w:p w15:c="3"/></w:body>"#
        ))
    );
}

#[test]
fn a_must_understand_namespace_the_target_lacks_is_an_error() {
    let xml = format!(r#"<w:body xmlns:w="{W}" {MC} {W16SE} mc:MustUnderstand="w16se"/>"#);
    assert_eq!(
        processed(&xml, OfficeVersion::Office2013),
        Err(XmlError::MustUnderstand {
            prefix: "w16se".to_owned(),
        })
    );
    assert_eq!(processed(&xml, OfficeVersion::Office2016), Ok(xml));
    let undeclared =
        format!(r#"<w:body xmlns:w="{W}" {MC}><w:p mc:MustUnderstand="w16se"/></w:body>"#);
    assert_eq!(
        processed(&undeclared, OfficeVersion::Microsoft365),
        Err(XmlError::MustUnderstand {
            prefix: "w16se".to_owned(),
        })
    );
}

#[test]
fn the_cleanup_then_removes_what_no_longer_applies() {
    let xml = format!(r#"<w:r xmlns:w="{W}" {MC} {W16SE} mc:Ignorable="w16se">{SYMBOL}</w:r>"#);
    let mut document = parse(xml.as_bytes()).unwrap();
    document
        .process_markup_compatibility(MarkupCompatibility::Process {
            target: OfficeVersion::Office2010,
        })
        .unwrap();
    assert_eq!(
        String::from_utf8(document.to_xml(WriteOptions::default())).unwrap(),
        format!(r#"<w:r xmlns:w="{W}"><w:sym w:font="A"/></w:r>"#)
    );
}
