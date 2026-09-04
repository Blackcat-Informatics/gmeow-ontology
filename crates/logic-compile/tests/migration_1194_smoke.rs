// SPDX-License-Identifier: AGPL-3.0-only
//! Fast smoke for the migrated procedural constraints: each hand-authored
//! constraint parses (no MALFORMED_CONSTRAINT), projects a `sh:SPARQLConstraint`
//! block, and flags its counter-example fixture focus — verified WITHOUT a full
//! pipeline regenerate.

#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};

use gmeow_logic_compile::frontend::parse_logic_str;
use gmeow_logic_compile::projections::shapes::project_procedural_constraints;
use purrdf::parse_dataset;
use purrdf::shapes::engine::{parse_shapes, validate_dataset};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

/// Parse a module, assert no MALFORMED_CONSTRAINT, project ALL its constraints to one
/// procedural-constraint document, validate over the fixture, and return the flagged foci.
fn flagged(module_rel: &str, fixture_rel: &str) -> Vec<String> {
    let r = root();
    let src = std::fs::read_to_string(r.join(module_rel)).expect("read module");
    let (program, diags) = parse_logic_str(&src, None).expect("module parses");
    let malformed: Vec<&str> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_CONSTRAINT")
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        malformed.is_empty(),
        "{module_rel} has MALFORMED_CONSTRAINT: {malformed:?}"
    );
    let shapes_ttl = project_procedural_constraints(&program);
    let shapes = parse_shapes(&shapes_ttl, None).expect("projected shapes parse");
    let data_bytes = std::fs::read(r.join(fixture_rel)).expect("read fixture");
    let data = parse_dataset(&data_bytes, "text/turtle", None).expect("fixture parses as Turtle");
    let report = validate_dataset(&data, &shapes).expect("validate");
    report
        .results
        .iter()
        .map(|res| res.focus_node.to_string())
        .collect()
}

fn assert_flags(module_rel: &str, fixture_rel: &str, must_flag: &[&str]) {
    let f = flagged(module_rel, fixture_rel);
    for want in must_flag {
        assert!(
            f.iter().any(|x| x.contains(want)),
            "{module_rel} over {fixture_rel} must flag {want}; flagged: {f:?}"
        );
    }
}

#[test]
fn metric_signature_dimension_flags_bad_signature() {
    assert_flags(
        "slices/grounding/math/module.ttl",
        "slices/grounding/math/tests/counter-examples/metric-signature-dimension-mismatch.ttl",
        &["badSignature"],
    );
}

#[test]
fn gmn_compaction_overclaim_flags_the_compaction() {
    assert_flags(
        "slices/grounding/lang/module.ttl",
        "slices/grounding/lang/tests/counter-examples/gmn-compaction-overclaim.ttl",
        &["compactionOverclaim"],
    );
}

#[test]
fn gmn_version_overclaim_flags_the_migration_unit() {
    assert_flags(
        "slices/grounding/lang/module.ttl",
        "slices/grounding/lang/tests/counter-examples/gmn-version-overclaim.ttl",
        &["unitMigration"],
    );
}

#[test]
fn observation_constraints_flag_the_conflated_act() {
    assert_flags(
        "slices/core/observations/module.ttl",
        "slices/grounding/lang/tests/counter-examples/meaning-act-observation-conflation.ttl",
        &["act"],
    );
}

#[test]
fn observation_constraints_flag_the_ungrounded_observation() {
    assert_flags(
        "slices/core/observations/module.ttl",
        "slices/grounding/lang/tests/counter-examples/meaning-ungrounded-claim.ttl",
        &["obs"],
    );
}

#[test]
fn superseded_gender_identity_flags_the_lagged_facet() {
    assert_flags(
        "slices/core/gender/module.ttl",
        "tests/fixtures/shapes/suppression-warning-only.ttl",
        &["laggedFacet"],
    );
}

#[test]
fn credence_band_flags_the_over_claimed_credence() {
    assert_flags(
        "slices/core/standpoint/module.ttl",
        "tests/fixtures/shapes/standpoint-credence-band-violation.ttl",
        &["over-claimed"],
    );
}

#[test]
fn consent_constraints_flag_rs2_rs3_rs4() {
    assert_flags(
        "slices/core/rights/module.ttl",
        "tests/fixtures/shapes/privacy-malformed.ttl",
        &["rs2", "rs3", "rs4"],
    );
}

#[test]
fn no_preferred_claim_flags_crimea() {
    assert_flags(
        "slices/core/standpoint/module.ttl",
        "tests/fixtures/shapes/standpoint-preferred-violation.ttl",
        &["crimea"],
    );
}

#[test]
fn score_anchor_range_flags_the_overflow_anchor() {
    assert_flags(
        "slices/core/norms/module.ttl",
        "tests/fixtures/shapes/rubrics-malformed.ttl",
        &["overflowAnchor"],
    );
}
