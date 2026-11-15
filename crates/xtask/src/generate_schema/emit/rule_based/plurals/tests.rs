use super::{PluralRule, Relation, conditions, read};
use crate::generate_schema::cldr;

fn relation(equal: bool, modulus: Option<u64>, ranges: &[(u64, u64)]) -> Relation {
    Relation {
        equal,
        modulus,
        ranges: ranges.to_vec(),
    }
}

#[test]
fn keeps_the_relations_on_the_number() {
    assert_eq!(
        conditions("n % 10 = 1 and n % 100 != 11,71 @integer 1, 21").unwrap(),
        Some(vec![vec![
            relation(true, Some(10), &[(1, 1)]),
            relation(false, Some(100), &[(11, 11), (71, 71)]),
        ]])
    );
    assert_eq!(
        conditions("i = 2..4 and v = 0").unwrap(),
        Some(vec![vec![relation(true, None, &[(2, 4)])]])
    );
}

#[test]
fn drops_a_condition_no_whole_number_meets() {
    assert_eq!(
        conditions("v != 0 or n = 1").unwrap(),
        Some(vec![vec![relation(true, None, &[(1, 1)])]])
    );
    assert_eq!(conditions("f = 1..9").unwrap(), None);
    assert_eq!(conditions("@integer 0~15").unwrap(), Some(vec![Vec::new()]));
}

#[test]
fn refuses_an_operand_or_relation_it_does_not_know() {
    conditions("x = 1").unwrap_err();
    conditions("n in 1").unwrap_err();
    conditions("n = one").unwrap_err();
}

#[test]
fn reads_the_rules_of_each_locale_but_other() {
    let file = cldr::read(
        "<supplementalData><plurals type=\"ordinal\">\
         <pluralRules locales=\"en fil\"><pluralRule count=\"one\">n % 10 = 1</pluralRule>\
         <pluralRule count=\"other\"> @integer 0</pluralRule></pluralRules>\
         </plurals></supplementalData>",
    )
    .unwrap();
    let rules = read(&file).unwrap();
    let one = vec![PluralRule {
        category: "one".to_owned(),
        conditions: vec![vec![relation(true, Some(10), &[(1, 1)])]],
    }];
    assert_eq!(rules.get("en"), Some(&one));
    assert_eq!(rules.get("fil"), Some(&one));
}
