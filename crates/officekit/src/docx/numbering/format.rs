//! A number in each number format of ECMA-376 Part 1, section 17.18.59.

#[cfg(test)]
mod tests;

use alloc::string::String;
use alloc::vec::Vec;

use officekit_markup::wordprocessing::StNumberFormatMember;

use super::digits::DIGIT_ZEROS;
use super::rule_based::{self, Spelled};

/// `aiueo`, positions 1 to 46.
const AIUEO: [&str; 46] = [
    "\u{FF71}", "\u{FF72}", "\u{FF73}", "\u{FF74}", "\u{FF75}", "\u{FF76}", "\u{FF77}", "\u{FF78}",
    "\u{FF79}", "\u{FF7A}", "\u{FF7B}", "\u{FF7C}", "\u{FF7D}", "\u{FF7E}", "\u{FF7F}", "\u{FF80}",
    "\u{FF81}", "\u{FF82}", "\u{FF83}", "\u{FF84}", "\u{FF85}", "\u{FF86}", "\u{FF87}", "\u{FF88}",
    "\u{FF89}", "\u{FF8A}", "\u{FF8B}", "\u{FF8C}", "\u{FF8D}", "\u{FF8E}", "\u{FF8F}", "\u{FF90}",
    "\u{FF91}", "\u{FF92}", "\u{FF93}", "\u{FF94}", "\u{FF95}", "\u{FF96}", "\u{FF97}", "\u{FF98}",
    "\u{FF99}", "\u{FF9A}", "\u{FF9B}", "\u{FF9C}", "\u{FF66}", "\u{FF9D}",
];

/// `aiueoFullWidth`: the 48 characters the section lists for its positions.
const AIUEO_FULL_WIDTH: [&str; 48] = [
    "\u{30A2}", "\u{30A4}", "\u{30A6}", "\u{30A8}", "\u{30AA}", "\u{30AB}", "\u{30AD}", "\u{30AF}",
    "\u{30B1}", "\u{30B3}", "\u{30B5}", "\u{30B7}", "\u{30B9}", "\u{30BB}", "\u{30BD}", "\u{30BF}",
    "\u{30C1}", "\u{30C4}", "\u{30C6}", "\u{30C8}", "\u{30CA}", "\u{30CB}", "\u{30CC}", "\u{30CD}",
    "\u{30CE}", "\u{30CF}", "\u{30D2}", "\u{30D5}", "\u{30D8}", "\u{30DB}", "\u{30DE}", "\u{30DF}",
    "\u{30E0}", "\u{30E1}", "\u{30E2}", "\u{30E4}", "\u{30E6}", "\u{30E8}", "\u{30E9}", "\u{30EA}",
    "\u{30EB}", "\u{30EC}", "\u{30ED}", "\u{30EF}", "\u{30F0}", "\u{30F1}", "\u{30F2}", "\u{30F3}",
];

/// `arabicAbjad`, positions 1 to 28.
const ARABIC_ABJAD: [&str; 28] = [
    "\u{623}", "\u{628}", "\u{62C}", "\u{62F}", "\u{647}", "\u{648}", "\u{632}", "\u{62D}",
    "\u{637}", "\u{64A}", "\u{643}", "\u{644}", "\u{645}", "\u{646}", "\u{633}", "\u{639}",
    "\u{641}", "\u{635}", "\u{642}", "\u{631}", "\u{634}", "\u{62A}", "\u{62B}", "\u{62E}",
    "\u{630}", "\u{636}", "\u{63A}", "\u{638}",
];

/// `arabicAlpha`, positions 1 to 28.
const ARABIC_ALPHA: [&str; 28] = [
    "\u{623}", "\u{628}", "\u{62A}", "\u{62B}", "\u{62C}", "\u{62D}", "\u{62E}", "\u{62F}",
    "\u{630}", "\u{631}", "\u{632}", "\u{633}", "\u{634}", "\u{635}", "\u{636}", "\u{637}",
    "\u{638}", "\u{639}", "\u{63A}", "\u{641}", "\u{642}", "\u{643}", "\u{644}", "\u{645}",
    "\u{646}", "\u{647}", "\u{648}", "\u{64A}",
];

/// The text `bahtText` writes after the `thaiCounting` text.
const BAHT: &str = "\u{E1A}\u{E32}\u{E17}\u{E16}\u{E49}\u{E27}\u{E19}";

/// `chicago`, positions 1 to 4.
const CHICAGO: [&str; 4] = ["*", "\u{2020}", "\u{2021}", "\u{A7}"];

/// `chineseCounting` and `taiwaneseCounting`, 0 to 10.
const COUNTING: [&str; 11] = [
    "\u{25CB}", "\u{4E00}", "\u{4E8C}", "\u{4E09}", "\u{56DB}", "\u{4E94}", "\u{516D}", "\u{4E03}",
    "\u{516B}", "\u{4E5D}", "\u{5341}",
];

/// `chineseCountingThousand`.
const CHINESE_COUNTING_THOUSAND: Grouped = Grouped {
    digits: HAN_DIGITS,
    one_ten: false,
    powers: [
        Some("\u{4E07}"),
        Some("\u{5343}"),
        Some("\u{767E}"),
        Some("\u{5341}"),
    ],
    zero: [Some(1_000), Some(100), Some(10)],
};

/// `chineseLegalSimplified`.
const CHINESE_LEGAL_SIMPLIFIED: Grouped = Grouped {
    digits: [
        "\u{58F9}", "\u{8D30}", "\u{53C1}", "\u{8086}", "\u{4F0D}", "\u{9646}", "\u{67D2}",
        "\u{634C}", "\u{7396}",
    ],
    one_ten: false,
    powers: [
        Some("\u{4E07}"),
        Some("\u{4EDF}"),
        Some("\u{4F70}"),
        Some("\u{62FE}"),
    ],
    zero: [Some(1_000), Some(100), Some(10)],
};

