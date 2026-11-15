//! A format code read into its sections and each section into its tokens (ECMA-376 Part 1,
//! section 18.8.31).

use core::mem;

use super::decimal::Decimal;
use super::{FormatColor, FormatError, Piece, Section};

/// The characters a code shows as they are (section 18.8.31, "Text and spacing").
const SHOWN: [char; 17] = [
    '$', '-', '+', '(', ')', ':', '!', '^', '&', '\'', '~', '{', '}', '<', '>', '=', ' ',
];

/// `上午/下午`, the half of the day in Chinese, which the standard's built-in codes 34 and 35
/// write (section 18.8.30).
const AM_PM_CHINESE: &str = "\u{4e0a}\u{5348}/\u{4e0b}\u{5348}";

/// The eight colours a section names in brackets (section 18.8.31, "Specify colors").
const COLORS: [(&str, FormatColor); 8] = [
    ("black", FormatColor::Black),
    ("blue", FormatColor::Blue),
    ("cyan", FormatColor::Cyan),
    ("green", FormatColor::Green),
    ("magenta", FormatColor::Magenta),
    ("red", FormatColor::Red),
    ("white", FormatColor::White),
    ("yellow", FormatColor::Yellow),
];

/// How a condition compares a number with its value (section 18.8.31, "Specify conditions").
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Comparison {
    Equal,
    Greater,
    GreaterOrEqual,
    Less,
    LessOrEqual,
    NotEqual,
}

/// A condition in brackets: the numbers its section serves.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Condition {
    comparison: Comparison,
    value: Decimal,
}

impl Condition {
    /// Whether `number` meets the condition.
    pub(super) fn holds(&self, number: &Decimal) -> bool {
        let order = number.cmp(&self.value);
        match self.comparison {
            Comparison::Equal => order.is_eq(),
            Comparison::Greater => order.is_gt(),
            Comparison::GreaterOrEqual => order.is_ge(),
            Comparison::Less => order.is_lt(),
            Comparison::LessOrEqual => order.is_le(),
            Comparison::NotEqual => order.is_ne(),
        }
    }

    /// The condition `text` states, as `<=100`; `None` when it states none.
    fn read(text: &str) -> Option<Self> {
        let (comparison, value) = [
            ("<=", Comparison::LessOrEqual),
            (">=", Comparison::GreaterOrEqual),
            ("<>", Comparison::NotEqual),
            ("<", Comparison::Less),
            (">", Comparison::Greater),
            ("=", Comparison::Equal),
        ]
        .into_iter()
        .find_map(|(operator, comparison)| {
            text.strip_prefix(operator).map(|value| (comparison, value))
        })?;
        Some(Self {
            comparison,
            value: Decimal::parse(value)?,
        })
    }
}

/// A digit placeholder (section 18.8.31).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Placeholder {
    /// `#`: a digit, or nothing.
    Hash,
    /// `?`: a digit, or a space.
    Question,
    /// `0`: a digit, or 0.
    Zero,
}

impl Section {
    /// Read the text of a bracket: a colour, a condition, a currency's text and language info, or
    /// an elapsed count.
    fn bracket(&mut self, text: &str) {
        let lower = text.to_ascii_lowercase();
        if let Some(currency) = text.strip_prefix('$') {
            let (shown, info) = currency
                .split_once('-')
                .map_or((currency, None), |(before, after)| (before, Some(after)));
            if !shown.is_empty() {
                self.tokens.push(Token::Literal(shown.to_owned()));
            }
            if let Some(id) = info.and_then(language_id) {
                self.language.get_or_insert(id);
            }
            return;
        }
        if let Some(named) = color(&lower) {
            self.color.get_or_insert(named);
            return;
        }
        if let Some(condition) = Condition::read(text) {
            self.condition.get_or_insert(condition);
            return;
        }
        let elapsed = lower.chars().next().filter(|letter| {
            matches!(letter, 'h' | 'm' | 's') && lower.chars().all(|other| other == *letter)
        });
        if let Some(letter) = elapsed {
            self.tokens.push(Token::Elapsed(letter, lower.len()));
        }
    }

