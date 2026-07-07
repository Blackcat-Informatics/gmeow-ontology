// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from slices/extensions/music/tests/test_music_timbre.py
//! (the mapping-file assertion; the TBox seed/property assertions are now
//! structural.ttl cells, and the fixture-observation assertions remain in Python
//! for a later batch).
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
