// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

/// Parse an N-Triples fixture into one frozen dataset.
#[cfg(test)]
pub(super) fn dataset_from_nt(nt: &str) -> gmeow_errors::Result<Arc<RdfDataset>> {
    parse_dataset(nt.as_bytes(), "application/n-triples", None).ctx("parse n-triples")
}
