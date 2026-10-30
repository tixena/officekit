//! `cargo xtask parity`: every error SDK 3.5.1 reports that officekit lacks.
//!
//! Parity runs one way: an error only officekit reports does not count.
//! The test-only table from officekit's codes to SDK 3.5.1's error ids
//! lives here and nowhere in `officekit::validation`.

use alloc::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result, bail};
use officekit::validation::error::ErrorCode;
use os_shim::System;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// Every error id SDK 3.5.1 reports, read from its validation source, and
/// officekit's code for it.
pub const SDK_KEYS: &[(ErrorCode, &str)] = &[
    (ErrorCode::RepeatedChildInAllGroup, "Sch_AllElement"),
    (ErrorCode::ValueNotInUnion, "Sch_AttributeUnionFailedEx"),
    (
        ErrorCode::InvalidAttributeValue,
        "Sch_AttributeValueDataTypeDetailed",
    ),
    (
        ErrorCode::InvalidElementValue,
        "Sch_ElementValueDataTypeDetailed",
    ),
    (
        ErrorCode::MissingChild,
        "Sch_IncompleteContentExpectingComplex",
    ),
    (
        ErrorCode::ChildInLeafElement,
        "Sch_InvalidChildinLeafElement",
    ),
    (
        ErrorCode::InvalidChild,
        "Sch_InvalidElementContentExpectingComplex",
    ),
    (
        ErrorCode::ChildOfWrongType,
        "Sch_InvalidElementContentWrongType",
    ),
    (
        ErrorCode::MissingRequiredAttribute,
        "Sch_MissRequiredAttribute",
    ),
    (ErrorCode::MissingRootElement, "Sch_MissingPartRootElement"),
    (ErrorCode::UndeclaredAttribute, "Sch_UndeclaredAttribute"),
    (
        ErrorCode::UnexpectedChild,
        "Sch_UnexpectedElementContentExpectingComplex",
    ),
    (
        ErrorCode::AttributeMustBeAbsentUnlessValue,
        "Sem_AttributeAbsentConditionToNonValue",
    ),
    (
        ErrorCode::AttributeMustBeAbsentForValue,
        "Sem_AttributeAbsentConditionToValue",
    ),
    (
        ErrorCode::AttributeCannotBeOmitted,
        "Sem_MissRequiredAttribute",
    ),
    (
        ErrorCode::MutuallyExclusiveAttributes,
        "Sem_AttributeMutualExclusive",
    ),
    (
        ErrorCode::AttributeRequiredForValue,
        "Sem_AttributeRequiredConditionToValue",
    ),
    (
        ErrorCode::AttributeValueConflictsWithAnother,
        "Sem_AttributeValueConditionToAnother",
    ),
    (
        ErrorCode::AttributeValueOutsideConstraint,
        "Sem_AttributeValueDataTypeDetailed",
    ),
    (
        ErrorCode::AttributeValueExceedsAnother,
        "Sem_AttributeValueLessEqualToAnother",
    ),
    (
        ErrorCode::IndexedElementNotFound,
        "Sem_MissingIndexedElement",
    ),
    (
        ErrorCode::ReferencedElementNotFound,
        "Sem_MissingReferenceElement",
    ),
    (ErrorCode::RelationshipNotFound, "Sem_InvalidRelationshipId"),
    (
        ErrorCode::WrongRelationshipType,
        "Sem_IncorrectRelationshipType",
    ),
    (
        ErrorCode::DuplicateAttributeValue,
        "Sem_UniqueAttributeValue",
    ),
    (
        ErrorCode::UnprefixedAttributeOnAlternateContent,
        "MC_ErrorOnUnprefixedAttributeName",
    ),
    (
        ErrorCode::UndeclaredPrefixInIgnorable,
        "MC_InvalidIgnorableAttribute",
    ),
    (
        ErrorCode::UndeclaredPrefixInMustUnderstand,
        "MC_InvalidMustUnderstandAttribute",
    ),
    (
        ErrorCode::InvalidPreserveAttributes,
        "MC_InvalidPreserveAttributesAttribute",
    ),
    (
        ErrorCode::InvalidPreserveElements,
        "MC_InvalidPreserveElementsAttribute",
    ),
    (
        ErrorCode::InvalidProcessContent,
        "MC_InvalidProcessContentAttribute",
    ),
    (
        ErrorCode::UndeclaredPrefixInRequires,
        "MC_InvalidRequiresAttribute",
    ),
    (
        ErrorCode::XmlAttributeOnCompatibilityElement,
        "MC_InvalidXmlAttribute",
    ),
    (
        ErrorCode::XmlAttributeOnProcessedElement,
        "MC_InvalidXmlAttributeWithProcessContent",
    ),
    (
        ErrorCode::ChoiceWithoutRequires,
        "MC_MissedRequiresAttribute",
    ),
    (
        ErrorCode::AlternateContentWithoutChoice,
        "Sch_IncompleteContentExpectingComplex",
    ),
    (
        ErrorCode::UnexpectedChildInAlternateContent,
        "Sch_InvalidElementContentExpectingComplex",
    ),
    (
        ErrorCode::NestedAlternateContent,
        "Sch_InvalidElementContentExpectingComplex",
    ),
    (
        ErrorCode::DataPartNotAllowed,
        "Pkg_DataPartReferenceIsNotAllowed",
    ),
    (
        ErrorCode::UnknownPartOfStandardType,
        "Pkg_ExtendedPartIsOpenXmlPart",
    ),
    (ErrorCode::DuplicatePart, "Pkg_OnlyOnePartAllowed"),
    (ErrorCode::PartNotAllowed, "Pkg_PartIsNotAllowed"),
    (ErrorCode::MissingRequiredPart, "Pkg_RequiredPartDoNotExist"),
];

