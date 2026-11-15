use officekit_markup::wordprocessing::StNumberFormatMember as F;

use super::{format_custom, format_number};

/// Every member of `ST_NumberFormat`.
const MEMBERS: [F; 63] = [
    F::Aiueo,
    F::AiueoFullWidth,
    F::ArabicAbjad,
    F::ArabicAlpha,
    F::BahtText,
    F::Bullet,
    F::CardinalText,
    F::Chicago,
    F::ChineseCounting,
    F::ChineseCountingThousand,
    F::ChineseLegalSimplified,
    F::Chosung,
    F::Custom,
    F::Decimal,
    F::DecimalEnclosedCircle,
    F::DecimalEnclosedCircleChinese,
    F::DecimalEnclosedFullstop,
    F::DecimalEnclosedParen,
    F::DecimalFullWidth,
    F::DecimalFullWidth2,
    F::DecimalHalfWidth,
    F::DecimalZero,
    F::DollarText,
    F::Ganada,
    F::Hebrew1,
    F::Hebrew2,
    F::Hex,
    F::HindiConsonants,
    F::HindiCounting,
    F::HindiNumbers,
    F::HindiVowels,
    F::IdeographDigital,
    F::IdeographEnclosedCircle,
    F::IdeographLegalTraditional,
    F::IdeographTraditional,
    F::IdeographZodiac,
    F::IdeographZodiacTraditional,
    F::Iroha,
    F::IrohaFullWidth,
    F::JapaneseCounting,
    F::JapaneseDigitalTenThousand,
    F::JapaneseLegal,
    F::KoreanCounting,
    F::KoreanDigital,
    F::KoreanDigital2,
    F::KoreanLegal,
    F::LowerLetter,
    F::LowerRoman,
    F::None,
    F::NumberInDash,
    F::Ordinal,
    F::OrdinalText,
    F::RussianLower,
    F::RussianUpper,
    F::TaiwaneseCounting,
    F::TaiwaneseCountingThousand,
    F::TaiwaneseDigital,
    F::ThaiCounting,
    F::ThaiLetters,
    F::ThaiNumbers,
    F::UpperLetter,
    F::UpperRoman,
    F::VietnameseCounting,
];

/// The members that give no text for any value.
const WITHOUT_TEXT: [F; 7] = [
    F::Bullet,
    F::CardinalText,
    F::Custom,
    F::DecimalFullWidth2,
    F::DollarText,
    F::Ordinal,
    F::OrdinalText,
];

