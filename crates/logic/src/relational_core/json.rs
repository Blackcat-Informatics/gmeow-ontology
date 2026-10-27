// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete prepared violation evidence, without lowering or recomputation.
use super::ViolationLowering;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ViolationLowering {
    pub(crate) fn json_value(&self) -> gmeow_errors::Result<Value> {
        Ok(Object::new()
            .with(
                "rules",
                Value::Array(
                    self.rules
                        .iter()
                        .enumerate()
                        .map(|(index, rule)| {
                            rule.json_value()
                                .map_err(|error| error.with_context(format!("rules[{index}]")))
                        })
                        .collect::<gmeow_errors::Result<Vec<_>>>()?,
                ),
            )
            .with("residue", self.residue.to_json())
            .with("failure_classes", self.failure_classes.to_json())
            .into())
    }
}
impl FromJson for ViolationLowering {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ViolationLowering")?;
        Ok(Self {
            rules: fields.required("rules")?,
            residue: fields.required("residue")?,
            failure_classes: fields.required("failure_classes")?,
        })
    }
}
