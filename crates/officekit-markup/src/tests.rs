use core::mem::{size_of, size_of_val};

use crate::wordprocessing::{
    self, Document, Paragraph, ParagraphContent, ParagraphWith, TextRun, TextWith,
};
use crate::{UserData, user_data};

#[derive(Clone, Debug, Default, PartialEq)]
struct Marks;

#[user_data]
impl UserData for Marks {
    type WordprocessingParagraph = Vec<(u32, f32)>;
}

fn eq<T>()
where
    T: Eq,
{
}

#[test]
fn a_node_defaults_with_nothing_naming_the_user_data_type() {
    let paragraph = Paragraph::default();
    assert_eq!(size_of_val(&paragraph.user_data), 0);
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
fn user_data_fills_the_named_kind_and_leaves_the_others_empty() {
    let mut paragraph = ParagraphWith::<Marks>::default();
    paragraph.user_data.push((2, 72.5));
    assert_eq!(paragraph.clone(), paragraph);
    assert!(format!("{paragraph:?}").contains("72.5"));
    let text: TextWith<Marks> = TextWith::default();
    assert_eq!(size_of_val(&text.user_data), 0);
}
