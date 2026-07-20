// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Non-vacuity gate for the functional-characteristic carrier completeness invariant (issue 1579).
//!
//! `gmeow_logic_compile::frontend::functional_carrier_integrity` is the migration-surviving
//! successor to the (now vacuous) `functional_properties_missing_logic_carrier` check. These tests
//! prove it against the REAL merged authored corpus — the exact `graph/logic-compile-inputs` entity
//! the `stage-compile-logic` hard-fail path consumes:
//!
//! * the committed corpus PASSES — the frozen 718-entry ledger matches the live carrier-bearing set
//!   and there are zero orphan / duplicate / re-introduction violations; and
//! * dropping a single property's carrier from that corpus HARD-FAILS the completeness check,
//!   NAMING the dropped property — proving the ledger is non-vacuous (a silently-lost carrier can
//!   never slip past the gate).
//!
//! Re-blessing the frozen ledger is a deliberate human act (see
//! `crates/logic-compile/src/frontend/functional_carrier_ledger.txt`), never an auto-update.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::frontend::{FunctionalCarrierViolation, functional_carrier_integrity};
use gmeow_pipeline::stages::carrier::GRAPH_LOGIC_COMPILE_INPUTS;
use gmeow_pipeline::stages::source_load::{load_authored_dataset, logic_compile_input_subgraph};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfTerm};

const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";
/// A property known to be in the frozen ledger (the first entry) — the carrier we drop.
const LEDGER_MEMBER: &str = "https://blackcatinformatics.ca/gmeow/acceptanceStatus";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The real `graph/logic-compile-inputs` corpus, flattened to the default graph exactly as the
/// `stage-compile-logic` gate reads it (see `compile_logic.rs`'s `project_named_graph`).
fn logic_compile_inputs() -> Arc<RdfDataset> {
    let root = repo_root();
    let base = load_authored_dataset(&root).expect("load the authored corpus");
    let narrowed = logic_compile_input_subgraph(base.as_ref()).expect("narrow the corpus");
    let graph = RdfTerm::Iri(GRAPH_LOGIC_COMPILE_INPUTS.to_string());
    let mut builder = RdfDatasetBuilder::new();
    for mut quad in narrowed.owned_quads() {
        quad.graph_name = Some(graph.clone());
        builder.push_owned_quad(&quad);
    }
    let dataset = builder.freeze().expect("freeze logic-compile-inputs graph");
    Arc::new(dataset.project_named_graph(GRAPH_LOGIC_COMPILE_INPUTS))
}

#[test]
fn functional_carrier_ledger_matches_corpus() {
    // The committed corpus PASSES every functional-carrier integrity check: the frozen ledger
    // equals the live carrier-bearing set, and there are no orphan / duplicate / re-introduction
    // violations. A failure here means either a carrier changed without a ledger re-bless, or a
    // carrier became orphaned/duplicated — both HARD FAILs on the sync path.
    let ds = logic_compile_inputs();
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.is_empty(),
        "the committed corpus must satisfy every functional-carrier integrity check; \
         re-bless crates/logic-compile/src/frontend/functional_carrier_ledger.txt ONLY if a \
         carrier add/drop is intended. Violations:\n{}",
        violations
            .iter()
            .map(|v| format!("  - {v}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn dropping_one_carrier_hard_fails_completeness_naming_the_property() {
    // Remove exactly the `logic:characterizes <LEDGER_MEMBER>` triple from the real corpus — the
    // carrier record survives but no longer names its property, so LEDGER_MEMBER drops out of the
    // live carrier-bearing set. The completeness check must then HARD-FAIL, naming LEDGER_MEMBER as
    // a silently-dropped carrier — the exact non-vacuity the migration required.
    let root = repo_root();
    let base = load_authored_dataset(&root).expect("load the authored corpus");
    let narrowed = logic_compile_input_subgraph(base.as_ref()).expect("narrow the corpus");
    let graph = RdfTerm::Iri(GRAPH_LOGIC_COMPILE_INPUTS.to_string());
    let mut builder = RdfDatasetBuilder::new();
    let mut dropped = 0usize;
    for mut quad in narrowed.owned_quads() {
        if quad.predicate == LOGIC_CHARACTERIZES
            && quad.object == RdfTerm::Iri(LEDGER_MEMBER.to_string())
        {
            dropped += 1;
            continue;
        }
        quad.graph_name = Some(graph.clone());
        builder.push_owned_quad(&quad);
    }
    assert_eq!(
        dropped, 1,
        "exactly one logic:characterizes triple for {LEDGER_MEMBER} should exist in the corpus"
    );
    let dataset = builder.freeze().expect("freeze");
    let ds = dataset.project_named_graph(GRAPH_LOGIC_COMPILE_INPUTS);

    let violations = functional_carrier_integrity(&ds);
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::LedgerMissing { property } if property == LEDGER_MEMBER
        )),
        "dropping {LEDGER_MEMBER}'s carrier must surface a LedgerMissing naming it: {violations:?}"
    );
    // The drop must be the ONLY violation — nothing else changed, so no spurious extra failures.
    assert_eq!(
        violations.len(),
        1,
        "the single dropped carrier is the only violation: {violations:?}"
    );
}
