// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Data-only JSON for relational atoms and retained numeric operand metadata.

use super::{NumericOperator, RcAtom, RcComparison, RcNumeric, RcViolationGap, RcViolationResidue};
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
        let (subject, predicate, object, negated) = read_signed_atom(value, "struct RcAtom")?;
        Ok(Self {
            subject,
            predicate,
            object,
            negated,
        })
    }
}

/// Read the shared persistent fields of compiler and executable signed atoms.
/// Operand hydration belongs to the caller's term type; no lowering is performed.
///
/// # Errors
/// Retains native record/member refusals and their exact field paths.
pub fn read_signed_atom<T: FromJson>(
    value: &Value,
    expected: &'static str,
) -> Result<(T, String, T, bool), DecodeError> {
    let mut fields = Record::new(value, expected)?;
    Ok((
        fields.required("subject")?,
        fields.required("predicate")?,
        fields.required("object")?,
        fields.required("negated")?,
    ))
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
impl ToJson for RcComparison {
    fn to_json(&self) -> Value {
        match self {
            Self::Less => "Less",
            Self::LessEqual => "LessEqual",
            Self::Greater => "Greater",
            Self::GreaterEqual => "GreaterEqual",
        }
        .to_json()
    }
}
impl FromJson for RcComparison {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "RcComparison")? {
            "Less" => Ok(Self::Less),
            "LessEqual" => Ok(Self::LessEqual),
            "Greater" => Ok(Self::Greater),
            "GreaterEqual" => Ok(Self::GreaterEqual),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Less", "LessEqual", "Greater", "GreaterEqual"],
            )),
        }
    }
}

impl ToJson for RcViolationGap {
    fn to_json(&self) -> Value {
        match self {
            Self::NotUniversalImplication => "NotUniversalImplication",
            Self::NoFailureClass => "NoFailureClass",
            Self::BuiltinBoundConsequent => "BuiltinBoundConsequent",
            Self::UnsupportedAntecedent => "UnsupportedAntecedent",
            Self::UnsupportedConsequent => "UnsupportedConsequent",
            Self::UnsafeFocusVariable => "UnsafeFocusVariable",
            Self::UnsafeTermKindVariable => "UnsafeTermKindVariable",
            Self::UnsafeEqualityVariable => "UnsafeEqualityVariable",
            Self::UnsafeUniversalVariable => "UnsafeUniversalVariable",
        }
        .to_json()
    }
}
impl FromJson for RcViolationGap {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "RcViolationGap")? {
            "NotUniversalImplication" => Ok(Self::NotUniversalImplication),
            "NoFailureClass" => Ok(Self::NoFailureClass),
            "BuiltinBoundConsequent" => Ok(Self::BuiltinBoundConsequent),
            "UnsupportedAntecedent" => Ok(Self::UnsupportedAntecedent),
            "UnsupportedConsequent" => Ok(Self::UnsupportedConsequent),
            "UnsafeFocusVariable" => Ok(Self::UnsafeFocusVariable),
            "UnsafeTermKindVariable" => Ok(Self::UnsafeTermKindVariable),
            "UnsafeEqualityVariable" => Ok(Self::UnsafeEqualityVariable),
            "UnsafeUniversalVariable" => Ok(Self::UnsafeUniversalVariable),
            name => Err(DecodeError::unknown_variant(
                name,
                &[
                    "NotUniversalImplication",
                    "NoFailureClass",
                    "BuiltinBoundConsequent",
                    "UnsupportedAntecedent",
                    "UnsupportedConsequent",
                    "UnsafeFocusVariable",
                    "UnsafeTermKindVariable",
                    "UnsafeEqualityVariable",
                    "UnsafeUniversalVariable",
                ],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for RcViolationResidue {
    "constraint_iri" => constraint_iri,
    "gap" => gap,
});
impl FromJson for RcViolationResidue {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RcViolationResidue")?;
        Ok(Self {
            constraint_iri: fields.required("constraint_iri")?,
            gap: fields.required("gap")?,
        })
    }
}