    /// Drop every `*` but the last (section 18.8.31, `*`).
    fn last_fill(&mut self) {
        if let Some(last) = self
            .tokens
            .iter()
            .rposition(|token| matches!(token, Token::Fill(_)))
        {
            let mut at = 0_usize;
            self.tokens.retain(|token| {
                at += 1;
                at - 1 == last || !matches!(token, Token::Fill(_))
            });
        }
    }

    /// Settle what each token of a section for numbers does: which `.`, `E` and `/` act and
    /// which show as text, and what each `,` and `%` does to the number.
    fn settle_number(&mut self) {
        self.dates = self.tokens.iter().any(|token| {
            matches!(
                token,
                Token::AmPm(_) | Token::Calendar(_) | Token::Date(..) | Token::Elapsed(..)
            )
        });
        if self.dates {
            return;
        }
        let tokens = mem::take(&mut self.tokens);
        let is_digit = |token: &Token| matches!(token, Token::Digit(_));
        let exponent = tokens.iter().enumerate().position(|(at, token)| {
            matches!(token, Token::Exponent(..)) && tokens[at + 1..].iter().any(is_digit)
        });
        let fraction = exponent
            .is_none()
            .then(|| tokens.iter().position(|token| *token == Token::Slash))
            .flatten();
        let mantissa_end = exponent.unwrap_or(tokens.len());
        let point = tokens[..mantissa_end]
            .iter()
            .position(|token| *token == Token::Point)
            .filter(|_| fraction.is_none());
        let last_digit = tokens[..mantissa_end].iter().rposition(is_digit);
        for (at, token) in tokens.iter().enumerate() {
            let kept = match token {
                Token::Comma => {
                    let before = tokens[..at]
                        .iter()
                        .rev()
                        .find(|other| **other != Token::Comma);
                    let after = tokens[at + 1..]
                        .iter()
                        .find(|other| **other != Token::Comma);
                    match (before.is_some_and(is_digit), after.is_some_and(is_digit)) {
                        (true, true) => self.thousands |= at < point.unwrap_or(mantissa_end),
                        (true, false)
                            if last_digit.is_some_and(|last| at > last) && at < mantissa_end =>
                        {
                            self.scale -= 3;
                        }
                        _ => {}
                    }
                    None
                }
                Token::Denominator(digits) if fraction.is_none_or(|slash| slash + 1 != at) => {
                    Some(Token::Literal(digits.clone()))
                }
                Token::Exponent(letter, plus) if exponent != Some(at) => {
                    let sign = if *plus { '+' } else { '-' };
                    Some(Token::Literal(format!("{letter}{sign}")))
                }
                Token::Percent => {
                    self.scale += 2;
                    Some(Token::Percent)
                }
                Token::Point if point != Some(at) => Some(Token::Literal(".".to_owned())),
                Token::Slash if fraction != Some(at) => Some(Token::Literal("/".to_owned())),
                Token::Text => None,
                Token::AmPm(_)
                | Token::Calendar(_)
                | Token::Date(..)
                | Token::Denominator(_)
                | Token::Digit(_)
                | Token::Elapsed(..)
                | Token::Exponent(..)
                | Token::Fill(_)
                | Token::General
                | Token::Literal(_)
                | Token::Point
                | Token::Skip(_)
                | Token::Slash => Some(token.clone()),
            };
            self.tokens.extend(kept);
        }
    }

    /// Keep only what a text section shows: literals, `_`, `*` and `@`.
    fn settle_text(&mut self) {
        self.tokens.retain(|token| {
            matches!(
                token,
                Token::Fill(_) | Token::Literal(_) | Token::Skip(_) | Token::Text
            )
        });
    }
}