/// For every other member, the values of the example of section 17.18.59 with their texts, and
/// the values of the examples its rules give.
const EXAMPLES: [(F, &[(i64, &str)]); 56] = [
    (
        F::Aiueo,
        &[
            (1, "\u{FF71}"),
            (2, "\u{FF72}"),
            (3, "\u{FF73}"),
            (45, "\u{FF66}"),
            (46, "\u{FF9D}"),
            (47, "\u{FF71}\u{FF71}"),
            (48, "\u{FF72}\u{FF72}"),
            (49, "\u{FF73}\u{FF73}"),
        ],
    ),
    (
        F::AiueoFullWidth,
        &[
            (1, "\u{30A2}"),
            (2, "\u{30A4}"),
            (3, "\u{30A6}"),
            (47, "\u{30F2}"),
            (48, "\u{30F3}"),
            (49, "\u{30A2}\u{30A2}"),
            (50, "\u{30A4}\u{30A4}"),
            (51, "\u{30A6}\u{30A6}"),
        ],
    ),
    (
        F::ArabicAbjad,
        &[
            (1, "\u{623}"),
            (2, "\u{628}"),
            (3, "\u{62C}"),
            (27, "\u{63A}"),
            (28, "\u{638}"),
            (29, "\u{623}\u{623}"),
            (30, "\u{628}\u{628}"),
            (31, "\u{62C}\u{62C}"),
            (55, "\u{63A}\u{63A}"),
            (56, "\u{638}\u{638}"),
            (57, "\u{623}\u{623}\u{623}"),
            (58, "\u{628}\u{628}\u{628}"),
            (59, "\u{62C}\u{62C}\u{62C}"),
        ],
    ),
    (
        F::ArabicAlpha,
        &[
            (1, "\u{623}"),
            (2, "\u{628}"),
            (3, "\u{62A}"),
            (27, "\u{648}"),
            (28, "\u{64A}"),
            (29, "\u{623}\u{623}"),
            (30, "\u{628}\u{628}"),
            (31, "\u{62A}\u{62A}"),
            (55, "\u{648}\u{648}"),
            (56, "\u{64A}\u{64A}"),
            (57, "\u{623}\u{623}\u{623}"),
            (58, "\u{628}\u{628}\u{628}"),
            (59, "\u{62A}\u{62A}\u{62A}"),
        ],
    ),
    (
        F::BahtText,
        &[
            (
                1,
                "\u{E2B}\u{E19}\u{E36}\u{E48}\u{E07}\u{E1A}\u{E32}\u{E17}\u{E16}\u{E49}\u{E27}\u{E19}",
            ),
            (
                2,
                "\u{E2A}\u{E2D}\u{E07}\u{E1A}\u{E32}\u{E17}\u{E16}\u{E49}\u{E27}\u{E19}",
            ),
            (
                3,
                "\u{E2A}\u{E32}\u{E21}\u{E1A}\u{E32}\u{E17}\u{E16}\u{E49}\u{E27}\u{E19}",
            ),
        ],
    ),
    (
        F::Chicago,
        &[
            (1, "*"),
            (2, "\u{2020}"),
            (3, "\u{2021}"),
            (4, "\u{A7}"),
            (5, "**"),
            (6, "\u{2020}\u{2020}"),
            (7, "\u{2021}\u{2021}"),
            (8, "\u{A7}\u{A7}"),
            (9, "***"),
        ],
    ),
    (
        F::ChineseCounting,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (9, "\u{4E5D}"),
            (10, "\u{5341}"),
            (11, "\u{5341}\u{4E00}"),
            (12, "\u{5341}\u{4E8C}"),
            (19, "\u{5341}\u{4E5D}"),
            (20, "\u{4E8C}\u{5341}"),
            (21, "\u{4E8C}\u{5341}\u{4E00}"),
            (99, "\u{4E5D}\u{5341}\u{4E5D}"),
            (100, "\u{4E00}\u{25CB}\u{25CB}"),
            (101, "\u{4E00}\u{25CB}\u{4E00}"),
        ],
    ),
    (
        F::ChineseCountingThousand,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{4E00}\u{5341}"),
            (11, "\u{4E00}\u{5341}\u{4E00}"),
            (12, "\u{4E00}\u{5341}\u{4E8C}"),
            (19, "\u{4E00}\u{5341}\u{4E5D}"),
            (20, "\u{4E8C}\u{5341}"),
            (21, "\u{4E8C}\u{5341}\u{4E00}"),
            (99, "\u{4E5D}\u{5341}\u{4E5D}"),
            (100, "\u{4E00}\u{767E}"),
            (101, "\u{4E00}\u{767E}\u{96F6}\u{4E00}"),
            (5000, "\u{4E94}\u{5343}"),
            (500, "\u{4E94}\u{767E}"),
            (50, "\u{4E94}\u{5341}"),
            (10_005, "\u{4E00}\u{4E07}\u{96F6}\u{4E94}"),
            (1005, "\u{4E00}\u{5343}\u{96F6}\u{4E94}"),
            (105, "\u{4E00}\u{767E}\u{96F6}\u{4E94}"),
        ],
    ),
    (
        F::ChineseLegalSimplified,
        &[
            (1, "\u{58F9}"),
            (2, "\u{8D30}"),
            (3, "\u{53C1}"),
            (8, "\u{634C}"),
            (9, "\u{7396}"),
            (10, "\u{58F9}\u{62FE}"),
            (11, "\u{58F9}\u{62FE}\u{58F9}"),
            (12, "\u{58F9}\u{62FE}\u{8D30}"),
            (19, "\u{58F9}\u{62FE}\u{7396}"),
            (20, "\u{8D30}\u{62FE}"),
            (21, "\u{8D30}\u{62FE}\u{58F9}"),
            (99, "\u{7396}\u{62FE}\u{7396}"),
            (100, "\u{58F9}\u{4F70}"),
            (101, "\u{58F9}\u{4F70}\u{96F6}\u{58F9}"),
            (5000, "\u{4F0D}\u{4EDF}"),
            (10_005, "\u{58F9}\u{4E07}\u{96F6}\u{4F0D}"),
            (1005, "\u{58F9}\u{4EDF}\u{96F6}\u{4F0D}"),
            (500, "\u{4F0D}\u{4F70}"),
            (50, "\u{4F0D}\u{62FE}"),
            (105, "\u{58F9}\u{4F70}\u{96F6}\u{4F0D}"),
        ],
    ),
    (
        F::Chosung,
        &[
            (1, "\u{3131}"),
            (2, "\u{3134}"),
            (3, "\u{3137}"),
            (13, "\u{314D}"),
            (14, "\u{314E}"),
            (15, "\u{3131}\u{3131}"),
            (16, "\u{3134}\u{3134}"),
            (17, "\u{3137}\u{3137}"),
        ],
    ),
    (
        F::Decimal,
        &[
            (1, "1"),
            (2, "2"),
            (3, "3"),
            (8, "8"),
            (9, "9"),
            (10, "10"),
            (11, "11"),
            (12, "12"),
            (18, "18"),
            (19, "19"),
            (20, "20"),
            (21, "21"),
        ],
    ),
    (
        F::DecimalEnclosedCircle,
        &[
            (1, "\u{2460}"),
            (2, "\u{2461}"),
            (3, "\u{2462}"),
            (19, "\u{2472}"),
            (20, "\u{2473}"),
            (21, "21"),
        ],
    ),
    (
        F::DecimalEnclosedCircleChinese,
        &[
            (1, "\u{2460}"),
            (2, "\u{2461}"),
            (3, "\u{2462}"),
            (19, "\u{2472}"),
            (20, "\u{2473}"),
            (21, "21"),
        ],
    ),
    (
        F::DecimalEnclosedFullstop,
        &[
            (1, "\u{2488}"),
            (2, "\u{2489}"),
            (3, "\u{248A}"),
            (19, "\u{249A}"),
            (20, "\u{249B}"),
            (21, "21"),
        ],
    ),
    (
        F::DecimalEnclosedParen,
        &[
            (1, "\u{2474}"),
            (2, "\u{2475}"),
            (3, "\u{2476}"),
            (19, "\u{2486}"),
            (20, "\u{2487}"),
            (21, "21"),
            (22, "22"),
        ],
    ),
    (
        F::DecimalFullWidth,
        &[
            (1, "\u{FF11}"),
            (2, "\u{FF12}"),
            (3, "\u{FF13}"),
            (8, "\u{FF18}"),
            (9, "\u{FF19}"),
            (10, "\u{FF11}\u{FF10}"),
            (11, "\u{FF11}\u{FF11}"),
            (12, "\u{FF11}\u{FF12}"),
            (18, "\u{FF11}\u{FF18}"),
            (19, "\u{FF11}\u{FF19}"),
            (20, "\u{FF12}\u{FF10}"),
            (21, "\u{FF12}\u{FF11}"),
        ],
    ),
    (
        F::DecimalHalfWidth,
        &[
            (1, "1"),
            (2, "2"),
            (3, "3"),
            (8, "8"),
            (9, "9"),
            (10, "10"),
            (11, "11"),
            (12, "12"),
            (18, "18"),
            (19, "19"),
            (20, "20"),
            (21, "21"),
        ],
    ),
    (
        F::DecimalZero,
        &[
            (1, "01"),
            (2, "02"),
            (3, "03"),
            (8, "08"),
            (9, "09"),
            (10, "10"),
            (11, "11"),
            (12, "12"),
            (18, "18"),
            (19, "19"),
            (20, "20"),
            (21, "21"),
            (22, "22"),
            (98, "98"),
            (99, "99"),
            (100, "100"),
            (101, "101"),
        ],
    ),
    (
        F::Ganada,
        &[
            (1, "\u{AC00}"),
            (2, "\u{B098}"),
            (3, "\u{B2E4}"),
            (13, "\u{D30C}"),
            (14, "\u{D558}"),
            (15, "\u{AC00}\u{AC00}"),
            (16, "\u{B098}\u{B098}"),
            (17, "\u{B2E4}\u{B2E4}"),
        ],
    ),
    (
        F::Hebrew1,
        &[
            (1, "\u{5D0}"),
            (2, "\u{5D1}"),
            (3, "\u{5D2}"),
            (15, "\u{5D8}\u{5D5}"),
            (16, "\u{5D8}\u{5D6}"),
            (17, "\u{5D9}\u{5D6}"),
            (18, "\u{5D9}\u{5D7}"),
            (19, "\u{5D9}\u{5D8}"),
            (20, "\u{5DB}"),
            (21, "\u{5DB}\u{5D0}"),
            (115, "\u{5E7}\u{5D8}\u{5D5}"),
            (1234, "\u{5D0}\u{5E8}\u{5DC}\u{5D3}"),
        ],
    ),
    (
        F::Hebrew2,
        &[
            (1, "\u{5D0}"),
            (2, "\u{5D1}"),
            (3, "\u{5D2}"),
            (22, "\u{5EA}"),
            (23, "\u{5EA}\u{5D0}"),
            (24, "\u{5EA}\u{5D1}"),
            (45, "\u{5EA}\u{5EA}\u{5D0}"),
        ],
    ),
    (
        F::Hex,
        &[
            (1, "1"),
            (2, "2"),
            (3, "3"),
            (14, "E"),
            (15, "F"),
            (16, "10"),
            (17, "11"),
            (18, "12"),
            (30, "1E"),
            (31, "1F"),
            (32, "20"),
            (33, "21"),
        ],
    ),
    (
        F::HindiConsonants,
        &[
            (1, "\u{905}"),
            (2, "\u{906}"),
            (3, "\u{907}"),
            (17, "\u{905}\u{902}"),
            (18, "\u{905}\u{903}"),
            (19, "\u{905}\u{905}"),
            (20, "\u{906}\u{906}"),
            (21, "\u{907}\u{907}"),
            (35, "\u{905}\u{902}\u{905}\u{902}"),
            (36, "\u{905}\u{903}\u{905}\u{903}"),
            (37, "\u{905}\u{905}\u{905}"),
            (38, "\u{906}\u{906}\u{906}"),
            (39, "\u{907}\u{907}\u{907}"),
        ],
    ),
    (
        F::HindiCounting,
        &[
            (1, "\u{90F}\u{915}"),
            (2, "\u{926}\u{94B}"),
            (3, "\u{924}\u{940}\u{928}"),
            (4, "\u{91A}\u{93E}\u{930}"),
            (5, "\u{92A}\u{93E}\u{901}\u{91A}"),
            (6, "\u{91B}\u{903}"),
            (7, "\u{938}\u{93E}\u{924}"),
            (8, "\u{906}\u{920}"),
            (9, "\u{928}\u{94C}"),
            (10, "\u{926}\u{938}"),
            (11, "11"),
        ],
    ),
    (
        F::HindiNumbers,
        &[
            (1, "\u{967}"),
            (2, "\u{968}"),
            (3, "\u{969}"),
            (8, "\u{96E}"),
            (9, "\u{96F}"),
            (10, "\u{967}\u{966}"),
            (11, "\u{967}\u{967}"),
            (12, "\u{967}\u{968}"),
            (18, "\u{967}\u{96E}"),
            (19, "\u{967}\u{96F}"),
            (20, "\u{968}\u{966}"),
            (21, "\u{968}\u{967}"),
        ],
    ),
    (
        F::HindiVowels,
        &[
            (1, "\u{915}"),
            (2, "\u{916}"),
            (3, "\u{917}"),
            (36, "\u{938}"),
            (37, "\u{939}"),
            (38, "\u{915}\u{915}"),
            (39, "\u{916}\u{916}"),
            (40, "\u{917}\u{917}"),
            (73, "\u{938}\u{938}"),
            (74, "\u{939}\u{939}"),
            (75, "\u{915}\u{915}\u{915}"),
            (76, "\u{916}\u{916}\u{916}"),
            (77, "\u{917}\u{917}\u{917}"),
        ],
    ),
    (
        F::IdeographDigital,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{4E00}\u{3007}"),
            (11, "\u{4E00}\u{4E00}"),
            (12, "\u{4E00}\u{4E8C}"),
            (18, "\u{4E00}\u{516B}"),
            (19, "\u{4E00}\u{4E5D}"),
            (20, "\u{4E8C}\u{3007}"),
            (21, "\u{4E8C}\u{4E00}"),
        ],
    ),
    (
        F::IdeographEnclosedCircle,
        &[
            (1, "\u{3220}"),
            (2, "\u{3221}"),
            (3, "\u{3222}"),
            (9, "\u{3228}"),
            (10, "\u{3229}"),
            (11, "11"),
            (12, "12"),
        ],
    ),
    (
        F::IdeographLegalTraditional,
        &[
            (1, "\u{58F9}"),
            (2, "\u{8CB3}"),
            (3, "\u{53C3}"),
            (8, "\u{634C}"),
            (9, "\u{7396}"),
            (10, "\u{58F9}\u{62FE}"),
            (11, "\u{58F9}\u{62FE}\u{58F9}"),
            (12, "\u{58F9}\u{62FE}\u{8CB3}"),
            (19, "\u{58F9}\u{62FE}\u{7396}"),
            (20, "\u{8CB3}\u{62FE}"),
            (21, "\u{8CB3}\u{62FE}\u{58F9}"),
            (500, "\u{4F0D}\u{4F70}"),
            (50, "\u{4F0D}\u{62FE}"),
        ],
    ),
    (
        F::IdeographTraditional,
        &[
            (1, "\u{7532}"),
            (2, "\u{4E59}"),
            (3, "\u{4E19}"),
            (4, "\u{4E01}"),
            (9, "\u{58EC}"),
            (10, "\u{7678}"),
            (11, "11"),
            (12, "12"),
        ],
    ),
    (
        F::IdeographZodiac,
        &[
            (1, "\u{5B50}"),
            (2, "\u{4E11}"),
            (3, "\u{5BC5}"),
            (11, "\u{620C}"),
            (12, "\u{4EA5}"),
            (13, "13"),
            (14, "14"),
        ],
    ),
    (
        F::IdeographZodiacTraditional,
        &[
            (1, "\u{7532}\u{5B50}"),
            (2, "\u{4E59}\u{4E11}"),
            (3, "\u{4E19}\u{5BC5}"),
            (59, "\u{58EC}\u{620C}"),
            (60, "\u{7678}\u{4EA5}"),
            (61, "\u{7532}\u{5B50}"),
            (62, "\u{4E59}\u{4E11}"),
            (63, "\u{4E19}\u{5BC5}"),
        ],
    ),
    (
        F::Iroha,
        &[
            (1, "\u{FF72}"),
            (2, "\u{FF9B}"),
            (3, "\u{FF8A}"),
            (47, "\u{FF7D}"),
            (48, "\u{FF9D}"),
            (49, "\u{FF72}"),
            (50, "\u{FF9B}"),
            (51, "\u{FF8A}"),
        ],
    ),
    (
        F::IrohaFullWidth,
        &[
            (1, "\u{30A4}"),
            (2, "\u{30ED}"),
            (3, "\u{30CF}"),
            (47, "\u{30B9}"),
            (48, "\u{30F3}"),
            (49, "\u{30A4}"),
            (50, "\u{30ED}"),
            (51, "\u{30CF}"),
        ],
    ),
    (
        F::JapaneseCounting,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{5341}"),
            (11, "\u{5341}\u{4E00}"),
            (12, "\u{5341}\u{4E8C}"),
            (18, "\u{5341}\u{516B}"),
            (19, "\u{5341}\u{4E5D}"),
            (20, "\u{4E8C}\u{5341}"),
            (21, "\u{4E8C}\u{5341}\u{4E00}"),
            (500, "\u{4E94}\u{767E}"),
            (50, "\u{4E94}\u{5341}"),
            (10_000, "10000"),
        ],
    ),
    (
        F::JapaneseDigitalTenThousand,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{4E00}\u{3007}"),
            (11, "\u{4E00}\u{4E00}"),
            (12, "\u{4E00}\u{4E8C}"),
            (18, "\u{4E00}\u{516B}"),
            (19, "\u{4E00}\u{4E5D}"),
            (20, "\u{4E8C}\u{3007}"),
            (21, "\u{4E8C}\u{4E00}"),
        ],
    ),
    (
        F::JapaneseLegal,
        &[
            (1, "\u{58F1}"),
            (2, "\u{5F10}"),
            (3, "\u{53C2}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{58F1}\u{62FE}"),
            (11, "\u{58F1}\u{62FE}\u{58F1}"),
            (12, "\u{58F1}\u{62FE}\u{5F10}"),
            (18, "\u{58F1}\u{62FE}\u{516B}"),
            (19, "\u{58F1}\u{62FE}\u{4E5D}"),
            (20, "\u{5F10}\u{62FE}"),
            (21, "\u{5F10}\u{62FE}\u{58F1}"),
            (500, "\u{4F0D}\u{767E}"),
            (50, "\u{4F0D}\u{62FE}"),
        ],
    ),
    (
        F::KoreanCounting,
        &[
            (1, "\u{C77C}"),
            (2, "\u{C774}"),
            (3, "\u{C0BC}"),
            (8, "\u{D314}"),
            (9, "\u{AD6C}"),
            (10, "\u{C2ED}"),
            (11, "\u{C2ED}\u{C77C}"),
            (500, "\u{C624}\u{BC31}"),
            (50, "\u{C624}\u{C2ED}"),
        ],
    ),
    (
        F::KoreanDigital,
        &[
            (1, "\u{C77C}"),
            (2, "\u{C774}"),
            (3, "\u{C0BC}"),
            (8, "\u{D314}"),
            (9, "\u{AD6C}"),
            (10, "\u{C77C}\u{C601}"),
            (11, "\u{C77C}\u{C77C}"),
        ],
    ),
    (
        F::KoreanDigital2,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{4E00}\u{96F6}"),
            (11, "\u{4E00}\u{4E00}"),
        ],
    ),
    (
        F::KoreanLegal,
        &[
            (1, "\u{D558}\u{B098}"),
            (2, "\u{B458}"),
            (3, "\u{C14B}"),
            (9, "\u{C544}\u{D649}"),
            (10, "\u{C5F4}"),
            (11, "\u{C5F4}\u{D558}\u{B098}"),
            (12, "\u{C5F4}\u{B458}"),
            (20, "\u{C2A4}\u{BB3C}"),
            (21, "\u{C2A4}\u{BB3C}\u{D558}\u{B098}"),
        ],
    ),
    (
        F::LowerLetter,
        &[
            (1, "a"),
            (2, "b"),
            (3, "c"),
            (25, "y"),
            (26, "z"),
            (27, "aa"),
            (28, "bb"),
            (29, "cc"),
            (51, "yy"),
            (52, "zz"),
            (53, "aaa"),
            (54, "bbb"),
            (55, "ccc"),
        ],
    ),
    (
        F::LowerRoman,
        &[
            (1, "i"),
            (2, "ii"),
            (3, "iii"),
            (4, "iv"),
            (18, "xviii"),
            (19, "xix"),
            (20, "xx"),
            (21, "xxi"),
            (450, "cdl"),
            (45, "xlv"),
        ],
    ),
    (F::None, &[(1, ""), (2, ""), (0, ""), (-1, "")]),
    (
        F::NumberInDash,
        &[
            (1, "- 1 -"),
            (2, "- 2 -"),
            (3, "- 3 -"),
            (8, "- 8 -"),
            (9, "- 9 -"),
            (10, "- 10 -"),
            (11, "- 11 -"),
            (12, "- 12 -"),
            (18, "- 18 -"),
            (19, "- 19 -"),
            (20, "- 20 -"),
            (21, "- 21 -"),
        ],
    ),
    (
        F::RussianLower,
        &[
            (1, "\u{430}"),
            (2, "\u{431}"),
            (3, "\u{432}"),
            (28, "\u{44E}"),
            (29, "\u{44F}"),
            (30, "\u{430}\u{430}"),
            (31, "\u{431}\u{431}"),
            (32, "\u{432}\u{432}"),
            (57, "\u{44E}\u{44E}"),
            (58, "\u{44F}\u{44F}"),
            (59, "\u{430}\u{430}\u{430}"),
            (60, "\u{431}\u{431}\u{431}"),
            (61, "\u{432}\u{432}\u{432}"),
        ],
    ),
    (
        F::RussianUpper,
        &[
            (1, "\u{410}"),
            (2, "\u{411}"),
            (3, "\u{412}"),
            (28, "\u{42E}"),
            (29, "\u{42F}"),
            (30, "\u{410}\u{410}"),
            (31, "\u{411}\u{411}"),
            (32, "\u{412}\u{412}"),
            (57, "\u{42E}\u{42E}"),
            (58, "\u{42F}\u{42F}"),
            (59, "\u{410}\u{410}\u{410}"),
            (60, "\u{411}\u{411}\u{411}"),
            (61, "\u{412}\u{412}\u{412}"),
        ],
    ),
    (
        F::TaiwaneseCounting,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (9, "\u{4E5D}"),
            (10, "\u{5341}"),
            (11, "\u{5341}\u{4E00}"),
            (12, "\u{5341}\u{4E8C}"),
            (19, "\u{5341}\u{4E5D}"),
            (20, "\u{4E8C}\u{5341}"),
            (21, "\u{4E8C}\u{5341}\u{4E00}"),
            (99, "\u{4E5D}\u{5341}\u{4E5D}"),
            (100, "\u{4E00}\u{25CB}\u{25CB}"),
            (101, "\u{4E00}\u{25CB}\u{4E00}"),
        ],
    ),
    (
        F::TaiwaneseCountingThousand,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (3, "\u{4E09}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{5341}"),
            (11, "\u{5341}\u{4E00}"),
            (12, "\u{5341}\u{4E8C}"),
            (19, "\u{5341}\u{4E5D}"),
            (20, "\u{4E8C}\u{5341}"),
            (21, "\u{4E8C}\u{5341}\u{4E00}"),
            (99, "\u{4E5D}\u{5341}\u{4E5D}"),
            (100, "\u{4E00}\u{767E}"),
            (101, "\u{4E00}\u{767E}\u{96F6}\u{4E00}"),
            (5000, "\u{4E94}\u{5343}"),
            (10_005, "\u{4E00}\u{842C}\u{96F6}\u{4E94}"),
            (500, "\u{4E94}\u{767E}"),
            (50, "\u{4E94}\u{5341}"),
            (105, "\u{4E00}\u{767E}\u{96F6}\u{4E94}"),
        ],
    ),
    (
        F::TaiwaneseDigital,
        &[
            (1, "\u{4E00}"),
            (2, "\u{4E8C}"),
            (8, "\u{516B}"),
            (9, "\u{4E5D}"),
            (10, "\u{4E00}\u{25CB}"),
            (11, "\u{4E00}\u{4E00}"),
            (12, "\u{4E00}\u{4E8C}"),
            (18, "\u{4E00}\u{516B}"),
            (19, "\u{4E00}\u{4E5D}"),
            (20, "\u{4E8C}\u{25CB}"),
            (21, "\u{4E8C}\u{4E00}"),
        ],
    ),
    (
        F::ThaiCounting,
        &[
            (1, "\u{E2B}\u{E19}\u{E36}\u{E48}\u{E07}"),
            (2, "\u{E2A}\u{E2D}\u{E07}"),
            (3, "\u{E2A}\u{E32}\u{E21}"),
            (4, "\u{E2A}\u{E35}\u{E48}"),
            (5, "\u{E2B}\u{E49}\u{E32}"),
            (6, "\u{E2B}\u{E01}"),
            (7, "\u{E40}\u{E08}\u{E47}\u{E14}"),
            (8, "\u{E41}\u{E1B}\u{E14}"),
            (9, "\u{E40}\u{E01}\u{E49}\u{E32}"),
            (10, "\u{E2A}\u{E34}\u{E1A}"),
            (11, "11"),
        ],
    ),
    (
        F::ThaiLetters,
        &[
            (1, "\u{E01}"),
            (2, "\u{E02}"),
            (3, "\u{E04}"),
            (40, "\u{E2D}"),
            (41, "\u{E2E}"),
            (42, "\u{E01}\u{E01}"),
            (43, "\u{E02}\u{E02}"),
            (44, "\u{E04}\u{E04}"),
            (81, "\u{E2D}\u{E2D}"),
            (82, "\u{E2E}\u{E2E}"),
            (83, "\u{E01}\u{E01}\u{E01}"),
            (84, "\u{E02}\u{E02}\u{E02}"),
            (85, "\u{E04}\u{E04}\u{E04}"),
        ],
    ),
    (
        F::ThaiNumbers,
        &[
            (1, "\u{E51}"),
            (2, "\u{E52}"),
            (3, "\u{E53}"),
            (8, "\u{E58}"),
            (9, "\u{E59}"),
            (10, "\u{E51}\u{E50}"),
            (11, "\u{E51}\u{E51}"),
            (12, "\u{E51}\u{E52}"),
            (18, "\u{E51}\u{E58}"),
            (19, "\u{E51}\u{E59}"),
            (20, "\u{E52}\u{E50}"),
            (21, "\u{E52}\u{E51}"),
            (22, "\u{E52}\u{E52}"),
            (28, "\u{E52}\u{E58}"),
            (29, "\u{E52}\u{E59}"),
        ],
    ),
    (
        F::UpperLetter,
        &[
            (1, "A"),
            (2, "B"),
            (3, "C"),
            (25, "Y"),
            (26, "Z"),
            (27, "AA"),
            (28, "BB"),
            (29, "CC"),
            (51, "YY"),
            (52, "ZZ"),
            (53, "AAA"),
            (54, "BBB"),
            (55, "CCC"),
        ],
    ),
    (
        F::UpperRoman,
        &[
            (1, "I"),
            (2, "II"),
            (3, "III"),
            (4, "IV"),
            (18, "XVIII"),
            (19, "XIX"),
            (20, "XX"),
            (21, "XXI"),
            (450, "CDL"),
            (45, "XLV"),
        ],
    ),
    (
        F::VietnameseCounting,
        &[
            (1, "m\u{1ED9}t"),
            (2, "hai"),
            (3, "ba"),
            (4, "b\u{1ED1}n"),
            (5, "n\u{103}m"),
            (6, "s\u{E1}u"),
            (7, "b\u{1EA3}y"),
            (8, "t\u{E1}m"),
            (9, "ch\u{ED}n"),
            (10, "m\u{1B0}\u{1EDD}i"),
            (11, "11"),
        ],
    ),
];

