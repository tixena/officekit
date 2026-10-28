//! Refilling a chart's caches from the workbook embedded in it.

#[cfg(test)]
mod tests;

use alloc::collections::BTreeMap;
use alloc::collections::btree_map::Entry;
use alloc::string::{String, ToString as _};
use alloc::vec::Vec;

use crate::parts::{chart, shared_string_table, worksheet};
use officekit_markup::spreadsheet::{Cell, RichTextInline, RichTextRun, StringItem, Text};
use officekit_xml::element::{Attribute, Element, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::packaging::error::PackageError;
use officekit_xml::packaging::package::{OpenOptions, Package};
use officekit_xml::packaging::part_name::PartName;
use officekit_xml::packaging::relationship::{RelationshipType, Target};

use crate::embedded::{self, Embedded};
use crate::xlsx::Xlsx;

use super::Docx;

const CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DATA_REFERENCES: [&str; 5] = ["cat", "val", "xVal", "yVal", "bubbleSize"];
const MAX_COLUMN: u32 = 0x4000;
const MAX_ROW: u32 = 0x0010_0000;

/// Which cells each cache is filled from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChartRanges {
    /// The cells each `c:f` names.
    AsWritten,
    /// A reference under `c:cat`, `c:val`, `c:xVal`, `c:yVal`, or `c:bubbleSize` that names cells
    /// of one column first ends at the last row holding a value in that column, never above its
    /// first row, and its `c:f` is rewritten to say so. A series name under `c:tx` stays as
    /// written.
    ToLastRow,
}

/// A parsed `c:f`: the sheet as written (quotes kept) and as named, and the area's corners.
struct Area {
    first_column: u32,
    first_row: u32,
    last_column: u32,
    last_row: u32,
    sheet: String,
    written_sheet: String,
}

/// One worksheet's cell texts by (row, column), each value resolved: a shared string's text, an
/// inline string's text, or the `v` text of any other cell.
type Cells = BTreeMap<(u32, u32), String>;

/// The workbook embedded in a chart: its sheets by name, its shared strings, and the cells of
/// each sheet read so far.
struct Workbook {
    options: OpenOptions,
    read: BTreeMap<String, Cells>,
    shared: Vec<String>,
    sheets: Vec<(String, PartName)>,
    xlsx: Xlsx,
}

impl Area {
    /// `Sheet1!$A$2:$A$5` or `'Q1 data'!$B$2`, with or without `$`; several areas, a defined name,
    /// or anything else is `None`.
    fn parse(text: &str) -> Option<Self> {
        let (written_sheet, area) = text.trim().rsplit_once('!')?;
        let sheet = match written_sheet
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
        {
            Some(quoted) => quoted.replace("''", "'"),
            None if !written_sheet.is_empty()
                && !written_sheet.contains(['\'', '"', '(', ')', ',', ' ', ':', '!']) =>
            {
                written_sheet.to_owned()
            }
            None => return None,
        };
        let (first, last) = area.split_once(':').unwrap_or((area, area));
        let (first_column, first_row) = cell_position(first)?;
        let (last_column, last_row) = cell_position(last)?;
        Some(Self {
            first_column: first_column.min(last_column),
            first_row: first_row.min(last_row),
            last_column: first_column.max(last_column),
            last_row: first_row.max(last_row),
            sheet,
            written_sheet: written_sheet.to_owned(),
        })
    }

    fn text(&self) -> String {
        let first = column_letters(self.first_column);
        let last = column_letters(self.last_column);
        format!(
            "{}!${first}${}:${last}${}",
            self.written_sheet, self.first_row, self.last_row
        )
    }
}