/// One item of a section (section 18.8.31).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Token {
    /// `AM/PM`, `A/P` or `上午/下午`, as the code writes it.
    AmPm(String),
    /// `b1` or `b2`: the calendar a date is written in.
    Calendar(u8),
    /// `,`, until the section is settled.
    Comma,
    /// A run of one letter of a date or a time, lowercased, and its length: `yyyy` is `('y', 4)`.
    Date(char, usize),
    /// A fraction's denominator written as a number, as in `# ?/8`.
    Denominator(String),
    Digit(Placeholder),
    /// `[h]`, `[mm]` or `[ss]`: a count of hours, minutes or seconds; the letter, and its length.
    Elapsed(char, usize),
    /// `E+`, `E-`, `e+` or `e-`: the letter, and whether a positive exponent shows its sign.
    Exponent(char, bool),
    Fill(char),
    General,
    Literal(String),
    Percent,
    Point,
    Skip(char),
    /// `/` between digit placeholders: a fraction.
    Slash,
    /// `@`: where a text goes.
    Text,
}

impl Token {
    /// What the token shows of itself, when it is a literal, a `%`, a `_` or a `*`.
    pub(super) fn piece(&self) -> Option<Piece> {
        match self {
            Self::Fill(fill) => Some(Piece::Fill(*fill)),
            Self::Literal(text) => Some(Piece::Text(text.clone())),
            Self::Percent => Some(Piece::Text("%".to_owned())),
            Self::Skip(skip) => Some(Piece::Skip(*skip)),
            Self::AmPm(_)
            | Self::Calendar(_)
            | Self::Comma
            | Self::Date(..)
            | Self::Denominator(_)
            | Self::Digit(_)
            | Self::Elapsed(..)
            | Self::Exponent(..)
            | Self::General
            | Self::Point
            | Self::Slash
            | Self::Text => None,
        }
    }
}

/// The colour a bracket's lowercased text names: one of eight, or `color1` to `color56`.
fn color(lower: &str) -> Option<FormatColor> {
    if let Some((_, named)) = COLORS.iter().find(|(name, _)| *name == lower) {
        return Some(*named);
    }
    let digits = lower.strip_prefix("color")?;
    if !(1..=2).contains(&digits.len()) || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits
        .parse::<u8>()
        .ok()
        .filter(|index| (1..=56).contains(index))
        .map(FormatColor::Palette)
}

/// The language id of a bracket's language info, its bytes 0 and 1 (section 18.8.31,
/// `[$USD-409]`); `None` when the info is not one to eight hexadecimal digits.
fn language_id(info: &str) -> Option<u16> {
    if info.is_empty() || !info.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(info, 16).ok()?;
    u16::try_from(value & 0xFFFF).ok()
}

