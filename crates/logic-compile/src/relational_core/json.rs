// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Data-only JSON for relational atoms and retained numeric operand metadata.

use super::{NumericOperator, RcAtom, RcNumeric};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

::purrdf_lex::json_record!(impl ToJson for RcAtom {
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
    "negated" => negated,
});
impl FromJson for RcAtom {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RcAtom")?;
        Ok(Self {
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
            negated: fields.required("negated")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for RcNumeric {
    "operator" => operator,
    "left" => left,
    "right" => right,
    "result" => result,
});
impl FromJson for RcNumeric {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RcNumeric")?;
        // This is retained diagnostic data, not numeric execution admission.
        Ok(Self {
            operator: fields.required("operator")?,
            left: fields.required("left")?,
            right: fields.required("right")?,
            result: fields.optional("result")?,
        })
    }
}

impl ToJson for NumericOperator {
    fn to_json(&self) -> Value {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Multiply => "Multiply",
            Self::Divide => "Divide",
            Self::ExactDivide => "ExactDivide",
            Self::Equal => "Equal",
            Self::NotEqual => "NotEqual",
            Self::Less => "Less",
            Self::LessOrEqual => "LessOrEqual",
            Self::Greater => "Greater",
            Self::GreaterOrEqual => "GreaterOrEqual",
        }
        .to_json()
    }
}
impl FromJson for NumericOperator {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "NumericOperator")? {
            "Add" => Ok(Self::Add),
            "Subtract" => Ok(Self::Subtract),
            "Multiply" => Ok(Self::Multiply),
            "Divide" => Ok(Self::Divide),
            "ExactDivide" => Ok(Self::ExactDivide),
            "Equal" => Ok(Self::Equal),
            "NotEqual" => Ok(Self::NotEqual),
            "Less" => Ok(Self::Less),
            "LessOrEqual" => Ok(Self::LessOrEqual),
            "Greater" => Ok(Self::Greater),
            "GreaterOrEqual" => Ok(Self::GreaterOrEqual),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "Add",
                    "Subtract",
                    "Multiply",
                    "Divide",
                    "ExactDivide",
                    "Equal",
                    "NotEqual",
                    "Less",
                    "LessOrEqual",
                    "Greater",
                    "GreaterOrEqual",
                ],
            )),
        }
    }
}
