use alloc::collections::BTreeMap;

use super::{chain, parent, parent_locales, read};

const SUPPLEMENTAL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\
    <!DOCTYPE supplementalData SYSTEM \"../../common/dtd/ldmlSupplemental.dtd\">\
    <supplementalData>\
    <parentLocales><parentLocale parent=\"es_419\" locales=\"es_AR es_MX\"/>\
    <parentLocale parent=\"root\" locales=\"sr_Latn\"/></parentLocales>\
    <parentLocales component=\"collations\"><parentLocale parent=\"zh_Hant\" locales=\"yue\"/>\
    </parentLocales></supplementalData>";

fn parents() -> BTreeMap<String, String> {
    parent_locales(&read(SUPPLEMENTAL).unwrap()).unwrap()
}

#[test]
fn reads_text_cdata_and_entities_past_a_document_type() {
    let tree = read(
        "<!DOCTYPE ldml SYSTEM \"ldml.dtd\"><ldml><a x=\"1\">b &amp; c<![CDATA[ <d> ]]></a></ldml>",
    )
    .unwrap();
    let element = tree.child("a").unwrap();
    assert_eq!(element.attribute("x"), Some("1"));
    assert_eq!(element.text, "b & c <d> ");
}

#[test]
fn a_child_skips_variants_and_unconfirmed_values() {
    let tree = read(
        "<ldml><a alt=\"short\">1</a><a draft=\"unconfirmed\">2</a>\
         <a draft=\"contributed\">3</a></ldml>",
    )
    .unwrap();
    assert_eq!(tree.child("a").unwrap().text, "3");
    assert_eq!(tree.path(&["a"]).unwrap().text, "3");
}

#[test]
fn reads_the_parent_locales_every_component_uses() {
    let found = parents();
    assert_eq!(found.get("es_MX").map(String::as_str), Some("es_419"));
    assert_eq!(found.get("sr_Latn").map(String::as_str), Some("root"));
    assert_eq!(found.get("yue"), None);
}

#[test]
fn a_parent_is_the_one_named_else_the_locale_less_its_last_subtag() {
    let found = parents();
    assert_eq!(parent("es_MX", &found).as_deref(), Some("es_419"));
    assert_eq!(parent("es_419", &found).as_deref(), Some("es"));
    assert_eq!(parent("es", &found).as_deref(), Some("root"));
    assert_eq!(parent("root", &found), None);
    assert_eq!(
        chain("sr_Latn_RS", &found),
        ["sr_Latn_RS", "sr_Latn", "root"]
    );
}
