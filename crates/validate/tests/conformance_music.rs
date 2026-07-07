// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from slices/extensions/music/tests/test_music_timbre.py
//! (the mapping-file assertion plus the two fixture-observation assertions; the TBox
//! seed/property assertions are now structural.ttl cells).
//!
//! `afo_timbre_mapping_exists` reads the committed music equivalences mapping
//! artifact (`slices/extensions/music/mappings/equivalences.ttl`) — a
//! generated/authored SSSOM-style file, not module.ttl — so it is a query over a
//! parsed artifact rather than a scopeModule TBox cell.

mod conformance_support;
use conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const AFO_AUDIO_FEATURE: &str = "https://w3id.org/afo/onto/1.1#AudioFeature";
const MUSIC_EQ_REL: &str = "slices/extensions/music/mappings/equivalences.ttl";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// The AFO AudioFeature ↔ gmeow:TimbreDescriptor closeMatch equivalence
/// (gmeow:eqMu039) is present in the music equivalences mapping artifact.
#[test]
fn afo_timbre_mapping_exists() {
    let g = GraphStore::parse_ttl_file(&repo_root().join(MUSIC_EQ_REL));
    let eq = gm("eqMu039");
    assert!(
        g.has(Some(&eq), Some(RDF_TYPE), Some(&gm("TermEquivalence"))),
        "gmeow:eqMu039 must be a gmeow:TermEquivalence"
    );
    assert!(
        g.has(
            Some(&eq),
            Some(&gm("alignPredicate")),
            Some(SKOS_CLOSE_MATCH)
        ),
        "gmeow:eqMu039 alignPredicate must be skos:closeMatch"
    );
    let objects = g.objects(&eq, &gm("alignObject"));
    assert!(
        objects.contains(AFO_AUDIO_FEATURE),
        "gmeow:eqMu039 alignObject must include the AFO AudioFeature; got {objects:?}"
    );
    let subjects = g.objects(&eq, &gm("alignSubject"));
    assert!(
        subjects.contains(&gm("TimbreDescriptor")),
        "gmeow:eqMu039 alignSubject must include gmeow:TimbreDescriptor; got {subjects:?}"
    );
}

// ── GraphStore twins migrated from test_music_timbre.py (fixture observations) ─
//
// The two timbre fixture observations are worked instances in the music module.ttl
// (not an examples/ file), so they load into the merged ontology graph — the twins
// run over `GraphStore::ontology()`, matching the Python `load_merged_graph`.

/// Twin of `test_timbre_fixture_observations_exist`: both fixture observations are
/// Observations of the shared tone event, each carrying a timbre result.
#[test]
fn fixture_observations_exist() {
    let g = GraphStore::ontology();
    let tone_event = gm("fixtureTimbreToneEvent");
    for term in [
        "fixtureHumanTimbreObservation",
        "fixtureMIRTimbreObservation",
    ] {
        let obs = gm(term);
        assert!(
            g.has(Some(&obs), Some(RDF_TYPE), Some(&gm("Observation"))),
            "gmeow:{term} must be a gmeow:Observation"
        );
        assert!(
            g.has(Some(&obs), Some(&gm("observedFeature")), Some(&tone_event)),
            "gmeow:{term} must observe the tone event"
        );
        assert!(
            !g.objects(&obs, &gm("timbreObservationResult")).is_empty(),
            "gmeow:{term} must carry a timbre observation result"
        );
    }
}

/// Twin of `test_timbre_fixture_coequal_vantages`: two co-equal vantages
/// (human/MIR), two distinct descriptor results, both over the same tone event.
#[test]
fn fixture_coequal_vantages() {
    let g = GraphStore::ontology();
    let human = gm("fixtureHumanTimbreObservation");
    let machine = gm("fixtureMIRTimbreObservation");
    let tone_event = gm("fixtureTimbreToneEvent");
    let result = gm("timbreObservationResult");
    let observed = gm("observedFeature");
    let vantage = gm("vantage");

    assert!(g.has(
        Some(&human),
        Some(&vantage),
        Some(&gm("fixtureHumanListener"))
    ));
    assert!(g.has(Some(&machine), Some(&vantage), Some(&gm("fixtureMIRAgent"))));
    assert!(g.has(
        Some(&human),
        Some(&result),
        Some(&gm("timbreDescriptorBright"))
    ));
    assert!(g.has(
        Some(&machine),
        Some(&result),
        Some(&gm("timbreDescriptorGritty"))
    ));
    // Co-equal: both observations point at the same tone event.
    assert!(g.has(Some(&human), Some(&observed), Some(&tone_event)));
    assert!(g.has(Some(&machine), Some(&observed), Some(&tone_event)));
}
