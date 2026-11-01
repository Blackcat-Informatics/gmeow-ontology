// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
pub(in super::super) fn assert_roundtrip(product: &crate::node::StageProduct) {
    let bytes = product
        .artifact(CHANNEL)
        .expect("authenticated correspondence reconstruction observation");
    let result: Outcome =
        purrdf_lex::json::record::from_slice(bytes).expect("complete typed observation");
    result
        .0
        .expect("native projection reconstructs the complete authored correspondence program");
}
