use super::PartName;
use crate::error::PackageError;

fn reason_for(name: &str) -> Option<&'static str> {
    let Err(PackageError::PartName {
        name: _name,
        reason,
    }) = PartName::new(name)
    else {
        return None;
    };
    Some(reason)
}

#[test]
fn accepts_well_formed_names() {
    for name in [
        "/word/document.xml",
        "/_rels/.rels",
        "/[Content_Types].xml",
        "/a",
    ] {
        let part = PartName::new(name).unwrap();
        assert_eq!(part.as_str(), name);
        assert_eq!(part.to_string(), name);
    }
}

#[test]
fn rejects_empty_name() {
    assert_eq!(reason_for(""), Some("must not be empty"));
}

#[test]
fn rejects_relative_name() {
    assert_eq!(reason_for("word/document.xml"), Some("must start with '/'"));
}

#[test]
fn rejects_trailing_slash() {
    assert_eq!(reason_for("/word/"), Some("must not end with '/'"));
}

#[test]
fn rejects_empty_segment() {
    assert_eq!(
        reason_for("/word//document.xml"),
        Some("must not hold an empty segment")
    );
}

#[test]
fn rejects_dot_segments() {
    let expected = Some("must not hold a segment that ends with '.'");
    assert_eq!(reason_for("/word/./document.xml"), expected);
    assert_eq!(reason_for("/word/../document.xml"), expected);
    assert_eq!(reason_for("/word/document."), expected);
}

#[test]
fn zip_entry_round_trip_drops_and_restores_the_leading_slash() {
    let part = PartName::from_zip_entry("word/document.xml").unwrap();
    assert_eq!(part.as_str(), "/word/document.xml");
    assert_eq!(part.zip_entry_name(), "word/document.xml");
}

#[test]
fn zip_entry_with_its_own_leading_slash_is_rejected() {
    let err = PartName::from_zip_entry("/word/document.xml").unwrap_err();
    assert!(
        matches!(
            &err,
            PackageError::PartName {
                name: _name,
                reason: _reason
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn comparisons_are_exact_and_folding_lowercases_ascii() {
    let lower = PartName::new("/word/document.xml").unwrap();
    let upper = PartName::new("/WORD/Document.XML").unwrap();
    assert_ne!(lower, upper);
    assert_eq!(upper.folded(), "/word/document.xml");
    assert_eq!(lower.folded(), upper.folded());
}
