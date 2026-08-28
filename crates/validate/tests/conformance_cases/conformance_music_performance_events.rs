// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from
//! slices/extensions/music/tests/test_music_performance_events.py (whole file;
//! the Python file is deleted).
//!
//! The 9 asserted-TBox / ABox guards run either over the music slice `module.ttl`
//! (`GraphStore::parse_ttl_file`) or, when the referenced seed lives in the core
//! `events` / `creative-works` slice (the EventType / ParticipantRole /
//! Contribution+Participant dual-typed roles), over the merged
//! `GraphStore::ontology()`. The 3 SHACL guards build the inline instance graphs
//! the Python assembled via `g.add(...)` and validate them against the whole
//! shapes corpus (`Case::inline`), reproducing the honest in-graph ABox
//! completion of the referenced concert / agent / roleSoloist targets.
//!
//! source -> dest map:
//!   test_performance_events_classes_exist                      -> performance_events_classes_exist
//!   test_performance_event_properties_exist                    -> performance_event_properties_exist
//!   test_event_type_seeds_exist                                -> event_type_seeds_exist
//!   test_participant_role_seeds_exist                          -> participant_role_seeds_exist
//!   test_dual_typed_music_roles                                -> dual_typed_music_roles
//!   test_instrument_type_seeds_exist                           -> instrument_type_seeds_exist
//!   test_playing_technique_seeds_exist                         -> playing_technique_seeds_exist
//!   test_session_fixture_exists                                -> session_fixture_exists
//!   test_who_played_what_on_take_3                             -> who_played_what_on_take_3
//!   test_performance_participation_valid_passes_shacl          -> case::performance_participation_valid_passes
//!   test_performance_participation_missing_role_fails_shacl    -> case::performance_participation_missing_role_fails
//!   test_performance_participation_two_instruments_warns_shacl -> case::performance_participation_two_instruments_warns

use crate::conformance_support::*;
use gmeow_test_batch_macros::batch_cases;
use purrdf::TermValue;

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC_CLASS: &str = "https://blackcatinformatics.ca/logic/Class";
const OWL_OBJECT_PROPERTY: &str = "http://www.w3.org/2002/07/owl#ObjectProperty";
const MUSIC_MODULE: &str = "slices/extensions/music/module.ttl";

fn g(local: &str) -> String {
    format!("{G}{local}")
}
fn module() -> &'static GraphStore {
    static STORE: std::sync::OnceLock<GraphStore> = std::sync::OnceLock::new();
    STORE.get_or_init(|| GraphStore::parse_ttl_file(&repo_root().join(MUSIC_MODULE)))
}

// ── Asserted-TBox / ABox guards ───────────────────────────────────────────────

