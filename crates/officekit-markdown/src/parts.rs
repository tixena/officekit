//! The styles part every converted document gets, Word's built-in styles by their built-in ids
//! and names, and the numbering part a document with a list gets.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use officekit::parts::{numbering_definitions, style_definitions};
use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    AbstractNumberingDefinition, AbstractNumberingDefinitionReference, AssociatedOutlineLevel,
    Bold, BorderStylesMember, ComplexScriptFontSize, Italics, KeepParagraphWithNextParagraph,
    LeftParagraphBorder, NonComplexScriptFontSize, NumberingDefinitionInstance,
    NumberingDefinitions, NumberingFormat, NumberingLevelDefinitionOverride,
    NumberingLevelOverrideDefinition, NumberingLevelStartingValueOverride, NumberingLevelText,
    ParagraphBorderAboveIdenticalParagraphs, ParagraphBorderBelowIdenticalParagraphs,
    ParagraphIndentation, ParagraphPropertiesPPrGeneral, ParentStyleID, PreviousTableProperties,
    PrimaryStyleName, RightParagraphBorder, RunContentColor, RunFonts, RunProperties,
    RunPropertiesContent, SpacingBetweenLinesAndAboveBelowParagraph, StNumberFormatMember,
    StartingValue, StyleDefinition, StyleDefinitions, StyleTypesMember, TableBorders,
    TableInsideHorizontalEdgesBorder, TableInsideVerticalEdgesBorder, Underline,
    UnderlinePatternsMember,
};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::Package;
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::write::WriteOptions;

/// A single border of half a point, flush with the text, of the color Word picks: a `w:top`,
/// `w:insideH`, or another border element of `$kind`.
macro_rules! single_border {
    ($kind:ty) => {{
        let mut border = <$kind>::default();
        border.color = Some("auto".to_owned().into());
        border.space = Some("0".to_owned().into());
        border.sz = Some("4".to_owned().into());
        border.val = Some(BorderStylesMember::Single.into());
        Box::new(border)
    }};
}

/// The numbering instance every bullet list takes.
pub const BULLET_INSTANCE: usize = 1;

/// The texts of the bullet levels, in turn: `•`, `◦` and `▪`.
const BULLETS: [&str; 3] = ["\u{2022}", "\u{25e6}", "\u{25aa}"];

/// The character style of inline code.
pub const CODE: &str = "HTMLCode";

/// The deepest list level Word has, counting from 0; deeper lists take it.
pub const DEEPEST_LEVEL: usize = 8;

/// The character style of a link's text.
pub const HYPERLINK: &str = "Hyperlink";

/// The paragraph style of the blocks of a list item after its first paragraph.
pub const LIST_PARAGRAPH: &str = "ListParagraph";

const NUMBERING_PART: &str = "/word/numbering.xml";

/// The paragraph style of each line of a code block.
pub const PREFORMATTED: &str = "HTMLPreformatted";

/// The paragraph style of each line of a block quote.
pub const QUOTE: &str = "Quote";

/// The table style of every table.
pub const TABLE_GRID: &str = "TableGrid";

/// The size of each heading level's text in half-points, from `Heading1` to `Heading9`.
const HEADING_SIZES: [&str; 9] = ["40", "32", "28", "26", "24", "22", "22", "22", "22"];

/// The deepest heading level with a style of its own; deeper sections take its style.
pub const DEEPEST_HEADING: usize = HEADING_SIZES.len();

const NORMAL: &str = "Normal";

const STYLES_PART: &str = "/word/styles.xml";

