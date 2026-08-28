// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_evidence.py
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
//! Fixture-based twins (whole file migrated here; the Python file is deleted).
//! These load `tests/fixtures/evidence/cogneto-cases.ttl` and inspect per-node
//! SHACL results via the `focus_node`-scoped `has_message_for_node` helper — the
//! native twin of the Python `_has_message_for_node`:
//!   - `test_infoworld_citation_passes` → `infoworld_citation_passes`.
//!   - `test_orgbook_citation_passes` → `orgbook_citation_passes`.
//!   - `test_private_contract_triggers_self_private_warning` →
//!     `private_contract_triggers_self_private_warning`.
//!   - `test_orgbook_notability_mutation_triggers_violation` →
//!     `orgbook_notability_mutation_triggers_violation` (mutates the fixture at the
//!     N-Triples level: flips OrgBook's `supportsNotability` false→true).

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::shapes::report::{Severity, ValidationReport};

const EVIDENCE_FIXTURE: &str = "tests/fixtures/evidence/cogneto-cases.ttl";
const EX_EVID: &str = "https://example.org/test/evidence/";

/// Native twin of Python's `_has_message_for_node`: true when some result at the
/// given severity targets `node_iri` and its message contains `substring`.
fn has_message_for_node(
    report: &ValidationReport,
    node_iri: &str,
    substring: &str,
    severity: Severity,
) -> bool {
    report.results.iter().any(|r| {
        r.severity == severity
            && r.focus_node.to_string().contains(node_iri)
            && r.message.as_deref().unwrap_or_default().contains(substring)
    })
}

fn evidence_fixture_report() -> ValidationReport {
    let nt = ttl_file_to_nt(&repo_root().join(EVIDENCE_FIXTURE));
    validate(&nt)
}

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

#[batch_cases]
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

// ── Fixture-based twins (cogneto-cases.ttl) ───────────────────────────────────

/// `test_infoworld_citation_passes`: InfoWorld = independent secondary significant
/// coverage → supports notability; conforms and gets no self/private warning.
#[gmeow_test_batch_macros::batch_test]
fn infoworld_citation_passes() {
    let report = evidence_fixture_report();
    assert!(
        ok(&report),
        "fixture should conform; violations: {:?}",
        violations(&report)
    );
    assert!(
        !has_message_for_node(
            &report,
            &format!("{EX_EVID}InfoWorldCognetoCitation"),
            "self-asserted or private evidence",
            Severity::Warning,
        ),
        "InfoWorld should not trigger the self/private-only warning"
    );
}

/// `test_orgbook_citation_passes`: OrgBook = official primary routine filing →
/// factual verification only; conforms and gets no self/private warning.
#[gmeow_test_batch_macros::batch_test]
fn orgbook_citation_passes() {
    let report = evidence_fixture_report();
    assert!(
        ok(&report),
        "fixture should conform; violations: {:?}",
        violations(&report)
    );
    assert!(
        !has_message_for_node(
            &report,
            &format!("{EX_EVID}OrgBookCognetoCitation"),
            "self-asserted or private evidence",
            Severity::Warning,
        ),
        "OrgBook should not trigger the self/private-only warning"
    );
}

/// `test_private_contract_triggers_self_private_warning`: a self-originated private
/// scan → Warning (Principle 10). Warning-only graphs still conform.
#[gmeow_test_batch_macros::batch_test]
fn private_contract_triggers_self_private_warning() {
    let report = evidence_fixture_report();
    assert!(
        ok(&report),
        "warning-only fixture should conform; violations: {:?}",
        violations(&report)
    );
    assert!(
        has_message_for_node(
            &report,
            &format!("{EX_EVID}PrivateCognetoContractCitation"),
            "self-asserted or private evidence",
            Severity::Warning,
        ),
        "Private contract should trigger the self/private-only warning"
    );
}

/// `test_orgbook_notability_mutation_triggers_violation`: flip OrgBook's
/// `supportsNotability` false→true → a WP:GNG triad Violation (primary ≠ secondary).
#[gmeow_test_batch_macros::batch_test]
fn orgbook_notability_mutation_triggers_violation() {
    let nt = ttl_file_to_nt(&repo_root().join(EVIDENCE_FIXTURE));
    // Mutate only the OrgBook citation's supportsNotability literal at the NT level
    // (the Private citation carries an identical `false` triple — leave it alone).
    let mutated: String = nt
        .lines()
        .map(|line| {
            if line.contains("OrgBookCognetoCitation") && line.contains("supportsNotability") {
                line.replace("\"false\"", "\"true\"")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_ne!(
        nt, mutated,
        "the OrgBook supportsNotability mutation must change the graph"
    );

    let report = validate(&mutated);
    assert!(
        !ok(&report),
        "OrgBook with supportsNotability true should fail SHACL"
    );
    assert!(
        has_message_for_node(
            &report,
            &format!("{EX_EVID}OrgBookCognetoCitation"),
            "WP:GNG triad",
            Severity::Violation,
        ),
        "expected a WP:GNG triad violation for OrgBook; violations: {:?}",
        violations(&report)
    );
}
