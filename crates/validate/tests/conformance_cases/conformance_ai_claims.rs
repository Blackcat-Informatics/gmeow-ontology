// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology SHACL conformance twin migrated from tests/test_ai_claims.py.
//!
//! Migrated test:
//!   - `test_normative_fixture_validates_against_the_full_graph` — MERGED mode:
//!     the Python test calls `load_merged_graph(include_imports=False)`, adds the
//!     `ai-normative.ttl` fixture, then runs `run_shacl(g)`.  The Rust twin uses
//!     `validate_with_ontology` which combines `base_ontology_nt()` with the
//!     fixture triples, identical semantics.
//!
//! This case unions the fixture with the WHOLE merged ontology and validates the
//! entire graph against the whole shape corpus, so it rides the H8 budget cliff
//! and is carved out of the per-commit profile in `.config/nextest.toml` (it runs
//! on `maint-rust-heavy`). The cheap tombstone / seam TBox guards that were also
//! migrated from tests/test_ai_claims.py live in the sibling
//! `conformance_ai_claims_tbox` group so they stay on the per-commit gate.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

#[batch_cases]
#[case::normative_fixture_validates_against_the_full_graph(
    Case::file("coverage", "ai-normative")
        .with_ontology()
)]
fn ai_claims(#[case] case: Case) {
    case.run();
}
