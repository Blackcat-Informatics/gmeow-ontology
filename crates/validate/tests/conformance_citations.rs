// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_citations.py (#867)
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! Retained in Python (not migrated):
//!   - `test_self_description_loader`: calls `load_self_description()` — not a
//!     SHACL conformance check; pure Python business-logic assertion.
//!   - `test_self_description_models_project_repository_and_brand_assets`: parses
//!     a disk file and uses `in g` triple-membership checks; not a SHACL run.
//!   - `test_canonical_description_is_standardized`: cross-format sweep (YAML,
//!     ontology header, self-desc) with SPARQL-like object iteration; no SHACL.

mod conformance_support;
use conformance_support::*;

/// Turtle prefix block shared by all citation tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";

// ── Tests migrated from tests/test_citations.py ───────────────────────────────

/// `test_citation_act_shacl_passes` — a well-formed CitationAct relator passes SHACL.
#[test]
fn citation_act_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:citation a gmeow:CitationAct .
ex:citation gmeow:citingEntity ex:claim .
ex:citation gmeow:citedEntity ex:work .
ex:citation gmeow:citationIntent gmeow:intentCitesAsDataSource .
ex:claim a gmeow:Entity .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed CitationAct must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_citation_act_missing_intent_fails_shacl` — a CitationAct without
/// citationIntent violates SHACL.
#[test]
fn citation_act_missing_intent_fails_shacl() {
    let ttl = format!(
        "{PREFIXES}\
ex:citation a gmeow:CitationAct .
ex:citation gmeow:citingEntity ex:claim .
ex:citation gmeow:citedEntity ex:work .
ex:claim a gmeow:Entity .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        !ok(&report),
        "CitationAct missing citationIntent must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(
        msgs.iter()
            .any(|e| e.to_lowercase().contains("citation intent")),
        "violation message must mention 'citation intent'; got: {msgs:?}"
    );
}

/// `test_contribution_with_degree_shacl_passes` — a Contribution with an
/// optional degree passes SHACL.
#[test]
fn contribution_with_degree_shacl_passes() {
    let ttl = format!(
        "{PREFIXES}\
ex:contribution a gmeow:Contribution .
ex:contribution gmeow:contributor ex:alice .
ex:contribution gmeow:contributionTarget ex:work .
ex:contribution gmeow:contributionRole gmeow:roleAuthor .
ex:contribution gmeow:contributionDegree gmeow:degreeLead .
ex:alice a gmeow:Agent .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "Contribution with optional degree must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
