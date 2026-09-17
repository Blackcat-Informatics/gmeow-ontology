// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer observation for the complete correspondence program's native inverse.
//! The consumer separately compares the retained program and terminal bytes to
//! its independent synthetic expectation. Reconstructing the authored projection
//! belongs here, where the producer already owns the native dataset.

use std::collections::BTreeMap;

use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, parse_correspondence,
};
use purrdf::RdfDataset;

const CHANNEL: &str = "pipeline/correspondence-roundtrip.json";

fn observe(authored: &CorrespondenceProgram, projection: &RdfDataset) -> gmeow_errors::Result<()> {
    let reconstructed = parse_correspondence(projection)?;
    if reconstructed != *authored {
        return Err(super::stage_err(
            "correspondence projection reconstruction differs from the complete authored program",
        ));
    }
    Ok(())
}

pub(super) fn record(
    authored: &CorrespondenceProgram,
    projection: &RdfDataset,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&super::observed(observe(authored, projection)))
            .map_err(gmeow_errors::Diag::from)?,
    );
    Ok(())
}

#[path = "correspondence_roundtrip.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "correspondence_roundtrip_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(super) use test_support::assert_roundtrip;
