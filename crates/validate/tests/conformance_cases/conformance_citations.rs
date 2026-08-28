// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_citations.py
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)`, converts to N-Triples, and validates
//! against the whole shapes corpus.
//!
//! Recreated natively (no longer Python) in `self_description_conformance.rs` —
//! these three checks are pure business-logic / cross-format assertions over the
//! authored self-model, not SHACL runs, so they ship there as native Rust gate
//! tests instead of as twins in this file:
//!   - `test_self_description_loader` → `self_description_loader_pins_fields`
//!     (loader field assertions via the public `load_self_description` API).
//!   - `test_self_description_models_project_repository_and_brand_assets` →
//!     `models_project_repository_and_brand_assets` (project / repository /
//!     license / brand-asset triple membership, incl. the negative
//!     `gmeow:depicts`-absent assertion).
//!   - `test_canonical_description_is_standardized` →
//!     `canonical_abstract_is_standardized` (one abstract, standardized across
//!     self-desc / ontology header / CITATION.cff, with the external-vocabulary
//!     count + no-hard-coded-slice-count prose guards).

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

/// Turtle prefix block shared by all citation tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";

// ── Tests migrated from tests/test_citations.py ───────────────────────────────

#[batch_cases]
#[case::citation_act_shacl_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:citation a gmeow:CitationAct .
ex:citation gmeow:citingEntity ex:claim .
ex:citation gmeow:citedEntity ex:work .
ex:citation gmeow:citationIntent gmeow:intentCitesAsDataSource .
ex:claim a gmeow:Entity .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    ))
)]
#[case::contribution_with_degree_shacl_passes(
    Case::inline(format!(
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
    ))
)]
// A CitationAct without citationIntent violates SHACL; the message check is
// case-insensitive (`violations_ci`), mirroring the original `.to_lowercase()`.
#[case::citation_act_missing_intent_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:citation a gmeow:CitationAct .
ex:citation gmeow:citingEntity ex:claim .
ex:citation gmeow:citedEntity ex:work .
ex:claim a gmeow:Entity .
ex:work a gmeow:Work .
ex:work rdfs:label \"Test Work\" .
"
    ))
    // The citationIntent exactly-one obligation migrated from the retired
    // gmeow:CitationActShape to an OWL restriction PROJECTED to the production shape union
    // (generated/shapes/validation-shapes.ttl as sh:minCount 1). The projected shape carries
    // no bespoke sh:message, so assert on the min-count component + path.
    .shape_union()
    .fails()
    .fails_on_path("https://blackcatinformatics.ca/gmeow/citationIntent", "MinCountConstraintComponent")
)]
fn citations(#[case] case: Case) {
    case.run();
}
