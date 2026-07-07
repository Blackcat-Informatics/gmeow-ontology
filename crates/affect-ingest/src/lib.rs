// SPDX-License-Identifier: AGPL-3.0-only
//! # gmeow-affect-ingest — captured classifier output → attributed GMEOW evidence
//!
//! A HuggingFace text-classification output is **not** "the user is joyful". It is
//! *this model, at this revision, over this target span, emitted this label with
//! this score under this label set* — evidence, never inner-state truth. This
//! crate is the Rust-first producer that turns a **captured** run (a checked-in
//! JSON envelope, never on-gate inference) into attributed
//! [`gmeow:ModelInferenceRun`] + [`gmeow:AffectClassifierOutput`]
//! (+ [`gmeow:AffectiveClaim`]) RDF that passes the affect evidence-spine Stage-4
//! hard-fail gates (`crates/validate/tests/conformance_affect.rs`).
//!
//! ## The put leg — [`produce`]
//! `ClassifierRunCapture → Turtle`. All labels survive losslessly (one
//! `AffectClassifierOutput` per label, carrying the raw score + score semantics +
//! applied threshold), the run carries its full provenance, and a claim is
//! supported (never entailed) only where the ontology already authors a reviewed
//! `skos:closeMatch` from the external label to a canonical `gmeow:EmotionType`.
//! Minted node IRIs are a pure function of the recoverable capture content, so
//! `produce` is idempotent (same capture → byte-identical Turtle).
//!
//! ## The get leg — [`recover`]
//! `Turtle → ClassifierRunCapture`, authored **independently** of `produce` (it
//! walks the emitted graph shape; it is never `produce.invert()`), so the
//! round-trip `recover(produce(cap)) == canonicalize(cap)` is a real proof of
//! losslessness, not a tautology. [`canonicalize`] is the normal form the
//! round-trip compares against — it derives the fields that are
//! reconstructable-but-not-directly-stored (`function_to_apply` from the score
//! semantics, `return_all_scores` from label-set completeness) so nothing is
//! silently dropped to make the round-trip pass.
//!
//! ## GoEmotions is one instance of a general functor
//! The transform is parameterized by an [`IngestConfig`] — the registered label
//! set and the authored closeMatch cells, **loaded from the ontology bundle**
//! (single source of truth; no hardcoded label list). GoEmotions
//! (`SamLowe/roberta-base-go_emotions`) is the reference instance; SST-2 /
//! CardiffNLP / j-hartmann / zero-shot are a capture + config, not new code
//! (tracked in the follow-on issue).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use gmeow_math::{
    TripleIndex, first_iri, first_literal, has_type, index_graph, index_turtle, subjects,
};
use purrdf::{RdfDatasetBuilder, RdfLiteral, SerializeGraph, serialize_dataset};
use serde::{Deserialize, Serialize};

// ───────────────────────────── namespaces ──────────────────────────────────

/// Canonical GMEOW namespace.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// GoEmotions per-registry label prefix — external label identities live here,
/// NEVER under `gmeow:`, so a model label can never be mistaken for an emotion.
const GOEMOTIONS: &str = "https://blackcatinformatics.ca/gmeow-registry/goemotions/";
/// Label-set registry prefix.
const LABELSET: &str = "https://blackcatinformatics.ca/gmeow-registry/labelset/";
/// Base under which this producer mints run/output/claim/concluded nodes.
const INGEST_BASE: &str = "https://blackcatinformatics.ca/gmeow/ingest/goemotions/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
/// The `@x-gmeow-english` tag every localizable GMEOW literal carries (lang-tag
/// discipline) — matches the tag on the canonical emotion terms' own labels.
const GMEOW_ENGLISH: &str = "x-gmeow-english";

/// `gmeow:<local>`.
fn g(local: &str) -> String {
    format!("{GMEOW}{local}")
}

// ───────────────────────────── capture model ───────────────────────────────

/// What a classifier score MEANS — the open `gmeow:ScoreSemantics` vocabulary,
/// seeded here with the terms the affect module authors. A raw sigmoid/softmax
/// score is evidence strength, NEVER a calibrated confidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreSemantics {
    /// `gmeow:scoreSigmoid` — a per-label sigmoid activation (GoEmotions default).
    Sigmoid,
    /// `gmeow:scoreSoftmax` — a normalized softmax probability across labels.
    Softmax,
    /// `gmeow:scoreCalibratedProbability` — REQUIRES a `score_calibration` profile.
    CalibratedProbability,
    /// `gmeow:scoreLogit` — the raw pre-activation value (unbounded).
    Logit,
    /// `gmeow:scoreMargin` — a decision margin (unbounded).
    Margin,
}

impl ScoreSemantics {
    /// The `gmeow:ScoreSemantics` individual IRI this maps onto.
    fn iri(self) -> String {
        g(match self {
            ScoreSemantics::Sigmoid => "scoreSigmoid",
            ScoreSemantics::Softmax => "scoreSoftmax",
            ScoreSemantics::CalibratedProbability => "scoreCalibratedProbability",
            ScoreSemantics::Logit => "scoreLogit",
            ScoreSemantics::Margin => "scoreMargin",
        })
    }

    /// The activation / `functionToApply` a run with this semantics must have
    /// declared, when it is pinned by a bounded activation. `None` for the
    /// unbounded/derived semantics, where no single activation is implied.
    fn required_function(self) -> Option<&'static str> {
        match self {
            ScoreSemantics::Sigmoid => Some("sigmoid"),
            ScoreSemantics::Softmax => Some("softmax"),
            _ => None,
        }
    }

    /// `true` when a score under this semantics must lie in the unit interval.
    fn bounded_unit_interval(self) -> bool {
        matches!(
            self,
            ScoreSemantics::Sigmoid
                | ScoreSemantics::Softmax
                | ScoreSemantics::CalibratedProbability
        )
    }
}

