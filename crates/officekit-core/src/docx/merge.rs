//! Bringing another document's body into this one: the blocks come back rewritten so that every
//! id they use resolves in this file, and the parts and entries those ids name are copied into
//! this package as they are met.

#[cfg(test)]
mod tests;

use alloc::collections::{BTreeMap, BTreeSet};

use officekit_markup::wordprocessing::{self, BlockContent, Document, DocumentBody};
use officekit_packaging::error::PackageError;
use officekit_packaging::package::{DECLARATION, OpenOptions, Package};
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelationshipType, Target};
use officekit_xml::element::{Attribute, Document as Part, Element, NamespaceDeclaration, Node};
use officekit_xml::name::{Namespace, QName};
use officekit_xml::write::Scope;

use super::Docx;
use crate::parts::{
    endnotes, footnotes, numbering_definitions, style_definitions, wordprocessing_comments,
};

/// The `w` elements whose attribute names an entry of an id space.
const ENTRY_REFERENCES: [(&str, &str, Space); 8] = [
    ("abstractNumId", "val", Space::AbstractNum),
    ("commentRangeEnd", "id", Space::Comment),
    ("commentRangeStart", "id", Space::Comment),
    ("commentReference", "id", Space::Comment),
    ("endnoteReference", "id", Space::Endnote),
    ("footnoteReference", "id", Space::Footnote),
    ("lvlPicBulletId", "val", Space::PictureBullet),
    ("numId", "val", Space::Num),
];

const MARKUP_COMPATIBILITY: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// The numbering part's entries in its schema's order; a copied entry goes after the last of its
/// kind.
const NUMBERING_ORDER: [&str; 4] = ["numPicBullet", "abstractNum", "num", "numIdMacAtCleanup"];

/// The content type of a copied part its source gives none.
const OCTET_STREAM: &str = "application/octet-stream";

/// The elements whose `w:id` SDK 3.5.1 requires unique among the elements of the same name in a
/// part: bookmarks, permission ranges, and revision marks.
const RANGE_IDS: [&str; 32] = [
    "bookmarkEnd",
    "bookmarkStart",
    "cellDel",
    "cellIns",
    "cellMerge",
    "customXmlDelRangeEnd",
    "customXmlDelRangeStart",
    "customXmlInsRangeEnd",
    "customXmlInsRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveToRangeEnd",
    "customXmlMoveToRangeStart",
    "del",
    "ins",
    "moveFrom",
    "moveFromRangeEnd",
    "moveFromRangeStart",
    "moveTo",
    "moveToRangeEnd",
    "moveToRangeStart",
    "numberingChange",
    "pPrChange",
    "permEnd",
    "permStart",
    "rPrChange",
    "sectPrChange",
    "tblGridChange",
    "tblPrChange",
    "tblPrExChange",
    "tcPrChange",
    "trPrChange",
];

const RELATIONSHIPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The `w` elements whose `w:val` names a style.
const STYLE_REFERENCES: [&str; 8] = [
    "basedOn",
    "link",
    "next",
    "numStyleLink",
    "pStyle",
    "rStyle",
    "styleLink",
    "tblStyle",
];

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const WORDPROCESSING_DRAWING: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";

/// A part of entries the import copies into: the source's, and this file's as the import changes
/// it.
struct Entries {
    /// Whether an entry went in: only then is this file's part written back.
    changed: bool,
    source: PartName,
    source_tree: Part,
    /// This file's part; `None` until the part the import makes for this file is added.
    target: Option<PartName>,
    tree: Part,
}

