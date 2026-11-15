use alloc::collections::BTreeMap;

use super::{Cldr, Main, Symbols, Table, alphabet, groupings, main, numbering_systems, render};
use crate::generate_schema::cldr;

/// The rules of a locale: one `SpelloutRules` grouping and one `OrdinalRules` grouping.
fn rules(spellout: &str, ordinal: &str) -> String {
    format!(
        "<ldml><rbnf><rulesetGrouping type=\"SpelloutRules\"><rbnfRules><![CDATA[{spellout}]]>\
         </rbnfRules></rulesetGrouping><rulesetGrouping type=\"OrdinalRules\"><rbnfRules>\
         <![CDATA[{ordinal}]]></rbnfRules></rulesetGrouping></rbnf></ldml>"
    )
}

/// A main file with the list pattern `and` and the index set `index`.
fn main_file(and: &str, index: &str) -> Main {
    main(
        &cldr::read(&format!(
            "<ldml><characters><exemplarCharacters type=\"index\">{index}</exemplarCharacters>\
             </characters><listPatterns><listPattern><listPatternPart type=\"2\">{and}\
             </listPatternPart></listPattern></listPatterns></ldml>"
        ))
        .unwrap(),
    )
}

/// English, Spanish and its Latin American variant, with the root locale's digits.
fn data() -> Cldr {
    let mut found = Cldr {
        numbering_systems: BTreeMap::from([("latn".to_owned(), "0123456789".to_owned())]),
        parents: BTreeMap::from([("es_MX".to_owned(), "es_419".to_owned())]),
        ..Cldr::default()
    };
    let mut root = main_file("{0}, {1}", "[A B C]");
    root.numbering = Some("latn".to_owned());
    root.symbols.insert(
        "latn".to_owned(),
        Symbols::Stated(Some(".".to_owned()), Some(",".to_owned())),
    );
    found.main.insert("root".to_owned(), root);
    found.main.insert(
        "en".to_owned(),
        main_file(
            "{0} and {1}",
            "[A B C D E F G H I J K L M N O P Q R S T U V W X Y Z]",
        ),
    );
    found.main.insert(
        "es".to_owned(),
        main_file(
            "{0} y {1}",
            "[A B C D E F G H I J K L M N \u{d1} O P Q R S T U V W X Y Z]",
        ),
    );
    found.main.insert("es_MX".to_owned(), Main::default());
    let groups = |text: &str| groupings(&cldr::read(text).unwrap()).unwrap();
    found.rules.insert(
        "root".to_owned(),
        groups(&rules(
            "%spellout-numbering:\n0: =#,##0=;\n",
            "%digits-ordinal:\n0: =#,##0=.;\n",
        )),
    );
    found.rules.insert(
        "en".to_owned(),
        groups(&rules(
            "%spellout-numbering:\n0: =%%cardinal=;\n%%cardinal:\n0: zero;\n1: one;\n",
            "%digits-ordinal:\n0: =#,##0=$(ordinal,one{st}other{th})$;\n",
        )),
    );
    found.rules.insert(
        "es".to_owned(),
        groups(
            &rules(
                "%spellout-numbering:\n0: cero;\n1: uno;\n%spellout-ordinal-masculine:\n1: primero;\n",
                "",
            )
            .replace(
                "<rulesetGrouping type=\"OrdinalRules\"><rbnfRules><![CDATA[]]></rbnfRules>\
                 </rulesetGrouping>",
                "",
            ),
        ),
    );
    found.rules.insert(
        "es_419".to_owned(),
        groups(&rules("", "%digits-ordinal:\n0: =#,##0=\u{ba};\n").replace(
            "<rulesetGrouping type=\"SpelloutRules\"><rbnfRules><![CDATA[]]></rbnfRules>\
             </rulesetGrouping>",
            "",
        )),
    );
    found.cardinal_plurals.insert("root".to_owned(), Vec::new());
    found.ordinal_plurals.insert("root".to_owned(), Vec::new());
    found
}