/// The threshold policy that travels with a run — a single global 0.5 must NOT be
/// assumed canonical (GoEmotions documents per-label threshold optimization).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThresholdPolicy {
    /// One threshold applied to every label.
    Global { value: f64 },
    /// A per-label threshold map (a label with no entry is a hard fail).
    PerLabel { thresholds: BTreeMap<String, f64> },
}

impl ThresholdPolicy {
    fn threshold_for(&self, label: &str) -> Option<f64> {
        match self {
            ThresholdPolicy::Global { value } => Some(*value),
            ThresholdPolicy::PerLabel { thresholds } => thresholds.get(label).copied(),
        }
    }
}

/// One emitted `(label, score)` cell over a target. The score's *meaning* is the
/// run-level `score_semantics` (a logit-valued run sets `score_semantics = Logit`);
/// there is no separate per-output raw-logit field, because the ontology authors
/// no property to emit one — carrying an unpreservable logit would make the
/// losslessness round-trip a lie.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LabelScore {
    pub label: String,
    pub score: f64,
}

/// One classified target span and the model's per-label scores over it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetInput {
    /// The IRI of the classified span/document/segment (`gmeow:classifiedTarget`).
    pub target_iri: String,
    pub scores: Vec<LabelScore>,
}

/// A captured classifier run — the producer's input schema, carrying the full
/// run provenance hard-fail rule 1 demands. Captured once from a real model run
/// and checked in; never produced by on-gate inference (determinism/no-network).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClassifierRunCapture {
    pub model_identifier: String,
    /// The pinned model revision/commit — MANDATORY (a name without a revision is
    /// unreproducible: a hard fail).
    pub model_revision: String,
    pub model_framework: String,
    pub model_task: String,
    /// The activation the pipeline applied (`sigmoid`/`softmax`/…) — validated for
    /// consistency with `score_semantics`.
    pub function_to_apply: String,
    /// Whether the run emitted a score for every label (vs a truncated top-k).
    /// `true` ⇒ every target must carry the full registered label set, or the
    /// capture is a lossy subset masquerading as complete (a hard fail).
    pub return_all_scores: bool,
    /// The registered label set the run emitted over (its local name, e.g.
    /// `GoEmotions`); validated to match the [`IngestConfig`].
    pub label_set_id: String,
    pub score_semantics: ScoreSemantics,
    pub threshold_policy: ThresholdPolicy,
    pub targets: Vec<TargetInput>,
    /// REQUIRED iff `score_semantics == CalibratedProbability`: a raw score stored
    /// as a calibrated probability without this profile is a hard fail. Genuinely
    /// N/A (absent) for a raw sigmoid/softmax score.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_calibration: Option<String>,
    /// The pinned tokenizer revision, when the capture recorded it (a tokenizer
    /// change silently shifts scores). Genuinely-absent source data ⇒ optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokenizer_revision: Option<String>,
    /// The pinned label-set revision, when recorded (a zero-shot candidate set is
    /// part of run identity). Genuinely-absent source data ⇒ optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_set_revision: Option<String>,
}

// ───────────────────────────── errors ──────────────────────────────────────

/// Every way a capture is rejected BEFORE any RDF is emitted. No-optionality: a
/// malformed/under-specified capture is a hard fail in Rust, not a degraded
/// fallback and not something only the downstream SHACL would catch.
#[derive(Clone, Debug, PartialEq)]
pub enum IngestError {
    MissingModelIdentifier,
    /// Rule 7: a model name without a pinned revision.
    MissingModelRevision,
    /// Rule 4: a calibrated-probability score with no calibration profile.
    MissingScoreCalibration,
    /// `function_to_apply` disagrees with the declared `score_semantics`.
    ActivationMismatch {
        expected: String,
        found: String,
    },
    /// The capture declares a different label set than this config serves.
    LabelSetMismatch {
        expected: String,
        found: String,
    },
    NoTargets,
    /// A target with no scores.
    EmptyTarget {
        target: String,
    },
    /// Rule 2: a label not registered in the configured `gmeow:AffectLabelSet`.
    UnregisteredLabel {
        label: String,
    },
    /// Two rows for the same label under one target (ambiguous score).
    DuplicateLabel {
        target: String,
        label: String,
    },
    /// A target IRI that is empty / not an absolute IRI.
    InvalidTargetIri {
        target: String,
    },
    /// Two targets sharing one IRI.
    DuplicateTargetIri {
        target: String,
    },
    /// A NaN/±Inf score, or a bounded-semantics score outside `[0, 1]`.
    ScoreOutOfRange {
        target: String,
        label: String,
        score: f64,
    },
    /// `return_all_scores == true` but a target omits a registered label.
    IncompleteScores {
        target: String,
        missing: String,
    },
    /// `PerLabel` policy with no threshold for a label the run emitted.
    MissingThreshold {
        label: String,
    },
    /// The get leg could not reconstruct a required triple from the graph.
    MalformedGraph {
        detail: String,
    },
}

impl fmt::Display for IngestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IngestError::MissingModelIdentifier => {
                write!(f, "run must declare exactly one gmeow:modelIdentifier")
            }
            IngestError::MissingModelRevision => write!(
                f,
                "run must declare a pinned gmeow:modelRevision (a model name without a revision is unreproducible)"
            ),
            IngestError::MissingScoreCalibration => write!(
                f,
                "gmeow:scoreCalibratedProbability requires a gmeow:scoreCalibration profile"
            ),
            IngestError::ActivationMismatch { expected, found } => write!(
                f,
                "function_to_apply {found:?} disagrees with the declared score semantics (expected {expected:?})"
            ),
            IngestError::LabelSetMismatch { expected, found } => {
                write!(f, "capture label set {found:?} != configured {expected:?}")
            }
            IngestError::NoTargets => write!(f, "capture has no classified targets"),
            IngestError::EmptyTarget { target } => {
                write!(f, "target {target:?} carries no scores")
            }
            IngestError::UnregisteredLabel { label } => write!(
                f,
                "label {label:?} is not registered in a gmeow:AffectLabelSet"
            ),
            IngestError::DuplicateLabel { target, label } => {
                write!(f, "target {target:?} has duplicate label {label:?}")
            }
            IngestError::InvalidTargetIri { target } => {
                write!(f, "target IRI {target:?} is not a valid absolute IRI")
            }
            IngestError::DuplicateTargetIri { target } => {
                write!(f, "duplicate target IRI {target:?}")
            }
            IngestError::ScoreOutOfRange {
                target,
                label,
                score,
            } => write!(
                f,
                "score {score} for {label:?} on {target:?} is NaN/±Inf or outside the [0,1] range its semantics requires"
            ),
            IngestError::IncompleteScores { target, missing } => write!(
                f,
                "return_all_scores is true but target {target:?} omits registered label {missing:?}"
            ),
            IngestError::MissingThreshold { label } => {
                write!(
                    f,
                    "per-label threshold policy has no threshold for {label:?}"
                )
            }
            IngestError::MalformedGraph { detail } => {
                write!(f, "cannot recover capture from graph: {detail}")
            }
        }
    }
}