/// One import: the counters and maps each id space keeps, and the parts copied so far.
struct Import<'docx> {
    /// Each kind of entry part once opened; `None` when the source relates no such part.
    entries: BTreeMap<Kind, Option<Entries>>,
    /// The source id each copied entry had, by id space, to the id it has here.
    mapped: BTreeMap<(Space, String), String>,
    /// The id the next copied entry of each space takes.
    next: BTreeMap<Space, i64>,
    next_drawing: i64,
    next_range: i64,
    /// Each source part copied, by its folded name, to its copy.
    parts: BTreeMap<String, PartName>,
    /// Each range id met to the one it has here.
    ranges: BTreeMap<String, String>,
    /// Each relationship id met, with its source part's folded name, to the id that names its
    /// target here.
    relationships: BTreeMap<(String, String), String>,
    source: &'docx Docx,
    /// The style ids met.
    styles: BTreeSet<String>,
    target: &'docx mut Docx,
}

/// A part of entries, related from the main document part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Comments,
    Endnotes,
    Footnotes,
    Numbering,
    Styles,
}

/// An id space of entries: the comments, footnotes, endnotes, and each numbering entry kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Space {
    AbstractNum,
    Comment,
    Endnote,
    Footnote,
    Num,
    PictureBullet,
}

impl<'docx> Import<'docx> {
    /// Put a copied entry into this file's part: a numbering entry after the last entry of its kind
    /// in the schema's order, any other at the end.
    fn add(&mut self, kind: Kind, entry: Element) {
        let Some(Some(entries)) = self.entries.get_mut(&kind) else {
            return;
        };
        let children = &mut entries.tree.root.children;
        let rank = |local: &str| NUMBERING_ORDER.iter().position(|order| *order == local);
        let ranked = |node: &Node| {
            if let Node::Element(element) = node
                && is_w(element)
            {
                rank(&element.name.local)
            } else {
                None
            }
        };
        let at = if kind == Kind::Numbering
            && let Some(own) = rank(&entry.name.local)
        {
            children
                .iter()
                .rposition(|node| ranked(node).is_some_and(|other| other <= own))
                .map_or_else(
                    || {
                        children
                            .iter()
                            .position(|node| ranked(node).is_some())
                            .unwrap_or(children.len())
                    },
                    |last| last + 1,
                )
        } else {
            children.len()
        };
        children.insert(at, Node::Element(entry));
    }

    /// Copy a source part under the first free name of its pattern, with its content type, and its
    /// relationship part under the same ids, copying each part it reaches the same way; a part
    /// reached twice is copied once.
    fn copy_part(&mut self, part: &PartName) -> Result<PartName, PackageError> {
        if let Some(copy) = self.parts.get(&part.folded()) {
            return Ok(copy.clone());
        }
        let copy = self.target.package.free_part_name(&pattern(part))?;
        let content_type = self
            .source
            .package
            .content_type(part)
            .unwrap_or(OCTET_STREAM)
            .to_owned();
        let bytes = self.source.package.part_bytes(part)?;
        self.target
            .package
            .set_part_bytes(&copy, &content_type, &bytes)?;
        self.parts.insert(part.folded(), copy.clone());
        for related in self.source.package.related(Some(part))? {
            let target = if let Target::Part(inner) = related.target {
                Target::Part(self.copy_part(&inner)?)
            } else {
                related.target
            };
            self.target.package.add_relationship(
                Some(&copy),
                &related.id,
                &target,
                &related.type_uri,
            )?;
        }
        Ok(copy)
    }

    /// The id the source's entry `id` of `space` has here: the entry copied under the space's next
    /// id, its content rewritten. An id the source's part does not hold, and `w:numId` 0, which
    /// means no numbering, stay.
    fn entry(&mut self, space: Space, id: &str) -> Result<String, PackageError> {
        if space == Space::Num && id == "0" {
            return Ok(id.to_owned());
        }
        let key = (space, id.to_owned());
        if let Some(mapped) = self.mapped.get(&key) {
            return Ok(mapped.clone());
        }
        let kind = space.kind();
        self.open(kind)?;
        let Some((source_part, mut entry)) = self.source_entry(kind, |element| {
            is_w_named(element, space.entry()) && w_value(element, space.id_attribute()) == Some(id)
        }) else {
            return Ok(id.to_owned());
        };
        let Some(target_part) = self.target_part(kind)? else {
            return Ok(id.to_owned());
        };
        let next = self.next.entry(space).or_default();
        let mapped = next.to_string();
        *next += 1;
        self.mapped.insert(key, mapped.clone());
        if let Some(attribute) = w_attribute(&mut entry, space.id_attribute()) {
            mapped.clone_into(&mut attribute.value);
        }
        self.rewrite_children(&mut entry, &source_part, &target_part)?;
        self.add(kind, entry);
        Ok(mapped)
    }

