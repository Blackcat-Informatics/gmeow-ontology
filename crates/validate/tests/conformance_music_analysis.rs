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
//!   - `test_statement_cells_include_contested_meter_pair`: uses `load_statement_dsl()`,
//!     no SHACL — DSL/statement-compile test.
//!   - `test_statement_cells_emit_owl_axioms_with_standpoints`: uses `emit_owl()`,
//!     no SHACL — DSL/statement-compile test.

mod conformance_support;
use conformance_support::*;

/// Turtle prefix block shared by all music analysis conformance tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/music/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

// ── Tests migrated from tests/test_music_analysis.py ─────────────────────────

/// `test_music_analysis_claim_shape_passes` — a fully-populated MusicAnalysisClaim
/// with all required properties passes SHACL.
///
/// Mode: `validate_with_ontology` (Python used `g = _graph()` as base).
#[test]
fn music_analysis_claim_shape_passes() {
    let fixture = format!(
        "{PREFIXES}\
ex:claim1 a gmeow:MusicAnalysisClaim .
ex:claim1 gmeow:analysisTarget   ex:segment1 .
ex:claim1 gmeow:analysisProperty gmeow:analysisPropertyHarmonyLabel .
ex:claim1 gmeow:analysisResult   gmeow:harmonicFunctionDominant .
ex:claim1 gmeow:vantage          ex:analyst1 .
ex:claim1 gmeow:analysisFrame    gmeow:theoryFrameRomanNumeral .
"
    );
    let nt = ttl_str_to_nt(&fixture);
    let report = validate_with_ontology(&nt);
    assert!(
        ok(&report),
        "fully-populated MusicAnalysisClaim must pass SHACL; violations: {:?}",
        violations(&report)
    );
}

/// `test_music_analysis_claim_missing_frame_fails` — a MusicAnalysisClaim missing
/// the required `gmeow:analysisFrame` fails SHACL with an `analysisFrame` message.
///
/// Mode: `validate_with_ontology` (Python used `g = _graph()` as base).
#[test]
fn music_analysis_claim_missing_frame_fails() {
    let fixture = format!(
        "{PREFIXES}\
ex:claim1 a gmeow:MusicAnalysisClaim .
ex:claim1 gmeow:analysisTarget   ex:segment1 .
ex:claim1 gmeow:analysisProperty gmeow:analysisPropertyHarmonyLabel .
ex:claim1 gmeow:analysisResult   gmeow:harmonicFunctionDominant .
ex:claim1 gmeow:vantage          ex:analyst1 .
# deliberately no gmeow:analysisFrame
"
    );
    let nt = ttl_str_to_nt(&fixture);
    let report = validate_with_ontology(&nt);
    assert!(
        !ok(&report),
        "MusicAnalysisClaim missing analysisFrame must fail SHACL"
    );
    let msgs = violations(&report);
    assert!(
        msgs.iter().any(|m| m.contains("analysisFrame")),
        "violation message must mention 'analysisFrame'; got: {:?}",
        msgs
    );
}

/// `test_genre_no_subclass_shape_fails_on_bad_subclass` — declaring a subclass of
/// `gmeow:Genre` fails SHACL with a "Genre must not be subclassed" message.
///
/// Mode: `validate` (Python used `g = Graph()` — fixture-only, no merged ontology).
#[test]
fn genre_no_subclass_shape_fails_on_bad_subclass() {
    let fixture = format!(
        "{PREFIXES}\
ex:FakeSubGenre a owl:Class .
ex:FakeSubGenre rdfs:subClassOf gmeow:Genre .
"
    );
    let nt = ttl_str_to_nt(&fixture);
    let report = validate(&nt);
    assert!(!ok(&report), "subclassing gmeow:Genre must fail SHACL");
    let msgs = violations(&report);
    assert!(
        msgs.iter()
            .any(|m| m.contains("Genre must not be subclassed")),
        "violation message must mention 'Genre must not be subclassed'; got: {:?}",
        msgs
    );
}
