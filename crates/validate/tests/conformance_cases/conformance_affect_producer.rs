// SPDX-License-Identifier: AGPL-3.0-only
//! Whole-ontology SHACL conformance for the `gmeow-affect-ingest` PRODUCER.
//!
//! The executable acceptance check on the production surface: EVERY real captured
//! classifier output (GoEmotions / SST-2 / CardiffNLP / j-hartmann), run through
//! [`produce`], validates clean against the SAME unmodified whole-ontology shapes
//! corpus (`whole_shapes()`) the affect evidence-spine twin
//! (`conformance_affect.rs`) uses — no parallel laxer path. It also proves the
//! blind `recover ∘ produce = id` round-trip end-to-end on each real fixture, that
//! tampering the emitted graph fires the exact Stage-4 shape, and that the producer
//! hard-fails in Rust on the rule-2 registration the fixture-only SHACL (which only
//! sees the output graph, not the typed label registrations) cannot itself catch.
//! The run-scoped zero-shot adapter (NLI entailment, an in-graph candidate set) is
//! exercised by the `zeroshot_*` cases below.

use crate::conformance_support::*;

use std::fs;

use gmeow_affect_ingest::{
    ClassifierRunCapture, IngestConfig, IngestError, LabelScore, ScoreSemantics, canonicalize,
    produce, recover,
};
use gmeow_math::index_turtle;

/// A statically-registered reference adapter: a `gmeow:AffectLabelSet` id, its real
/// captured fixture, and whether its labels review onto emotion types (so the
/// producer routes a supported claim) or are sentiment/social labels (evidence
/// only, never an expresses-claim).
struct Adapter {
    label_set_id: &'static str,
    fixture: &'static str,
    /// A canonical emotion word a claim must gloss, or `None` for the
    /// sentiment/social adapters that route NO expresses-claim.
    expected_claim_word: Option<&'static str>,
    /// `true` for a single-label (`gmeow:decisionArgmax`) set, which mints a
    /// `gmeow:AffectDecision` per target; `false` for the multi-label GoEmotions.
    mints_decision: bool,
}

const ADAPTERS: &[Adapter] = &[
    Adapter {
        label_set_id: "GoEmotions",
        fixture: "goemotions-sample.json",
        expected_claim_word: Some("joy"),
        mints_decision: false,
    },
    Adapter {
        label_set_id: "Ekman7",
        fixture: "ekman7-sample.json",
        expected_claim_word: Some("joy"),
        mints_decision: true,
    },
    Adapter {
        label_set_id: "SST2",
        fixture: "sst2-sample.json",
        expected_claim_word: None,
        mints_decision: true,
    },
    Adapter {
        label_set_id: "CardiffTweetEval",
        fixture: "cardiff-sample.json",
        expected_claim_word: None,
        mints_decision: true,
    },
];

/// Build a producer config straight from the authored affect slice sources
/// (`module.ttl` registers the label sets; `mappings/equivalences.ttl` authors the
/// reviewed `closeMatch` cells) — the single source of truth, read the same way the
/// CLI reads it from the compiled bundle.
fn affect_config(label_set_id: &str) -> IngestConfig {
    let root = repo_root();
    let module = fs::read_to_string(root.join("slices/core/affect/module.ttl"))
        .expect("read affect module.ttl");
    let equivalences =
        fs::read_to_string(root.join("slices/core/affect/mappings/equivalences.ttl"))
            .expect("read affect equivalences.ttl");
    let combined = format!("{module}\n{equivalences}");
    let index = index_turtle(combined.as_bytes()).expect("index affect ontology");
    IngestConfig::config_for_label_set(&index, label_set_id).expect("config for label set")
}

/// A real captured classifier run (`crates/affect-ingest/fixtures/...`).
fn fixture(name: &str) -> ClassifierRunCapture {
    let root = repo_root();
    let json = fs::read_to_string(root.join("crates/affect-ingest/fixtures").join(name))
        .unwrap_or_else(|e| panic!("read fixture {name}: {e}"));
    serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize fixture {name}: {e}"))
}