    /// Write back every part the import added entries to, in this file's class.
    fn finish(self) -> Result<(), PackageError> {
        let class = self.target.class;
        let options = self.target.write_options;
        for (kind, opened) in self.entries {
            if let Some(mut entries) = opened
                && entries.changed
                && let Some(name) = &entries.target
            {
                entries.tree.root.set_class(class);
                let content_type = self
                    .target
                    .package
                    .content_type(name)
                    .unwrap_or_else(|| kind.content_type())
                    .to_owned();
                self.target
                    .package
                    .store_tree(name, &content_type, &entries.tree, options)?;
            }
        }
        Ok(())
    }

    /// An import into `target`, whose main document is `document`, from `source`: the range ids
    /// start above every one this main document and its comments, footnotes, and endnotes parts
    /// use, the drawing ids above every one this main document uses.
    fn new(
        source: &'docx Docx,
        target: &'docx mut Docx,
        document: &Document,
    ) -> Result<Self, PackageError> {
        let written = wordprocessing::write::document(document, &mut Scope::default());
        let mut highest_range = highest(&written, &is_range, &Namespace::from_uri(W));
        for kind in [Kind::Comments, Kind::Endnotes, Kind::Footnotes] {
            if let Some(part) = first_part(&target.package, &target.main, kind)? {
                let tree = target.package.load_tree(&part, OpenOptions::default())?;
                highest_range =
                    highest_range.max(highest(&tree.root, &is_range, &Namespace::from_uri(W)));
            }
        }
        let highest_drawing = highest(&written, &is_drawing, &Namespace::None);
        Ok(Self {
            entries: BTreeMap::new(),
            mapped: BTreeMap::new(),
            next: BTreeMap::new(),
            next_drawing: highest_drawing.max(0) + 1,
            next_range: highest_range + 1,
            parts: BTreeMap::new(),
            ranges: BTreeMap::new(),
            relationships: BTreeMap::new(),
            source,
            styles: BTreeSet::new(),
            target,
        })
    }

    /// Read the source's part of `kind` and this file's, or start a new one for this file whose
    /// root declares what the source's root declares, holding, for footnotes and endnotes, the
    /// source's separator entries under their own ids. Each id space of the kind continues above
    /// the highest id this file's part holds.
    fn open(&mut self, kind: Kind) -> Result<(), PackageError> {
        if self.entries.contains_key(&kind) {
            return Ok(());
        }
        let Some(source) = first_part(&self.source.package, &self.source.main, kind)? else {
            self.entries.insert(kind, None);
            return Ok(());
        };
        let source_tree = self
            .source
            .package
            .load_tree(&source, self.source.options)?;
        let (target, tree) = match first_part(&self.target.package, &self.target.main, kind)? {
            Some(name) => {
                let tree = self
                    .target
                    .package
                    .load_tree(&name, OpenOptions::default())?;
                (Some(name), tree)
            }
            None => (None, new_tree(kind, &source_tree.root)),
        };
        for space in kind.spaces() {
            let highest = tree
                .root
                .child_elements()
                .filter(|element| is_w_named(element, space.entry()))
                .filter_map(|element| w_value(element, space.id_attribute())?.parse::<i64>().ok())
                .max()
                .unwrap_or(-1);
            // `w:numId` 0 means no numbering, so lists start at 1.
            let lowest = i64::from(*space == Space::Num);
            self.next.insert(*space, (highest + 1).max(lowest));
        }
        self.entries.insert(
            kind,
            Some(Entries {
                changed: false,
                source,
                source_tree,
                target,
                tree,
            }),
        );
        Ok(())
    }

