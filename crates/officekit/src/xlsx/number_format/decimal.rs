//! A number read from the text of an XML Schema double as a sign, decimal digits and a power of
//! ten, scaled and rounded in whole numbers.

#[cfg(test)]
mod tests;

use core::cmp::Ordering;

/// The greatest power of ten of a double's first digit.
const LARGEST: i64 = 308;

/// The least power of ten of a double's first digit; a number below it is zero.
const SMALLEST: i64 = -324;

/// A finite number: its digits times ten to `power`, negative when `negative`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Decimal {
    /// One digit, 0 to 9, per entry; no zero first or last, and none at all for zero.
    digits: Vec<u8>,
    negative: bool,
    /// The power of ten of the last digit.
    power: i64,
}

impl Decimal {
    /// The magnitude of this number.
    pub(super) fn abs(&self) -> Self {
        Self {
            digits: self.digits.clone(),
            negative: false,
            power: self.power,
        }
    }

    /// The digit worth ten to `power`.
    fn digit_at(&self, power: i64) -> u8 {
        power
            .checked_sub(self.power)
            .and_then(|steps| usize::try_from(steps).ok())
            .and_then(|steps| self.digits.len().checked_sub(steps.checked_add(1)?))
            .and_then(|index| self.digits.get(index))
            .copied()
            .unwrap_or(0)
    }

    /// The power of ten of the first digit; `None` for zero.
    pub(super) fn exponent(&self) -> Option<i64> {
        (!self.digits.is_empty()).then(|| self.power + count(self.digits.len()) - 1)
    }

    /// The number with no exponent: its whole part, and its decimals when it has any.
    pub(super) fn fixed(&self) -> String {
        let whole = self.whole_digits();
        let places = usize::try_from(-self.power).unwrap_or(0);
        let mut text = if whole.is_empty() {
            "0".to_owned()
        } else {
            whole
        };
        if places > 0 {
            text.push('.');
            text.push_str(&self.fraction_digits(places));
        }
        text
    }

    /// `places` digits after the decimal point, as text, the digits beyond them dropped.
    pub(super) fn fraction_digits(&self, places: usize) -> String {
        (1..=count(places))
            .map(|place| char::from(b'0' + self.digit_at(-place)))
            .collect()
    }

    pub(super) const fn is_negative(&self) -> bool {
        self.negative
    }

    pub(super) const fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// The number of `digits` times ten to `power`, with no zero first or last.
    fn normal(negative: bool, mut digits: Vec<u8>, mut power: i64) -> Self {
        let leading = digits.iter().take_while(|digit| **digit == 0).count();
        digits.drain(..leading);
        while digits.last() == Some(&0) {
            digits.pop();
            power += 1;
        }
        if digits.is_empty() {
            return Self::zero();
        }
        Self {
            digits,
            negative,
            power,
        }
    }

    /// The number the text of an XML Schema double states; `None` for `INF`, `-INF`, `NaN`, no
    /// number, and a number beyond a double's range (XML Schema Part 2, section 3.2.5).
    pub(super) fn parse(written: &str) -> Option<Self> {
        let text = written.trim_matches([' ', '\t', '\n', '\r']);
        let (negative, unsigned) = match text.as_bytes().first() {
            Some(b'-') => (true, &text[1..]),
            Some(b'+') => (false, &text[1..]),
            _ => (false, text),
        };
        let (mantissa, exponent_text) = unsigned
            .split_once(['e', 'E'])
            .map_or((unsigned, None), |(before, after)| (before, Some(after)));
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let decimal = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
        if (whole.is_empty() && fraction.is_empty()) || !decimal(whole) || !decimal(fraction) {
            return None;
        }
        let exponent = match exponent_text {
            Some(stated) => {
                let digits = stated.strip_prefix(['+', '-']).unwrap_or(stated);
                if digits.is_empty() || !decimal(digits) {
                    return None;
                }
                stated.parse::<i64>().ok()?
            }
            None => 0,
        };
        let digits = whole
            .bytes()
            .chain(fraction.bytes())
            .map(|byte| byte - b'0')
            .collect();
        let power = exponent.checked_sub(count(fraction.len()))?;
        let number = Self::normal(negative, digits, power);
        match number.exponent() {
            Some(first) if first > LARGEST => None,
            Some(first) if first < SMALLEST => Some(Self::zero()),
            _ => Some(number),
        }
    }

