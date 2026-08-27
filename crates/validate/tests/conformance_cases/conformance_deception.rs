// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_deception.py
//!
//! Each test builds an inline Turtle graph containing the triples that the
//! Python test assembled via `g.add(...)` / `_doxastic_claim(...)`, converts
//! to N-Triples, and validates against the whole shapes corpus.
//!
//! `_doxastic_claim(g, claim, agent, proposition, method=EX.method1)` expanded
//! inline for every call site:
//!
//! ```text
//!   <claim>      a gmeow:DoxasticStandpointClaim, gmeow:StandpointClaim ;
//!                gmeow:observationMethod <method> ;
//!                gmeow:claimOfBelief <claimState> .
//!   <claimState> a gmeow:DoxasticState ;
//!                gmeow:epistemicAgent <agent> ;
//!                gmeow:doxasticContent <proposition> ;
//!                gmeow:doxasticClaim <claim> .
//! ```
//!
//! where `<claimState>` is `str(claim) + "State"`.
//!
//! Source: tests/test_deception.py (whole file migrated here; the Python file is deleted).
//!   - `test_licensed_falsehood_not_a_lie` → the `licensed_falsehood_conforms` SHACL case
//!     (the `run_shacl` conform half) plus the `licensed_falsehood_vocab` GraphStore test
//!     (the two cross-slice `_graph()` vocabulary assertions).
//!   - `test_blame_deflection_example_uses_doxastic_standpoint_claims` →
//!     the `blame_deflection_example_doxastic_claims` GraphStore test (loads the example
//!     file and iterates held/projected standpoints).
//!
//! Migrated to declarative slicetest cells (already, before this PR), NOT re-migrated here:
//!   - `test_bullshit_modality_exists` → slices/core/standpoint/tests/structural.ttl.
//!   - `test_disinformation_boundary_query` → slices/core/deception/tests/competency.ttl.

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const G_CLAIM_VERIDICALITY: &str = "https://blackcatinformatics.ca/gmeow/ClaimVeridicality";
const G_VERIDICALITY_LICENSED_FALSEHOOD: &str =
    "https://blackcatinformatics.ca/gmeow/veridicalityLicensedFalsehood";
const G_NARRATIVE_REFERENCE_FRAME: &str =
    "https://blackcatinformatics.ca/gmeow/NarrativeReferenceFrame";

// ── Helpers for the inline Turtle snippets ────────────────────────────────────

/// Turtle prefix block shared by all deception tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
";

/// Inline expansion of `_doxastic_claim(g, claimA, agent1, propA, method1)`.
///
/// Emits the 7 triples the Python helper adds (explicit double-typing included).
fn doxastic_claim_ttl(claim: &str, state: &str, agent: &str, prop: &str, method: &str) -> String {
    format!(
        "\
{claim} a gmeow:DoxasticStandpointClaim .
{claim} a gmeow:StandpointClaim .
{claim} gmeow:observationMethod {method} .
{claim} gmeow:claimOfBelief {state} .
{state} a gmeow:DoxasticState .
{state} gmeow:epistemicAgent {agent} .
{state} gmeow:doxasticContent {prop} .
{state} gmeow:doxasticClaim {claim} .
"
    )
}

// ── Tests migrated from tests/test_deception.py ───────────────────────────────

