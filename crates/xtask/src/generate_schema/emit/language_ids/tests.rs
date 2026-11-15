use alloc::collections::BTreeMap;
use core::fmt::{self, Write as _};

use super::{checked, language_ids, render, tag, version};
use crate::generate_schema::cldr::{self, Tree};

/// A DOCX table of `rows`, each cell one paragraph of two runs where it holds a space.
fn table(rows: &[&[&str]]) -> String {
    let mut text = String::from("<w:tbl><w:tblPr/>");
    for row in rows {
        text.push_str("<w:tr><w:trPr/>");
        for cell in *row {
            let runs = cell.split_once(' ').map_or_else(
                || format!("<w:r><w:t>{cell}</w:t></w:r>"),
                |(head, rest)| {
                    format!(
                        "<w:r><w:t xml:space=\"preserve\">{head} </w:t></w:r><w:r><w:t>{rest}</w:t></w:r>"
                    )
                },
            );
            let _: fmt::Result = write!(text, "<w:tc><w:tcPr/><w:p>{runs}</w:p></w:tc>");
        }
        text.push_str("</w:tr>");
    }
    text.push_str("</w:tbl>");
    text
}

fn document(tables: &[String]) -> Tree {
    cldr::read(&format!(
        "<w:document><w:body><w:p><w:r><w:t>2.2 LCID Structure</w:t></w:r></w:p>{}</w:body></w:document>",
        tables.concat()
    ))
    .unwrap()
}

fn owned(ids: &[(u16, &str)]) -> BTreeMap<u16, String> {
    ids.iter()
        .map(|(id, named)| (*id, (*named).to_owned()))
        .collect()
}

#[test]
fn a_cell_names_its_tag_before_a_comma_and_descriptions_name_none() {
    assert_eq!(tag("en-US"), Some("en-US"));
    assert_eq!(tag("es-ES_tradnl"), Some("es-ES_tradnl"));
    assert_eq!(tag("zh-yue-HK, reserved"), Some("zh-yue-HK"));
    assert_eq!(tag("ff-NG, ff-Latn-NG"), Some("ff-NG"));
    assert_eq!(tag("pap-029, reserved"), Some("pap-029"));
    for none in [
        "reserved",
        "Neither defined nor reserved",
        "Reserved for invariant locale behavior",
        "Unassigned LCID locale temporarily assigned to LCID 0x3000. See section 2.2.1.",
        "",
        "en--US",
    ] {
        assert_eq!(tag(none), None, "{none}");
    }
}

#[test]
fn the_table_headed_language_id_gives_each_id_its_tag() {
    let ids = table(&[
        &["Language ID", "Language tag"],
        &["0x0409", "en-US"],
        &["0x040A", "es-ES_tradnl"],
        &["0x0C6b", "quz-PE"],
        &["0x007B", "Neither defined nor reserved"],
        &["0x0001", "ar"],
    ]);
    let other = table(&[
        &["LCID", "Language tag (string name)"],
        &["0x00010407", "de-DE_phoneb"],
    ]);
    assert_eq!(
        language_ids(&document(&[other, ids])).unwrap(),
        owned(&[
            (0x0001, "ar"),
            (0x0409, "en-US"),
            (0x040A, "es-ES_tradnl"),
            (0x0C6B, "quz-PE"),
        ])
    );
}

#[test]
fn a_malformed_or_repeated_id_stops_the_generation() {
    let heading: &[&str] = &["Language ID", "Language tag"];
    for (row, message) in [
        (["0x409", "en-US"], "[MS-LCID] gives 0x409 as a language id"),
        (
            ["0x0409", "en-US"],
            "[MS-LCID] gives the language id 0x0409 twice",
        ),
    ] {
        let tables = [table(&[heading, &["0x0409", "en-US"], &row])];
        assert_eq!(
            language_ids(&document(&tables)).unwrap_err().to_string(),
            message
        );
    }
    assert_eq!(
        language_ids(&document(&[])).unwrap_err().to_string(),
        "[MS-LCID] has no table headed [\"Language ID\", \"Language tag\"]"
    );
}

#[test]
fn the_version_is_the_last_revision() {
    let revisions = table(&[
        &["Date", "Revision History", "Revision Class", "Comments"],
        &["6/25/2021", "15.0", "Major", "Changed."],
        &["4/23/2024", "16.0", "Major", "Changed."],
    ]);
    assert_eq!(version(&document(&[revisions])).unwrap(), "16.0");
}

#[test]
fn the_standards_ids_must_name_the_same_languages() {
    let standard = [(0x404, "zh-tw".to_owned()), (0x411, "ja-jp".to_owned())];
    let ids = owned(&[(0x0404, "zh-TW"), (0x0409, "en-US"), (0x0411, "ja-JP")]);
    assert_eq!(checked(ids.clone(), &standard).unwrap(), ids);
    let other = owned(&[(0x0404, "zh-TW"), (0x0411, "ko-KR")]);
    assert_eq!(
        checked(other, &standard).unwrap_err().to_string(),
        "section 18.8.30 gives the language id 411 to ja-jp, [MS-LCID] to ko-KR"
    );
    assert_eq!(
        checked(ids, &standard[..1]).unwrap_err().to_string(),
        "section 18.8.30 gives ja-jp no language id"
    );
}

#[test]
fn the_table_holds_each_id_and_its_tag() {
    let text = render(
        "// header\n",
        "16.0",
        &owned(&[(0x0409, "en-US"), (0x040A, "es-ES_tradnl")]),
    );
    assert!(
        text.starts_with("// header\n\n//! The language ids of [MS-LCID]"),
        "{text}"
    );
    for expected in [
        "//! 16.0, section 2.2",
        "pub(super) static LANGUAGE_IDS: [(u16, &str); 2] = [\n    (0x0409, \"en-US\"),\n    \
         (0x040a, \"es-ES_tradnl\"),\n];\n",
    ] {
        assert!(text.contains(expected), "{expected}\n{text}");
    }
}