#[test]
fn each_member_writes_the_values_of_its_example() {
    for (member, cases) in EXAMPLES {
        for (value, text) in cases {
            assert_eq!(
                format_number(*value, member).as_deref(),
                Some(*text),
                "{member:?} of {value}"
            );
        }
    }
}

#[test]
fn every_member_but_those_without_text_has_an_example() {
    for member in MEMBERS {
        let examples = EXAMPLES
            .iter()
            .filter(|(found, _)| *found == member)
            .count();
        let expected = usize::from(!WITHOUT_TEXT.contains(&member));
        assert_eq!(examples, expected, "{member:?}");
    }
}

#[test]
fn every_member_gives_a_text_or_none_for_every_value_to_32767() {
    for member in MEMBERS {
        let without = WITHOUT_TEXT.contains(&member);
        for value in 0..=i64::from(i16::MAX) {
            assert_eq!(
                format_number(value, member).is_none(),
                without,
                "{member:?} of {value}"
            );
        }
    }
}

#[test]
fn the_formats_in_the_run_s_language_and_custom_give_no_text() {
    for member in [
        F::CardinalText,
        F::OrdinalText,
        F::Ordinal,
        F::DollarText,
        F::Custom,
    ] {
        assert_eq!(format_number(7, member), None, "{member:?}");
    }
}