impl std::error::Error for IngestError {}

// ───────────────────────────── config ──────────────────────────────────────

/// A canonical `gmeow:EmotionType` an external label reviews onto by an authored
/// `skos:closeMatch` — the routing target for a supported affective claim.
#[derive(Clone, Debug, PartialEq)]
struct EmotionMatch {
    /// The canonical `gmeow:emotion*` IRI.
    #[allow(dead_code)]
    iri: String,
    /// Its `rdfs:label` word (e.g. `joy`) — sourced from the CANONICAL term, so
    /// the claim never smuggles the raw external label string.
    word: String,
}

/// The producer parameters: which label set, and the reviewed label→canonical
/// closeMatch cells — both loaded FROM the ontology bundle (single source of
/// truth). Never a hardcoded label list.
#[derive(Clone, Debug)]
pub struct IngestConfig {
    label_set_id: String,
    registry_prefix: String,
    /// Full IRIs of every registered label in the set.
    registered: BTreeSet<String>,
    /// Full label IRI → the `gmeow:EmotionType` it closeMatches (a subset — only
    /// the labels that review onto an emotion type; social/cognitive/`neutral`
    /// labels and non-`closeMatch` rungs are deliberately absent).
    emotion_close_match: BTreeMap<String, EmotionMatch>,
}

impl IngestConfig {
    /// Build the GoEmotions config by reading the registered labels and authored
    /// closeMatch cells straight from a compiled ontology bundle (`gmeow.gts`).
    pub fn goemotions_from_gts(bundle: &[u8]) -> Self {
        let graph = purrdf::gts::reader::read(bundle, false, None);
        Self::goemotions_from_index(&index_graph(&graph))
    }

    /// Build the GoEmotions config from an already-indexed graph (the shared
    /// entry point for the bundle path and the unit-test turtle path).
    pub fn goemotions_from_index(index: &TripleIndex) -> Self {
        let label_set_iri = format!("{LABELSET}GoEmotions");
        Self::from_label_set(index, "GoEmotions", &label_set_iri, GOEMOTIONS)
    }

    /// The generic loader — the functor's parameterization point. Any registered
    /// `gmeow:AffectLabelSet` yields a config by reading its members + closeMatch
    /// cells; new adapters differ only in these arguments + a capture.
    fn from_label_set(
        index: &TripleIndex,
        label_set_id: &str,
        label_set_iri: &str,
        registry_prefix: &str,
    ) -> Self {
        let member_of = g("memberOfLabelSet");
        let registered: BTreeSet<String> = subjects(index)
            .filter(|s| first_iri(index, s, &member_of).as_deref() == Some(label_set_iri))
            .cloned()
            .collect();

        let term_equivalence = g("TermEquivalence");
        let align_subject = g("alignSubject");
        let align_predicate = g("alignPredicate");
        let align_object = g("alignObject");
        let emotion_type = g("EmotionType");

        let mut emotion_close_match = BTreeMap::new();
        for cell in subjects(index).filter(|s| has_type(index, s, &term_equivalence)) {
            let (Some(subject), Some(predicate), Some(object)) = (
                first_iri(index, cell, &align_subject),
                first_iri(index, cell, &align_predicate),
                first_iri(index, cell, &align_object),
            ) else {
                continue;
            };
            // A supported "expresses" claim is honest only when the reviewed rung
            // is closeMatch (not broadMatch) AND the target is an EmotionType (not,
            // e.g., teleology's gmeow:Desire).
            if predicate == SKOS_CLOSE_MATCH
                && subject.starts_with(registry_prefix)
                && has_type(index, &object, &emotion_type)
            {
                let word = first_literal(index, &object, RDFS_LABEL)
                    .unwrap_or_else(|| local_name(&object).to_owned());
                emotion_close_match.insert(subject, EmotionMatch { iri: object, word });
            }
        }

        IngestConfig {
            label_set_id: label_set_id.to_owned(),
            registry_prefix: registry_prefix.to_owned(),
            registered,
            emotion_close_match,
        }
    }

    /// Full label IRI for a bare label local (`joy` → `gmeow-goemotions:joy`).
    fn label_iri(&self, label: &str) -> String {
        format!("{}{label}", self.registry_prefix)
    }

    /// The registered label locals, for tests/callers that enumerate the set.
    pub fn registered_labels(&self) -> impl Iterator<Item = &str> {
        self.registered
            .iter()
            .filter_map(|iri| iri.strip_prefix(self.registry_prefix.as_str()))
    }
}

// ───────────────────────────── put leg: produce ─────────────────────────────

