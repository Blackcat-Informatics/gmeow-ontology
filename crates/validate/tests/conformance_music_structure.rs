// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from slices/extensions/music/tests/test_music_structure.py
//! (whole file; the Python file is deleted).
//!
//! The 13 asserted-TBox guards run over the slice `module.ttl`
//! (`GraphStore::parse_ttl_file`) — every referenced class, property, and
//! individual is module-local, so no merged-ontology fallback is needed. The 10
//! SHACL guards build the inline instance graphs the Python assembled via
//! `g.add(...)` and validate them against the whole shapes corpus (`Case::inline`),
//! reproducing the honest in-graph ABox completion of the referenced canonical
//! individuals.
//!
//! Source → dest map (23):
//!   TBox (module.ttl via GraphStore):
//!     test_musical_segment_subclass_of_content_segment -> musical_segment_subclass_of_content_segment
//!     test_tone_event_subclass_of_musical_segment      -> tone_event_subclass_of_musical_segment
//!     test_segment_kind_values_exist                   -> segment_kind_values_exist
//!     test_transformation_types_exist                  -> transformation_types_exist
//!     test_interpolation_kinds_exist                   -> interpolation_kinds_exist
//!     test_dynamics_values_exist                       -> dynamics_values_exist
//!     test_articulation_kinds_exist                    -> articulation_kinds_exist
//!     test_riff_transformation_chain_exists            -> riff_transformation_chain_exists
//!     test_tone_event_fixture_exists                   -> tone_event_fixture_exists
//!     test_pitch_trajectory_fixture_exists             -> pitch_trajectory_fixture_exists
//!     test_voice_fixture_exists                        -> voice_fixture_exists
//!     test_placeholder_voices_retyped                  -> placeholder_voices_retyped
//!     test_structure_functional_properties             -> structure_functional_properties
//!   SHACL (Case::inline via music_structure_shacl #[case]):
//!     test_musical_segment_valid_passes_shacl                      -> case::musical_segment_valid_passes
//!     test_musical_segment_missing_kind_fails_shacl               -> case::musical_segment_missing_kind_fails
//!     test_tone_event_pitch_value_passes_shacl                    -> case::tone_event_pitch_value_passes
//!     test_tone_event_multiple_pitch_modes_fails_shacl            -> case::tone_event_multiple_pitch_modes_fails
//!     test_pitch_trajectory_valid_passes_shacl                    -> case::pitch_trajectory_valid_passes
//!     test_pitch_trajectory_single_control_point_fails_shacl      -> case::pitch_trajectory_single_control_point_fails
//!     test_segment_transformation_valid_passes_shacl              -> case::segment_transformation_valid_passes
//!     test_segment_transformation_missing_source_fails_shacl      -> case::segment_transformation_missing_source_fails
//!     test_segment_transformation_source_equals_target_fails_shacl -> case::segment_transformation_source_equals_target_fails
//!     test_voice_valid_passes_shacl                               -> case::voice_valid_passes

mod conformance_support;
use conformance_support::*;
use rstest::rstest;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const MUSIC_MODULE: &str = "slices/extensions/music/module.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn module() -> &'static GraphStore {
    static STORE: std::sync::OnceLock<GraphStore> = std::sync::OnceLock::new();
    STORE.get_or_init(|| GraphStore::parse_ttl_file(&repo_root().join(MUSIC_MODULE)))
}

// ── Asserted-TBox guards (slice module) ───────────────────────────────────────

#[test]
fn musical_segment_subclass_of_content_segment() {
    assert!(module().has(
        Some(&g("MusicalSegment")),
        Some(RDFS_SUBCLASSOF),
        Some(&g("ContentSegment"))
    ));
}

#[test]
fn tone_event_subclass_of_musical_segment() {
    assert!(module().has(
        Some(&g("ToneEvent")),
        Some(RDFS_SUBCLASSOF),
        Some(&g("MusicalSegment"))
    ));
}

#[test]
fn segment_kind_values_exist() {
    let s = module();
    for term in [
        "segmentKindToneEventContainer",
        "segmentKindMotif",
        "segmentKindRiff",
        "segmentKindCell",
        "segmentKindPhrase",
        "segmentKindSection",
        "segmentKindFragment",
        "segmentKindTalea",
        "segmentKindColor",
        "segmentKindDrone",
        "segmentKindLoop",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("SegmentKind"))),
            "{term} should be a SegmentKind"
        );
    }
}

