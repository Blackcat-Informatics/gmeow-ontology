// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology native SHACL conformance harness (#867).
//!
//! Migrated from Python `run_shacl` pytest functions:
//!
//! - `tests/test_shapes.py`:
//!   `test_wellformed_relator_fixture_conforms`,
//!   `test_malformed_relator_fixture_is_flagged`,
//!   `test_suppression_warning_does_not_fail_validation`,
//!   `test_orthogonality_data_check_rejects_two_axes`,
//!   `test_wellformed_facet_cardinality_passes`,
//!   `test_internal_language_tag_shape_is_case_insensitive`,
//!   `test_wellformed_reference_frame_passes`,
//!   `test_reference_frame_axis_count_must_match_dimension_count`,
//!   `test_malformed_reference_frame_fails`,
//!   `test_profile_open_value_guard_warns_on_orphan`,
//!   `test_wellformed_proximity_fixture_conforms`,
//!   `test_malformed_proximity_fixture_is_flagged`,
//!   `test_wellformed_expertise_fixture_conforms`,
//!   `test_malformed_expertise_fixture_is_flagged`.
//!
//! - `tests/test_attestation.py`:
//!   `test_contested_attestation_coexists`,
//!   `test_all_fixture_files_load`.
//!
//! - `tests/test_coreference.py`:
//!   `test_authority_link_without_match_strength_warns_only`.
//!
//! Each test calls [`validate`] against the real merged shapes corpus — the
//! same corpus that `make validate` uses — so regressions in shape authoring
//! are caught at Rust compile+test speed, not after Python import.

mod conformance_support;
use conformance_support::*;

// ── Tests migrated from tests/test_shapes.py ─────────────────────────────────

