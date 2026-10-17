//! Adding, replacing, and removing the header or footer of a section.

#[cfg(test)]
mod tests;

use officekit_markup::common_simple_types::OnOffValue;
use officekit_markup::wordprocessing::{
    BlockContent, ContentBlockContent, DifferentEvenOddPageHeadersAndFooters,
    DifferentFirstPageHeadersAndFooters, Document, DocumentBody, DocumentFinalSectionProperties,
    DocumentSettings, FooterReference, HeaderOrFooterTypeMember, HeaderReference, Paragraph,
    SectionPropertiesContent,
};
use officekit_packaging::error::PackageError;
use officekit_packaging::part_name::PartName;
use officekit_packaging::relationship::{RelatedPart, RelationshipType, Target};
use officekit_parts::part_root::{self, PartRoot};
use officekit_parts::{document_settings, footer, header};
use officekit_xml::conformance::ConformanceClass;
use officekit_xml::element::NamespaceDeclaration;
use officekit_xml::name::Namespace;

use crate::file;

use super::Docx;

const RELATIONSHIPS_URI: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const SETTINGS_TEMPLATE: &str = "/word/settings{}.xml";

/// Which of the two reference elements an operation works on.
#[derive(Clone, Copy)]
pub enum Which {
    Footer,
    Header,
}

impl Which {
    const fn content_type(self) -> &'static str {
        match self {
            Self::Footer => footer::CONTENT_TYPE,
            Self::Header => header::CONTENT_TYPE,
        }
    }

    fn item(self, id: String, kind: HeaderOrFooterTypeMember) -> SectionPropertiesContent {
        match self {
            Self::Footer => {
                let mut reference = FooterReference::default();
                reference.id = Some(id);
                reference.r#type = Some(kind.into());
                SectionPropertiesContent::FooterReference(reference)
            }
            Self::Header => {
                let mut reference = HeaderReference::default();
                reference.id = Some(id);
                reference.r#type = Some(kind.into());
                SectionPropertiesContent::HeaderReference(reference)
            }
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Footer => "footer",
            Self::Header => "header",
        }
    }

    const fn relationship(self) -> &'static RelationshipType {
        match self {
            Self::Footer => &RelationshipType::FOOTER,
            Self::Header => &RelationshipType::HEADER,
        }
    }

    const fn template(self) -> &'static str {
        match self {
            Self::Footer => "/word/footer{}.xml",
            Self::Header => "/word/header{}.xml",
        }
    }
}

/// Give section `section` a part holding `root` as its `kind` header or footer; `blocks` is how
/// many blocks `root` holds.
pub fn add<R>(
    docx: &mut Docx,
    section: usize,
    kind: HeaderOrFooterTypeMember,
    which: Which,
    root: &R,
    blocks: usize,
) -> Result<PartName, PackageError>
where
    R: PartRoot,
{
    if blocks == 0 {
        return Err(PackageError::Read {
            part: docx.main.clone(),
            reason: format!("a {} must hold at least one block", which.name()),
        });
    }
    let mut document = docx.main_document()?;
    declare_relationships(&mut document, docx.class);
    let main = docx.main.clone();
    let properties =
        nth_section(&mut document, section).ok_or_else(|| no_section(&main, section))?;
    let name = docx.package.free_part_name(which.template())?;
    part_root::store(
        &mut docx.package,
        &name,
        which.content_type(),
        root,
        docx.class,
        docx.write_options,
    )?;
    let id = docx.package.relate(
        Some(&main),
        &name,
        which.relationship().in_class(docx.class),
    )?;
    let replaced = put_reference(properties, which, kind, id);
    if kind == HeaderOrFooterTypeMember::First
        && !properties
            .different_first_page_headers_and_footers
            .as_ref()
            .is_some_and(|flag| is_on(flag.val.as_ref()))
    {
        properties.different_first_page_headers_and_footers =
            Some(Box::new(DifferentFirstPageHeadersAndFooters::default()));
    }
    if kind == HeaderOrFooterTypeMember::Even {
        turn_on_even_and_odd_headers(docx)?;
    }
    docx.set_main_document(&document)?;
    if let Some(old) = replaced {
        remove_if_unreferenced(docx, &mut document, &old)?;
    }
    Ok(name)
}

