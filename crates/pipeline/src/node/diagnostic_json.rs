// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact stage contribution tuple, distinct from the public bundle node vector.

use gmeow_errors::DiagNode;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Within},
};

struct Contribution {
    stage: String,
    nodes: Vec<DiagNode>,
}

impl FromJson for Contribution {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let items = value
            .as_array()
            .ok_or_else(|| DecodeError::invalid_type(value, "stage contribution tuple"))?;
        let [stage, nodes] = items.as_slice() else {
            return Err(DecodeError::invalid_value(
                value,
                "exactly two stage contribution items",
            ));
        };
        Ok(Self {
            stage: String::from_json(stage).map_err(|error| error.within("0"))?,
            nodes: Vec::<DiagNode>::from_json(nodes).map_err(|error| error.within("1"))?,
        })
    }
}

pub(super) fn decode_contribution(bytes: &[u8]) -> Result<(String, Vec<DiagNode>), DecodeError> {
    purrdf_lex::json::record::from_slice::<Contribution>(bytes)
        .map(|value| (value.stage, value.nodes))
}

#[cfg(test)]
mod tests;