    /// The id a range id has here: the next value of the range counter, the same for each
    /// occurrence of one source id.
    fn range(&mut self, id: &str) -> String {
        if id.parse::<i64>().is_err() {
            return id.to_owned();
        }
        if let Some(mapped) = self.ranges.get(id) {
            return mapped.clone();
        }
        let mapped = self.next_range.to_string();
        self.next_range += 1;
        self.ranges.insert(id.to_owned(), mapped.clone());
        mapped
    }

    /// The id that names here, from `to`, what the relationship `id` of the source part `from`
    /// names: an external target related again with the same type, a part copied and related.
    fn relationship(
        &mut self,
        from: &PartName,
        to: &PartName,
        id: &str,
    ) -> Result<String, PackageError> {
        let key = (from.folded(), id.to_owned());
        if let Some(mapped) = self.relationships.get(&key) {
            return Ok(mapped.clone());
        }
        let Some(related) = self
            .source
            .package
            .related(Some(from))?
            .into_iter()
            .find(|related| related.id == id)
        else {
            return Ok(id.to_owned());
        };
        let mapped = if let Target::Part(part) = &related.target {
            let copy = self.copy_part(part)?;
            self.target
                .package
                .relate(Some(to), &copy, &related.type_uri)?
        } else if let Target::External(uri) = &related.target {
            self.target
                .package
                .relate_external(Some(to), uri, &related.type_uri)?
        } else {
            return Ok(id.to_owned());
        };
        self.relationships.insert(key, mapped.clone());
        Ok(mapped)
    }

    /// Rewrite one element of copied markup, then its children; `from` is the source part it came
    /// from and `to` the part of this file it goes to.
    fn rewrite(
        &mut self,
        element: &mut Element,
        from: &PartName,
        to: &PartName,
    ) -> Result<(), PackageError> {
        let relationships = Namespace::from_uri(RELATIONSHIPS);
        for attribute in &mut element.attributes {
            if attribute.name.namespace == relationships {
                attribute.value = self.relationship(from, to, &attribute.value)?;
            }
        }
        if is_w(element) {
            let local = element.name.local.as_str();
            if let Some(&(_, attribute_local, space)) =
                ENTRY_REFERENCES.iter().find(|(name, _, _)| *name == local)
            {
                if let Some(attribute) = w_attribute(element, attribute_local) {
                    attribute.value = self.entry(space, &attribute.value)?;
                }
            } else if STYLE_REFERENCES.contains(&local) {
                if let Some(id) = w_value(element, "val").map(str::to_owned) {
                    self.style(&id)?;
                }
            } else if RANGE_IDS.contains(&local)
                && let Some(attribute) = w_attribute(element, "id")
            {
                attribute.value = self.range(&attribute.value);
            } else {
                // Every other attribute is copied as it is.
            }
        } else if is_drawing(element)
            && let Some(attribute) = element.attributes.iter_mut().find(|attribute| {
                attribute.name.namespace == Namespace::None && attribute.name.local == "id"
            })
        {
            attribute.value = self.next_drawing.to_string();
            self.next_drawing += 1;
        } else {
            // Not an element whose ids the import maps.
        }
        self.rewrite_children(element, from, to)
    }

    fn rewrite_children(
        &mut self,
        element: &mut Element,
        from: &PartName,
        to: &PartName,
    ) -> Result<(), PackageError> {
        for node in &mut element.children {
            if let Node::Element(child) = node {
                self.rewrite(child, from, to)?;
            }
        }
        Ok(())
    }

    /// The source's part of `kind` and a copy of its first entry `select` picks.
    fn source_entry<F>(&self, kind: Kind, select: F) -> Option<(PartName, Element)>
    where
        F: Fn(&Element) -> bool,
    {
        let entries = self.entries.get(&kind)?.as_ref()?;
        let entry = entries
            .source_tree
            .root
            .child_elements()
            .find(|element| select(element))?;
        Some((entries.source.clone(), entry.clone()))
    }