/// Emit attributed GMEOW evidence Turtle for a captured classifier run.
///
/// Hard-fails (returns `Err`) on any rule-1/2/4/7 or integrity violation BEFORE
/// emitting a single triple. On success emits, deterministically and
/// idempotently: one `gmeow:ModelInferenceRun`; one `gmeow:AffectClassifierOutput`
/// per `(target, label)` (all labels → lossless); a supported
/// `gmeow:AffectiveClaim` for each above-threshold label that reviews onto a
/// `gmeow:EmotionType`; and a `gmeow:AffectEvaluationConcluded` for each target
/// where nothing crossed threshold ("checked and flat" ≠ "never checked").
pub fn produce(
    capture: &ClassifierRunCapture,
    config: &IngestConfig,
) -> Result<String, IngestError> {
    validate(capture, config)?;

    let run_iri = mint_run_iri(capture);
    let mut sink = Sink::default();

    // ── the run ──
    sink.iri(&run_iri, RDF_TYPE, &g("ModelInferenceRun"));
    sink.string(&run_iri, &g("modelIdentifier"), &capture.model_identifier);
    sink.string(&run_iri, &g("modelRevision"), &capture.model_revision);
    sink.string(&run_iri, &g("modelFramework"), &capture.model_framework);
    sink.string(&run_iri, &g("modelTask"), &capture.model_task);
    if let Some(tok) = &capture.tokenizer_revision {
        sink.string(&run_iri, &g("tokenizerRevision"), tok);
    }
    if let Some(ls) = &capture.label_set_revision {
        sink.string(&run_iri, &g("labelSetRevision"), ls);
    }

    let semantics_iri = capture.score_semantics.iri();
    for target in &capture.targets {
        sink.iri(&run_iri, &g("usedInput"), &target.target_iri);

        let mut any_crossed = false;
        for cell in &target.scores {
            let label_iri = config.label_iri(&cell.label);
            let threshold = capture
                .threshold_policy
                .threshold_for(&cell.label)
                .ok_or_else(|| IngestError::MissingThreshold {
                    label: cell.label.clone(),
                })?;
            let out_iri = mint_output_iri(&run_iri, &target.target_iri, &cell.label);

            sink.iri(&out_iri, RDF_TYPE, &g("AffectClassifierOutput"));
            sink.iri(&out_iri, &g("producedBy"), &run_iri);
            sink.iri(&out_iri, &g("vantage"), &run_iri);
            sink.iri(&out_iri, &g("classifiedTarget"), &target.target_iri);
            sink.iri(&out_iri, &g("emittedLabel"), &label_iri);
            sink.decimal(&out_iri, &g("classifierScore"), cell.score);
            sink.iri(&out_iri, &g("scoreSemantics"), &semantics_iri);
            sink.decimal(&out_iri, &g("thresholdApplied"), threshold);
            if let Some(cal) = &capture.score_calibration {
                sink.string(&out_iri, &g("scoreCalibration"), cal);
            }

            if cell.score >= threshold {
                any_crossed = true;
                // Claim boundary (rule 5): the output SUPPORTS a claim, never
                // asserts inner affect. Only route a claim where the ontology
                // authors a reviewed closeMatch to an EmotionType.
                if let Some(m) = config.emotion_close_match.get(&label_iri) {
                    let claim_iri = format!("{out_iri}/claim");
                    sink.iri(&out_iri, &g("supportsAffectiveClaim"), &claim_iri);
                    sink.iri(&claim_iri, RDF_TYPE, &g("AffectiveClaim"));
                    sink.iri(&claim_iri, &g("vantage"), &run_iri);
                    sink.iri(&claim_iri, &g("observedFeature"), &target.target_iri);
                    sink.lang(
                        &claim_iri,
                        RDFS_LABEL,
                        &format!("the text expresses {}", m.word),
                    );
                }
            }
        }

        // "Concluded and flat" is not "never checked": record the positive fact.
        if !any_crossed {
            let concluded_iri = mint_concluded_iri(&run_iri, &target.target_iri);
            sink.iri(&concluded_iri, RDF_TYPE, &g("AffectEvaluationConcluded"));
            sink.iri(&concluded_iri, &g("vantage"), &run_iri);
            sink.iri(&concluded_iri, &g("observedFeature"), &target.target_iri);
        }
    }

    Ok(sink.serialize())
}

/// All hard-fail checks, keyed to the affect design's rules. Runs to completion
/// before any emission (no-optionality: reject in Rust, not via a downstream
/// SHACL the producer's own output would have to be re-fed to).
fn validate(capture: &ClassifierRunCapture, config: &IngestConfig) -> Result<(), IngestError> {
    if capture.model_identifier.trim().is_empty() {
        return Err(IngestError::MissingModelIdentifier);
    }
    if capture.model_revision.trim().is_empty() {
        return Err(IngestError::MissingModelRevision); // rule 7
    }
    if capture.label_set_id != config.label_set_id {
        return Err(IngestError::LabelSetMismatch {
            expected: config.label_set_id.clone(),
            found: capture.label_set_id.clone(),
        });
    }
    if capture.score_semantics == ScoreSemantics::CalibratedProbability
        && capture
            .score_calibration
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(IngestError::MissingScoreCalibration); // rule 4
    }
    if let Some(expected) = capture.score_semantics.required_function()
        && capture.function_to_apply != expected
    {
        return Err(IngestError::ActivationMismatch {
            expected: expected.to_owned(),
            found: capture.function_to_apply.clone(),
        });
    }
    if capture.targets.is_empty() {
        return Err(IngestError::NoTargets);
    }

    let mut seen_targets = BTreeSet::new();
    for target in &capture.targets {
        if !is_absolute_iri(&target.target_iri) {
            return Err(IngestError::InvalidTargetIri {
                target: target.target_iri.clone(),
            });
        }
        if !seen_targets.insert(target.target_iri.clone()) {
            return Err(IngestError::DuplicateTargetIri {
                target: target.target_iri.clone(),
            });
        }
        if target.scores.is_empty() {
            return Err(IngestError::EmptyTarget {
                target: target.target_iri.clone(),
            });
        }

        let mut seen_labels = BTreeSet::new();
        for cell in &target.scores {
            let label_iri = config.label_iri(&cell.label);
            if !config.registered.contains(&label_iri) {
                return Err(IngestError::UnregisteredLabel {
                    label: cell.label.clone(),
                }); // rule 2
            }
            if !seen_labels.insert(cell.label.clone()) {
                return Err(IngestError::DuplicateLabel {
                    target: target.target_iri.clone(),
                    label: cell.label.clone(),
                });
            }
            if !cell.score.is_finite()
                || (capture.score_semantics.bounded_unit_interval()
                    && !(0.0..=1.0).contains(&cell.score))
            {
                return Err(IngestError::ScoreOutOfRange {
                    target: target.target_iri.clone(),
                    label: cell.label.clone(),
                    score: cell.score,
                });
            }
            // Rule: a per-label policy must cover every label the run emitted.
            if capture
                .threshold_policy
                .threshold_for(&cell.label)
                .is_none()
            {
                return Err(IngestError::MissingThreshold {
                    label: cell.label.clone(),
                });
            }
        }

        // return_all_scores ⇒ the full registered set must be present, else this
        // "complete" capture is silently a truncated subset (lossy).
        if capture.return_all_scores {
            for reg in config.registered_labels() {
                if !seen_labels.contains(reg) {
                    return Err(IngestError::IncompleteScores {
                        target: target.target_iri.clone(),
                        missing: reg.to_owned(),
                    });
                }
            }
        }
    }
    Ok(())
}

