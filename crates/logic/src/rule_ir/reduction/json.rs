// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Raw prepared reduction metadata; no lowering or scope validation.
use super::{Function, Reduction};
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for Function {
    fn to_json(&self) -> Value {
        match self {
            Self::Count => "Count",
            Self::Sum => "Sum",
            Self::Avg => "Avg",
            Self::Min => "Min",
            Self::Max => "Max",
        }
        .to_json()
    }
}
impl FromJson for Function {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        match gmeow_errors::json::read_unit_name(value, "Function")? {
            "Count" => Ok(Self::Count),
            "Sum" => Ok(Self::Sum),
            "Avg" => Ok(Self::Avg),
            "Min" => Ok(Self::Min),
            "Max" => Ok(Self::Max),
            name => Err(DecodeError::unknown_variant(
                name,
                &["Count", "Sum", "Avg", "Min", "Max"],
            )),
        }
    }
}

::purrdf_lex::json_record!(impl ToJson for Reduction {
    "function" => function,
    "input" => input,
    "result" => result,
    "groups" => groups,
});
impl FromJson for Reduction {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Reduction")?;
        Ok(Self {
            function: fields.required("function")?,
            input: fields.required("input")?,
            result: fields.required("result")?,
            groups: fields.required("groups")?,
        })
    }
}
