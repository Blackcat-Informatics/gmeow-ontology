// SPDX-License-Identifier: AGPL-3.0-only
//! Whole-ontology SHACL conformance for the `gmeow-affect-ingest` PRODUCER.
//!
//! The executable acceptance check on the production surface: the real captured
//! GoEmotions output, run through [`produce`], validates clean against the SAME
//! unmodified whole-ontology shapes corpus (`whole_shapes()`) the affect
//! evidence-spine twin (`conformance_affect.rs`) uses — no parallel laxer path.
//! It also proves the blind `recover ∘ produce = id` round-trip end-to-end on the
//! real fixture, that tampering the emitted graph fires the exact Stage-4 shape,
//! and that the producer hard-fails in Rust on the rule-2 registration the
//! fixture-only SHACL (which only sees the output graph, not the typed label
//! registrations) cannot itself catch.

mod conformance_support;
use conformance_support::*;

use std::fs;

use gmeow_affect_ingest::{
    ClassifierRunCapture, IngestConfig, IngestError, canonicalize, produce, recover,
};
use gmeow_math::index_turtle;

/// Build the GoEmotions producer config straight from the authored affect slice
/// sources (`module.ttl` registers the 28 labels; `mappings/equivalences.ttl`
/// authors the reviewed `closeMatch` cells) — the single source of truth, read
/// the same way the CLI reads it from the compiled bundle.
fn goemotions_config() -> IngestConfig {
    let root = repo_root();
    let module = fs::read_to_string(root.join("slices/core/affect/module.ttl"))
        .expect("read affect module.ttl");
    let equivalences =
        fs::read_to_string(root.join("slices/core/affect/mappings/equivalences.ttl"))
            .expect("read affect equivalences.ttl");
    let combined = format!("{module}\n{equivalences}");
    let index = index_turtle(combined.as_bytes()).expect("index affect ontology");
    IngestConfig::goemotions_from_index(&index)
}

/// The real captured GoEmotions run (`crates/affect-ingest/fixtures/...`).
fn fixture() -> ClassifierRunCapture {
    let root = repo_root();
    let json =
        fs::read_to_string(root.join("crates/affect-ingest/fixtures/goemotions-sample.json"))
            .expect("read goemotions fixture");
    serde_json::from_str(&json).expect("deserialize goemotions fixture")
}

#[test]
fn producer_output_conforms_and_is_lossless() {
    let cfg = goemotions_config();
    let cap = fixture();
    let ttl = produce(&cap, &cfg).expect("produce over real fixture");

    // The real output validates clean against the UNMODIFIED whole-ontology shapes.
    Case::inline(ttl.clone()).run();

    // Lossless: an AffectClassifierOutput for EVERY emitted label survives.
    for score in &cap.targets[0].scores {
        assert!(
            ttl.contains(&format!("gmeow-registry/goemotions/{}", score.label)),
            "lossless ingest dropped label {:?}",
            score.label
        );
    }
    // External labels live under the per-registry prefix, never canonical gmeow:.
    assert!(ttl.contains("gmeow-registry/goemotions/joy"));

    // Claim routing: joy (0.82) and surprise (0.55) cross threshold AND carry an
    // authored closeMatch to a gmeow:EmotionType → each supports a claim.
    assert!(ttl.contains("the text expresses joy"));
    assert!(ttl.contains("the text expresses surprise"));
    // gratitude (0.90) crosses threshold but is a social label with no emotion
    // closeMatch → evidence survives as an output, but NO expresses-claim.
    assert!(!ttl.contains("the text expresses gratitude"));
    // Rule 5: the output never directly asserts inner affect.
    assert!(!ttl.contains("gmeow:emotionType"));
}

#[test]
fn blind_round_trip_holds_on_real_fixture() {
    let cfg = goemotions_config();
    let cap = fixture();
    let ttl = produce(&cap, &cfg).expect("produce");
    // The losslessness acceptance criterion, exercised end-to-end on the real
    // captured output: recover is authored independently, never produce.invert().
    assert_eq!(
        recover(&ttl, &cfg).expect("recover"),
        canonicalize(&cap, &cfg)
    );
}

#[test]
fn all_sub_threshold_emits_conforming_evaluation_concluded() {
    let cfg = goemotions_config();
    let mut cap = fixture();
    for score in &mut cap.targets[0].scores {
        score.score = 0.10; // nothing crosses the 0.5 threshold
    }
    let ttl = produce(&cap, &cfg).expect("produce all-sub-threshold");
    // "Concluded and flat" is a positive, queryable fact — not "never checked".
    assert!(ttl.contains("AffectEvaluationConcluded"));
    assert!(!ttl.contains("AffectiveClaim"));
    // and it still validates against the unmodified shapes.
    Case::inline(ttl).run();
}

#[test]
fn tampered_output_missing_revision_fires_the_stage4_shape() {
    let cfg = goemotions_config();
    let ttl = produce(&fixture(), &cfg).expect("produce");
    // Drop the mandatory modelRevision from the PRODUCER's exact output: the same
    // ModelInferenceRunShape that guards the hand-written fixtures must bite here.
    let tampered: String = ttl
        .lines()
        .filter(|line| !line.contains("modelRevision"))
        .collect::<Vec<_>>()
        .join("\n");
    Case::inline(tampered)
        .fails()
        .violations(&["pinned gmeow:modelRevision"])
        .run();
}

#[test]
fn producer_hard_fails_in_rust_on_rule2_and_rule7() {
    let cfg = goemotions_config();

    // Rule 7 (missing pinned revision) — caught in Rust before any emission.
    let mut cap = fixture();
    cap.model_revision = String::new();
    assert!(matches!(
        produce(&cap, &cfg),
        Err(IngestError::MissingModelRevision)
    ));

    // Rule 2 (unregistered label) — the fixture-only SHACL never sees the label's
    // AffectClassifierLabel typing, so the no-optionality guard lives in Rust.
    let mut cap = fixture();
    cap.return_all_scores = false;
    cap.targets[0].scores[0].label = "notARealGoEmotionsLabel".to_owned();
    assert!(matches!(
        produce(&cap, &cfg),
        Err(IngestError::UnregisteredLabel { .. })
    ));
}

#[test]
fn hand_authored_example_still_conforms() {
    // F2 reconciliation: the pre-existing curated docs example is a distinct,
    // minimal illustration (not a competing producer). Pin it against the shapes
    // so it cannot silently drift out of validity as the corpus evolves.
    Case::repo_path("slices/core/affect/examples/goemotions-run.ttl").run();
}
