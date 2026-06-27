// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_evidence.py (#867)
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)` / `_make_citation_act(...)`, converts
//! to N-Triples, and validates against the whole shapes corpus.
//!
//! `_make_citation_act(g, uri)` expanded inline for every call site:
//!
//! ```text
//!   <uri>  a gmeow:CitationAct ;
//!           gmeow:citingEntity  ex:claim ;
//!           gmeow:citedEntity   ex:sourceWork ;
//!           gmeow:citationIntent gmeow:intentCitesAsDataSource .
//! ```
//!
//! Retained in Python (not migrated):
//!   - `test_infoworld_citation_passes`: loads cogneto-cases.ttl from disk and
//!     inspects per-node SHACL results using `_has_message_for_node`.
//!   - `test_orgbook_citation_passes`: same fixture file load.
//!   - `test_private_contract_triggers_self_private_warning`: same fixture file
//!     load with per-node warning check.
//!   - `test_orgbook_notability_mutation_triggers_violation`: loads fixture then
//!     dynamically mutates the graph (remove+add triples).

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

/// Turtle prefix block shared by all evidence tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/evidence/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// Inline expansion of `_make_citation_act(g, uri)`.
///
/// Emits the 4 triples the Python helper adds.
fn citation_act_ttl(uri: &str) -> String {
    format!(
        "\
{uri} a gmeow:CitationAct .
{uri} gmeow:citingEntity  ex:claim .
{uri} gmeow:citedEntity   ex:sourceWork .
{uri} gmeow:citationIntent gmeow:intentCitesAsDataSource .
"
    )
}

// ── Tests migrated from tests/test_evidence.py ────────────────────────────────

#[rstest]
#[case::self_private_evidence_triggers_warning(
    Case::inline(format!(
        "{PREFIXES}\
{citation_act}\
ex:citation gmeow:hasEvidenceClass    gmeow:evidenceSELF .
ex:citation gmeow:sourceIndependence  gmeow:sourceIndependenceSelfOrIssuerOriginated .
",
        citation_act = citation_act_ttl("ex:citation"),
    ))
    .warnings(&["self-asserted or private evidence"])
)]
#[case::mixed_evidence_does_not_trigger_self_private_warning(
    Case::inline(format!(
        "{PREFIXES}\
{citation_act}\
ex:citation gmeow:hasEvidenceClass    gmeow:evidenceSELF .
ex:citation gmeow:hasEvidenceClass    gmeow:evidenceIndependentTradePress .
ex:citation gmeow:sourceIndependence  gmeow:sourceIndependenceSelfOrIssuerOriginated .
",
        citation_act = citation_act_ttl("ex:citation"),
    ))
    .no_warning("self-asserted or private evidence")
)]
#[case::notability_without_triad_triggers_violation(
    Case::inline(format!(
        "{PREFIXES}\
{citation_act}\
ex:citation gmeow:supportsNotability  \"true\"^^xsd:boolean .
ex:citation gmeow:sourceIndependence  gmeow:sourceIndependenceIndependent .
",
        citation_act = citation_act_ttl("ex:citation"),
    ))
    .fails()
    .violations(&["WP:GNG triad"])
)]
#[case::notability_with_full_triad_passes(
    Case::inline(format!(
        "{PREFIXES}\
{citation_act}\
ex:citation gmeow:supportsNotability  \"true\"^^xsd:boolean .
ex:citation gmeow:sourceIndependence  gmeow:sourceIndependenceIndependent .
ex:citation gmeow:sourceTier          gmeow:sourceTierSecondary .
ex:citation gmeow:coverageDepth       gmeow:coverageDepthSignificantCoverage .
",
        citation_act = citation_act_ttl("ex:citation"),
    ))
)]
#[case::notability_false_does_not_require_triad(
    Case::inline(format!(
        "{PREFIXES}\
{citation_act}\
ex:citation gmeow:supportsNotability  \"false\"^^xsd:boolean .
",
        citation_act = citation_act_ttl("ex:citation"),
    ))
)]
fn evidence(#[case] case: Case) {
    case.run();
}