#[test]
fn a_locale_takes_each_grouping_and_fact_from_the_nearest_of_its_chain() {
    let found = data();
    let mexico = found.facts("es_MX").unwrap().words.unwrap();
    assert_eq!(mexico.and, "{0} y {1}");
    assert_eq!(
        mexico.cardinal,
        Some((
            "es".to_owned(),
            "SpelloutRules".to_owned(),
            "%spellout-numbering".to_owned()
        ))
    );
    assert_eq!(
        mexico.ordinal_text,
        Some((
            "es".to_owned(),
            "SpelloutRules".to_owned(),
            "%spellout-ordinal-masculine".to_owned()
        ))
    );
    assert_eq!(
        mexico.ordinal,
        Some((
            "es_419".to_owned(),
            "OrdinalRules".to_owned(),
            "%digits-ordinal".to_owned()
        ))
    );
    assert_eq!(
        (mexico.digits.as_str(), mexico.group.as_str()),
        ("0123456789", ",")
    );
    let spanish = found.facts("es").unwrap();
    assert_eq!(
        spanish.words.unwrap().ordinal.map(|key| key.0),
        Some("root".to_owned())
    );
    assert_eq!(
        spanish.letters.unwrap().1.get(14).map(String::as_str),
        Some("\u{f1}")
    );
}

#[test]
fn a_locale_with_no_rules_of_its_own_has_no_words() {
    let found = data();
    assert_eq!(found.facts("xx").unwrap(), super::Facts::default());
    assert_eq!(found.facts("en").unwrap().letters, None);
    assert_eq!(found.facts("en").unwrap().words.unwrap().ordinal_text, None);
}

#[test]
fn the_table_holds_each_locale_that_differs_from_its_parent_and_the_sets_reached() {
    let table = Table::build(&data(), &[]).unwrap();
    let ids: Vec<&str> = table.locales.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(ids, ["en", "es", "es_419"]);
    let names: Vec<&str> = table
        .rule_sets
        .iter()
        .map(|set| set.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "%digits-ordinal",
            "%%cardinal",
            "%spellout-numbering",
            "%spellout-numbering",
            "%spellout-ordinal-masculine",
            "%digits-ordinal",
            "%digits-ordinal"
        ]
    );
    let text = render("// header\n", "Copyright", &table).unwrap();
    assert!(text.contains("Rule::new(1, 1, &[Part::Text(\"one\")])"));
    assert!(text.contains("Part::Whole(Target::Set(1))"));
    assert!(text.contains("(Category::Other, \"th\")"));
    assert!(text.contains("pub(super) static WORDS: [Words; 3] = ["));
}