/// The sections of `code` for numbers, and its section for text when it has one (section
/// 18.8.31): the fourth, or the last of fewer when it holds `@`.
pub(super) fn sections(code: &str) -> Result<(Vec<Section>, Option<Section>), FormatError> {
    let chars: Vec<char> = code.chars().collect();
    let mut sections = Vec::new();
    let mut current = Section::default();
    let mut at = 0_usize;
    while let Some(&found) = chars.get(at) {
        let error = FormatError { at };
        let rest = &chars[at..];
        let next = rest.get(1).copied();
        let tokens = &mut current.tokens;
        let mut push = |token: Token, length: usize| {
            tokens.push(token);
            length
        };
        at += match found {
            ';' if sections.len() == 3 => return Err(error),
            ';' => {
                sections.push(mem::take(&mut current));
                1
            }
            '[' => {
                let close = rest.iter().position(|other| *other == ']').ok_or(error)?;
                current.bracket(&rest[1..close].iter().collect::<String>());
                close + 1
            }
            '"' => {
                let close = rest[1..]
                    .iter()
                    .position(|other| *other == '"')
                    .ok_or(error)?;
                push(Token::Literal(written(&rest[1..], close)), close + 2)
            }
            '\\' => push(Token::Literal(next.ok_or(error)?.to_string()), 2),
            '_' => push(Token::Skip(next.ok_or(error)?), 2),
            '*' => push(Token::Fill(next.ok_or(error)?), 2),
            '0' => push(Token::Digit(Placeholder::Zero), 1),
            '#' => push(Token::Digit(Placeholder::Hash), 1),
            '?' => push(Token::Digit(Placeholder::Question), 1),
            '.' => push(Token::Point, 1),
            ',' => push(Token::Comma, 1),
            '%' => push(Token::Percent, 1),
            '@' => push(Token::Text, 1),
            '/' => slash(tokens, rest),
            'E' | 'e' if matches!(next, Some('+' | '-')) => {
                push(Token::Exponent(found, next == Some('+')), 2)
            }
            'G' | 'g' if starts_with(rest, "general") => push(Token::General, 7),
            'A' | 'a' if starts_with(rest, "am/pm") => push(Token::AmPm(written(rest, 5)), 5),
            'A' | 'a' if starts_with(rest, "a/p") => push(Token::AmPm(written(rest, 3)), 3),
            '\u{4e0a}' if starts_with(rest, AM_PM_CHINESE) => {
                push(Token::AmPm(written(rest, 5)), 5)
            }
            'B' | 'b' if matches!(next, Some('1' | '2')) => {
                push(Token::Calendar(if next == Some('1') { 1 } else { 2 }), 2)
            }
            'Y' | 'y' | 'M' | 'm' | 'D' | 'd' | 'H' | 'h' | 'S' | 's' | 'E' | 'e' | 'G' | 'g'
            | 'R' | 'r' => {
                let letter = found.to_ascii_lowercase();
                let length = rest
                    .iter()
                    .take_while(|other| other.to_ascii_lowercase() == letter)
                    .count();
                push(Token::Date(letter, length), length)
            }
            shown if SHOWN.contains(&shown) => push(Token::Literal(shown.to_string()), 1),
            _ => return Err(error),
        };
    }
    sections.push(current);
    for section in &mut sections {
        section.last_fill();
    }
    let holds_text = sections
        .last()
        .is_some_and(|last| last.tokens.contains(&Token::Text));
    let mut text = if sections.len() == 4 || holds_text {
        sections.pop()
    } else {
        None
    };
    if let Some(section) = text.as_mut() {
        section.settle_text();
    }
    for section in &mut sections {
        section.settle_number();
    }
    Ok((sections, text))
}

/// `/` at the start of `rest`: a fraction's, after a digit placeholder and before another or a
/// denominator's number, or else shown as it is (section 18.8.31, `/`).
fn slash(tokens: &mut Vec<Token>, rest: &[char]) -> usize {
    let after_digit = matches!(tokens.last(), Some(Token::Digit(_)));
    match rest.get(1) {
        Some('0' | '#' | '?') if after_digit => {
            tokens.push(Token::Slash);
            1
        }
        Some('1'..='9') if after_digit => {
            let digits: String = rest[1..]
                .iter()
                .take_while(|digit| digit.is_ascii_digit())
                .collect();
            let length = digits.len() + 1;
            tokens.push(Token::Slash);
            tokens.push(Token::Denominator(digits));
            length
        }
        _ => {
            tokens.push(Token::Literal("/".to_owned()));
            1
        }
    }
}

/// Whether `rest` starts with `word`, in any case.
fn starts_with(rest: &[char], word: &str) -> bool {
    let mut letters = rest.iter();
    word.chars().all(|expected| {
        letters
            .next()
            .is_some_and(|found| found.to_lowercase().eq(expected.to_lowercase()))
    })
}

/// The first `length` characters of `rest`, as the code writes them.
fn written(rest: &[char], length: usize) -> String {
    rest[..length].iter().collect()
}
