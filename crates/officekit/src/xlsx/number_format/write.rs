//! A number laid onto the placeholders of a section (ECMA-376 Part 1, section 18.8.31).

use super::decimal::{Decimal, count, increment};
use super::general::{MOST, unsigned};
use super::parse::{Placeholder, Token};
use super::{FormatColor, Piece, Section, Shown};

/// The most digits of a fraction's denominator searched (section 18.8.31, `/`).
const DENOMINATOR_DIGITS: usize = 9;

/// The decimal places of a number's part below one that a fraction is found from.
const FRACTION_PLACES: usize = 20;

/// The text each token of a section shows, and the pieces they make, `Text` pieces joined.
#[derive(Default)]
pub(super) struct Writer {
    pieces: Vec<Piece>,
}

impl Writer {
    pub(super) fn push(&mut self, piece: Piece) {
        match (self.pieces.last_mut(), piece) {
            (_, Piece::Text(text)) if text.is_empty() => {}
            (Some(Piece::Text(before)), Piece::Text(text)) => before.push_str(&text),
            (_, other) => self.pieces.push(other),
        }
    }

    /// What the pieces show, in `color`.
    pub(super) fn shown(self, color: Option<FormatColor>) -> Shown {
        Shown {
            color,
            pieces: self.pieces,
        }
    }

    pub(super) fn text(&mut self, text: &str) {
        self.push(Piece::Text(text.to_owned()));
    }

    /// What `token` shows of itself, when it is a literal, a `%`, a `_` or a `*`.
    pub(super) fn token(&mut self, token: &Token) {
        if let Some(piece) = token.piece() {
            self.push(piece);
        }
    }
}

/// The whole digits of a fraction, its numerator and its denominator.
struct Fraction {
    denominator: String,
    numerator: String,
    whole: String,
}

/// `value` under `section`; the section writes the minus sign when `signed` and the value is
/// negative. `None` when the general format cannot write it.
pub(super) fn number(section: &Section, value: &Decimal, signed: bool) -> Option<Shown> {
    let mut writer = Writer::default();
    let has = |wanted: fn(&Token) -> bool| section.tokens.iter().any(wanted);
    let general = has(|token| *token == Token::General);
    let bare =
        section.tokens.is_empty() && (section.color.is_some() || section.condition.is_some());
    if !general && !bare && !has(|token| matches!(token, Token::Digit(_))) {
        for token in &section.tokens {
            writer.token(token);
        }
    } else {
        if signed && value.is_negative() {
            writer.text("-");
        }
        if general || bare {
            let text = unsigned(&value.abs(), MOST)?;
            if bare {
                writer.text(&text);
            }
            for token in &section.tokens {
                if *token == Token::General {
                    writer.text(&text);
                } else {
                    writer.token(token);
                }
            }
        } else {
            let scaled = value.abs().scaled(section.scale);
            if has(|token| matches!(token, Token::Exponent(..))) {
                exponent(&mut writer, section, &scaled);
            } else if has(|token| *token == Token::Slash) {
                fraction(&mut writer, section, &scaled);
            } else {
                plain(&mut writer, section, &scaled);
            }
        }
    }
    Some(Shown {
        color: section.color,
        pieces: writer.pieces,
    })
}

/// `text` under the text section `section`, at each `@`.
pub(super) fn text(section: &Section, text: &str) -> Shown {
    let mut writer = Writer::default();
    for token in &section.tokens {
        if *token == Token::Text {
            writer.text(text);
        } else {
            writer.token(token);
        }
    }
    Shown {
        color: section.color,
        pieces: writer.pieces,
    }
}

/// The digit placeholders among `tokens`.
fn placeholders(tokens: &[Token]) -> Vec<Placeholder> {
    tokens
        .iter()
        .filter_map(|token| {
            if let Token::Digit(placeholder) = token {
                Some(*placeholder)
            } else {
                None
            }
        })
        .collect()
}