#[test]
fn a_format_spelled_to_a_limit_takes_its_rule_set_from_any_grouping() {
    let mut found = data();
    found.rules.get_mut("root").unwrap().extend(
        groupings(
            &cldr::read(
                "<ldml><rbnf><rulesetGrouping type=\"NumberingSystemRules\"><rbnfRules>\
                 <![CDATA[%hebrew:\n1: =%%item=;\n%%item:\n1: \u{5d0};\n]]></rbnfRules>\
                 </rulesetGrouping></rbnf></ldml>",
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let formats = [
        ("Hebrew1", "root", "NumberingSystemRules", "%hebrew"),
        ("HindiCounting", "en", "SpelloutRules", "%%cardinal"),
    ];
    let table = Table::build(&found, &formats).unwrap();
    let named: Vec<(&str, &str)> = table
        .formats
        .iter()
        .map(|(variant, at)| (variant.as_str(), table.rule_sets[*at].name.as_str()))
        .collect();
    assert_eq!(
        named,
        [("Hebrew1", "%hebrew"), ("HindiCounting", "%%cardinal")]
    );
    assert!(table.rule_sets.iter().any(|set| set.name == "%%item"));
    let text = render("// header\n", "Copyright", &table).unwrap();
    assert!(text.contains("pub(super) static FORMATS: [(StNumberFormatMember, usize); 2] = ["));
    assert!(text.contains("(StNumberFormatMember::Hebrew1, "));

    let missing = [("Hebrew1", "root", "NumberingSystemRules", "%greek")];
    let err = Table::build(&found, &missing).unwrap_err();
    assert_eq!(
        err.to_string(),
        "root's NumberingSystemRules holds no rule set %greek"
    );
}

#[test]
fn a_rule_set_named_that_its_grouping_lacks_stops_the_table() {
    let mut found = data();
    found.rules.insert(
        "en".to_owned(),
        groupings(
            &cldr::read(&rules(
                "%spellout-numbering:\n0: =%%missing=;\n",
                "%digits-ordinal:\n0: x;\n",
            ))
            .unwrap(),
        )
        .unwrap(),
    );
    let err = Table::build(&found, &[]).unwrap_err();
    assert_eq!(
        err.to_string(),
        "en's SpelloutRules holds no rule set %%missing"
    );
}

#[test]
fn a_rule_the_evaluator_does_not_follow_stops_the_table() {
    let mut found = data();
    found.rules.insert(
        "en".to_owned(),
        groupings(
            &cldr::read(&rules(
                "%spellout-numbering:\n0: zero;\n10: <<>>>;\n",
                "%digits-ordinal:\n0: x;\n",
            ))
            .unwrap(),
        )
        .unwrap(),
    );
    let err = Table::build(&found, &[]).unwrap_err();
    assert!(err.to_string().contains("does not follow `>>>`"));
}

#[test]
fn an_alphabet_is_kept_when_latin_and_not_a_to_z() {
    let (upper, lower) = alphabet("[A B {CH} \u{d8} \\u00C5]").unwrap().unwrap();
    assert_eq!(upper, ["A", "B", "CH", "\u{d8}", "\u{c5}"]);
    assert_eq!(lower, ["a", "b", "ch", "\u{f8}", "\u{e5}"]);
    assert_eq!(alphabet("[\u{3042} \u{304b} \u{3055}]").unwrap(), None);
    assert_eq!(
        alphabet("[A B C D E F G H I J K L M N O P Q R S T U V W X Y Z]").unwrap(),
        None
    );
    alphabet("[A-Z]").unwrap_err();
    alphabet("A B").unwrap_err();
}

#[test]
fn reads_the_symbols_of_each_numbering_system_and_their_aliases() {
    let file = cldr::read(
        "<ldml><numbers><defaultNumberingSystem>arab</defaultNumberingSystem>\
         <defaultNumberingSystem alt=\"latn\">latn</defaultNumberingSystem>\
         <symbols numberSystem=\"latn\"><decimal>,</decimal><group>.</group></symbols>\
         <symbols numberSystem=\"beng\"><alias source=\"locale\" \
         path=\"../symbols[@numberSystem='latn']\"/></symbols></numbers></ldml>",
    )
    .unwrap();
    let found = main(&file);
    assert_eq!(found.numbering.as_deref(), Some("arab"));
    assert_eq!(
        found.symbols.get("latn"),
        Some(&Symbols::Stated(Some(",".to_owned()), Some(".".to_owned())))
    );
    assert_eq!(
        found.symbols.get("beng"),
        Some(&Symbols::Alias("latn".to_owned()))
    );
    let systems = numbering_systems(
        &cldr::read(
            "<supplementalData><numberingSystems>\
             <numberingSystem id=\"latn\" type=\"numeric\" digits=\"0123456789\"/>\
             <numberingSystem id=\"roman\" type=\"algorithmic\" rules=\"%roman-upper\"/>\
             </numberingSystems></supplementalData>",
        )
        .unwrap(),
    );
    assert_eq!(
        systems,
        BTreeMap::from([("latn".to_owned(), "0123456789".to_owned())])
    );
}
