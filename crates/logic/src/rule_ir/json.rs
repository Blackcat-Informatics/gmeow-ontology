// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native prepared-rule transport; execution and lowering remain separate.
use super::{EvalAtom, EvalRule, EvalTerm};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within},
};
impl ToJson for EvalTerm {
    fn to_json(&self) -> Value {
        match self {
            Self::Var(value) => Object::new().with("Var", value.to_json()).into(),
            Self::ConstNamed(value) => Object::new().with("ConstNamed", value.to_json()).into(),
            Self::ConstLit(value) => Object::new()
                .with("ConstLit", crate::term_serde::to_json(value))
                .into(),
        }
    }
}
impl FromJson for EvalTerm {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (tag, payload) =
            gmeow_errors::json::read_variant(value, "EvalTerm", "one EvalTerm variant")?;
        let result = match tag {
            "Var" => String::from_json(payload).map(Self::Var),
            "ConstNamed" => String::from_json(payload).map(Self::ConstNamed),
            "ConstLit" => crate::term_serde::from_json(payload).map(Self::ConstLit),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Var", "ConstNamed", "ConstLit"],
            )),
        };
        result.map_err(|error| error.within(tag))
    }
}
::purrdf_lex::json_record!(impl ToJson for EvalAtom {
    "subject" => subject,
    "predicate" => predicate,
    "object" => object,
    "negated" => negated,
});
impl FromJson for EvalAtom {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let (subject, predicate, object, negated) =
            gmeow_logic_compile::relational_core::read_signed_atom(value, "struct EvalAtom")?;
        Ok(Self {
            subject,
            predicate,
            object,
            negated,
        })
    }
}

impl EvalRule {
    pub(crate) fn json_value(&self) -> gmeow_errors::Result<Value> {
        Ok(Object::new()
            .with("numeric", self.numeric.to_json())
            .with("reduction", self.reduction.to_json())
            .with("head", self.head.to_json())
            .with("body", self.body.to_json())
            .with("rule_iri", self.rule_iri.to_json())
            .with("distinct_pairs", self.distinct_pairs.to_json())
            .with(
                "builtins",
                Value::Array(
                    self.builtins
                        .iter()
                        .enumerate()
                        .map(|(index, builtin)| {
                            builtin.json_value().map_err(|error| {
                                error.with_context(format!(
                                    "rule {:?} builtins[{index}]",
                                    self.rule_iri
                                ))
                            })
                        })
                        .collect::<gmeow_errors::Result<Vec<_>>>()?,
                ),
            )
            .with("constraint_tag", self.constraint_tag.to_json())
            .into())
    }
}
impl FromJson for EvalRule {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct EvalRule")?;
        Ok(Self {
            numeric: fields.required("numeric")?,
            reduction: fields.optional("reduction")?,
            head: fields.required("head")?,
            body: fields.required("body")?,
            rule_iri: fields.required("rule_iri")?,
            distinct_pairs: fields.required_with("distinct_pairs", |value| {
                purrdf_lex::json::record::items_with(value, |row| -> Result<_, DecodeError> {
                    let [left, right] = gmeow_errors::json::read_string_row::<2>(row)?;
                    Ok((left, right))
                })
            })?,
            builtins: fields.required("builtins")?,
            constraint_tag: fields.optional("constraint_tag")?,
        })
    }
}
