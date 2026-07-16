// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Whole-ontology native SHACL conformance harness.
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
//! Each case validates against the real merged shapes corpus — the same corpus
//! that `make validate` uses — so regressions in shape authoring are caught at
//! Rust compile+test speed, not after Python import.
//!
//! Collapsed onto the shared [`Case`] harness. `Case::raw_nt` feeds raw
//! N-Triples straight to the validator (no Turtle round-trip) for the originals
//! that called `validate(nt)` on an N-Triples literal — notably the
//! case-insensitive language-tag check, whose tag casing must not be normalised.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
// ── Tests migrated from tests/test_shapes.py ─────────────────────────────────
#[case::wellformed_relator_fixture_conforms(Case::file("shapes", "relator-wellformed"))]
#[case::malformed_relator_fixture_is_flagged(
    // The genderValue AND usageAppellation exactly-one bounds migrated to the projected
    // surface (generated/shapes/validation-shapes.ttl GenderIdentity-shape / NameUsage-shape),
    // which the fixture corpus deliberately excludes — their witnesses ride the production
    // shape union below (`malformed_relator_gender_value_bounds_on_union`). The suppression
    // warning likewise migrated to gmeow:SupersededFacetSuppressionConstraint on that union.
    // What remains authored on the fixture corpus is the orthogonality disjointness check.
    Case::file("shapes", "relator-malformed")
        .fails()
        .violations(&[
            "may fill at most one of these mutually disjoint classes",
        ])
)]
#[case::suppression_warning_does_not_fail_validation(
    Case::file("shapes", "suppression-warning-only")
        .warnings(&["should set gmeow:displayable false"])
)]
// Closed-world dual of the OWL 2 DL oracle's two-axis inconsistency check (no reasoner).
#[case::orthogonality_data_check_rejects_two_axes(
    Case::inline(
        "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:x a gmeow:GenderIdentity .
ex:x a gmeow:SexualOrientation .
"
    )
    .fails()
    .violations(&["may fill at most one of these mutually disjoint classes"])
)]
#[case::wellformed_facet_cardinality_passes(Case::inline(
    "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test/> .
ex:f a gmeow:GenderIdentity .
ex:f gmeow:facetSubject ex:person .
ex:f gmeow:facetVantage ex:person .
ex:f gmeow:genderValue gmeow:genderNonBinary .
"
))]
// Raw N-Triples (no Turtle round-trip): an uppercase BCP-47 private-use language
// tag must be accepted by the `sh:languageIn` check (case-insensitive).
#[case::internal_language_tag_shape_is_case_insensitive(Case::raw_nt(
    "<https://example.org/test/name> <https://blackcatinformatics.ca/gmeow/fullName> \"Japanese\"@x-GMEOW-Japanese .\n"
))]
#[case::wellformed_reference_frame_passes(Case::inline(
    "\
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
"
))]
// One axis but dimensionCount = 3.
#[case::reference_frame_axis_count_must_match_dimension_count(
    Case::inline(
        "\
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
"
    )
    .fails()
    .violations(&["dimension count must equal"])
)]
// Bare ReferenceFrame with no required properties (raw N-Triples).
#[case::malformed_reference_frame_fails(
    Case::raw_nt(
        "<https://example.org/test/frame> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://blackcatinformatics.ca/gmeow/ReferenceFrame> .\n"
    )
    .fails()
    .violations(&[
        "declare its frame realm",
        "have at least one coordinate axis",
    ])
)]
#[case::profile_open_value_guard_violates_on_orphan(
    Case::inline(
        "\
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
"
    )
    .fails()
    .violations(&["Open value individuals must be referenced by at least one profile descriptor"])
)]
#[case::wellformed_proximity_fixture_conforms(Case::file("shapes", "proximity-wellformed"))]
// Malformed fixtures assert against the UNION of violations + warnings (the
// originals joined `violations().chain(warnings())`), hence `.messages(...)`.
#[case::malformed_proximity_fixture_is_flagged(
    Case::file("shapes", "proximity-malformed")
        .fails()
        .messages(&[
            "exactly one starting entity (gmeow:observedFeature)",
            "exactly one target entity (gmeow:proximityTo)",
            "exactly one scalar quantity result",
        ])
)]
#[case::wellformed_expertise_fixture_conforms(Case::file("shapes", "expertise-wellformed"))]
#[case::malformed_expertise_fixture_is_flagged(
    Case::file("shapes", "expertise-malformed")
        .fails()
        .messages(&[
            "must reference exactly one Skill",
            "levelScale should match",
            "must be an Organization",
            "should reference a gmeow:Attestation",
        ])
)]
// ── Tests migrated from tests/test_attestation.py ────────────────────────────
// A contested attestation (one standpoint affirms, another refutes) loads and
// SHACL-passes — an ABox multi-file conformance check, not a TBox cell.
#[case::contested_attestation_coexists(Case::file("coverage", "attestation-vc"))]
// `test_all_fixture_files_load` — every scenario in the attestation coverage
// fixture set loads and SHACL-passes. Split into one case per fixture so each is
// independently named/runnable under nextest (finer granularity than the loop).
#[case::all_fixture_files_load_software_release(Case::file(
    "coverage",
    "attestation-software-release"
))]
#[case::all_fixture_files_load_vc(Case::file("coverage", "attestation-vc"))]
#[case::all_fixture_files_load_email_reuse(Case::file("coverage", "attestation-email-reuse"))]
#[case::all_fixture_files_load_quality_report(Case::file(
    "coverage",
    "attestation-quality-report"
))]
#[case::all_fixture_files_load_blockchain_claim(Case::file(
    "coverage",
    "attestation-blockchain-claim"
))]
#[case::all_fixture_files_load_ledger_evidence(Case::file(
    "coverage",
    "attestation-ledger-evidence"
))]
// ── Tests migrated from tests/test_coreference.py ────────────────────────────
// A bare `gmeow:authorityLink` (no `gmeow:matchStrength`) passes (conforms) but
// emits a warning recommending the strength annotation (raw N-Triples).
#[case::authority_link_without_match_strength_warns_only(
    Case::raw_nt(
        "<https://example.org/coref/entity> <https://blackcatinformatics.ca/gmeow/authorityLink> <https://example.org/coref/authority> .\n"
    )
    .warnings(&["authority link should also assert"])
)]
fn ontology_conformance(#[case] case: Case) {
    case.run();
}

/// The genderValue exactly-one bound of the retired hand-authored
/// `gmeow:GenderIdentityFacetShape` now rides the projected declarative surface
/// (`generated/shapes/validation-shapes.ttl`, `GenderIdentity-shape`
/// `sh:minCount 1` / `sh:maxCount 1` on `gmeow:genderValue`), which the fixture
/// corpus deliberately excludes — witness both bounds by path on the LIVE
/// production shape union (projected shapes carry no `sh:message`).
#[test]
fn malformed_relator_gender_value_bounds_on_union() {
    Case::file("shapes", "relator-malformed")
        .shape_union()
        .fails()
        .fails_on_path(
            "https://blackcatinformatics.ca/gmeow/genderValue",
            "MinCountConstraintComponent",
        )
        .fails_on_path(
            "https://blackcatinformatics.ca/gmeow/genderValue",
            "MaxCountConstraintComponent",
        )
        .run();
}
