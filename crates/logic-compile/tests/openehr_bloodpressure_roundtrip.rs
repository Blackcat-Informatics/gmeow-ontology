// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//
//! openEHR data-axis vertical: the blood-pressure round trip against the REAL GECCO
//! Blutdruck fixtures.
//!
//! This is the full-fixture sense of the section/retraction round trip. The conformance case
//! `correspondence/openehr-bloodpressure-section-retraction` executes the bounded, complete
//! three-edge source query class through get and candidate put; this module proves the larger
//! RM-slice + complement recovery against the actual vendored artifacts:
//!
//!   * `down_projection_is_non_destructive_on_rm_slice_and_fhir_lineage` — the down-projection
//!     `d` does not mutate the RM slice or the FHIR provenance lineage; it is purely additive;
//!   * `up_projection_recovers_the_complement_from_the_augmented_artifact` — the complement
//!     carrier embedded in `feeder_audit.original_content` (DV_PARSABLE, `text/turtle`) parses
//!     and canonicalizes (RDFC-1.0) byte-losslessly through the native RDF-1.2 parser, equal to
//!     the standalone `blood_pressure.complement.ttl`. This is a carrier round-trip check, NOT
//!     a proof of `u ∘ d = id`: the complement is authored to already equal itself, so this test
//!     alone cannot detect a `d` that silently dropped RM data;
//!   * `up_projection_reconstructs_source_from_rm_slice_and_complement` — the actual `u ∘ d = id`
//!     proof. It RE-LIFTS the RM slice: for each `logic:Quality` in the complement carrying a
//!     `gmeow:rmPath`, it parses the archetype-node token (`at0004`/`at0005`) out of the path,
//!     uses that token to locate the matching `items[…].value` `DV_QUANTITY` in
//!     `blood_pressure.augmented.json`, MINTS the asserted `gmeow:measuredValue` leaf from the
//!     live RM magnitude/units, unions it with the parsed complement dataset, canonicalizes, and
//!     asserts the result equals the canonicalization of the standalone golden
//!     `blood_pressure.source.ttl`, and additionally asserts the reconstruction strictly
//!     contains both the minted `gmeow:measuredValue` lines and the complement's YAMATO ladder
//!     lines (exactly two `measuredValue` triples). Because the minted leaves are read from the
//!     RM slice at test time (not copied from a pre-baked fixture), corrupting the RM magnitude
//!     in `blood_pressure.augmented.json` makes this test fail;
//!   * `seqpath_witness_and_complement_rmpath_reference_the_same_archetype_node` — ties the
//!     structural witness (the conformance case's `gm:SeqPath`, whose `at0004` step names the
//!     systolic archetype node) to the data witness (the complement's `gmeow:rmPath` literal),
//!     proving both proofs are anchored to the same RM coordinate.
//!
//! No openEHR/COMPOSITION parser is built: only narrow `serde_json` field access plus the
//! first-party `purrdf` parse + RDFC-1.0 canonicalization. The complement carries an
//! RDF-1.2 reifier triple term (`<<( … )>>`); the native parser admits it in full — there
//! is no subset path, parsing the whole complement is a hard requirement.

use std::path::{Path, PathBuf};

use purrdf::RdfDataset;

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
    let ds = purrdf::parse_dataset(turtle, "text/turtle", None)
        .expect("native parse of RDF-1.2 Turtle (full complement, triple term included)");
    purrdf::canonicalize(&ds).nquads
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

    // Carrier round trip: the embedded Turtle canonicalizes (RDFC-1.0) equal to the
    // standalone complement.ttl. This proves the complement carrier survives the native
    // RDF-1.2 parser byte-losslessly; it is NOT the `u ∘ d = id` proof (both sides are
    // authored to already be equal) — see
    // `up_projection_reconstructs_source_from_rm_slice_and_complement` below for the real
    // reconstruction against the RM slice.
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

/// A `logic:Quality` subject carrying a `gmeow:rmPath` witness, extracted from the
/// complement's canonical N-Quads text.
struct QualityWitness {
    /// The complement subject IRI, e.g. `<urn:gmeow:quality:sysBP>`.
    subject_iri: String,
    /// The full `gmeow:rmPath` literal string, e.g.
    /// `.../items[at0004]`.
    rm_path: String,
    /// The archetype-node token parsed out of `rm_path`'s trailing `items[atNNNN]` — the
    /// load-bearing structural witness that selects the RM item index (not a hardcoded guess).
    archetype_node: String,
}