/// The text of each placeholder for a whole number's `digits`, empty for zero: the last
/// placeholder takes the last digit, and the first every digit beyond the placeholders.
fn whole_texts(written: &str, places: &[Placeholder], thousands: bool) -> Vec<String> {
    let digits: Vec<char> = written.chars().collect();
    let mut texts = vec![String::new(); places.len()];
    for position in (0..places.len().max(digits.len())).rev() {
        let place = places.len().saturating_sub(position + 1);
        let character = if position < digits.len() {
            Some(digits[digits.len() - 1 - position])
        } else {
            match places[place] {
                Placeholder::Hash => None,
                Placeholder::Question => Some(' '),
                Placeholder::Zero => Some('0'),
            }
        };
        let Some(shown) = character else { continue };
        let Some(text) = texts.get_mut(place) else {
            continue;
        };
        text.push(shown);
        if thousands && position > 0 && position % 3 == 0 {
            text.push(if shown == ' ' { ' ' } else { ',' });
        }
    }
    texts
}

/// The text of each placeholder after the decimal point for exactly as many `digits`: past the
/// last digit that is not zero, `0` shows 0, `?` a space and `#` nothing.
fn decimal_texts(digits: &str, places: &[Placeholder]) -> Vec<String> {
    let significant = digits.rfind(|digit| digit != '0');
    digits
        .chars()
        .zip(places)
        .enumerate()
        .map(|(at, (digit, place))| {
            if significant.is_some_and(|last| at <= last) {
                return digit.to_string();
            }
            match place {
                Placeholder::Hash => String::new(),
                Placeholder::Question => " ".to_owned(),
                Placeholder::Zero => "0".to_owned(),
            }
        })
        .collect()
}

/// The text of each placeholder of a denominator: its digits from the first placeholder on,
/// the last taking any beyond them; a `?` with none shows a space.
fn denominator_texts(digits: &str, places: &[Placeholder]) -> Vec<String> {
    let mut texts: Vec<String> = places
        .iter()
        .enumerate()
        .map(|(at, place)| match digits.chars().nth(at) {
            Some(digit) => digit.to_string(),
            None if *place == Placeholder::Question => " ".to_owned(),
            None => String::new(),
        })
        .collect();
    if let Some(last) = texts.last_mut() {
        last.extend(digits.chars().skip(places.len()));
    }
    texts
}

/// Lay a number with no exponent and no fraction: its whole digits onto the placeholders before
/// the decimal point, and its decimals, rounded, onto those after it.
fn plain(writer: &mut Writer, section: &Section, value: &Decimal) {
    let point = section
        .tokens
        .iter()
        .position(|token| *token == Token::Point)
        .unwrap_or(section.tokens.len());
    let whole_places = placeholders(&section.tokens[..point]);
    let decimal_places = placeholders(&section.tokens[point..]);
    let rounded = value.rounded(count(decimal_places.len()));
    let whole = rounded.whole_digits();
    let mut texts = whole_texts(&whole, &whole_places, section.thousands);
    texts.extend(decimal_texts(
        &rounded.fraction_digits(decimal_places.len()),
        &decimal_places,
    ));
    let point_text = if whole_places.is_empty() {
        format!("{whole}.")
    } else {
        ".".to_owned()
    };
    lay(writer, &section.tokens, texts, |token| {
        if *token == Token::Point {
            point_text.clone()
        } else {
            String::new()
        }
    });
}

