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
//! hard-fail gates (`crates/validate/tests/conformance_cases/conformance_affect.rs`).
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
    RdfSink as Sink, TripleIndex, all_iris, f64_decimal_lexical, first_iri, first_literal,
    has_type, index_graph, index_turtle, subjects,
};
use purrdf::RdfLiteral;
use purrdf_lex::json::{
    Object, Value,
    record::{DecodeError, FromJson, Record, ToJson, Within as _},
};

// ───────────────────────────── namespaces ──────────────────────────────────

/// Canonical GMEOW namespace.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// Label-set registry prefix — each `gmeow:AffectLabelSet`'s IRI is `{LABELSET}{id}`.
const LABELSET: &str = "https://blackcatinformatics.ca/gmeow-registry/labelset/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_CLOSE_MATCH: &str = "http://www.w3.org/2004/02/skos/core#closeMatch";
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

::purrdf_lex::json_string_enum!(ScoreSemantics {
    Sigmoid => "sigmoid",
    Softmax => "softmax",
    CalibratedProbability => "calibrated_probability",
    Entailment => "entailment",
    Logit => "logit",
    Margin => "margin",
});

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
#[derive(Clone, Debug, PartialEq)]
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

impl FromJson for ThresholdPolicy {
    /// Decode the selected policy branch, retaining native member error locations.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ThresholdPolicy")?;
        let kind: String = fields.required("kind")?;
        // Unknown members remain ignored; known members retain duplicate checks.
        match kind.as_str() {
            "global" => Ok(Self::Global {
                value: fields.required("value")?,
            }),
            "per_label" => Ok(Self::PerLabel {
                thresholds: fields.required("thresholds")?,
            }),
            other => {
                Err(DecodeError::unknown_variant(other, &["global", "per_label"]).within("kind"))
            }
        }
    }
}

impl ToJson for ThresholdPolicy {
    /// Emit the kind before its payload; per-label map keys retain sorted order.
    fn to_json(&self) -> Value {
        Value::from(match self {
            Self::Global { value } => Object::new().with("kind", "global").with("value", *value),
            Self::PerLabel { thresholds } => Object::new()
                .with("kind", "per_label")
                .with("thresholds", thresholds.to_json()),
        })
    }
}

/// One emitted `(label, score)` cell over a target. The score's *meaning* is the
/// run-level `score_semantics` (a logit-valued run sets `score_semantics = Logit`);
/// there is no separate per-output raw-logit field, because the ontology authors
/// no property to emit one — carrying an unpreservable logit would make the
/// losslessness round-trip a lie.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelScore {
    pub label: String,
    pub score: f64,
}

impl FromJson for LabelScore {
    /// Read a required label string and finite binary64 score from an object.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct LabelScore")?;
        Ok(Self {
            label: fields.required("label")?,
            score: fields.required("score")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for LabelScore {
    "label" => label,
    "score" => score,
});

/// One classified target span and the model's per-label scores over it.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetInput {
    /// The IRI of the classified span/document/segment (`gmeow:classifiedTarget`).
    pub target_iri: String,
    pub scores: Vec<LabelScore>,
}

impl FromJson for TargetInput {
    /// Read target and score fields; ingest admission owns IRI and registry checks.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct TargetInput")?;
        Ok(Self {
            target_iri: fields.required("target_iri")?,
            scores: fields.required("scores")?,
        })
    }
}

::purrdf_lex::json_record!(impl ToJson for TargetInput {
    "target_iri" => target_iri,
    "scores" => scores,
});