#[test]
fn zero_and_negative_numbers_with_no_text_are_decimal() {
    assert_eq!(format_number(0, F::LowerLetter).as_deref(), Some("0"));
    assert_eq!(format_number(-4, F::UpperRoman).as_deref(), Some("-4"));
    assert_eq!(format_number(-4, F::Decimal).as_deref(), Some("-4"));
    assert_eq!(format_number(0, F::Decimal).as_deref(), Some("0"));
    assert_eq!(format_number(0, F::DecimalZero).as_deref(), Some("0"));
    assert_eq!(format_number(0, F::NumberInDash).as_deref(), Some("0"));
    assert_eq!(format_number(0, F::Hebrew1).as_deref(), Some("0"));
    assert_eq!(
        format_number(0, F::ChineseCounting).as_deref(),
        Some("\u{25CB}")
    );
}

#[test]
fn numbers_past_a_format_s_reach_are_decimal() {
    assert_eq!(
        format_number(100_000, F::ChineseCountingThousand).as_deref(),
        Some("100000")
    );
    assert_eq!(format_number(10_000, F::Hebrew1).as_deref(), Some("10000"));
    assert_eq!(format_number(100, F::KoreanLegal).as_deref(), Some("100"));
    assert_eq!(
        format_number(i64::MAX, F::UpperLetter),
        Some(i64::MAX.to_string())
    );
    assert_eq!(
        format_number(i64::MAX, F::LowerRoman),
        Some(i64::MAX.to_string())
    );
}

