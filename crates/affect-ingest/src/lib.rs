// SPDX-License-Identifier: AGPL-3.0-only
//! # gmeow-affect-ingest — captured classifier output → attributed GMEOW evidence
//!
//! A HuggingFace text-classification output is **not** "the user is joyful". It is
//! *this model, at this revision, over this target span, emitted this label with
//! this score under this label set* — evidence, never inner-state truth. This
//! crate is the Rust-first producer that turns a **captured** run (a checked-in
//! JSON envelope, never on-gate inference) into attributed
//! `gmeow:ModelInferenceRun` + `gmeow:AffectClassifierOutput`
//! (+ `gmeow:AffectiveClaim`) RDF that passes the affect evidence-spine Stage-4
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
//! ## Every adapter is one instance of a general functor
//! The transform is parameterized by an [`IngestConfig`] — the registered label
//! set and the authored closeMatch cells, **loaded from the ontology bundle**
//! (single source of truth; no hardcoded label list). The registry namespace and
//! the mint base are DERIVED from the label set's registered members, so a new
//! adapter is a capture + a `label_set_id`, never new code. GoEmotions
//! (`SamLowe/roberta-base-go_emotions`, sigmoid), SST-2 and CardiffNLP (softmax
//! sentiment), and j-hartmann Ekman-7 (softmax emotion) are all statically
//! registered instances dispatched by [`config_for_capture`]; the run-scoped
//! zero-shot adapter (`facebook/bart-large-mnli`, NLI entailment) declares its
//! candidate set per run instead of pointing at a static `gmeow:AffectLabelSet`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use gmeow_math::{
    TripleIndex, all_iris, first_iri, first_literal, has_type, index_graph, index_turtle, subjects,
};
use purrdf::{RdfDatasetBuilder, RdfLiteral, SerializeGraph, serialize_dataset};
use serde::{Deserialize, Serialize};

// ───────────────────────────── namespaces ──────────────────────────────────

/// Canonical GMEOW namespace.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// Label-set registry prefix — each `gmeow:AffectLabelSet`'s IRI is `{LABELSET}{id}`.
const LABELSET: &str = "https://blackcatinformatics.ca/gmeow-registry/labelset/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// Tolerance on the simplex (partition-of-unity) constraint for a softmax
/// distribution over an exclusive label set: `|Σ scores − 1| ≤ SIMPLEX_EPS`. Loose
/// enough to admit a probability distribution rounded to a few decimals (the
/// small, ≤~10-label exclusive sets), tight enough that a genuine off-simplex
/// vector — a sigmoid/multi-label distribution mislabeled softmax, or a truncated
/// set — deviates by whole tenths and is caught. NOT a magic constant: it is the
/// declared rounding budget of a captured categorical distribution.
const SIMPLEX_EPS: f64 = 1e-3;
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
    /// `gmeow:scoreEntailment` — an NLI entailment probability from a zero-shot run
    /// (per run-scoped candidate + hypothesis template). Bounded to `[0, 1]`.
    Entailment,
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
            ScoreSemantics::Entailment => "scoreEntailment",
            ScoreSemantics::Logit => "scoreLogit",
            ScoreSemantics::Margin => "scoreMargin",
        })
    }

    /// The activation / `functionToApply` a run with this semantics must have
    /// declared, when it is pinned by a bounded activation. `None` for the
    /// unbounded/derived semantics (entailment/logit/margin), where no single
    /// activation is implied.
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
                | ScoreSemantics::Entailment
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
    /// The NLI hypothesis template a zero-shot run instantiated per candidate
    /// (`This text expresses {}.`). Present iff this is a run-scoped (zero-shot)
    /// capture; absent for a fixed-label classifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hypothesis_template: Option<String>,
    /// The run-scoped candidate label surfaces a zero-shot run classified against —
    /// part of the run identity, NOT a static `gmeow:AffectLabelSet`. Present iff
    /// this is a zero-shot capture; the labels in `targets` must be exactly this set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_labels: Option<Vec<String>>,
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
    /// A zero-shot (entailment) run without its NLI hypothesis template — the
    /// template is part of the run's identity, so its absence is a hard fail.
    MissingHypothesisTemplate,
    /// A zero-shot (entailment) run without a declared run-scoped candidate set.
    MissingCandidateLabels,
    /// The get leg could not reconstruct a required triple from the graph.
    MalformedGraph {
        detail: String,
    },
    /// A statically-registered `gmeow:AffectLabelSet` declares no
    /// `gmeow:labelSetDecision` — its exclusivity is unknown, so the producer
    /// cannot judge whether more than one crossing is a violation (a hard fail).
    MissingLabelSetDecision {
        label_set: String,
    },
    /// More than one label crossed its claim threshold over a single-label
    /// (`gmeow:decisionArgmax`) set — an exclusive set admits at most one claim.
    ExclusivityViolation {
        target: String,
        labels: Vec<String>,
    },
    /// An exact top-score tie over an exclusive set — `gmeow:fnArgmax` has no
    /// faithful single winner (a near-tie is recorded via `gmeow:decisionMargin`).
    AmbiguousArgmax {
        target: String,
        labels: Vec<String>,
    },
    /// A softmax distribution over an exclusive set whose scores do not sum to 1
    /// (within `SIMPLEX_EPS`) — not a valid categorical distribution on the simplex.
    NonNormalizedExclusiveScores {
        target: String,
        sum: f64,
    },
    /// The run's `gmeow:scoreSemantics` implies a decision rule (softmax → argmax,
    /// sigmoid → independent-threshold) inconsistent with the set's declared
    /// `gmeow:labelSetDecision`.
    ScoreSemanticsDecisionMismatch {
        implied: String,
        declared: String,
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
            IngestError::MissingHypothesisTemplate => write!(
                f,
                "a zero-shot (gmeow:scoreEntailment) run must declare its gmeow:hypothesisTemplate (part of run identity)"
            ),
            IngestError::MissingCandidateLabels => write!(
                f,
                "a zero-shot (gmeow:scoreEntailment) run must declare its run-scoped candidate label set"
            ),
            IngestError::MalformedGraph { detail } => {
                write!(f, "cannot recover capture from graph: {detail}")
            }
            IngestError::MissingLabelSetDecision { label_set } => write!(
                f,
                "label set {label_set:?} declares no gmeow:labelSetDecision (single-label vs multi-label) — its exclusivity is unknown"
            ),
            IngestError::ExclusivityViolation { target, labels } => write!(
                f,
                "more than one label crossed threshold over an exclusive (single-label) set on target {target:?}: {labels:?}"
            ),
            IngestError::AmbiguousArgmax { target, labels } => write!(
                f,
                "exact top-score tie over an exclusive set on target {target:?} — no faithful argmax winner: {labels:?}"
            ),
            IngestError::NonNormalizedExclusiveScores { target, sum } => write!(
                f,
                "softmax scores over an exclusive set on target {target:?} sum to {sum} (not 1 within the simplex tolerance)"
            ),
            IngestError::ScoreSemanticsDecisionMismatch { implied, declared } => write!(
                f,
                "score semantics implies a {implied} label set but the set declares {declared} (a softmax over a multi-label set, or a sigmoid over an exclusive set)"
            ),
        }
    }
}

