// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance tests for music/collections slice — migrated from tests/test_music_collections.py
//!
//! Migrated: all `run_shacl(Graph())` fixture-only SHACL tests (10 Python tests → 11 Rust tests).
//! `test_standpoint_memberships_pass_shacl` (loop over 2 cases) → 2 Rust tests.
//!
//! Retained in Python (not migrated):
//!   - `test_pitch_collection_kind_is_quality_value`: `(triple) in graph` TBox check.
//!   - `test_collection_member_role_is_quality_value`: `(triple) in graph` TBox check.
//!   - `test_pitch_spelling_system_is_information_object_kind`: `(triple) in graph` TBox check.
//!   - `test_pitch_collection_membership_is_relator`: `(triple) in graph` TBox check.
//!   - `test_pitch_spelling_is_relator`: `(triple) in graph` TBox check.
//!   - `test_collection_properties_are_functional`: `(triple) in graph` TBox check.
//!   - `test_membership_constituents_are_functional`: `(triple) in graph` TBox check.
//!   - `test_membership_context_is_not_functional`: `(triple) not in graph` TBox check.
//!   - `test_spelling_constituents_are_functional`: `(triple) in graph` TBox check.
//!   - `test_enharmonic_spellings_coexist`: `_graph()` + triple membership.
//!   - `test_rast_maqam_seeds_exist`: `_graph()` + triple membership.
//!   - `test_yaman_raga_seeds_exist`: `_graph()` + triple membership.
//!   - `test_messiaen_mode_seed_exists`: `_graph()` + triple membership.
//!   - `test_pcset_seed_exists`: `_graph()` + triple membership.
//!   - `test_standpoint_memberships_coexist`: `_graph()` + triple membership.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

/// Turtle prefix block shared by all music/collections tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-collections/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

