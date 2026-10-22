// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Compact terminal metadata; native RDF spelling and selection stay upstream.

use super::{Node, Observations};
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson},
};

impl ToJson for Node {
    fn to_json(&self) -> Value {
        Object::new()
            .with("properties", self.properties.to_json())
            .into()
    }
}
impl FromJson for Node {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Node")?;
        Ok(Self {
            properties: fields.required("properties")?,
        })
    }
}
impl ToJson for Observations {
    fn to_json(&self) -> Value {
        Object::new()
            .with("nodes", self.nodes.to_json())
            .with("readiness_graphs", self.readiness_graphs.to_json())
            .into()
    }
}
impl FromJson for Observations {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observations")?;
        Ok(Self {
            nodes: fields.required("nodes")?,
            readiness_graphs: fields.required("readiness_graphs")?,
        })
    }
}

#[cfg(test)]
mod tests;