#[batch_cases]
#[case::standpoint_divergence_coexists(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeDeception .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::deception_event_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeDeception .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
ex:cue1    a gmeow:Observation .
ex:cue1    gmeow:vantage          ex:analyst .
ex:cue1    gmeow:observationResult ex:result1 .
ex:cue1    gmeow:observedFeature  ex:event1 .
ex:event1  gmeow:deceptionCue     ex:cue1 .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::deception_cue_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeDeception .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
ex:cue1    a gmeow:Observation .
ex:cue1    gmeow:vantage          ex:analyst .
ex:cue1    gmeow:observationResult ex:result1 .
ex:cue1    gmeow:observedFeature  ex:event1 .
ex:event1  gmeow:deceptionCue     ex:cue1 .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::paltering_implicates_structure(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType  gmeow:eventTypePaltering .
ex:event1 gmeow:implicates ex:propositionPprime .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::self_deception_same_agent(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeSelfDeception .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
ex:partDeceiver a gmeow:Participation .
ex:partDeceiver gmeow:participationEvent       ex:event1 .
ex:partDeceiver gmeow:participationParticipant ex:agent1 .
ex:partDeceiver gmeow:participationRole        gmeow:roleDeceiver .
ex:partDeceived a gmeow:Participation .
ex:partDeceived gmeow:participationEvent       ex:event1 .
ex:partDeceived gmeow:participationParticipant ex:agent1 .
ex:partDeceived gmeow:participationRole        gmeow:roleDeceived .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::distortion_shacl_passes(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeDistortion .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1 a gmeow:ObservationMethod .
ex:partSpin a gmeow:Participation .
ex:partSpin gmeow:participationEvent       ex:event1 .
ex:partSpin gmeow:participationParticipant ex:spinDoctor .
ex:partSpin gmeow:participationRole        gmeow:roleSpinDoctor .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::fabrication_refuted_provenance(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeFabrication .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1     a gmeow:ObservationMethod .
ex:work1       a gmeow:CreativeWork .
ex:event1      gmeow:implicates ex:work1 .
ex:verification1 a gmeow:VerificationResult .
ex:verification1 gmeow:hasVerificationStatus gmeow:verificationStatusFailed .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::forgery_failed_signature_structure(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeForgery .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1    a gmeow:ObservationMethod .
ex:forgedWork  a gmeow:CreativeWork .
ex:genuineWork a gmeow:CreativeWork .
ex:forgedWork  gmeow:counterpartOf ex:genuineWork .
ex:event1      gmeow:implicates    ex:forgedWork .
ex:signature1  a gmeow:CryptographicSignature .
ex:signature1  gmeow:signatureOf             ex:forgedWork .
ex:signature1  gmeow:hasVerificationStatus   gmeow:verificationStatusFailed .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::impersonation_facet_subject_mismatch(Case::inline(format!(
    "{PREFIXES}\
ex:event1 a gmeow:Event .
ex:event1 gmeow:eventType gmeow:eventTypeImpersonation .
ex:agent1 a gmeow:Agent .
ex:propA  a gmeow:Proposition .
ex:propB  a gmeow:Proposition .
ex:event1 gmeow:heldStandpoint      ex:claimA .
ex:event1 gmeow:projectedStandpoint ex:claimB .
{claim_a}\
{claim_b}\
ex:method1    a gmeow:ObservationMethod .
ex:facet1     a gmeow:IdentityFacet .
ex:facet1     gmeow:facetSubject    ex:victim .
ex:facet1     gmeow:facetVantage    ex:victim .
ex:facet1     gmeow:observedFeature ex:event1 .
ex:authResult1 a gmeow:AuthenticationResult .
ex:authResult1 gmeow:authResult \"fail\" .
",
    claim_a = doxastic_claim_ttl(
        "ex:claimA",
        "ex:claimAState",
        "ex:agent1",
        "ex:propA",
        "ex:method1"
    ),
    claim_b = doxastic_claim_ttl(
        "ex:claimB",
        "ex:claimBState",
        "ex:agent1",
        "ex:propB",
        "ex:method1"
    ),
)))]
#[case::disinformation_propagation_chain(Case::inline(format!(
    "{PREFIXES}\
# --- Hop 0: Disinformation origin ---
ex:originEvent a gmeow:Event .
ex:originEvent gmeow:eventType gmeow:eventTypeDisinformation .
ex:deceiver    a gmeow:Agent .
ex:originHeldProp      a gmeow:Proposition .
ex:originProjectedProp a gmeow:Proposition .
ex:originEvent gmeow:heldStandpoint      ex:originHeld .
ex:originEvent gmeow:projectedStandpoint ex:originProjected .
{origin_held}\
{origin_projected}\
ex:originEvent gmeow:deceptiveIntentClaim ex:originIntent .
ex:originIntent a gmeow:StandpointClaim .
ex:originIntent gmeow:observationMethod ex:method1 .
ex:partOriginDeceiver a gmeow:Participation .
ex:partOriginDeceiver gmeow:participationEvent       ex:originEvent .
ex:partOriginDeceiver gmeow:participationParticipant ex:deceiver .
ex:partOriginDeceiver gmeow:participationRole        gmeow:roleDeceiver .
# --- Hop 1: Dupe resharing ---
ex:dupeEvent a gmeow:Event .
ex:dupeEvent gmeow:eventType gmeow:eventTypeDeception .
ex:dupe    a gmeow:Agent .
ex:dupeProp a gmeow:Proposition .
ex:dupeEvent gmeow:heldStandpoint      ex:dupeBelief .
ex:dupeEvent gmeow:projectedStandpoint ex:dupeBelief .
{dupe_belief}\
ex:dupeBelief gmeow:claimVeridicality gmeow:veridicalityUntrue .
ex:partDupe a gmeow:Participation .
ex:partDupe gmeow:participationEvent       ex:dupeEvent .
ex:partDupe gmeow:participationParticipant ex:dupe .
ex:partDupe gmeow:participationRole        gmeow:roleDupe .
# --- Hop 2: Downstream resharing ---
ex:downstreamEvent a gmeow:Event .
ex:downstreamEvent gmeow:eventType gmeow:eventTypeDeception .
ex:downstream    a gmeow:Agent .
ex:downstreamProp a gmeow:Proposition .
ex:downstreamEvent gmeow:heldStandpoint      ex:downstreamBelief .
ex:downstreamEvent gmeow:projectedStandpoint ex:downstreamBelief .
{downstream_belief}\
ex:downstreamBelief gmeow:claimVeridicality gmeow:veridicalityUntrue .
# Shared method
ex:method1 a gmeow:ObservationMethod .
",
    origin_held = doxastic_claim_ttl(
        "ex:originHeld",
        "ex:originHeldState",
        "ex:deceiver",
        "ex:originHeldProp",
        "ex:method1"
    ),
    origin_projected = doxastic_claim_ttl(
        "ex:originProjected",
        "ex:originProjectedState",
        "ex:deceiver",
        "ex:originProjectedProp",
        "ex:method1"
    ),
    dupe_belief = doxastic_claim_ttl(
        "ex:dupeBelief",
        "ex:dupeBeliefState",
        "ex:dupe",
        "ex:dupeProp",
        "ex:method1"
    ),
    downstream_belief = doxastic_claim_ttl(
        "ex:downstreamBelief",
        "ex:downstreamBeliefState",
        "ex:downstream",
        "ex:downstreamProp",
        "ex:method1"
    ),
)))]
// test_licensed_falsehood_not_a_lie (run_shacl half): a fiction claim under a
// NarrativeReferenceFrame conforms — the licensed-falsehood safety structure is well-formed.
#[case::licensed_falsehood_conforms(Case::inline(format!(
    "{PREFIXES}\
ex:fictionClaim a gmeow:StandpointClaim .
ex:fictionClaim gmeow:claimVeridicality gmeow:veridicalityLicensedFalsehood .
ex:fictionClaim gmeow:accordingTo ex:narrativeFrame .
ex:fictionClaim gmeow:observationMethod ex:method1 .
ex:method1 a gmeow:ObservationMethod .
ex:narrativeFrame a gmeow:NarrativeReferenceFrame .
ex:narrativeFrame gmeow:frameRealm gmeow:frameRealmNarrative .
ex:narrativeFrame gmeow:frameKind gmeow:frameKindNarrative .
")))]
fn deception(#[case] case: Case) {
    case.run();
}

