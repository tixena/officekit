use core::mem::{size_of, size_of_val};

use officekit_xml::element::{Element, Node};
use officekit_xml::write::Scope;

use crate::drawing::AnyElementContent;
use crate::office_drawing_2017_model3d::{self as model3d, Model3DExtensionContent};
use crate::wordprocessing::{
    self, Document, Paragraph, ParagraphContent, ParagraphWith, TextRun, TextWith,
};
use crate::{UserData, chart, drawing, spreadsheet, user_data};

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
    assert_eq!(size_of::<Paragraph>(), 232);
    assert_eq!(size_of::<Document>(), 48);
    assert_eq!(size_of::<TextRun>(), 112);
}

#[test]
fn a_node_holds_its_unknown_content_behind_one_pointer() {
    assert_eq!(size_of::<wordprocessing::NonComplexScriptFontSize>(), 32);
    assert_eq!(size_of::<wordprocessing::RunFonts>(), 224);
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

/// The elements under `element`, at any depth, by qualified name.
fn element_names(element: &Element) -> Vec<String> {
    element
        .child_elements()
        .flat_map(|child| {
            let mut names = vec![format!("{}:{}", child.name.prefix, child.name.local)];
            names.extend(element_names(child));
            names
        })
        .collect()
}

#[test]
fn a_required_wildcard_position_defaults_to_no_element() {
    let nothing = || Node::Text(String::new());
    assert_eq!(
        AnyElementContent::default(),
        AnyElementContent::Other(nothing())
    );
    assert_eq!(
        *chart::Extension::default().content,
        AnyElementContent::Other(nothing())
    );
    assert_eq!(
        *spreadsheet::Extension::default().content,
        AnyElementContent::Other(nothing())
    );
    assert_eq!(
        *model3d::Ext::default().content,
        Model3DExtensionContent::Other(nothing())
    );
}

#[test]
fn an_extension_written_from_its_default_holds_no_element() {
    let written = [
        chart::write::extension(&chart::Extension::default(), &mut Scope::default()),
        drawing::write::extension(&drawing::Extension::default(), &mut Scope::default()),
        spreadsheet::write::extension(&spreadsheet::Extension::default(), &mut Scope::default()),
        model3d::write::ext(&model3d::Ext::default(), &mut Scope::default()),
    ];
    for element in written {
        assert_eq!(element_names(&element), Vec::<String>::new(), "{element:?}");
    }
}

#[test]
fn a_required_choice_position_defaults_to_its_first_alternative() {
    assert!(matches!(
        *chart::CategoryAxisDataCat::default().content,
        chart::AxisDataSourceContent::MultiLevelStringReference(_)
    ));
}