impl std::error::Error for IngestError {}

// ───────────────────────────── config ──────────────────────────────────────

/// The decision structure a `gmeow:AffectLabelSet` carries — the categorical
/// (partition/simplex) vs Bernoulli-product (hypercube) duality, read from the
/// set's `gmeow:labelSetDecision`. `Unknown` is the run-scoped/zero-shot stance
/// only: a static set with no declared rule is a hard fail, never `Unknown`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelSetDecision {
    /// `gmeow:decisionArgmax` — single-label / mutually-exclusive / winner-take-all.
    Argmax,
    /// `gmeow:decisionIndependentThreshold` — multi-label / independent Bernoullis.
    IndependentThreshold,
    /// Run-scoped (zero-shot) candidate set: no reviewed decision rule.
    Unknown,
}

impl LabelSetDecision {
    /// A human name for the rule, for the mismatch error message.
    fn label(self) -> String {
        match self {
            LabelSetDecision::Argmax => "single-label (argmax)".to_owned(),
            LabelSetDecision::IndependentThreshold => {
                "multi-label (independent-threshold)".to_owned()
            }
            LabelSetDecision::Unknown => "unknown".to_owned(),
        }
    }
}

/// The decision rule a normalized score semantics ENTAILS (the ontology's
/// `gmeow:impliesLabelSetDecision`): softmax → argmax, sigmoid → independent
/// threshold. The unbounded / per-hypothesis semantics carry no implication.
fn implied_decision(sem: ScoreSemantics) -> Option<LabelSetDecision> {
    match sem {
        ScoreSemantics::Softmax => Some(LabelSetDecision::Argmax),
        ScoreSemantics::Sigmoid => Some(LabelSetDecision::IndependentThreshold),
        _ => None,
    }
}

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
/// truth). Never a hardcoded label list. Every adapter (GoEmotions, SST-2,
/// CardiffNLP, j-hartmann, …) is one instance, distinguished only by which
/// registered `gmeow:AffectLabelSet` it serves.
#[derive(Clone, Debug)]
pub struct IngestConfig {
    label_set_id: String,
    /// The registry namespace every label in the set lives under (derived from the
    /// registered members, never hardcoded) — e.g. `…/gmeow-registry/hf/`.
    registry_prefix: String,
    /// Base under which this adapter mints run/output/claim/concluded nodes — a
    /// per-label-set path (`…/gmeow/ingest/<id>/`), so two adapters never collide.
    mint_base: String,
    /// Full IRIs of every registered label in the set.
    registered: BTreeSet<String>,
    /// Full label IRI → the `gmeow:EmotionType` it closeMatches (a subset — only
    /// the labels that review onto an emotion type; social/cognitive/`neutral`
    /// labels and non-`closeMatch` rungs are deliberately absent).
    emotion_close_match: BTreeMap<String, EmotionMatch>,
    /// The label set's decision rule (read from `gmeow:labelSetDecision`). Drives
    /// the exclusivity guards and `gmeow:AffectDecision` emission for a single-label
    /// set; `Unknown` for a run-scoped candidate set (no reviewed rule).
    decision_rule: LabelSetDecision,
}

impl IngestConfig {
    /// Build the config for a named `gmeow:AffectLabelSet` from a compiled ontology
    /// bundle plus its bundled SSSOM correspondence surface.
    ///
    /// The bundle's **base graph** carries the label registrations
    /// (`gmeow:memberOfLabelSet`) and the canonical `gmeow:EmotionType` typing +
    /// `rdfs:label`, but NOT the reviewed label→emotion `skos:closeMatch` cells —
    /// those are a correspondence lowering the pipeline keeps out of the base graph
    /// (Principle 17) and materializes as an **SSSOM blob**. The claim-routing
    /// mapping is therefore read from `sssom_texts` (the caller reads the blob;
    /// this crate stays free of the pipeline dependency), while the EmotionType
    /// typing + label gloss come from the base graph.
    pub fn from_gts_with_sssom(
        bundle: &[u8],
        sssom_texts: &[String],
        label_set_id: &str,
    ) -> Result<Self, IngestError> {
        let graph = purrdf::gts::reader::read(bundle, false, None);
        let index = index_graph(&graph);
        let mut config = Self::config_for_label_set(&index, label_set_id)?;
        for tsv in sssom_texts {
            config.add_sssom_correspondences(tsv, &index)?;
        }
        Ok(config)
    }

    /// Build the config for a named label set from an already-indexed graph. Reads
    /// the registered labels, derives the registry namespace + mint base from them,
    /// and folds in any closeMatch cells present IN the graph (the authored slice
    /// sources — `module.ttl` + `mappings/equivalences.ttl` — carry them as reified
    /// `gmeow:TermEquivalence` / direct triples). For the compiled bundle,
    /// supplement with [`Self::add_sssom_correspondences`].
    pub fn config_for_label_set(
        index: &TripleIndex,
        label_set_id: &str,
    ) -> Result<Self, IngestError> {
        let label_set_iri = format!("{LABELSET}{label_set_id}");
        Self::from_label_set(index, label_set_id, &label_set_iri)
    }

    /// Build a RUN-SCOPED config for a zero-shot capture. The candidate set is not
    /// a static `gmeow:AffectLabelSet` — it is part of the run's identity — so the
    /// registered members and the candidate registry namespace are DERIVED from the
    /// capture's `candidate_labels`, keyed by the candidate set itself (so a
    /// different candidate set is a different registry, but the same set is
    /// deterministic/idempotent). Routes NO auto-claim: a run-scoped prompt
    /// candidate has no pre-reviewed closeMatch, so the claim/evidence boundary holds.
    pub fn run_scoped_from_capture(capture: &ClassifierRunCapture) -> Result<Self, IngestError> {
        let candidates = capture
            .candidate_labels
            .as_deref()
            .ok_or(IngestError::MissingCandidateLabels)?;
        Self::run_scoped(capture.label_set_id.clone(), candidates)
    }