/// `chosung`, positions 1 to 14.
const CHOSUNG: [&str; 14] = [
    "\u{3131}", "\u{3134}", "\u{3137}", "\u{3139}", "\u{3141}", "\u{3142}", "\u{3145}", "\u{3147}",
    "\u{3148}", "\u{314A}", "\u{314B}", "\u{314C}", "\u{314D}", "\u{314E}",
];

/// `decimal` and `decimalHalfWidth`, 0 to 9.
const DECIMAL: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

/// `decimalEnclosedCircle` and `decimalEnclosedCircleChinese`, 1 to 20.
const DECIMAL_ENCLOSED_CIRCLE: [&str; 20] = [
    "\u{2460}", "\u{2461}", "\u{2462}", "\u{2463}", "\u{2464}", "\u{2465}", "\u{2466}", "\u{2467}",
    "\u{2468}", "\u{2469}", "\u{246A}", "\u{246B}", "\u{246C}", "\u{246D}", "\u{246E}", "\u{246F}",
    "\u{2470}", "\u{2471}", "\u{2472}", "\u{2473}",
];

/// `decimalEnclosedFullstop`, 1 to 20.
const DECIMAL_ENCLOSED_FULLSTOP: [&str; 20] = [
    "\u{2488}", "\u{2489}", "\u{248A}", "\u{248B}", "\u{248C}", "\u{248D}", "\u{248E}", "\u{248F}",
    "\u{2490}", "\u{2491}", "\u{2492}", "\u{2493}", "\u{2494}", "\u{2495}", "\u{2496}", "\u{2497}",
    "\u{2498}", "\u{2499}", "\u{249A}", "\u{249B}",
];

/// `decimalEnclosedParen`, 1 to 20.
const DECIMAL_ENCLOSED_PAREN: [&str; 20] = [
    "\u{2474}", "\u{2475}", "\u{2476}", "\u{2477}", "\u{2478}", "\u{2479}", "\u{247A}", "\u{247B}",
    "\u{247C}", "\u{247D}", "\u{247E}", "\u{247F}", "\u{2480}", "\u{2481}", "\u{2482}", "\u{2483}",
    "\u{2484}", "\u{2485}", "\u{2486}", "\u{2487}",
];

/// `decimalFullWidth`, 0 to 9.
const DECIMAL_FULL_WIDTH: [&str; 10] = [
    "\u{FF10}", "\u{FF11}", "\u{FF12}", "\u{FF13}", "\u{FF14}", "\u{FF15}", "\u{FF16}", "\u{FF17}",
    "\u{FF18}", "\u{FF19}",
];

/// `ganada`, positions 1 to 14.
const GANADA: [&str; 14] = [
    "\u{AC00}", "\u{B098}", "\u{B2E4}", "\u{B77C}", "\u{B9C8}", "\u{BC14}", "\u{C0AC}", "\u{C544}",
    "\u{C790}", "\u{CC28}", "\u{CE74}", "\u{D0C0}", "\u{D30C}", "\u{D558}",
];

/// The digits 1 to 9 of `chineseCountingThousand`, `japaneseCounting` and
/// `taiwaneseCountingThousand`.
const HAN_DIGITS: [&str; 9] = [
    "\u{4E00}", "\u{4E8C}", "\u{4E09}", "\u{56DB}", "\u{4E94}", "\u{516D}", "\u{4E03}", "\u{516B}",
    "\u{4E5D}",
];

/// `hebrew1`: the symbol of a remainder of 15.
const HEBREW_FIFTEEN: &str = "\u{5D8}\u{5D5}";

/// `hebrew1`: the symbols of the hundreds digit, 1 to 9.
const HEBREW_HUNDREDS: [&str; 9] = [
    "\u{5E7}", "\u{5E8}", "\u{5E9}", "\u{5EA}", "\u{5DA}", "\u{5DD}", "\u{5DF}", "\u{5E3}",
    "\u{5E5}",
];

/// `hebrew1`: the symbols of the thousands and the ones digit, 1 to 9.
const HEBREW_ONES: [&str; 9] = [
    "\u{5D0}", "\u{5D1}", "\u{5D2}", "\u{5D3}", "\u{5D4}", "\u{5D5}", "\u{5D6}", "\u{5D7}",
    "\u{5D8}",
];

/// `hebrew1`: the symbol of a remainder of 16.
const HEBREW_SIXTEEN: &str = "\u{5D8}\u{5D6}";

/// `hebrew1`: the symbols of the tens digit, 1 to 9.
const HEBREW_TENS: [&str; 9] = [
    "\u{5D9}", "\u{5DB}", "\u{5DC}", "\u{5DE}", "\u{5E0}", "\u{5E1}", "\u{5E2}", "\u{5E4}",
    "\u{5E6}",
];

/// `hebrew2`, positions 1 to 22.
const HEBREW2: [&str; 22] = [
    "\u{5D0}", "\u{5D1}", "\u{5D2}", "\u{5D3}", "\u{5D4}", "\u{5D5}", "\u{5D6}", "\u{5D7}",
    "\u{5D8}", "\u{5D9}", "\u{5DB}", "\u{5DC}", "\u{5DE}", "\u{5E0}", "\u{5E1}", "\u{5E2}",
    "\u{5E4}", "\u{5E6}", "\u{5E7}", "\u{5E8}", "\u{5E9}", "\u{5EA}",
];

/// `hebrew2`: the symbol written once for each time the size of the set is subtracted.
const HEBREW2_TAV: &str = "\u{5EA}";

/// `hex`, 0 to 15.
const HEX: [&str; 16] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F",
];