#[test]
fn transformation_types_exist() {
    let s = module();
    for term in [
        "transformTransposition",
        "transformInversion",
        "transformRetrograde",
        "transformAugmentation",
        "transformDiminution",
        "transformPhaseShift",
        "transformReaccentuation",
        "transformOctaveDisplacement",
        "transformTimbreReorchestration",
        "transformSpectralCompression",
        "transformOrnamentation",
        "transformQuotation",
        "transformReduction",
    ] {
        assert!(
            s.has(
                Some(&g(term)),
                Some(RDF_TYPE),
                Some(&g("TransformationType"))
            ),
            "{term} should be a TransformationType"
        );
    }
}

#[test]
fn interpolation_kinds_exist() {
    let s = module();
    for term in [
        "interpolationLinearCents",
        "interpolationExponential",
        "interpolationStochasticByReference",
    ] {
        assert!(s.has(
            Some(&g(term)),
            Some(RDF_TYPE),
            Some(&g("PitchTrajectoryInterpolationKind"))
        ));
    }
}

#[test]
fn dynamics_values_exist() {
    let s = module();
    for term in [
        "dynamicsPpp",
        "dynamicsPp",
        "dynamicsP",
        "dynamicsMp",
        "dynamicsMf",
        "dynamicsF",
        "dynamicsFf",
        "dynamicsFff",
    ] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("DynamicsValue"))));
    }
}

#[test]
fn articulation_kinds_exist() {
    let s = module();
    for term in [
        "articulationStaccato",
        "articulationLegato",
        "articulationTenuto",
        "articulationAccent",
        "articulationMarcato",
        "articulationPizzicato",
        "articulationHarmonic",
    ] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("ArticulationKind"))));
    }
}

#[test]
fn riff_transformation_chain_exists() {
    let s = module();
    for term in [
        "fixtureStructureRiffA",
        "fixtureStructureRiffATransposed",
        "fixtureStructureRiffAReaccented",
    ] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("MusicalSegment"))));
    }
    for term in [
        "fixtureStructureTransposition",
        "fixtureStructureReaccentuation",
    ] {
        assert!(s.has(
            Some(&g(term)),
            Some(RDF_TYPE),
            Some(&g("SegmentTransformation"))
        ));
    }
}

#[test]
fn tone_event_fixture_exists() {
    let s = module();
    let tone_event = g("fixtureStructureToneEventC4");
    assert!(s.has(Some(&tone_event), Some(RDF_TYPE), Some(&g("ToneEvent"))));
    assert!(s.has(
        Some(&tone_event),
        Some(&g("toneEventPitchValue")),
        Some(&g("pitchValueC4Fixture"))
    ));
}

#[test]
fn pitch_trajectory_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureStructureGlissando")),
        Some(RDF_TYPE),
        Some(&g("PitchTrajectory"))
    ));
    for term in [
        "fixtureStructureGlissPointC4",
        "fixtureStructureGlissPointG4",
    ] {
        assert!(s.has(
            Some(&g(term)),
            Some(RDF_TYPE),
            Some(&g("PitchTrajectoryControlPoint"))
        ));
    }
}

#[test]
fn voice_fixture_exists() {
    let s = module();
    let voice = g("fixtureStructureVoiceBass");
    assert!(s.has(Some(&voice), Some(RDF_TYPE), Some(&g("Voice"))));
    assert!(s.has(
        Some(&voice),
        Some(&g("voiceTimeFrame")),
        Some(&g("musicalTimeFrameCommon"))
    ));
    assert!(s.has(
        Some(&voice),
        Some(&g("voiceTuningFrame")),
        Some(&g("tuningSystem12EDO"))
    ));
}

#[test]
fn placeholder_voices_retyped() {
    let s = module();
    for term in ["voiceGuitarPlaceholder", "voiceDrumsPlaceholder"] {
        assert!(s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("Voice"))));
    }
}

