// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_registers.py (whole file; the
//! Python file is deleted).
//!
//! The two fixture-only SHACL cases were migrated in a prior batch. This PR
//! ports the remaining retained tests:
//!   - `test_no_primary_persona_machinery` → `no_primary_persona_machinery`
//!     (GraphStore::ontology() subject sweep for banned persona/register selectors).
//!   - `test_divergence_query_surfaces_legal_divergence` →
//!     `divergence_query_surfaces_legal_divergence` (injects a private-only norm,
//!     validates SHACL still conforms, and runs the competency divergence query
//!     via the native SPARQL engine).
//!
//! The remaining `_graph()` TBox-membership checks were already migrated to
//! slices/core/names/tests/structural.ttl and the norms slicetest cells.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use std::collections::HashSet;
use std::fs;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX: &str = "https://example.org/shapes/";
const REGISTERS_FIXTURE: &str = "tests/fixtures/shapes/registers-wellformed.ttl";
const DIVERGENCE_QUERY: &str = "queries/competency/registers-norm-divergence.rq";

#[batch_cases]
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

/// `test_no_primary_persona_machinery` (Principle 9): no `primary*`/`preferred*`
/// persona or register selector term is declared anywhere in the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn no_primary_persona_machinery() {
    let store = GraphStore::ontology();
    let (_, rows) = store.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let banned = [
        "primarypersona",
        "preferredpersona",
        "primaryregister",
        "preferredregister",
    ];
    let mut offenders: Vec<String> = Vec::new();
    for row in rows {
        let Some(Some(purrdf::TermValue::Iri(iri))) = row.into_iter().next() else {
            continue;
        };
        let Some(local) = iri.strip_prefix(GMEOW) else {
            continue;
        };
        if local.contains('/') {
            continue;
        }
        let lower = local.to_lowercase();
        if banned.iter().any(|b| lower.starts_with(b)) {
            offenders.push(iri);
        }
    }
    assert!(
        offenders.is_empty(),
        "primary/preferred persona/register selectors must not exist: {offenders:?}"
    );
}

/// `test_divergence_query_surfaces_legal_divergence`: adding a private-only norm
/// leaves the graph SHACL-conformant (divergence is legal, Principle 9) and the
/// competency divergence query reports the private persona's extra norm — but not
/// a spurious row for the public persona.
#[gmeow_test_batch_macros::batch_test]
fn divergence_query_surfaces_legal_divergence() {
    let fixture_ttl = fs::read_to_string(repo_root().join(REGISTERS_FIXTURE))
        .expect("registers-wellformed fixture");
    let injected = format!(
        "@prefix ex:    <{EX}> .\n\
         @prefix gmeow: <{GMEOW}> .\n\
         ex:playNorm a gmeow:Norm ;\n\
             gmeow:deonticModality gmeow:deonticRecommendation ;\n\
             gmeow:normIssuer ex:issuer .\n\
         ex:privatePersona gmeow:expressesNorm ex:playNorm .\n"
    );
    let combined = format!("{fixture_ttl}\n{injected}");

    // SHACL still conforms — divergence is not a violation.
    let report = validate(&ttl_str_to_nt(&combined));
    assert!(
        ok(&report),
        "divergent-but-legal graph should conform; violations: {:?}",
        violations(&report)
    );

    // The competency query reports the private persona's extra norm.
    let store = GraphStore::parse_ttl(&combined);
    let query = fs::read_to_string(repo_root().join(DIVERGENCE_QUERY)).expect("divergence query");
    let (vars, rows) = store.select(&[], &query);
    let p_idx = vars
        .iter()
        .position(|v| v == "persona")
        .expect("?persona projected");
    let n_idx = vars
        .iter()
        .position(|v| v == "norm")
        .expect("?norm projected");

    let mut diverged: HashSet<(String, String)> = HashSet::new();
    for row in rows {
        if let (Some(purrdf::TermValue::Iri(persona)), Some(purrdf::TermValue::Iri(norm))) =
            (&row[p_idx], &row[n_idx])
        {
            diverged.insert((persona.clone(), norm.clone()));
        }
    }

    let play_norm = format!("{EX}playNorm");
    assert!(
        diverged.contains(&(format!("{EX}privatePersona"), play_norm.clone())),
        "private persona's playNorm divergence should be reported; got {diverged:?}"
    );
    assert!(
        !diverged.contains(&(format!("{EX}publicPersona"), play_norm)),
        "public persona must not spuriously diverge on playNorm; got {diverged:?}"
    );
}
