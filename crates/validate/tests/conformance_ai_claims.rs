// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_ai_claims.py (#867)
//!
//! Migrated test:
//!   - `test_normative_fixture_validates_against_the_full_graph` — MERGED mode:
//!     the Python test calls `load_merged_graph(include_imports=False)`, adds the
//!     `ai-normative.ttl` fixture, then runs `run_shacl(g)`.  The Rust twin uses
//!     `validate_with_ontology` which combines `base_ontology_nt()` with the
//!     fixture triples, identical semantics.
//!
//! Retained in Python (not migrated):
//!   - All tombstone absence tests (`test_no_parallel_claim_construct_exists`,
//!     etc.) — these iterate `load_merged_graph` checking `(s, None, None) not in g`
//!     patterns; no `run_shacl` call.
//!   - Seam membership tests (`test_memory_is_a_role_on_the_universal_claim_construct`,
//!     etc.) — vocabulary structural checks, no `run_shacl` call.
//!   - `test_assessment_seam_is_the_norms_extensions` — parses the fixture and
//!     checks specific triples; no `run_shacl` call.

mod conformance_support;
use conformance_support::*;

/// `test_normative_fixture_validates_against_the_full_graph` — evaluation-as-Assessment
/// and hallucination-as-hazard are pure instance data over the norms and risk
/// extensions — zero new TBox, SHACL-clean.
///
/// Mode: MERGED — fixture is validated together with the full merged ontology
/// because the instance data references class/property declarations from the
/// norms and risk slices.
#[test]
fn normative_fixture_validates_against_the_full_graph() {
    let nt = fixture_as_nt("coverage", "ai-normative");
    let report = validate_with_ontology(&nt);
    assert!(ok(&report), "violations: {:?}", violations(&report));
}
