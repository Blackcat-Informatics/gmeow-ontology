// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native selected consistency observations; unknown record fields remain open.

use super::Observation;
use gmeow_errors::RecordedDiag;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, Within},
};
use std::collections::BTreeMap;

/// Read every source-world count with strict unsigned tokens and key diagnostics.
fn world_counts(value: &Value) -> Result<BTreeMap<String, u64>, DecodeError> {
    let object = value
        .as_object()
        .ok_or_else(|| DecodeError::invalid_type(value, "a map"))?;
    let mut counts = BTreeMap::new();
    for (world, value) in object {
        let count = gmeow_errors::json::read_unsigned::<u64>(value, "u64")
            .map_err(|error| error.within(world.as_str()))?;
        counts.insert(world.clone(), count);
    }
    Ok(counts)
}

::purrdf_lex::json_record! { impl ToJson for Observation {
    "verdict" => verdict,
    "world_counts" => world_counts,
}}

impl FromJson for Observation {
    /// Hydrate the complete typed record without normalizing its fields.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct Observation")?;
        Ok(Self {
            verdict: fields.required("verdict")?,
            world_counts: fields.required_with("world_counts", world_counts)?,
        })
    }
}

struct RecordedObservation(Result<Observation, RecordedDiag>);
impl FromJson for RecordedObservation {
    /// Hydrate the complete typed record without normalizing its fields.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "consistency observation outcome",
            "one consistency observation outcome variant",
        )
        .map(Self)
    }
}

/// Hydrate complete keyed observations without normalizing their typed records.
///
/// # Errors
/// Refuses malformed records or outcomes, preserving their source-key pointers.
pub fn recorded_observations_from_bytes(
    bytes: &[u8],
) -> Result<BTreeMap<String, Result<Observation, RecordedDiag>>, DecodeError> {
    let outcomes: BTreeMap<String, RecordedObservation> =
        purrdf_lex::json::record::from_slice(bytes)?;
    Ok(outcomes
        .into_iter()
        .map(|(key, outcome)| (key, outcome.0))
        .collect())
}