    /// A style id: kept when this file defines it, whose definition then applies; else the source's
    /// `w:style` copied, with the styles it names.
    fn style(&mut self, id: &str) -> Result<(), PackageError> {
        if !self.styles.insert(id.to_owned()) {
            return Ok(());
        }
        self.open(Kind::Styles)?;
        let defines = |element: &Element| {
            is_w_named(element, "style") && w_value(element, "styleId") == Some(id)
        };
        let here = self
            .entries
            .get(&Kind::Styles)
            .and_then(Option::as_ref)
            .is_some_and(|entries| entries.tree.root.child_elements().any(defines));
        if here {
            return Ok(());
        }
        let Some((source_part, mut style)) = self.source_entry(Kind::Styles, defines) else {
            return Ok(());
        };
        let Some(target_part) = self.target_part(Kind::Styles)? else {
            return Ok(());
        };
        self.rewrite_children(&mut style, &source_part, &target_part)?;
        self.add(Kind::Styles, style);
        Ok(())
    }

    /// This file's part of `kind`, ready for an entry: on the first one, the part the import makes
    /// is added and related from the main part, and the part's root gains the source root's
    /// declarations and ignorable prefixes.
    fn target_part(&mut self, kind: Kind) -> Result<Option<PartName>, PackageError> {
        let Some(Some(entries)) = self.entries.get_mut(&kind) else {
            return Ok(None);
        };
        if entries.changed {
            return Ok(entries.target.clone());
        }
        let name = if let Some(name) = &entries.target {
            name.clone()
        } else {
            let main = &self.target.main;
            let template = format!("{}/{}{{}}.xml", main.directory(), kind.root());
            let (name, _) = self.target.package.add_part(
                Some(main),
                kind.relationship().in_class(self.target.class),
                kind.content_type(),
                &template,
                &entries.tree.to_xml(self.target.write_options),
            )?;
            entries.target = Some(name.clone());
            name
        };
        let source_root = &entries.source_tree.root;
        merge_declarations(
            &mut entries.tree.root.namespace_declarations,
            &mut entries.tree.root.attributes,
            &source_root.namespace_declarations,
            &source_root.attributes,
        )
        .map_err(|reason| PackageError::Read {
            part: name.clone(),
            reason,
        })?;
        entries.changed = true;
        Ok(Some(name))
    }
}

impl Kind {
    const fn content_type(self) -> &'static str {
        match self {
            Self::Comments => wordprocessing_comments::CONTENT_TYPE,
            Self::Endnotes => endnotes::CONTENT_TYPE,
            Self::Footnotes => footnotes::CONTENT_TYPE,
            Self::Numbering => numbering_definitions::CONTENT_TYPE,
            Self::Styles => style_definitions::CONTENT_TYPE,
        }
    }

    const fn relationship(self) -> RelationshipType {
        match self {
            Self::Comments => wordprocessing_comments::RELATIONSHIP_TYPE,
            Self::Endnotes => endnotes::RELATIONSHIP_TYPE,
            Self::Footnotes => footnotes::RELATIONSHIP_TYPE,
            Self::Numbering => numbering_definitions::RELATIONSHIP_TYPE,
            Self::Styles => style_definitions::RELATIONSHIP_TYPE,
        }
    }

    const fn root(self) -> &'static str {
        match self {
            Self::Comments => "comments",
            Self::Endnotes => "endnotes",
            Self::Footnotes => "footnotes",
            Self::Numbering => "numbering",
            Self::Styles => "styles",
        }
    }

    /// The entry whose separators a new part of the kind starts with.
    const fn separators(self) -> Option<&'static str> {
        match self {
            Self::Endnotes => Some("endnote"),
            Self::Footnotes => Some("footnote"),
            Self::Comments | Self::Numbering | Self::Styles => None,
        }
    }

    const fn spaces(self) -> &'static [Space] {
        match self {
            Self::Comments => &[Space::Comment],
            Self::Endnotes => &[Space::Endnote],
            Self::Footnotes => &[Space::Footnote],
            Self::Numbering => &[Space::PictureBullet, Space::AbstractNum, Space::Num],
            Self::Styles => &[],
        }
    }
}