/// Every divergence from SDK 3.5.1 officekit intends.
pub const DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "Sch_UnexpectedElementContentExpectingComplex",
        mentioning: None,
        path: &[(CHART, "chart"), (CHART, "extLst")],
        reason: CHART_EXTENSION_LIST,
        targets: &["Office2010", "Office2013", "Office2016"],
    },
    Divergence {
        id: "Sch_UndeclaredAttribute",
        mentioning: None,
        path: &[(CHART, "chart"), (CHART, "extLst"), (CHART, "ext")],
        reason: CHART_EXTENSION_LIST,
        targets: &["Office2019", "Office2021", "Microsoft365"],
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: None,
        path: &[(CHART, "chart"), (CHART, "extLst"), (CHART, "ext")],
        reason: CHART_EXTENSION_LIST,
        targets: &["Office2019", "Office2021", "Microsoft365"],
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "lvlJc")],
        reason: LEVEL_JUSTIFICATION,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "tblPr"), (WORD, "jc")],
        reason: TABLE_JUSTIFICATION,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "tblPrEx"), (WORD, "jc")],
        reason: TABLE_JUSTIFICATION,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "bidiVisual")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "cantSplit")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "tblHeader")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "hidden")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "noWrap")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "tcFitText")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "hideMark")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "noResizeAllowed")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "linkedToFile")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "noBorder")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "flatBorders")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "autoRedefine")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "semiHidden")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "unhideWhenUsed")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "qFormat")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "locked")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "personal")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "personalCompose")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "personalReply")],
        reason: ON_OFF_ONLY,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_UndeclaredAttribute",
        mentioning: Some("'filled'"),
        path: &[(WORD, "background"), (VML, "background")],
        reason: BACKGROUND_FILLED,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:smartTag'"),
        path: &[],
        reason: SMART_TAG,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:smartTagType'"),
        path: &[(WORD, "settings")],
        reason: SMART_TAG,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:doNotEmbedSmartTags'"),
        path: &[(WORD, "settings")],
        reason: SMART_TAG_SETTINGS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:saveSmartTagsAsXml'"),
        path: &[(WORD, "webSettings")],
        reason: SMART_TAG_SETTINGS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:divId'"),
        path: &[(WORD, "pPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:cnfStyle'"),
        path: &[(WORD, "pPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblStyleRowBandSize'"),
        path: &[(WORD, "tblPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblStyleColBandSize'"),
        path: &[(WORD, "tblPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:drawing'"),
        path: &[(WORD, "background")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:movie'"),
        path: &[(WORD, "object")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:label'"),
        path: &[(WORD, "ffData")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:label'"),
        path: &[(WORD, "sdtPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tabIndex'"),
        path: &[(WORD, "ffData")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tabIndex'"),
        path: &[(WORD, "sdtPr")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:title'"),
        path: &[(WORD, "frame")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:title'"),
        path: &[(WORD, "frameset")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:longDesc'"),
        path: &[(WORD, "frame")],
        reason: ECMA_ELEMENTS,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("wordml:txbxContent'"),
        path: &[(WORDPROCESSING_SHAPE, "txbx")],
        reason: STRICT_TEXT_BOX,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "alpha")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "gs")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "lumMod")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "miter")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "satMod")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "shade")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(DRAWING, "tint")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: None,
        path: &[(WORD, "tblInd")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeUnionFailedEx",
        mentioning: None,
        path: &[(WORD, "top")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeUnionFailedEx",
        mentioning: None,
        path: &[(WORD, "start")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeUnionFailedEx",
        mentioning: None,
        path: &[(WORD, "bottom")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeUnionFailedEx",
        mentioning: None,
        path: &[(WORD, "end")],
        reason: STRICT_VALUES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:pStyle'"),
        path: &[(WORD, "pPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:pPrChange'"),
        path: &[(WORD, "pPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblStyle'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblpPr'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblOverlap'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:bidiVisual'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblW'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblLayout'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblLook'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblCaption'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_InvalidElementContentExpectingComplex",
        mentioning: Some("main:tblDescription'"),
        path: &[(WORD, "tblPr")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeUnionFailedEx",
        mentioning: None,
        path: &[(WORD, "divId")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: Some("less than or equal to 3"),
        path: &[(WORD, "tblStyleRowBandSize")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
    Divergence {
        id: "Sch_AttributeValueDataTypeDetailed",
        mentioning: Some("less than or equal to 3"),
        path: &[(WORD, "tblStyleColBandSize")],
        reason: STYLE_PROPERTIES,
        targets: ALL_TARGETS,
    },
];

const ALL_TARGETS: &[&str] = &[
    "Office2010",
    "Office2013",
    "Office2016",
    "Office2019",
    "Office2021",
    "Microsoft365",
];

const BACKGROUND_FILLED: &str = "SDK 3.5.1 spells the attribute of v:background `fill`; ECMA-376 \
    names it `filled` (vml-main.xsd, AG_Fill)";

const ON_OFF_ONLY: &str = "SDK 3.5.1 types these elements as CT_OnOffOnly, which takes only on \
    and off; ECMA-376 types them CT_OnOff (CT_TblPrBase, CT_TrPrBase, CT_TcPrBase, CT_Frame, \
    CT_FramesetSplitbar, CT_Style), which takes every ST_OnOff value";

const CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";

const CHART_EXTENSION_LIST: &str = "SDK 3.5.1 types the c:ext of c:chart's extension list as the \
    content of c16r3:dataDisplayOptions16, with no uri, and from Office 2019; ECMA-376 types it \
    CT_Extension, and Word writes the extension at every version";

const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

const LEVEL_JUSTIFICATION: &str = "SDK 3.5.1 gives w:lvlJc the values left, center and right; \
    ECMA-376 types it CT_Jc (CT_Lvl), whose val is ST_Jc, with start and end";

const SMART_TAG: &str = "SDK 3.5.1 leaves out w:smartTag and w:smartTagType; ECMA-376 declares \
    them in EG_ContentRunContent, EG_ContentRunContentBase and CT_Settings, and Word 16 writes them";

const ECMA_ELEMENTS: &str = "SDK 3.5.1 leaves out elements ECMA-376 declares: divId and cnfStyle \
    in CT_PPrBase, the band sizes in CT_TblPrBase, drawing in CT_Background, movie in CT_Object, \
    label and tabIndex in CT_FFData and CT_SdtPr, title and longDesc in CT_Frame and CT_Frameset";

const SMART_TAG_SETTINGS: &str = "SDK 3.5.1 leaves out w:doNotEmbedSmartTags and \
    w:saveSmartTagsAsXml; ECMA-376 declares them in CT_Settings and CT_WebSettings";

const STRICT_TEXT_BOX: &str = "SDK 3.5.1 gives wps:txbx only w:txbxContent; Microsoft documents \
    wne:txbxContent in its place in Strict ([MS-ODRAWXML] 2.13.3.2, CT_TextboxInfo)";

const STRICT_VALUES: &str = "SDK 3.5.1 checks a Strict file's values against their Transitional \
    types; ECMA-376 Strict types them differently";

const STYLE_PROPERTIES: &str = "SDK 3.5.1 narrows the properties of styles, table style \
    conditions, the document defaults and numbering levels, and the ranges of divId and the band \
    sizes; ECMA-376 types them CT_PPrGeneral, CT_TblPrBase and CT_DecimalNumber";

const TABLE_JUSTIFICATION: &str = "SDK 3.5.1 gives the w:jc of a table's properties the values \
    left, center and right; ECMA-376 types it CT_JcTable, whose val is ST_JcTable, with start and end";

const VML: &str = "urn:schemas-microsoft-com:vml";

const WORDPROCESSING_SHAPE: &str =
    "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

const WORD: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// An SDK 3.5.1 error officekit deliberately does not report, because the
/// markup it flags is valid in ECMA-376 and written by Office.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Divergence {
    /// SDK 3.5.1's error id.
    pub id: &'static str,
    /// Text the error's message must hold, when the id and path alone would cover errors that stay.
    pub mentioning: Option<&'static str>,
    /// The end of the error's path: namespace URI and local name of each step.
    pub path: &'static [(&'static str, &'static str)],
    pub reason: &'static str,
    /// The targets the divergence holds at, as the records name them.
    pub targets: &'static [&'static str],
}

/// What two errors must share to be the same.
///
/// File, target, kind, SDK 3.5.1's id, part (folded to ASCII lower case, as
/// part names compare), and path, step by step by namespace URI, local name,
/// and index, whatever prefixes the files used.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct Key {
    pub file: String,
    pub id: String,
    pub kind: String,
    pub part: String,
    pub path: Vec<(String, String, u64)>,
    pub target: String,
}

/// What a parity run found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Report {
    /// The number of missing errors of each kind asked for, in the order asked.
    pub counts: Vec<(String, usize)>,
    /// The oracle errors officekit lacks on purpose, each with its reason.
    pub divergent: Vec<(Value, &'static str)>,
    /// Every oracle error of those kinds officekit's result lacks, the intended divergences apart.
    pub missing: Vec<Value>,
}

impl Report {
    /// Whether officekit's result lacks no error.
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.missing.is_empty()
    }

    /// `missing by kind: schema 0, markup-compatibility 0`.
    #[must_use]
    pub fn summary(&self) -> String {
        let counts: Vec<String> = self
            .counts
            .iter()
            .map(|(kind, count)| format!("{kind} {count}"))
            .collect();
        format!("missing by kind: {}", counts.join(", "))
    }
}

/// The key of a record, with `id` as its error id.
fn key(record: &Value, id: &str) -> Key {
    let text = |name: &str| {
        record
            .get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let path = record
        .get("path")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |steps| {
            steps
                .iter()
                .map(|step| {
                    let field = |name: &str| {
                        step.get(name)
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned()
                    };
                    (
                        field("namespace"),
                        field("local"),
                        step.get("index").and_then(Value::as_u64).unwrap_or(0),
                    )
                })
                .collect()
        });
    Key {
        file: text("file"),
        id: id.to_owned(),
        kind: text("kind"),
        part: text("part").to_ascii_lowercase(),
        path,
        target: text("target"),
    }
}

/// The reason officekit lacks an oracle error on purpose, when it does.
#[must_use]
pub fn divergence(record: &Value) -> Option<&'static str> {
    let key = oracle_key(record);
    DIVERGENCES
        .iter()
        .find(|row| {
            row.id == key.id
                && row.targets.contains(&key.target.as_str())
                && row.mentioning.is_none_or(|text| {
                    record
                        .get("message")
                        .and_then(Value::as_str)
                        .is_some_and(|message| message.contains(text))
                })
                && key.path.len() >= row.path.len()
                && key.path[key.path.len() - row.path.len()..]
                    .iter()
                    .zip(row.path)
                    .all(|((namespace, local, _), (row_namespace, row_local))| {
                        namespace == row_namespace && local == row_local
                    })
        })
        .map(|row| row.reason)
}

/// Every oracle error officekit's result lacks.
///
/// Each officekit error answers one oracle error with its key, so two SDK
/// 3.5.1 errors with one key need two from officekit; officekit's errors left
/// over do not count.
#[must_use]
pub fn missing<'oracle>(oracle: &'oracle [Value], officekit: &[Value]) -> Vec<&'oracle Value> {
    let mut available: BTreeMap<Key, usize> = BTreeMap::new();
    for record in officekit {
        if let Some(found) = officekit_key(record) {
            *available.entry(found).or_default() += 1;
        }
    }
    oracle
        .iter()
        .filter(|record| {
            let count = available.entry(oracle_key(record)).or_default();
            if *count == 0 {
                return true;
            }
            *count -= 1;
            false
        })
        .collect()
}

/// The key of an officekit record, its `code` mapped to SDK 3.5.1's id;
/// `None` for a code SDK 3.5.1 has no id for.
#[must_use]
pub fn officekit_key(record: &Value) -> Option<Key> {
    let code = ErrorCode::parse(record.get("code").and_then(Value::as_str)?)?;
    Some(key(record, sdk_key(code)?))
}

/// The key of an oracle record, whose `id` is SDK 3.5.1's.
#[must_use]
pub fn oracle_key(record: &Value) -> Key {
    key(
        record,
        record.get("id").and_then(Value::as_str).unwrap_or_default(),
    )
}

/// The records of a JSON array file.
fn records<S>(system: &S, path: &Path) -> Result<Vec<Value>>
where
    S: System + ?Sized,
{
    let text = system
        .read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let parsed: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let Value::Array(items) = parsed else {
        bail!("{} holds no JSON array", path.display());
    };
    Ok(items)
}

/// Compare the oracle's records in `oracle` with officekit's in every
/// `.json` file of the folder `officekit`, for the errors of `kinds`.
///
/// # Errors
///
/// A file that cannot be read or holds no JSON array of records.
pub fn run<S>(system: &S, oracle: &Path, officekit: &Path, kinds: &[String]) -> Result<Report>
where
    S: System + ?Sized,
{
    let expected: Vec<Value> = records(system, oracle)?
        .into_iter()
        .filter(|record| {
            record
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kinds.iter().any(|wanted| wanted == kind))
        })
        .collect();
    let mut found = Vec::new();
    let mut files = system
        .read_dir(officekit)
        .with_context(|| format!("listing {}", officekit.display()))?;
    files.sort();
    for file in files {
        if file
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            found.extend(records(system, &file)?);
        }
    }
    let (intended, lacking): (Vec<Value>, Vec<Value>) = missing(&expected, &found)
        .into_iter()
        .cloned()
        .partition(|record| divergence(record).is_some());
    let divergent = intended
        .into_iter()
        .filter_map(|record| divergence(&record).map(|reason| (record, reason)))
        .collect();
    let counts = kinds
        .iter()
        .map(|kind| {
            let count = lacking
                .iter()
                .filter(|record| record.get("kind").and_then(Value::as_str) == Some(kind.as_str()))
                .count();
            (kind.clone(), count)
        })
        .collect();
    Ok(Report {
        counts,
        divergent,
        missing: lacking,
    })
}

/// SDK 3.5.1's id for one of officekit's codes; `None` for the three codes of
/// a file that declares Strict.
#[must_use]
pub fn sdk_key(code: ErrorCode) -> Option<&'static str> {
    SDK_KEYS
        .iter()
        .find(|(known, _)| *known == code)
        .map(|(_, id)| *id)
}