/// Remove section `section`'s `kind` reference, and its part once no section names it.
pub fn remove(
    docx: &mut Docx,
    section: usize,
    kind: HeaderOrFooterTypeMember,
    which: Which,
) -> Result<Option<PartName>, PackageError> {
    let mut document = docx.main_document()?;
    let main = docx.main.clone();
    let properties =
        nth_section(&mut document, section).ok_or_else(|| no_section(&main, section))?;
    let Some(at) = properties
        .content
        .iter_mut()
        .position(|item| reference(item, which).is_some_and(|(_, found)| found == kind.as_str()))
    else {
        return Ok(None);
    };
    let id = reference(&mut properties.content[at], which).and_then(|(id, _)| id.clone());
    properties.content.remove(at);
    docx.set_main_document(&document)?;
    let Some(named) = id else {
        return Ok(None);
    };
    remove_if_unreferenced(docx, &mut document, &named)
}

fn declare_relationships(document: &mut Document, class: ConformanceClass) {
    let namespace = Namespace::from_uri(RELATIONSHIPS_URI);
    let declarations = &mut document.unknown.namespace_declarations;
    if declarations.iter().any(|declaration| {
        !declaration.prefix.is_empty() && Namespace::from_uri(&declaration.uri) == namespace
    }) {
        return;
    }
    let mut prefix = "r".to_owned();
    let mut suffix = 0_u32;
    while declarations
        .iter()
        .any(|declaration| declaration.prefix == prefix)
    {
        suffix += 1;
        prefix = format!("r{suffix}");
    }
    declarations.push(NamespaceDeclaration::new(
        prefix,
        namespace.uri(class).to_owned(),
    ));
}

fn is_on(flag: Option<&OnOffValue>) -> bool {
    flag.is_none_or(|text| text.value().unwrap_or(true))
}

fn no_section(main: &PartName, section: usize) -> PackageError {
    PackageError::Read {
        part: main.clone(),
        reason: format!("the document has no section {section}"),
    }
}

/// Section `section` of the document: the paragraphs that end one, in order, then the body's own,
/// created when missing.
fn nth_section(
    document: &mut Document,
    section: usize,
) -> Option<&mut DocumentFinalSectionProperties> {
    sections(document).into_iter().nth(section)
}

fn sections(document: &mut Document) -> Vec<&mut DocumentFinalSectionProperties> {
    let body: &mut DocumentBody = document.body.get_or_insert_with(Box::default);
    let mut found = Vec::new();
    blocks_sections(&mut body.content, &mut found);
    found.push(
        body.document_final_section_properties
            .get_or_insert_with(Box::default),
    );
    found
}

fn blocks_sections<'doc>(
    blocks: &'doc mut [BlockContent],
    found: &mut Vec<&'doc mut DocumentFinalSectionProperties>,
) {
    for block in blocks {
        let control = if let BlockContent::Paragraph(paragraph) = block {
            paragraph_section(paragraph, found);
            None
        } else if let BlockContent::BlockLevelStructuredDocumentTag(control) = block {
            control.content.as_mut()
        } else {
            None
        };
        if let Some(content) = control {
            content_sections(&mut content.content, found);
        }
    }
}

fn content_sections<'doc>(
    blocks: &'doc mut [ContentBlockContent],
    found: &mut Vec<&'doc mut DocumentFinalSectionProperties>,
) {
    for block in blocks {
        let control = if let ContentBlockContent::Paragraph(paragraph) = block {
            paragraph_section(paragraph, found);
            None
        } else if let ContentBlockContent::BlockLevelStructuredDocumentTag(control) = block {
            control.content.as_mut()
        } else {
            None
        };
        if let Some(content) = control {
            content_sections(&mut content.content, found);
        }
    }
}

fn paragraph_section<'doc>(
    paragraph: &'doc mut Paragraph,
    found: &mut Vec<&'doc mut DocumentFinalSectionProperties>,
) {
    if let Some(section) = paragraph
        .properties
        .as_mut()
        .and_then(|properties| properties.document_final_section_properties.as_mut())
    {
        found.push(section);
    }
}