    /// The run-scoped constructor shared by the put leg (from a capture) and the
    /// get leg (from the evidence graph's declared candidate set).
    fn run_scoped(label_set_id: String, candidates: &[String]) -> Result<Self, IngestError> {
        if candidates.is_empty() {
            return Err(IngestError::MissingCandidateLabels);
        }
        let mint_base = format!("{GMEOW}ingest/{}/", label_set_id.to_lowercase());
        let mut sorted: Vec<&str> = candidates.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        sorted.dedup();
        let registry_prefix = format!("{mint_base}candidate-{}/", fnv1a_hex(&sorted.join("\n")));
        let registered: BTreeSet<String> = sorted
            .iter()
            .map(|c| format!("{registry_prefix}{c}"))
            .collect();
        Ok(IngestConfig {
            label_set_id,
            registry_prefix,
            mint_base,
            registered,
            emotion_close_match: BTreeMap::new(),
            // A run-scoped candidate set has no reviewed decision rule: its NLI
            // entailment scores are per-hypothesis, not normalized across candidates,
            // so it is neither a clean simplex nor a clean product space.
            decision_rule: LabelSetDecision::Unknown,
        })
    }

    /// The run-scoped candidate-set node this config mints (an in-graph
    /// `gmeow:AffectLabelSet`, since the set is not registered in the bundle).
    fn candidate_set_iri(&self) -> String {
        format!("{}#set", self.registry_prefix)
    }

