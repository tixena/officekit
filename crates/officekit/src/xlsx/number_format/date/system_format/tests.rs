use super::super::super::parse::Token;
use super::tokens;

#[test]
fn quoted_text_and_other_characters_are_literals() {
    assert_eq!(
        tokens("d 'de' MMMM ''yy 'o''clock'", "AM/PM"),
        [
            Token::Date('d', 1),
            Token::Literal(" de ".to_owned()),
            Token::Date('m', 4),
            Token::Literal(" '".to_owned()),
            Token::Date('y', 2),
            Token::Literal(" o'clock".to_owned()),
        ]
    );
}

#[test]
fn each_letter_as_the_sections_codes() {
    assert_eq!(
        tokens("y yyy EEEE EEEEE MMMMMM a HH K mm s", "am/pm"),
        [
            Token::Date('y', 4),
            Token::Literal(" ".to_owned()),
            Token::Date('y', 4),
            Token::Literal(" ".to_owned()),
            Token::Date('d', 4),
            Token::Literal(" ".to_owned()),
            Token::Date('d', 3),
            Token::Literal(" ".to_owned()),
            Token::Date('m', 5),
            Token::Literal(" ".to_owned()),
            Token::AmPm("am/pm".to_owned()),
            Token::Literal(" ".to_owned()),
            Token::Date('h', 2),
            Token::Literal(" ".to_owned()),
            Token::Date('h', 1),
            Token::Literal(" ".to_owned()),
            Token::Date('m', 2),
            Token::Literal(" ".to_owned()),
            Token::Date('s', 1),
        ]
    );
}

#[test]
fn an_era_a_quarter_a_week_and_a_zone_write_nothing() {
    assert_eq!(
        tokens("G QQQ w zzzz y", "AM/PM"),
        [Token::Literal("    ".to_owned()), Token::Date('y', 4)]
    );
}