/// The relationship id and the type text of `item` when it is a reference of `which`.
fn reference(
    item: &mut SectionPropertiesContent,
    which: Which,
) -> Option<(&mut Option<String>, &str)> {
    match (item, which) {
        (SectionPropertiesContent::HeaderReference(found), Which::Header) => Some((
            &mut found.id,
            found
                .r#type
                .as_ref()
                .map_or("default", |kind| kind.0.as_str()),
        )),
        (SectionPropertiesContent::FooterReference(found), Which::Footer) => Some((
            &mut found.id,
            found
                .r#type
                .as_ref()
                .map_or("default", |kind| kind.0.as_str()),
        )),
        _ => None,
    }
}

/// Point the section's `kind` reference at `id`, or add one after the references it has; returns
/// the id it named before.
fn put_reference(
    properties: &mut DocumentFinalSectionProperties,
    which: Which,
    kind: HeaderOrFooterTypeMember,
    id: String,
) -> Option<String> {
    let existing = properties
        .content
        .iter_mut()
        .find_map(|item| reference(item, which).filter(|(_, found)| *found == kind.as_str()));
    if let Some((slot, _)) = existing {
        return slot.replace(id);
    }
    let at = properties
        .content
        .iter()
        .take_while(|item| {
            matches!(
                item,
                SectionPropertiesContent::HeaderReference(_)
                    | SectionPropertiesContent::FooterReference(_)
            )
        })
        .count();
    properties.content.insert(at, which.item(id, kind));
    None
}

/// Remove the relationship `id` of the main part, and its part, unless a section still names it.
fn remove_if_unreferenced(
    docx: &mut Docx,
    document: &mut Document,
    id: &str,
) -> Result<Option<PartName>, PackageError> {
    let named = sections(document).into_iter().any(|properties| {
        properties.content.iter_mut().any(|item| {
            [Which::Header, Which::Footer].into_iter().any(|which| {
                reference(item, which).is_some_and(|(found, _)| found.as_deref() == Some(id))
            })
        })
    });
    if named {
        return Ok(None);
    }
    let related = docx.package.related(Some(&docx.main))?;
    if !related.iter().any(|part| part.id == id) {
        return Ok(None);
    }
    docx.package.remove_relationship(Some(&docx.main), id)
}

/// Turn on `w:evenAndOddHeaders` in the settings part, making and relating the part when the
/// document has none.
fn turn_on_even_and_odd_headers(docx: &mut Docx) -> Result<(), PackageError> {
    let existing = docx
        .package
        .related_by_type(Some(&docx.main), &RelationshipType::SETTINGS)?
        .into_iter()
        .find_map(|related: RelatedPart| {
            if let Target::Part(name) = related.target {
                Some(name)
            } else {
                None
            }
        });
    let (name, mut settings) = if let Some(found) = existing {
        let loaded = document_settings::load(&docx.package, &found, docx.options)?;
        (found, loaded)
    } else {
        (
            docx.package.free_part_name(SETTINGS_TEMPLATE)?,
            DocumentSettings::default(),
        )
    };
    let is_new = !docx.package.contains(&name);
    if settings
        .different_even_odd_page_headers_and_footers
        .as_ref()
        .is_some_and(|flag| is_on(flag.val.as_ref()))
    {
        return Ok(());
    }
    settings.different_even_odd_page_headers_and_footers =
        Some(Box::new(DifferentEvenOddPageHeadersAndFooters::default()));
    if is_new {
        part_root::store(
            &mut docx.package,
            &name,
            document_settings::CONTENT_TYPE,
            &settings,
            docx.class,
            docx.write_options,
        )?;
        docx.package.relate(
            Some(&docx.main),
            &name,
            RelationshipType::SETTINGS.in_class(docx.class),
        )?;
    } else {
        file::store(
            &mut docx.package,
            &name,
            &settings,
            document_settings::CONTENT_TYPE,
            docx.class,
            docx.write_options,
        )?;
    }
    Ok(())
}
