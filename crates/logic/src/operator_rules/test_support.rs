// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Test-only support retained outside the production source inventory.

use super::*;

/// Unit controls consume the producer's exact native law export, never source RDF.
pub(crate) fn fixture() -> &'static PreparedOperatorRules {
    struct Selected {
        selector: String,
        rules: PreparedOperatorRules,
    }
    static OBSERVED: OnceLock<gmeow_errors::Result<Selected>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("operator controls require the exact producer selector");
    let selected = OBSERVED
        .get_or_init(|| {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                PREPARED_OPERATOR_CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let rules = PreparedOperatorRules::from_bytes(&bytes)?;
            Ok(Selected {
                selector: selector.clone(),
                rules,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated native operator rules: {error}"));
    assert_eq!(
        selected.selector, selector,
        "operator rules cannot cross selector identities"
    );
    &selected.rules
}
