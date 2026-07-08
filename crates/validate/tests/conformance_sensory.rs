// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from tests/test_sensory.py — the SOSA / AFO
//! mapping-alignment checks (the file's asserted-TBox structural checks and OWL
//! 2 RL entailments were migrated earlier to `structural.ttl` cells and the
//! native RL harness respectively; only the `load_mappings()` reads remained).
//!
//! Every twin asserts against the GENERATED SSSOM projection
//! (`generated/mappings/*.sssom.tsv`) — never the authored `equivalences.ttl`
//! cells — so the checks bind to the stable Principle-17 projection surface.
//!
//! The Python `load_mappings()` aggregates EVERY slice's SSSOM set, so the
//! sensory SOSA rows actually live in `gmeow-observations.sssom.tsv` (the sensory
//! classes' precise SOSA alignments), not the slice-named `gmeow-sensory` file.
//! The six SOSA twins therefore scan the whole mapping corpus; the AFO twin reads
//! the sensory set's audio-feature rows directly.
//!
//! Migrated twins:
//!   - `test_sensor_mapped_to_sosa_sensor`                       → `sensor_mapped_to_sosa_sensor`
//!   - `test_sensor_platform_mapped_to_sosa_platform`            → `sensor_platform_mapped_to_sosa_platform`
//!   - `test_observable_property_mapped_to_sosa`                 → `observable_property_mapped_to_sosa`
//!   - `test_sensory_quantity_mapped_to_sosa_result`            → `sensory_quantity_mapped_to_sosa_result`
//!   - `test_sensory_property_mapped_to_sosa_observed_property` → `sensory_property_mapped_to_sosa_observed_property`
//!   - `test_platform_location_mapped_to_geo_location`          → `platform_location_mapped_to_geo_location`
//!   - `test_sensory_afo_mappings_exist`                        → `sensory_afo_mappings_exist`

mod conformance_support;
use conformance_support::*;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The `(subject_id, predicate_id, object_id)` CURIE rows of the WHOLE generated
/// mapping corpus (`generated/mappings/*.sssom.tsv`), skipping `#`-prefixed YAML
/// metadata lines and the TSV header. Mirrors `load_mappings()`, which aggregates
/// every slice's SSSOM set.
fn corpus_rows() -> BTreeSet<(String, String, String)> {
    let dir = repo_root().join("generated").join("mappings");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read dir {}: {e}", dir.display()))
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".sssom.tsv"))
        })
        .collect();
    assert!(
        !files.is_empty(),
        "no SSSOM artifacts under {} — the generated mapping corpus is empty",
        dir.display()
    );
    files.sort();
    let mut rows = BTreeSet::new();
    for path in files {
        rows.extend(sssom_rows_of(&path));
    }
    rows
}

/// The `(subject_id, predicate_id, object_id)` CURIE rows of a single SSSOM file.
fn sssom_rows_of(path: &std::path::Path) -> BTreeSet<(String, String, String)> {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let mut rows = BTreeSet::new();
    for line in text.lines() {
        if line.starts_with('#') || line.starts_with("subject_id") {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() >= 3 {
            rows.insert((cols[0].to_owned(), cols[1].to_owned(), cols[2].to_owned()));
        }
    }
    rows
}

/// Assert a `skos:closeMatch` alignment row is present in the corpus. Mirrors the
/// Python pattern: at least one mapping for `subject`, whose object is `object`,
/// carried by `skos:closeMatch`.
fn assert_close_match(rows: &BTreeSet<(String, String, String)>, subject: &str, object: &str) {
    let row = (
        subject.to_owned(),
        "skos:closeMatch".to_owned(),
        object.to_owned(),
    );
    assert!(
        rows.contains(&row),
        "expected mapping {subject} skos:closeMatch {object} in the generated SSSOM corpus"
    );
}

/// Twin of `test_sensor_mapped_to_sosa_sensor`.
#[test]
fn sensor_mapped_to_sosa_sensor() {
    assert_close_match(&corpus_rows(), "gmeow:Sensor", "sosa:Sensor");
}

/// Twin of `test_sensor_platform_mapped_to_sosa_platform`.
#[test]
fn sensor_platform_mapped_to_sosa_platform() {
    assert_close_match(&corpus_rows(), "gmeow:SensorPlatform", "sosa:Platform");
}

/// Twin of `test_observable_property_mapped_to_sosa`.
#[test]
fn observable_property_mapped_to_sosa() {
    assert_close_match(
        &corpus_rows(),
        "gmeow:ObservableProperty",
        "sosa:ObservableProperty",
    );
}

/// Twin of `test_sensory_quantity_mapped_to_sosa_result`.
#[test]
fn sensory_quantity_mapped_to_sosa_result() {
    assert_close_match(&corpus_rows(), "gmeow:SensoryQuantity", "sosa:Result");
}

/// Twin of `test_sensory_property_mapped_to_sosa_observed_property`.
#[test]
fn sensory_property_mapped_to_sosa_observed_property() {
    assert_close_match(
        &corpus_rows(),
        "gmeow:sensoryProperty",
        "sosa:observedProperty",
    );
}

/// Twin of `test_platform_location_mapped_to_geo_location`.
#[test]
fn platform_location_mapped_to_geo_location() {
    assert_close_match(&corpus_rows(), "gmeow:platformLocation", "geo:location");
}

/// Twin of `test_sensory_afo_mappings_exist`: the five sensory AFO/AFV audio-feature
/// alignments (authored as `eqSens001`–`eqSens005` `TermEquivalence` cells) are
/// present as compiled rows in the sensory SSSOM set — asserted over the generated
/// projection, not the volatile `equivalences.ttl` source.
#[test]
fn sensory_afo_mappings_exist() {
    let rows = sssom_rows_of(
        &repo_root()
            .join("generated")
            .join("mappings")
            .join("gmeow-sensory.sssom.tsv"),
    );
    let expected: [(&str, &str, &str); 5] = [
        (
            "gmeow:observablePropertyTimbre",
            "skos:closeMatch",
            "afo:AudioFeature",
        ),
        (
            "gmeow:observablePropertyTimbre",
            "skos:relatedMatch",
            "afv:TimbreDistribution",
        ),
        (
            "gmeow:observablePropertyLoudness",
            "skos:closeMatch",
            "afv:Loudness",
        ),
        (
            "gmeow:observablePropertyRoughness",
            "skos:closeMatch",
            "afv:Roughness",
        ),
        (
            "gmeow:observablePropertyTimingDeviation",
            "skos:relatedMatch",
            "afv:Onset",
        ),
    ];
    for (s, p, o) in expected {
        assert!(
            rows.contains(&(s.to_owned(), p.to_owned(), o.to_owned())),
            "expected sensory AFO row {s} {p} {o} in gmeow-sensory.sssom.tsv"
        );
    }
}
