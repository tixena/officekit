use officekit_xml::element::{Document, Element, NamespaceDeclaration, Node};
use officekit_xml::name::Namespace;
use officekit_xml::parse::parse;
use officekit_xml::write::{Layout, WriteOptions};

use super::{Children, Extra, Pending, Placement, Scope, Unknown};

const EXTENDED_PROPERTIES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/extended-properties";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const DECLARE_W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
const DECLARE_X: &str = "xmlns:x=\"urn:x\"";

/// `w:p`, written by hand the way the generator writes a struct, with its
/// reader and writer as free functions; `w:pPr` stands in as a generic
/// element.
#[derive(Debug, Default)]
struct Paragraph {
    content: Vec<ParagraphContent>,
    properties: Option<Element>,
    unknown: Unknown,
}

#[derive(Debug)]
enum ParagraphContent {
    Other(Node),
    TextRun(TextRun),
}

#[derive(Debug)]
enum RunContent {
    Other(Node),
    Text(String),
}

/// `w:r`, holding a list of `w:t`.
#[derive(Debug, Default)]
struct TextRun {
    content: Vec<RunContent>,
    unknown: Unknown,
}

fn read(xml: &str) -> Paragraph {
    read_paragraph(&parse(xml.as_bytes()).unwrap().root)
}

fn read_paragraph(element: &Element) -> Paragraph {
    let mut paragraph = Paragraph {
        unknown: unknown_of(element),
        ..Paragraph::default()
    };
    let mut placement = Placement::default();
    for node in &element.children {
        if let Some(child) = tag(node, "pPr")
            && paragraph.properties.is_none()
            && paragraph.content.is_empty()
        {
            match placement.single() {
                Pending::Extra(extra) => paragraph.unknown.children.extend(extra),
                Pending::List(nodes) => paragraph
                    .content
                    .extend(nodes.into_iter().map(ParagraphContent::Other)),
            }
            paragraph.properties = Some(child.clone());
        } else if let Some(child) = tag(node, "r") {
            paragraph
                .content
                .extend(placement.item().into_iter().map(ParagraphContent::Other));
            paragraph
                .content
                .push(ParagraphContent::TextRun(read_text_run(child)));
        } else {
            placement.unknown(node.clone());
        }
    }
    match placement.end() {
        Pending::Extra(extra) => paragraph.unknown.children.extend(extra),
        Pending::List(nodes) => paragraph
            .content
            .extend(nodes.into_iter().map(ParagraphContent::Other)),
    }
    paragraph
}

fn read_text_run(element: &Element) -> TextRun {
    let mut run = TextRun {
        unknown: unknown_of(element),
        ..TextRun::default()
    };
    let mut placement = Placement::default();
    for node in &element.children {
        if let Some(child) = tag(node, "t") {
            run.content
                .extend(placement.item().into_iter().map(RunContent::Other));
            run.content.push(RunContent::Text(text_of(child)));
        } else {
            placement.unknown(node.clone());
        }
    }
    match placement.end() {
        Pending::Extra(extra) => run.unknown.children.extend(extra),
        Pending::List(nodes) => run.content.extend(nodes.into_iter().map(RunContent::Other)),
    }
    run
}

/// The element `node` is when it is the `WordprocessingML` tag `local`.
fn tag<'node>(node: &'node Node, local: &str) -> Option<&'node Element> {
    if let Node::Element(element) = node
        && element.name.local == local
        && element.name.namespace == Namespace::from_uri(W)
    {
        Some(element)
    } else {
        None
    }
}