/// `test_wellformed_relator_fixture_conforms` — a well-formed data graph passes
/// every closed-world shape (AC#1 positive, #39).
#[test]
fn wellformed_relator_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "relator-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "relator-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_relator_fixture_is_flagged` — a malformed data graph is
/// rejected, and each shape names its violation (AC#1 negative, #39).
#[test]
fn malformed_relator_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "relator-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "relator-malformed.ttl must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("exactly one gmeow:Gender value"),
        "must flag GenderIdentity cardinality; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("must use exactly one appellation"),
        "must flag appellation cardinality; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("may fill at most one identity axis"),
        "must flag identity-axis orthogonality (P9); got: {all_msgs}"
    );
    // Suppression is a Warning, not a Violation.
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("should set gmeow:displayable false"),
        "suppression warning must appear; got: {warn_msgs}"
    );
}

/// `test_suppression_warning_does_not_fail_validation` — a superseded-but-unsuppressed
/// facet warns but does NOT hard-fail (`result.ok`, Principle 10).
///
/// Python's `result.ok` is `not result.errors`, which is `true` when only
/// `sh:Warning`-severity results are present.  The Rust equivalent is
/// `ok(&report)` (no Violation results); `report.conforms` would be `false`
/// here because SHACL's `conforms` field is `false` whenever any result exists.
#[test]
fn suppression_warning_does_not_fail_validation() {
    let nt = fixture_as_nt("shapes", "suppression-warning-only");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "warning-only graph must pass (no violations); violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("should set gmeow:displayable false"),
        "suppression warning must be present; got: {warn_msgs}"
    );
}

/// `test_orthogonality_data_check_rejects_two_axes` — the closed-world dual
/// of HermiT's two-axis inconsistency check: a node typed in two disjoint
/// identity axes is caught by SHACL without a reasoner.
#[test]
fn orthogonality_data_check_rejects_two_axes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:x a gmeow:GenderIdentity .
ex:x a gmeow:SexualOrientation .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(!report.conforms, "dual-axis node must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("may fill at most one identity axis"),
        "orthogonality message must appear; got: {all_msgs}"
    );
}

/// `test_wellformed_facet_cardinality_passes` — a lone facet with exactly one
/// value conforms (cardinality-shape control case).
#[test]
fn wellformed_facet_cardinality_passes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:f a gmeow:GenderIdentity .
ex:f gmeow:facetSubject ex:person .
ex:f gmeow:facetVantage ex:person .
ex:f gmeow:genderValue gmeow:genderNonBinary .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed GenderIdentity facet must pass; violations: {:?}",
        violations(&report)
    );
}

/// `test_internal_language_tag_shape_is_case_insensitive` — BCP-47 private-use
/// tags are case-insensitive in SHACL too.
#[test]
fn internal_language_tag_shape_is_case_insensitive() {
    // N-Triples directly: a gmeow:fullName literal with an uppercase private-use
    // language tag. The SHACL `sh:languageIn` check must accept this.
    let nt = "<https://example.org/test/name> \
               <https://blackcatinformatics.ca/gmeow/fullName> \
               \"Japanese\"@x-GMEOW-Japanese .\n";
    let report = validate(nt);
    assert!(
        ok(&report),
        "uppercase @x-GMEOW-Japanese tag must be accepted; violations: {:?}",
        violations(&report)
    );
}

/// `test_wellformed_reference_frame_passes` — a reference frame profile with all
/// required properties passes SHACL.
#[test]
fn wellformed_reference_frame_passes() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:frame a gmeow:ReferenceFrame .
ex:frame gmeow:frameRealm gmeow:frameRealmTerrestrial .
ex:frame gmeow:hasAxis ex:axisX .
ex:frame gmeow:dimensionCount \"1\"^^xsd:nonNegativeInteger .
ex:frame gmeow:frameKind gmeow:frameKindCartesian .
ex:frame gmeow:requiresHost \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .
ex:frame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmTerrestrial a gmeow:FrameRealm .
ex:axisX a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "well-formed ReferenceFrame must pass; violations: {:?}",
        violations(&report)
    );
}

/// `test_reference_frame_axis_count_must_match_dimension_count` — frame profiles
/// reject mismatched axis cardinality and dimension count.
#[test]
fn reference_frame_axis_count_must_match_dimension_count() {
    // One axis but dimensionCount = 3.
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
ex:frame a gmeow:ReferenceFrame .
ex:frame gmeow:frameRealm gmeow:frameRealmTerrestrial .
ex:frame gmeow:hasAxis ex:axisX .
ex:frame gmeow:dimensionCount \"3\"^^xsd:nonNegativeInteger .
ex:frame gmeow:frameKind gmeow:frameKindCartesian .
ex:frame gmeow:requiresHost \"false\"^^<http://www.w3.org/2001/XMLSchema#boolean> .
ex:frame gmeow:determinacyModel gmeow:determinacyCrisp .
gmeow:frameRealmTerrestrial a gmeow:FrameRealm .
ex:axisX a gmeow:Axis .
gmeow:frameKindCartesian a gmeow:FrameKind .
gmeow:determinacyCrisp a gmeow:Determinacy .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(!report.conforms, "axis/dimension mismatch must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("dimension count must equal"),
        "dimension-count mismatch message must appear; got: {all_msgs}"
    );
}

/// `test_malformed_reference_frame_fails` — a reference frame profile missing
/// required descriptors fails SHACL validation.
#[test]
fn malformed_reference_frame_fails() {
    // Bare ReferenceFrame with no required properties.
    let nt = "<https://example.org/test/frame> \
               <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
               <https://blackcatinformatics.ca/gmeow/ReferenceFrame> .\n";
    let report = validate(nt);
    assert!(!report.conforms, "bare ReferenceFrame must fail SHACL");
    let all_msgs = violations(&report).join("\n");
    assert!(
        all_msgs.contains("declare its frame realm"),
        "missing frameRealm message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("have at least one coordinate axis"),
        "missing hasAxis message must appear; got: {all_msgs}"
    );
}

/// `test_profile_open_value_guard_warns_on_orphan` — a novel open-value
/// individual with no profile descriptor triggers a warning but still conforms.
#[test]
fn profile_open_value_guard_warns_on_orphan() {
    let data_ttl = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
gmeow:profileReferenceFrame a gmeow:Profile .
gmeow:profileReferenceFrame rdfs:label \"Reference Frame Profile\" .
gmeow:profileReferenceFrame skos:definition \"Closed descriptor schema for reference frames.\" .
gmeow:profileReferenceFrame gmeow:profileDescriptor gmeow:frameRealm .
gmeow:profileReferenceFrame gmeow:profileOpenValue gmeow:FrameRealm .
ex:customRealm a gmeow:FrameRealm .
";
    let nt = ttl_str_to_nt(data_ttl);
    let report = validate(&nt);
    assert!(
        ok(&report),
        "open-value orphan is Warning only — must not have violations; violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains(
            "Open value individuals must be referenced by at least one profile descriptor"
        ),
        "open-value warning must appear; got: {warn_msgs}"
    );
}

/// `test_wellformed_proximity_fixture_conforms` — a well-formed
/// ProximityMeasurement passes every shape (AC#1 positive, #95).
#[test]
fn wellformed_proximity_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "proximity-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "proximity-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_proximity_fixture_is_flagged` — a malformed
/// ProximityMeasurement is rejected by SHACL (#95).
#[test]
fn malformed_proximity_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "proximity-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "proximity-malformed.ttl must fail SHACL");
    let all_msgs = (violations(&report).into_iter())
        .chain(warnings(&report))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_msgs.contains("exactly one starting entity (gmeow:observedFeature)"),
        "observedFeature cardinality message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("exactly one target entity (gmeow:proximityTo)"),
        "proximityTo cardinality message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("exactly one scalar quantity result"),
        "scalar quantity result message must appear; got: {all_msgs}"
    );
}

