// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
pub(in super::super) fn assert_roundtrip(product: &crate::node::StageProduct) {
    let bytes = product
        .artifact(CHANNEL)
        .expect("authenticated correspondence reconstruction observation");
    let result: Result<(), gmeow_errors::RecordedDiag> =
        serde_json::from_slice(bytes).expect("complete typed observation");
    result.expect("native projection reconstructs the complete authored correspondence program");
}