/// Lay a number with an exponent: the shown exponent is a multiple of the count of whole
/// placeholders of the mantissa, as `#0.0E+0` writes 12200000 as `12.2E+6` (section 18.8.31).
fn exponent(writer: &mut Writer, section: &Section, value: &Decimal) {
    let tokens = &section.tokens;
    let marker = tokens
        .iter()
        .position(|token| matches!(token, Token::Exponent(..)))
        .unwrap_or(tokens.len());
    let point = tokens[..marker]
        .iter()
        .position(|token| *token == Token::Point)
        .unwrap_or(marker);
    let whole_places = placeholders(&tokens[..point]);
    let decimal_places = placeholders(&tokens[point..marker]);
    let exponent_places = placeholders(&tokens[marker..]);
    let places = count(decimal_places.len());
    let shown_exponent = |first: i64| {
        let step = count(whole_places.len());
        if step == 0 {
            first + 1
        } else {
            first.div_euclid(step) * step
        }
    };
    let (mut power, mut mantissa) = (0_i64, value.clone());
    if let Some(first) = value.exponent() {
        power = shown_exponent(first);
        mantissa = value.scaled(-power).rounded(places);
        let limit = count(whole_places.len()) - 1;
        if mantissa.exponent().is_some_and(|carried| carried > limit) {
            power = shown_exponent(first + 1);
            mantissa = value.scaled(-power).rounded(places);
        }
    }
    let whole = mantissa.whole_digits();
    let mut texts = whole_texts(&whole, &whole_places, false);
    texts.extend(decimal_texts(
        &mantissa.fraction_digits(decimal_places.len()),
        &decimal_places,
    ));
    let magnitude = if power == 0 {
        String::new()
    } else {
        power.unsigned_abs().to_string()
    };
    texts.extend(whole_texts(&magnitude, &exponent_places, false));
    let point_text = if whole_places.is_empty() {
        format!("{whole}.")
    } else {
        ".".to_owned()
    };
    lay(writer, tokens, texts, |token| {
        if let Token::Exponent(letter, plus) = token {
            let sign = if power < 0 {
                "-"
            } else if *plus {
                "+"
            } else {
                ""
            };
            format!("{letter}{sign}")
        } else if *token == Token::Point {
            point_text.clone()
        } else {
            String::new()
        }
    });
}

/// Lay a fraction: the nearest one with a denominator of the placeholders' digits or the given
/// number, after the whole part when placeholders come before the numerator's (section 18.8.31).
fn fraction(writer: &mut Writer, section: &Section, value: &Decimal) {
    let tokens = &section.tokens;
    let slash = tokens
        .iter()
        .position(|token| *token == Token::Slash)
        .unwrap_or(tokens.len());
    let numerator_start = tokens[..slash]
        .iter()
        .rposition(|token| !matches!(token, Token::Digit(_)))
        .map_or(0, |at| at + 1);
    let whole_places = placeholders(&tokens[..numerator_start]);
    let numerator_places = placeholders(&tokens[numerator_start..slash]);
    let after = &tokens[slash..];
    let denominator_places = placeholders(
        &after[..after
            .iter()
            .skip(1)
            .position(|token| !matches!(token, Token::Digit(_)))
            .map_or(after.len(), |at| at + 1)],
    );
    let fixed = after.iter().find_map(|token| {
        if let Token::Denominator(digits) = token {
            digits.parse::<u128>().ok()
        } else {
            None
        }
    });
    let found = nearest(
        value,
        !whole_places.is_empty(),
        fixed,
        denominator_places.len(),
    );
    let blank = !whole_places.is_empty() && found.numerator.is_empty();
    let whole = if blank && found.whole.is_empty() {
        "0"
    } else {
        &found.whole
    };
    let mut texts = whole_texts(whole, &whole_places, section.thousands);
    let mut part = |digits: &str, places: &[Placeholder], denominator: bool| {
        if blank {
            texts.extend(places.iter().map(|_| " ".to_owned()));
        } else if denominator {
            texts.extend(denominator_texts(digits, places));
        } else {
            texts.extend(whole_texts(digits, places, false));
        }
    };
    part(&found.numerator, &numerator_places, false);
    part(&found.denominator, &denominator_places, true);
    lay(writer, tokens, texts, |token| {
        if let Token::Denominator(digits) = token {
            if blank {
                " ".repeat(digits.len())
            } else {
                digits.clone()
            }
        } else if *token == Token::Slash {
            (if blank { " " } else { "/" }).to_owned()
        } else {
            String::new()
        }
    });
}

