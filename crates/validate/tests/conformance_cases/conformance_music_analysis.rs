// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_music_analysis.py
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

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::{RdfTerm, flat_rdf_quads_from_dataset, parse_dataset};

/// Turtle prefix block shared by all music analysis conformance tests.
const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://blackcatinformatics.ca/gmeow/examples/music/> .
@prefix rdf:   <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl:   <http://www.w3.org/2002/07/owl#> .
";

// ── Tests migrated from tests/test_music_analysis.py ─────────────────────────

#[batch_cases]
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

fn nt_iri(local: &str) -> String {
    format!("<https://blackcatinformatics.ca/gmeow/{local}>")
}

fn nt_term(value: &str) -> String {
    if value.starts_with("http") {
        format!("<{value}>")
    } else {
        nt_iri(value)
    }
}

fn nt_triple(subject: &str, predicate: &str, object: &str) -> String {
    format!(
        "{} {} {} .",
        nt_term(subject),
        nt_term(predicate),
        nt_term(object)
    )
}

#[gmeow_test_batch_macros::batch_test]
fn music_foundation_tbox_assertions_are_rust_covered() {
    let nt = base_ontology_nt();
    let subclass_genre = format!(
        " <http://www.w3.org/2000/01/rdf-schema#subClassOf> {} .",
        nt_iri("Genre")
    );
    let offenders = nt
        .lines()
        .filter(|line| line.contains(&subclass_genre) && !line.starts_with(&nt_iri("Genre")))
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "Genre must not be subclassed; found: {offenders:?}"
    );

    for role in ["rolePerformer", "roleConductor", "roleProducer"] {
        assert!(
            nt.contains(&nt_triple(
                role,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "ContributionRole"
            )),
            "{role} missing ContributionRole"
        );
        assert!(
            nt.contains(&nt_triple(
                role,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                "ParticipantRole"
            )),
            "{role} missing ParticipantRole"
        );
    }

    // Functionality is now carried by the canonical `logic:` characteristic records
    // (issue 1579 deprecated the source `owl:FunctionalProperty` marker to a
    // generated-view-only projection), so query the carrier over the merged ontology.
    let g = GraphStore::ontology();
    let gmeow = |local: &str| format!("https://blackcatinformatics.ca/gmeow/{local}");
    for property in ["derivationSource", "derivationProduct", "realizationMode"] {
        assert!(
            g.is_functional_carrier(&gmeow(property)),
            "{property} must carry a logic: functionalProperty characteristic"
        );
    }
    for property in ["derivationType", "hasGenre"] {
        assert!(
            !g.is_functional_carrier(&gmeow(property)),
            "{property} must NOT carry a logic: functionalProperty characteristic"
        );
    }
}

/// The IRI string of an object term, or `None` for blank/literal/triple terms.
fn object_iri(term: &RdfTerm) -> Option<&str> {
    match term {
        RdfTerm::Iri(iri) => Some(iri.as_str()),
        _ => None,
    }
}

/// A stable string key for a node term (IRI or blank node) used to join shape rows.
fn node_key(term: &RdfTerm) -> String {
    match term {
        RdfTerm::Iri(iri) => format!("<{iri}>"),
        RdfTerm::BlankNode(label) => format!("_:{label}"),
        other => format!("{other:?}"),
    }
}

#[gmeow_test_batch_macros::batch_test]
fn work_shapes_do_not_require_notated_realization() {
    let shapes = read_ttl(&repo_root().join("shapes").join("gmeow-shapes.ttl"));
    let dataset =
        parse_dataset(shapes.as_bytes(), "text/turtle", None).expect("parse gmeow shapes");
    let quads = flat_rdf_quads_from_dataset(&dataset);
    let sh_target_class = "http://www.w3.org/ns/shacl#targetClass";
    let sh_property = "http://www.w3.org/ns/shacl#property";
    let sh_path = "http://www.w3.org/ns/shacl#path";
    let sh_has_value = "http://www.w3.org/ns/shacl#hasValue";
    let work = "https://blackcatinformatics.ca/gmeow/Work";
    let realization_mode = "https://blackcatinformatics.ca/gmeow/realizationMode";
    let realization_mode_notated = "https://blackcatinformatics.ca/gmeow/realizationModeNotated";

    let work_shapes = quads
        .iter()
        .filter(|quad| quad.predicate == sh_target_class && object_iri(&quad.object) == Some(work))
        .map(|quad| node_key(&quad.subject))
        .collect::<std::collections::BTreeSet<_>>();
    let work_shape_properties = quads
        .iter()
        .filter(|quad| {
            quad.predicate == sh_property && work_shapes.contains(&node_key(&quad.subject))
        })
        .map(|quad| node_key(&quad.object))
        .collect::<std::collections::BTreeSet<_>>();

    let offending = work_shape_properties
        .iter()
        .filter(|property| {
            let path_is_realization_mode = quads.iter().any(|quad| {
                node_key(&quad.subject) == **property
                    && quad.predicate == sh_path
                    && object_iri(&quad.object) == Some(realization_mode)
            });
            if !path_is_realization_mode {
                return false;
            }
            quads.iter().any(|quad| {
                node_key(&quad.subject) == **property
                    && quad.predicate == sh_has_value
                    && object_iri(&quad.object) == Some(realization_mode_notated)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        offending.is_empty(),
        "SHACL Work shapes must not require notated realization: {offending:?}"
    );
}
