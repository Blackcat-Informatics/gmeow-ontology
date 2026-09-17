// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only helpers for the owning production module.

use super::*;

#[cfg(test)]
pub(crate) fn diagnostic_observations() -> &'static diagnostics::Observations {
    static OBSERVATIONS: std::sync::OnceLock<diagnostics::Observations> =
        std::sync::OnceLock::new();
    OBSERVATIONS.get_or_init(|| {
        let bytes = crate::fixture::authenticated_artifact(
            &gmeow_conformance::paths::repo_root(),
            "stage-conformance",
            diagnostics::CHANNEL,
        )
        .expect("authenticated diagnostic observations");
        serde_json::from_slice(&bytes).expect("typed diagnostic observations")
    })
}
