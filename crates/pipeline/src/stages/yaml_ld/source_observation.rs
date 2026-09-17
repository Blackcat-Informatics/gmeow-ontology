// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned conversion of the authored standpoint example.

use std::collections::BTreeMap;
use std::path::Path;

pub(crate) const SOURCE: &str = "slices/core/standpoint/examples/claim-bullshit.yamlld";
pub(crate) const CHANNEL: &str = "pipeline/yaml-ld-statement-observation.purrpack";

pub(crate) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: String| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-conformance".to_owned(),
            message: error,
        })
    };
    let bytes = std::fs::read(root.join(SOURCE))
        .map_err(|error| fail(format!("read {SOURCE}: {error}")))?;
    let nquads = super::yaml_ld_star_to_gmeow_statement_metadata_nquads(&bytes)?;
    let native = super::dataset_from_nquads(nquads.as_bytes())?;
    let pack =
        purrdf::PackBuilder::build_bytes(&native).map_err(|error| fail(error.to_string()))?;
    artifacts.insert(CHANNEL.to_owned(), pack);
    Ok(())
}
