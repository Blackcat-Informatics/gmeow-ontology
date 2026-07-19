// SPDX-License-Identifier: AGPL-3.0-only
//! Conformance twins migrated from tests/test_interior.py

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

#[rstest]
#[case::wellformed_interior_fixture_conforms(Case::file("shapes", "interior-wellformed"))]
#[case::malformed_interior_fixture_is_flagged(
    Case::file("shapes", "interior-malformed")
        .fails()
        .violations(&[
            "exactly one gmeow:samplePosition",
            "exactly one gmeow:sampleState",
            "protagonist-of-WHAT is half the claim",
            "an unnameable recurring unit is a tag",
            "rides the narration seam into a ContentSegment",
            "exactly one gmeow:emotionBearer",
            "at least one gmeow:emotionType",
            "must read SOMETHING",
            "half a reading is no reading",
        ])
)]
fn interior(#[case] case: Case) {
    case.run();
}

// ── SPARQL / GraphStore twins migrated from tests/test_interior.py ─────────────
//
// Every `def test_*` in the Python module gets exactly one named twin below,
// asserting the same invariant over the native merged ontology / fixtures.

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const EX_SHAPES: &str = "https://example.org/shapes/";

const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_SUBPROPERTY_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";
const RDFS_DOMAIN: &str = "http://www.w3.org/2000/01/rdf-schema#domain";
const RDFS_RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

fn ex_shapes(local: &str) -> String {
    format!("{EX_SHAPES}{local}")
}

/// Twin of `test_plutchik_seeds_are_present_and_open`: the eight Plutchik seeds
/// are declared `gmeow:EmotionType` individuals, and the vocabulary is open (the
/// seeds are a subset — extra members are allowed).
#[test]
fn plutchik_seeds_are_present_and_open() {
    let g = GraphStore::ontology();
    let members = g.subjects_of_type(&gm("EmotionType"));
    for seed in [
        "emotionJoy",
        "emotionTrust",
        "emotionFear",
        "emotionSurprise",
        "emotionSadness",
        "emotionDisgust",
        "emotionAnger",
        "emotionAnticipation",
    ] {
        assert!(
            members.contains(&gm(seed)),
            "missing Plutchik seed gmeow:{seed}"
        );
    }
}

/// Twin of `test_appraisal_is_a_vantage_indexed_observation`: Appraisal ⊑
/// Observation, appraisalOf is a functional observedFeature subproperty, and the
/// per-cell reading properties are NOT OWL-functional (rival cells coexist, P9).
#[test]
fn appraisal_is_a_vantage_indexed_observation() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("Appraisal")),
        Some(RDFS_SUBCLASS_OF),
        Some(&gm("Observation"))
    ));
    assert!(g.has(
        Some(&gm("appraisalOf")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("observedFeature"))
    ));
    assert!(
        g.is_functional_carrier(&gm("appraisalOf")),
        "gmeow:appraisalOf must carry a logic: functionalProperty characteristic"
    );
    for prop in ["appraisalDimension", "appraisalValue", "appraisalQuality"] {
        assert!(
            !g.is_functional_carrier(&gm(prop)),
            "gmeow:{prop} must NOT carry a logic: functionalProperty characteristic (P9)"
        );
    }
}

/// Twin of `test_no_emotion_tenure_class_exists`: thin means thin — no
/// `gmeow:EmotionTenure` class is declared.
#[test]
fn no_emotion_tenure_class_exists() {
    let g = GraphStore::ontology();
    assert!(!g.has(Some(&gm("EmotionTenure")), Some(RDF_TYPE), Some(OWL_CLASS)));
}

/// Twin of `test_arc_sample_constituents`: ArcSample ⊑ Observation with the
/// functional sample constituents, a NarrativePosition-ranged samplePosition, an
/// open-range sampleState (soft cross-slice ref, P16), and the localizable
/// development-signal convention.
#[test]
fn arc_sample_constituents() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("ArcSample")),
        Some(RDFS_SUBCLASS_OF),
        Some(&gm("Observation"))
    ));
    assert!(g.has(
        Some(&gm("sampleSubject")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("observedFeature"))
    ));
    for prop in ["sampleSubject", "samplePosition", "sampleState"] {
        assert!(
            g.is_functional_carrier(&gm(prop)),
            "gmeow:{prop} must carry a logic: functionalProperty characteristic"
        );
    }
    assert!(g.has(
        Some(&gm("samplePosition")),
        Some(RDFS_RANGE),
        Some(&gm("NarrativePosition"))
    ));
    // sampleState is range-open by design (soft cross-slice reference).
    assert!(g.objects(&gm("sampleState"), RDFS_RANGE).is_empty());
    assert!(!g.is_functional_carrier(&gm("developmentSignalText")));
    assert!(g.has(
        Some(&gm("developmentSignalEvent")),
        Some(RDFS_RANGE),
        Some(&gm("Event"))
    ));
}

