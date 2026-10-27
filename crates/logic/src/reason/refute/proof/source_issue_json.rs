// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact source-issue transport, separate from source admission and proof checking.

use super::RefutationSourceIssue;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

const VARIANTS: &[&str] = &[
    "MalformedNil",
    "ConflictingListField",
    "ExpressionMultiplicity",
    "UnsupportedExpressionOwner",
    "UnsupportedExpressionOperand",
    "UnsupportedListMember",
    "CyclicList",
    "IncompleteList",
];

impl ToJson for RefutationSourceIssue {
    fn to_json(&self) -> Value {
        let (tag, fields) = match self {
            Self::MalformedNil => return "MalformedNil".to_json(),
            Self::ConflictingListField { subject, predicate } => (
                "ConflictingListField",
                Object::new()
                    .with("subject", subject.to_json())
                    .with("predicate", predicate.to_json()),
            ),
            Self::ExpressionMultiplicity { subject, predicate } => (
                "ExpressionMultiplicity",
                Object::new()
                    .with("subject", subject.to_json())
                    .with("predicate", predicate.to_json()),
            ),
            Self::UnsupportedExpressionOwner { owner, predicate } => (
                "UnsupportedExpressionOwner",
                Object::new()
                    .with("owner", crate::term_serde::to_json(owner))
                    .with("predicate", predicate.to_json()),
            ),
            Self::UnsupportedExpressionOperand {
                owner,
                predicate,
                operand,
            } => (
                "UnsupportedExpressionOperand",
                Object::new()
                    .with("owner", owner.to_json())
                    .with("predicate", predicate.to_json())
                    .with("operand", crate::term_serde::to_json(operand)),
            ),
            Self::UnsupportedListMember { owner, head } => (
                "UnsupportedListMember",
                Object::new()
                    .with("owner", owner.to_json())
                    .with("head", head.to_json()),
            ),
            Self::CyclicList { owner, head } => (
                "CyclicList",
                Object::new()
                    .with("owner", owner.to_json())
                    .with("head", head.to_json()),
            ),
            Self::IncompleteList { owner, head } => (
                "IncompleteList",
                Object::new()
                    .with("owner", owner.to_json())
                    .with("head", head.to_json()),
            ),
        };
        Object::new().with(tag, Value::from(fields)).into()
    }
}
impl FromJson for RefutationSourceIssue {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        if let Some(name) = value.as_str() {
            return match name {
                "MalformedNil" => Ok(Self::MalformedNil),
                name => Err(DecodeError::unknown_variant(name, VARIANTS)),
            };
        }
        let (tag, payload) = gmeow_errors::json::read_variant(
            value,
            "RefutationSourceIssue",
            "one RefutationSourceIssue variant",
        )?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "MalformedNil" => {
                    gmeow_errors::json::read_null(payload, "null MalformedNil payload")
                        .map(|()| Self::MalformedNil)
                }
                "ConflictingListField" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::ConflictingListField",
                    )?;
                    Ok(Self::ConflictingListField {
                        subject: fields.required("subject")?,
                        predicate: fields.required("predicate")?,
                    })
                }
                "ExpressionMultiplicity" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::ExpressionMultiplicity",
                    )?;
                    Ok(Self::ExpressionMultiplicity {
                        subject: fields.required("subject")?,
                        predicate: fields.required("predicate")?,
                    })
                }
                "UnsupportedExpressionOwner" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::UnsupportedExpressionOwner",
                    )?;
                    Ok(Self::UnsupportedExpressionOwner {
                        owner: fields.required_with("owner", crate::term_serde::from_json)?,
                        predicate: fields.required("predicate")?,
                    })
                }
                "UnsupportedExpressionOperand" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::UnsupportedExpressionOperand",
                    )?;
                    Ok(Self::UnsupportedExpressionOperand {
                        owner: fields.required("owner")?,
                        predicate: fields.required("predicate")?,
                        operand: fields.required_with("operand", crate::term_serde::from_json)?,
                    })
                }
                "UnsupportedListMember" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::UnsupportedListMember",
                    )?;
                    Ok(Self::UnsupportedListMember {
                        owner: fields.required("owner")?,
                        head: fields.required("head")?,
                    })
                }
                "CyclicList" => {
                    let mut fields =
                        Record::new(payload, "struct variant RefutationSourceIssue::CyclicList")?;
                    Ok(Self::CyclicList {
                        owner: fields.required("owner")?,
                        head: fields.required("head")?,
                    })
                }
                "IncompleteList" => {
                    let mut fields = Record::new(
                        payload,
                        "struct variant RefutationSourceIssue::IncompleteList",
                    )?;
                    Ok(Self::IncompleteList {
                        owner: fields.required("owner")?,
                        head: fields.required("head")?,
                    })
                }
                name => Err(DecodeError::unknown_variant(name, VARIANTS)),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