impl Space {
    const fn entry(self) -> &'static str {
        match self {
            Self::AbstractNum => "abstractNum",
            Self::Comment => "comment",
            Self::Endnote => "endnote",
            Self::Footnote => "footnote",
            Self::Num => "num",
            Self::PictureBullet => "numPicBullet",
        }
    }

    const fn id_attribute(self) -> &'static str {
        match self {
            Self::AbstractNum => "abstractNumId",
            Self::Comment | Self::Endnote | Self::Footnote => "id",
            Self::Num => "numId",
            Self::PictureBullet => "numPicBulletId",
        }
    }

    const fn kind(self) -> Kind {
        match self {
            Self::AbstractNum | Self::Num | Self::PictureBullet => Kind::Numbering,
            Self::Comment => Kind::Comments,
            Self::Endnote => Kind::Endnotes,
            Self::Footnote => Kind::Footnotes,
        }
    }
}

/// The part of `kind` the main part `main` relates to first.
fn first_part(
    package: &Package,
    main: &PartName,
    kind: Kind,
) -> Result<Option<PartName>, PackageError> {
    Ok(package
        .related_by_type(Some(main), &kind.relationship())?
        .into_iter()
        .find_map(|related| {
            if let Target::Part(part) = related.target {
                Some(part)
            } else {
                None
            }
        }))
}

/// The highest integer `id` attribute in `namespace` over the elements `select` picks, or -1.
fn highest<F>(element: &Element, select: &F, namespace: &Namespace) -> i64
where
    F: Fn(&Element) -> bool,
{
    let own: i64 = if select(element) {
        element
            .attribute(namespace, "id")
            .and_then(|value| value.parse().ok())
            .unwrap_or(-1)
    } else {
        -1
    };
    element
        .child_elements()
        .map(|child| highest(child, select, namespace))
        .fold(own, i64::max)
}

/// `source`'s body blocks without its final `w:sectPr`, rewritten so that every id they use
/// resolves in `target`, whose main document is `document`; see [`Docx::import_body`].
pub fn import_body(
    target: &mut Docx,
    document: &mut Document,
    source: &Docx,
) -> Result<Vec<BlockContent>, PackageError> {
    let source_document = source.main_document()?;
    merge_declarations(
        &mut document.unknown.namespace_declarations,
        &mut document.unknown.attributes,
        &source_document.unknown.namespace_declarations,
        &source_document.unknown.attributes,
    )
    .map_err(|reason| PackageError::Read {
        part: target.main.clone(),
        reason,
    })?;
    let target_main = target.main.clone();
    let mut import = Import::new(source, target, document)?;
    let mut body = DocumentBody::default();
    body.content = source_document
        .body
        .map_or_else(Vec::new, |source_body| source_body.content);
    let mut scope = Scope::default();
    scope.start(&source_document.unknown.namespace_declarations);
    let mut element = wordprocessing::write::document_body(&body, &mut scope);
    scope.end();
    import.rewrite_children(&mut element, &source.main, &target_main)?;
    import.finish()?;
    Ok(wordprocessing::read::document_body(&element).content)
}

fn is_drawing(element: &Element) -> bool {
    element.name.namespace == Namespace::from_uri(WORDPROCESSING_DRAWING)
        && element.name.local == "docPr"
}

fn is_ignorable(attribute: &Attribute) -> bool {
    attribute.name.local == "Ignorable"
        && attribute.name.namespace == Namespace::from_uri(MARKUP_COMPATIBILITY)
}

