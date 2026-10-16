use super::PartWriter;
use crate::xml::element::{NamespaceDeclaration, Node};
use crate::xml::parse::parse;
use crate::xml::write::Layout;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// The bytes `write` sends through a `PartWriter` in `layout`.
fn written<F>(layout: Layout, write: F) -> String
where
    F: FnOnce(&mut PartWriter<'_>),
{
    let mut sink = Vec::new();
    write(&mut PartWriter::new(&mut sink, layout));
    String::from_utf8(sink).unwrap()
}

#[test]
fn each_layout_gives_its_bytes() {
    let paragraph = parse(b"<w:p xmlns:w=\"urn:w\"><w:r><w:t>b</w:t></w:r></w:p>")
        .unwrap()
        .root;
    let declarations = [NamespaceDeclaration {
        prefix: "w".to_owned(),
        uri: W.to_owned(),
    }];
    let part = |out: &mut PartWriter<'_>| {
        out.declaration().unwrap();
        out.start("w:body", &declarations, &[("w:a", "1")]).unwrap();
        out.start("w:t", &[], &[]).unwrap();
        out.text("x < y").unwrap();
        out.end("w:t").unwrap();
        out.element(&paragraph).unwrap();
        out.node(&Node::Comment("c".to_owned())).unwrap();
        out.raw(b"<w:raw/>").unwrap();
        out.empty("w:sectPr", &[]).unwrap();
        out.end("w:body").unwrap();
    };
    assert_eq!(
        written(Layout::Compact, part),
        format!(
            "{DECLARATION}<w:body xmlns:w=\"{W}\" w:a=\"1\"><w:t>x &lt; y</w:t>\
             <w:p xmlns:w=\"urn:w\"><w:r><w:t>b</w:t></w:r></w:p><!--c--><w:raw/><w:sectPr/></w:body>"
        )
    );
    assert_eq!(
        written(Layout::Pretty, part),
        format!(
            "{DECLARATION}<w:body xmlns:w=\"{W}\" w:a=\"1\">\n  <w:t>x &lt; y</w:t>\n  \
             <w:p xmlns:w=\"urn:w\">\n    <w:r>\n      <w:t>b</w:t>\n    </w:r>\n  </w:p>\n  <!--c--><w:raw/>\n  \
             <w:sectPr/>\n</w:body>"
        )
    );
}

#[test]
fn values_are_escaped_as_the_generic_tree_writer_escapes_them() {
    let declarations = [NamespaceDeclaration {
        prefix: String::new(),
        uri: "urn:a&b".to_owned(),
    }];
    let bytes = written(Layout::Compact, |out| {
        out.start("a", &declarations, &[("b", "\"\t\n\r<&>'")])
            .unwrap();
        out.empty("c", &[("d", "<\"&\">")]).unwrap();
        out.text("1 < 2 & 3 > 2\r\t\"'\n").unwrap();
        out.end("a").unwrap();
    });
    assert_eq!(
        bytes,
        "<a xmlns=\"urn:a&amp;b\" b=\"&quot;&#x9;&#xA;&#xD;&lt;&amp;>'\"><c d=\"&lt;&quot;&amp;&quot;>\"/>\
         1 &lt; 2 &amp; 3 &gt; 2&#xD;\t\"'\n</a>"
    );
    let document = parse(bytes.as_bytes()).unwrap();
    assert_eq!(document.root.attributes[0].value, "\"\t\n\r<&>'");
}

#[test]
fn whitespace_only_text_is_dropped_in_the_compact_layout_outside_a_preserve_scope() {
    let part = |out: &mut PartWriter<'_>| {
        out.start("a", &[], &[]).unwrap();
        out.start("t", &[], &[]).unwrap();
        out.text(" ").unwrap();
        out.end("t").unwrap();
        out.start("t", &[], &[("xml:space", "preserve")]).unwrap();
        out.text(" ").unwrap();
        out.end("t").unwrap();
        out.end("a").unwrap();
    };
    assert_eq!(
        written(Layout::Compact, part),
        "<a><t/><t xml:space=\"preserve\"> </t></a>"
    );
    assert_eq!(
        written(Layout::Pretty, part),
        "<a>\n  <t> </t>\n  <t xml:space=\"preserve\"> </t>\n</a>"
    );
}

#[test]
fn a_preserve_scope_reaches_the_descendants_until_an_inner_default_ends_it() {
    let bytes = written(Layout::Compact, |out| {
        out.start("a", &[], &[("xml:space", "preserve")]).unwrap();
        out.start("b", &[], &[]).unwrap();
        out.text(" ").unwrap();
        out.end("b").unwrap();
        out.start("c", &[], &[("xml:space", "default")]).unwrap();
        out.text(" ").unwrap();
        out.end("c").unwrap();
        out.text(" ").unwrap();
        out.end("a").unwrap();
    });
    assert_eq!(
        bytes,
        "<a xml:space=\"preserve\"><b> </b><c xml:space=\"default\"/> </a>"
    );
}

#[test]
fn whitespace_only_text_written_as_a_node_is_dropped_in_the_compact_layout() {
    let part = |out: &mut PartWriter<'_>| {
        out.start("a", &[], &[]).unwrap();
        out.node(&Node::Text(" ".to_owned())).unwrap();
        out.node(&Node::Text("b < c".to_owned())).unwrap();
        out.end("a").unwrap();
    };
    assert_eq!(written(Layout::Compact, part), "<a>b &lt; c</a>");
    assert_eq!(written(Layout::Pretty, part), "<a> b &lt; c</a>");
}

#[test]
fn text_before_and_after_the_root_is_written_as_read() {
    let part = |out: &mut PartWriter<'_>| {
        out.node(&Node::Text("\r\n".to_owned())).unwrap();
        out.empty("a", &[]).unwrap();
        out.text("\r\n").unwrap();
    };
    assert_eq!(written(Layout::Compact, part), "\r\n<a/>\r\n");
    assert_eq!(written(Layout::Pretty, part), "\r\n<a/>\r\n");
}
