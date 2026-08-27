// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from the temporal slice's graph-traversal tests
//! (tests/test_temporal_frame.py, tests/test_temporal_measurement.py,
//! tests/test_temporal.py).
//!
//! These are the temporal invariants the module-scoped `structural.ttl` ASK
//! harness cannot reach because the triples they assert are cross-slice: a
//! component class typed elsewhere, a seed-individual enumeration over the full
//! merged ontology, or a subclass edge declared in another slice's module. Each
//! runs over [`GraphStore::ontology`] — the merged, imports-free ontology — the
//! native twin of `load_merged_graph(include_imports=True)`.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_temporal_frame_component_classes_exist`: the frame's three
/// component classes — gmeow:TimeScale, gmeow:CalendarSystem,
/// gmeow:ReferencePosition — MUST each be declared `a owl:Class`. They are not
/// declared inside temporal/module.ttl (cross-slice), so the module-scoped cell
/// harness cannot see them; this runs over the merged ontology.
#[gmeow_test_batch_macros::batch_test]
fn temporal_frame_component_classes_exist() {
    let g = GraphStore::ontology();
    for term in ["TimeScale", "CalendarSystem", "ReferencePosition"] {
        assert!(
            g.has(Some(&gm(term)), Some(RDF_TYPE), Some(OWL_CLASS)),
            "gmeow:{term} must be declared a owl:Class in the merged ontology"
        );
    }
}

/// Twin of `test_temporal_frame_seed_individuals`: the merged ontology MUST carry
/// at least two time scales, at least two calendar systems including the Gregorian
/// seed plus at least one non-Gregorian calendar, and at least two temporal frames
/// each anchored to the temporal frame realm.
#[gmeow_test_batch_macros::batch_test]
fn temporal_frame_seed_individuals() {
    let g = GraphStore::ontology();

    let scales = g.subjects_of_type(&gm("TimeScale"));
    assert!(
        scales.len() >= 2,
        "at least two gmeow:TimeScale seeds; got {}",
        scales.len()
    );

    let calendars = g.subjects_of_type(&gm("CalendarSystem"));
    assert!(
        calendars.len() >= 2,
        "at least two gmeow:CalendarSystem seeds; got {}",
        calendars.len()
    );
    assert!(
        calendars.contains(&gm("calendarGregorian")),
        "gmeow:calendarGregorian must be a seed calendar system"
    );
    let non_gregorian = [
        "calendarJulian",
        "calendarHebrew",
        "calendarIslamic",
        "calendarChinese",
        "calendarPersian",
        "calendarEthiopian",
        "calendarCoptic",
        "calendarISOWeek",
    ];
    assert!(
        non_gregorian.iter().any(|c| calendars.contains(&gm(c))),
        "at least one non-Gregorian calendar system must be seeded; calendars: {calendars:?}"
    );

    let frames = g.subjects_of_type(&gm("TemporalFrame"));
    assert!(
        frames.len() >= 2,
        "at least two gmeow:TemporalFrame seeds; got {}",
        frames.len()
    );
    for frame in &frames {
        assert!(
            g.has(
                Some(frame),
                Some(&gm("frameRealm")),
                Some(&gm("frameRealmTemporal")),
            ),
            "temporal frame {frame} must carry gmeow:frameRealm gmeow:frameRealmTemporal"
        );
    }
}

/// Twin of `test_temporal_measurement_is_subclass_of_measurement`:
/// gmeow:TemporalMeasurement ⊑ gmeow:Measurement, and gmeow:Measurement ⊑
/// gmeow:Observation (the second edge lives in another slice), so a temporal
/// measurement is transitively an observation (P9).
#[gmeow_test_batch_macros::batch_test]
fn temporal_measurement_is_subclass_of_measurement() {
    let g = GraphStore::ontology();
    assert!(
        g.has(
            Some(&gm("TemporalMeasurement")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("Measurement")),
        ),
        "gmeow:TemporalMeasurement must be a subclass of gmeow:Measurement"
    );
    assert!(
        g.has(
            Some(&gm("Measurement")),
            Some(RDFS_SUBCLASS_OF),
            Some(&gm("Observation")),
        ),
        "gmeow:Measurement must be a subclass of gmeow:Observation (cross-slice)"
    );
}

/// Twin of `test_reified_residence_and_tenure_are_time_scoped`: the reified
/// gmeow:MailboxResidence (extensions/email) and gmeow:AddressTenure
/// (core/contacts) each declare their ⊑ gmeow:TimeScopedRelation edge in their OWN
/// slice modules — a merged-ontology integration check, not a temporal-module one.
#[gmeow_test_batch_macros::batch_test]
fn reified_residence_and_tenure_are_time_scoped() {
    let g = GraphStore::ontology();
    for term in ["MailboxResidence", "AddressTenure"] {
        assert!(
            g.has(
                Some(&gm(term)),
                Some(RDFS_SUBCLASS_OF),
                Some(&gm("TimeScopedRelation")),
            ),
            "gmeow:{term} must be a subclass of gmeow:TimeScopedRelation in the merged ontology"
        );
    }
}