/// `hindiConsonants`, positions 1 to 18.
const HINDI_CONSONANTS: [&str; 18] = [
    "\u{905}",
    "\u{906}",
    "\u{907}",
    "\u{908}",
    "\u{909}",
    "\u{90A}",
    "\u{90B}",
    "\u{90C}",
    "\u{90D}",
    "\u{90E}",
    "\u{90F}",
    "\u{910}",
    "\u{911}",
    "\u{912}",
    "\u{913}",
    "\u{914}",
    "\u{905}\u{902}",
    "\u{905}\u{903}",
];

/// `hindiCounting`, 1 to 10: the names the section's example gives.
const HINDI_COUNTING: [&str; 10] = [
    "\u{90F}\u{915}",
    "\u{926}\u{94B}",
    "\u{924}\u{940}\u{928}",
    "\u{91A}\u{93E}\u{930}",
    "\u{92A}\u{93E}\u{901}\u{91A}",
    "\u{91B}\u{903}",
    "\u{938}\u{93E}\u{924}",
    "\u{906}\u{920}",
    "\u{928}\u{94C}",
    "\u{926}\u{938}",
];

/// `hindiNumbers`, 0 to 9.
const HINDI_NUMBERS: [&str; 10] = [
    "\u{966}", "\u{967}", "\u{968}", "\u{969}", "\u{96A}", "\u{96B}", "\u{96C}", "\u{96D}",
    "\u{96E}", "\u{96F}",
];

/// `hindiVowels`, positions 1 to 37.
const HINDI_VOWELS: [&str; 37] = [
    "\u{915}", "\u{916}", "\u{917}", "\u{918}", "\u{919}", "\u{91A}", "\u{91B}", "\u{91C}",
    "\u{91D}", "\u{91E}", "\u{91F}", "\u{920}", "\u{921}", "\u{922}", "\u{923}", "\u{924}",
    "\u{925}", "\u{926}", "\u{927}", "\u{928}", "\u{929}", "\u{92A}", "\u{92B}", "\u{92C}",
    "\u{92D}", "\u{92E}", "\u{92F}", "\u{930}", "\u{931}", "\u{932}", "\u{933}", "\u{934}",
    "\u{935}", "\u{936}", "\u{937}", "\u{938}", "\u{939}",
];

/// `ideographDigital` and `japaneseDigitalTenThousand`, 0 to 9.
const IDEOGRAPH_DIGITAL: [&str; 10] = [
    "\u{3007}", "\u{4E00}", "\u{4E8C}", "\u{4E09}", "\u{56DB}", "\u{4E94}", "\u{516D}", "\u{4E03}",
    "\u{516B}", "\u{4E5D}",
];

/// `ideographEnclosedCircle`, 1 to 10.
const IDEOGRAPH_ENCLOSED_CIRCLE: [&str; 10] = [
    "\u{3220}", "\u{3221}", "\u{3222}", "\u{3223}", "\u{3224}", "\u{3225}", "\u{3226}", "\u{3227}",
    "\u{3228}", "\u{3229}",
];

/// `ideographLegalTraditional`.
const IDEOGRAPH_LEGAL_TRADITIONAL: Grouped = Grouped {
    digits: [
        "\u{58F9}", "\u{8CB3}", "\u{53C3}", "\u{8086}", "\u{4F0D}", "\u{9678}", "\u{67D2}",
        "\u{634C}", "\u{7396}",
    ],
    one_ten: false,
    powers: [
        Some("\u{842C}"),
        Some("\u{4EDF}"),
        Some("\u{4F70}"),
        Some("\u{62FE}"),
    ],
    zero: [None, None, None],
};

/// `ideographTraditional`, 1 to 10.
const IDEOGRAPH_TRADITIONAL: [&str; 10] = [
    "\u{7532}", "\u{4E59}", "\u{4E19}", "\u{4E01}", "\u{620A}", "\u{5DF1}", "\u{5E9A}", "\u{8F9B}",
    "\u{58EC}", "\u{7678}",
];

/// `ideographZodiac`, 1 to 12.
const IDEOGRAPH_ZODIAC: [&str; 12] = [
    "\u{5B50}", "\u{4E11}", "\u{5BC5}", "\u{536F}", "\u{8FB0}", "\u{5DF3}", "\u{5348}", "\u{672A}",
    "\u{7533}", "\u{9149}", "\u{620C}", "\u{4EA5}",
];

/// `ideographZodiacTraditional`, positions 1 to 60: each pair of an element and an animal of
/// `ideographZodiac`.
const IDEOGRAPH_ZODIAC_TRADITIONAL: [&str; 60] = [
    "\u{7532}\u{5B50}",
    "\u{4E59}\u{4E11}",
    "\u{4E19}\u{5BC5}",
    "\u{4E01}\u{536F}",
    "\u{620A}\u{8FB0}",
    "\u{5DF1}\u{5DF3}",
    "\u{5E9A}\u{5348}",
    "\u{8F9B}\u{672A}",
    "\u{58EC}\u{7533}",
    "\u{7678}\u{9149}",
    "\u{7532}\u{620C}",
    "\u{4E59}\u{4EA5}",
    "\u{4E19}\u{5B50}",
    "\u{4E01}\u{4E11}",
    "\u{620A}\u{5BC5}",
    "\u{5DF1}\u{536F}",
    "\u{5E9A}\u{8FB0}",
    "\u{8F9B}\u{5DF3}",
    "\u{58EC}\u{5348}",
    "\u{7678}\u{672A}",
    "\u{7532}\u{7533}",
    "\u{4E59}\u{9149}",
    "\u{4E19}\u{620C}",
    "\u{4E01}\u{4EA5}",
    "\u{620A}\u{5B50}",
    "\u{5DF1}\u{4E11}",
    "\u{5E9A}\u{5BC5}",
    "\u{8F9B}\u{536F}",
    "\u{58EC}\u{8FB0}",
    "\u{7678}\u{5DF3}",
    "\u{7532}\u{5348}",
    "\u{4E59}\u{672A}",
    "\u{4E19}\u{7533}",
    "\u{4E01}\u{9149}",
    "\u{620A}\u{620C}",
    "\u{5DF1}\u{4EA5}",
    "\u{5E9A}\u{5B50}",
    "\u{8F9B}\u{4E11}",
    "\u{58EC}\u{5BC5}",
    "\u{7678}\u{536F}",
    "\u{7532}\u{8FB0}",
    "\u{4E59}\u{5DF3}",
    "\u{4E19}\u{5348}",
    "\u{4E01}\u{672A}",
    "\u{620A}\u{7533}",
    "\u{5DF1}\u{9149}",
    "\u{5E9A}\u{620C}",
    "\u{8F9B}\u{4EA5}",
    "\u{58EC}\u{5B50}",
    "\u{7678}\u{4E11}",
    "\u{7532}\u{5BC5}",
    "\u{4E59}\u{536F}",
    "\u{4E19}\u{8FB0}",
    "\u{4E01}\u{5DF3}",
    "\u{620A}\u{5348}",
    "\u{5DF1}\u{672A}",
    "\u{5E9A}\u{7533}",
    "\u{8F9B}\u{9149}",
    "\u{58EC}\u{620C}",
    "\u{7678}\u{4EA5}",
];

