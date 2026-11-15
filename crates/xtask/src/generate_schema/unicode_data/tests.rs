use super::{DecimalDigit, decimal_digits};

#[test]
fn every_character_of_category_nd_is_read_with_its_name_and_value() {
    let text = "002F;SOLIDUS;Po;0;CS;;;;;N;SLASH;;;;\n\
                0030;DIGIT ZERO;Nd;0;EN;;0;0;0;N;;;;;\n\
                00B2;SUPERSCRIPT TWO;No;0;EN;<super> 0032;;2;2;N;;;;;\n\
                FF11;FULLWIDTH DIGIT ONE;Nd;0;EN;<wide> 0031;1;1;1;N;;;;;\n";
    assert_eq!(
        decimal_digits(text).unwrap(),
        [
            DecimalDigit {
                character: '0',
                name: "DIGIT ZERO".to_owned(),
                value: 0,
            },
            DecimalDigit {
                character: '\u{FF11}',
                name: "FULLWIDTH DIGIT ONE".to_owned(),
                value: 1,
            },
        ]
    );
}

#[test]
fn a_digit_without_a_decimal_value_stops_the_reading() {
    let err = decimal_digits("0030;DIGIT ZERO;Nd;0;EN;;;0;0;N;;;;;\n").unwrap_err();
    assert!(err.to_string().contains("cannot read 0030;DIGIT ZERO"));
}