    /// This number rounded to `places` decimal places, a half away from zero.
    pub(super) fn rounded(&self, places: i64) -> Self {
        let least = -places;
        if self.power >= least {
            return self.clone();
        }
        let dropped = least - self.power;
        let Ok(kept) = usize::try_from(count(self.digits.len()) - dropped) else {
            return Self::zero();
        };
        let mut digits = self.digits[..kept].to_vec();
        if self.digits[kept] >= 5 {
            increment(&mut digits);
        }
        Self::normal(self.negative, digits, least)
    }

    /// This number times ten to `power`.
    pub(super) fn scaled(&self, power: i64) -> Self {
        if self.is_zero() {
            return self.clone();
        }
        Self {
            digits: self.digits.clone(),
            negative: self.negative,
            power: self.power + power,
        }
    }

    /// This number times `factor`, rounded to a whole number, a half upward; `None` beyond an
    /// `i128`.
    pub(super) fn times_rounded(&self, factor: u128) -> Option<i128> {
        let mut carry = 0_u128;
        let mut reversed = Vec::with_capacity(self.digits.len());
        for digit in self.digits.iter().rev() {
            let sum = u128::from(*digit).checked_mul(factor)?.checked_add(carry)?;
            reversed.push(u8::try_from(sum % 10).ok()?);
            carry = sum / 10;
        }
        while carry > 0 {
            reversed.push(u8::try_from(carry % 10).ok()?);
            carry /= 10;
        }
        reversed.reverse();
        let product = Self::normal(false, reversed, self.power);
        let whole_text = product.whole_digits();
        let whole = if whole_text.is_empty() {
            0
        } else {
            whole_text.parse::<i128>().ok()?
        };
        let half = product.digit_at(-1).cmp(&5).then(if product.power < -1 {
            Ordering::Greater
        } else {
            Ordering::Equal
        });
        if self.negative {
            whole.checked_add(i128::from(half.is_gt()))?.checked_neg()
        } else {
            whole.checked_add(i128::from(half.is_ge()))
        }
    }

    /// The digits of the whole part, with no zero first; empty when it is zero.
    pub(super) fn whole_digits(&self) -> String {
        match self.exponent() {
            Some(first) if first >= 0 => (0..=first)
                .rev()
                .map(|power| char::from(b'0' + self.digit_at(power)))
                .collect(),
            _ => String::new(),
        }
    }

    const fn zero() -> Self {
        Self {
            digits: Vec::new(),
            negative: false,
            power: 0,
        }
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let sign = |number: &Self| match (number.is_zero(), number.negative) {
            (true, _) => 0_i8,
            (false, true) => -1_i8,
            (false, false) => 1_i8,
        };
        let by_sign = sign(self).cmp(&sign(other));
        if by_sign != Ordering::Equal || self.is_zero() {
            return by_sign;
        }
        let magnitude = self.exponent().cmp(&other.exponent()).then_with(|| {
            let first = self.exponent().unwrap_or(0);
            let last = self.power.min(other.power);
            (last..=first)
                .rev()
                .map(|power| self.digit_at(power).cmp(&other.digit_at(power)))
                .find(|order| *order != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        });
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// `length` as a count that a power of ten can be added to.
pub(super) fn count(length: usize) -> i64 {
    i64::try_from(length).unwrap_or(i64::MAX)
}

/// Add one to the last of `digits`, carrying.
pub(super) fn increment(digits: &mut Vec<u8>) {
    for digit in digits.iter_mut().rev() {
        if *digit == 9 {
            *digit = 0;
        } else {
            *digit += 1;
            return;
        }
    }
    digits.insert(0, 1);
}