/// A captured classifier run — the producer's input schema, carrying the full
/// run provenance hard-fail rule 1 demands. Captured once from a real model run
/// and checked in; never produced by on-gate inference (determinism/no-network).
#[derive(Clone, Debug, PartialEq)]
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
    pub score_calibration: Option<String>,
    /// The pinned tokenizer revision, when the capture recorded it (a tokenizer
    /// change silently shifts scores). Genuinely-absent source data ⇒ optional.
    pub tokenizer_revision: Option<String>,
    /// The pinned label-set revision, when recorded (a zero-shot candidate set is
    /// part of run identity). Genuinely-absent source data ⇒ optional.
    pub label_set_revision: Option<String>,
    /// The NLI hypothesis template a zero-shot run instantiated per candidate
    /// (`This text expresses {}.`). Present iff this is a run-scoped (zero-shot)
    /// capture; absent for a fixed-label classifier.
    pub hypothesis_template: Option<String>,
    /// The run-scoped candidate label surfaces a zero-shot run classified against —
    /// part of the run identity, NOT a static `gmeow:AffectLabelSet`. Present iff
    /// this is a zero-shot capture; the labels in `targets` must be exactly this set.
    pub candidate_labels: Option<Vec<String>>,
}

impl FromJson for ClassifierRunCapture {
    /// Read capture evidence and nullable provenance; admission checks cross-field rules.
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct ClassifierRunCapture")?;
        Ok(Self {
            model_identifier: fields.required("model_identifier")?,
            model_revision: fields.required("model_revision")?,
            model_framework: fields.required("model_framework")?,
            model_task: fields.required("model_task")?,
            function_to_apply: fields.required("function_to_apply")?,
            return_all_scores: fields.required("return_all_scores")?,
            label_set_id: fields.required("label_set_id")?,
            score_semantics: fields.required("score_semantics")?,
            threshold_policy: fields.required("threshold_policy")?,
            targets: fields.required("targets")?,
            score_calibration: fields.optional("score_calibration")?,
            tokenizer_revision: fields.optional("tokenizer_revision")?,
            label_set_revision: fields.optional("label_set_revision")?,
            hypothesis_template: fields.optional("hypothesis_template")?,
            candidate_labels: fields.optional("candidate_labels")?,
        })
    }
}

