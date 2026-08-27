// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_norms.py
//!
//! Migrated: the two `run_shacl`-based fixture tests.
//!
//! Retained in Python (not migrated):
//!   - `test_graft_axioms_live_extension_side_only`: cross-slice file-load check
//!     that iterates subjects dynamically; no `run_shacl` call.
//!   - `test_graft_preserves_core_trio_classhood`: uses `_graph()` /
//!     `load_merged_graph` + `(triple) in g` membership assertions.
//!   - `test_competency_deontic_modalities_query`: external `.rq` file +
//!     SPARQL SELECT result-set check.
//!   - `test_competency_authority_order_query`: external `.rq` file +
//!     SPARQL SELECT result-set check.
//!
//! Both fixture tests use `validate` (fixture-only, no merged ontology), which
//! mirrors Python's `run_shacl` which validates the fixture N-Triples directly
//! against the shapes corpus without merging the base ontology.
use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::wellformed_norms_fixture_conforms(Case::file("shapes", "norms-wellformed"))]
#[case::malformed_norms_fixture_is_flagged(
    Case::file("shapes", "norms-malformed")
        .fails()
        .violations(&[
            "no ought, only ought-according-to",
            "its own gmeow:overrides",
            "at least two gmeow:groupMember",
            "exactly one gmeow:groupOperator",
            "binds exactly one of gmeow:parameterValue",
            "gmeow:precedenceHigher and gmeow:precedenceLower must be distinct",
            "must be scoped to exactly one gmeow:precedenceScope",
            "must be gmeow:deonticPermission",
            "exactly one gmeow:evaluationVerdict",
            "at most one gmeow:deonticModality",
        ])
)]
fn norms(#[case] case: Case) {
    case.run();
}