/// `value` as a whole part and the nearest fraction with a denominator of `fixed`, or of at most
/// `places` digits. With no whole part the fraction holds the whole number. A zero numerator
/// with a whole part gives an empty numerator.
fn nearest(value: &Decimal, with_whole: bool, fixed: Option<u128>, places: usize) -> Fraction {
    let rounded = value.rounded(count(FRACTION_PLACES));
    let mut whole: Vec<u8> = rounded
        .whole_digits()
        .bytes()
        .map(|digit| digit - b'0')
        .collect();
    let below: u128 = rounded
        .fraction_digits(FRACTION_PLACES)
        .parse()
        .unwrap_or(0);
    let unit = 10_u128.pow(u32::try_from(FRACTION_PLACES).unwrap_or(0));
    let (mut numerator, denominator) = match fixed {
        Some(given) if given > 0 => {
            let scaled = below * given;
            (scaled / unit + u128::from(scaled % unit * 2 >= unit), given)
        }
        _ => {
            let limit = 10_u128.pow(u32::try_from(places.min(DENOMINATOR_DIGITS)).unwrap_or(0));
            best(below, unit, limit.saturating_sub(1).max(1))
        }
    };
    if with_whole {
        if numerator == denominator {
            increment(&mut whole);
            numerator = 0;
        }
        let text = |digits: &[u8]| {
            digits
                .iter()
                .map(|digit| char::from(b'0' + digit))
                .collect()
        };
        return Fraction {
            denominator: denominator.to_string(),
            numerator: if numerator == 0 {
                String::new()
            } else {
                numerator.to_string()
            },
            whole: text(&whole),
        };
    }
    Fraction {
        denominator: denominator.to_string(),
        numerator: times_plus(&whole, denominator, numerator),
        whole: String::new(),
    }
}

/// The fraction nearest `numerator / denominator`, below one, whose denominator is at most
/// `limit`: the last convergent of its continued fraction within the limit, or the
/// semiconvergent after it when that is nearer; the smaller denominator on a tie.
fn best(numerator: u128, denominator: u128, limit: u128) -> (u128, u128) {
    let (mut before, mut last) = ((0_u128, 1_u128), (1_u128, 0_u128));
    let (mut top, mut bottom) = (numerator, denominator);
    while bottom != 0 {
        let term = top / bottom;
        let next_q = term.saturating_mul(last.1).saturating_add(before.1);
        if next_q > limit {
            let steps = (limit - before.1) / last.1;
            let semi = (before.0 + steps * last.0, before.1 + steps * last.1);
            let error = |(p, q): (u128, u128)| (numerator * q).abs_diff(p * denominator);
            return if error(semi) * last.1 < error(last) * semi.1 {
                semi
            } else {
                last
            };
        }
        let next = (term * last.0 + before.0, next_q);
        before = last;
        last = next;
        (top, bottom) = (bottom, top - term * bottom);
    }
    last
}

/// The digits of `digits × factor + addend`, with no zero first; `0` for zero.
fn times_plus(digits: &[u8], factor: u128, addend: u128) -> String {
    let mut carry = addend;
    let mut reversed = Vec::new();
    for digit in digits.iter().rev() {
        let sum = u128::from(*digit) * factor + carry;
        reversed.push(sum % 10);
        carry = sum / 10;
    }
    while carry > 0 {
        reversed.push(carry % 10);
        carry /= 10;
    }
    while reversed.len() > 1 && reversed.last() == Some(&0) {
        reversed.pop();
    }
    if reversed.is_empty() {
        return "0".to_owned();
    }
    reversed
        .iter()
        .rev()
        .map(|digit| char::from_digit(u32::try_from(*digit).unwrap_or(0), 10).unwrap_or('0'))
        .collect()
}

/// Walk `tokens`, each digit placeholder showing the next of `texts`, a token that shows itself
/// as it does, and every other token what `special` gives it.
fn lay<F>(writer: &mut Writer, tokens: &[Token], texts: Vec<String>, special: F)
where
    F: Fn(&Token) -> String,
{
    let mut shown = texts.into_iter();
    for token in tokens {
        if let Some(piece) = token.piece() {
            writer.push(piece);
        } else if matches!(token, Token::Digit(_)) {
            if let Some(text) = shown.next() {
                writer.text(&text);
            }
        } else {
            writer.text(&special(token));
        }
    }
}
