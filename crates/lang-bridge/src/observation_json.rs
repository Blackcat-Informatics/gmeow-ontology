// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native transport of complete GMN and grammar observations, without normalization.
use crate::bridge::{IngestDiagnostic, LangFailure};
use crate::gmn1_codec::{Gmn1ConstructCategory, Gmn1Error, QuadCoverage, UncoveredTerm};
use crate::gmn1_witness::StandaloneReport;
use crate::grammar::{GrammarRule, RuleExpr};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within as _},
};

::gmeow_errors::json::json_string_newtype!(UncoveredTerm);
::purrdf_lex::json_record!(impl ToJson for StandaloneReport {
    "checked" => checked,
    "skipped" => skipped,
});
impl FromJson for StandaloneReport {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct StandaloneReport")?;
        Ok(Self {
            checked: fields.required("checked")?,
            skipped: fields.required("skipped")?,
        })
    }
}
impl ToJson for Gmn1Error {
    fn to_json(&self) -> Value {
        match self {
            Self::Uncovered(value) => Object::new().with("Uncovered", value.to_json()).into(),
            Self::NamedGraphOutOfDomain { graph } => Object::new()
                .with(
                    "NamedGraphOutOfDomain",
                    Object::new().with("graph", graph.as_str()),
                )
                .into(),
            Self::NonCanonicalOrder { detail } => Object::new()
                .with(
                    "NonCanonicalOrder",
                    Object::new().with("detail", detail.as_str()),
                )
                .into(),
            Self::MalformedNumber { token } => Object::new()
                .with(
                    "MalformedNumber",
                    Object::new().with("token", token.as_str()),
                )
                .into(),
            Self::UndeclaredDialectVersion { detail } => Object::new()
                .with(
                    "UndeclaredDialectVersion",
                    Object::new().with("detail", detail.as_str()),
                )
                .into(),
            Self::NonDecodableGrammar { detail } => Object::new()
                .with(
                    "NonDecodableGrammar",
                    Object::new().with("detail", detail.as_str()),
                )
                .into(),
            Self::NonNfcLiteral { lexical } => Object::new()
                .with(
                    "NonNfcLiteral",
                    Object::new().with("lexical", lexical.as_str()),
                )
                .into(),
            Self::PerClaimMismatch { subject } => Object::new()
                .with(
                    "PerClaimMismatch",
                    Object::new().with("subject", subject.as_str()),
                )
                .into(),
        }
    }
}
impl FromJson for Gmn1Error {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "Gmn1Error", "one Gmn1Error variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Uncovered" => UncoveredTerm::from_json(payload).map(Self::Uncovered),
                "NamedGraphOutOfDomain" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::NamedGraphOutOfDomain")?;
                    fields
                        .required("graph")
                        .map(|graph| Self::NamedGraphOutOfDomain { graph })
                }
                "NonCanonicalOrder" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::NonCanonicalOrder")?;
                    fields
                        .required("detail")
                        .map(|detail| Self::NonCanonicalOrder { detail })
                }
                "MalformedNumber" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::MalformedNumber")?;
                    fields
                        .required("token")
                        .map(|token| Self::MalformedNumber { token })
                }
                "UndeclaredDialectVersion" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant Gmn1Error::UndeclaredDialectVersion",
                    )?;
                    fields
                        .required("detail")
                        .map(|detail| Self::UndeclaredDialectVersion { detail })
                }
                "NonDecodableGrammar" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::NonDecodableGrammar")?;
                    fields
                        .required("detail")
                        .map(|detail| Self::NonDecodableGrammar { detail })
                }
                "NonNfcLiteral" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::NonNfcLiteral")?;
                    fields
                        .required("lexical")
                        .map(|lexical| Self::NonNfcLiteral { lexical })
                }
                "PerClaimMismatch" => {
                    let mut fields =
                        Record::new(payload, "struct variant Gmn1Error::PerClaimMismatch")?;
                    fields
                        .required("subject")
                        .map(|subject| Self::PerClaimMismatch { subject })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "Uncovered",
                        "NamedGraphOutOfDomain",
                        "NonCanonicalOrder",
                        "MalformedNumber",
                        "UndeclaredDialectVersion",
                        "NonDecodableGrammar",
                        "NonNfcLiteral",
                        "PerClaimMismatch",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
impl ToJson for Gmn1ConstructCategory {
    fn to_json(&self) -> Value {
        match self {
            Self::IriGlyph => "IriGlyph".to_json(),
            Self::IriDictAlias => "IriDictAlias".to_json(),
            Self::IriPrefixMangled => "IriPrefixMangled".to_json(),
            Self::IriPrefixMangledSlashEscaped => "IriPrefixMangledSlashEscaped".to_json(),
            Self::IriBareNamespaceRoot => "IriBareNamespaceRoot".to_json(),
            Self::IriExternalByReference => "IriExternalByReference".to_json(),
            Self::BlankNode => "BlankNode".to_json(),
            Self::LiteralIdentifier => "LiteralIdentifier".to_json(),
            Self::LiteralInteger => "LiteralInteger".to_json(),
            Self::LiteralDecimal => "LiteralDecimal".to_json(),
            Self::LiteralByReference => "LiteralByReference".to_json(),
            Self::TripleTerm => "TripleTerm".to_json(),
        }
    }
}
impl FromJson for Gmn1ConstructCategory {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Gmn1ConstructCategory")? {
            "IriGlyph" => Ok(Self::IriGlyph),
            "IriDictAlias" => Ok(Self::IriDictAlias),
            "IriPrefixMangled" => Ok(Self::IriPrefixMangled),
            "IriPrefixMangledSlashEscaped" => Ok(Self::IriPrefixMangledSlashEscaped),
            "IriBareNamespaceRoot" => Ok(Self::IriBareNamespaceRoot),
            "IriExternalByReference" => Ok(Self::IriExternalByReference),
            "BlankNode" => Ok(Self::BlankNode),
            "LiteralIdentifier" => Ok(Self::LiteralIdentifier),
            "LiteralInteger" => Ok(Self::LiteralInteger),
            "LiteralDecimal" => Ok(Self::LiteralDecimal),
            "LiteralByReference" => Ok(Self::LiteralByReference),
            "TripleTerm" => Ok(Self::TripleTerm),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "IriGlyph",
                    "IriDictAlias",
                    "IriPrefixMangled",
                    "IriPrefixMangledSlashEscaped",
                    "IriBareNamespaceRoot",
                    "IriExternalByReference",
                    "BlankNode",
                    "LiteralIdentifier",
                    "LiteralInteger",
                    "LiteralDecimal",
                    "LiteralByReference",
                    "TripleTerm",
                ],
            )),
        }
    }
}
impl ToJson for QuadCoverage {
    fn to_json(&self) -> Value {
        match self {
            Self::Uncovered(value) => Object::new().with("Uncovered", value.to_json()).into(),
            Self::Covered {
                subject,
                predicate,
                object,
            } => Object::new()
                .with(
                    "Covered",
                    Object::new()
                        .with("subject", subject.to_json())
                        .with("predicate", predicate.to_json())
                        .with("object", object.to_json()),
                )
                .into(),
        }
    }
}
impl FromJson for QuadCoverage {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "QuadCoverage", "one QuadCoverage variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Uncovered" => UncoveredTerm::from_json(payload).map(Self::Uncovered),
                "Covered" => {
                    let mut fields = Record::new(payload, "struct variant QuadCoverage::Covered")?;
                    Ok(Self::Covered {
                        subject: fields.required("subject")?,
                        predicate: fields.required("predicate")?,
                        object: fields.required("object")?,
                    })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &["Covered", "Uncovered"],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
impl ToJson for LangFailure {
    fn to_json(&self) -> Value {
        self.as_str().to_json()
    }
}
impl FromJson for LangFailure {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "LangFailure")? {
            "SilentIngestDrop" => Ok(Self::SilentIngestDrop),
            "NonUtf8Surface" => Ok(Self::NonUtf8Surface),
            name => Err(DecodeError::unknown_variant(
                name,
                &["SilentIngestDrop", "NonUtf8Surface"],
            )),
        }
    }
}
::purrdf_lex::json_record!(impl ToJson for IngestDiagnostic {
    "failure_class" => failure_class,
    "construct" => construct,
});
impl FromJson for IngestDiagnostic {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct IngestDiagnostic")?;
        Ok(Self {
            failure_class: fields.required("failure_class")?,
            construct: fields.required("construct")?,
        })
    }
}
::purrdf_lex::json_record!(impl ToJson for GrammarRule {
    "name" => name,
    "body" => body,
});
impl FromJson for GrammarRule {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct GrammarRule")?;
        Ok(Self {
            name: fields.required("name")?,
            body: fields.required("body")?,
        })
    }
}
impl ToJson for RuleExpr {
    fn to_json(&self) -> Value {
        match self {
            Self::Ref(value) => Object::new().with("Ref", value.to_json()).into(),
            Self::Terminal(value) => Object::new().with("Terminal", value.to_json()).into(),
            Self::CharClass(value) => Object::new().with("CharClass", value.to_json()).into(),
            Self::Hex(value) => Object::new().with("Hex", value.to_json()).into(),
            Self::Seq(value) => Object::new().with("Seq", value.to_json()).into(),
            Self::Alt(value) => Object::new().with("Alt", value.to_json()).into(),
            Self::Star(value) => Object::new().with("Star", value.to_json()).into(),
            Self::Plus(value) => Object::new().with("Plus", value.to_json()).into(),
            Self::Opt(value) => Object::new().with("Opt", value.to_json()).into(),
            Self::Group(value) => Object::new().with("Group", value.to_json()).into(),
            Self::Range(low, high) => Object::new().with("Range", (low, high).to_json()).into(),
            Self::Diff(left, right) => Object::new().with("Diff", (left, right).to_json()).into(),
            Self::Repeat(low, high, inner) => Object::new()
                .with("Repeat", (low, high, inner).to_json())
                .into(),
        }
    }
}
fn operands<'a>(
    value: &'a Value,
    width: usize,
    expected: &str,
) -> Result<&'a [Value], DecodeError> {
    let items = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, expected))?;
    if items.len() != width {
        return Err(DecodeError::invalid_value(value, expected));
    }
    Ok(items)
}
impl FromJson for RuleExpr {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "RuleExpr", "one RuleExpr variant")?;
        let decode = || -> Result<Self, DecodeError> {
            Ok(match tag {
                "Ref" => Self::Ref(String::from_json(payload)?),
                "Terminal" => Self::Terminal(String::from_json(payload)?),
                "CharClass" => Self::CharClass(String::from_json(payload)?),
                "Hex" => Self::Hex(String::from_json(payload)?),
                "Seq" => Self::Seq(Vec::<Self>::from_json(payload)?),
                "Alt" => Self::Alt(Vec::<Self>::from_json(payload)?),
                "Star" => Self::Star(Box::<Self>::from_json(payload)?),
                "Plus" => Self::Plus(Box::<Self>::from_json(payload)?),
                "Opt" => Self::Opt(Box::<Self>::from_json(payload)?),
                "Group" => Self::Group(Box::<Self>::from_json(payload)?),
                "Range" => {
                    let [low, high] = gmeow_errors::json::read_string_row(payload)?;
                    Self::Range(low, high)
                }
                "Diff" => {
                    let items = operands(payload, 2, "two RuleExpr operands")?;
                    Self::Diff(
                        Box::new(Self::from_json(&items[0]).map_err(|error| error.within("0"))?),
                        Box::new(Self::from_json(&items[1]).map_err(|error| error.within("1"))?),
                    )
                }
                "Repeat" => {
                    let items = operands(payload, 3, "three RuleExpr repeat operands")?;
                    let bound = |index: usize| -> Result<Option<u32>, DecodeError> {
                        if matches!(items[index], Value::Null) {
                            Ok(None)
                        } else {
                            gmeow_errors::json::read_unsigned(&items[index], "u32 repeat bound")
                                .map(Some)
                                .map_err(|error| error.within(&index.to_string()))
                        }
                    };
                    Self::Repeat(
                        bound(0)?,
                        bound(1)?,
                        Box::new(Self::from_json(&items[2]).map_err(|error| error.within("2"))?),
                    )
                }
                name => {
                    return Err(DecodeError::unknown_variant(
                        name,
                        &[
                            "Ref",
                            "Terminal",
                            "CharClass",
                            "Hex",
                            "Range",
                            "Seq",
                            "Alt",
                            "Diff",
                            "Star",
                            "Plus",
                            "Opt",
                            "Repeat",
                            "Group",
                        ],
                    ));
                }
            })
        };
        decode().map_err(|error| error.within(tag))
    }
}
#[cfg(test)]
mod tests;