#[test]
fn structure_functional_properties() {
    // Functionality is now carried by the canonical `logic:` characteristic records
    // (in the logic slice), not by a local `owl:FunctionalProperty` marker on the music
    // module — so this asserts over the merged ontology, not `module()`.
    let s = GraphStore::ontology();
    let functional = [
        "segmentKind",
        "segmentSpan",
        "toneEventPitchValue",
        "toneEventPitchTrajectory",
        "toneEventDynamics",
        "toneEventArticulation",
        "toneEventTimbre",
        "toneEventIsUnpitched",
        "controlPointOfTrajectory",
        "controlPointPitch",
        "controlPointTimeFrame",
        "controlPointTimePositionNumerator",
        "controlPointTimePositionDenominator",
        "controlPointOrder",
        "interpolationKind",
        "voiceTimeFrame",
        "voiceTuningFrame",
        "voiceMetricStructure",
        "transformationSource",
        "transformationTarget",
        "transformationType",
        "transformationParameter",
    ];
    for prop in functional {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} should carry a logic: functionalProperty characteristic"
        );
    }
}

// ── SHACL guards (whole shapes corpus, inline fixtures) ───────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-structure/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
";

/// The canonical `pitchValueC4Fixture` / `pitchValueG4Fixture` individuals restated
/// in-graph (honest ABox completion, values verbatim from module.ttl) so the
/// generated `sh:class` range shape plus `PitchValueShape` (tuning frame + exactly
/// one encoding, `centsFromOrigin` here) resolve without loading the merged ontology.
const PITCH_VALUES: &str = "\
gmeow:pitchValueC4Fixture a gmeow:PitchValue .
gmeow:pitchValueC4Fixture gmeow:hasTuningFrame gmeow:tuningSystem12EDO .
gmeow:pitchValueC4Fixture gmeow:centsFromOrigin \"0.0\"^^xsd:decimal .
gmeow:pitchValueG4Fixture a gmeow:PitchValue .
gmeow:pitchValueG4Fixture gmeow:hasTuningFrame gmeow:tuningSystem12EDO .
gmeow:pitchValueG4Fixture gmeow:centsFromOrigin \"700.0\"^^xsd:decimal .
";