// ── TBox / example-membership twins (GraphStore) ──────────────────────────────

/// `test_blame_deflection_example_uses_doxastic_standpoint_claims`: every held or
/// projected standpoint in the blame-deflection example is a `DoxasticStandpointClaim`.
///
/// Matches the Python test's subject-agnostic collection: it gathers the objects of
/// `heldStandpoint`/`projectedStandpoint` regardless of subject type, asserts at least
/// one exists, and asserts none fails to be a `DoxasticStandpointClaim`.
#[gmeow_test_batch_macros::batch_test]
fn blame_deflection_example_doxastic_claims() {
    let example = repo_root().join("slices/core/deception/examples/blame-deflection.ttl");
    let g = GraphStore::parse_ttl_file(&example);

    let any_standpoint = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         ASK { { ?s gmeow:heldStandpoint ?o } UNION { ?s gmeow:projectedStandpoint ?o } }"
        .to_string();
    assert!(
        g.ask(&[], &any_standpoint),
        "expected at least one held/projected standpoint in the example"
    );

    let non_doxastic = "PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
         ASK {
           { ?s gmeow:heldStandpoint ?o } UNION { ?s gmeow:projectedStandpoint ?o }
           FILTER NOT EXISTS { ?o a gmeow:DoxasticStandpointClaim }
         }"
    .to_string();
    assert!(
        !g.ask(&[], &non_doxastic),
        "a held/projected standpoint is not a DoxasticStandpointClaim"
    );
}

/// `test_licensed_falsehood_not_a_lie` (`_graph()` half): the licensed-falsehood
/// vocabulary terms exist in the merged ontology with their expected metatypes.
#[gmeow_test_batch_macros::batch_test]
fn licensed_falsehood_vocab() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(G_VERIDICALITY_LICENSED_FALSEHOOD),
            Some(RDF_TYPE),
            Some(G_CLAIM_VERIDICALITY)
        ),
        "veridicalityLicensedFalsehood is not a ClaimVeridicality"
    );
    assert!(
        g.has(
            Some(G_NARRATIVE_REFERENCE_FRAME),
            Some(RDF_TYPE),
            Some(OWL_CLASS)
        ),
        "NarrativeReferenceFrame is not an owl:Class"
    );
}