/// Scan `nquads` (canonical N-Quads text) for every `gmeow:rmPath "..."` triple and return
/// the owning subject IRI, the raw rmPath literal, and the `atNNNN` archetype-node token
/// parsed out of the path's trailing `items[atNNNN]` segment.
fn quality_witnesses_from_nquads(nquads: &str) -> Vec<QualityWitness> {
    let predicate = "/gmeow/rmPath> \"";
    let mut out = Vec::new();
    for line in nquads.lines() {
        let Some(pred_at) = line.find(predicate) else {
            continue;
        };
        // Subject is the leading `<...>` term of the line.
        let subject_end = line.find("> ").expect("subject IRI term") + 1;
        let lit_start = pred_at + predicate.len();
        let rest = &line[lit_start..];
        let lit_end = rest.find('"').expect("closing quote of rmPath literal");
        let rm_path = rest[..lit_end].to_string();

        // Parse the `items[atNNNN]` token out of the path tail — the archetype-node witness.
        let marker = "items[";
        let idx = rm_path
            .rfind(marker)
            .unwrap_or_else(|| panic!("rmPath {rm_path:?} has no items[...] segment"));
        let after = &rm_path[idx + marker.len()..];
        let close = after
            .find(']')
            .unwrap_or_else(|| panic!("rmPath {rm_path:?} items[...] is unterminated"));
        let archetype_node = after[..close].to_string();

        out.push(QualityWitness {
            subject_iri: line[..subject_end].to_string(),
            rm_path,
            archetype_node,
        });
    }
    assert!(
        !out.is_empty(),
        "expected at least one gmeow:rmPath witness in the complement"
    );
    out
}

/// Locate the openEHR `HISTORY` items array in the augmented RM composition:
/// `content[0].data.events[0].data.items`.
fn rm_items(augmented: &serde_json::Value) -> &Vec<serde_json::Value> {
    augmented["content"][0]["data"]["events"][0]["data"]["items"]
        .as_array()
        .expect("RM items array")
}

/// Find the `DV_QUANTITY` item whose `archetype_node_id` equals `archetype_node`, and render
/// its `magnitude units` as a plain literal in the complement's own citation convention
/// (e.g. `"1.0 mm[Hg]"`, matching the reifier term in `blood_pressure.complement.ttl`).
fn measured_value_literal(augmented: &serde_json::Value, archetype_node: &str) -> String {
    let items = rm_items(augmented);
    let item = items
        .iter()
        .find(|it| it["archetype_node_id"] == archetype_node)
        .unwrap_or_else(|| panic!("no RM item with archetype_node_id {archetype_node:?}"));
    assert_eq!(item["value"]["_type"], "DV_QUANTITY");
    let magnitude = &item["value"]["magnitude"];
    let units = item["value"]["units"]
        .as_str()
        .expect("DV_QUANTITY units must be a string");
    format!("{magnitude} {units}")
}

/// Mint the Turtle text for the asserted `gmeow:measuredValue` leaves, one per witness, in
/// the same prefixes as the complement/golden fixtures.
fn mint_measured_value_turtle(
    witnesses: &[QualityWitness],
    augmented: &serde_json::Value,
) -> String {
    let mut ttl = String::from("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n");
    for w in witnesses {
        let literal = measured_value_literal(augmented, &w.archetype_node);
        ttl.push_str(&format!(
            "{} gmeow:measuredValue \"{literal}\" .\n",
            w.subject_iri
        ));
    }
    ttl
}