#[gmeow_test_batch_macros::batch_test]
fn every_adapter_output_conforms_and_is_lossless() {
    for adapter in ADAPTERS {
        let cfg = affect_config(adapter.label_set_id);
        let cap = fixture(adapter.fixture);
        let ttl =
            produce(&cap, &cfg).unwrap_or_else(|e| panic!("produce {}: {e}", adapter.label_set_id));

        // The real output validates clean against the UNMODIFIED whole-ontology
        // shapes — the same corpus that guards the hand-authored fixtures.
        Case::inline(ttl.clone()).run();

        // Lossless: exactly one AffectClassifierOutput per (target, label) survives.
        let expected_outputs: usize = cap.targets.iter().map(|t| t.scores.len()).sum();
        assert_eq!(
            ttl.matches("AffectClassifierOutput").count(),
            expected_outputs,
            "{}: an output per emitted label",
            adapter.label_set_id
        );

        // Claim routing depends on the reviewed rung: emotion labels gloss a claim
        // from the CANONICAL term; sentiment/social labels route NONE.
        match adapter.expected_claim_word {
            Some(word) => assert!(
                ttl.contains(&format!("the text expresses {word}")),
                "{}: expected an emotion claim for {word:?}",
                adapter.label_set_id
            ),
            None => assert!(
                !ttl.contains("the text expresses"),
                "{}: sentiment labels must route NO expresses-claim",
                adapter.label_set_id
            ),
        }
        // Rule 5: the output never directly asserts inner affect.
        assert!(
            !ttl.contains("gmeow:emotionType"),
            "{}",
            adapter.label_set_id
        );

        // A single-label (argmax) set records the model's decision as evidence
        // (gmeow:AffectDecision); a multi-label set mints none.
        assert_eq!(
            ttl.contains("AffectDecision"),
            adapter.mints_decision,
            "{}: AffectDecision presence must match the set's decision rule",
            adapter.label_set_id
        );
    }
}

/// A single-label (argmax) adapter with everything below threshold records the
/// model's argmax as a conforming `gmeow:AffectDecision` (crossedThreshold false)
/// — it does NOT fall back to `gmeow:AffectEvaluationConcluded` (that is the
/// multi-label idiom). The faithful argmax-under-threshold representation.
#[gmeow_test_batch_macros::batch_test]
fn argmax_sub_threshold_emits_conforming_decision_not_concluded() {
    let cfg = affect_config("Ekman7");
    let mut cap = fixture("ekman7-sample.json");
    // A non-uniform distribution summing to 1 with a UNIQUE max at 0.45 (< 0.5):
    // every label sub-threshold, but the argmax is unambiguous.
    let dist = [0.45, 0.20, 0.15, 0.08, 0.05, 0.04, 0.03];
    for target in &mut cap.targets {
        for (i, score) in target.scores.iter_mut().enumerate() {
            score.score = dist[i];
        }
    }
    let ttl = produce(&cap, &cfg).expect("produce sub-threshold argmax");
    assert!(ttl.contains("AffectDecision"), "the model still decided");
    assert!(
        !ttl.contains("AffectEvaluationConcluded"),
        "an argmax set is not 'concluded flat' — it decided a winner"
    );
    assert!(
        !ttl.contains("the text expresses"),
        "no claim below threshold"
    );
    // and the decision node validates against the unmodified shapes.
    Case::inline(ttl).run();
}