    /// Fold an SSSOM correspondence surface into the emotion-claim routing map:
    /// each `<label> skos:closeMatch <emotion>` row whose subject is a registered
    /// member of THIS label set and whose object is a `gmeow:EmotionType` (per
    /// `index`) becomes a routing target. `broadMatch` rows (`grief`) and
    /// closeMatch rows to a non-EmotionType (`desire → gmeow:Desire`) are correctly
    /// excluded — the SSSOM's own semantics carry the distinction. CURIEs are
    /// expanded via the file's `# curie_map:` header (absolute IRIs pass through),
    /// so no prefix is hardcoded. No-optionality: an unparsable data row is a
    /// HARD FAIL, never silently dropped.
    pub fn add_sssom_correspondences(
        &mut self,
        sssom_tsv: &str,
        index: &TripleIndex,
    ) -> Result<(), IngestError> {
        let curie_map = parse_sssom_curie_map(sssom_tsv);
        let expand = |curie: &str| -> Option<String> {
            // Absolute IRIs are standard in SSSOM and pass through unexpanded.
            if curie.starts_with("http://") || curie.starts_with("https://") {
                return Some(curie.to_owned());
            }
            let (prefix, local) = curie.split_once(':')?;
            Some(format!("{}{}", curie_map.get(prefix)?, local))
        };
        let emotion_type = g("EmotionType");
        for line in sssom_tsv.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            if cols[0] == "subject_id" {
                continue; // the column header row
            }
            // A data row that cannot be parsed into three expandable CURIEs/IRIs is
            // a malformed correspondence surface — reject it, do not drop the claim
            // it would have routed.
            let (Some(subject), Some(predicate), Some(object)) = (
                cols.first().copied().and_then(expand),
                cols.get(1).copied().and_then(expand),
                cols.get(2).copied().and_then(expand),
            ) else {
                return Err(IngestError::MalformedGraph {
                    detail: format!("unparsable SSSOM correspondence row: {line:?}"),
                });
            };
            if predicate == SKOS_CLOSE_MATCH
                && self.registered.contains(&subject)
                && has_type(index, &object, &emotion_type)
            {
                let word = first_literal(index, &object, RDFS_LABEL)
                    .unwrap_or_else(|| local_name(&object).to_owned());
                self.emotion_close_match
                    .insert(subject, EmotionMatch { iri: object, word });
            }
        }
        Ok(())
    }

    /// The generic loader — the functor's parameterization point. Any registered
    /// `gmeow:AffectLabelSet` yields a config by reading its members + closeMatch
    /// cells; new adapters differ only in the `label_set_id` + a capture. The
    /// registry namespace and the mint base are DERIVED from the registered members
    /// (never hardcoded), so a mixed-namespace or empty set is a hard fail.
    fn from_label_set(
        index: &TripleIndex,
        label_set_id: &str,
        label_set_iri: &str,
    ) -> Result<Self, IngestError> {
        let member_of = g("memberOfLabelSet");
        let registered: BTreeSet<String> = subjects(index)
            .filter(|s| first_iri(index, s, &member_of).as_deref() == Some(label_set_iri))
            .cloned()
            .collect();
        let registry_prefix = derive_registry_prefix(&registered, label_set_id)?;
        let mint_base = format!("{GMEOW}ingest/{}/", label_set_id.to_lowercase());

        // The decision rule is a MANDATORY property of a static label set: without it
        // exclusivity is unknown and the producer cannot judge a multi-crossing.
        let decision_rule = match first_iri(index, label_set_iri, &g("labelSetDecision")) {
            Some(iri) if iri == g("decisionArgmax") => LabelSetDecision::Argmax,
            Some(iri) if iri == g("decisionIndependentThreshold") => {
                LabelSetDecision::IndependentThreshold
            }
            Some(other) => {
                return Err(malformed(format!(
                    "label set {label_set_id:?} has an unknown gmeow:labelSetDecision {other:?}"
                )));
            }
            None => {
                return Err(IngestError::MissingLabelSetDecision {
                    label_set: label_set_id.to_owned(),
                });
            }
        };

        let term_equivalence = g("TermEquivalence");
        let align_subject = g("alignSubject");
        let align_predicate = g("alignPredicate");
        let align_object = g("alignObject");
        let emotion_type = g("EmotionType");

        let mut emotion_close_match = BTreeMap::new();
        // A supported "expresses" claim is honest only when the reviewed rung is
        // closeMatch (not broadMatch) AND the target is an EmotionType (not, e.g.,
        // teleology's gmeow:Desire). Scope by set MEMBERSHIP, not by namespace, so
        // sibling sets sharing a registry prefix (SST-2 / CardiffNLP / Ekman-7 all
        // live under `gmeow-registry/hf/`) never cross-route. The mapping reaches
        // the loader in one of two shapes: reified gmeow:TermEquivalence cells (the
        // authored slice source, `mappings/equivalences.ttl`) OR the direct
        // skos:closeMatch triple the pipeline lowers those cells into in the
        // compiled bundle. Read BOTH so the producer works identically off the slice
        // sources and off `gmeow.gts`.
        let mut record = |subject: &str, object: String| {
            if registered.contains(subject) && has_type(index, &object, &emotion_type) {
                let word = first_literal(index, &object, RDFS_LABEL)
                    .unwrap_or_else(|| local_name(&object).to_owned());
                emotion_close_match.insert(subject.to_owned(), EmotionMatch { iri: object, word });
            }
        };
        // Reified form: gmeow:TermEquivalence with alignSubject/Predicate/Object.
        for cell in subjects(index).filter(|s| has_type(index, s, &term_equivalence)) {
            let (Some(subject), Some(predicate), Some(object)) = (
                first_iri(index, cell, &align_subject),
                first_iri(index, cell, &align_predicate),
                first_iri(index, cell, &align_object),
            ) else {
                continue;
            };
            if predicate == SKOS_CLOSE_MATCH {
                record(&subject, object);
            }
        }
        // Lowered form: a direct `<label> skos:closeMatch <emotion>` triple.
        for subject in &registered {
            for object in all_iris(index, subject, SKOS_CLOSE_MATCH) {
                record(subject, object);
            }
        }

        Ok(IngestConfig {
            label_set_id: label_set_id.to_owned(),
            registry_prefix,
            mint_base,
            registered,
            emotion_close_match,
            decision_rule,
        })
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

// ─────────────────────── generic adapter dispatch ──────────────────────────

/// Build the producer config a capture calls for, off the compiled bundle — the
/// put-leg entry point. A zero-shot capture (`candidate_labels` present) yields a
/// run-scoped config; otherwise it dispatches on the capture's declared
/// `label_set_id` (`GoEmotions` / `SST2` / `CardiffTweetEval` / `Ekman7`), so a
/// single CLI/API surface serves every adapter with no per-model code.
pub fn config_for_capture(
    bundle: &[u8],
    sssom_texts: &[String],
    capture: &ClassifierRunCapture,
) -> Result<IngestConfig, IngestError> {
    if capture.candidate_labels.is_some() {
        IngestConfig::run_scoped_from_capture(capture)
    } else {
        IngestConfig::from_gts_with_sssom(bundle, sssom_texts, &capture.label_set_id)
    }
}

/// Select the producer config an evidence graph was minted under — the get-leg
/// entry point. A run-scoped (zero-shot) graph carries its own candidate
/// `gmeow:AffectLabelSet` in-graph (the run declares a `gmeow:hypothesisTemplate`),
/// so its config is rebuilt from the graph itself. Otherwise recovery enumerates
/// every `gmeow:AffectLabelSet` in the bundle and picks the one whose registered
/// members cover the graph's `gmeow:emittedLabel`s (membership, not namespace — so
/// it disambiguates sibling sets sharing a registry prefix). Hard-fails if no
/// single set matches.
pub fn config_for_evidence(
    bundle: &[u8],
    sssom_texts: &[String],
    turtle: &str,
) -> Result<IngestConfig, IngestError> {
    let ev = index_turtle(turtle.as_bytes()).map_err(|d| IngestError::MalformedGraph {
        detail: d.to_string(),
    })?;
    let output_type = g("AffectClassifierOutput");
    let emitted_label = g("emittedLabel");
    let emitted: BTreeSet<String> = subjects(&ev)
        .filter(|s| has_type(&ev, s, &output_type))
        .filter_map(|out| first_iri(&ev, out, &emitted_label))
        .collect();
    if emitted.is_empty() {
        return Err(malformed("evidence graph carries no gmeow:emittedLabel"));
    }

    // Run-scoped (zero-shot): the run declares a hypothesis template, and the
    // candidate set is the in-graph gmeow:AffectLabelSet (labelled with its
    // label_set_id) whose members' rdfs:labels are the candidate surfaces.
    let run = sole_subject_of_type(&ev, &g("ModelInferenceRun"))?;
    if first_literal(&ev, &run, &g("hypothesisTemplate")).is_some() {
        let set_iri = sole_subject_of_type(&ev, &g("AffectLabelSet"))?;
        let label_set_id = first_literal(&ev, &set_iri, RDFS_LABEL).ok_or_else(|| {
            malformed("run-scoped candidate set has no rdfs:label (its label_set_id)")
        })?;
        let member_of = g("memberOfLabelSet");
        let candidates: Vec<String> = subjects(&ev)
            .filter(|s| first_iri(&ev, s, &member_of).as_deref() == Some(set_iri.as_str()))
            .filter_map(|label_iri| first_literal(&ev, label_iri, RDFS_LABEL))
            .collect();
        return IngestConfig::run_scoped(label_set_id, &candidates);
    }

    let graph = purrdf::gts::reader::read(bundle, false, None);
    let index = index_graph(&graph);
    let label_set_type = g("AffectLabelSet");
    for set_iri in subjects(&index).filter(|s| has_type(&index, s, &label_set_type)) {
        let Some(id) = set_iri.strip_prefix(LABELSET) else {
            continue;
        };
        let mut config = IngestConfig::config_for_label_set(&index, id)?;
        for tsv in sssom_texts {
            config.add_sssom_correspondences(tsv, &index)?;
        }
        if emitted.iter().all(|l| config.registered.contains(l)) {
            return Ok(config);
        }
    }
    Err(malformed(
        "no registered gmeow:AffectLabelSet covers the graph's emitted labels",
    ))
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

    let run_iri = mint_run_iri(capture, &config.mint_base);
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
    if let Some(template) = &capture.hypothesis_template {
        sink.string(&run_iri, &g("hypothesisTemplate"), template);
    }

    // Run-scoped (zero-shot): the candidate set is part of the run identity, not a
    // static registry — so mint it IN the evidence graph as a gmeow:AffectLabelSet
    // (labelled with its label_set_id) whose members are the candidate labels
    // (registered via memberOfLabelSet, so the emitted labels are honestly
    // registered exactly as the fixed-label adapters' labels are in the base graph).
    if capture.candidate_labels.is_some() {
        let set_iri = config.candidate_set_iri();
        sink.iri(&set_iri, RDF_TYPE, &g("AffectLabelSet"));
        sink.lang(&set_iri, RDFS_LABEL, &config.label_set_id);
        for surface in config.registered_labels() {
            let label_iri = config.label_iri(surface);
            sink.iri(&label_iri, RDF_TYPE, &g("AffectClassifierLabel"));
            sink.iri(&label_iri, &g("memberOfLabelSet"), &set_iri);
            sink.lang(&label_iri, RDFS_LABEL, surface);
        }
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

        if config.decision_rule == LabelSetDecision::Argmax {
            // A single-label classifier ALWAYS decides (argmax) — record that
            // categorical decision as evidence, faithfully, even when it falls below
            // the claim threshold. This supersedes AffectEvaluationConcluded for
            // exclusive sets: a decided winner is the positive "checked" fact.
            // Derived-from-scores, so recover() ignores it and the round-trip holds.
            let winner = target
                .scores
                .iter()
                .max_by(|a, b| a.score.partial_cmp(&b.score).expect("finite scores"))
                .expect("a validated target carries ≥1 score");
            let winner_threshold = capture
                .threshold_policy
                .threshold_for(&winner.label)
                .expect("threshold present (validated)");
            let decision_iri = mint_decision_iri(&run_iri, &target.target_iri);
            sink.iri(&decision_iri, RDF_TYPE, &g("AffectDecision"));
            sink.iri(&decision_iri, &g("vantage"), &run_iri);
            sink.iri(&decision_iri, &g("observedFeature"), &target.target_iri);
            sink.iri(
                &decision_iri,
                &g("decidedLabel"),
                &config.label_iri(&winner.label),
            );
            sink.iri(&decision_iri, &g("derivedByFunction"), &g("fnArgmax"));
            sink.boolean(
                &decision_iri,
                &g("decisionCrossedThreshold"),
                winner.score >= winner_threshold,
            );
            // The margin (top1 − top2) is the argmax confidence — recorded only when a
            // runner-up exists (a single top-1-score capture has none).
            if target.scores.len() >= 2 {
                let mut desc: Vec<f64> = target.scores.iter().map(|c| c.score).collect();
                desc.sort_by(|a, b| b.partial_cmp(a).expect("finite scores"));
                sink.decimal(&decision_iri, &g("decisionMargin"), desc[0] - desc[1]);
            }
        } else if !any_crossed {
            // Multi-label / run-scoped: "concluded and flat" is not "never checked".
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
    // The scoring function and the set's declared decision rule must agree: a
    // softmax over a multi-label set (or a sigmoid over an exclusive set) is a
    // contradiction (gmeow:impliesLabelSetDecision). Skipped for a run-scoped set,
    // whose rule is Unknown.
    if config.decision_rule != LabelSetDecision::Unknown
        && let Some(implied) = implied_decision(capture.score_semantics)
        && implied != config.decision_rule
    {
        return Err(IngestError::ScoreSemanticsDecisionMismatch {
            implied: implied.label(),
            declared: config.decision_rule.label(),
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
    // A zero-shot (entailment) run carries its candidate set + hypothesis template
    // as run identity — neither is optional, both are hard-fails when absent.
    if capture.score_semantics == ScoreSemantics::Entailment {
        if capture
            .hypothesis_template
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Err(IngestError::MissingHypothesisTemplate);
        }
        if capture
            .candidate_labels
            .as_ref()
            .is_none_or(|c| c.is_empty())
        {
            return Err(IngestError::MissingCandidateLabels);
        }
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

        // Exclusivity guards, single-label sets only. A multi-label
        // (IndependentThreshold) or run-scoped (Unknown) set legitimately admits
        // many crossings and has no single argmax winner, so it is exempt.
        if config.decision_rule == LabelSetDecision::Argmax {
            // The defining property of an exclusive set: a softmax score vector is a
            // point on the probability simplex (Σ = 1). Checkable only with the full
            // distribution present.
            if capture.score_semantics == ScoreSemantics::Softmax && capture.return_all_scores {
                let sum: f64 = target.scores.iter().map(|c| c.score).sum();
                if (sum - 1.0).abs() > SIMPLEX_EPS {
                    return Err(IngestError::NonNormalizedExclusiveScores {
                        target: target.target_iri.clone(),
                        sum,
                    });
                }
            }
            // At most one label may cross its claim threshold (mutually-exclusive
            // claims). thresholds are proven present by the loop above.
            let crossed: Vec<String> = target
                .scores
                .iter()
                .filter(|c| {
                    c.score
                        >= capture
                            .threshold_policy
                            .threshold_for(&c.label)
                            .expect("threshold present (validated above)")
                })
                .map(|c| c.label.clone())
                .collect();
            if crossed.len() > 1 {
                return Err(IngestError::ExclusivityViolation {
                    target: target.target_iri.clone(),
                    labels: crossed,
                });
            }
            // The argmax must be unambiguous — an EXACT top-score tie has no faithful
            // single winner (a near-tie is recorded honestly as gmeow:decisionMargin).
            let max = target
                .scores
                .iter()
                .map(|c| c.score)
                .fold(f64::NEG_INFINITY, f64::max);
            let top: Vec<String> = target
                .scores
                .iter()
                .filter(|c| c.score == max)
                .map(|c| c.label.clone())
                .collect();
            if top.len() > 1 {
                return Err(IngestError::AmbiguousArgmax {
                    target: target.target_iri.clone(),
                    labels: top,
                });
            }
        }
    }
    Ok(())
}

// ───────────────────────────── deterministic IRIs ───────────────────────────

/// The run node — a pure function of the recoverable identity of the run
/// (model + revision + the sorted set of classified targets). Same capture ⇒
/// same IRI ⇒ byte-identical Turtle (idempotent); no `NOW()`, no randomness.
fn mint_run_iri(capture: &ClassifierRunCapture, mint_base: &str) -> String {
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
    format!("{mint_base}run-{}", fnv1a_hex(&key))
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

fn mint_decision_iri(run_iri: &str, target_iri: &str) -> String {
    format!("{run_iri}/decision-{}", fnv1a_hex(target_iri))
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

/// The registry namespace (up to and including the last `/` or `#`) of an IRI.
fn namespace_of(iri: &str) -> &str {
    match iri.rfind(['/', '#']) {
        Some(i) => &iri[..=i],
        None => iri,
    }
}

/// Derive the single registry namespace every label in a set shares — the seam
/// that lets a config strip/rebuild label locals without a hardcoded prefix. A
/// label set that is empty (an unknown/unregistered id) or whose members span more
/// than one namespace is malformed: a HARD FAIL, never a silent guess.
fn derive_registry_prefix(
    registered: &BTreeSet<String>,
    label_set_id: &str,
) -> Result<String, IngestError> {
    let mut namespaces = registered.iter().map(|iri| namespace_of(iri));
    let Some(first) = namespaces.next() else {
        return Err(IngestError::LabelSetMismatch {
            expected: format!("a registered gmeow:AffectLabelSet {label_set_id:?}"),
            found: "no registered labels".to_owned(),
        });
    };
    if namespaces.any(|ns| ns != first) {
        return Err(IngestError::MalformedGraph {
            detail: format!("label set {label_set_id:?} spans more than one registry namespace"),
        });
    }
    Ok(first.to_owned())
}

/// Parse the `# curie_map:` block of an SSSOM TSV into prefix → namespace IRI.
/// Only `#`-comment lines whose value is an absolute `http(s)` IRI are taken, so
/// the metadata lines (`mapping_tool:`, `mapping_date:`) are naturally skipped.
fn parse_sssom_curie_map(sssom_tsv: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in sssom_tsv.lines() {
        let Some(rest) = line.strip_prefix('#') else {
            continue;
        };
        let Some((prefix, iri)) = rest.trim().split_once(':') else {
            continue;
        };
        let iri = iri.trim();
        if !prefix.is_empty()
            && !prefix.contains(char::is_whitespace)
            && (iri.starts_with("http://") || iri.starts_with("https://"))
        {
            map.insert(prefix.to_owned(), iri.to_owned());
        }
    }
    map
}

/// Format an f64 as an `xsd:decimal` lexical (`0.84`, `1.0`) — NEVER exponent
/// form, which `xsd:decimal` forbids. Rust's `f64` `Display` already emits the
/// shortest round-trip decimal WITHOUT scientific notation (only the `{:e}`
/// formatter does that), even for tiny in-range scores like `1e-7` → `0.0000001`;
/// we only normalize a whole number to carry a trailing `.0`. (NaN/±Inf are
/// rejected upstream, so inputs are tame.)
fn format_decimal(value: f64) -> String {
    let s = format!("{value}");
    debug_assert!(
        !s.contains(['e', 'E']),
        "f64 Display must never emit scientific notation (invalid xsd:decimal): {s}"
    );
    if s.contains('.') { s } else { format!("{s}.0") }
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

    fn boolean(&mut self, s: &str, p: &str, value: bool) {
        self.lit(
            s,
            p,
            RdfLiteral::typed(
                if value { "true" } else { "false" }.to_owned(),
                XSD_BOOLEAN.to_owned(),
            ),
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
            "scoreEntailment" => ScoreSemantics::Entailment,
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
    let index = index_turtle(turtle.as_bytes()).map_err(|d| IngestError::MalformedGraph {
        detail: d.to_string(),
    })?;

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

    // Run-scoped (zero-shot) provenance: the hypothesis template pins the run as
    // NLI, and the candidate set is exactly the labels it scored — read back from
    // the emitted evidence, never a static label set.
    let hypothesis_template = first_literal(&index, &run, &g("hypothesisTemplate"));
    let candidate_labels = hypothesis_template.as_ref().map(|_| {
        let set: BTreeSet<String> = targets
            .iter()
            .flat_map(|t| t.scores.iter().map(|s| s.label.clone()))
            .collect();
        set.into_iter().collect()
    });

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
        hypothesis_template,
        candidate_labels,
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

    // The candidate set is normalized the same way `recover` reads it back: the
    // sorted, de-duplicated set of scored surfaces (present iff run-scoped).
    let candidate_labels = capture.candidate_labels.as_ref().map(|_| {
        let set: BTreeSet<String> = targets
            .iter()
            .flat_map(|t| t.scores.iter().map(|s| s.label.clone()))
            .collect();
        set.into_iter().collect()
    });

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
        hypothesis_template: capture.hypothesis_template.clone(),
        candidate_labels,
    }
}

/// `function_to_apply` implied by the score semantics — the reconstructable form
/// of the pipeline's activation. Sigmoid/softmax pin their activation (and are
/// validated against it); entailment names itself; the remaining unbounded
/// semantics (logit/margin/calibrated) pin no single activation → `""`.
fn derived_function(sem: ScoreSemantics) -> String {
    match sem {
        ScoreSemantics::Entailment => "entailment".to_owned(),
        other => other.required_function().unwrap_or("").to_owned(),
    }
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
        "gmeow-labelset:GoEmotions gmeow:labelSetDecision gmeow:decisionIndependentThreshold .\n",
        "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow-goemotions:neutral a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
        "gmeow:eq1 a gmeow:TermEquivalence ; gmeow:alignSubject gmeow-goemotions:joy ; gmeow:alignPredicate skos:closeMatch ; gmeow:alignObject gmeow:emotionJoy .\n",
    );

    fn config() -> IngestConfig {
        let index = index_turtle(ONTO.as_bytes()).expect("index onto fixture");
        IngestConfig::config_for_label_set(&index, "GoEmotions").expect("goemotions config")
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
            hypothesis_template: None,
            candidate_labels: None,
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
        assert!(!cap.targets.is_empty(), "at least one captured target");
        for target in &cap.targets {
            assert_eq!(target.scores.len(), 28, "GoEmotions emits 28 labels");
        }
    }

    /// Labels + canonical EmotionType typing but NO in-graph closeMatch — the
    /// shape of the compiled bundle, where the mapping arrives via SSSOM.
    const ONTO_NO_MAPPING: &str = concat!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        "@prefix gmeow-goemotions: <https://blackcatinformatics.ca/gmeow-registry/goemotions/> .\n",
        "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
        "gmeow-labelset:GoEmotions gmeow:labelSetDecision gmeow:decisionIndependentThreshold .\n",
        "gmeow-goemotions:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow-goemotions:grief a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow-goemotions:desire a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:GoEmotions .\n",
        "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
        "gmeow:emotionSadness a gmeow:EmotionType ; rdfs:label \"sadness\"@x-gmeow-english .\n",
        "gmeow:Desire a gmeow:Kind .\n",
    );

    /// An SSSOM surface shaped like `generated/mappings/gmeow-affect.sssom.tsv`:
    /// a curie_map header, a column header, and rows — including the broadMatch
    /// (grief) and closeMatch-to-non-EmotionType (desire) rows that must NOT route.
    const SSSOM: &str = concat!(
        "# mapping_tool: gmeow-dev sync --mode update --outputs generated (mappings)\n",
        "# curie_map:\n",
        "#   gmeow: https://blackcatinformatics.ca/gmeow/\n",
        "#   gmeow-goemotions: https://blackcatinformatics.ca/gmeow-registry/goemotions/\n",
        "#   skos: http://www.w3.org/2004/02/skos/core#\n",
        "#   semapv: https://w3id.org/semapv/vocab/\n",
        "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\tcomment\n",
        "gmeow-goemotions:joy\tskos:closeMatch\tgmeow:emotionJoy\tsemapv:ManualMappingCuration\t0.9\t\n",
        "gmeow-goemotions:grief\tskos:broadMatch\tgmeow:emotionSadness\tsemapv:ManualMappingCuration\t0.8\tnarrower\n",
        "gmeow-goemotions:desire\tskos:closeMatch\tgmeow:Desire\tsemapv:ManualMappingCuration\t0.85\tconative\n",
    );

    #[test]
    fn sssom_correspondences_route_only_closematch_emotiontypes() {
        let index = index_turtle(ONTO_NO_MAPPING.as_bytes()).expect("index onto");
        let mut cfg =
            IngestConfig::config_for_label_set(&index, "GoEmotions").expect("goemotions config");
        // the bundle-shaped graph carries NO in-graph closeMatch.
        assert!(cfg.emotion_close_match.is_empty());

        cfg.add_sssom_correspondences(SSSOM, &index)
            .expect("well-formed sssom");
        // joy: closeMatch → EmotionType → routed, glossed by the CANONICAL term.
        assert_eq!(cfg.emotion_close_match[&cfg.label_iri("joy")].word, "joy");
        // grief: broadMatch (not closeMatch) → excluded.
        assert!(
            !cfg.emotion_close_match
                .contains_key(&cfg.label_iri("grief"))
        );
        // desire: closeMatch but object is teleology's gmeow:Desire, not an
        // EmotionType → excluded.
        assert!(
            !cfg.emotion_close_match
                .contains_key(&cfg.label_iri("desire"))
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

    /// A minimal zero-shot (run-scoped) capture: NLI entailment over a candidate set
    /// declared per run, not read from a static label set.
    fn zeroshot_capture() -> ClassifierRunCapture {
        ClassifierRunCapture {
            model_identifier: "facebook/bart-large-mnli".to_owned(),
            model_revision: "deadbeef".to_owned(),
            model_framework: "transformers".to_owned(),
            model_task: "zero-shot-classification".to_owned(),
            function_to_apply: "entailment".to_owned(),
            return_all_scores: true,
            label_set_id: "ZeroShotEmotion2".to_owned(),
            score_semantics: ScoreSemantics::Entailment,
            threshold_policy: ThresholdPolicy::Global { value: 0.5 },
            targets: vec![TargetInput {
                target_iri: "https://example.org/affect/passage-1".to_owned(),
                scores: vec![
                    LabelScore {
                        label: "joy".to_owned(),
                        score: 0.97,
                    },
                    LabelScore {
                        label: "fear".to_owned(),
                        score: 0.02,
                    },
                ],
            }],
            score_calibration: None,
            tokenizer_revision: None,
            label_set_revision: Some("candidates:fear,joy".to_owned()),
            hypothesis_template: Some("This text expresses {}.".to_owned()),
            candidate_labels: Some(vec!["joy".to_owned(), "fear".to_owned()]),
        }
    }

    #[test]
    fn zeroshot_run_scoped_round_trips_and_routes_no_claim() {
        let cap = zeroshot_capture();
        let cfg = IngestConfig::run_scoped_from_capture(&cap).expect("run-scoped config");
        let ttl = produce(&cap, &cfg).expect("produce zeroshot");
        // Entailment semantics + run-scoped provenance are emitted; the candidate
        // set is minted in-graph (not a static registry).
        assert!(ttl.contains("scoreEntailment"));
        assert!(ttl.contains("hypothesisTemplate"));
        assert!(ttl.contains("AffectLabelSet"));
        // Evidence only — a run-scoped prompt candidate routes NO expresses-claim.
        assert!(!ttl.contains("the text expresses"));
        // recover reads the candidate set + template back from the graph.
        assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
    }

    #[test]
    fn zeroshot_hard_fails_without_template_or_candidates() {
        let cfg = IngestConfig::run_scoped_from_capture(&zeroshot_capture()).unwrap();
        let mut no_template = zeroshot_capture();
        no_template.hypothesis_template = None;
        assert_eq!(
            produce(&no_template, &cfg),
            Err(IngestError::MissingHypothesisTemplate)
        );
    }

    #[test]
    fn format_decimal_is_always_a_valid_xsd_decimal() {
        // xsd:decimal forbids scientific notation. A tiny in-range score must
        // still serialize as a plain decimal (and round-trip through the lexical).
        for v in [0.0, 1.0, 0.84, 0.0000001, 1e-12, 0.5_f64] {
            let s = format_decimal(v);
            assert!(
                !s.contains(['e', 'E']),
                "format_decimal({v}) = {s:?} must not use scientific notation"
            );
            assert!(
                s.contains('.'),
                "format_decimal({v}) = {s:?} must carry a point"
            );
            assert_eq!(renormalize_decimal(v), s.parse::<f64>().unwrap());
        }
    }

    // ── label-set exclusivity ──────────────────────────────────────────────

    /// A single-label (argmax) set: a 3-label Ekman-style softmax set with one
    /// reviewed closeMatch (joy → emotionJoy), declared gmeow:decisionArgmax.
    const ARGMAX_ONTO: &str = concat!(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
        "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
        "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n",
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
        "gmeow-labelset:Ekman7 gmeow:labelSetDecision gmeow:decisionArgmax .\n",
        "gmeow-hf:joy a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
        "gmeow-hf:anger a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
        "gmeow-hf:neutral a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:Ekman7 .\n",
        "gmeow:emotionJoy a gmeow:EmotionType ; rdfs:label \"joy\"@x-gmeow-english .\n",
        "gmeow:eqJoy a gmeow:TermEquivalence ; gmeow:alignSubject gmeow-hf:joy ; gmeow:alignPredicate skos:closeMatch ; gmeow:alignObject gmeow:emotionJoy .\n",
    );

    fn argmax_config() -> IngestConfig {
        let index = index_turtle(ARGMAX_ONTO.as_bytes()).expect("index argmax onto");
        IngestConfig::config_for_label_set(&index, "Ekman7").expect("ekman7 config")
    }

    /// A softmax capture whose argmax (neutral, 0.45) falls BELOW the 0.5 threshold.
    fn argmax_capture() -> ClassifierRunCapture {
        ClassifierRunCapture {
            model_identifier: "j-hartmann/emotion-english-distilroberta-base".to_owned(),
            model_revision: "rev-ek".to_owned(),
            model_framework: "transformers".to_owned(),
            model_task: "text-classification".to_owned(),
            function_to_apply: "softmax".to_owned(),
            return_all_scores: true,
            label_set_id: "Ekman7".to_owned(),
            score_semantics: ScoreSemantics::Softmax,
            threshold_policy: ThresholdPolicy::Global { value: 0.5 },
            targets: vec![TargetInput {
                target_iri: "https://example.org/affect/chunk-e".to_owned(),
                scores: vec![
                    LabelScore {
                        label: "joy".to_owned(),
                        score: 0.20,
                    },
                    LabelScore {
                        label: "anger".to_owned(),
                        score: 0.35,
                    },
                    LabelScore {
                        label: "neutral".to_owned(),
                        score: 0.45,
                    },
                ],
            }],
            score_calibration: None,
            tokenizer_revision: None,
            label_set_revision: None,
            hypothesis_template: None,
            candidate_labels: None,
        }
    }

    fn sole_decision(ttl: &str) -> (String, TripleIndex) {
        let idx = index_turtle(ttl.as_bytes()).expect("index evidence");
        let decision = subjects(&idx)
            .find(|s| has_type(&idx, s, &g("AffectDecision")))
            .expect("an AffectDecision node")
            .clone();
        (decision, idx)
    }

    #[test]
    fn argmax_decision_recorded_faithfully_below_threshold() {
        let ttl = produce(&argmax_capture(), &argmax_config()).unwrap();
        let (decision, idx) = sole_decision(&ttl);
        // the argmax winner (neutral) is recorded even though nothing crossed.
        assert_eq!(
            first_iri(&idx, &decision, &g("decidedLabel")).as_deref(),
            Some("https://blackcatinformatics.ca/gmeow-registry/hf/neutral")
        );
        assert_eq!(
            first_literal(&idx, &decision, &g("decisionCrossedThreshold")).as_deref(),
            Some("false")
        );
        // margin = 0.45 − 0.35 = 0.10, recorded as the argmax confidence.
        let margin: f64 = first_literal(&idx, &decision, &g("decisionMargin"))
            .expect("a margin (≥2 scores)")
            .parse()
            .expect("decimal margin");
        assert!((margin - 0.10).abs() < 1e-9, "margin {margin}");
        assert_eq!(
            first_iri(&idx, &decision, &g("derivedByFunction")).as_deref(),
            Some(g("fnArgmax").as_str())
        );
        // faithful evidence, NOT a forced claim; and NOT the multi-label concluded node.
        assert!(!ttl.contains("AffectiveClaim"));
        assert!(!ttl.contains("AffectEvaluationConcluded"));
    }

    #[test]
    fn single_score_argmax_decision_has_no_margin() {
        let mut cap = argmax_capture();
        cap.return_all_scores = false;
        cap.targets[0].scores = vec![LabelScore {
            label: "joy".to_owned(),
            score: 0.90,
        }];
        let ttl = produce(&cap, &argmax_config()).unwrap();
        let (decision, idx) = sole_decision(&ttl);
        // decision minted from a single top-1 score, with no runner-up → no margin,
        // and NEVER a hard fail on a lossless partial capture (Finding-1 path).
        assert!(first_iri(&idx, &decision, &g("decidedLabel")).is_some());
        assert_eq!(
            first_literal(&idx, &decision, &g("decisionCrossedThreshold")).as_deref(),
            Some("true")
        );
        assert!(first_literal(&idx, &decision, &g("decisionMargin")).is_none());
    }

    #[test]
    fn exclusivity_violation_on_two_crossings() {
        let mut cap = argmax_capture();
        // low per-label thresholds so both anger (0.35) and neutral (0.45) cross.
        cap.threshold_policy = ThresholdPolicy::PerLabel {
            thresholds: BTreeMap::from([
                ("joy".to_owned(), 0.5),
                ("anger".to_owned(), 0.3),
                ("neutral".to_owned(), 0.3),
            ]),
        };
        assert!(matches!(
            produce(&cap, &argmax_config()),
            Err(IngestError::ExclusivityViolation { .. })
        ));
    }

    #[test]
    fn ambiguous_argmax_on_exact_tie() {
        let mut cap = argmax_capture();
        cap.targets[0].scores = vec![
            LabelScore {
                label: "joy".to_owned(),
                score: 0.40,
            },
            LabelScore {
                label: "anger".to_owned(),
                score: 0.40,
            },
            LabelScore {
                label: "neutral".to_owned(),
                score: 0.20,
            },
        ];
        assert!(matches!(
            produce(&cap, &argmax_config()),
            Err(IngestError::AmbiguousArgmax { .. })
        ));
    }

    #[test]
    fn non_normalized_softmax_over_exclusive_set_hard_fails() {
        let mut cap = argmax_capture();
        // sums to 0.6 — off the probability simplex.
        cap.targets[0].scores = vec![
            LabelScore {
                label: "joy".to_owned(),
                score: 0.20,
            },
            LabelScore {
                label: "anger".to_owned(),
                score: 0.20,
            },
            LabelScore {
                label: "neutral".to_owned(),
                score: 0.20,
            },
        ];
        assert!(matches!(
            produce(&cap, &argmax_config()),
            Err(IngestError::NonNormalizedExclusiveScores { .. })
        ));
    }

    #[test]
    fn score_semantics_decision_rule_mismatch_hard_fails() {
        let mut cap = argmax_capture();
        // a sigmoid (multi-label) semantics over an exclusive (argmax) set.
        cap.score_semantics = ScoreSemantics::Sigmoid;
        cap.function_to_apply = "sigmoid".to_owned();
        assert!(matches!(
            produce(&cap, &argmax_config()),
            Err(IngestError::ScoreSemanticsDecisionMismatch { .. })
        ));
    }

    #[test]
    fn multilabel_two_crossings_no_guard_no_decision() {
        let mut cap = capture(); // GoEmotions, sigmoid, multi-label
        cap.targets[0].scores = vec![
            LabelScore {
                label: "joy".to_owned(),
                score: 0.84,
            },
            LabelScore {
                label: "neutral".to_owned(),
                score: 0.72,
            },
        ];
        // two crossings are legitimate for a multi-label set — no hard fail.
        let ttl = produce(&cap, &config()).unwrap();
        assert!(!ttl.contains("AffectDecision"));
        assert!(ttl.contains("AffectiveClaim")); // joy still routes its claim
    }

    #[test]
    fn missing_label_set_decision_hard_fails() {
        const NO_RULE: &str = concat!(
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
            "@prefix gmeow-hf: <https://blackcatinformatics.ca/gmeow-registry/hf/> .\n",
            "@prefix gmeow-labelset: <https://blackcatinformatics.ca/gmeow-registry/labelset/> .\n",
            "gmeow-hf:pos a gmeow:AffectClassifierLabel ; gmeow:memberOfLabelSet gmeow-labelset:SST2 .\n",
        );
        let index = index_turtle(NO_RULE.as_bytes()).expect("index");
        assert!(matches!(
            IngestConfig::config_for_label_set(&index, "SST2"),
            Err(IngestError::MissingLabelSetDecision { .. })
        ));
    }

    #[test]
    fn argmax_round_trip_with_decision_is_identity() {
        let (cap, cfg) = (argmax_capture(), argmax_config());
        let ttl = produce(&cap, &cfg).unwrap();
        // the decision node is present in the graph…
        assert!(ttl.contains("AffectDecision"));
        // …but recover ignores it (derived interpretation), so losslessness holds.
        assert_eq!(recover(&ttl, &cfg).unwrap(), canonicalize(&cap, &cfg));
    }

    #[test]
    fn run_scoped_mints_no_decision() {
        let cap = zeroshot_capture();
        let cfg = IngestConfig::run_scoped_from_capture(&cap).unwrap();
        let ttl = produce(&cap, &cfg).unwrap();
        assert!(!ttl.contains("AffectDecision"));
    }
}