/// Twin of `test_character_arc_extension_is_additive`: arcSample is a
/// CharacterArc-domained hasPart subproperty, and the pre-existing arc machinery
/// is untouched.
#[test]
fn character_arc_extension_is_additive() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("arcSample")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("hasPart"))
    ));
    assert!(g.has(
        Some(&gm("arcSample")),
        Some(RDFS_DOMAIN),
        Some(&gm("CharacterArc"))
    ));
    assert!(g.has(
        Some(&gm("arcType")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
}

/// Twin of `test_no_primary_protagonist_machinery`: no `gmeow:` term whose local
/// name (no `/`) starts with a banned primary/preferred protagonist/role prefix.
/// Native re-expression of the Python subject sweep via a DISTINCT-subject SELECT.
#[test]
fn no_primary_protagonist_machinery() {
    let g = GraphStore::ontology();
    let offenders = gmeow_local_offenders(
        &g,
        &[
            "primaryprotagonist",
            "preferredprotagonist",
            "primaryrole",
            "preferredrole",
        ],
    );
    assert!(
        offenders.is_empty(),
        "protagonist machinery leaked: {offenders:?}"
    );
}

/// Twin of `test_motif_rides_the_seam`: Motif ⊑ SocialObject, motifOccursIn
/// specialises narratedIn into a ContentSegment, and motifKind is not functional.
#[test]
fn motif_rides_the_seam() {
    let g = GraphStore::ontology();
    assert!(g.has(
        Some(&gm("Motif")),
        Some(RDFS_SUBCLASS_OF),
        Some(&gm("SocialObject"))
    ));
    assert!(g.has(
        Some(&gm("motifOccursIn")),
        Some(RDFS_SUBPROPERTY_OF),
        Some(&gm("narratedIn"))
    ));
    assert!(g.has(
        Some(&gm("motifOccursIn")),
        Some(RDFS_RANGE),
        Some(&gm("ContentSegment"))
    ));
    assert!(!g.is_functional_carrier(&gm("motifKind")));
}

/// Twin of `test_trajectory_query_orders_and_surfaces_disagreement`: the
/// `narrative-arc-trajectory.rq` competency query (ORDER BY ?vantage ?ordinal)
/// over the interior-wellformed fixture surfaces both analyzers' readings — two
/// coexisting readings at ordinal 31, never resolved (P9).
#[test]
fn trajectory_query_orders_and_surfaces_disagreement() {
    QueryCase::new("narrative/arc-trajectory", &[])
        .over_ttl_file("tests/fixtures/shapes/interior-wellformed.ttl")
        .query_file("narrative-arc-trajectory.rq")
        .select_ordered(vec![
            vec![
                iri(&ex_shapes("modelA")),
                int_lit(3),
                iri(&gm("emotionAnticipation")),
            ],
            vec![
                iri(&ex_shapes("modelA")),
                int_lit(31),
                iri(&gm("emotionFear")),
            ],
            vec![
                iri(&ex_shapes("modelB")),
                int_lit(31),
                iri(&gm("emotionAnticipation")),
            ],
        ])
        .run();
}

/// Native re-expression of the Python "no `gmeow:` local subject starts with a
/// banned prefix" sweep: enumerate DISTINCT subjects, keep IRIs under the gmeow
/// namespace whose local name has no `/`, and flag those whose lowercased local
/// name starts with one of `banned`.
fn gmeow_local_offenders(store: &GraphStore, banned: &[&str]) -> Vec<String> {
    let (_vars, rows) = store.select(&[], "SELECT DISTINCT ?s WHERE { ?s ?p ?o }");
    let mut offenders = Vec::new();
    for row in &rows {
        let Some(Some(term)) = row.first() else {
            continue;
        };
        let Some(iri) = term.as_iri() else {
            continue;
        };
        if let Some(local) = iri.strip_prefix(GMEOW) {
            let lower = local.to_lowercase();
            if !local.contains('/') && banned.iter().any(|b| lower.starts_with(b)) {
                offenders.push(iri.to_owned());
            }
        }
    }
    offenders
}