#[test]
fn the_sample_values_of_a_page_number() {
    assert_eq!(format_number(4, F::LowerRoman).as_deref(), Some("iv"));
    assert_eq!(format_number(27, F::UpperLetter).as_deref(), Some("AA"));
    assert_eq!(format_number(7, F::Decimal).as_deref(), Some("7"));
    assert_eq!(format_number(7, F::CardinalText), None);
}

#[test]
fn a_token_of_zeros_ending_in_one_is_decimal_of_its_least_width() {
    assert_eq!(format_custom(7, "001").as_deref(), Some("007"));
    assert_eq!(format_custom(123, "001").as_deref(), Some("123"));
    assert_eq!(format_custom(123, "01").as_deref(), Some("123"));
    assert_eq!(format_custom(7, "1").as_deref(), Some("7"));
    assert_eq!(format_custom(123, "1").as_deref(), Some("123"));
}

#[test]
fn the_letter_tokens_count_past_z_with_two_letters() {
    for (value, upper) in [
        (1, "A"),
        (26, "Z"),
        (27, "AA"),
        (28, "AB"),
        (52, "AZ"),
        (53, "BA"),
    ] {
        assert_eq!(format_custom(value, "A").as_deref(), Some(upper));
        let lower = upper.to_lowercase();
        assert_eq!(format_custom(value, "a"), Some(lower));
    }
    assert_eq!(format_custom(702, "A").as_deref(), Some("ZZ"));
    assert_eq!(format_custom(703, "A").as_deref(), Some("AAA"));
}

