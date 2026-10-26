// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Observe the shipped fragment registry from the producer's parsed source.

use std::collections::BTreeMap;

use purrdf::RdfDataset;
use purrdf_lex::json::{
    Value,
    record::{self, DecodeError, FromJson, ToJson},
};

pub(super) use gmeow_logic::reason::NativeFragmentRegistry as RegistrySurface;

pub(crate) const CHANNEL: &str = "pipeline/native-fragment-registry.json";

/// Complete producer observation, including an original-source refusal.
pub(super) struct Observation(pub Result<RegistrySurface, gmeow_errors::RecordedDiag>);

impl ToJson for Observation {
    fn to_json(&self) -> Value {
        gmeow_errors::json::recorded_result_to_json(&self.0)
    }
}

impl FromJson for Observation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        gmeow_errors::json::read_recorded_result(
            value,
            "native fragment registry observation",
            "one native fragment registry outcome",
        )
        .map(Self)
    }
}

pub(super) fn record(
    dataset: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    artifacts.insert(
        CHANNEL.to_owned(),
        record::to_vec(&Observation(
            observe(dataset).map_err(super::record_failure),
        )),
    );
    Ok(())
}

fn observe(dataset: &RdfDataset) -> Result<RegistrySurface, gmeow_errors::Diag> {
    RegistrySurface::observe(dataset)
}

#[path = "registry_surface.tests.rs"]
#[cfg(test)]
mod tests;
