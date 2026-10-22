// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native canonical AST transport with checked term/atom admission.
use super::{Formula, Term, literal_serde};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};

impl ToJson for Term {
    fn to_json(&self) -> Value {
        match self {
            Self::Var(value) => Object::new().with("Var", value.to_json()).into(),
            Self::Iri(value) => Object::new().with("Iri", value.to_json()).into(),
            Self::Literal(value) => Object::new()
                .with("Literal", literal_serde::single::to_json(value))
                .into(),
            Self::SequenceMarker(value) => {
                Object::new().with("SequenceMarker", value.to_json()).into()
            }
            Self::App { symbol, args } => Object::new()
                .with(
                    "App",
                    Value::from(
                        Object::new()
                            .with("symbol", symbol.to_json())
                            .with("args", args.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for Term {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) = gmeow_errors::json::read_variant(value, "Term", "one Term variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Var" => Self::var(String::from_json(payload)?).map_err(DecodeError::custom),
                "Iri" => Self::iri(String::from_json(payload)?).map_err(DecodeError::custom),
                "Literal" => Self::rdf_literal(literal_serde::single::from_json(payload)?)
                    .map_err(DecodeError::custom),
                "SequenceMarker" => {
                    Self::sequence_marker(String::from_json(payload)?).map_err(DecodeError::custom)
                }
                "App" => {
                    let mut fields = Record::new(payload, "struct variant Term::App")?;
                    let symbol: String = fields.required("symbol")?;
                    let args = fields.required("args")?;
                    Self::app(symbol, args).map_err(DecodeError::custom)
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &["Var", "Iri", "Literal", "SequenceMarker", "App"],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
impl ToJson for Formula {
    fn to_json(&self) -> Value {
        match self {
            Self::Atom { relation, args } => Object::new()
                .with(
                    "Atom",
                    Value::from(
                        Object::new()
                            .with("relation", relation.to_json())
                            .with("args", args.to_json()),
                    ),
                )
                .into(),
            Self::Not(body) => Object::new().with("Not", body.to_json()).into(),
            Self::And(body) => Object::new().with("And", body.to_json()).into(),
            Self::Or(body) => Object::new().with("Or", body.to_json()).into(),
            Self::Implies(left, right) => Object::new()
                .with("Implies", (left, right).to_json())
                .into(),
            Self::Iff(left, right) => Object::new().with("Iff", (left, right).to_json()).into(),
            Self::Forall { vars, body } => Object::new()
                .with(
                    "Forall",
                    Value::from(
                        Object::new()
                            .with("vars", vars.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
            Self::Exists { vars, body } => Object::new()
                .with(
                    "Exists",
                    Value::from(
                        Object::new()
                            .with("vars", vars.to_json())
                            .with("body", body.to_json()),
                    ),
                )
                .into(),
        }
    }
}
impl FromJson for Formula {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "Formula", "one Formula variant")?;
        let decode = || -> Result<Self, DecodeError> {
            match tag {
                "Atom" => {
                    let mut fields = Record::new(payload, "struct variant Formula::Atom")?;
                    let relation = fields.required("relation")?;
                    let args = fields.required("args")?;
                    Self::atom(relation, args).map_err(DecodeError::custom)
                }
                "Not" => Box::<Self>::from_json(payload).map(Self::Not),
                "And" => Vec::<Self>::from_json(payload).map(Self::And),
                "Or" => Vec::<Self>::from_json(payload).map(Self::Or),
                "Implies" => {
                    let (left, right) = binary_operands(payload)?;
                    Ok(Self::Implies(left, right))
                }
                "Iff" => {
                    let (left, right) = binary_operands(payload)?;
                    Ok(Self::Iff(left, right))
                }
                "Forall" => {
                    let mut fields = Record::new(payload, "struct variant Formula::Forall")?;
                    Ok(Self::Forall {
                        vars: fields.required("vars")?,
                        body: fields.required("body")?,
                    })
                }
                "Exists" => {
                    let mut fields = Record::new(payload, "struct variant Formula::Exists")?;
                    Ok(Self::Exists {
                        vars: fields.required("vars")?,
                        body: fields.required("body")?,
                    })
                }
                name => Err(DecodeError::unknown_variant(
                    name,
                    &[
                        "Atom", "Not", "And", "Or", "Implies", "Iff", "Forall", "Exists",
                    ],
                )),
            }
        };
        decode().map_err(|error| error.within(tag))
    }
}
/// Admit exactly two ordered operands for the two binary AST constructors.
fn binary_operands(value: &Value) -> Result<(Box<Formula>, Box<Formula>), DecodeError> {
    let cells = value
        .as_array()
        .ok_or_else(|| DecodeError::invalid_type(value, "two Formula operands"))?;
    if cells.len() != 2 {
        return Err(DecodeError::invalid_value(
            value,
            "exactly two Formula operands",
        ));
    }
    let left =
        Box::<Formula>::from_json(&cells[0]).map_err(|error| error.within_index_native(0, None))?;
    let right =
        Box::<Formula>::from_json(&cells[1]).map_err(|error| error.within_index_native(1, None))?;
    Ok((left, right))
}
#[cfg(test)]
mod tests;
