//! Document protection: the `w:documentProtection` entry of the settings part, in the form
//! Aspose.Words writes for the Transitional class, or in the form ECMA-376 Part 1 defines for the
//! Strict one.

#[cfg(test)]
mod tests;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha1::{Digest as _, Sha1};

use crate::markup::common_simple_types::{
    CryptographicAlgorithmClasses, CryptographicAlgorithmTypes, CryptographicProviderTypes,
    OnOffValue,
};
use crate::markup::wordprocessing::{
    DecimalNumberValue, DocumentEditingRestrictions, DocumentProtectionTypesMember,
};
use crate::xml::conformance::ConformanceClass;

/// The rounds Aspose.Words 24.6.0 runs after the first hash.
pub const SPIN_COUNT: u32 = 50_000;

const INITIAL_CODE: [u16; 15] = [
    0xE1F0, 0x1D0F, 0xCC9C, 0x84C0, 0x110C, 0x0E10, 0xF1CE, 0x313E, 0x1872, 0xE139, 0xD40F, 0x84F9,
    0x280C, 0xA96A, 0x4EC3,
];

const ENCRYPTION_MATRIX: [[u16; 7]; 15] = [
    [0xAEFC, 0x4DD9, 0x9BB2, 0x2745, 0x4E8A, 0x9D14, 0x2A09],
    [0x7B61, 0xF6C2, 0xFDA5, 0xEB6B, 0xC6F7, 0x9DCF, 0x2BBF],
    [0x4563, 0x8AC6, 0x05AD, 0x0B5A, 0x16B4, 0x2D68, 0x5AD0],
    [0x0375, 0x06EA, 0x0DD4, 0x1BA8, 0x3750, 0x6EA0, 0xDD40],
    [0xD849, 0xA0B3, 0x5147, 0xA28E, 0x553D, 0xAA7A, 0x44D5],
    [0x6F45, 0xDE8A, 0xAD35, 0x4A4B, 0x9496, 0x390D, 0x721A],
    [0xEB23, 0xC667, 0x9CEF, 0x29FF, 0x53FE, 0xA7FC, 0x5FD9],
    [0x47D3, 0x8FA6, 0x0F6D, 0x1EDA, 0x3DB4, 0x7B68, 0xF6D0],
    [0xB861, 0x60E3, 0xC1C6, 0x93AD, 0x377B, 0x6EF6, 0xDDEC],
    [0x45A0, 0x8B40, 0x06A1, 0x0D42, 0x1A84, 0x3508, 0x6A10],
    [0xAA51, 0x4483, 0x8906, 0x022D, 0x045A, 0x08B4, 0x1168],
    [0x76B4, 0xED68, 0xCAF1, 0x85C3, 0x1BA7, 0x374E, 0x6E9C],
    [0x3730, 0x6E60, 0xDCC0, 0xA9A1, 0x4363, 0x86C6, 0x1DAD],
    [0x3331, 0x6662, 0xCCC4, 0x89A9, 0x0373, 0x06E6, 0x0DCC],
    [0x1021, 0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48C4],
];

/// The low byte of `value`.
fn low_byte(value: u32) -> u8 {
    u8::try_from(value & 0xFF_u32).unwrap_or(0)
}

/// `unit` as its two UTF-16LE bytes, low first.
fn utf16_le(unit: u16) -> [u8; 2] {
    let value = u32::from(unit);
    [low_byte(value), low_byte(value >> 8_u32)]
}

/// `value` as four little-endian bytes.
fn u32_le(value: u32) -> [u8; 4] {
    [
        low_byte(value),
        low_byte(value >> 8_u32),
        low_byte(value >> 16_u32),
        low_byte(value >> 24_u32),
    ]
}