/// `iroha`, positions 1 to 48.
const IROHA: [&str; 48] = [
    "\u{FF72}", "\u{FF9B}", "\u{FF8A}", "\u{FF86}", "\u{FF8E}", "\u{FF8D}", "\u{FF84}", "\u{FF81}",
    "\u{FF98}", "\u{FF87}", "\u{FF99}", "\u{FF66}", "\u{FF9C}", "\u{FF76}", "\u{FF96}", "\u{FF80}",
    "\u{FF9A}", "\u{FF7F}", "\u{FF82}", "\u{FF88}", "\u{FF85}", "\u{FF97}", "\u{FF91}", "\u{FF73}",
    "\u{30F0}", "\u{FF89}", "\u{FF75}", "\u{FF78}", "\u{FF94}", "\u{FF8F}", "\u{FF79}", "\u{FF8C}",
    "\u{FF7A}", "\u{FF74}", "\u{FF83}", "\u{FF71}", "\u{FF7B}", "\u{FF77}", "\u{FF95}", "\u{FF92}",
    "\u{FF90}", "\u{FF7C}", "\u{30F1}", "\u{FF8B}", "\u{FF93}", "\u{FF7E}", "\u{FF7D}", "\u{FF9D}",
];

/// `irohaFullWidth`, positions 1 to 48.
const IROHA_FULL_WIDTH: [&str; 48] = [
    "\u{30A4}", "\u{30ED}", "\u{30CF}", "\u{30CB}", "\u{30DB}", "\u{30D8}", "\u{30C8}", "\u{30C1}",
    "\u{30EA}", "\u{30CC}", "\u{30EB}", "\u{30F2}", "\u{30EF}", "\u{30AB}", "\u{30E8}", "\u{30BF}",
    "\u{30EC}", "\u{30BD}", "\u{30C4}", "\u{30CD}", "\u{30CA}", "\u{30E9}", "\u{30E0}", "\u{30A6}",
    "\u{30F0}", "\u{30CE}", "\u{30AA}", "\u{30AF}", "\u{30E4}", "\u{30DE}", "\u{30B1}", "\u{30D5}",
    "\u{30B3}", "\u{30A8}", "\u{30C6}", "\u{30A2}", "\u{30B5}", "\u{30AD}", "\u{30E6}", "\u{30E1}",
    "\u{30DF}", "\u{30B7}", "\u{30F1}", "\u{30D2}", "\u{30E2}", "\u{30BB}", "\u{30B9}", "\u{30F3}",
];

/// `japaneseCounting`, which names no symbol for ten thousand.
const JAPANESE_COUNTING: Grouped = Grouped {
    digits: HAN_DIGITS,
    one_ten: true,
    powers: [None, Some("\u{5343}"), Some("\u{767E}"), Some("\u{5341}")],
    zero: [None, None, None],
};

/// `japaneseLegal`.
const JAPANESE_LEGAL: Grouped = Grouped {
    digits: [
        "\u{58F1}", "\u{5F10}", "\u{53C2}", "\u{56DB}", "\u{4F0D}", "\u{516D}", "\u{4E03}",
        "\u{516B}", "\u{4E5D}",
    ],
    one_ten: false,
    powers: [
        Some("\u{842C}"),
        Some("\u{9621}"),
        Some("\u{767E}"),
        Some("\u{62FE}"),
    ],
    zero: [None, None, None],
};

/// `koreanCounting`.
const KOREAN_COUNTING: Grouped = Grouped {
    digits: [
        "\u{C77C}", "\u{C774}", "\u{C0BC}", "\u{C0AC}", "\u{C624}", "\u{C721}", "\u{CE60}",
        "\u{D314}", "\u{AD6C}",
    ],
    one_ten: true,
    powers: [
        Some("\u{B9CC}"),
        Some("\u{CC9C}"),
        Some("\u{BC31}"),
        Some("\u{C2ED}"),
    ],
    zero: [None, None, None],
};

/// `koreanDigital`, 0 to 9.
const KOREAN_DIGITAL: [&str; 10] = [
    "\u{C601}", "\u{C77C}", "\u{C774}", "\u{C0BC}", "\u{C0AC}", "\u{C624}", "\u{C721}", "\u{CE60}",
    "\u{D314}", "\u{AD6C}",
];

/// `koreanDigital2`, 0 to 9.
const KOREAN_DIGITAL2: [&str; 10] = [
    "\u{96F6}", "\u{4E00}", "\u{4E8C}", "\u{4E09}", "\u{56DB}", "\u{4E94}", "\u{516D}", "\u{4E03}",
    "\u{516B}", "\u{4E5D}",
];

