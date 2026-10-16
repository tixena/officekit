use super::{Layout, WriteOptions};
use crate::xml::parse::parse;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const W16SE: &str = r#"xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex""#;
const WPS: &str =
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";

const KEEP: WriteOptions = WriteOptions {
    layout: Layout::Compact,
    namespace_cleanup: false,
};
const PRETTY: WriteOptions = WriteOptions {
    layout: Layout::Pretty,
    namespace_cleanup: false,
};

fn write(xml: &str, options: WriteOptions) -> String {
    String::from_utf8(parse(xml.as_bytes()).unwrap().to_xml(options)).unwrap()
}

#[test]
fn the_default_options_are_compact_with_the_cleanup() {
    assert_eq!(
        WriteOptions::default(),
        WriteOptions {
            layout: Layout::Compact,
            namespace_cleanup: true,
        }
    );
    assert_eq!(Layout::default(), Layout::Compact);
}

#[test]
fn compact_keeps_preserved_space() {
    let xml = format!(r#"<w:t xmlns:w="{W}" xml:space="preserve">  a  </w:t>"#);
    assert_eq!(write(&xml, KEEP), xml);
    let blank = format!(r#"<w:t xmlns:w="{W}" xml:space="preserve">   </w:t>"#);
    assert_eq!(write(&blank, KEEP), blank);
}

#[test]
fn compact_drops_whitespace_between_elements_but_not_preserved_space() {
    let xml = format!(
        "<w:p xmlns:w=\"{W}\">\n  <w:r>\n    <w:t xml:space=\"preserve\"> </w:t>\n  </w:r>\n  <w:r><w:t>a b</w:t></w:r>\n  <w:r>\r\n</w:r>\n</w:p>"
    );
    assert_eq!(
        write(&xml, KEEP),
        format!(
            "<w:p xmlns:w=\"{W}\"><w:r><w:t xml:space=\"preserve\"> </w:t></w:r><w:r><w:t>a b</w:t></w:r><w:r/></w:p>"
        )
    );
}

#[test]
fn an_inner_xml_space_default_ends_the_preserve() {
    let xml = "<p xml:space=\"preserve\"><r xml:space=\"default\">\n  <t>x</t>\n</r><s> </s>\n</p>";
    assert_eq!(
        write(xml, KEEP),
        "<p xml:space=\"preserve\"><r xml:space=\"default\"><t>x</t></r><s> </s>\n</p>"
    );
}

#[test]
fn attribute_values_escape_quotes_tabs_and_line_breaks() {
    let document = parse(b"<a b=\"&quot;&#x9;&#xA;&#xD;&lt;&amp;>'\"/>").unwrap();
    assert_eq!(document.root.attributes[0].value, "\"\t\n\r<&>'");
    assert_eq!(
        String::from_utf8(document.to_xml(KEEP)).unwrap(),
        "<a b=\"&quot;&#x9;&#xA;&#xD;&lt;&amp;>'\"/>"
    );
}

#[test]
fn text_escapes_ampersands_angle_brackets_and_carriage_returns() {
    let xml = "<a>1 &lt; 2 &amp; 3 &gt; 2&#xD;\t\"'\n</a>";
    let document = parse(xml.as_bytes()).unwrap();
    assert_eq!(String::from_utf8(document.to_xml(KEEP)).unwrap(), xml);
    let other_spellings = "<a>&#60;&#x3E;&#13;&apos;&quot;</a>";
    assert_eq!(write(other_spellings, KEEP), "<a>&lt;&gt;&#xD;'\"</a>");
}

#[test]
fn declarations_come_before_attributes() {
    let xml = format!(r#"<w:p w:rsidR="1" xmlns:w="{W}" a="2" xmlns="urn:d"/>"#);
    assert_eq!(
        write(&xml, KEEP),
        format!(r#"<w:p xmlns:w="{W}" xmlns="urn:d" w:rsidR="1" a="2"/>"#)
    );
}

#[test]
fn the_prolog_the_epilog_and_the_declaration_come_out_as_read() {
    let xml = format!(r#"<w:t xmlns:w="{W}">text</w:t>"#);
    for (before, after) in [
        ("\r\n", ""),
        ("\r", "\r\n"),
        ("", "\n"),
        ("\r\n<!--c--><?app x?>\n", "\n<!--end-->\r\n"),
    ] {
        let part = format!("{DECLARATION}{before}{xml}{after}");
        assert_eq!(write(&part, KEEP), part);
        assert_eq!(write(&part, PRETTY), part);
        assert_eq!(write(&part, WriteOptions::default()), part);
    }
    let single_quoted = "<?xml version='1.0' encoding='utf-8'?>\n<a/>";
    assert_eq!(write(single_quoted, KEEP), single_quoted);
}

#[test]
fn writing_what_was_written_gives_the_same_bytes() {
    let xml = format!(
        "{DECLARATION}\r\n<w:document xmlns:w=\"{W}\" {MC} {W14} mc:Ignorable=\"w14\">\n  <w:body>\n    \
         <w:p w14:paraId=\"1\"><w:r><w:t xml:space=\"preserve\"> a &amp; b &lt;c&gt; \"d\" 'e' </w:t></w:r>\
         <!-- note --><?pi data?><w:r><w:instrText><![CDATA[ x < y ]]></w:instrText></w:r>\
         <w:r><w:t>line&#xD;\nbreak</w:t></w:r></w:p>\n    <w:p w:rsidR=\"a&#x9;b&#xA;c&quot;\"></w:p>\n  </w:body>\n</w:document>\r\n"
    );
    for options in [KEEP, PRETTY, WriteOptions::default()] {
        let once = write(&xml, options);
        assert_eq!(write(&once, options), once);
    }
}

#[test]
fn pretty_indents_element_only_content_and_compacting_it_gives_the_compact_form_back() {
    let compact = format!(
        r#"<w:p xmlns:w="{W}"><w:pPr><w:jc w:val="center"/></w:pPr><!--c--><w:r><w:t xml:space="preserve"> a </w:t><w:t>b<w:x/>c</w:t></w:r></w:p>"#
    );
    let pretty = write(&compact, PRETTY);
    assert_eq!(
        pretty,
        format!(
            "<w:p xmlns:w=\"{W}\">\n  <w:pPr>\n    <w:jc w:val=\"center\"/>\n  </w:pPr>\n  <!--c-->\n  <w:r>\n    \
             <w:t xml:space=\"preserve\"> a </w:t>\n    <w:t>b<w:x/>c</w:t>\n  </w:r>\n</w:p>"
        )
    );
    assert_eq!(write(&pretty, KEEP), compact);
    assert_eq!(write(&pretty, PRETTY), pretty);
}

#[test]
fn pretty_passes_every_text_node_through() {
    let xml = format!("<w:p xmlns:w=\"{W}\">\n<w:r/>   <w:r/>\r\n</w:p>");
    assert_eq!(
        write(&xml, PRETTY),
        format!("<w:p xmlns:w=\"{W}\">\n<w:r/>   <w:r/>\n</w:p>")
    );
    assert_eq!(
        write(&write(&xml, PRETTY), KEEP),
        format!("<w:p xmlns:w=\"{W}\"><w:r/><w:r/></w:p>")
    );
}

#[test]
fn pretty_does_not_indent_inside_preserved_space_until_an_inner_default_ends_it() {
    let compact = "<p xml:space=\"preserve\"><r/></p>";
    let pretty = write(compact, PRETTY);
    assert_eq!(pretty, compact);
    assert_eq!(write(&pretty, KEEP), compact);
    let nested = "<p xml:space=\"preserve\"><r xml:space=\"default\"><s/><t/></r></p>";
    let nested_pretty = write(nested, PRETTY);
    assert_eq!(
        nested_pretty,
        "<p xml:space=\"preserve\"><r xml:space=\"default\">\n    <s/>\n    <t/>\n  </r></p>"
    );
    assert_eq!(write(&nested_pretty, KEEP), nested);
}

#[test]
fn the_cleanup_removes_an_unused_declaration_and_its_list_entries() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {MC} {W14} {W15} mc:Ignorable="w14 w15"><w:p w14:paraId="1"/></w:document>"#
    );
    assert_eq!(
        write(&xml, WriteOptions::default()),
        format!(
            r#"<w:document xmlns:w="{W}" {MC} {W14} mc:Ignorable="w14"><w:p w14:paraId="1"/></w:document>"#
        )
    );
}

#[test]
fn the_cleanup_repeats_until_mc_itself_goes() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:body/></w:document>"#
    );
    assert_eq!(
        write(&xml, WriteOptions::default()),
        format!(r#"<w:document xmlns:w="{W}"><w:body/></w:document>"#)
    );
}

#[test]
fn the_cleanup_keeps_a_prefix_a_requires_names() {
    let xml = format!(
        r#"<w:r xmlns:w="{W}" {MC} {WPS} {W16SE}><mc:AlternateContent><mc:Choice Requires="wps"><w:t/></mc:Choice><mc:Fallback><w:t/></mc:Fallback></mc:AlternateContent></w:r>"#
    );
    assert_eq!(
        write(&xml, WriteOptions::default()),
        format!(
            r#"<w:r xmlns:w="{W}" {MC} {WPS}><mc:AlternateContent><mc:Choice Requires="wps"><w:t/></mc:Choice><mc:Fallback><w:t/></mc:Fallback></mc:AlternateContent></w:r>"#
        )
    );
}

#[test]
fn the_cleanup_keeps_a_declaration_used_only_below_it() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {W14}><w:body><w:p w14:paraId="1"/></w:body></w:document>"#
    );
    assert_eq!(write(&xml, WriteOptions::default()), xml);
}

