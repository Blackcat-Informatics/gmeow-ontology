// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_registers.py (#867)
//!
//! Two tests from test_registers.py are reproduced here because they use
//! `run_shacl(g)` and rely solely on fixtures — no Python-side merged graph,
//! no SPARQL query, no dynamic subject sweep.
//!
//! Retained in Python (not migrated):
//!   - `test_register_spine_lives_in_names_core`: calls `_graph()` /
//!     `load_merged_graph` + TBox membership checks.
//!   - `test_persona_is_a_relator_with_one_bearer`: calls `_graph()`.
//!   - `test_expression_machinery_is_open_and_plural`: calls `_graph()`.
//!   - `test_style_guide_voice_doctrine`: calls `_graph()`.
//!   - `test_no_primary_persona_machinery`: calls `_graph()` + dynamic
//!     subject sweep (never freeze to a finite list).
//!   - `test_same_norms_invariant_holds_on_wellformed_fixture`: loads a
//!     SPARQL competency query from disk and asserts empty result rows.
//!   - `test_divergence_query_surfaces_legal_divergence`: hybrid test —
//!     `run_shacl` PLUS a SPARQL divergence query on a mutated graph;
//!     the SPARQL half is not portable to Rust without a query engine.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_registers_fixture_conforms(Case::file("shapes", "registers-wellformed"))]
#[case::malformed_registers_fixture_is_flagged(
    Case::file("shapes", "registers-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:personaBearer",
            "at least one gmeow:personaRegister",
            "a style guide for nothing is just a document",
            "gmeow:contentDigest",
        ])
)]
fn registers(#[case] case: Case) {
    case.run();
}
