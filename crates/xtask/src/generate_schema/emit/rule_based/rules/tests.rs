use super::{Part, Pattern, Rule, RuleSet, Target, read};

fn text(value: &str) -> Part {
    Part::Text(value.to_owned())
}

/// The pattern `#,##0`.
fn grouped() -> Pattern {
    Pattern {
        minimum: 1,
        primary: 3,
        secondary: 3,
        ..Pattern::default()
    }
}

#[test]
fn reads_rule_sets_and_their_whole_number_rules() {
    let sets = read(
        "\n%spellout-numbering:\n-x: menos >>;\nx.x: << punto >>;\nInf: infinito;\n0: cero;\n\
         30: treinta[ y >>];\n101: ciento >>;\n1000000: <%spellout-cardinal-masculine< millones;\n\
         100000/1000: << mil;\n%%private:\n0: =%spellout-numbering=;\n",
    )
    .unwrap();
    let rule = |base: u64, divisor: u64, parts: Vec<Part>| Rule {
        base,
        divisor,
        parts,
    };
    assert_eq!(
        sets,
        [
            RuleSet {
                name: "%spellout-numbering".to_owned(),
                negative: Some(vec![text("menos "), Part::Remainder(Target::Same)]),
                rules: vec![
                    rule(0, 1, vec![text("cero")]),
                    rule(
                        30,
                        10,
                        vec![
                            text("treinta"),
                            Part::Optional(
                                vec![text(" y "), Part::Remainder(Target::Same)],
                                Vec::new()
                            ),
                        ]
                    ),
                    rule(
                        101,
                        100,
                        vec![text("ciento "), Part::Remainder(Target::Same)]
                    ),
                    rule(
                        1_000_000,
                        1_000_000,
                        vec![
                            Part::Quotient(Target::Set("%spellout-cardinal-masculine".to_owned())),
                            text(" millones"),
                        ]
                    ),
                    rule(
                        100_000,
                        1_000,
                        vec![Part::Quotient(Target::Same), text(" mil")]
                    ),
                ],
            },
            RuleSet {
                name: "%%private".to_owned(),
                negative: None,
                rules: vec![rule(
                    0,
                    1,
                    vec![Part::Whole(Target::Set("%spellout-numbering".to_owned()))]
                )],
            },
        ]
    );
}

#[test]
fn reads_digits_plurals_alternatives_and_a_leading_apostrophe() {
    let sets = read(
        "%digits-ordinal:\n0: =#,##0=$(ordinal,one{st}other{th})$;\n\
         %spellout-ordinal:\n20: twent[y->>|ieth];\n1: ' and =#,##,##0.=;\n",
    )
    .unwrap();
    assert_eq!(
        sets.first().unwrap().rules.first().unwrap().parts,
        [
            Part::Whole(Target::Digits(grouped())),
            Part::Plural(
                "ordinal".to_owned(),
                vec![
                    ("one".to_owned(), "st".to_owned()),
                    ("other".to_owned(), "th".to_owned())
                ]
            ),
        ]
    );
    let ordinal = &sets[1].rules;
    assert_eq!(
        ordinal.first().unwrap().parts,
        [
            text("twent"),
            Part::Optional(
                vec![text("y-"), Part::Remainder(Target::Same)],
                vec![text("ieth")]
            ),
        ]
    );
    assert_eq!(
        ordinal[1].parts,
        [
            text(" and "),
            Part::Whole(Target::Digits(Pattern {
                point: true,
                secondary: 2,
                ..grouped()
            })),
        ]
    );
}

#[test]
fn a_lone_angle_is_text_and_three_angles_the_preceding_rule() {
    let sets = read("%a:\n10: <<>>>;\n100: 0* <#,##0<<;\n").unwrap();
    let rules = &sets.first().unwrap().rules;
    assert_eq!(
        rules.first().unwrap().parts,
        [Part::Quotient(Target::Same), Part::Preceding]
    );
    assert_eq!(
        rules[1].parts,
        [
            text("0* "),
            Part::Quotient(Target::Digits(grouped())),
            text("<"),
        ]
    );
}

#[test]
fn refuses_what_is_not_a_rule() {
    read("0: zero;\n").unwrap_err();
    read("%a:\nzero\n").unwrap_err();
    read("%a:\n0: [zero;\n").unwrap_err();
    read("%a:\n0: $(ordinal,one{st;\n").unwrap_err();
    read("%a:\n0: =#,##x=;\n").unwrap_err();
    read("%a:\n1.5: x;\n").unwrap_err();
}