impl Workbook {
    fn open(xlsx: Xlsx, options: OpenOptions) -> Result<Self, PackageError> {
        let main = xlsx.workbook_part().clone();
        let related = xlsx.package().related(Some(&main))?;
        let workbook = xlsx.workbook()?;
        let sheets = workbook
            .sheets
            .sheet_information
            .iter()
            .filter_map(|sheet| {
                let (name, id) = (sheet.name.as_ref()?, sheet.id.as_ref()?);
                related.iter().find(|part| &part.id == id).and_then(|part| {
                    if let Target::Part(target) = &part.target {
                        Some((name.clone(), target.clone()))
                    } else {
                        None
                    }
                })
            })
            .collect();
        let shared = match xlsx
            .package()
            .related_by_type(Some(&main), &RelationshipType::SHARED_STRINGS)?
            .into_iter()
            .find_map(|related_part| {
                if let Target::Part(target) = related_part.target {
                    Some(target)
                } else {
                    None
                }
            }) {
            Some(part) => shared_string_table::load(xlsx.package(), &part, options)?
                .string_item
                .iter()
                .map(string_item_text)
                .collect(),
            None => Vec::new(),
        };
        Ok(Self {
            options,
            read: BTreeMap::new(),
            shared,
            sheets,
            xlsx,
        })
    }

    fn sheet(&mut self, name: &str) -> Result<Option<&Cells>, PackageError> {
        let Some((_, part)) = self.sheets.iter().find(|(sheet, _)| sheet == name) else {
            return Ok(None);
        };
        let cells = match self.read.entry(name.to_owned()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert(read_cells(
                self.xlsx.package(),
                part,
                self.options,
                &self.shared,
            )?),
        };
        Ok(Some(cells))
    }
}

/// Refill every `c:numCache` and `c:strCache` of the chart part `name` from the workbook embedded
/// in it, and store the chart back.
///
/// # Errors
///
/// [`PackageError::Read`] when the chart embeds no workbook or the embedded document is not one,
/// and the errors of reading the workbook and the chart and of storing the chart.
pub fn refresh(docx: &mut Docx, name: &PartName, ranges: ChartRanges) -> Result<(), PackageError> {
    let workbook_part = docx
        .package
        .related_by_type(Some(name), &RelationshipType::PACKAGE)?
        .into_iter()
        .find_map(|related| {
            if let Target::Part(target) = related.target {
                Some(target)
            } else {
                None
            }
        })
        .ok_or_else(|| PackageError::Read {
            part: name.clone(),
            reason: "the chart embeds no workbook".to_owned(),
        })?;
    let Embedded::Xlsx(xlsx) = embedded::open(&docx.package, &workbook_part, docx.options)? else {
        return Err(PackageError::Read {
            part: workbook_part,
            reason: "the document the chart embeds is not a workbook".to_owned(),
        });
    };
    let mut workbook = Workbook::open(xlsx, docx.options)?;
    let mut tree = docx.package.load_tree(name, docx.options)?;
    walk(&mut tree.root, false, &mut workbook, ranges)?;
    let content_type = docx
        .package
        .content_type(name)
        .unwrap_or(chart::CONTENT_TYPE)
        .to_owned();
    docx.package
        .store_tree(name, &content_type, &tree, docx.write_options)
}

fn walk(
    element: &mut Element,
    data: bool,
    workbook: &mut Workbook,
    ranges: ChartRanges,
) -> Result<(), PackageError> {
    let text_allowed = is(element, "cat") || is(element, "xVal");
    for node in &mut element.children {
        let Node::Element(child) = node else {
            continue;
        };
        if is(child, "strRef") || is(child, "numRef") {
            refresh_reference(child, (data, text_allowed), workbook, ranges)?;
        } else {
            let inner =
                (data || DATA_REFERENCES.iter().any(|local| is(child, local))) && !is(child, "tx");
            walk(child, inner, workbook, ranges)?;
        }
    }
    Ok(())
}

