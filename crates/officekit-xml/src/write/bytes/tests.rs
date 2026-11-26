use super::clean_namespace_bytes;
use crate::parse::parse;
use crate::write::{Layout, WriteOptions};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const W16SE: &str = r#"xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex""#;
const WPS: &str =
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";

/// The parts the cleanup of the tree changes, and parts it leaves alone.
fn parts() -> Vec<String> {
    vec![
        format!(
            r#"<w:document xmlns:w="{W}" {MC} {W14} {W15} mc:Ignorable="w14 w15"><w:p w14:paraId="1"/></w:document>"#
        ),
        format!(
            r#"<w:document xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:body/></w:document>"#
        ),
        format!(
            r#"<w:r xmlns:w="{W}" {MC} {WPS} {W16SE}><mc:AlternateContent><mc:Choice Requires="wps"><w:t/></mc:Choice><mc:Fallback><w:t/></mc:Fallback></mc:AlternateContent></w:r>"#
        ),
        format!(
            r#"<w:document xmlns:w="{W}" {W14}><w:body><w:p w14:paraId="1"/></w:body></w:document>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} {W14} {W15} mc:MustUnderstand="w15" mc:ProcessContent="w15:wrap w14:a" mc:PreserveElements="w15:b" mc:PreserveAttributes="w15:c w14:d"><w:p w14:paraId="1"/></w:body>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:p {W15} mc:Ignorable="w15" w15:x="1"/></w:body>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} xmlns:x="urn:x" mc:Ignorable="x"><w:p xmlns:x="urn:x" x:a="1"/></w:body>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} xmlns:x="urn:x" mc:Ignorable="x"><w:p xmlns:y="urn:x" y:a="1"/></w:body>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} xmlns:x="urn:x" mc:Ignorable="x"><x:a xmlns:x="urn:x"/></w:body>"#
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} xmlns:x="urn:x" mc:Ignorable="x"><w:p xmlns:x="urn:y" x:a="1"/></w:body>"#
        ),
        format!(
            r#"{DECLARATION}<!--a--><w:document xmlns:w="{W_STRICT}" xmlns:o="urn:o" xmlns:v="urn:v"><w:body><w:p><w:r><w:t xml:space="preserve"> a &amp; &lt;b&gt; </w:t></w:r></w:p><![CDATA[x<y]]><?pi data?><w:p xmlns:v="urn:v" v:a="&quot;"/></w:body></w:document><!--z-->"#
        ),
        r#"<a xmlns="urn:a" xmlns:b="urn:b"><c xmlns="urn:c"><d/></c><e xmlns:b="urn:b2" b:f="1"/></a>"#
            .to_owned(),
        format!(r#"<w:p xmlns:w="{W}"><w:r/></w:p>"#),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} xmlns:a="urn:x" xmlns:b="urn:x" mc:Ignorable="a"><w:p b:x="1"/></w:body>"#
        ),
        format!(
            "<w:body xmlns:w=\"{W}\" {MC} {W14} mc:Ignorable=\"w14  w14\"><w:p w14:paraId=\"1\"/></w:body>"
        ),
        format!(
            "{DECLARATION}\r\n<w:document {MC} \
             xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
             xmlns:wp14=\"http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing\" \
             xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
             xmlns:w=\"{W}\" {W14} {W15} {W16SE} mc:Ignorable=\"w14 w15 w16se wp14\"><w:body><w:p><w:r><w:drawing>\
             <wp:inline wp14:anchorId=\"1\"/></w:drawing></w:r><w:hyperlink r:id=\"rId1\"/></w:p></w:body></w:document>"
        ),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:p><w:r mc:Ignorable="w15" w15:x="1"/></w:p></w:body>"#
        ),
    ]
}

/// The bytes of a part written compact with the cleanup on or off.
fn written(xml: &str, cleanup: bool) -> Vec<u8> {
    parse(xml.as_bytes())
        .unwrap()
        .to_xml(WriteOptions::new(Layout::Compact, cleanup))
}

#[test]
fn cleaning_the_bytes_gives_what_cleaning_the_tree_gives() {
    for xml in parts() {
        let mut bytes = written(&xml, false);
        clean_namespace_bytes(&mut bytes);
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            String::from_utf8(written(&xml, true)).unwrap(),
            "{xml}"
        );
    }
}

#[test]
fn a_list_naming_a_prefix_two_open_elements_declare_is_cleaned_as_the_tree_is() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:p {W15}><w:r mc:Ignorable="w15"/></w:p></w:body>"#
    );
    let mut bytes = written(&xml, false);
    clean_namespace_bytes(&mut bytes);
    assert_eq!(bytes, written(&xml, true));
}

#[test]
fn bytes_that_are_not_well_formed_stay_as_they_are() {
    let mut bytes = b"<a xmlns:b=\"urn:b\"><c></a".to_vec();
    let before = bytes.clone();
    clean_namespace_bytes(&mut bytes);
    assert_eq!(bytes, before);
}