#[gmeow_test_batch_macros::batch_test]
fn performance_events_classes_exist() {
    let s = module();
    for term in [
        "PerformanceParticipation",
        "InstrumentType",
        "InstrumentConfiguration",
        "PlayingTechnique",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(LOGIC_CLASS)),
            "{term} should be a canonical logic:Class"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn performance_event_properties_exist() {
    // Functionality is now carried by the canonical `logic:` characteristic records
    // (in the logic slice), not by a local `owl:FunctionalProperty` marker on the music
    // module — so this asserts over the merged ontology (a superset of `module()`, so the
    // owl:ObjectProperty declarations from the music module are still present).
    let s = GraphStore::ontology();
    // These participation properties carry a logic: functionalProperty characteristic.
    for prop in [
        "participationInstrumentItem",
        "participationConfiguration",
        "participationPart",
        "participationTechnique",
    ] {
        assert!(
            s.is_functional_carrier(&g(prop)),
            "{prop} should carry a logic: functionalProperty characteristic"
        );
    }
    // participationInstrument is a non-functional object property (deployments may
    // mint multiple participations rather than force identity entailments, P12).
    assert!(s.has(
        Some(&g("participationInstrument")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(
        !s.is_functional_carrier(&g("participationInstrument")),
        "participationInstrument must not be functional"
    );
    // performanceOf is a non-functional object property.
    assert!(s.has(
        Some(&g("performanceOf")),
        Some(RDF_TYPE),
        Some(OWL_OBJECT_PROPERTY)
    ));
    assert!(
        !s.is_functional_carrier(&g("performanceOf")),
        "performanceOf must be non-functional"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn event_type_seeds_exist() {
    // EventType value seeds live in the core `events` slice — use the merged ontology.
    let s = GraphStore::ontology();
    for term in [
        "eventTypeMusicalPerformance",
        "eventTypeConcert",
        "eventTypeRecordingSession",
        "eventTypeTake",
        "eventTypeOverdub",
        "eventTypeRehearsal",
        "eventTypeJamSession",
        "eventTypeSoundcheck",
        "eventTypeDJSet",
        "eventTypeTransmission",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("EventType"))),
            "{term} should be an EventType"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn participant_role_seeds_exist() {
    // ParticipantRole value seeds live in the core `events` slice.
    let s = GraphStore::ontology();
    for term in [
        "roleSoloist",
        "roleAccompanist",
        "roleEnsembleMember",
        "roleSessionMusician",
        "roleImproviser",
        "roleTransmitter",
        "roleLearner",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("ParticipantRole"))),
            "{term} should be a ParticipantRole"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn dual_typed_music_roles() {
    // performer / conductor / producer are BOTH ContributionRole (creative-works)
    // and ParticipantRole (events) — one concept, one IRI (P5). Needs the merge.
    let s = GraphStore::ontology();
    for term in ["rolePerformer", "roleConductor", "roleProducer"] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("ContributionRole"))),
            "{term} should be a ContributionRole"
        );
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("ParticipantRole"))),
            "{term} should be a ParticipantRole"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn instrument_type_seeds_exist() {
    let s = module();
    for term in [
        "instrumentTypePiano",
        "instrumentTypeViolin",
        "instrumentTypeDoubleBass",
        "instrumentTypeDrumKit",
        "instrumentTypeElectricGuitar",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("InstrumentType"))),
            "{term} should be an InstrumentType"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn playing_technique_seeds_exist() {
    let s = module();
    for term in [
        "playingTechniqueArco",
        "playingTechniquePizzicato",
        "playingTechniquePreparedPiano",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("PlayingTechnique"))),
            "{term} should be a PlayingTechnique"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn session_fixture_exists() {
    let s = module();
    assert!(s.has(
        Some(&g("fixtureSessionEvent")),
        Some(RDF_TYPE),
        Some(&g("Event"))
    ));
    for term in [
        "fixtureSessionTake1Event",
        "fixtureSessionTake2Event",
        "fixtureSessionTake3Event",
        "fixtureSessionOverdubEvent",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("Event"))),
            "{term} should be an Event"
        );
    }
    for term in [
        "fixtureSessionTake1Recording",
        "fixtureSessionTake2Recording",
        "fixtureSessionTake3Recording",
        "fixtureSessionOverdubRecording",
        "fixtureSessionComposite",
    ] {
        assert!(
            s.has(Some(&g(term)), Some(RDF_TYPE), Some(&g("Recording"))),
            "{term} should be a Recording"
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn who_played_what_on_take_3() {
    // The fixture SPARQL query returns bassist + drummer on take 3.
    let s = module();
    let (vars, rows) = s.select(
        &[],
        "
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?participant ?instrument ?technique
        WHERE {
            gmeow:fixtureSessionTake3Event
                gmeow:performanceOf gmeow:fixtureSessionWork .
            ?participation a gmeow:PerformanceParticipation ;
                gmeow:participationEvent gmeow:fixtureSessionTake3Event ;
                gmeow:participationParticipant ?participant ;
                gmeow:participationInstrument ?instrument .
            OPTIONAL {
                ?participation gmeow:participationTechnique ?technique .
            }
        }
        ORDER BY ?participant
    ",
    );
    assert_eq!(rows.len(), 2, "expected bassist and drummer on take 3");

    let col = |name: &str| vars.iter().position(|v| v == name).expect("var present");
    let p_ix = col("participant");
    let i_ix = col("instrument");
    let t_ix = col("technique");

    let bassist = TermValue::iri(g("fixtureSessionBassist"));
    let drummer = TermValue::iri(g("fixtureSessionDrummer"));
    let participants: Vec<&TermValue> = rows.iter().filter_map(|r| r[p_ix].as_ref()).collect();
    assert!(
        participants.contains(&&bassist),
        "bassist missing: {participants:?}"
    );
    assert!(
        participants.contains(&&drummer),
        "drummer missing: {participants:?}"
    );

    // Bassist uses pizzicato on double bass.
    let bassist_row = rows
        .iter()
        .find(|r| r[p_ix].as_ref() == Some(&bassist))
        .expect("bassist row present");
    assert_eq!(
        bassist_row[i_ix].as_ref(),
        Some(&TermValue::iri(g("instrumentTypeDoubleBass")))
    );
    assert_eq!(
        bassist_row[t_ix].as_ref(),
        Some(&TermValue::iri(g("playingTechniquePizzicato")))
    );
}

// ── SHACL guards (whole shapes corpus, inline fixtures) ───────────────────────

const PREFIXES: &str = "\
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/test-music-performance-events/> .
";

/// The referenced canonical targets restated in-graph (honest ABox completion) so
/// the generated `sh:class` shapes on participationEvent / participationParticipant
/// / participationRole resolve without loading the merged ontology (P11).
const TARGETS: &str = "\
ex:concert a gmeow:Event .
ex:agent a gmeow:Entity .
gmeow:roleSoloist a gmeow:ParticipantRole .
";

#[batch_cases]
#[case::performance_participation_valid_passes(Case::inline(format!(
    "{PREFIXES}\
ex:performanceParticipationValid a gmeow:PerformanceParticipation .
ex:performanceParticipationValid gmeow:participationEvent ex:concert .
ex:performanceParticipationValid gmeow:participationParticipant ex:agent .
ex:performanceParticipationValid gmeow:participationRole gmeow:roleSoloist .
ex:performanceParticipationValid gmeow:participationInstrument gmeow:instrumentTypePiano .
ex:performanceParticipationValid gmeow:participationTechnique gmeow:playingTechniqueArco .
{TARGETS}"
)))]
#[case::performance_participation_missing_role_fails(Case::inline(format!(
    "{PREFIXES}\
ex:performanceParticipationBad a gmeow:PerformanceParticipation .
ex:performanceParticipationBad gmeow:participationEvent ex:concert .
ex:performanceParticipationBad gmeow:participationParticipant ex:agent .
"
)).fails().violations(&["exactly one ParticipantRole"]))]
#[case::performance_participation_two_instruments_warns(Case::inline(format!(
    "{PREFIXES}\
ex:performanceParticipationBad a gmeow:PerformanceParticipation .
ex:performanceParticipationBad gmeow:participationEvent ex:concert .
ex:performanceParticipationBad gmeow:participationParticipant ex:agent .
ex:performanceParticipationBad gmeow:participationRole gmeow:roleSoloist .
ex:performanceParticipationBad gmeow:participationInstrument gmeow:instrumentTypePiano .
ex:performanceParticipationBad gmeow:participationInstrument gmeow:instrumentTypeViolin .
{TARGETS}"
)).warnings(&["At most one instrument"]))]
fn performance_events_shacl(#[case] case: Case) {
    case.run();
}
