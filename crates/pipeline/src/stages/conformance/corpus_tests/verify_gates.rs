// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Named read-only contracts over exact native reasoned-graph scene observations.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use gmeow_action_cache::bytes_digest;
use gmeow_errors::Report;
use gmeow_logic::verify::embedded_verify_queries;

use super::super::verify_gates::{CHANNEL, Observations, RepairObservation, SourceObservation};

mod dimension;
mod enactment;

type Contract = (&'static str, fn());

fn observations() -> &'static Observations {
    struct Selected {
        selector: String,
        observed: Observations,
    }
    static OBSERVED: OnceLock<Result<Selected, gmeow_errors::Diag>> = OnceLock::new();
    let selector = std::env::var(gmeow_action_cache::selection::MANIFEST_SHA256_ENV)
        .expect("verify contracts require the exact producer-selected source identity");
    let selected = OBSERVED
        .get_or_init(|| {
            let root = gmeow_conformance::paths::repo_root();
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &root,
                "stage-conformance",
                CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            let observed: Observations =
                serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)?;
            let queries: BTreeMap<_, _> = embedded_verify_queries()
                .into_iter()
                .map(|(name, text)| (name, bytes_digest(text.as_bytes())))
                .collect();
            if observed.query_digests != queries
                || observed.query_set_digest
                    != bytes_digest(
                        &serde_json::to_vec(&queries).map_err(gmeow_errors::Diag::from)?,
                    )
            {
                return Err(super::super::stage_err(
                    "native verify observations used a different selected query inventory",
                ));
            }
            if observed.sources.len() != 23 || observed.law_source_digests.len() != 2 {
                return Err(super::super::stage_err(
                    "native verify observation source inventory is incomplete",
                ));
            }
            Ok(Selected {
                selector: selector.clone(),
                observed,
            })
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated reasoned-graph observations: {error}"));
    assert_eq!(
        selected.selector, selector,
        "verify observations cannot cross selector identities"
    );
    &selected.observed
}

fn source(path: &str) -> &'static SourceObservation {
    let observed = observations()
        .sources
        .get(path)
        .unwrap_or_else(|| panic!("required verify source {path} is absent"));
    assert_eq!(observed.source_path, path);
    assert_eq!(observed.source_digest.len(), 64, "exact source SHA-256");
    observed
}

fn report(path: &str) -> &'static Report {
    &source(path).original
}

fn repair(path: &str, name: &str, removed: usize) -> &'static RepairObservation {
    let observed = source(path)
        .repairs
        .get(name)
        .unwrap_or_else(|| panic!("required native repair {name} is absent"));
    assert_eq!(
        observed.mutation_digest.len(),
        64,
        "exact native repair identity"
    );
    assert_eq!(
        observed.removed_rows, removed,
        "the repair must remove its exact original binding"
    );
    assert_eq!(
        observed.inserted_rows, 1,
        "the edit must actually change the fixture, or the green half proves nothing"
    );
    observed
}

#[test]
fn authored_reasoned_gate_contracts_share_one_authenticated_source_action() {
    let contracts: Vec<Contract> = dimension::contracts()
        .into_iter()
        .chain(enactment::contracts())
        .collect();
    assert_eq!(
        contracts.len(),
        28,
        "all four dimension and twenty-four enactment contracts remain"
    );
    let names: BTreeSet<_> = contracts.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names.len(),
        contracts.len(),
        "contract names must be unique"
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
        "{} of 28 native gate contracts failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