/// Word's legacy 32-bit key of a password (ECMA-376 Part 4, 14.8.1): the first 15 UTF-16 code
/// units, each reduced to its low byte, or its high byte when the low one is zero.
fn legacy_key(password: &str) -> u32 {
    let bytes: Vec<u8> = password
        .encode_utf16()
        .take(15)
        .map(|unit| {
            let [low, high] = utf16_le(unit);
            if low == 0 { high } else { low }
        })
        .collect();
    let Some(last) = bytes.len().checked_sub(1) else {
        return 0;
    };
    let mut high = INITIAL_CODE[last];
    for (index, byte) in bytes.iter().enumerate() {
        let row = &ENCRYPTION_MATRIX[15 - bytes.len() + index];
        for (bit, value) in row.iter().enumerate() {
            if byte & (1_u8 << bit) != 0 {
                high ^= value;
            }
        }
    }
    let rotate = |low: u16| ((low >> 14_u16) & 1_u16) | ((low << 1_u16) & 0x7FFF_u16);
    let mut low = 0_u16;
    for byte in bytes.iter().rev() {
        low = rotate(low) ^ u16::from(*byte);
    }
    low = rotate(low) ^ u16::try_from(bytes.len()).unwrap_or(0) ^ 0xCE4B;
    (u32::from(high) << 16) | u32::from(low)
}

/// SHA-1 of the salt then the input, then `SPIN_COUNT` rounds, each hashing the last hash then
/// the round's number as four little-endian bytes; the first hash is not a round.
fn spin(salt: &[u8], input: &[u8]) -> [u8; 20] {
    let mut hash: [u8; 20] = Sha1::new()
        .chain_update(salt)
        .chain_update(input)
        .finalize()
        .into();
    for round in 0..SPIN_COUNT {
        hash = Sha1::new()
            .chain_update(hash)
            .chain_update(u32_le(round))
            .finalize()
            .into();
    }
    hash
}

/// The `w:hash` Aspose.Words writes: the legacy key's four bytes, little-endian, as eight
/// upper-case hex digits in UTF-16LE, through the rounds.
#[must_use]
pub fn legacy_hash(password: &str, salt: &[u8]) -> [u8; 20] {
    let input: Vec<u8> = u32_le(legacy_key(password))
        .into_iter()
        .flat_map(|byte| [byte >> 4_u8, byte & 0x0F_u8])
        .filter_map(|nibble| char::from_digit(u32::from(nibble), 16))
        .map(|digit| digit.to_ascii_uppercase())
        .flat_map(|digit| utf16_le(u16::from(u8::try_from(digit).unwrap_or(b'0'))))
        .collect();
    spin(salt, &input)
}

/// The `w:hashValue` of Part 1, 17.15.1.29: the password in UTF-16LE through the rounds, with no
/// legacy step.
#[must_use]
pub fn hash(password: &str, salt: &[u8]) -> [u8; 20] {
    let input: Vec<u8> = password.encode_utf16().flat_map(utf16_le).collect();
    spin(salt, &input)
}

/// The `w:documentProtection` entry. An empty password gives `w:edit` and `w:enforcement` alone,
/// as Aspose.Words writes it.
#[must_use]
pub fn entry(
    edit: DocumentProtectionTypesMember,
    password: &str,
    salt: &[u8; 16],
    class: ConformanceClass,
) -> DocumentEditingRestrictions {
    let mut entry = DocumentEditingRestrictions {
        edit: Some(edit.into()),
        enforcement: Some(OnOffValue("1".to_owned())),
        ..DocumentEditingRestrictions::default()
    };
    if password.is_empty() {
        return entry;
    }
    let salt_text = STANDARD.encode(salt);
    let spin_count = DecimalNumberValue(SPIN_COUNT.to_string());
    match class {
        ConformanceClass::Transitional => {
            entry.crypt_provider_type = Some(CryptographicProviderTypes("rsaFull".to_owned()));
            entry.crypt_algorithm_class = Some(CryptographicAlgorithmClasses("hash".to_owned()));
            entry.crypt_algorithm_type = Some(CryptographicAlgorithmTypes("typeAny".to_owned()));
            entry.crypt_algorithm_sid = Some(DecimalNumberValue("4".to_owned()));
            entry.crypt_spin_count = Some(spin_count);
            entry.hash = Some(STANDARD.encode(legacy_hash(password, salt)));
            entry.salt = Some(salt_text);
        }
        ConformanceClass::Strict => {
            entry.algorithm_name = Some("SHA-1".to_owned());
            entry.hash_value = Some(STANDARD.encode(hash(password, salt)));
            entry.salt_value = Some(salt_text);
            entry.spin_count = Some(spin_count);
        }
    }
    entry
}