/// `koreanLegal`: the words of the ones digit, 1 to 9.
const KOREAN_LEGAL_ONES: [&str; 9] = [
    "\u{D558}\u{B098}",
    "\u{B458}",
    "\u{C14B}",
    "\u{B137}",
    "\u{B2E4}\u{C12F}",
    "\u{C5EC}\u{C12F}",
    "\u{C77C}\u{ACF1}",
    "\u{C5EC}\u{B35F}",
    "\u{C544}\u{D649}",
];

/// `koreanLegal`: the words of the tens digit, 1 to 9.
const KOREAN_LEGAL_TENS: [&str; 9] = [
    "\u{C5F4}",
    "\u{C2A4}\u{BB3C}",
    "\u{C11C}\u{B978}",
    "\u{B9C8}\u{D754}",
    "\u{C270}",
    "\u{C608}\u{C21C}",
    "\u{C77C}\u{D754}",
    "\u{C5EC}\u{B4E0}",
    "\u{C544}\u{D754}",
];

/// Past this many repetitions of one character a text is written in decimal, so that a level
/// starting at a huge number cannot ask for a string of that length.
const LONGEST_RUN: usize = 10_000;

/// `lowerLetter`, positions 1 to 26.
const LOWER_LETTER: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
    "t", "u", "v", "w", "x", "y", "z",
];