fn refresh_reference(
    reference: &mut Element,
    (data, text_allowed): (bool, bool),
    workbook: &mut Workbook,
    ranges: ChartRanges,
) -> Result<(), PackageError> {
    let Some(formula_at) = position(reference, "f") else {
        return Ok(());
    };
    let Some(Node::Element(formula)) = reference.children.get_mut(formula_at) else {
        return Ok(());
    };
    let Some(mut area) = Area::parse(&text_of(formula)) else {
        return Ok(());
    };
    let Some(cells) = workbook.sheet(&area.sheet)? else {
        return Ok(());
    };
    if ranges == ChartRanges::ToLastRow && data && area.first_column == area.last_column {
        area.last_row = cells
            .keys()
            .rev()
            .find(|(_, column)| *column == area.first_column)
            .map_or(area.first_row, |(row, _)| *row)
            .max(area.first_row);
        formula.children = alloc::vec![Node::Text(area.text())];
    }
    let width = u64::from(area.last_column - area.first_column + 1);
    let count = width * u64::from(area.last_row - area.first_row + 1);
    let mut points: Vec<(u64, &String)> = cells
        .range((area.first_row, area.first_column)..=(area.last_row, area.last_column))
        .filter(|((_, column), _)| (area.first_column..=area.last_column).contains(column))
        .map(|((row, column), text)| {
            let index =
                u64::from(row - area.first_row) * width + u64::from(column - area.first_column);
            (index, text)
        })
        .collect();
    if is(reference, "numRef") && text_allowed && !all_numbers(&points) {
        to_string_reference(reference);
    }
    let numbers_only = is(reference, "numRef");
    if numbers_only {
        points.retain(|(_, text)| is_number(text));
    }
    let cache_local = if numbers_only { "numCache" } else { "strCache" };
    let prefix = reference.name.prefix.clone();
    let cache_at = position(reference, cache_local).unwrap_or_else(|| {
        let at = formula_at + 1;
        reference.children.insert(
            at,
            Node::Element(Element::new(QName::prefixed(&prefix, cache_local, CHART))),
        );
        at
    });
    if let Some(Node::Element(cache)) = reference.children.get_mut(cache_at) {
        fill_cache(cache, &prefix, count, &points);
    }
    Ok(())
}

fn is_number(text: &str) -> bool {
    text.trim().parse::<f64>().is_ok_and(f64::is_finite)
}

/// Whether every cell text of `points` is a number, as `c:numCache` takes it.
fn all_numbers(points: &[(u64, &String)]) -> bool {
    points.iter().all(|(_, text)| is_number(text))
}

/// `c:numRef` becomes `c:strRef` and its `c:numCache` a `c:strCache`, which has no `c:formatCode`.
fn to_string_reference(reference: &mut Element) {
    "strRef".clone_into(&mut reference.name.local);
    reference
        .children
        .retain(|node| !matches!(node, Node::Element(child) if is(child, "strCache")));
    for node in &mut reference.children {
        if let Node::Element(cache) = node
            && is(cache, "numCache")
        {
            "strCache".clone_into(&mut cache.name.local);
            cache
                .children
                .retain(|inner| !matches!(inner, Node::Element(child) if is(child, "formatCode")));
        }
    }
}

/// `c:ptCount` set to the cells in the area and every `c:pt` replaced, in the schema's place:
/// after `c:formatCode` and `c:ptCount`, before `c:extLst`.
fn fill_cache(cache: &mut Element, prefix: &str, count: u64, points: &[(u64, &String)]) {
    cache
        .children
        .retain(|node| !matches!(node, Node::Element(element) if is(element, "pt")));
    let count_at = position(cache, "ptCount").unwrap_or_else(|| {
        let at = position(cache, "formatCode").map_or(0, |at| at + 1);
        cache.children.insert(
            at,
            Node::Element(Element::new(QName::prefixed(prefix, "ptCount", CHART))),
        );
        at
    });
    if let Some(Node::Element(element)) = cache.children.get_mut(count_at) {
        set_attribute(element, "val", &count.to_string());
    }
    for (offset, (index, text)) in points.iter().enumerate() {
        let mut value = Element::new(QName::prefixed(prefix, "v", CHART));
        value.children.push(Node::Text((*text).clone()));
        let mut point = Element::new(QName::prefixed(prefix, "pt", CHART));
        set_attribute(&mut point, "idx", &index.to_string());
        point.children.push(Node::Element(value));
        cache
            .children
            .insert(count_at + 1 + offset, Node::Element(point));
    }
}

