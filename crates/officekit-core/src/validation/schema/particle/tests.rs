use super::{Child, check};
use crate::validation::error::ErrorCode;
use crate::validation::generated::{SCHEMA_TYPES, type_id};
use crate::validation::schema::types::{CompositeKind, Particle, TypeId};
use crate::xml::name::table::NAMESPACES;
use crate::xml::name::{Namespace, OfficeVersion, QName};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

const TARGET: OfficeVersion = OfficeVersion::Microsoft365;

fn id(name: &str) -> TypeId {
    type_id(name).unwrap()
}

fn model(type_name: &str) -> &'static Particle {
    SCHEMA_TYPES[usize::from(id(type_name))]
        .particles
        .iter()
        .find(|(versions, _)| versions.contains(TARGET))
        .and_then(|(_, particle)| particle.as_ref())
        .unwrap()
}

/// The children of a parent: each a `w:` element name, typed through the
/// parent's own children, or untyped when the parent does not declare it.
fn names(parent: &str, locals: &[&str]) -> Vec<(QName, Option<TypeId>)> {
    let parent_type = &SCHEMA_TYPES[usize::from(id(parent))];
    locals
        .iter()
        .map(|local| {
            let name = QName::prefixed("w", local, W);
            let typed = parent_type
                .children
                .iter()
                .find(|(namespace, own, _, _)| {
                    name.namespace == Namespace::Known(*namespace) && own == local
                })
                .map(|(_, _, type_id, _)| *type_id);
            (name, typed)
        })
        .collect()
}

fn mismatch(parent: &str, locals: &[&str]) -> Option<(Option<usize>, ErrorCode, String)> {
    let named = names(parent, locals);
    let namespace = Namespace::from_uri(W);
    let children: Vec<Child<'_>> = named
        .iter()
        .map(|(name, typed)| Child::new(name, &namespace, *typed))
        .collect();
    check(model(parent), &children, id(parent), TARGET)
        .map(|found| (found.child, found.code, found.expected))
}

#[test]
fn children_in_the_models_order_follow_it() {
    let margins = "w:CT_TblCellMar/w:tblCellMar";
    assert_eq!(mismatch(margins, &["top", "left", "bottom", "right"]), None);
    assert_eq!(mismatch(margins, &[]), None);
}

#[test]
fn an_out_of_order_child_is_unexpected() {
    assert_eq!(
        mismatch("w:CT_TblCellMar/w:tblCellMar", &["left", "top"]),
        Some((Some(1), ErrorCode::UnexpectedChild, String::new()))
    );
}

#[test]
fn a_repeated_child_allowed_once_is_unexpected() {
    let found = mismatch("w:CT_TblCellMar/w:tblCellMar", &["top", "top"]);
    assert_eq!(
        found.map(|(child, code, _)| (child, code)),
        Some((Some(1), ErrorCode::UnexpectedChild))
    );
}

#[test]
fn a_required_child_missing_is_missing_with_what_is_expected() {
    assert_eq!(
        mismatch("w:CT_Tbl/w:tbl", &[]),
        Some((
            None,
            ErrorCode::MissingChild,
            ", expected w:tblPr".to_owned()
        ))
    );
}

#[test]
fn a_child_the_parent_does_not_declare_is_invalid() {
    let found = mismatch("w:CT_P/w:p", &["pPr", "smartTag"]);
    assert_eq!(
        found.map(|(child, code, _)| (child, code)),
        Some((Some(1), ErrorCode::InvalidChild))
    );
}

/// The first type whose content model is an `xsd:all` at the target, and the
/// type of the group's first element.
fn all_group() -> Option<(TypeId, TypeId)> {
    for (index, schema_type) in SCHEMA_TYPES.iter().enumerate() {
        for (versions, particle) in schema_type.particles {
            if !versions.contains(TARGET) {
                continue;
            }
            if let Some(Particle::Composite {
                items,
                kind: CompositeKind::All,
                max: _group_max,
                min: _group_min,
            }) = particle
                && let Some(Particle::Element {
                    max: _item_max,
                    min: _item_min,
                    type_id,
                }) = items.first()
            {
                return Some((TypeId::try_from(index).ok()?, *type_id));
            }
        }
    }
    None
}

#[test]
fn a_child_of_an_all_group_seen_twice_is_repeated() {
    let (parent, first) = all_group().unwrap();
    let parent_type = &SCHEMA_TYPES[usize::from(parent)];
    let (namespace, local, _, _) = parent_type
        .children
        .iter()
        .find(|(_, _, type_id, _)| *type_id == first)
        .unwrap();
    let uri = NAMESPACES[usize::from(*namespace)].uri;
    let name = QName::prefixed("p", local, uri);
    let parent_namespace = Namespace::from_uri(uri);
    let children = [
        Child::new(&name, &parent_namespace, Some(first)),
        Child::new(&name, &parent_namespace, Some(first)),
    ];
    let particle = parent_type
        .particles
        .iter()
        .find(|(versions, _)| versions.contains(TARGET))
        .and_then(|(_, found)| found.as_ref())
        .unwrap();
    let found = check(particle, &children, parent, TARGET).unwrap();
    assert_eq!(
        (found.child, found.code),
        (Some(1), ErrorCode::RepeatedChildInAllGroup)
    );
}

#[test]
fn a_wildcard_takes_any_element() {
    let data = "a:CT_GraphicalObjectData/a:graphicData";
    let name = QName::prefixed(
        "pic",
        "pic",
        "http://schemas.openxmlformats.org/drawingml/2006/picture",
    );
    let other = QName::prefixed("x", "anything", "urn:example");
    let parent_namespace =
        Namespace::from_uri("http://schemas.openxmlformats.org/drawingml/2006/main");
    let children = [
        Child::new(&name, &parent_namespace, None),
        Child::new(&other, &parent_namespace, None),
    ];
    assert_eq!(check(model(data), &children, id(data), TARGET), None);
}