fn text_of(element: &Element) -> String {
    element
        .children
        .iter()
        .filter_map(|node| {
            if let Node::Text(text) = node {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect()
}

fn to_xml(root: Element) -> String {
    let mut document = Document::new("", root);
    document.declaration = None;
    document.prolog.clear();
    let options = WriteOptions::new(Layout::Compact, false);
    String::from_utf8(document.to_xml(options)).unwrap()
}

fn unknown_of(element: &Element) -> Unknown {
    Unknown {
        absent: Vec::new(),
        attributes: element.attributes.clone(),
        children: Vec::new(),
        namespace_declarations: element.namespace_declarations.clone(),
    }
}

fn write(paragraph: &Paragraph) -> String {
    to_xml(write_paragraph(paragraph, &mut Scope::default()))
}

fn write_paragraph(paragraph: &Paragraph, scope: &mut Scope) -> Element {
    let mut element = scope.element(W, "p", &paragraph.unknown.namespace_declarations);
    element
        .attributes
        .extend(paragraph.unknown.attributes.iter().cloned());
    let mut children = Children::new(&paragraph.unknown.children);
    if let Some(properties) = &paragraph.properties {
        children.known(properties.clone());
    }
    for item in &paragraph.content {
        match item {
            ParagraphContent::Other(node) => children.other(node.clone()),
            ParagraphContent::TextRun(run) => children.known(write_text_run(run, scope)),
        }
    }
    element.children = children.finish();
    scope.end();
    element
}

fn write_text_run(run: &TextRun, scope: &mut Scope) -> Element {
    let mut element = scope.element(W, "r", &run.unknown.namespace_declarations);
    element
        .attributes
        .extend(run.unknown.attributes.iter().cloned());
    let mut children = Children::new(&run.unknown.children);
    for item in &run.content {
        match item {
            RunContent::Other(node) => children.other(node.clone()),
            RunContent::Text(value) => {
                let mut text = scope.element(W, "t", &[]);
                text.children.push(Node::Text(value.clone()));
                scope.end();
                children.known(text);
            }
        }
    }
    element.children = children.finish();
    scope.end();
    element
}

#[test]
fn an_unknown_element_between_two_runs_stays_between_them() {
    let xml = format!("<w:p {DECLARE_W} {DECLARE_X}><w:r/><x:foo/><w:r/></w:p>");
    let paragraph = read(&xml);
    assert_eq!(paragraph.content.len(), 3);
    assert!(matches!(
        &paragraph.content[1],
        ParagraphContent::Other(Node::Element(foo)) if foo.name.local == "foo"
    ));
    assert_eq!(write(&paragraph), xml);
}

#[test]
fn unknown_nodes_before_the_paragraph_properties_keep_their_place() {
    let xml = format!("<w:p {DECLARE_W} {DECLARE_X}>\n  <!--c--><x:foo/>\n  <w:pPr/><w:r/></w:p>");
    let paragraph = read(&xml);
    let places: Vec<usize> = paragraph
        .unknown
        .children
        .iter()
        .map(|extra| extra.after)
        .collect();
    assert_eq!(places, [0, 0]);
    assert_eq!(
        paragraph.unknown.children[0],
        Extra {
            after: 0,
            node: Node::Comment("c".to_owned()),
        }
    );
    assert_eq!(
        write(&paragraph),
        format!("<w:p {DECLARE_W} {DECLARE_X}><!--c--><x:foo/><w:pPr/><w:r/></w:p>")
    );
}

#[test]
fn an_unknown_node_moves_with_the_list_item_after_it_when_a_run_is_inserted() {
    let mut paragraph = read(&format!(
        "<w:p {DECLARE_W} {DECLARE_X}><w:r/><x:foo/><w:r/></w:p>"
    ));
    let inserted = TextRun {
        content: vec![RunContent::Text("new".to_owned())],
        ..TextRun::default()
    };
    paragraph
        .content
        .insert(0, ParagraphContent::TextRun(inserted));
    assert_eq!(
        write(&paragraph),
        format!("<w:p {DECLARE_W} {DECLARE_X}><w:r><w:t>new</w:t></w:r><w:r/><x:foo/><w:r/></w:p>")
    );
}

#[test]
fn alternate_content_that_is_a_runs_only_child_stays_before_text_added_later() {
    let declare_mc = "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"";
    let block = "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing/></mc:Choice></mc:AlternateContent>";
    let xml = format!("<w:r {DECLARE_W} {declare_mc}>{block}</w:r>");
    let mut run = read_text_run(&parse(xml.as_bytes()).unwrap().root);
    assert!(run.content.is_empty());
    assert_eq!(run.unknown.children.len(), 1);
    run.content.push(RunContent::Text("after".to_owned()));
    assert_eq!(
        to_xml(write_text_run(&run, &mut Scope::default())),
        format!("<w:r {DECLARE_W} {declare_mc}>{block}<w:t>after</w:t></w:r>")
    );
}

#[test]
fn a_prefix_other_than_the_tables_is_kept() {
    for uri in [W, W_STRICT] {
        let xml = format!("<ns0:p xmlns:ns0=\"{uri}\"><ns0:r><ns0:t>a</ns0:t></ns0:r></ns0:p>");
        assert_eq!(write(&read(&xml)), xml);
    }
}

#[test]
fn a_new_paragraph_declares_the_tables_prefix() {
    let paragraph = Paragraph {
        content: vec![ParagraphContent::TextRun(TextRun::default())],
        ..Paragraph::default()
    };
    assert_eq!(write(&paragraph), format!("<w:p {DECLARE_W}><w:r/></w:p>"));
}

#[test]
fn a_scope_gives_the_bound_prefix_until_an_inner_declaration_rebinds_it() {
    let declare =
        |prefix: &str, uri: &str| NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned());
    let mut scope = Scope::default();
    let mut paragraph = scope.element(W, "p", &[declare("a", W_STRICT)]);
    scope.attribute(&mut paragraph, Some(W), "rsidR", "1");
    scope.attribute(&mut paragraph, None, "id", "2");
    scope.attribute(&mut paragraph, Some(XML), "space", "preserve");
    scope.attribute(&mut paragraph, Some(W14), "paraId", "3");
    let run = scope.element(W, "r", &[declare("a", "urn:other")]);
    scope.end();
    let text = scope.element(W, "t", &[]);
    scope.end();
    scope.end();
    paragraph.children = vec![Node::Element(run), Node::Element(text)];
    assert_eq!(
        to_xml(paragraph),
        format!(
            "<a:p xmlns:a=\"{W_STRICT}\" xmlns:w14=\"{W14}\" a:rsidR=\"1\" id=\"2\" xml:space=\"preserve\" \
             w14:paraId=\"3\"><w:r xmlns:a=\"urn:other\" xmlns:w=\"{W}\"/><a:t/></a:p>"
        )
    );
}

fn declaration(prefix: &str, uri: &str) -> NamespaceDeclaration {
    NamespaceDeclaration::new(prefix.to_owned(), uri.to_owned())
}

#[test]
fn an_element_in_the_default_namespace_keeps_no_prefix() {
    let mut scope = Scope::default();
    let properties = scope.element(
        EXTENDED_PROPERTIES,
        "Properties",
        &[declaration("", EXTENDED_PROPERTIES)],
    );
    scope.end();
    assert_eq!(
        to_xml(properties),
        format!("<Properties xmlns=\"{EXTENDED_PROPERTIES}\"/>")
    );
}

#[test]
fn an_inner_default_namespace_hides_the_outer_one() {
    let mut scope = Scope::default();
    let mut outer = scope.element(W, "p", &[declaration("", W)]);
    let inner = scope.element(W, "r", &[declaration("", "urn:other")]);
    scope.end();
    scope.end();
    outer.children = vec![Node::Element(inner)];
    assert_eq!(
        to_xml(outer),
        format!("<p xmlns=\"{W}\"><w:r xmlns=\"urn:other\" xmlns:w=\"{W}\"/></p>")
    );
}

#[test]
fn an_attribute_never_takes_the_default_namespace() {
    let mut scope = Scope::default();
    let mut paragraph = scope.element(W, "p", &[declaration("", W)]);
    scope.attribute(&mut paragraph, Some(W), "rsidR", "1");
    scope.end();
    assert_eq!(
        to_xml(paragraph),
        format!("<p xmlns=\"{W}\" xmlns:w=\"{W}\" w:rsidR=\"1\"/>")
    );
}