/// The Rust-only exclusivity guards on the REAL Ekman7 producer config — the
/// fixture-scoped SHACL cannot see the score/threshold arithmetic these enforce.
#[gmeow_test_batch_macros::batch_test]
fn producer_hard_fails_exclusivity_guards_on_real_config() {
    let cfg = affect_config("Ekman7");

    // >1 crossing over the single-label set: two Ekman labels both above threshold.
    let mut cap = fixture("ekman7-sample.json");
    cap.return_all_scores = false;
    cap.targets[0].scores = vec![
        LabelScore {
            label: "ekmanAnger".to_owned(),
            score: 0.60,
        },
        LabelScore {
            label: "ekmanJoy".to_owned(),
            score: 0.60,
        },
    ];
    assert!(
        matches!(
            produce(&cap, &cfg),
            Err(IngestError::ExclusivityViolation { .. })
        ),
        "two crossings over an exclusive set must hard-fail"
    );

    // A sigmoid (multi-label) semantics over the exclusive Ekman7 set.
    let mut cap = fixture("ekman7-sample.json");
    cap.score_semantics = ScoreSemantics::Sigmoid;
    cap.function_to_apply = "sigmoid".to_owned();
    assert!(
        matches!(
            produce(&cap, &cfg),
            Err(IngestError::ScoreSemanticsDecisionMismatch { .. })
        ),
        "a sigmoid over an argmax set must hard-fail"
    );

    // An off-simplex softmax distribution (does not sum to 1).
    let mut cap = fixture("ekman7-sample.json");
    for score in &mut cap.targets[0].scores {
        score.score = 0.10; // 7 × 0.10 = 0.70 ≠ 1
    }
    assert!(
        matches!(
            produce(&cap, &cfg),
            Err(IngestError::NonNormalizedExclusiveScores { .. })
        ),
        "an off-simplex softmax over an exclusive set must hard-fail"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn every_adapter_blind_round_trips() {
    for adapter in ADAPTERS {
        let cfg = affect_config(adapter.label_set_id);
        let cap = fixture(adapter.fixture);
        let ttl = produce(&cap, &cfg).expect("produce");
        // The losslessness acceptance criterion, end-to-end on each real captured
        // output: recover is authored independently, never produce.invert().
        assert_eq!(
            recover(&ttl, &cfg).expect("recover"),
            canonicalize(&cap, &cfg),
            "{}: blind round-trip",
            adapter.label_set_id
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn all_sub_threshold_emits_conforming_evaluation_concluded() {
    let cfg = affect_config("GoEmotions");
    let mut cap = fixture("goemotions-sample.json");
    for target in &mut cap.targets {
        for score in &mut target.scores {
            score.score = 0.10; // nothing crosses the 0.5 threshold
        }
    }
    let ttl = produce(&cap, &cfg).expect("produce all-sub-threshold");
    // "Concluded and flat" is a positive, queryable fact — not "never checked".
    assert!(ttl.contains("AffectEvaluationConcluded"));
    assert!(!ttl.contains("AffectiveClaim"));
    // and it still validates against the unmodified shapes.
    Case::inline(ttl).run();
}

#[gmeow_test_batch_macros::batch_test]
fn tampered_output_missing_revision_fires_the_stage4_shape() {
    let cfg = affect_config("GoEmotions");
    let ttl = produce(&fixture("goemotions-sample.json"), &cfg).expect("produce");
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

#[gmeow_test_batch_macros::batch_test]
fn every_adapter_hard_fails_in_rust_on_rule2_and_rule7() {
    for adapter in ADAPTERS {
        let cfg = affect_config(adapter.label_set_id);

        // Rule 7 (missing pinned revision) — caught in Rust before any emission.
        let mut cap = fixture(adapter.fixture);
        cap.model_revision = String::new();
        assert!(
            matches!(produce(&cap, &cfg), Err(IngestError::MissingModelRevision)),
            "{}: rule 7",
            adapter.label_set_id
        );

        // Rule 2 (unregistered label) — the fixture-only SHACL never sees the
        // label's AffectClassifierLabel typing, so the guard lives in Rust.
        let mut cap = fixture(adapter.fixture);
        cap.return_all_scores = false;
        cap.targets[0].scores[0].label = "notARegisteredLabel".to_owned();
        assert!(
            matches!(
                produce(&cap, &cfg),
                Err(IngestError::UnregisteredLabel { .. })
            ),
            "{}: rule 2",
            adapter.label_set_id
        );
    }
}

#[gmeow_test_batch_macros::batch_test]
fn zeroshot_run_scoped_output_conforms_and_round_trips() {
    let root = repo_root();
    let json = fs::read_to_string(root.join("crates/affect-ingest/fixtures/zeroshot-sample.json"))
        .expect("read zeroshot fixture");
    let cap: ClassifierRunCapture = serde_json::from_str(&json).expect("deserialize zeroshot");
    // The candidate set is declared per run, not read from a static AffectLabelSet.
    let cfg = IngestConfig::run_scoped_from_capture(&cap).expect("run-scoped config");
    let ttl = produce(&cap, &cfg).expect("produce zeroshot");

    // Validates clean against the UNMODIFIED whole-ontology shapes — the minted
    // run-scoped candidate set + labels are honestly registered in-graph.
    Case::inline(ttl.clone()).run();

    // The new NLI entailment score semantics + run-scoped provenance are emitted.
    assert!(ttl.contains("scoreEntailment"), "entailment semantics");
    assert!(
        ttl.contains("hypothesisTemplate"),
        "run-scoped hypothesis template"
    );
    assert!(ttl.contains("AffectLabelSet"), "in-graph candidate set");

    // Evidence only: a run-scoped prompt candidate has no pre-reviewed closeMatch,
    // so the claim/evidence boundary holds — NO auto-claim.
    assert!(
        !ttl.contains("the text expresses"),
        "no auto-claim for zero-shot"
    );

    // Lossless: one output per (target, candidate).
    let expected: usize = cap.targets.iter().map(|t| t.scores.len()).sum();
    assert_eq!(
        ttl.matches("AffectClassifierOutput").count(),
        expected,
        "an output per candidate per target"
    );

    // Blind round-trip on the run-scoped path: recover reads the candidate set +
    // hypothesis template back from the evidence graph.
    assert_eq!(
        recover(&ttl, &cfg).expect("recover"),
        canonicalize(&cap, &cfg),
        "zero-shot blind round-trip"
    );
}

#[gmeow_test_batch_macros::batch_test]
fn zeroshot_hard_fails_without_run_scoped_provenance() {
    let root = repo_root();
    let json = fs::read_to_string(root.join("crates/affect-ingest/fixtures/zeroshot-sample.json"))
        .expect("read zeroshot fixture");
    let cap: ClassifierRunCapture = serde_json::from_str(&json).expect("deserialize zeroshot");
    let cfg = IngestConfig::run_scoped_from_capture(&cap).expect("run-scoped config");

    // Strip the hypothesis template: an entailment run without it is a hard fail.
    let mut broken = cap.clone();
    broken.hypothesis_template = None;
    assert!(matches!(
        produce(&broken, &cfg),
        Err(IngestError::MissingHypothesisTemplate)
    ));
}

#[gmeow_test_batch_macros::batch_test]
fn hand_authored_example_still_conforms() {
    // The pre-existing curated docs example is a distinct, minimal illustration
    // (not a competing producer). Pin it against the shapes so it cannot silently
    // drift out of validity as the corpus evolves.
    Case::repo_path("slices/core/affect/examples/goemotions-run.ttl").run();
}
