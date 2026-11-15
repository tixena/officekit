use alloc::collections::BTreeMap;

use super::{render, rows};
use crate::generate_schema::cldr;

fn file(rows: &str) -> String {
    format!(
        "<supplementalData><version number=\"$Revision$\"/><likelySubtags>{rows}</likelySubtags>\
         </supplementalData>"
    )
}

fn read(rows_text: &str) -> anyhow::Result<BTreeMap<String, String>> {
    rows(&cldr::read(&file(rows_text))?)
}

#[test]
fn the_rows_are_sorted_by_id() {
    let found = read(
        "<likelySubtag from=\"zh_TW\" to=\"zh_Hant_TW\"/><!--Chinese-->\
         <likelySubtag from=\"ja\" to=\"ja_Jpan_JP\"/>\
         <likelySubtag from=\"und_419\" to=\"es_Latn_419\" origin=\"sil1\"/>",
    )
    .unwrap();
    assert_eq!(
        found.into_iter().collect::<Vec<(String, String)>>(),
        [
            ("ja".to_owned(), "ja_Jpan_JP".to_owned()),
            ("und_419".to_owned(), "es_Latn_419".to_owned()),
            ("zh_TW".to_owned(), "zh_Hant_TW".to_owned()),
        ]
    );
}

#[test]
fn an_id_given_twice_stops_the_generation() {
    let err = read(
        "<likelySubtag from=\"ja\" to=\"ja_Jpan_JP\"/><likelySubtag from=\"ja\" to=\"ja_Hira_JP\"/>",
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "likelySubtags.xml gives ja twice");
}

#[test]
fn a_full_id_with_no_script_stops_the_generation() {
    let err = read("<likelySubtag from=\"ja\" to=\"ja_JP\"/>").unwrap_err();
    assert_eq!(
        err.to_string(),
        "the likelySubtag of ja gives ja_JP, not a language, a script and a region"
    );
}

#[test]
fn a_row_with_no_to_stops_the_generation() {
    let err = read("<likelySubtag from=\"ja\"/>").unwrap_err();
    assert_eq!(err.to_string(), "the likelySubtag of ja has no to");
}

#[test]
fn a_file_with_no_list_stops_the_generation() {
    let err = rows(&cldr::read("<supplementalData/>").unwrap()).unwrap_err();
    assert_eq!(err.to_string(), "likelySubtags.xml has no likelySubtags");
}

#[test]
fn the_table_has_one_row_per_id() {
    let found = BTreeMap::from([
        ("ja".to_owned(), "ja_Jpan_JP".to_owned()),
        ("zh_TW".to_owned(), "zh_Hant_TW".to_owned()),
    ]);
    let text = render("// header\n", "Copyright 1991-2026 Unicode, Inc.", &found);
    assert!(text.starts_with("// header\n\n//! Unicode CLDR 49's likely subtags"));
    assert!(text.contains("Copyright 1991-2026 Unicode, Inc. Unicode License v3."));
    assert!(text.ends_with(
        "pub(super) static LIKELY_SUBTAGS: [(&str, &str); 2] = [\n    (\"ja\", \"ja_Jpan_JP\"),\n    \
         (\"zh_TW\", \"zh_Hant_TW\"),\n];\n"
    ));
}
