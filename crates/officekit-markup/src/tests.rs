use core::mem::{size_of, size_of_val};

use crate::wordprocessing::{
    self, Document, Paragraph, ParagraphContent, ParagraphWith, TextRun, TextWith,
};
use crate::{NodeExtra, node_extra};

#[derive(Clone, Debug, Default, PartialEq)]
struct Marks;

#[node_extra]
impl NodeExtra for Marks {
    type WordprocessingParagraph = Vec<(u32, f32)>;
}

fn eq<T>()
where
    T: Eq,
{
}

#[test]
fn a_node_defaults_with_nothing_naming_the_extra_type() {
    let paragraph = Paragraph::default();
    assert_eq!(size_of_val(&paragraph.extra), 0);
}

#[test]
fn the_plain_names_are_the_generic_types_holding_nothing() {
    let mut paragraph = Paragraph::default();
    paragraph
        .content
        .push(ParagraphContent::TextRun(TextRun::default()));
    let copy: ParagraphWith<()> = paragraph.clone();
    assert_eq!(copy, paragraph);
    let content: wordprocessing::ParagraphContentWith<()> =
        ParagraphContent::TextRun(TextRun::default());
    assert_eq!(content, paragraph.content[0]);
}

#[test]
fn the_plain_tree_is_eq() {
    eq::<Paragraph>();
    eq::<ParagraphContent>();
    eq::<Document>();
}

#[test]
fn the_slot_adds_nothing_to_the_size() {
    assert_eq!(size_of::<Paragraph>(), 320);
    assert_eq!(size_of::<Document>(), 136);
    assert_eq!(size_of::<TextRun>(), 200);
}

#[test]
fn node_extra_fills_the_named_kind_and_leaves_the_others_empty() {
    let mut paragraph = ParagraphWith::<Marks>::default();
    paragraph.extra.push((2, 72.5));
    assert_eq!(paragraph.clone(), paragraph);
    assert!(format!("{paragraph:?}").contains("72.5"));
    let text: TextWith<Marks> = TextWith::default();
    assert_eq!(size_of_val(&text.extra), 0);
}