#[test]
fn up_projection_reconstructs_source_from_rm_slice_and_complement() {
    // S ∖ im(get): parse the standalone complement (the part the RM slice cannot hold).
    let complement_bytes = std::fs::read(fixtures_dir().join("blood_pressure.complement.ttl"))
        .expect("read complement.ttl");
    let complement_ds = purrdf::parse_dataset(&complement_bytes, "text/turtle", None)
        .expect("native parse of the complement");
    let complement_canon = purrdf::canonicalize(&complement_ds).nquads;

    // Derive the archetype-node witnesses from the complement itself — load-bearing: the
    // RM-item index is READ from gmeow:rmPath, never hardcoded.
    let witnesses = quality_witnesses_from_nquads(&complement_canon);
    assert_eq!(
        witnesses.len(),
        2,
        "expected exactly two rmPath witnesses (systolic + diastolic)"
    );
    for w in &witnesses {
        assert!(
            w.rm_path.contains(&format!("items[{}]", w.archetype_node)),
            "archetype_node {:?} must be parsed from its own rm_path {:?}",
            w.archetype_node,
            w.rm_path
        );
    }

    // Use each witness's archetype-node token to re-lift the DV_QUANTITY value out of the
    // RM slice — this is `get`, applied for real against the vendored augmented artifact.
    let augmented = read_json("blood_pressure.augmented.json");
    let minted_ttl = mint_measured_value_turtle(&witnesses, &augmented);
    let minted_ds = purrdf::parse_dataset(minted_ttl.as_bytes(), "text/turtle", None)
        .expect("native parse of the minted measuredValue leaves");

    // u(d(S)): union the re-lifted RM values with the parsed complement, canonicalize.
    let reconstructed = RdfDataset::union(&[complement_ds.as_ref(), minted_ds.as_ref()]);
    let reconstructed_canon = purrdf::canonicalize(&reconstructed).nquads;

    // The golden: the standalone canonical source object S.
    let source_bytes = std::fs::read(fixtures_dir().join("blood_pressure.source.ttl"))
        .expect("read blood_pressure.source.ttl");
    let source_ds = purrdf::parse_dataset(&source_bytes, "text/turtle", None)
        .expect("native parse of blood_pressure.source.ttl");
    let source_canon = purrdf::canonicalize(&source_ds).nquads;

    assert_eq!(
        reconstructed_canon, source_canon,
        "u(d(S)) must canonically equal the standalone source golden blood_pressure.source.ttl"
    );

    // Load-bearing structure: the reconstruction must carry BOTH the RM-derived measured
    // values AND the complement's YAMATO ladder — dropping either would desync it from the
    // golden above, but assert it directly too so the failure mode is legible.
    assert!(
        reconstructed_canon.contains("gmeow/measuredValue> \"1.0 mm[Hg]\""),
        "the RM-derived systolic measuredValue must be present in the reconstruction"
    );
    assert!(
        reconstructed_canon.contains("gmeow/measuredValue> \"60.0 mm[Hg]\""),
        "the RM-derived diastolic measuredValue must be present in the reconstruction"
    );
    assert!(reconstructed_canon.contains("/logic/qualityRole>"));
    assert!(reconstructed_canon.contains("/logic/genericQuality>"));
    // Count only ASSERTED measuredValue triples (predicate position of a top-level line),
    // not the one nested inside the reifier's quoted triple term `<<( ... measuredValue ... )>>`
    // — that occurrence is a citation, not an assertion, and must not be double-counted.
    let asserted_measured_value_count = reconstructed_canon
        .lines()
        .filter(|line| line.contains("gmeow/measuredValue> \"") && !line.contains("<<("))
        .count();
    assert_eq!(
        asserted_measured_value_count, 2,
        "exactly two asserted measuredValue triples: one per YAMATO quality"
    );
}

#[test]
fn seqpath_witness_and_complement_rmpath_reference_the_same_archetype_node() {
    // The structural witness: the conformance case's gm:SeqPath step naming the systolic
    // archetype node (see conformance/logic/cases/correspondence/
    // openehr-bloodpressure-section-retraction/input.logic.ttl, subject `ex:rmItemsAt0004`).
    let conformance_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../conformance/logic/cases/correspondence/openehr-bloodpressure-section-retraction/input.logic.ttl",
    );
    let conformance_ttl = std::fs::read_to_string(&conformance_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", conformance_path.display()));
    assert!(
        conformance_ttl.contains("at0004"),
        "the conformance case's SeqPath step must name the systolic archetype node at0004"
    );

    // The data witness: the complement's gmeow:rmPath literal for the systolic Quality.
    let complement = std::fs::read_to_string(fixtures_dir().join("blood_pressure.complement.ttl"))
        .expect("read complement.ttl");
    assert!(
        complement.contains("items[at0004]"),
        "the complement's systolic rmPath must name the same archetype node at0004"
    );

    // Both witnesses provably reference the same RM coordinate — the SeqPath structural
    // proof and the rmPath data proof are not independently free to disagree.
}