#[rstest]
#[case::musical_segment_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:segmentValid a gmeow:MusicalSegment .
ex:segmentValid gmeow:segmentKind gmeow:segmentKindRiff .
ex:segmentValid gmeow:segmentSpan gmeow:musicalTimeSpanBarOne .
"
)))]
#[case::musical_segment_missing_kind_fails(Case::inline(format!(
    "{PREFIXES}\
ex:segmentNoKind a gmeow:MusicalSegment .
ex:segmentNoKind gmeow:segmentSpan gmeow:musicalTimeSpanBarOne .
"
)).fails().violations(&["exactly one segmentKind"]))]
#[case::tone_event_pitch_value_passes(Case::inline(format!(
    "{PREFIXES}\
ex:toneEventC4 a gmeow:ToneEvent .
ex:toneEventC4 gmeow:segmentKind gmeow:segmentKindToneEventContainer .
ex:toneEventC4 gmeow:segmentSpan gmeow:musicalTimeSpanBarOne .
ex:toneEventC4 gmeow:toneEventPitchValue gmeow:pitchValueC4Fixture .
ex:toneEventC4 gmeow:toneEventDynamics gmeow:dynamicsMf .
{PITCH_VALUES}"
)))]
#[case::tone_event_multiple_pitch_modes_fails(Case::inline(format!(
    "{PREFIXES}\
ex:toneEventBad a gmeow:ToneEvent .
ex:toneEventBad gmeow:segmentKind gmeow:segmentKindToneEventContainer .
ex:toneEventBad gmeow:segmentSpan gmeow:musicalTimeSpanBarOne .
ex:toneEventBad gmeow:toneEventPitchValue gmeow:pitchValueC4Fixture .
ex:toneEventBad gmeow:toneEventIsUnpitched \"true\"^^xsd:boolean .
{PITCH_VALUES}"
)).fails().violations(&["exactly one pitch-content mode"]))]
#[case::pitch_trajectory_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:glissandoValid a gmeow:PitchTrajectory .
ex:glissandoValid gmeow:interpolationKind gmeow:interpolationLinearCents .
ex:glissandoValid gmeow:trajectoryControlPoint ex:glissPointOne .
ex:glissandoValid gmeow:trajectoryControlPoint ex:glissPointTwo .
ex:glissPointOne a gmeow:PitchTrajectoryControlPoint .
ex:glissPointOne gmeow:controlPointOfTrajectory ex:glissandoValid .
ex:glissPointOne gmeow:controlPointOrder \"0\"^^xsd:nonNegativeInteger .
ex:glissPointOne gmeow:controlPointTimeFrame gmeow:musicalTimeFrameCommon .
ex:glissPointOne gmeow:controlPointTimePositionNumerator \"0\"^^xsd:integer .
ex:glissPointOne gmeow:controlPointTimePositionDenominator \"1\"^^xsd:integer .
ex:glissPointOne gmeow:controlPointPitch gmeow:pitchValueC4Fixture .
ex:glissPointTwo a gmeow:PitchTrajectoryControlPoint .
ex:glissPointTwo gmeow:controlPointOfTrajectory ex:glissandoValid .
ex:glissPointTwo gmeow:controlPointOrder \"1\"^^xsd:nonNegativeInteger .
ex:glissPointTwo gmeow:controlPointTimeFrame gmeow:musicalTimeFrameCommon .
ex:glissPointTwo gmeow:controlPointTimePositionNumerator \"1\"^^xsd:integer .
ex:glissPointTwo gmeow:controlPointTimePositionDenominator \"1\"^^xsd:integer .
ex:glissPointTwo gmeow:controlPointPitch gmeow:pitchValueG4Fixture .
{PITCH_VALUES}"
)))]
#[case::pitch_trajectory_single_control_point_fails(Case::inline(format!(
    "{PREFIXES}\
ex:glissandoShort a gmeow:PitchTrajectory .
ex:glissandoShort gmeow:interpolationKind gmeow:interpolationLinearCents .
ex:glissandoShort gmeow:trajectoryControlPoint ex:glissPointOne .
ex:glissPointOne a gmeow:PitchTrajectoryControlPoint .
ex:glissPointOne gmeow:controlPointOfTrajectory ex:glissandoShort .
ex:glissPointOne gmeow:controlPointOrder \"0\"^^xsd:nonNegativeInteger .
ex:glissPointOne gmeow:controlPointTimeFrame gmeow:musicalTimeFrameCommon .
ex:glissPointOne gmeow:controlPointTimePositionNumerator \"0\"^^xsd:integer .
ex:glissPointOne gmeow:controlPointTimePositionDenominator \"1\"^^xsd:integer .
ex:glissPointOne gmeow:controlPointPitch gmeow:pitchValueC4Fixture .
{PITCH_VALUES}"
)).fails().violations(&["at least two control points"]))]
#[case::segment_transformation_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:transpositionValid a gmeow:SegmentTransformation .
ex:transpositionValid gmeow:transformationSource ex:sourceRiff .
ex:transpositionValid gmeow:transformationTarget ex:targetRiff .
ex:transpositionValid gmeow:transformationType gmeow:transformTransposition .
ex:sourceRiff a gmeow:MusicalSegment .
ex:sourceRiff gmeow:segmentKind gmeow:segmentKindRiff .
ex:targetRiff a gmeow:MusicalSegment .
ex:targetRiff gmeow:segmentKind gmeow:segmentKindRiff .
gmeow:transformTransposition a gmeow:TransformationType .
"
)))]
#[case::segment_transformation_missing_source_fails(Case::inline(format!(
    "{PREFIXES}\
ex:transpositionBad a gmeow:SegmentTransformation .
ex:transpositionBad gmeow:transformationTarget ex:targetRiff .
ex:transpositionBad gmeow:transformationType gmeow:transformTransposition .
ex:targetRiff a gmeow:MusicalSegment .
ex:targetRiff gmeow:segmentKind gmeow:segmentKindRiff .
"
)).fails().violations(&["exactly one source MusicalSegment"]))]
#[case::segment_transformation_source_equals_target_fails(Case::inline(format!(
    "{PREFIXES}\
ex:retrogradeBad a gmeow:SegmentTransformation .
ex:retrogradeBad gmeow:transformationSource ex:palindromeRiff .
ex:retrogradeBad gmeow:transformationTarget ex:palindromeRiff .
ex:retrogradeBad gmeow:transformationType gmeow:transformRetrograde .
ex:palindromeRiff a gmeow:MusicalSegment .
ex:palindromeRiff gmeow:segmentKind gmeow:segmentKindRiff .
"
)).fails().violations(&["source and target must be distinct"]))]
#[case::voice_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:voiceValid a gmeow:Voice .
ex:voiceValid gmeow:voiceTimeFrame gmeow:musicalTimeFrameCommon .
ex:voiceValid gmeow:voiceTuningFrame gmeow:tuningSystem12EDO .
"
)))]
fn music_structure_shacl(#[case] case: Case) {
    case.run();
}
