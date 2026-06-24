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

/// `test_wellformed_registers_fixture_conforms` — the well-formed registers
/// fixture must pass SHACL without any violations.
///
/// Covers: two co-equal personas with bearer, register IRIs, same-norm
/// expression, activation conditions, and a byte-perfect style guide with
/// `gmeow:contentDigest`.
#[test]
fn wellformed_registers_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "registers-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed registers fixture must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_registers_fixture_is_flagged` — the malformed registers
/// fixture must produce violations for each of the four documented defects:
///
/// 1. `ex:orphanPersona` — no `gmeow:personaBearer` (exactly-one violated).
/// 2. `ex:mutePersona` — `gmeow:personaRegister` is a literal, not an IRI
///    (at-least-one IRI-kind violated).
/// 3. `ex:aimlessGuide` — `gmeow:StyleGuide` with no `gmeow:styleGuideFor`.
/// 4. `ex:driftingGuide` — voice exemplar document missing `gmeow:contentDigest`.
#[test]
fn malformed_registers_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "registers-malformed");
    let report = validate(&nt);
    assert!(!ok(&report), "malformed registers fixture must fail SHACL");
    let errors = violations(&report).join("\n");
    assert!(
        errors.contains("exactly one gmeow:personaBearer"),
        "expected 'exactly one gmeow:personaBearer' in violations; got: {errors}"
    );
    assert!(
        errors.contains("at least one gmeow:personaRegister"),
        "expected 'at least one gmeow:personaRegister' in violations; got: {errors}"
    );
    assert!(
        errors.contains("a style guide for nothing is just a document"),
        "expected 'a style guide for nothing is just a document' in violations; got: {errors}"
    );
    assert!(
        errors.contains("gmeow:contentDigest"),
        "expected 'gmeow:contentDigest' in violations; got: {errors}"
    );
}