#[test]
fn the_roman_tokens_write_roman_numerals() {
    for (value, upper) in [(4, "IV"), (9, "IX"), (14, "XIV")] {
        assert_eq!(format_custom(value, "I").as_deref(), Some(upper));
        let lower = upper.to_lowercase();
        assert_eq!(format_custom(value, "i"), Some(lower));
    }
    assert_eq!(format_custom(i64::MAX, "I"), None);
}

#[test]
fn a_token_the_recommendation_leaves_to_the_implementation_gives_none() {
    for token in ["\u{30A2}", "nonsense", "11", "0", "2", "01a", "B", "ii", ""] {
        assert_eq!(format_custom(1, token), None, "{token}");
    }
}

#[test]
fn a_value_below_one_gives_none() {
    for token in ["1", "001", "A", "a", "I", "i"] {
        assert_eq!(format_custom(0, token), None, "{token}");
        assert_eq!(format_custom(-3, token), None, "{token}");
    }
}

#[test]
fn the_sample_values_of_a_custom_format() {
    assert_eq!(format_custom(7, "001").as_deref(), Some("007"));
    assert_eq!(format_custom(123, "01").as_deref(), Some("123"));
    assert_eq!(format_custom(27, "A").as_deref(), Some("AA"));
    assert_eq!(format_custom(4, "i").as_deref(), Some("iv"));
    assert_eq!(format_custom(1, "\u{30A2}"), None);
}