#[test]
fn the_cleanup_drops_qualified_names_and_must_understand_prefixes() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W14} {W15} mc:MustUnderstand="w15" mc:ProcessContent="w15:wrap w14:a" mc:PreserveElements="w15:b" mc:PreserveAttributes="w15:c w14:d"><w:p w14:paraId="1"/></w:body>"#
    );
    assert_eq!(
        write(&xml, WriteOptions::default()),
        format!(
            r#"<w:body xmlns:w="{W}" {MC} {W14} mc:ProcessContent="w14:a" mc:PreserveAttributes="w14:d"><w:p w14:paraId="1"/></w:body>"#
        )
    );
}

#[test]
fn the_cleanup_drops_a_list_entry_only_where_the_removed_declaration_is_in_scope() {
    let xml = format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15"><w:p {W15} mc:Ignorable="w15" w15:x="1"/></w:body>"#
    );
    assert_eq!(
        write(&xml, WriteOptions::default()),
        format!(r#"<w:body xmlns:w="{W}" {MC}><w:p {W15} mc:Ignorable="w15" w15:x="1"/></w:body>"#)
    );
}

#[test]
fn the_cleanup_leaves_a_list_nothing_was_dropped_from_as_read() {
    let xml = format!(
        "<w:body xmlns:w=\"{W}\" {MC} {W14} mc:Ignorable=\"w14  w14\"><w:p w14:paraId=\"1\"/></w:body>"
    );
    assert_eq!(write(&xml, WriteOptions::default()), xml);
}