// ───────────────────────────── deterministic IRIs ───────────────────────────

/// The run node — a pure function of the recoverable identity of the run
/// (model + revision + the sorted set of classified targets). Same capture ⇒
/// same IRI ⇒ byte-identical Turtle (idempotent); no `NOW()`, no randomness.
fn mint_run_iri(capture: &ClassifierRunCapture) -> String {
    let mut targets: Vec<&str> = capture
        .targets
        .iter()
        .map(|t| t.target_iri.as_str())
        .collect();
    targets.sort_unstable();
    let key = format!(
        "{}\n{}\n{}",
        capture.model_identifier,
        capture.model_revision,
        targets.join("\n")
    );
    format!("{INGEST_BASE}run-{}", fnv1a_hex(&key))
}

fn mint_output_iri(run_iri: &str, target_iri: &str, label: &str) -> String {
    format!(
        "{run_iri}/out-{}",
        fnv1a_hex(&format!("{target_iri}\n{label}"))
    )
}

fn mint_concluded_iri(run_iri: &str, target_iri: &str) -> String {
    format!("{run_iri}/concluded-{}", fnv1a_hex(target_iri))
}

/// FNV-1a 64-bit, hex. A stable, portable, deterministic content hash (no crate,
/// no platform-dependent `DefaultHasher`) used only to disambiguate minted IRIs.
fn fnv1a_hex(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

// ───────────────────────────── helpers ─────────────────────────────────────

/// A light absolute-IRI check: non-empty, no whitespace, and a scheme separator
/// (`scheme:`). Enough to reject the empty/garbage target IRIs a capture must not
/// carry; the codec rejects anything that is still malformed downstream.
fn is_absolute_iri(iri: &str) -> bool {
    !iri.is_empty()
        && !iri.chars().any(char::is_whitespace)
        && iri
            .split_once(':')
            .is_some_and(|(scheme, _)| !scheme.is_empty() && scheme.chars().all(|c| c != '/'))
}

/// The local name after the last `/` or `#` — a fallback claim word when a
/// canonical term has no `rdfs:label`.
fn local_name(iri: &str) -> &str {
    iri.rsplit(['/', '#']).next().unwrap_or(iri)
}

/// Format an f64 as an `xsd:decimal` lexical (`0.84`, `1.0`) — never exponent
/// form (out-of-range/NaN are rejected upstream, so inputs are tame).
fn format_decimal(value: f64) -> String {
    let s = format!("{value}");
    if s.contains('.') || s.contains(['e', 'E']) {
        s
    } else {
        format!("{s}.0")
    }
}

/// A thin accumulator over `purrdf::RdfDatasetBuilder` that emits deterministic,
/// canonical Turtle (the codec sorts; no manual pre-sort needed). Mirrors the
/// projection `TripleSink` at `crates/logic-compile/src/projections/rdf.rs`.
#[derive(Default)]
struct Sink {
    builder: RdfDatasetBuilder,
}

impl Sink {
    fn iri(&mut self, s: &str, p: &str, o: &str) {
        let s = self.builder.intern_iri(s);
        let p = self.builder.intern_iri(p);
        let o = self.builder.intern_iri(o);
        self.builder.push_quad(s, p, o, None);
    }

    fn lit(&mut self, s: &str, p: &str, lit: RdfLiteral) {
        let s = self.builder.intern_iri(s);
        let p = self.builder.intern_iri(p);
        let o = self.builder.intern_literal(lit);
        self.builder.push_quad(s, p, o, None);
    }

    fn string(&mut self, s: &str, p: &str, value: &str) {
        self.lit(s, p, RdfLiteral::simple(value));
    }

    fn decimal(&mut self, s: &str, p: &str, value: f64) {
        self.lit(
            s,
            p,
            RdfLiteral::typed(format_decimal(value), XSD_DECIMAL.to_owned()),
        );
    }

    fn lang(&mut self, s: &str, p: &str, text: &str) {
        self.lit(s, p, RdfLiteral::language_tagged(text, GMEOW_ENGLISH));
    }

    fn serialize(self) -> String {
        let dataset = self
            .builder
            .freeze()
            .expect("well-formed triple set freezes");
        let bytes = serialize_dataset(
            dataset.as_ref(),
            "text/turtle",
            SerializeGraph::DefaultGraph,
        )
        .expect("turtle serialization");
        String::from_utf8(bytes).expect("utf-8 turtle")
    }
}

// ───────────────────────────── get leg: recover ─────────────────────────────

impl ScoreSemantics {
    /// Inverse of [`ScoreSemantics::iri`] — the get leg's independent reading of
    /// the emitted `gmeow:scoreSemantics` object.
    fn from_iri(iri: &str) -> Option<Self> {
        Some(match iri.strip_prefix(GMEOW)? {
            "scoreSigmoid" => ScoreSemantics::Sigmoid,
            "scoreSoftmax" => ScoreSemantics::Softmax,
            "scoreCalibratedProbability" => ScoreSemantics::CalibratedProbability,
            "scoreLogit" => ScoreSemantics::Logit,
            "scoreMargin" => ScoreSemantics::Margin,
            _ => return None,
        })
    }
}

/// Reconstruct the capture from an emitted evidence graph — the get leg.
///
/// Authored **independently** of [`produce`]: it walks the graph shape (the one
/// `gmeow:ModelInferenceRun`, its `gmeow:AffectClassifierOutput`s via
/// `gmeow:producedBy`), never by inverting the put leg's control flow. Claims and
/// `AffectEvaluationConcluded` are GMEOW's *derived* interpretation, not capture
/// data, so `recover` reads only the run + outputs. The result equals
/// [`canonicalize`] of the original capture — the losslessness proof.
pub fn recover(turtle: &str, config: &IngestConfig) -> Result<ClassifierRunCapture, IngestError> {
    let index =
        index_turtle(turtle.as_bytes()).map_err(|detail| IngestError::MalformedGraph { detail })?;

    let run = sole_subject_of_type(&index, &g("ModelInferenceRun"))?;
    let model_identifier = required_literal(&index, &run, &g("modelIdentifier"))?;
    let model_revision = required_literal(&index, &run, &g("modelRevision"))?;
    let model_framework = required_literal(&index, &run, &g("modelFramework"))?;
    let model_task = required_literal(&index, &run, &g("modelTask"))?;
    let tokenizer_revision = first_literal(&index, &run, &g("tokenizerRevision"));
    let label_set_revision = first_literal(&index, &run, &g("labelSetRevision"));

    let output_type = g("AffectClassifierOutput");
    let produced_by = g("producedBy");
    let mut per_target: BTreeMap<String, Vec<LabelScore>> = BTreeMap::new();
    let mut score_semantics: Option<ScoreSemantics> = None;
    let mut score_calibration: Option<String> = None;
    let mut thresholds: BTreeMap<String, f64> = BTreeMap::new();

    for out in subjects(&index).filter(|s| has_type(&index, s, &output_type)) {
        if first_iri(&index, out, &produced_by).as_deref() != Some(run.as_str()) {
            continue;
        }
        let target = required_iri(&index, out, &g("classifiedTarget"))?;
        let label_iri = required_iri(&index, out, &g("emittedLabel"))?;
        let label = label_iri
            .strip_prefix(config.registry_prefix.as_str())
            .ok_or_else(|| {
                malformed(format!(
                    "emitted label {label_iri:?} outside the registry prefix"
                ))
            })?
            .to_owned();
        let score = required_decimal(&index, out, &g("classifierScore"))?;

        let sem = ScoreSemantics::from_iri(&required_iri(&index, out, &g("scoreSemantics"))?)
            .ok_or_else(|| malformed("unknown gmeow:scoreSemantics"))?;
        match score_semantics {
            Some(prev) if prev != sem => {
                return Err(malformed("outputs disagree on score semantics"));
            }
            _ => score_semantics = Some(sem),
        }

        let threshold = required_decimal(&index, out, &g("thresholdApplied"))?;
        if let Some(prev) = thresholds.insert(label.clone(), threshold)
            && prev != threshold
        {
            return Err(malformed(format!("label {label:?} carries two thresholds")));
        }
        if let Some(cal) = first_literal(&index, out, &g("scoreCalibration")) {
            score_calibration = Some(cal);
        }

        per_target
            .entry(target)
            .or_default()
            .push(LabelScore { label, score });
    }

    let score_semantics = score_semantics.ok_or_else(|| malformed("run has no outputs"))?;

    let mut targets: Vec<TargetInput> = per_target
        .into_iter()
        .map(|(target_iri, mut scores)| {
            scores.sort_by(|a, b| a.label.cmp(&b.label));
            TargetInput { target_iri, scores }
        })
        .collect();
    targets.sort_by(|a, b| a.target_iri.cmp(&b.target_iri));

    Ok(ClassifierRunCapture {
        model_identifier,
        model_revision,
        model_framework,
        model_task,
        function_to_apply: derived_function(score_semantics),
        return_all_scores: covers_full_set(&targets, config),
        label_set_id: config.label_set_id.clone(),
        score_semantics,
        threshold_policy: policy_from_thresholds(thresholds),
        targets,
        score_calibration,
        tokenizer_revision,
        label_set_revision,
    })
}

/// The normal form a capture takes on the round-trip — everything [`recover`]
/// can reconstruct, normalized the same way. Fields not stored as their own
/// triple are *derived from what is stored* (`function_to_apply` ⇐ score
/// semantics; `return_all_scores` ⇐ label-set completeness), so equality with
/// `recover(produce(cap))` is a real losslessness assertion, not the result of
/// quietly discarding data.
pub fn canonicalize(capture: &ClassifierRunCapture, config: &IngestConfig) -> ClassifierRunCapture {
    let mut targets: Vec<TargetInput> = capture
        .targets
        .iter()
        .map(|t| {
            let mut scores: Vec<LabelScore> = t
                .scores
                .iter()
                .map(|s| LabelScore {
                    label: s.label.clone(),
                    score: renormalize_decimal(s.score),
                })
                .collect();
            scores.sort_by(|a, b| a.label.cmp(&b.label));
            TargetInput {
                target_iri: t.target_iri.clone(),
                scores,
            }
        })
        .collect();
    targets.sort_by(|a, b| a.target_iri.cmp(&b.target_iri));

    let mut thresholds: BTreeMap<String, f64> = BTreeMap::new();
    for t in &targets {
        for s in &t.scores {
            if let Some(v) = capture.threshold_policy.threshold_for(&s.label) {
                thresholds.insert(s.label.clone(), v);
            }
        }
    }

    ClassifierRunCapture {
        model_identifier: capture.model_identifier.clone(),
        model_revision: capture.model_revision.clone(),
        model_framework: capture.model_framework.clone(),
        model_task: capture.model_task.clone(),
        function_to_apply: derived_function(capture.score_semantics),
        return_all_scores: covers_full_set(&targets, config),
        label_set_id: config.label_set_id.clone(),
        score_semantics: capture.score_semantics,
        threshold_policy: policy_from_thresholds(thresholds),
        targets,
        score_calibration: capture.score_calibration.clone(),
        tokenizer_revision: capture.tokenizer_revision.clone(),
        label_set_revision: capture.label_set_revision.clone(),
    }
}

/// `function_to_apply` implied by a bounded activation semantics, `""` otherwise
/// (the unbounded/derived semantics pin no single activation).
fn derived_function(sem: ScoreSemantics) -> String {
    sem.required_function().unwrap_or("").to_owned()
}

/// `true` iff every target carries the full registered label set — the emitted
/// witness of `return_all_scores`.
fn covers_full_set(targets: &[TargetInput], config: &IngestConfig) -> bool {
    targets.iter().all(|t| {
        let present: BTreeSet<&str> = t.scores.iter().map(|s| s.label.as_str()).collect();
        config.registered_labels().all(|r| present.contains(r))
    })
}

/// Collapse a per-label threshold map to `Global` when uniform, else `PerLabel`
/// — the normal form both legs agree on.
fn policy_from_thresholds(thresholds: BTreeMap<String, f64>) -> ThresholdPolicy {
    let mut values = thresholds.values().copied();
    match values.next() {
        Some(first) if thresholds.values().all(|v| *v == first) => {
            ThresholdPolicy::Global { value: first }
        }
        _ => ThresholdPolicy::PerLabel { thresholds },
    }
}

/// Round a score through the emitted `xsd:decimal` lexical, so a capture value
/// compares equal to the value the get leg parses back from the graph.
fn renormalize_decimal(value: f64) -> f64 {
    format_decimal(value).parse().unwrap_or(value)
}

fn malformed(detail: impl Into<String>) -> IngestError {
    IngestError::MalformedGraph {
        detail: detail.into(),
    }
}

/// Exactly one subject of `class`, else a malformed-graph error.
fn sole_subject_of_type(index: &TripleIndex, class: &str) -> Result<String, IngestError> {
    let mut found = subjects(index).filter(|s| has_type(index, s, class));
    let first = found
        .next()
        .ok_or_else(|| malformed(format!("no subject of type {class}")))?
        .clone();
    if found.next().is_some() {
        return Err(malformed(format!("more than one subject of type {class}")));
    }
    Ok(first)
}

fn required_literal(index: &TripleIndex, s: &str, p: &str) -> Result<String, IngestError> {
    first_literal(index, s, p).ok_or_else(|| malformed(format!("{s} missing literal {p}")))
}

fn required_iri(index: &TripleIndex, s: &str, p: &str) -> Result<String, IngestError> {
    first_iri(index, s, p).ok_or_else(|| malformed(format!("{s} missing IRI {p}")))
}

fn required_decimal(index: &TripleIndex, s: &str, p: &str) -> Result<f64, IngestError> {
    required_literal(index, s, p)?
        .trim()
        .parse()
        .map_err(|_| malformed(format!("{s} {p} is not a decimal")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal ontology fixture: a two-label GoEmotions set (`joy` mapped to an
    /// EmotionType, `neutral` unmapped) with the closeMatch cell authored the way
    /// `mappings/equivalences.ttl` authors it.
    const ONTO: &str = concat!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
        "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
        "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow-goemotions:neutral a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
        "gmeow:eq1 a gmeow:TermEquivalence ; gmeow:alignSubject gmeow-goemotions:joy ; gmeow:alignPredicate skos:closeMatch ; gmeow:alignObject gmeow:emotionJoy .\n",
    );

    fn config() -> IngestConfig {
        let index = index_turtle(ONTO.as_bytes()).expect("index onto fixture");
        IngestConfig::goemotions_from_index(&index)
    }

    fn capture() -> ClassifierRunCapture {
        ClassifierRunCapture {
            model_identifier: "SamLowe/roberta-base-go_emotions".to_owned(),
            model_revision: "a1b2c3d4e5f6".to_owned(),
            model_framework: "transformers".to_owned(),
            model_task: "text-classification".to_owned(),
            function_to_apply: "sigmoid".to_owned(),
            return_all_scores: true,
            label_set_id: "GoEmotions".to_owned(),
            score_semantics: ScoreSemantics::Sigmoid,
            threshold_policy: ThresholdPolicy::Global { value: 0.5 },
            targets: vec![TargetInput {
                target_iri: "https://example.org/affect/chunk-1".to_owned(),
                scores: vec![
                    LabelScore {
                        label: "joy".to_owned(),
                        score: 0.84,
                    },
                    LabelScore {
                        label: "neutral".to_owned(),
                        score: 0.10,
                    },
                ],
            }],
            score_calibration: None,
            tokenizer_revision: Some("tok-9".to_owned()),
            label_set_revision: None,
        }
    }

    #[test]
    fn sample_fixture_deserializes_with_28_labels() {
        let json = include_str!("../fixtures/goemotions-sample.json");
        let cap: ClassifierRunCapture = serde_json::from_str(json).expect("valid fixture");
        assert_eq!(
            cap.model_revision,
            "d75048347613a25d77de8cf6412eaae9fa7b26be"
        );
        assert_eq!(cap.score_semantics, ScoreSemantics::Sigmoid);
        assert!(cap.return_all_scores);
        assert_eq!(cap.targets.len(), 1);
        assert_eq!(
            cap.targets[0].scores.len(),
            28,
            "GoEmotions emits 28 labels"
        );
    }

    #[test]
    fn config_loads_labels_and_only_closematch_emotiontypes() {
        let c = config();
        let mut labels: Vec<&str> = c.registered_labels().collect();
        labels.sort_unstable();
        assert_eq!(labels, vec!["joy", "neutral"]);
        // joy reviews onto an EmotionType; neutral does not.
        assert!(c.emotion_close_match.contains_key(&c.label_iri("joy")));
        assert!(!c.emotion_close_match.contains_key(&c.label_iri("neutral")));
        assert_eq!(c.emotion_close_match[&c.label_iri("joy")].word, "joy");
    }

    #[test]
    fn produce_emits_run_all_labels_and_a_mapped_claim() {
        let ttl = produce(&capture(), &config()).expect("produce");
        assert!(ttl.contains("ModelInferenceRun"));
        assert!(ttl.contains("SamLowe/roberta-base-go_emotions"));
        // lossless: an output for BOTH labels, incl. the below-threshold neutral.
        assert!(ttl.contains("gmeow-registry/goemotions/joy"));
        assert!(ttl.contains("gmeow-registry/goemotions/neutral"));
        // the claim references the CANONICAL word, not the raw label string source.
        assert!(ttl.contains("the text expresses joy"));
        assert!(ttl.contains("AffectiveClaim"));
        // rule 5: the output never carries emotionType directly.
        assert!(!ttl.contains("gmeow:emotionType"));
    }

    #[test]
    fn produce_is_idempotent() {
        let (cap, cfg) = (capture(), config());
        assert_eq!(produce(&cap, &cfg).unwrap(), produce(&cap, &cfg).unwrap());
    }

    #[test]
    fn round_trip_recover_produce_is_identity() {
        let (cap, cfg) = (capture(), config());
        let ttl = produce(&cap, &cfg).unwrap();
        // recover is authored independently of produce; the round-trip is the
        // losslessness proof, compared against the canonical normal form.
        assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
    }

    #[test]
    fn round_trip_multi_target() {
        let cfg = config();
        let mut cap = capture();
        cap.threshold_policy = ThresholdPolicy::PerLabel {
            thresholds: BTreeMap::from([("joy".to_owned(), 0.4), ("neutral".to_owned(), 0.6)]),
        };
        cap.targets.push(TargetInput {
            target_iri: "https://example.org/affect/chunk-2".to_owned(),
            scores: vec![
                LabelScore {
                    label: "neutral".to_owned(),
                    score: 0.72,
                },
                LabelScore {
                    label: "joy".to_owned(),
                    score: 0.20,
                },
            ],
        });
        let ttl = produce(&cap, &cfg).unwrap();
        assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
    }

    #[test]
    fn recover_hard_fails_on_missing_required_triple() {
        let (cap, cfg) = (capture(), config());
        let ttl = produce(&cap, &cfg).unwrap();
        // drop the mandatory modelRevision line: the graph is no longer a
        // well-formed run and the get leg must refuse it.
        let broken: String = ttl
            .lines()
            .filter(|l| !l.contains("modelRevision"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(matches!(
            recover(&broken, &cfg),
            Err(IngestError::MalformedGraph { .. })
        ));
    }

    #[test]
    fn concluded_when_nothing_crosses_threshold() {
        let mut cap = capture();
        for cell in &mut cap.targets[0].scores {
            cell.score = 0.10; // everything sub-threshold
        }
        let ttl = produce(&cap, &config()).unwrap();
        assert!(ttl.contains("AffectEvaluationConcluded"));
        assert!(!ttl.contains("AffectiveClaim"));
    }

    #[test]
    fn hard_fail_missing_revision() {
        let mut cap = capture();
        cap.model_revision = "  ".to_owned();
        assert_eq!(
            produce(&cap, &config()),
            Err(IngestError::MissingModelRevision)
        );
    }

    #[test]
    fn hard_fail_unregistered_label() {
        let mut cap = capture();
        cap.return_all_scores = false;
        cap.targets[0].scores = vec![LabelScore {
            label: "notALabel".to_owned(),
            score: 0.9,
        }];
        assert_eq!(
            produce(&cap, &config()),
            Err(IngestError::UnregisteredLabel {
                label: "notALabel".to_owned()
            })
        );
    }

    #[test]
    fn hard_fail_calibrated_without_calibration() {
        let mut cap = capture();
        cap.score_semantics = ScoreSemantics::CalibratedProbability;
        cap.function_to_apply = "sigmoid".to_owned(); // n/a for calibrated → no activation check
        assert_eq!(
            produce(&cap, &config()),
            Err(IngestError::MissingScoreCalibration)
        );
    }

    #[test]
    fn hard_fail_score_out_of_range_and_nan() {
        let mut cap = capture();
        cap.targets[0].scores[0].score = 1.5;
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::ScoreOutOfRange { .. })
        ));
        cap.targets[0].scores[0].score = f64::NAN;
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::ScoreOutOfRange { .. })
        ));
    }

    #[test]
    fn hard_fail_duplicate_label() {
        let mut cap = capture();
        cap.return_all_scores = false;
        cap.targets[0].scores = vec![
            LabelScore {
                label: "joy".to_owned(),
                score: 0.8,
            },
            LabelScore {
                label: "joy".to_owned(),
                score: 0.7,
            },
        ];
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::DuplicateLabel { .. })
        ));
    }

    #[test]
    fn hard_fail_invalid_and_duplicate_target_iri() {
        let mut cap = capture();
        cap.targets[0].target_iri = "not an iri".to_owned();
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::InvalidTargetIri { .. })
        ));

        let mut cap = capture();
        let dup = cap.targets[0].clone();
        cap.targets.push(dup);
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::DuplicateTargetIri { .. })
        ));
    }

    #[test]
    fn hard_fail_incomplete_when_return_all_scores() {
        let mut cap = capture();
        // drop neutral while claiming completeness
        cap.targets[0].scores = vec![LabelScore {
            label: "joy".to_owned(),
            score: 0.8,
        }];
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::IncompleteScores { .. })
        ));
    }

    #[test]
    fn hard_fail_activation_mismatch() {
        let mut cap = capture();
        cap.function_to_apply = "softmax".to_owned();
        assert!(matches!(
            produce(&cap, &config()),
            Err(IngestError::ActivationMismatch { .. })
        ));
    }
}
