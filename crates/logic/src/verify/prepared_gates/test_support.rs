// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only hydration of authenticated producer-selected verification laws.

use super::*;

pub(crate) fn shared() -> &'static PreparedReasonedGates {
    fixture()
}

fn fixture() -> &'static PreparedReasonedGates {
    struct Selected {
        selector: String,
        gates: PreparedReasonedGates,
    }
    static SELECTED: OnceLock<gmeow_errors::Result<Selected>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("native authored law tests require the producer-selected fixture identity");
    let selected = SELECTED
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                PREPARED_GATES_CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let gates: PreparedReasonedGates =
                serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            gates.validate_source_identity()?;
            Ok(Selected {
                selector: selector.clone(),
                gates,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated native gate laws: {error}"));
    assert_eq!(
        selected.selector, selector,
        "native gate preparation cannot cross selector identities"
    );
    &selected.gates
}
