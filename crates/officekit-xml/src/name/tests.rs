use alloc::sync::Arc;

use super::table::NAMESPACES;
use super::{ConformanceClass, Namespace, OfficeVersion, QName};

const WORDPROCESSING: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const WORDPROCESSING_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";

#[test]
fn a_transitional_and_a_strict_uri_give_one_id() {
    let transitional = Namespace::from_uri(WORDPROCESSING);
    let strict = Namespace::from_uri(WORDPROCESSING_STRICT);
    assert!(matches!(transitional, Namespace::Known(_)));
    assert_eq!(transitional, strict);
}

#[test]
fn an_unknown_uri_is_kept_as_written() {
    let uri = "http://documentgeneration.prodoctivity.com/2011/";
    assert_eq!(Namespace::from_uri(uri), Namespace::Other(Arc::from(uri)));
}

#[test]
fn the_empty_uri_is_no_namespace() {
    assert_eq!(Namespace::from_uri(""), Namespace::None);
    assert_eq!(Namespace::None.uri(ConformanceClass::Strict), "");
}

#[test]
fn a_uri_is_written_in_the_class_asked() {
    let namespace = Namespace::from_uri(WORDPROCESSING_STRICT);
    assert_eq!(
        namespace.uri(ConformanceClass::Strict),
        WORDPROCESSING_STRICT
    );
    assert_eq!(
        namespace.uri(ConformanceClass::Transitional),
        WORDPROCESSING
    );
}

#[test]
fn a_namespace_with_one_uri_is_written_alike_in_both_classes() {
    let uri = "http://schemas.microsoft.com/office/word/2010/wordml";
    let namespace = Namespace::from_uri(uri);
    assert!(matches!(namespace, Namespace::Known(_)));
    assert_eq!(namespace.uri(ConformanceClass::Strict), uri);
    assert_eq!(namespace.uri(ConformanceClass::Transitional), uri);
}

#[test]
fn an_other_uri_is_written_as_kept() {
    let namespace = Namespace::Other(Arc::from("urn:example"));
    assert_eq!(namespace.uri(ConformanceClass::Strict), "urn:example");
}

#[test]
fn the_table_dates_the_word_2010_extensions() {
    let extension = NAMESPACES.iter().find(|row| row.prefix == "w14").unwrap();
    assert_eq!(extension.introduced, Some(OfficeVersion::Office2010));
    let own = NAMESPACES.iter().find(|row| row.prefix == "w").unwrap();
    assert_eq!(own.introduced, None);
    assert_eq!(own.strict_uri, Some(WORDPROCESSING_STRICT));
}

#[test]
fn versions_are_ordered_by_rank() {
    let by_release = [
        OfficeVersion::Office2010,
        OfficeVersion::Office2013,
        OfficeVersion::Office2016,
        OfficeVersion::Office2019,
        OfficeVersion::Office2021,
        OfficeVersion::Microsoft365,
    ];
    for (index, version) in by_release.into_iter().enumerate() {
        assert_eq!(usize::from(version.rank()), index);
    }
}

#[test]
fn a_prefixed_name_stands_for_its_namespace() {
    let name = QName::prefixed("w", "p", WORDPROCESSING);
    assert_eq!(name.local, "p");
    assert_eq!(name.prefix, "w");
    assert_eq!(name.namespace, Namespace::from_uri(WORDPROCESSING_STRICT));
}

#[test]
fn an_unprefixed_name_has_an_empty_prefix() {
    let name = QName::unprefixed("p", "");
    assert_eq!(name.prefix, "");
    assert_eq!(name.namespace, Namespace::None);
}