#[test]
fn a_decimal_token_in_another_digit_family_writes_that_familys_digits() {
    assert_eq!(
        format_custom(5, "\u{FF10}\u{FF11}").as_deref(),
        Some("\u{FF10}\u{FF15}")
    );
    assert_eq!(
        format_custom(12, "\u{0660}\u{0660}\u{0661}").as_deref(),
        Some("\u{0660}\u{0661}\u{0662}")
    );
    assert_eq!(
        format_custom(1234, "\u{0661}").as_deref(),
        Some("\u{0661}\u{0662}\u{0663}\u{0664}")
    );
    assert_eq!(
        format_custom(90, "\u{1E950}\u{1E950}\u{1E951}").as_deref(),
        Some("\u{1E950}\u{1E959}\u{1E950}")
    );
}

#[test]
fn a_token_mixing_two_digit_families_gives_none() {
    for token in ["0\u{FF11}", "\u{FF10}1", "\u{0660}\u{FF11}"] {
        assert_eq!(format_custom(5, token), None, "{token}");
    }
}

#[test]
fn a_token_of_another_family_that_is_not_zeros_then_one_gives_none() {
    for token in [
        "\u{FF11}\u{FF11}",
        "\u{FF10}",
        "\u{FF12}",
        "\u{0661}\u{0660}",
    ] {
        assert_eq!(format_custom(5, token), None, "{token}");
    }
}
