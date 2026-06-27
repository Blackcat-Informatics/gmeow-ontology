// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_music_analysis.py (#867)
//!
//! Migrated tests assert `result.ok` / `not result.ok` + substring checks using
//! inline fixture Turtle only (no `_graph()` / `load_merged_graph` dependency).
//!
//! Mode discipline:
//!   - `test_genre_no_subclass_shape_fails_on_bad_subclass`: `g = Graph()` only
//!     → `validate(&nt)`
//!   - `test_music_analysis_claim_shape_passes`,
//!     `test_music_analysis_claim_missing_frame_fails`: `g = _graph()` + fixture
//!     triples → `validate_with_ontology(&nt)` (class membership checked by SHACL
//!     requires ontology TBox in scope).
//!
//! Retained in Python (not migrated):
//!   - `test_music_analysis_claim_subclass_of_observation`: TBox triple membership
//!     (`RDFS.subClassOf`) via `_graph()` — pure TBox check, not SHACL.
//!   - `test_analysis_target_subproperty_of_observed_feature`: TBox triple membership.
//!   - `test_analysis_frame_subproperty_of_has_reference_frame`: TBox triple membership.
//!   - `test_analysis_claim_constitutive_properties_are_functional`: dynamic sweep of
//!     4 properties via `_graph()` — pure TBox membership check.
//!   - `test_theory_frames_are_reference_frames`: dynamic sweep of 9 frames via
//!     `_graph()` — pure TBox membership check.
//!   - `test_genre_seeds_coexist`: dynamic sweep of 13 genre seeds via `_graph()`.
//!   - `test_genre_derivation_links_exist`: TBox triple membership via `_graph()`.
//!
//! Statement compiler checks for the contested meter cells live with the native
//! statement stage tests in `gmeow-pipeline`.

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

/// Turtle prefix block shared by all music analysis conformance tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/music/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

// ── Tests migrated from tests/test_music_analysis.py ─────────────────────────

#[rstest]
#[case::music_analysis_claim_shape_passes(
    Case::inline(format!(
        "{PREFIXES}\
ex:claim1 a gmeow:MusicAnalysisClaim .
ex:claim1 gmeow:analysisTarget   ex:segment1 .
ex:claim1 gmeow:analysisProperty gmeow:analysisPropertyHarmonyLabel .
ex:claim1 gmeow:analysisResult   gmeow:harmonicFunctionDominant .
ex:claim1 gmeow:vantage          ex:analyst1 .
ex:claim1 gmeow:analysisFrame    gmeow:theoryFrameRomanNumeral .
"
    ))
    .with_ontology()
)]
#[case::music_analysis_claim_missing_frame_fails(
    Case::inline(format!(
        "{PREFIXES}\
ex:claim1 a gmeow:MusicAnalysisClaim .
ex:claim1 gmeow:analysisTarget   ex:segment1 .
ex:claim1 gmeow:analysisProperty gmeow:analysisPropertyHarmonyLabel .
ex:claim1 gmeow:analysisResult   gmeow:harmonicFunctionDominant .
ex:claim1 gmeow:vantage          ex:analyst1 .
# deliberately no gmeow:analysisFrame
"
    ))
    .with_ontology()
    .fails()
    .violations(&["analysisFrame"])
)]
#[case::genre_no_subclass_shape_fails_on_bad_subclass(
    Case::inline(format!(
        "{PREFIXES}\
ex:FakeSubGenre a owl:Class .
ex:FakeSubGenre rdfs:subClassOf gmeow:Genre .
"
    ))
    .fails()
    .violations(&["Genre must not be subclassed"])
)]
fn music_analysis(#[case] case: Case) {
    case.run();
}
