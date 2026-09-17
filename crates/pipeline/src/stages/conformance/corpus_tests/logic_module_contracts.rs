// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only GMEOW abduction-ownership and calculus vocabulary contracts.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use super::super::logic_module_contracts::{CHANNEL, Observations, SOURCE};

mod abduction;
mod grounding;

type Contract = (&'static str, fn());

fn observations() -> &'static Observations {
    struct Selected {
        selector: String,
        observed: Observations,
    }
    static OBSERVED: OnceLock<Result<Selected, gmeow_errors::Diag>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("logic vocabulary contracts require the exact producer selector");
    let selected = OBSERVED
        .get_or_init(|| {
            let root = gmeow_conformance::paths::repo_root();
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let observed = serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            Ok(Selected {
                selector: selector.clone(),
                observed,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated logic vocabulary observations: {error}"));
    assert_eq!(
        selected.selector, selector,
        "logic vocabulary cannot cross selector identities"
    );
    assert_eq!(selected.observed.source_path, SOURCE);
    assert_eq!(
        selected.observed.source_digest.len(),
        64,
        "exact authored source digest"
    );
    assert_eq!(
        selected.observed.source_iri,
        gmeow_logic::verify::GATE_SOURCES[1].1
    );
    &selected.observed
}

#[test]
fn logic_module_contracts_share_one_authenticated_source_action() {
    let contracts: [Contract; 4] = [
        (
            "abductive_vocabulary_parses_without_malformed_nodes",
            abduction::abductive_vocabulary_parses_without_malformed_nodes,
        ),
        (
            "completeness_formulas_reconstruct_but_never_become_axioms",
            abduction::completeness_formulas_reconstruct_but_never_become_axioms,
        ),
        (
            "calculus_vocabulary_is_backed_by_shipped_grounding_laws",
            grounding::calculus_vocabulary_is_backed_by_shipped_grounding_laws,
        ),
        (
            "ns_typing_marker_constants_match_shipped_laws",
            grounding::ns_typing_marker_constants_match_shipped_laws,
        ),
    ];
    assert_eq!(
        contracts
            .iter()
            .map(|(name, _)| *name)
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
    let mut failures = Vec::new();
    for (name, contract) in contracts {
        if let Err(payload) = std::panic::catch_unwind(contract) {
            let detail = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "non-string assertion panic".to_owned());
            failures.push(format!("{name}: {detail}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 4 logic module contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