// Every "missing constituent" counter-example below used to be rejected by a
// hand-authored `sh:NodeShape` in `slices/extensions/music/shapes.ttl` carrying a
// prose `sh:message`. Those shapes were retired by the shapes-to-logic migration
// (see docs/MIGRATING-SHAPES-TO-LOGIC.md); the obligations now live as EL-safe
// `logic:Restriction` axioms in `slices/extensions/music/module.ttl` and are
// PROJECTED to `generated/shapes/validation-shapes.ttl` as message-less
// `sh:minCount` property shapes. `whole_shapes()` deliberately drops that projected
// file from this fixture corpus (an open-world someValuesFrom reading would
// over-flag ABox-incomplete fixtures elsewhere in this suite), so exercising the
// projected bound requires the LIVE production shape union — the corpus
// `gmeow validate` actually runs — and the message-less result is asserted by
// `sh:resultPath` + `sh:sourceConstraintComponent` rather than by prose. Same
// rationale and same shape as the migrated `conformance_music_time` cases.
#[rstest]
#[case::pitch_collection_shape_requires_kind(
    Case::inline(format!(
        "{PREFIXES}\
ex:badCollection a gmeow:PitchCollection .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/collectionKind",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_collection_membership_valid_passes_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:membershipValid a gmeow:PitchCollectionMembership .
ex:membershipValid gmeow:membershipCollection gmeow:pitchCollectionPCSet027 .
ex:membershipValid gmeow:membershipPitch      gmeow:pitchValue12EDOOrigin .
ex:membershipValid gmeow:membershipRole       gmeow:collectionMemberRoleMember .
ex:membershipValid gmeow:membershipDegreeIndex \"0\"^^xsd:integer .
"
    ))
)]
#[case::pitch_collection_membership_missing_collection_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:membershipNoCollection a gmeow:PitchCollectionMembership .
ex:membershipNoCollection gmeow:membershipPitch gmeow:pitchValue12EDOOrigin .
ex:membershipNoCollection gmeow:membershipRole  gmeow:collectionMemberRoleMember .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/membershipCollection",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_collection_membership_missing_pitch_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:membershipNoPitch a gmeow:PitchCollectionMembership .
ex:membershipNoPitch gmeow:membershipCollection gmeow:pitchCollectionPCSet027 .
ex:membershipNoPitch gmeow:membershipRole       gmeow:collectionMemberRoleMember .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/membershipPitch",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_collection_membership_missing_role_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:membershipNoRole a gmeow:PitchCollectionMembership .
ex:membershipNoRole gmeow:membershipCollection gmeow:pitchCollectionPCSet027 .
ex:membershipNoRole gmeow:membershipPitch      gmeow:pitchValue12EDOOrigin .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/membershipRole",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_spelling_valid_passes_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:spellingValid a gmeow:PitchSpelling .
ex:spellingValid gmeow:spellingPitch  gmeow:pitchValue12EDOCSharp4 .
ex:spellingValid gmeow:spellingSystem gmeow:pitchSpellingSystemCMN .
ex:spellingValid gmeow:spelledName    \"C\u{266f}4\"^^xsd:string .
"
    ))
)]
#[case::pitch_spelling_missing_pitch_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:spellingNoPitch a gmeow:PitchSpelling .
ex:spellingNoPitch gmeow:spellingSystem gmeow:pitchSpellingSystemCMN .
ex:spellingNoPitch gmeow:spelledName    \"C\u{266f}4\"^^xsd:string .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/spellingPitch",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_spelling_missing_system_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:spellingNoSystem a gmeow:PitchSpelling .
ex:spellingNoSystem gmeow:spellingPitch gmeow:pitchValue12EDOCSharp4 .
ex:spellingNoSystem gmeow:spelledName   \"C\u{266f}4\"^^xsd:string .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/spellingSystem",
        "MinCountConstraintComponent",
    )
)]
#[case::pitch_spelling_missing_name_fails_shacl(
    Case::inline(format!(
        "{PREFIXES}\
ex:spellingNoName a gmeow:PitchSpelling .
ex:spellingNoName gmeow:spellingPitch  gmeow:pitchValue12EDOCSharp4 .
ex:spellingNoName gmeow:spellingSystem gmeow:pitchSpellingSystemCMN .
"
    ))
    .shape_union()
    .fails()
    .fails_on_path(
        "https://blackcatinformatics.ca/gmeow/spelledName",
        "MinCountConstraintComponent",
    )
)]
#[case::standpoint_memberships_pass_shacl_arabic(
    Case::inline(format!(
        "{PREFIXES}\
gmeow:membershipRastThirdArabic a gmeow:PitchCollectionMembership .
gmeow:membershipRastThirdArabic gmeow:membershipCollection  gmeow:pitchCollectionRastMaqam .
gmeow:membershipRastThirdArabic gmeow:membershipPitch       gmeow:pitchValue24EDOEHalfFlat4 .
gmeow:membershipRastThirdArabic gmeow:membershipRole        gmeow:collectionMemberRoleMember .
gmeow:membershipRastThirdArabic gmeow:membershipDegreeIndex \"2\"^^xsd:integer .
gmeow:membershipRastThirdArabic gmeow:accordingTo           gmeow:standpointArabicTheory .
"
    ))
)]
#[case::standpoint_memberships_pass_shacl_turkish(
    Case::inline(format!(
        "{PREFIXES}\
gmeow:membershipRastThirdTurkish a gmeow:PitchCollectionMembership .
gmeow:membershipRastThirdTurkish gmeow:membershipCollection  gmeow:pitchCollectionRastMaqam .
gmeow:membershipRastThirdTurkish gmeow:membershipPitch       gmeow:pitchValue24EDOE4 .
gmeow:membershipRastThirdTurkish gmeow:membershipRole        gmeow:collectionMemberRoleMember .
gmeow:membershipRastThirdTurkish gmeow:membershipDegreeIndex \"2\"^^xsd:integer .
gmeow:membershipRastThirdTurkish gmeow:accordingTo           gmeow:standpointTurkishTheory .
"
    ))
)]
fn music_collections(#[case] case: Case) {
    case.run();
}