/// `test_wellformed_expertise_fixture_conforms` — a well-formed SkillProficiency
/// + Credential graph passes expertise shapes (#263).
#[test]
fn wellformed_expertise_fixture_conforms() {
    let nt = fixture_as_nt("shapes", "expertise-wellformed");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "expertise-wellformed.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_malformed_expertise_fixture_is_flagged` — a malformed expertise graph
/// is rejected by the SHACL shapes (#263).
#[test]
fn malformed_expertise_fixture_is_flagged() {
    let nt = fixture_as_nt("shapes", "expertise-malformed");
    let report = validate(&nt);
    assert!(!report.conforms, "expertise-malformed.ttl must fail SHACL");
    let all_msgs = (violations(&report).into_iter())
        .chain(warnings(&report))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        all_msgs.contains("must reference exactly one Skill"),
        "missing Skill reference message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("levelScale should match"),
        "levelScale mismatch message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("must be an Organization"),
        "Organization constraint message must appear; got: {all_msgs}"
    );
    assert!(
        all_msgs.contains("should reference a gmeow:Attestation"),
        "Attestation reference message must appear; got: {all_msgs}"
    );
}

// ── Tests migrated from tests/test_attestation.py ────────────────────────────

/// `test_contested_attestation_coexists` — a contested attestation: one
/// standpoint affirms, another refutes. Both claims load and SHACL-pass.
///
/// This is an ABox multi-file conformance check, not a TBox cell.
#[test]
fn contested_attestation_coexists() {
    let nt = fixture_as_nt("coverage", "attestation-vc");
    let report = validate(&nt);
    assert!(
        ok(&report),
        "attestation-vc.ttl must pass all shapes; violations: {:?}",
        violations(&report)
    );
}

/// `test_all_fixture_files_load` — every scenario in the attestation coverage
/// fixture set loads and SHACL-passes.
#[test]
fn attestation_all_fixture_files_load() {
    let fixtures = [
        "attestation-software-release",
        "attestation-vc",
        "attestation-email-reuse",
        "attestation-quality-report",
        "attestation-blockchain-claim",
        "attestation-ledger-evidence",
    ];
    for name in fixtures {
        let nt = fixture_as_nt("coverage", name);
        let report = validate(&nt);
        assert!(
            ok(&report),
            "{name}.ttl failed SHACL; violations: {:?}",
            violations(&report)
        );
    }
}

// ── Tests migrated from tests/test_coreference.py ────────────────────────────

/// `test_authority_link_without_match_strength_warns_only` — a bare
/// `gmeow:authorityLink` (no `gmeow:matchStrength`) passes (conforms) but
/// emits a warning recommending the strength annotation.
#[test]
fn authority_link_without_match_strength_warns_only() {
    let nt = "\
<https://example.org/coref/entity> \
<https://blackcatinformatics.ca/gmeow/authorityLink> \
<https://example.org/coref/authority> .\n";
    let report = validate(nt);
    assert!(
        ok(&report),
        "bare authorityLink must still pass (Warning only); violations: {:?}",
        violations(&report)
    );
    let warn_msgs = warnings(&report).join("\n");
    assert!(
        warn_msgs.contains("authority link should also assert"),
        "missing-matchStrength warning must appear; got: {warn_msgs}"
    );
}
