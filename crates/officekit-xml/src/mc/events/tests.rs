use super::Processed;
use crate::element::{Document, Element, Node, XmlError};
use crate::mc::MarkupCompatibility;
use crate::name::OfficeVersion;
use crate::parse::parse;
use crate::stream::reader::{PartEvent, PartReader, ReadEvents as _};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const MC: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;
const W14: &str = r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#;
const W15: &str = r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml""#;
const W16SE: &str = r#"xmlns:w16se="http://schemas.microsoft.com/office/word/2015/wordml/symex""#;
const WPS: &str =
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
const OTHER: &str = r#"xmlns:x="urn:not-in-the-table""#;

const SYMBOL: &str = r#"<mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx w16se:font="A"/></mc:Choice><mc:Fallback><w:sym w:font="A"/></mc:Fallback></mc:AlternateContent>"#;

const TARGETS: [OfficeVersion; 4] = [
    OfficeVersion::Office2010,
    OfficeVersion::Office2013,
    OfficeVersion::Office2016,
    OfficeVersion::Microsoft365,
];

/// The root `Processed` gives, built from its events.
fn streamed(xml: &str, mode: MarkupCompatibility) -> Result<Element, XmlError> {
    let mut reader = PartReader::new(xml.as_bytes());
    let mut events = Processed::new(&mut reader, mode);
    let mut open: Vec<Element> = Vec::new();
    let mut root = None;
    while let Some(event) = events.event()? {
        let done = match event {
            PartEvent::Declaration(_) => continue,
            PartEvent::End => open.pop().unwrap(),
            PartEvent::Node(Node::Element(element)) if open.is_empty() => element,
            PartEvent::Node(node) => {
                if let Some(parent) = open.last_mut() {
                    parent.children.push(node);
                }
                continue;
            }
            PartEvent::Start(element) => {
                open.push(element);
                continue;
            }
        };
        match open.last_mut() {
            Some(parent) => parent.children.push(Node::Element(done)),
            None => root = Some(done),
        }
    }
    Ok(root.unwrap())
}

/// The root the tree processing gives.
fn tree(xml: &str, mode: MarkupCompatibility) -> Result<Element, XmlError> {
    let mut document: Document = parse(xml.as_bytes())?;
    document.process_markup_compatibility(mode)?;
    Ok(document.root)
}

/// Every mode gives the same root, or the same error, read forward as on the tree.
fn same_as_the_tree(xml: &str) {
    let modes = TARGETS
        .map(|target| MarkupCompatibility::Process { target })
        .into_iter()
        .chain([MarkupCompatibility::Keep]);
    for mode in modes {
        assert_eq!(streamed(xml, mode), tree(xml, mode), "{mode:?}: {xml}");
    }
}

#[test]
fn an_alternate_content_block_gives_the_branch_the_tree_chooses() {
    same_as_the_tree(&format!(
        r#"<w:r xmlns:w="{W}" {MC} {W16SE}>{SYMBOL}</w:r>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:r xmlns:w="{W}" {MC} {W14} {W15}><mc:AlternateContent><mc:Choice Requires="w15"><w:a/></mc:Choice><mc:Choice Requires="w14"><w:b/></mc:Choice><mc:Fallback><w:d/></mc:Fallback></mc:AlternateContent></w:r>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:r xmlns:w="{W}" {MC} {W16SE}><w:t/><mc:AlternateContent><mc:Choice Requires="w16se"><w16se:symEx/></mc:Choice></mc:AlternateContent><w:t/></w:r>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:r xmlns:w="{W}" {MC}><mc:AlternateContent {WPS}><mc:Choice Requires="wps" {W14}><wps:wsp w14:a="1"/>text<wps:txbx {WPS}/></mc:Choice></mc:AlternateContent></w:r>"#
    ));
}

#[test]
fn ignorable_content_is_dropped_or_unwrapped_as_on_the_tree() {
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W14} {W15} mc:Ignorable="w14 w15" mc:ProcessContent="w15:wrap"><w:p w14:paraId="1" w15:x="2"/><w15:gone><w:p/></w15:gone><w15:wrap><w:p/><w:p/></w15:wrap></w:body>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {OTHER} mc:Ignorable="x"><w:p x:a="1"/><x:b/></w:body>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15}><w:p mc:Ignorable="w15" w15:a="1"><w:r w15:b="2"/></w:p><w:p w15:c="3"/><w15:d mc:Ignorable="w15"/></w:body>"#
    ));
}

#[test]
fn nested_unwrapped_elements_hand_their_declarations_on_innermost_first() {
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15" mc:ProcessContent="w15:wrap"><w15:wrap {W14}><w:p w14:paraId="1"/>text</w15:wrap></w:body>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W15} mc:Ignorable="w15" mc:ProcessContent="w15:wrap"><w15:wrap {W14}><w15:wrap {W16SE} xmlns:w14="urn:inner"><w:p w14:a="1"><w:r/></w:p>{SYMBOL}</w15:wrap><w:p/></w15:wrap></w:body>"#
    ));
}

#[test]
fn a_must_understand_namespace_the_target_lacks_is_the_same_error() {
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W16SE} mc:MustUnderstand="w16se"/>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC} {W16SE} mc:MustUnderstand="w16se"><w:p/></w:body>"#
    ));
    same_as_the_tree(&format!(
        r#"<w:body xmlns:w="{W}" {MC}><w:p mc:MustUnderstand="w16se"><w:r/></w:p></w:body>"#
    ));
}

#[test]
fn the_nodes_around_the_root_pass_unchanged() {
    let xml = format!(
        r#"<?xml version="1.0"?><!--before--><w:r xmlns:w="{W}" {MC} {W16SE}>{SYMBOL}</w:r><!--after-->"#
    );
    let mut reader = PartReader::new(xml.as_bytes());
    let mut events = Processed::new(
        &mut reader,
        MarkupCompatibility::Process {
            target: OfficeVersion::Office2010,
        },
    );
    let mut seen = Vec::new();
    while let Some(event) = events.event().unwrap() {
        seen.push(event);
    }
    assert_eq!(
        seen.first(),
        Some(&PartEvent::Declaration("xml version=\"1.0\"".to_owned()))
    );
    assert_eq!(
        seen.get(1),
        Some(&PartEvent::Node(Node::Comment("before".to_owned())))
    );
    assert_eq!(
        seen.last(),
        Some(&PartEvent::Node(Node::Comment("after".to_owned())))
    );
}
