use alloc::collections::BTreeSet;

use super::{fill, first_free, first_free_number};
use crate::packaging::error::PackageError;

fn taken(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|name| name.to_ascii_lowercase()).collect()
}

#[test]
fn the_number_starts_at_one() {
    assert_eq!(
        first_free("/word/header{}.xml", &taken(&[]))
            .unwrap()
            .as_str(),
        "/word/header1.xml"
    );
}

#[test]
fn a_taken_name_is_skipped_compared_case_folded() {
    let names = taken(&[
        "/word/Header1.xml",
        "/WORD/HEADER2.XML",
        "/word/header4.xml",
    ]);
    assert_eq!(
        first_free("/word/header{}.xml", &names).unwrap().as_str(),
        "/word/header3.xml"
    );
}

#[test]
fn every_template_must_be_free_at_the_number() {
    let names = taken(&["/customXml/item1.xml", "/customXml/itemProps2.xml"]);
    assert_eq!(
        first_free_number(
            &["/customXml/item{}.xml", "/customXml/itemProps{}.xml"],
            &names
        )
        .unwrap(),
        3
    );
}

#[test]
fn a_template_without_a_placeholder_is_refused() {
    let error = first_free("/word/header.xml", &taken(&[])).unwrap_err();
    assert!(
        matches!(&error, PackageError::PartName { name, reason: _reason } if name == "/word/header.xml"),
        "got {error:?}"
    );
}

#[test]
fn a_template_that_gives_no_part_name_is_refused() {
    let error = first_free("word/header{}.xml", &taken(&[])).unwrap_err();
    assert!(
        matches!(
            &error,
            PackageError::PartName {
                name: _name,
                reason: _reason
            }
        ),
        "got {error:?}"
    );
}

#[test]
fn fill_puts_the_number_in_place_of_the_placeholder() {
    assert_eq!(
        fill("/word/media/image{}.png", 12),
        "/word/media/image12.png"
    );
}