/// `lowerRoman`: each group, largest first.
const LOWER_ROMAN: [(u64, &str); 13] = [
    (1_000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

/// `russianLower`, positions 1 to 29.
const RUSSIAN_LOWER: [&str; 29] = [
    "\u{430}", "\u{431}", "\u{432}", "\u{433}", "\u{434}", "\u{435}", "\u{436}", "\u{437}",
    "\u{438}", "\u{43A}", "\u{43B}", "\u{43C}", "\u{43D}", "\u{43E}", "\u{43F}", "\u{440}",
    "\u{441}", "\u{442}", "\u{443}", "\u{444}", "\u{445}", "\u{446}", "\u{447}", "\u{448}",
    "\u{449}", "\u{44B}", "\u{44D}", "\u{44E}", "\u{44F}",
];

/// `russianUpper`, positions 1 to 29.
const RUSSIAN_UPPER: [&str; 29] = [
    "\u{410}", "\u{411}", "\u{412}", "\u{413}", "\u{414}", "\u{415}", "\u{416}", "\u{417}",
    "\u{418}", "\u{41A}", "\u{41B}", "\u{41C}", "\u{41D}", "\u{41E}", "\u{41F}", "\u{420}",
    "\u{421}", "\u{422}", "\u{423}", "\u{424}", "\u{425}", "\u{426}", "\u{427}", "\u{428}",
    "\u{429}", "\u{42B}", "\u{42D}", "\u{42E}", "\u{42F}",
];

/// `taiwaneseCountingThousand`.
const TAIWANESE_COUNTING_THOUSAND: Grouped = Grouped {
    digits: HAN_DIGITS,
    one_ten: true,
    powers: [
        Some("\u{842C}"),
        Some("\u{5343}"),
        Some("\u{767E}"),
        Some("\u{5341}"),
    ],
    zero: [Some(10), None, Some(10)],
};

/// `taiwaneseDigital`, 0 to 9.
const TAIWANESE_DIGITAL: [&str; 10] = [
    "\u{25CB}", "\u{4E00}", "\u{4E8C}", "\u{4E09}", "\u{56DB}", "\u{4E94}", "\u{516D}", "\u{4E03}",
    "\u{516B}", "\u{4E5D}",
];

/// `thaiCounting`, 1 to 10: the names the section's example gives.
const THAI_COUNTING: [&str; 10] = [
    "\u{E2B}\u{E19}\u{E36}\u{E48}\u{E07}",
    "\u{E2A}\u{E2D}\u{E07}",
    "\u{E2A}\u{E32}\u{E21}",
    "\u{E2A}\u{E35}\u{E48}",
    "\u{E2B}\u{E49}\u{E32}",
    "\u{E2B}\u{E01}",
    "\u{E40}\u{E08}\u{E47}\u{E14}",
    "\u{E41}\u{E1B}\u{E14}",
    "\u{E40}\u{E01}\u{E49}\u{E32}",
    "\u{E2A}\u{E34}\u{E1A}",
];

/// `thaiLetters`, positions 1 to 41.
const THAI_LETTERS: [&str; 41] = [
    "\u{E01}", "\u{E02}", "\u{E04}", "\u{E07}", "\u{E08}", "\u{E09}", "\u{E0A}", "\u{E0B}",
    "\u{E0C}", "\u{E0D}", "\u{E0E}", "\u{E0F}", "\u{E10}", "\u{E11}", "\u{E12}", "\u{E13}",
    "\u{E14}", "\u{E15}", "\u{E16}", "\u{E17}", "\u{E18}", "\u{E19}", "\u{E1A}", "\u{E1B}",
    "\u{E1C}", "\u{E1D}", "\u{E1E}", "\u{E1F}", "\u{E20}", "\u{E21}", "\u{E22}", "\u{E23}",
    "\u{E25}", "\u{E27}", "\u{E28}", "\u{E29}", "\u{E2A}", "\u{E2B}", "\u{E2C}", "\u{E2D}",
    "\u{E2E}",
];

/// `thaiNumbers`, 0 to 9.
const THAI_NUMBERS: [&str; 10] = [
    "\u{E50}", "\u{E51}", "\u{E52}", "\u{E53}", "\u{E54}", "\u{E55}", "\u{E56}", "\u{E57}",
    "\u{E58}", "\u{E59}",
];

/// `upperLetter`, positions 1 to 26.
const UPPER_LETTER: [&str; 26] = [
    "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S",
    "T", "U", "V", "W", "X", "Y", "Z",
];

/// `upperRoman`: each group, largest first.
const UPPER_ROMAN: [(u64, &str); 13] = [
    (1_000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

/// `vietnameseCounting`, 1 to 10: the names the section's example gives.
const VIETNAMESE_COUNTING: [&str; 10] = [
    "m\u{1ED9}t",
    "hai",
    "ba",
    "b\u{1ED1}n",
    "n\u{103}m",
    "s\u{E1}u",
    "b\u{1EA3}y",
    "t\u{E1}m",
    "ch\u{ED}n",
    "m\u{1B0}\u{1EDD}i",
];

/// The symbol that stands for a missing group in `chineseCountingThousand`,
/// `chineseLegalSimplified` and `taiwaneseCountingThousand`.
const ZERO_GROUP: &str = "\u{96F6}";

/// A counting system that writes, for each group of ten thousand, a thousand, a hundred and ten,
/// the digit of the group and the symbol of its power.
struct Grouped {
    /// The digits 1 to 9.
    digits: [&'static str; 9],
    /// Whether a single group of ten is the symbol of ten alone.
    one_ten: bool,
    /// The symbols of ten thousand, a thousand, a hundred and ten.
    powers: [Option<&'static str>; 4],
    /// For the groups of a thousand, a hundred and ten: the number a value must not be a multiple
    /// of for the zero symbol to stand for the group when it is missing.
    zero: [Option<u64>; 3],
}

/// `value` in the number format `format` (ECMA-376 Part 1, section 17.18.59), in decimal where
/// the section gives the value no text.
///
/// `None` for `bullet`, `custom`, `decimalFullWidth2` and the formats in the run's language,
/// `cardinalText`, `ordinalText`, `ordinal` and `dollarText`, which [`format_number_in`] writes.
#[must_use = "this only returns the value"]
pub fn format_number(value: i64, format: StNumberFormatMember) -> Option<String> {
    use StNumberFormatMember as F;

    let whole = u64::try_from(value).ok();
    let positive = whole.filter(|number| *number > 0);
    let text = match format {
        F::Aiueo => positive.and_then(|n| repeated(n, &AIUEO)),
        F::AiueoFullWidth => positive.and_then(|n| repeated(n, &AIUEO_FULL_WIDTH)),
        F::ArabicAbjad => positive.and_then(|n| repeated(n, &ARABIC_ABJAD)),
        F::ArabicAlpha => positive.and_then(|n| repeated(n, &ARABIC_ALPHA)),
        F::BahtText => positive
            .and_then(|n| within(n, &THAI_COUNTING))
            .map(|counted| format!("{counted}{BAHT}")),
        F::Chicago => positive.and_then(|n| repeated(n, &CHICAGO)),
        F::ChineseCounting | F::TaiwaneseCounting => whole.map(counting),
        F::ChineseCountingThousand => positive.and_then(|n| grouped(n, &CHINESE_COUNTING_THOUSAND)),
        F::ChineseLegalSimplified => positive.and_then(|n| grouped(n, &CHINESE_LEGAL_SIMPLIFIED)),
        F::Chosung => positive.and_then(|n| repeated(n, &CHOSUNG)),
        F::Decimal | F::DecimalHalfWidth => whole.map(|n| positional(n, &DECIMAL)),
        F::DecimalEnclosedCircle | F::DecimalEnclosedCircleChinese => {
            positive.and_then(|n| within(n, &DECIMAL_ENCLOSED_CIRCLE))
        }
        F::DecimalEnclosedFullstop => positive.and_then(|n| within(n, &DECIMAL_ENCLOSED_FULLSTOP)),
        F::DecimalEnclosedParen => positive.and_then(|n| within(n, &DECIMAL_ENCLOSED_PAREN)),
        F::DecimalFullWidth => whole.map(|n| positional(n, &DECIMAL_FULL_WIDTH)),
        F::DecimalZero => whole.map(decimal_zero),
        F::Ganada => positive.and_then(|n| repeated(n, &GANADA)),
        F::Hebrew1 => positive.and_then(hebrew_letters),
        F::Hebrew2 => positive.and_then(hebrew_alphabet),
        F::Hex => whole.map(|n| positional(n, &HEX)),
        F::HindiConsonants => positive.and_then(|n| repeated(n, &HINDI_CONSONANTS)),
        F::HindiCounting => positive.and_then(|n| within(n, &HINDI_COUNTING)),
        F::HindiNumbers => whole.map(|n| positional(n, &HINDI_NUMBERS)),
        F::HindiVowels => positive.and_then(|n| repeated(n, &HINDI_VOWELS)),
        F::IdeographDigital | F::JapaneseDigitalTenThousand => {
            whole.map(|n| positional(n, &IDEOGRAPH_DIGITAL))
        }
        F::IdeographEnclosedCircle => positive.and_then(|n| within(n, &IDEOGRAPH_ENCLOSED_CIRCLE)),
        F::IdeographLegalTraditional => {
            positive.and_then(|n| grouped(n, &IDEOGRAPH_LEGAL_TRADITIONAL))
        }
        F::IdeographTraditional => positive.and_then(|n| within(n, &IDEOGRAPH_TRADITIONAL)),
        F::IdeographZodiac => positive.and_then(|n| within(n, &IDEOGRAPH_ZODIAC)),
        F::IdeographZodiacTraditional => {
            positive.and_then(|n| cycled(n, &IDEOGRAPH_ZODIAC_TRADITIONAL))
        }
        F::Iroha => positive.and_then(|n| cycled(n, &IROHA)),
        F::IrohaFullWidth => positive.and_then(|n| cycled(n, &IROHA_FULL_WIDTH)),
        F::JapaneseCounting => positive.and_then(|n| grouped(n, &JAPANESE_COUNTING)),
        F::JapaneseLegal => positive.and_then(|n| grouped(n, &JAPANESE_LEGAL)),
        F::KoreanCounting => positive.and_then(|n| grouped(n, &KOREAN_COUNTING)),
        F::KoreanDigital => whole.map(|n| positional(n, &KOREAN_DIGITAL)),
        F::KoreanDigital2 => whole.map(|n| positional(n, &KOREAN_DIGITAL2)),
        F::KoreanLegal => positive.and_then(korean_legal),
        F::LowerLetter => positive.and_then(|n| repeated(n, &LOWER_LETTER)),
        F::LowerRoman => positive.and_then(|n| roman(n, &LOWER_ROMAN)),
        F::None => Some(String::new()),
        F::NumberInDash => positive.map(|n| format!("- {n} -")),
        F::RussianLower => positive.and_then(|n| repeated(n, &RUSSIAN_LOWER)),
        F::RussianUpper => positive.and_then(|n| repeated(n, &RUSSIAN_UPPER)),
        F::TaiwaneseCountingThousand => {
            positive.and_then(|n| grouped(n, &TAIWANESE_COUNTING_THOUSAND))
        }
        F::TaiwaneseDigital => whole.map(|n| positional(n, &TAIWANESE_DIGITAL)),
        F::ThaiCounting => positive.and_then(|n| within(n, &THAI_COUNTING)),
        F::ThaiLetters => positive.and_then(|n| repeated(n, &THAI_LETTERS)),
        F::ThaiNumbers => whole.map(|n| positional(n, &THAI_NUMBERS)),
        F::UpperLetter => positive.and_then(|n| repeated(n, &UPPER_LETTER)),
        F::UpperRoman => positive.and_then(|n| roman(n, &UPPER_ROMAN)),
        F::VietnameseCounting => positive.and_then(|n| within(n, &VIETNAMESE_COUNTING)),
        F::Bullet
        | F::CardinalText
        | F::Custom
        | F::DecimalFullWidth2
        | F::DollarText
        | F::Ordinal
        | F::OrdinalText
        | _ => return None,
    };
    Some(text.unwrap_or_else(|| value.to_string()))
}

/// `value` in `format` for a run in `language`, a tag such as `en-US` (section 17.18.59).
///
/// Words and letters as Unicode CLDR gives them; `None` in words for a language it lacks.
#[must_use = "this only returns the value"]
pub fn format_number_in(
    value: i64,
    format: StNumberFormatMember,
    language: &str,
) -> Option<String> {
    use StNumberFormatMember as F;

    if format == F::DollarText {
        return rule_based::dollars(value, language);
    }
    if matches!(format, F::LowerLetter | F::UpperLetter) {
        let Some(letters) = rule_based::alphabet(language, format == F::UpperLetter) else {
            return format_number(value, format);
        };
        let positive = u64::try_from(value).ok().filter(|number| *number > 0);
        return Some(
            positive
                .and_then(|number| repeated(number, letters))
                .unwrap_or_else(|| value.to_string()),
        );
    }
    let kind = if format == F::CardinalText {
        Spelled::Cardinal
    } else if format == F::OrdinalText {
        Spelled::OrdinalText
    } else if format == F::Ordinal {
        Spelled::Ordinal
    } else {
        return format_number(value, format);
    };
    let text = rule_based::spelled(value, kind, language)?;
    Some(if format == F::Ordinal {
        text
    } else {
        capitalized(&text)
    })
}

/// `value` in the format token `token` of the XSLT `format` attribute, which `w:numFmt w:format`
/// uses (ECMA-376 Part 1, section 17.9.17; XSLT 1.0, section 7.7.1).
///
/// `None` for a token the recommendation leaves to the implementation, and for a value below 1.
#[must_use = "this only returns the value"]
pub fn format_custom(value: i64, token: &str) -> Option<String> {
    let number = u64::try_from(value).ok().filter(|found| *found > 0)?;
    match token {
        "A" => Some(alphabetic(number, &UPPER_LETTER)),
        "a" => Some(alphabetic(number, &LOWER_LETTER)),
        "I" => roman(number, &UPPER_ROMAN),
        "i" => roman(number, &LOWER_ROMAN),
        _ => decimal(number, token),
    }
}

/// A decimal token: the digit one of a Unicode digit family after any number of that family's
/// zeros; the number in the family's digits, padded with its zero to the token's length.
fn decimal(number: u64, token: &str) -> Option<String> {
    let one = token.chars().next_back()?;
    let zero = char::from_u32(u32::from(one).checked_sub(1)?)?;
    DIGIT_ZEROS.binary_search(&zero).ok()?;
    token
        .strip_suffix(one)?
        .chars()
        .all(|leading| leading == zero)
        .then(|| format!("{number:0>width$}", width = token.chars().count()))?
        .chars()
        .map(|digit| char::from_u32(u32::from(zero) + digit.to_digit(10)?))
        .collect()
}

/// The XSLT alphabetic sequence of `letters`: past the last letter, two letters, the first
/// counting as a digit before the second (`Z`, `AA`, `AB`).
fn alphabetic(value: u64, letters: &[&str]) -> String {
    let size = letters.len() as u64;
    let mut written = Vec::new();
    let mut rest = value;
    while let Some(index) = rest.checked_sub(1) {
        written.push(symbol(letters, index % size));
        rest = index / size;
    }
    written.into_iter().rev().collect()
}

/// `text` with its first letter in upper case, as the section's examples of `cardinalText` and
/// `ordinalText` write it.
fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// `chineseCounting` and `taiwaneseCounting`: 0 to 10 from the set; to 99 the tens digit, unless
/// it is one, the symbol of ten and the ones digit, unless it is zero; past 99 digit by digit.
fn counting(value: u64) -> String {
    match value {
        0..=10 => symbol(&COUNTING, value).to_owned(),
        11..=99 => {
            let tens = value / 10;
            let ones = value % 10;
            let mut text = String::new();
            if tens > 1 {
                text.push_str(symbol(&COUNTING, tens));
            }
            text.push_str(symbol(&COUNTING, 10));
            if ones > 0 {
                text.push_str(symbol(&COUNTING, ones));
            }
            text
        }
        _ => positional(value, &COUNTING[..10]),
    }
}

/// The symbol of position `value` of `set`, the set starting again past its last position.
fn cycled(value: u64, set: &[&str]) -> Option<String> {
    let index = value.checked_sub(1)?;
    Some(symbol(set, index % set.len() as u64).to_owned())
}

/// `decimalZero`: a zero before the numbers 1 to 9.
fn decimal_zero(value: u64) -> String {
    if (1..10).contains(&value) {
        format!("0{value}")
    } else {
        positional(value, &DECIMAL)
    }
}

/// The digit `value` of `digits`, the digits 1 to 9; empty for 0.
fn digit(digits: &[&'static str; 9], value: u64) -> &'static str {
    value
        .checked_sub(1)
        .map_or("", |index| symbol(digits, index))
}

/// `value` below one hundred thousand in a counting system of groups; `None` past it, and for a
/// power the system names no symbol for.
fn grouped(value: u64, system: &Grouped) -> Option<String> {
    if value >= 100_000 {
        return None;
    }
    let mut text = String::new();
    let mut rest = value;
    for (step, unit) in [10_000_u64, 1_000, 100, 10].into_iter().enumerate() {
        let count = rest / unit;
        rest %= unit;
        if count == 0 {
            let missing = step
                .checked_sub(1)
                .and_then(|at| system.zero.get(at))
                .copied()
                .flatten()
                .is_some_and(|divisor| {
                    (unit * 10..unit * 100).contains(&value) && !value.is_multiple_of(divisor)
                });
            if missing {
                text.push_str(ZERO_GROUP);
            }
            continue;
        }
        let power = system.powers.get(step).copied().flatten()?;
        if !(system.one_ten && unit == 10 && count == 1) {
            text.push_str(digit(&system.digits, count));
        }
        text.push_str(power);
    }
    text.push_str(digit(&system.digits, rest));
    Some(text)
}

/// `hebrew2`: the symbol of the value once the size of the set is subtracted, with the tav before
/// it in the text, to its right on the line, once for each subtraction.
fn hebrew_alphabet(value: u64) -> Option<String> {
    let index = value.checked_sub(1)?;
    let size = HEBREW2.len() as u64;
    let times = usize::try_from(index / size)
        .ok()
        .filter(|count| *count <= LONGEST_RUN)?;
    Some(format!(
        "{}{}",
        HEBREW2_TAV.repeat(times),
        symbol(&HEBREW2, index % size)
    ))
}

/// `hebrew1`: the thousands, hundreds, tens and ones digits, written from right to left; 15 and
/// 16 are written with their own symbols.
fn hebrew_letters(value: u64) -> Option<String> {
    if value >= 10_000 {
        return None;
    }
    let mut text = String::new();
    text.push_str(digit(&HEBREW_ONES, value / 1_000));
    text.push_str(digit(&HEBREW_HUNDREDS, value / 100 % 10));
    match value % 100 {
        15 => text.push_str(HEBREW_FIFTEEN),
        16 => text.push_str(HEBREW_SIXTEEN),
        rest => {
            text.push_str(digit(&HEBREW_TENS, rest / 10));
            text.push_str(digit(&HEBREW_ONES, rest % 10));
        }
    }
    Some(text)
}

/// `koreanLegal`: the word of the tens digit and the word of the ones digit, below one hundred.
fn korean_legal(value: u64) -> Option<String> {
    (value < 100).then(|| {
        format!(
            "{}{}",
            digit(&KOREAN_LEGAL_TENS, value / 10),
            digit(&KOREAN_LEGAL_ONES, value % 10)
        )
    })
}

/// `value` written digit by digit in the base the number of `digits` gives, `digits` being the
/// symbols of 0 upward.
fn positional(value: u64, digits: &[&str]) -> String {
    let base = digits.len() as u64;
    let mut written = Vec::new();
    let mut rest = value;
    loop {
        written.push(symbol(digits, rest % base));
        rest /= base;
        if rest == 0 {
            break;
        }
    }
    written.into_iter().rev().collect()
}

/// The symbol of the value once the size of `set` is subtracted from it as often as it can be,
/// written once and once more for each subtraction.
fn repeated(value: u64, set: &[&str]) -> Option<String> {
    let index = value.checked_sub(1)?;
    let size = set.len() as u64;
    let times = usize::try_from(index / size + 1)
        .ok()
        .filter(|count| *count <= LONGEST_RUN)?;
    Some(symbol(set, index % size).repeat(times))
}

/// `value` in roman numerals: as many of each group as it holds, largest first.
fn roman(value: u64, groups: &[(u64, &str); 13]) -> Option<String> {
    let mut text = String::new();
    let mut rest = value;
    for (size, numeral) in groups {
        let times = usize::try_from(rest / size)
            .ok()
            .filter(|count| *count <= LONGEST_RUN)?;
        rest %= size;
        text.push_str(&numeral.repeat(times));
    }
    Some(text)
}

/// The symbol at `index` of `set`; empty past its end.
fn symbol<'set>(set: &[&'set str], index: u64) -> &'set str {
    usize::try_from(index)
        .ok()
        .and_then(|at| set.get(at))
        .copied()
        .unwrap_or_default()
}

/// The symbol of position `value` of `set`; `None` past its last position.
fn within(value: u64, set: &[&str]) -> Option<String> {
    let index = usize::try_from(value.checked_sub(1)?).ok()?;
    set.get(index).map(|found| (*found).to_owned())
}