#[test]
fn the_cleanup_of_a_word_saved_root_keeps_what_is_used() {
    let xml = format!(
        "{DECLARATION}\r\n<w:document {MC} \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp14=\"http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:w=\"{W}\" {W14} {W15} {W16SE} mc:Ignorable=\"w14 w15 w16se wp14\"><w:body><w:p><w:r><w:drawing>\
         <wp:inline wp14:anchorId=\"1\"/></w:drawing></w:r><w:hyperlink r:id=\"rId1\"/></w:p></w:body></w:document>"
    );
    let cleaned = write(&xml, WriteOptions::default());
    let document = parse(cleaned.as_bytes()).unwrap();
    let prefixes: Vec<&str> = document
        .root
        .namespace_declarations
        .iter()
        .map(|declaration| declaration.prefix.as_str())
        .collect();
    assert_eq!(prefixes, ["mc", "r", "wp14", "wp", "w"]);
    assert_eq!(document.root.attributes[0].name.local, "Ignorable");
    assert_eq!(document.root.attributes[0].value, "wp14");
}

#[test]
fn an_unused_namespace_is_removed_and_preserved_text_kept() {
    let part = format!(
        "{DECLARATION}\n<w:document xmlns:w=\"{W}\" xmlns:v=\"urn:schemas-microsoft-com:vml\"><w:body><w:p><w:r>\
         <w:t xml:space=\"preserve\">  two spaces</w:t></w:r></w:p></w:body></w:document>"
    );
    let compact = write(&part, WriteOptions::default());
    assert!(!compact.contains("xmlns:v"));
    assert!(compact.contains(">  two spaces<"));
}