fn set_attribute(element: &mut Element, local: &str, value: &str) {
    if let Some(attribute) = element.attributes.iter_mut().find(|attribute| {
        attribute.name.local == local && attribute.name.namespace == Namespace::None
    }) {
        value.clone_into(&mut attribute.value);
    } else {
        element.attributes.push(Attribute::new(
            QName::unprefixed(local, ""),
            value.to_owned(),
        ));
    }
}

fn is(element: &Element, local: &str) -> bool {
    element.name.local == local && element.name.namespace == Namespace::from_uri(CHART)
}

fn position(element: &Element, local: &str) -> Option<usize> {
    element
        .children
        .iter()
        .position(|node| matches!(node, Node::Element(child) if is(child, local)))
}

fn text_of(element: &Element) -> String {
    element
        .children
        .iter()
        .filter_map(|node| {
            if let Node::Text(text) | Node::CData(text) = node {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect()
}

fn read_cells(
    package: &Package,
    part: &PartName,
    options: OpenOptions,
    shared: &[String],
) -> Result<Cells, PackageError> {
    let sheet = worksheet::load(package, part, options)?;
    Ok(sheet
        .sheet_data
        .row
        .iter()
        .flat_map(|row| &row.cell)
        .filter_map(|cell| {
            let (column, row) = cell_position(cell.r.as_deref()?)?;
            Some(((row, column), cell_text(cell, shared)?))
        })
        .collect())
}

fn cell_text(cell: &Cell, shared: &[String]) -> Option<String> {
    let value = cell.value.as_ref().map(|value| value.value.as_str());
    match cell.t.as_ref().map(|kind| kind.0.as_str()) {
        Some("s") => shared.get(value?.trim().parse::<usize>().ok()?).cloned(),
        Some("inlineStr") => cell.rich_text_inline.as_deref().map(inline_text),
        _ => value.map(str::to_owned),
    }
}

/// A string item's text: its `t`, then the `t` of each run; phonetic runs are not text.
fn string_item_text(item: &StringItem) -> String {
    text_and_runs(item.text.as_deref(), &item.rich_text_run)
}

fn inline_text(item: &RichTextInline) -> String {
    text_and_runs(item.text.as_deref(), &item.rich_text_run)
}

fn text_and_runs(text: Option<&Text>, runs: &[RichTextRun]) -> String {
    text.map(|plain| plain.value.as_str())
        .into_iter()
        .chain(runs.iter().map(|run| run.text.value.as_str()))
        .collect()
}

/// The column and row of a cell reference such as `$B$2`, whose `$` are optional.
fn cell_position(reference: &str) -> Option<(u32, u32)> {
    let plain = reference.replace('$', "");
    let split = plain.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = plain.split_at(split);
    if letters.is_empty() || !letters.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let column = letters.bytes().try_fold(0_u32, |number, letter| {
        number
            .checked_mul(26)?
            .checked_add(u32::from(letter - b'A' + 1))
    })?;
    let row: u32 = digits.parse().ok()?;
    ((1..=MAX_COLUMN).contains(&column) && (1..=MAX_ROW).contains(&row)).then_some((column, row))
}

fn column_letters(mut number: u32) -> String {
    let mut letters = Vec::new();
    while number > 0 {
        let rest = (number - 1) % 26;
        letters.push(char::from(b'A' + u8::try_from(rest).unwrap_or(0)));
        number = (number - 1) / 26;
    }
    letters.iter().rev().collect()
}
