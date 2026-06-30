// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//
//! F6 — openEHR data-axis vertical: the blood-pressure round trip against the REAL
//! GECCO Blutdruck fixtures.
//!
//! This is the *data* sense of the section/retraction round trip (the conformance case
//! `correspondence/openehr-bloodpressure-section-retraction` is the *structural* sense).
//! It proves, against the actual vendored artifacts, that the down-projection `d` is
//! non-destructive on the RM slice + FHIR lineage, and that `u` recovers the GMEOW
//! complement byte-for-byte up to RDF canonical isomorphism (RDFC-1.0):
//!
//!   * the systolic `DV_QUANTITY` (magnitude 1.0 mm[Hg]) is byte-identical between
//!     `blood_pressure.source.json` and `blood_pressure.augmented.json`, as is the FHIR
//!     `originating_system_item_ids` lineage — `d` only *added* the complement;
//!   * the complement embedded in `feeder_audit.original_content` (DV_PARSABLE,
//!     `text/turtle`) canonicalizes equal to the standalone `blood_pressure.complement.ttl`.
//!
//! No openEHR/COMPOSITION parser is built: only narrow `serde_json` field access plus the
//! first-party `gmeow_rdf` parse + RDFC-1.0 canonicalization. The complement carries an
//! RDF-1.2 reifier triple term (`<<( … )>>`); the native parser admits it in full — there
//! is no subset path, parsing the whole complement is a hard requirement.

use std::path::{Path, PathBuf};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/APPLIED_CATEGORY_THEORY/fixtures")
}

fn read_json(name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// The systolic `DV_QUANTITY` element value: `content[0].data.events[0].data.items[0].value`.
fn systolic_value(composition: &serde_json::Value) -> &serde_json::Value {
    &composition["content"][0]["data"]["events"][0]["data"]["items"][0]["value"]
}

fn canonical_nquads(turtle: &[u8]) -> String {
    let ds = gmeow_rdf::parse_dataset(turtle, "text/turtle", None)
        .expect("native parse of RDF-1.2 Turtle (full complement, triple term included)");
    gmeow_rdf::canonicalize(&ds).nquads
}

#[test]
fn down_projection_is_non_destructive_on_rm_slice_and_fhir_lineage() {
    let source = read_json("blood_pressure.source.json");
    let augmented = read_json("blood_pressure.augmented.json");

    // The load-bearing RM leaf round-trips byte-identically: same systolic DV_QUANTITY.
    let src_sys = systolic_value(&source);
    let aug_sys = systolic_value(&augmented);
    assert_eq!(
        aug_sys, src_sys,
        "down-projection must not mutate the systolic DV_QUANTITY"
    );
    assert_eq!(aug_sys["_type"], "DV_QUANTITY");
    assert_eq!(aug_sys["units"], "mm[Hg]");
    assert_eq!(
        aug_sys["magnitude"].as_f64(),
        Some(1.0),
        "systolic magnitude must be the byte-preserved 1.0 mm[Hg]"
    );

    // The FHIR provenance lineage (the reason this case is a triangle) is preserved.
    let src_fhir = &source["feeder_audit"]["originating_system_item_ids"][0];
    let aug_fhir = &augmented["feeder_audit"]["originating_system_item_ids"][0];
    assert_eq!(
        aug_fhir, src_fhir,
        "FHIR originating_system_item_ids lineage must be preserved verbatim"
    );
    assert_eq!(
        aug_fhir["id"], "Observation/816ddebd-ef90-4d6a-9c97-cba47eafb292/_history/1",
        "the FHIR logical id must survive the down-projection"
    );
    // The source carried no complement; the augmentation is purely additive.
    assert!(
        source["feeder_audit"].get("original_content").is_none(),
        "the vendored source must not already carry an original_content complement"
    );
}

#[test]
fn up_projection_recovers_the_complement_from_the_augmented_artifact() {
    let augmented = read_json("blood_pressure.augmented.json");

    // The in-band complement carrier: feeder_audit.original_content (DV_PARSABLE turtle).
    let original_content = &augmented["feeder_audit"]["original_content"];
    assert_eq!(
        original_content["_type"], "DV_PARSABLE",
        "the complement must ride in a DV_PARSABLE carrier"
    );
    assert_eq!(
        original_content["formalism"], "text/turtle",
        "the complement formalism must be text/turtle"
    );
    let embedded = original_content["value"]
        .as_str()
        .expect("original_content.value must be a string");

    // u recovers the complement: the embedded Turtle canonicalizes (RDFC-1.0) equal to the
    // standalone complement.ttl — the section/retraction recovery, against the real bytes.
    let embedded_canon = canonical_nquads(embedded.as_bytes());
    let standalone = std::fs::read(fixtures_dir().join("blood_pressure.complement.ttl"))
        .expect("read complement.ttl");
    let standalone_canon = canonical_nquads(&standalone);
    assert_eq!(
        embedded_canon, standalone_canon,
        "the embedded complement must canonically equal blood_pressure.complement.ttl"
    );

    // Spot-check the recovery carries the YAMATO ladder + the RDF-1.2 reifier identity —
    // exactly the S∖im(get) the RM slice cannot hold.
    assert!(embedded_canon.contains("/logic/qualityRole>"));
    assert!(embedded_canon.contains("/logic/genericQuality>"));
    assert!(
        embedded_canon.contains("/logic/reifies> <<("),
        "the RDF-1.2 reifier triple term must survive canonicalization"
    );
}
