// SPDX-License-Identifier: AGPL-3.0-only
//! Procedural-constraint findings recorded by the explicit corpus producer.
//! Tests grade the exact selected module and counterexample result without recompilation.

fn flagged(module_rel: &str, fixture_rel: &str) -> Vec<String> {
    super::findings(module_rel, fixture_rel)
        .into_iter()
        .map(|(focus, _)| focus)
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
