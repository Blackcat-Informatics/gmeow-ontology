// SPDX-License-Identifier: AGPL-3.0-only
//! Attestation reconciliation assertions over producer-recorded findings.
//! The exact source-shape and focus-node checks consume an authenticated case result.

const MODULE: &str = "slices/core/attestation/module.ttl";

/// The derived procedural-constraint shape IRIs the projector mints for the two
/// authored substrate constraints (the `{ConstraintLocalName}ProceduralConstraintShape`
/// convention). Findings are pinned to these so the tests assert the substrate
/// constraints fire — not merely that *some* shape flagged the node.
const AGREE_SHAPE: &str = "PinAgreementConstraintProceduralConstraintShape";
const COVER_SHAPE: &str = "PinCoverageConstraintProceduralConstraintShape";

fn flagged(fixture_rel: &str) -> Vec<(String, String)> {
    super::findings(MODULE, fixture_rel)
}

#[test]
fn pin_agreement_flags_the_disagreeing_claim() {
    // Two claims about purrdf's crate version disagree (0.12.0 vs 0.13.0); the
    // projected gmeow:PinAgreementConstraint must flag a disagreeing pin claim.
    let f = flagged("slices/core/attestation/tests/counter-examples/pin-disagreement.ttl");
    assert!(
        f.iter()
            .any(|(focus, shape)| shape.contains(AGREE_SHAPE) && focus.contains("claim")),
        "PinAgreementConstraint ({AGREE_SHAPE}) must flag a disagreeing pin claim; flagged: {f:?}"
    );
}

#[test]
fn pin_coverage_flags_the_component_missing_a_site() {
    // purrdf expects a prose-site claim but none exists; the projected
    // gmeow:PinCoverageConstraint must flag the component.
    let f = flagged("slices/core/attestation/tests/counter-examples/pin-missing-site.ttl");
    assert!(
        f.iter()
            .any(|(focus, shape)| shape.contains(COVER_SHAPE) && focus.contains("purrdfComp")),
        "PinCoverageConstraint ({COVER_SHAPE}) must flag the component missing a site; flagged: {f:?}"
    );
}

#[test]
fn reconciliation_is_silent_when_sites_agree_and_coverage_holds() {
    // Agreeing claims with every expected site witnessed: neither substrate constraint
    // fires. Restricted to the two substrate shapes so an unrelated slice constraint
    // flagging the fixture would not mask a substrate regression.
    let f =
        flagged("slices/core/attestation/tests/conformance-fixtures/pin-reconciliation-holds.ttl");
    let substrate: Vec<_> = f
        .iter()
        .filter(|(_, shape)| shape.contains(AGREE_SHAPE) || shape.contains(COVER_SHAPE))
        .collect();
    assert!(
        substrate.is_empty(),
        "no substrate reconciliation finding may fire on the conforming fixture; got: {substrate:?}"
    );
}