fn is_range(element: &Element) -> bool {
    is_w(element) && RANGE_IDS.contains(&element.name.local.as_str())
}

fn is_w(element: &Element) -> bool {
    element.name.namespace == Namespace::from_uri(W)
}

fn is_w_named(element: &Element, local: &str) -> bool {
    is_w(element) && element.name.local == local
}

/// Give a root the source root's namespace declarations whose prefix it lacks, and the prefixes
/// of the source's `mc:Ignorable` it does not ignore; nothing changes when a prefix is bound to
/// different namespaces in the two roots, which is the error.
fn merge_declarations(
    declarations: &mut Vec<NamespaceDeclaration>,
    attributes: &mut Vec<Attribute>,
    source_declarations: &[NamespaceDeclaration],
    source_attributes: &[Attribute],
) -> Result<(), String> {
    let mut added = Vec::new();
    for declaration in source_declarations
        .iter()
        .filter(|declaration| !declaration.prefix.is_empty())
    {
        match declarations
            .iter()
            .find(|own| own.prefix == declaration.prefix)
        {
            Some(own) if Namespace::from_uri(&own.uri) != Namespace::from_uri(&declaration.uri) => {
                return Err(format!(
                    "the prefix {} names {} here and {} in the source",
                    declaration.prefix, own.uri, declaration.uri
                ));
            }
            Some(_) => {}
            None => added.push(declaration.clone()),
        }
    }
    declarations.extend(added);
    let Some(source_ignorable) = source_attributes
        .iter()
        .find(|attribute| is_ignorable(attribute))
    else {
        return Ok(());
    };
    if let Some(own) = attributes
        .iter_mut()
        .find(|attribute| is_ignorable(attribute))
    {
        let mut prefixes: Vec<&str> = own.value.split_whitespace().collect();
        for prefix in source_ignorable.value.split_whitespace() {
            if !prefixes.contains(&prefix) {
                prefixes.push(prefix);
            }
        }
        own.value = prefixes.join(" ");
    } else {
        attributes.push(source_ignorable.clone());
    }
    Ok(())
}

/// The root of a part of `kind` the import makes for this file: the source root's name and
/// declarations, and the separator entries of the source's footnotes or endnotes.
fn new_tree(kind: Kind, source_root: &Element) -> Part {
    let mut root = Element::new(QName::new(
        source_root.name.prefix.clone(),
        kind.root().to_owned(),
        source_root.name.namespace.clone(),
    ));
    root.namespace_declarations
        .clone_from(&source_root.namespace_declarations);
    if let Some(separators) = kind.separators() {
        root.children.extend(
            source_root
                .child_elements()
                .filter(|element| {
                    is_w_named(element, separators) && w_value(element, "type").is_some()
                })
                .cloned()
                .map(Node::Element),
        );
    }
    Part::new(DECLARATION, root)
}

/// The template a copy of `part` is named by: its file's trailing digits replaced by `{}`,
/// `/word/media/image3.png` giving `/word/media/image{}.png`.
fn pattern(part: &PartName) -> String {
    let name = part.as_str();
    let (directory, file) = name.rsplit_once('/').unwrap_or(("", name));
    let (stem, dotted) = file
        .rsplit_once('.')
        .map_or((file, String::new()), |(stem, extension)| {
            (stem, format!(".{extension}"))
        });
    let base = stem.trim_end_matches(|character: char| character.is_ascii_digit());
    format!("{directory}/{base}{{}}{dotted}")
}

fn w_attribute<'element>(
    element: &'element mut Element,
    local: &str,
) -> Option<&'element mut Attribute> {
    let namespace = Namespace::from_uri(W);
    element
        .attributes
        .iter_mut()
        .find(|attribute| attribute.name.local == local && attribute.name.namespace == namespace)
}

fn w_value<'element>(element: &'element Element, local: &str) -> Option<&'element str> {
    element.attribute(&Namespace::from_uri(W), local)
}
