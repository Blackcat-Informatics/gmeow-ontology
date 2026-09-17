// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Observe the shipped fragment registry from the producer's parsed source.

use std::collections::BTreeMap;

use purrdf::RdfDataset;

pub(super) use gmeow_logic::reason::NativeFragmentRegistry as RegistrySurface;

pub(crate) const CHANNEL: &str = "pipeline/native-fragment-registry.json";

pub(super) fn record(
    dataset: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observe(dataset).map_err(super::record_failure))
            .map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(())
}

fn observe(dataset: &RdfDataset) -> Result<RegistrySurface, gmeow_errors::Diag> {
    RegistrySurface::observe(dataset)
}

#[path = "registry_surface.tests.rs"]
#[cfg(test)]
mod tests;
