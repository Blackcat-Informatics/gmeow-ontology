// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
pub(super) fn deep_semantic_findings_prepared(
    gts_bytes: &[u8],
    report: &mut Report,
    verification: &gmeow_logic::verify::PreparedVerification<'_>,
) -> gmeow_errors::Result<()> {
    let bundle = purrdf::import_gts_events(gts_bytes).map_err(|e| {
        Diag::of_kind(crate::error::Dataset {
            detail: format!("validate --deep: GTS read error: {e}"),
        })
    })?;
    deep_semantic_findings_dataset(gts_bytes, bundle.dataset.as_ref(), report, verification)
}
