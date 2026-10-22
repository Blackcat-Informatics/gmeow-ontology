// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rendered RL rows are transported verbatim, without interpreting RDF strings.
use super::RlTriple;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};
impl ToJson for RlTriple {
    fn to_json(&self) -> Value {
        Object::new()
            .with("subject", self.subject.to_json())
            .with("predicate", self.predicate.to_json())
            .with("object", self.object.to_json())
            .with("world", self.world.to_json())
            .with("is_edb", self.is_edb)
            .into()
    }
}
impl FromJson for RlTriple {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct RlTriple")?;
        Ok(Self {
            subject: fields.required("subject")?,
            predicate: fields.required("predicate")?,
            object: fields.required("object")?,
            world: fields.required("world")?,
            is_edb: fields.required("is_edb")?,
        })
    }
}
#[cfg(test)]
mod tests;
