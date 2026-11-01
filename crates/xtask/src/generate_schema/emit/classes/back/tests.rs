use super::{BackRule, build};
use crate::generate_schema::emit::classes::correspondences::{Codes, Correspondences, Spelling};
use crate::generate_schema::emit::classes::rules::{AttributeRule, ChildRule, Rules, Tag};
use crate::generate_schema::emit::namespaces::Row;

const TABLE: &str = r#"
[[value]]
type = "w:ST_Jc"
values = { left = "start", right = "end" }

[[character_set]]
type = "w:CT_TextPr"
attribute = "codePage"
to = "characterSet"
codes = "code_page"
written = "decimal"

[code_page]
50220 = "iso-2022-jp"
50222 = "iso-2022-jp"
1252 = "windows-1252"

[code_page_back]
"iso-2022-jp" = 50220
"#;

fn tag(local: &str) -> Tag {
    (Some(0), local.to_owned())
}

fn table(text: &str) -> Correspondences {
    let rows = [Row {
        introduced: None,
        prefix: "w".to_owned(),
        strict_uri: Some("http://purl.oclc.org/ooxml/wordprocessingml/main".to_owned()),
        uri: "http://schemas.openxmlformats.org/wordprocessingml/2006/main".to_owned(),
    }];
    Correspondences::parse(text, &rows).unwrap()
}

/// Rules of each kind the way back reverses.
fn rules() -> Rules {
    let jc = vec![Spelling::Map(vec![
        ("left".to_owned(), "start".to_owned()),
        ("right".to_owned(), "end".to_owned()),
    ])];
    let on_off = vec![Spelling::Map(vec![("on".to_owned(), "true".to_owned())])];
    let mut found = Rules::default();
    found
        .attributes
        .insert((tag("jc"), tag("val")), AttributeRule::Values(jc));
    found
        .attributes
        .insert((tag("b"), tag("val")), AttributeRule::Values(on_off));
    found.attributes.insert(
        (tag("alpha"), (None, "val".to_owned())),
        AttributeRule::Values(vec![Spelling::Percent {
            decimals: None,
            per_percent: 1000,
        }]),
    );
    found.attributes.insert(
        (tag("textPr"), tag("codePage")),
        AttributeRule::CharacterSet {
            codes: "CODE_PAGE".to_owned(),
            label: "textPr/@codePage".to_owned(),
            to: "characterSet".to_owned(),
        },
    );
    found.codes.insert(
        "CODE_PAGE".to_owned(),
        Codes::Number(vec![
            (1252, "windows-1252".to_owned()),
            (50220, "iso-2022-jp".to_owned()),
            (50222, "iso-2022-jp".to_owned()),
        ]),
    );
    found.moved.insert(
        (tag("textPr"), tag("characterSet")),
        (
            tag("codePage"),
            AttributeRule::CharacterSet {
                codes: "CODE_PAGE".to_owned(),
                label: "textPr/@codePage".to_owned(),
                to: "characterSet".to_owned(),
            },
        ),
    );
    found.moved.insert(
        (tag("ind"), tag("start")),
        (tag("left"), AttributeRule::Rename("start".to_owned())),
    );
    found.children.insert(
        (tag("tblBorders"), tag("left")),
        ChildRule::Rename {
            before: Vec::new(),
            to: "start".to_owned(),
        },
    );
    found.twips.insert((tag("ind"), tag("start")));
    found
}

#[test]
fn each_rule_has_its_way_back_and_a_member_s_map_stays() {
    let back = build(&rules(), &table(TABLE)).unwrap();
    assert_eq!(
        back.attributes.get(&(tag("jc"), tag("val"))),
        Some(&vec![BackRule::Values(vec![
            ("end".to_owned(), "right".to_owned()),
            ("start".to_owned(), "left".to_owned()),
        ])])
    );
    assert_eq!(back.attributes.get(&(tag("b"), tag("val"))), None);
    assert_eq!(
        back.attributes
            .get(&(tag("alpha"), (None, "val".to_owned()))),
        Some(&vec![BackRule::Percent(1000)])
    );
    assert_eq!(
        back.attributes.get(&(tag("ind"), tag("start"))),
        Some(&vec![BackRule::Twips, BackRule::Rename("left".to_owned())]),
        "the value's rule comes before the rename"
    );
    assert_eq!(
        back.attributes.get(&(tag("textPr"), tag("characterSet"))),
        Some(&vec![BackRule::Code {
            codes: "CODE_PAGE".to_owned(),
            to: "codePage".to_owned(),
        }])
    );
    assert_eq!(
        back.children.get(&(tag("tblBorders"), tag("start"))),
        Some(&"left".to_owned())
    );
}

#[test]
fn a_name_two_codes_share_goes_back_to_the_code_the_table_names() {
    let back = build(&rules(), &table(TABLE)).unwrap();
    assert_eq!(
        back.codes.get("CODE_PAGE"),
        Some(&vec![
            ("iso-2022-jp".to_owned(), "50220".to_owned()),
            ("windows-1252".to_owned(), "1252".to_owned()),
        ])
    );
    let unsettled = TABLE.replace("[code_page_back]\n\"iso-2022-jp\" = 50220\n", "");
    let error = build(&rules(), &table(&unsettled)).unwrap_err().to_string();
    assert!(error.contains("iso-2022-jp"), "{error}");
}

#[test]
fn two_values_sharing_a_strict_one_stop_the_build() {
    let shared = TABLE.replace(
        "values = { left = \"start\", right = \"end\" }",
        "values = { left = \"start\", right = \"start\" }",
    );
    let mut found = rules();
    found.attributes.insert(
        (tag("jc"), tag("val")),
        AttributeRule::Values(vec![Spelling::Map(vec![
            ("left".to_owned(), "start".to_owned()),
            ("right".to_owned(), "start".to_owned()),
        ])]),
    );
    let error = build(&found, &table(&shared)).unwrap_err().to_string();
    assert!(error.contains("share a Strict one"), "{error}");
}