/// Store the numbering part at `/word/numbering.xml`, related from `main`: the bullet levels, the
/// decimal levels, instance [`BULLET_INSTANCE`] for every bullet list, and one instance for each
/// ordered list, starting at its first number in `starts` at the level it is nested at.
///
/// # Errors
///
/// The errors of [`numbering_definitions::store_transitional`] and [`Package::relate`].
pub fn add_numbering(
    package: &mut Package,
    main: &PartName,
    starts: &[(u64, usize)],
) -> Result<(), PackageError> {
    let name = PartName::new(NUMBERING_PART)?;
    numbering_definitions::store_transitional(
        package,
        &name,
        &numbering(starts),
        WriteOptions::default(),
    )?;
    package
        .relate(
            Some(main),
            &name,
            numbering_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .map(|_| ())
}

/// Store the styles part at `/word/styles.xml`, related from `main`.
///
/// # Errors
///
/// The errors of [`style_definitions::store_transitional`] and [`Package::relate`].
pub fn add_styles(package: &mut Package, main: &PartName) -> Result<(), PackageError> {
    let name = PartName::new(STYLES_PART)?;
    style_definitions::store_transitional(package, &name, &styles(), WriteOptions::default())?;
    package
        .relate(
            Some(main),
            &name,
            style_definitions::RELATIONSHIP_TYPE.transitional,
        )
        .map(|_| ())
}

/// The id of the paragraph style of a heading at `level`, from 1 to [`DEEPEST_HEADING`].
pub fn heading(level: usize) -> String {
    format!("Heading{level}")
}

/// The left indent of list level `level`, from 0, in twentieths of a point: half an inch per
/// level.
pub fn level_indent(level: usize) -> String {
    (720 * (level + 1)).to_string()
}

/// The numbering instance of the ordered list at `index`, counting from 0 in document order.
pub const fn ordered_instance(index: usize) -> usize {
    index + BULLET_INSTANCE + 1
}

/// An abstract numbering of nine levels, `format` and `text` giving each level's.
fn abstract_numbering<F>(
    id: usize,
    format: StNumberFormatMember,
    text: F,
) -> AbstractNumberingDefinition
where
    F: Fn(usize) -> String,
{
    let mut definition = AbstractNumberingDefinition::default();
    definition.abstract_num_id = Some(id.to_string().into());
    definition.numbering_level_override_definition = (0..=DEEPEST_LEVEL)
        .map(|level| numbering_level(level, format, text(level)))
        .collect();
    definition
}

/// The run's fonts: `name` for every script but East Asian text.
fn fonts(name: &str) -> RunPropertiesContent {
    let mut fonts = RunFonts::default();
    fonts.ascii = Some(name.to_owned());
    fonts.cs = Some(name.to_owned());
    fonts.h_ansi = Some(name.to_owned());
    RunPropertiesContent::RunFonts(fonts)
}

/// `w:sz` and `w:szCs` of `half_points`.
fn font_size(half_points: &str) -> [RunPropertiesContent; 2] {
    let mut latin = NonComplexScriptFontSize::default();
    latin.val = Some(half_points.to_owned().into());
    let mut complex = ComplexScriptFontSize::default();
    complex.val = Some(half_points.to_owned().into());
    [
        RunPropertiesContent::NonComplexScriptFontSize(latin),
        RunPropertiesContent::ComplexScriptFontSize(complex),
    ]
}

/// A paragraph style based on `Normal`.
fn based_on_normal(
    id: &str,
    name: &str,
    paragraph: Option<ParagraphPropertiesPPrGeneral>,
    run: Vec<RunPropertiesContent>,
) -> StyleDefinition {
    let mut parent = ParentStyleID::default();
    parent.val = Some(NORMAL.to_owned());
    let mut definition = style(StyleTypesMember::Paragraph, id, name);
    definition.paragraph_properties_p_pr_general = paragraph.map(Box::new);
    definition.parent_style_id = Some(Box::new(parent));
    definition.run_properties = run_properties(run);
    definition
}

fn heading_style(level: usize, half_points: &str) -> StyleDefinition {
    let mut outline = AssociatedOutlineLevel::default();
    outline.val = Some((level - 1).to_string().into());
    let mut paragraph = ParagraphPropertiesPPrGeneral::default();
    paragraph.associated_outline_level = Some(Box::new(outline));
    paragraph.keep_paragraph_with_next_paragraph =
        Some(Box::new(KeepParagraphWithNextParagraph::default()));
    paragraph.spacing_between_lines_and_above_below_paragraph = Some(spacing(Some("240"), "120"));
    let [latin, complex] = font_size(half_points);
    let run = alloc::vec![RunPropertiesContent::Bold(Bold::default()), latin, complex];
    based_on_normal(
        &heading(level),
        &format!("heading {level}"),
        Some(paragraph),
        run,
    )
}

fn normal() -> StyleDefinition {
    let mut paragraph = ParagraphPropertiesPPrGeneral::default();
    paragraph.spacing_between_lines_and_above_below_paragraph = Some(spacing(None, "160"));
    let [latin, complex] = font_size("22");
    let mut definition = style(StyleTypesMember::Paragraph, NORMAL, NORMAL);
    definition.default = Some(OnOffValue::from(true));
    definition.paragraph_properties_p_pr_general = Some(Box::new(paragraph));
    definition.run_properties = run_properties(alloc::vec![fonts("Calibri"), latin, complex]);
    definition
}

/// The root of the numbering part: abstract numbering 0 of bullets, abstract numbering 1 of
/// decimal numbers, the bullet instance, and an instance for each ordered list.
fn numbering(starts: &[(u64, usize)]) -> NumberingDefinitions {
    let bullets = abstract_numbering(0, StNumberFormatMember::Bullet, |level| {
        BULLETS[level % BULLETS.len()].to_owned()
    });
    let decimals = abstract_numbering(1, StNumberFormatMember::Decimal, |level| {
        format!("%{}.", level + 1)
    });
    let mut instances = alloc::vec![numbering_instance(BULLET_INSTANCE, 0)];
    for (index, &(start, depth)) in starts.iter().enumerate() {
        let mut first = NumberingLevelStartingValueOverride::default();
        first.val = Some(start.to_string().into());
        let mut level = NumberingLevelDefinitionOverride::default();
        level.ilvl = Some(depth.to_string().into());
        level.numbering_level_starting_value_override = Some(Box::new(first));
        let mut instance = numbering_instance(ordered_instance(index), 1);
        instance.numbering_level_definition_override = alloc::vec![level];
        instances.push(instance);
    }
    let mut root = NumberingDefinitions::default();
    root.abstract_numbering_definition = alloc::vec![bullets, decimals];
    root.numbering_definition_instance = instances;
    root
}

/// Numbering instance `id`, on abstract numbering `abstract_id`.
fn numbering_instance(id: usize, abstract_id: usize) -> NumberingDefinitionInstance {
    let mut reference = AbstractNumberingDefinitionReference::default();
    reference.val = Some(abstract_id.to_string().into());
    let mut instance = NumberingDefinitionInstance::default();
    instance.abstract_numbering_definition_reference = Box::new(reference);
    instance.num_id = Some(id.to_string().into());
    instance
}

/// Level `level` of an abstract numbering, from 0, in `format`, its marker `text`, indented by
/// [`level_indent`] with a hanging indent of a quarter inch.
fn numbering_level(
    level: usize,
    format: StNumberFormatMember,
    text: String,
) -> NumberingLevelOverrideDefinition {
    let mut start = StartingValue::default();
    start.val = Some("1".to_owned().into());
    let mut number_format = NumberingFormat::default();
    number_format.val = Some(format.into());
    let mut marker = NumberingLevelText::default();
    marker.val = Some(text);
    let mut indentation = ParagraphIndentation::default();
    indentation.hanging = Some("360".to_owned().into());
    indentation.left = Some(level_indent(level).into());
    let mut paragraph = ParagraphPropertiesPPrGeneral::default();
    paragraph.paragraph_indentation = Some(Box::new(indentation));
    let mut definition = NumberingLevelOverrideDefinition::default();
    definition.ilvl = Some(level.to_string().into());
    definition.numbering_format = Some(Box::new(number_format));
    definition.numbering_level_text = Some(Box::new(marker));
    definition.paragraph_properties_p_pr_general = Some(Box::new(paragraph));
    definition.starting_value = Some(Box::new(start));
    definition
}

/// Run properties of `content`, `None` when it is empty.
fn run_properties(content: Vec<RunPropertiesContent>) -> Option<Box<RunProperties>> {
    (!content.is_empty()).then(|| {
        let mut properties = RunProperties::default();
        properties.content = content;
        Box::new(properties)
    })
}

/// `w:spacing`, `before` and `after` in twentieths of a point.
fn spacing(before: Option<&str>, after: &str) -> Box<SpacingBetweenLinesAndAboveBelowParagraph> {
    let mut spacing = SpacingBetweenLinesAndAboveBelowParagraph::default();
    spacing.after = Some(after.to_owned().into());
    spacing.before = before.map(|twips| twips.to_owned().into());
    Box::new(spacing)
}

/// A style of `kind` with its id and name, and nothing else.
fn style(kind: StyleTypesMember, id: &str, name: &str) -> StyleDefinition {
    let mut primary = PrimaryStyleName::default();
    primary.val = Some(name.to_owned());
    let mut definition = StyleDefinition::default();
    definition.primary_style_name = Some(Box::new(primary));
    definition.r#type = Some(kind.into());
    definition.style_id = Some(id.to_owned());
    definition
}

/// The root of the styles part: `Normal`, the nine headings, `Quote`, `HTMLPreformatted`,
/// `ListParagraph`, `HTMLCode`, `Hyperlink`, and `TableGrid`.
fn styles() -> StyleDefinitions {
    let mut definitions = alloc::vec![normal()];
    for (index, half_points) in HEADING_SIZES.iter().enumerate() {
        definitions.push(heading_style(index + 1, half_points));
    }

    let mut indentation = ParagraphIndentation::default();
    indentation.left = Some("720".to_owned().into());
    let mut quote = ParagraphPropertiesPPrGeneral::default();
    quote.paragraph_indentation = Some(Box::new(indentation));
    definitions.push(based_on_normal(
        QUOTE,
        QUOTE,
        Some(quote),
        alloc::vec![RunPropertiesContent::Italics(Italics::default())],
    ));

    let mut preformatted = ParagraphPropertiesPPrGeneral::default();
    preformatted.spacing_between_lines_and_above_below_paragraph = Some(spacing(None, "0"));
    let [latin, complex] = font_size("20");
    definitions.push(based_on_normal(
        PREFORMATTED,
        "HTML Preformatted",
        Some(preformatted),
        alloc::vec![fonts("Consolas"), latin, complex],
    ));

    definitions.push(based_on_normal(
        LIST_PARAGRAPH,
        "List Paragraph",
        None,
        Vec::new(),
    ));

    let [code_latin, code_complex] = font_size("20");
    let mut code = style(StyleTypesMember::Character, CODE, "HTML Code");
    code.run_properties = run_properties(alloc::vec![fonts("Consolas"), code_latin, code_complex]);
    definitions.push(code);

    let mut color = RunContentColor::default();
    color.val = Some("0563C1".to_owned().into());
    let mut underline = Underline::default();
    underline.val = Some(UnderlinePatternsMember::Single.into());
    let mut hyperlink = style(StyleTypesMember::Character, HYPERLINK, HYPERLINK);
    hyperlink.run_properties = run_properties(alloc::vec![
        RunPropertiesContent::RunContentColor(color),
        RunPropertiesContent::Underline(underline),
    ]);
    definitions.push(hyperlink);

    definitions.push(table_grid());

    let mut styles = StyleDefinitions::default();
    styles.style_definition = definitions;
    styles
}

/// `TableGrid`: a single border of half a point on every edge and between every cell.
fn table_grid() -> StyleDefinition {
    let mut borders = TableBorders::default();
    borders.left_paragraph_border = Some(single_border!(LeftParagraphBorder));
    borders.paragraph_border_above_identical_paragraphs =
        Some(single_border!(ParagraphBorderAboveIdenticalParagraphs));
    borders.paragraph_border_below_identical_paragraphs =
        Some(single_border!(ParagraphBorderBelowIdenticalParagraphs));
    borders.right_paragraph_border = Some(single_border!(RightParagraphBorder));
    borders.table_inside_horizontal_edges_border =
        Some(single_border!(TableInsideHorizontalEdgesBorder));
    borders.table_inside_vertical_edges_border =
        Some(single_border!(TableInsideVerticalEdgesBorder));
    let mut table = PreviousTableProperties::default();
    table.table_borders = Some(Box::new(borders));
    let mut definition = style(StyleTypesMember::Table, TABLE_GRID, "Table Grid");
    definition.previous_table_properties = Some(Box::new(table));
    definition
}
