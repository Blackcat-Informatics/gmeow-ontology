// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_deception.py (#867)
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
//! Retained in Python (not migrated):
//!   - `test_blame_deflection_example_uses_doxastic_standpoint_claims`: loads an
//!     example file from disk and iterates subjects dynamically.
//!   - `test_bullshit_modality_exists`: calls `_graph()` / `load_merged_graph`.
//!   - `test_licensed_falsehood_not_a_lie`: calls both `run_shacl` AND `_graph()`
//!     for cross-slice vocabulary assertions; the `_graph()` half is not portable.
//!   - `test_disinformation_boundary_query`: uses `load_merged_graph` + a `.rq`
//!     competency file and inspects SPARQL SELECT results.

mod conformance_support;
use conformance_support::*;

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

/// `test_standpoint_divergence_coexists` — Principle 9: held and projected
/// standpoints are coexisting claims, neither privileged.  The graph must
/// permit both on the same event.
#[test]
fn standpoint_divergence_coexists() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "standpoint divergence must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_deception_event_shacl_passes` — a fully-populated deception event
/// passes SHACL.
#[test]
fn deception_event_shacl_passes() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "fully-populated deception event must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_deception_cue_shacl_passes` — a deception cue observation passes SHACL.
#[test]
fn deception_cue_shacl_passes() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "deception cue observation must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_paltering_implicates_structure` — a paltering event can carry
/// `gmeow:implicates` to a proposition.
#[test]
fn paltering_implicates_structure() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "paltering event with implicates must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_self_deception_same_agent` — the same agent can bear both deceiver and
/// deceived roles on the same event via distinct Participation relators.
#[test]
fn self_deception_same_agent() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "self-deception (same agent, two roles) must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_distortion_shacl_passes` — a distortion event with spin-doctor role
/// passes SHACL.
#[test]
fn distortion_shacl_passes() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "distortion event with spin-doctor role must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_fabrication_refuted_provenance` — a fabrication event with refuted
/// provenance passes SHACL.
#[test]
fn fabrication_refuted_provenance() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "fabrication with refuted provenance must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_forgery_failed_signature_structure` — a forgery event with
/// `counterpartOf` + failed `CryptographicSignature` passes SHACL.
#[test]
fn forgery_failed_signature_structure() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "forgery with failed CryptographicSignature must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_impersonation_facet_subject_mismatch` — an impersonation event where
/// projected identity facet subject ≠ deceiver passes SHACL.
#[test]
fn impersonation_facet_subject_mismatch() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "impersonation with victim-subject facet must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_disinformation_propagation_chain` — a 3-hop disinformation chain
/// (origin + dupe + downstream) passes SHACL.
#[test]
fn disinformation_propagation_chain() {
    let ttl = format!(
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
    );
    let nt = ttl_str_to_nt(&ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "3-hop disinformation chain must pass SHACL; violations: {:?}",
        violations(&report)
    );
}
