// SPDX-License-Identifier: AGPL-3.0-only
//! Source-level smoke for the multi-sited reconciliation kernel: the two
//! authored `logic:Constraint`s in the attestation slice parse (no
//! MALFORMED_CONSTRAINT), project a `sh:SPARQLConstraint` block, fire on their
//! counter-example fixtures, and stay silent on the conforming fixture — all
//! verified WITHOUT a full pipeline regenerate (no `generated/` dependency), the
//! same harness `migration_1194_smoke.rs` uses.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::projections::shapes::project_procedural_constraints;
use purrdf::parse_dataset;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

const MODULE: &str = "slices/core/attestation/module.ttl";

/// The derived procedural-constraint shape IRIs the projector mints for the two
/// authored substrate constraints (the `{ConstraintLocalName}ProceduralConstraintShape`
/// convention). Findings are pinned to these so the tests assert the substrate
/// constraints fire — not merely that *some* shape flagged the node.
const AGREE_SHAPE: &str = "PinAgreementConstraintProceduralConstraintShape";
const COVER_SHAPE: &str = "PinCoverageConstraintProceduralConstraintShape";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Parse the attestation module, assert no MALFORMED_CONSTRAINT, project ALL its
/// procedural constraints to one document, validate over `fixture_rel`, and return
/// each finding's `(focus-node IRI, source-shape IRI)` so tests can pin BOTH the
/// flagged node and the exact derived shape that flagged it.
fn flagged(fixture_rel: &str) -> Vec<(String, String)> {
    let r = root();
    let src = std::fs::read_to_string(r.join(MODULE)).expect("read attestation module");
    let (program, diags) = parse_logic_str(&src, None).expect("attestation module parses");
    let malformed: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_CONSTRAINT")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        malformed.is_empty(),
        "attestation module has MALFORMED_CONSTRAINT: {malformed:?}"
    );
    let shapes_ttl = project_procedural_constraints(&program);
    let shapes = parse_shapes(&shapes_ttl, None).expect("projected shapes parse");
    let data_bytes = std::fs::read(r.join(fixture_rel)).expect("read fixture");
    let data = parse_dataset(&data_bytes, "text/turtle", None).expect("fixture parses as Turtle");
    let report = validate_dataset(&data, &shapes).expect("validate");
    report
        .results
        .iter()
        .map(|res| (res.focus_node.to_string(), res.source_shape.to_string()))
        .collect()
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