impl ToJson for ClassifierRunCapture {
    /// Emit required members in declaration order, then present optional provenance.
    fn to_json(&self) -> Value {
        let mut fields = Object::new()
            .with("model_identifier", self.model_identifier.to_json())
            .with("model_revision", self.model_revision.to_json())
            .with("model_framework", self.model_framework.to_json())
            .with("model_task", self.model_task.to_json())
            .with("function_to_apply", self.function_to_apply.to_json())
            .with("return_all_scores", self.return_all_scores)
            .with("label_set_id", self.label_set_id.to_json())
            .with("score_semantics", self.score_semantics.to_json())
            .with("threshold_policy", self.threshold_policy.to_json())
            .with("targets", self.targets.to_json());
        if let Some(value) = &self.score_calibration {
            fields.insert("score_calibration", value.to_json());
        }
        if let Some(value) = &self.tokenizer_revision {
            fields.insert("tokenizer_revision", value.to_json());
        }
        if let Some(value) = &self.label_set_revision {
            fields.insert("label_set_revision", value.to_json());
        }
        if let Some(value) = &self.hypothesis_template {
            fields.insert("hypothesis_template", value.to_json());
        }
        if let Some(value) = &self.candidate_labels {
            fields.insert("candidate_labels", value.to_json());
        }
        Value::from(fields)
    }
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
    /// An SSSOM correspondence file could not be parsed by `purrdf::sssom`.
    InvalidSssom {
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
            IngestError::InvalidSssom { detail } => {
                write!(f, "malformed SSSOM correspondence file: {detail}")
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
        let curie_map = purrdf::sssom::parse_tsv(sssom_tsv)
            .map_err(|d| IngestError::InvalidSssom {
                detail: d.to_string(),
            })?
            .meta
            .curie_map;
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

        let emotion_type = g("EmotionType");

        let mut emotion_close_match = BTreeMap::new();
        // A supported "expresses" claim is honest only when the reviewed rung is
        // closeMatch (not broadMatch) AND the target is an EmotionType (not, e.g.,
        // teleology's gmeow:Desire). Scope by set MEMBERSHIP, not by namespace, so
        // sibling sets sharing a registry prefix (SST-2 / CardiffNLP / Ekman-7 all
        // live under `gmeow-registry/hf/`) never cross-route. A native alignment cell
        // (`<label> skos:closeMatch <emotion> {| gmeow:sssomFile … |}`) ASSERTS its base
        // `skos:closeMatch` triple, so it is read through the direct-triple path below —
        // identically off the authored slice sources and off `gmeow.gts` (there is no
        // longer a separate reified `gmeow:TermEquivalence` cell shape to read).
        let mut record = |subject: &str, object: String| {
            if registered.contains(subject) && has_type(index, &object, &emotion_type) {
                let word = first_literal(index, &object, RDFS_LABEL)
                    .unwrap_or_else(|| local_name(&object).to_owned());
                emotion_close_match.insert(subject.to_owned(), EmotionMatch { iri: object, word });
            }
        };
        // A direct `<label> skos:closeMatch <emotion>` triple — the asserted base of a
        // native alignment cell (or the same triple as lowered into the compiled bundle).
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
///
/// # Errors
/// Retains the typed capture-validation refusal or the original native storage,
/// codec or UTF-8 failure. A failed emission never returns a partial graph.
pub fn produce(
    capture: &ClassifierRunCapture,
    config: &IngestConfig,
) -> gmeow_errors::Result<String> {
    validate(capture, config)?;

    let run_iri = mint_run_iri(capture, &config.mint_base);
    let mut sink = Sink::default();

    // ── the run ──
    sink.iri(&run_iri, RDF_TYPE, &g("ModelInferenceRun"))?;
    sink.string(&run_iri, &g("modelIdentifier"), &capture.model_identifier)?;
    sink.string(&run_iri, &g("modelRevision"), &capture.model_revision)?;
    sink.string(&run_iri, &g("modelFramework"), &capture.model_framework)?;
    sink.string(&run_iri, &g("modelTask"), &capture.model_task)?;
    if let Some(tok) = &capture.tokenizer_revision {
        sink.string(&run_iri, &g("tokenizerRevision"), tok)?;
    }
    if let Some(ls) = &capture.label_set_revision {
        sink.string(&run_iri, &g("labelSetRevision"), ls)?;
    }
    if let Some(template) = &capture.hypothesis_template {
        sink.string(&run_iri, &g("hypothesisTemplate"), template)?;
    }

    // Run-scoped (zero-shot): the candidate set is part of the run identity, not a
    // static registry — so mint it IN the evidence graph as a gmeow:AffectLabelSet
    // (labelled with its label_set_id) whose members are the candidate labels
    // (registered via memberOfLabelSet, so the emitted labels are honestly
    // registered exactly as the fixed-label adapters' labels are in the base graph).
    if capture.candidate_labels.is_some() {
        let set_iri = config.candidate_set_iri();
        sink.iri(&set_iri, RDF_TYPE, &g("AffectLabelSet"))?;
        sink.lit(
            &set_iri,
            RDFS_LABEL,
            RdfLiteral::language_tagged(&config.label_set_id, GMEOW_ENGLISH),
        )?;
        for surface in config.registered_labels() {
            let label_iri = config.label_iri(surface);
            sink.iri(&label_iri, RDF_TYPE, &g("AffectClassifierLabel"))?;
            sink.iri(&label_iri, &g("memberOfLabelSet"), &set_iri)?;
            sink.lit(
                &label_iri,
                RDFS_LABEL,
                RdfLiteral::language_tagged(surface, GMEOW_ENGLISH),
            )?;
        }
    }

    let semantics_iri = capture.score_semantics.iri();
    for target in &capture.targets {
        sink.iri(&run_iri, &g("usedInput"), &target.target_iri)?;

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

            sink.iri(&out_iri, RDF_TYPE, &g("AffectClassifierOutput"))?;
            sink.iri(&out_iri, &g("producedBy"), &run_iri)?;
            sink.iri(&out_iri, &g("vantage"), &run_iri)?;
            sink.iri(&out_iri, &g("classifiedTarget"), &target.target_iri)?;
            sink.iri(&out_iri, &g("emittedLabel"), &label_iri)?;
            sink.decimal(&out_iri, &g("classifierScore"), cell.score)?;
            sink.iri(&out_iri, &g("scoreSemantics"), &semantics_iri)?;
            sink.decimal(&out_iri, &g("thresholdApplied"), threshold)?;
            if let Some(cal) = &capture.score_calibration {
                sink.string(&out_iri, &g("scoreCalibration"), cal)?;
            }

            if cell.score >= threshold {
                any_crossed = true;
                // Claim boundary (rule 5): the output SUPPORTS a claim, never
                // asserts inner affect. Only route a claim where the ontology
                // authors a reviewed closeMatch to an EmotionType.
                if let Some(m) = config.emotion_close_match.get(&label_iri) {
                    let claim_iri = format!("{out_iri}/claim");
                    sink.iri(&out_iri, &g("supportsAffectiveClaim"), &claim_iri)?;
                    sink.iri(&claim_iri, RDF_TYPE, &g("AffectiveClaim"))?;
                    sink.iri(&claim_iri, &g("vantage"), &run_iri)?;
                    sink.iri(&claim_iri, &g("observedFeature"), &target.target_iri)?;
                    sink.lit(
                        &claim_iri,
                        RDFS_LABEL,
                        RdfLiteral::language_tagged(
                            format!("the text expresses {}", m.word),
                            GMEOW_ENGLISH,
                        ),
                    )?;
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
            sink.iri(&decision_iri, RDF_TYPE, &g("AffectDecision"))?;
            sink.iri(&decision_iri, &g("vantage"), &run_iri)?;
            sink.iri(&decision_iri, &g("observedFeature"), &target.target_iri)?;
            sink.iri(
                &decision_iri,
                &g("decidedLabel"),
                &config.label_iri(&winner.label),
            )?;
            sink.iri(&decision_iri, &g("derivedByFunction"), &g("fnArgmax"))?;
            sink.boolean(
                &decision_iri,
                &g("decisionCrossedThreshold"),
                winner.score >= winner_threshold,
            )?;
            // The margin (top1 − top2) is the argmax confidence — recorded only when a
            // runner-up exists (a single top-1-score capture has none).
            if target.scores.len() >= 2 {
                let mut desc: Vec<f64> = target.scores.iter().map(|c| c.score).collect();
                desc.sort_by(|a, b| b.partial_cmp(a).expect("finite scores"));
                sink.decimal(&decision_iri, &g("decisionMargin"), desc[0] - desc[1])?;
            }
        } else if !any_crossed {
            // Multi-label / run-scoped: "concluded and flat" is not "never checked".
            let concluded_iri = mint_concluded_iri(&run_iri, &target.target_iri);
            sink.iri(&concluded_iri, RDF_TYPE, &g("AffectEvaluationConcluded"))?;
            sink.iri(&concluded_iri, &g("vantage"), &run_iri)?;
            sink.iri(&concluded_iri, &g("observedFeature"), &target.target_iri)?;
        }
    }

    sink.serialize()
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
    f64_decimal_lexical(value).parse().unwrap_or(value)
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

/// Read a finite recovered number, naming its source field in every refusal.
fn required_decimal(index: &TripleIndex, s: &str, p: &str) -> Result<f64, IngestError> {
    let value = required_literal(index, s, p)?
        .trim()
        .parse::<f64>()
        .map_err(|_| malformed(format!("{s} {p} is not a decimal")))?;
    if !value.is_finite() {
        return Err(malformed(format!("{s} {p} requires a finite decimal")));
    }
    Ok(value)
}

#[cfg(test)]
#[path = "lib.tests.rs"]
mod tests;
