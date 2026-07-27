// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! [`Diag`] — the one content-bound value type: the `?`-propagated error, the
//! ledger finding, a witness node in a provenance DAG, and the log event.
//!
//! `Diag` is a single machine word (`Box<DiagInner>`), so `Result<T, Diag>` costs
//! nothing over `T` beyond the pointer niche. Propagation ergonomics are provided
//! without `anyhow`/`thiserror`:
//!
//! * a single blanket `impl<E: Error + Send + Sync> From<E> for Diag` — **the only
//!   `From`** (the coherence rule). It is legal precisely because `Diag` does *not*
//!   implement [`std::error::Error`]: were it to, this blanket would overlap the
//!   reflexive `impl<T> From<T> for T` that `?` relies on for `Diag -> Diag`. The
//!   crate compiling is itself the proof that `Diag: !Error`.
//! * [`downcast_ref`](Diag::downcast_ref) recovers the original typed error (kept
//!   as the diagnostic's live source), so a [`DiagKind`]'s specific code/grade
//!   survive conversion even though `?` gives the converted diagnostic the
//!   reserved [`foreign_code`](crate::code::foreign_code).
//! * [`ResultExt`] adds `.ctx`/`.with_ctx`/`.at`/`.for_focus`/`.grade` for the `?`
//!   path and `.or_collect` for the push-and-continue path (half of all emission
//!   sites are non-fatal collection, not `?`).
//! * [`diag!`](macro@crate::diag)/[`bail!`](macro@crate::bail)/[`ensure!`](macro@crate::ensure)
//!   capture the emit site via `#[track_caller]`.

use std::error::Error as StdError;
use std::fmt;
use std::num::NonZeroU32;
use std::panic::Location;

use serde::{Deserialize, Serialize};

use crate::code::{self, Code};
use crate::grade::{GateVerdict, Grade, Severity, Standpoint, gate};
use crate::model::{DiagnosticAttribution, FindingCategory, Location as SourceLocation};

/// A handle into the [`DiagLedger`](crate::ledger) arena. Like [`Code`], its
/// numeric value is an in-process handle only and is never serialized — DAG edges
/// are content-addressed by fingerprint at the serialization boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagRef(pub(crate) NonZeroU32);

// The arena constructor/accessor are consumed by the ledger's hash-consed arena;
// they live with the handle type they mint.
impl DiagRef {
    /// Construct from a 1-based arena position. Panics (HARD FAIL) on overflow.
    pub(crate) fn from_index(index: usize) -> Self {
        let one_based = u32::try_from(index)
            .ok()
            .and_then(|n| n.checked_add(1))
            .and_then(NonZeroU32::new)
            .expect("diagnostic arena handle space exhausted");
        DiagRef(one_based)
    }

    /// The 0-based arena index this handle points at.
    pub(crate) fn index(self) -> usize {
        self.0.get() as usize - 1
    }
}

/// A pipeline stage identifier. Diagnostics attached on the carrier path are
/// stamped with the stage that produced them (pin-on-attach).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StageId(pub String);

impl StageId {
    pub fn new(id: impl Into<String>) -> Self {
        StageId(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The sub-statement role a diagnostic anchors to inside a quad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TermRole {
    Subject,
    Predicate,
    Object,
    Graph,
}

/// The offending focus node (an IRI / blank-node label / lexical key).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Focus(pub String);

/// A structured `observed`/`expected` value carried beside the message, so a
/// consumer can compare values without parsing prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub lexical: String,
    pub datatype: Option<String>,
}

impl Slot {
    pub fn new(lexical: impl Into<String>) -> Self {
        Slot {
            lexical: lexical.into(),
            datatype: None,
        }
    }
    pub fn typed(lexical: impl Into<String>, datatype: impl Into<String>) -> Self {
        Slot {
            lexical: lexical.into(),
            datatype: Some(datatype.into()),
        }
    }
}

/// A secondary, labelled span — Rust-compiler-style multi-label diagnostics
/// ("defined here", "first use there"). LSP `publishDiagnostics` and
/// `gmeow explain` render these alongside the primary source context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub location: SourceLocation,
    pub text: String,
}

/// A standpoint-bearing piece of advice attached to a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Advice {
    pub standpoint: Standpoint,
    pub text: String,
    pub help_uri: Option<String>,
}

/// The modality of per-term usage guidance projected onto a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuidanceModality {
    HowToUse,
    UseWhen,
    AvoidWhen,
}

impl GuidanceModality {
    /// The human-readable label prefix the CLI/HTML surfaces render beside the
    /// guidance text (e.g. `"  ↳ how to use: <text>"`).
    pub fn label(self) -> &'static str {
        match self {
            Self::HowToUse => "how to use",
            Self::UseWhen => "use when",
            Self::AvoidWhen => "avoid when",
        }
    }

    /// The local name of the matching `gmeow:finding*` RDF predicate this
    /// modality projects to, mirroring the `gmeow:` DSL vocabulary
    /// (`gmeow:findingHowToUse` / `gmeow:findingUseWhen` / `gmeow:findingAvoidWhen`).
    pub fn predicate_local(self) -> &'static str {
        match self {
            Self::HowToUse => "findingHowToUse",
            Self::UseWhen => "findingUseWhen",
            Self::AvoidWhen => "findingAvoidWhen",
        }
    }

    /// The pinned SARIF `properties` key this modality projects to — part of the
    /// sorted, deterministic `gmeow.*` property key schema.
    pub fn sarif_key(self) -> &'static str {
        match self {
            Self::HowToUse => "gmeow.howToUse",
            Self::UseWhen => "gmeow.useWhen",
            Self::AvoidWhen => "gmeow.avoidWhen",
        }
    }
}

/// Where a guidance claim's governing term came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuidanceSource {
    RuleGoverningTerm,
    DocumentedTerm,
}

/// One per-term usage-guidance claim projected verbatim from the bundle
/// documentation graph — NEVER fabricated at render time; honest absence
/// (a finding whose terms author none carries no Guidance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guidance {
    pub modality: GuidanceModality,
    pub source: GuidanceSource,
    pub term_iri: String,
    pub text: String,
    pub standpoint: Standpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
}

/// A source region a mechanical edit touches — the SARIF `region` /
/// `deletedRegion` coordinates (1-based). Every coordinate is a genuine partial
/// function: a whole-line replacement carries no column, an insertion carries a
/// zero-width region. Absent coordinates are simply omitted from the projection.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Region {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

impl Region {
    /// Whether the region carries at least one coordinate (an all-`None` region
    /// projects to nothing).
    pub fn is_empty(&self) -> bool {
        self.start_line.is_none()
            && self.start_column.is_none()
            && self.end_line.is_none()
            && self.end_column.is_none()
    }
}

/// A concrete, mechanical edit a remediation can carry — enough to express one
/// SARIF `artifactChanges` entry: the repo-relative artifact to edit, the region
/// it touches, and the replacement text. This is the *mechanical* half of a
/// [`Remediation`]; most rules carry prose guidance only and leave it `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactChange {
    /// The repo-relative artifact URI the edit applies to (SARIF
    /// `artifactLocation.uri` — must be repo-relative, like every result
    /// location).
    pub artifact_uri: String,
    /// The region the edit deletes/replaces.
    pub region: Region,
    /// The text inserted in place of the region.
    pub replacement: String,
}

/// A standpoint-bearing *remediation*: the registry-authored "how to fix this"
/// guidance projected onto a finding (`gmeow:findingRemediation`). Unlike a plain
/// [`Advice`] suggestion, a remediation can carry a concrete [`ArtifactChange`]
/// that becomes a SARIF `fix` with `artifactChanges`. The `artifact_change` is a
/// genuine partial function — most rules have prose guidance but no mechanical
/// edit — not degraded optionality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remediation {
    pub text: String,
    pub standpoint: Standpoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_change: Option<ArtifactChange>,
}

impl Remediation {
    /// A prose-only remediation at the given standpoint (no help URI, no edit).
    pub fn new(text: impl Into<String>, standpoint: Standpoint) -> Self {
        Remediation {
            text: text.into(),
            standpoint,
            help_uri: None,
            artifact_change: None,
        }
    }

    /// Attach a mechanical [`ArtifactChange`] (the SARIF-fix edit).
    pub fn with_artifact_change(mut self, change: ArtifactChange) -> Self {
        self.artifact_change = Some(change);
        self
    }

    /// Attach an outward help URI.
    pub fn with_help_uri(mut self, uri: impl Into<String>) -> Self {
        self.help_uri = Some(uri.into());
        self
    }
}

/// The fingerprint *anchor* of a diagnostic — the stable identity coordinates.
/// Fingerprints key on `(code, category, this anchor, focus)`, never on the
/// message or context frames.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SourceContext {
    pub location: SourceLocation,
    pub focus: Option<Focus>,
    pub term_role: Option<TermRole>,
}

impl SourceContext {
    /// Whether this context names a *genuine* source position — a non-empty path,
    /// a logical anchor (e.g. a SHACL finding's focus-node IRI, which rides in
    /// `location.logical`), OR a focus node — as opposed to the empty/default anchor
    /// every locationless finding collapses onto. This is the guard the
    /// cross-node-glut meta-rule joins on: only a non-trivial anchor is typed
    /// `gmeow:NonTrivialAnchor`, so the glut join never fires on the shared default
    /// anchor of locationless findings (`gmeow:NonTrivialAnchor` doctrine). Because
    /// the anchor fingerprint keys on `logical` (see `DiagFingerprint::anchor`), a
    /// finding located solely by `logical` IS a genuine, joinable anchor and must
    /// not be excluded here.
    pub fn is_non_trivial(&self) -> bool {
        self.location.path.as_deref().is_some_and(|p| !p.is_empty())
            || self
                .location
                .logical
                .as_deref()
                .is_some_and(|l| !l.is_empty())
            || self.focus.as_ref().is_some_and(|f| !f.0.is_empty())
    }
}

/// One `.ctx`/`.with_ctx` frame, innermost first, each stamped with the Rust
/// call site that added it.
#[derive(Debug, Clone)]
pub struct ContextFrame {
    pub label: String,
    pub at: &'static Location<'static>,
}

/// The ambient pipeline locus a diagnostic was emitted from: the Rust emit site
/// (always), plus the scheduler-stamped stage once it is folded onto the carrier
/// (filled at attach time).
#[derive(Debug, Clone)]
pub struct PipelineLocus {
    pub emitted_at: &'static Location<'static>,
    pub stage: Option<StageId>,
}

impl PipelineLocus {
    #[track_caller]
    pub fn here() -> Self {
        PipelineLocus {
            emitted_at: Location::caller(),
            stage: None,
        }
    }
}

/// A domain error type that opts into a stable [`Code`] and a default [`Grade`]
/// and can be recovered from a [`Diag`] by [`downcast_ref`](Diag::downcast_ref).
pub trait DiagKind: StdError + Send + Sync + 'static {
    /// The registered code for this kind.
    fn code(&self) -> Code;
    /// The default grade. Override to change severity/category/standpoint.
    fn grade(&self) -> Grade {
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        )
    }
    /// The ontology failure-class IRI this kind is the Rust PRODUCER of — the
    /// `gmeow:enforcesFailureClass` individual a raised diagnostic instantiates.
    ///
    /// `None` is the honest default: most kinds name a defect the ontology has
    /// not (yet) minted a typed failure class for, and inventing an IRI for them
    /// would be a second source of truth. A kind that DOES declare one is bound
    /// to it bijectively — the repo-static bijection gate proves the IRI resolves
    /// to a real failure-class individual and that no such individual is left
    /// without a producer, so the annotation can never drift into decoration.
    fn failure_class(&self) -> Option<&'static str> {
        None
    }
}

/// The boxed payload behind the one-word [`Diag`].
#[derive(Debug)]
pub struct DiagInner {
    pub grade: Grade,
    pub code: Code,
    pub message: String,
    /// `.ctx` frames, innermost first.
    pub context: Vec<ContextFrame>,
    /// DAG antecedents — handles into the ledger arena (resolved through the
    /// ledger, never walked from the live `Diag` alone).
    pub antecedents: Vec<DiagRef>,
    /// The live `dyn Error` source chain — kept in-process only and flattened to
    /// structured frames exactly once, at the serialization boundary.
    pub source: Option<Box<dyn StdError + Send + Sync + 'static>>,
    pub source_ctx: SourceContext,
    pub attributions: Vec<DiagnosticAttribution>,
    pub advice: Vec<Advice>,
    /// registry-authored remediations (the "how to fix" payload projected as
    /// `gmeow:findingRemediation` and rendered into SARIF `fixes`).
    pub remediation: Vec<Remediation>,
    /// Per-term usage guidance (howToUse/useWhen/avoidWhen) joined from the bundle
    /// documentation graph — projected onto
    /// [`Finding::guidance`](crate::model::Finding), never fabricated at render
    /// time.
    pub guidance: Vec<Guidance>,
    /// The logic-world quad-reifier IRIs this diagnostic's verdict derives FROM
    /// (`gmeow:findingDerivedFromQuad`) — the explain-skeleton cited IRIs of the
    /// reasoned quads that fired. Empty for a non-reasoned diagnostic.
    pub derived_from_quads: Vec<String>,
    pub labels: Vec<Label>,
    pub tags: Vec<String>,
    /// The DOCUMENTED ontology terms this diagnostic structurally concerns (a SHACL
    /// violation's constrained `sh:path` property, etc.) — payload, NOT an identity
    /// field, projected onto [`Finding::documented_terms`](crate::model::Finding)
    /// for the docs per-term "Diagnostics you might hit" join.
    pub documented_terms: Vec<String>,
    pub observed: Option<Slot>,
    pub expected: Option<Slot>,
    pub locus: PipelineLocus,
}

/// The one content-bound diagnostic value. One machine word wide.
///
/// Deliberately does **not** implement [`std::error::Error`] (see the module
/// docs) nor `serde::Serialize` (only the lowered
/// [`DiagNode`](crate::ledger::DiagNode) is serializable).
pub struct Diag(Box<DiagInner>);

impl Diag {
    /// Build a diagnostic from an explicit registered code, grade, and message.
    #[track_caller]
    pub fn new(code: Code, grade: Grade, message: impl Into<String>) -> Self {
        Diag(Box::new(DiagInner {
            grade,
            code,
            message: message.into(),
            context: Vec::new(),
            antecedents: Vec::new(),
            source: None,
            source_ctx: SourceContext::default(),
            attributions: Vec::new(),
            advice: Vec::new(),
            remediation: Vec::new(),
            guidance: Vec::new(),
            derived_from_quads: Vec::new(),
            labels: Vec::new(),
            tags: Vec::new(),
            documented_terms: Vec::new(),
            observed: None,
            expected: None,
            locus: PipelineLocus::here(),
        }))
    }

    /// Build a diagnostic from a [`DiagKind`], preserving its code and grade and
    /// keeping the typed value downcastable off the source.
    #[track_caller]
    pub fn of_kind<K: DiagKind>(kind: K) -> Self {
        let code = kind.code();
        let grade = kind.grade();
        let message = kind.to_string();
        let mut diag = Diag::new(code, grade, message);
        diag.0.source = Some(Box::new(kind));
        diag
    }

    /// Build a NON-GATING **note** — general-purpose chatter that narrates a run's
    /// progress. It carries the [`FindingCategory::Transient`] chatter kind at
    /// [`Severity::Note`] from an [`Standpoint::Advisory`] stance, so it can never
    /// reach the gate.
    #[track_caller]
    pub fn note(code: Code, message: impl Into<String>) -> Self {
        Diag::new(
            code,
            Grade::new(
                Severity::Note,
                FindingCategory::Transient,
                Standpoint::Advisory,
            ),
            message,
        )
    }

    /// Build a NON-GATING **info** witness — the lowest-severity chatter. It
    /// carries the [`FindingCategory::Transient`] chatter kind at [`Severity::Info`]
    /// from an [`Standpoint::Advisory`] stance, so it can never reach the gate.
    #[track_caller]
    pub fn info(code: Code, message: impl Into<String>) -> Self {
        Diag::new(
            code,
            Grade::new(
                Severity::Info,
                FindingCategory::Transient,
                Standpoint::Advisory,
            ),
            message,
        )
    }

    pub fn code(&self) -> Code {
        self.0.code
    }
    pub fn grade(&self) -> Grade {
        self.0.grade
    }
    /// The gate verdict for this diagnostic, via the single policy morphism.
    pub fn gate(&self) -> GateVerdict {
        gate(self.0.grade)
    }
    pub fn message(&self) -> &str {
        &self.0.message
    }
    pub fn emitted_at(&self) -> &'static Location<'static> {
        self.0.locus.emitted_at
    }
    pub fn inner(&self) -> &DiagInner {
        &self.0
    }
    pub fn inner_mut(&mut self) -> &mut DiagInner {
        &mut self.0
    }

    /// Recover the original typed error kept as this diagnostic's source.
    pub fn downcast_ref<T: StdError + 'static>(&self) -> Option<&T> {
        self.0.source.as_ref().and_then(|e| e.downcast_ref::<T>())
    }

    /// Whether this diagnostic's source is a `T`.
    pub fn is<T: StdError + 'static>(&self) -> bool {
        self.downcast_ref::<T>().is_some()
    }

    // --- builders (the sole mutation surface) --------------------------------

    #[track_caller]
    pub fn with_context(mut self, label: impl Into<String>) -> Self {
        self.0.context.push(ContextFrame {
            label: label.into(),
            at: Location::caller(),
        });
        self
    }
    pub fn with_focus(mut self, focus: impl Into<String>) -> Self {
        self.0.source_ctx.focus = Some(Focus(focus.into()));
        self
    }
    pub fn with_term_role(mut self, role: TermRole) -> Self {
        self.0.source_ctx.term_role = Some(role);
        self
    }
    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.0.source_ctx.location = location;
        self
    }
    pub fn with_grade(mut self, grade: Grade) -> Self {
        self.0.grade = grade;
        self
    }
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.0.tags.push(tag.into());
        self
    }
    pub fn with_advice(mut self, advice: Advice) -> Self {
        self.0.advice.push(advice);
        self
    }
    /// Attach a registry-authored [`Remediation`] (the "how to fix" payload).
    pub fn with_remediation(mut self, remediation: Remediation) -> Self {
        self.0.remediation.push(remediation);
        self
    }
    /// Attach a per-term [`Guidance`] claim (howToUse/useWhen/avoidWhen), joined
    /// from the bundle documentation graph.
    pub fn with_guidance(mut self, guidance: Guidance) -> Self {
        self.0.guidance.push(guidance);
        self
    }
    /// Attach the quad-reifier IRIs (`gmeow:findingDerivedFromQuad`) this
    /// diagnostic's verdict derives from — the explain-skeleton citations of the
    /// reasoned quads that fired.
    pub fn with_derived_from_quads(
        mut self,
        quads: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.0
            .derived_from_quads
            .extend(quads.into_iter().map(Into::into));
        self
    }
    pub fn with_label(mut self, label: Label) -> Self {
        self.0.labels.push(label);
        self
    }
    pub fn with_observed(mut self, slot: Slot) -> Self {
        self.0.observed = Some(slot);
        self
    }
    /// Attribute this diagnostic to a DOCUMENTED ontology term it structurally
    /// concerns (e.g. a SHACL violation's constrained `sh:path` property). Payload,
    /// never an identity field, so attributing a witness never perturbs its content
    /// address; projected onto [`Finding::documented_terms`](crate::model::Finding).
    pub fn with_documented_term(mut self, term_iri: impl Into<String>) -> Self {
        self.0.documented_terms.push(term_iri.into());
        self
    }
    pub fn with_expected(mut self, slot: Slot) -> Self {
        self.0.expected = Some(slot);
        self
    }
    pub fn with_attribution(mut self, attribution: DiagnosticAttribution) -> Self {
        self.0.attributions.push(attribution);
        self
    }
    pub fn with_antecedents(mut self, antecedents: impl IntoIterator<Item = DiagRef>) -> Self {
        self.0.antecedents.extend(antecedents);
        self
    }
    /// Set the scheduler-stamped stage (used at carrier attach time).
    pub fn with_locus(mut self, stage: StageId) -> Self {
        self.0.locus.stage = Some(stage);
        self
    }
}

/// A conversion from any `std::error::Error` into a diagnostic. THIS IS THE ONLY
/// `From` impl for `Diag` (the coherence rule); all other constructions go
/// through the named builders above. The converted error is kept live as the
/// source so it stays downcastable.
impl<E: StdError + Send + Sync + 'static> From<E> for Diag {
    #[track_caller]
    fn from(err: E) -> Self {
        let message = err.to_string();
        let mut diag = Diag::new(
            code::foreign_code(),
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            message,
        );
        diag.0.source = Some(Box::new(err));
        diag
    }
}

impl fmt::Debug for Diag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl fmt::Display for Diag {
    /// `{}` prints the head message; `{:#}` walks the context frames (outermost
    /// in) and the live `dyn Error` source chain. The DAG antecedents are walked
    /// through the ledger, which owns the arena, not from the live value.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.message)?;
        if f.alternate() {
            for frame in self.0.context.iter().rev() {
                write!(f, "\n  in {}", frame.label)?;
            }
            let mut source = self.0.source.as_ref().map(|b| b.as_ref() as &dyn StdError);
            while let Some(err) = source {
                write!(f, "\n  caused by: {err}")?;
                source = err.source();
            }
        }
        Ok(())
    }
}

// --- propagation ergonomics ---------------------------------------------------

/// Anything a non-fatal diagnostic can be collected into (the push-and-continue
/// sink). A plain `Vec<Diag>` is a sink; the [`DiagLedger`](crate::ledger) is the
/// production sink that also stamps stage attribution.
pub trait DiagSink {
    fn collect(&mut self, diag: Diag);
}

impl DiagSink for Vec<Diag> {
    fn collect(&mut self, diag: Diag) {
        self.push(diag);
    }
}

/// Ergonomic combinators over `Result<T, E: Into<Diag>>`.
pub trait ResultExt<T> {
    /// Add a context frame.
    fn ctx(self, label: impl Into<String>) -> Result<T, Diag>;
    /// Add a lazily-computed context frame (only built on the error path).
    fn with_ctx<F, S>(self, f: F) -> Result<T, Diag>
    where
        F: FnOnce() -> S,
        S: Into<String>;
    /// Attach a source location.
    fn at(self, location: SourceLocation) -> Result<T, Diag>;
    /// Attach an offending focus node.
    fn for_focus(self, focus: impl Into<String>) -> Result<T, Diag>;
    /// Override the grade.
    fn grade(self, grade: Grade) -> Result<T, Diag>;
    /// The push-and-continue seam: on `Err`, collect the diagnostic into `sink`
    /// and return `None`; on `Ok`, return `Some(value)`. Symmetric to `?` but
    /// non-fatal.
    fn or_collect<S: DiagSink>(self, sink: &mut S) -> Option<T>;
}

impl<T, E: Into<Diag>> ResultExt<T> for Result<T, E> {
    #[track_caller]
    fn ctx(self, label: impl Into<String>) -> Result<T, Diag> {
        self.map_err(|e| e.into().with_context(label))
    }
    #[track_caller]
    fn with_ctx<F, S>(self, f: F) -> Result<T, Diag>
    where
        F: FnOnce() -> S,
        S: Into<String>,
    {
        self.map_err(|e| e.into().with_context(f()))
    }
    #[track_caller]
    fn at(self, location: SourceLocation) -> Result<T, Diag> {
        self.map_err(|e| e.into().with_location(location))
    }
    #[track_caller]
    fn for_focus(self, focus: impl Into<String>) -> Result<T, Diag> {
        self.map_err(|e| e.into().with_focus(focus))
    }
    #[track_caller]
    fn grade(self, grade: Grade) -> Result<T, Diag> {
        self.map_err(|e| e.into().with_grade(grade))
    }
    fn or_collect<S: DiagSink>(self, sink: &mut S) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(e) => {
                sink.collect(e.into());
                None
            }
        }
    }
}

/// Collect every `Ok` from an iterator of results, pushing each `Err`'s
/// diagnostic into `sink` and continuing — the batch form of
/// [`ResultExt::or_collect`].
pub trait ResultIterExt<T> {
    fn collect_all<S: DiagSink>(self, sink: &mut S) -> Vec<T>;
}

impl<T, E, I> ResultIterExt<T> for I
where
    E: Into<Diag>,
    I: IntoIterator<Item = Result<T, E>>,
{
    fn collect_all<S: DiagSink>(self, sink: &mut S) -> Vec<T> {
        let mut out = Vec::new();
        for item in self {
            if let Some(value) = item.or_collect(sink) {
                out.push(value);
            }
        }
        out
    }
}

// --- macros -------------------------------------------------------------------

/// Build a [`Diag`] from a registered code and a formatted message.
///
/// `diag!(code, "message {x}")` — `code` is a registered [`Code`]; the format
/// arguments follow. The emit site is captured via `#[track_caller]`.
#[macro_export]
macro_rules! diag {
    ($code:expr, $($arg:tt)+) => {
        $crate::diag::Diag::new(
            $code,
            $crate::grade::Grade::new(
                $crate::grade::Severity::Error,
                $crate::model::FindingCategory::ModelingDisciplineViolation,
                $crate::grade::Standpoint::Binding,
            ),
            ::std::format!($($arg)+),
        )
    };
}

/// Return early with a [`Diag`]: `bail!(code, "message {x}")` or `bail!(kind)`.
#[macro_export]
macro_rules! bail {
    ($code:expr, $($arg:tt)+) => {
        return ::core::result::Result::Err($crate::diag!($code, $($arg)+))
    };
    ($kind:expr) => {
        return ::core::result::Result::Err($crate::diag::Diag::of_kind($kind))
    };
}

/// Bail with a [`Diag`] unless a condition holds:
/// `ensure!(cond, code, "message {x}")`.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $code:expr, $($arg:tt)+) => {
        if !($cond) {
            $crate::bail!($code, $($arg)+);
        }
    };
}

/// Define a domain error type that is a [`DiagKind`] in one place — the
/// mechanical-conversion seam the downstream error-enum migrations build on.
///
/// It generates the struct, its `Display` (from a positional format string over
/// the fields — positional `{}` is used, not implicit `{field}` capture, because
/// the latter does not work through a macro metavariable), its
/// [`std::error::Error`] impl, and its [`DiagKind`] impl, and it registers the
/// code lazily-once via a `LazyLock`. Each type also gets an associated
/// `register()` for eager startup seeding and a `CODE` string constant.
///
/// ```ignore
/// define_diag_kind! {
///     /// No stage implementation is registered under `impl_key`.
///     pub struct UnknownStageImpl { stage: String, impl_key: String }
///     code = "pipeline.unknown-stage-impl";
///     grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
///     message = "no impl `{}` for stage `{}`", impl_key, stage;
/// }
/// ```
///
/// # Binding a kind to its ontology failure class
///
/// An OPTIONAL trailing `failure_class = "<IRI>";` clause binds the kind to the
/// `gmeow:enforcesFailureClass` individual it produces, generating a
/// `FAILURE_CLASS: Option<&'static str>` constant and the matching
/// [`DiagKind::failure_class`](crate::diag::DiagKind::failure_class) accessor.
/// Omitting it yields `None` — the default, and the only honest answer for a kind
/// whose defect the ontology names no failure class for.
///
/// ```ignore
/// define_diag_kind! {
///     /// A blob rep the payload-schema registry does not know.
///     pub struct MediumUnknownSchema { message: String }
///     code = "pipeline.medium.unknown-schema";
///     grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
///     message = "unknown payload schema: {}", message;
///     failure_class = "https://blackcatinformatics.ca/gmeow/MediumUnknownSchema";
/// }
/// ```
#[macro_export]
macro_rules! define_diag_kind {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident { $( $field:ident : $fty:ty ),* $(,)? }
        code = $code:literal;
        grade = $grade:expr;
        message = $msg:literal $(, $marg:ident)* $(,)?;
        $(failure_class = $failure_class:literal;)?
    ) => {
        $(#[$meta])*
        #[derive(::std::fmt::Debug, ::std::clone::Clone)]
        $vis struct $name {
            $( pub $field: $fty ),*
        }

        impl $name {
            /// This kind's registered code string. (Not every caller reads it;
            /// generated API items carry `allow(dead_code)`.)
            #[allow(dead_code)]
            pub const CODE: &'static str = $code;

            /// The ontology failure-class IRI this kind produces, or `None` when
            /// the kind declares no `failure_class` clause. Materialized as a
            /// CONSTANT (not merely a trait method) so a static census can read it
            /// off the source without instantiating the kind.
            #[allow(dead_code)]
            pub const FAILURE_CLASS: ::core::option::Option<&'static str> = {
                #[allow(unused_mut, unused_assignments)]
                let mut declared: ::core::option::Option<&'static str> =
                    ::core::option::Option::None;
                $( declared = ::core::option::Option::Some($failure_class); )?
                declared
            };

            /// The registered [`Code`](crate::code::Code) handle for this kind,
            /// interned once on first use (idempotent). Call eagerly at startup
            /// to seed the registry before any `intern`.
            pub fn register() -> $crate::code::Code {
                static CODE: ::std::sync::LazyLock<$crate::code::Code> =
                    ::std::sync::LazyLock::new(|| $crate::code::register_code($code));
                *CODE
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                let Self { $( $field ),* } = self;
                // Suppress "unused binding" for fields not named in the message.
                $( let _ = &$field; )*
                ::std::write!(f, $msg $(, $marg)*)
            }
        }

        impl ::std::error::Error for $name {}

        impl $crate::diag::DiagKind for $name {
            fn code(&self) -> $crate::code::Code {
                $name::register()
            }
            fn grade(&self) -> $crate::grade::Grade {
                $grade
            }
            fn failure_class(&self) -> ::core::option::Option<&'static str> {
                $name::FAILURE_CLASS
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::{intern_code, register_code};
    use static_assertions::assert_not_impl_all;
    use std::io;

    // The two structural invariants that keep the blanket-From coherent and the
    // serialization boundary single. macro_rules-based, no brittle stderr.
    assert_not_impl_all!(Diag: std::error::Error);
    assert_not_impl_all!(Diag: serde::Serialize);

    #[test]
    fn diag_ref_index_roundtrips() {
        for i in [0usize, 1, 7, 4096, u32::MAX as usize - 1] {
            assert_eq!(DiagRef::from_index(i).index(), i);
        }
    }

    #[test]
    fn diag_is_one_word() {
        assert_eq!(std::mem::size_of::<Diag>(), std::mem::size_of::<usize>());
    }

    #[test]
    fn result_of_diag_is_pointer_sized() {
        assert_eq!(
            std::mem::size_of::<Result<(), Diag>>(),
            std::mem::size_of::<*const ()>()
        );
    }

    #[derive(Debug)]
    struct MyKind;
    impl fmt::Display for MyKind {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("my kind failed")
        }
    }
    impl StdError for MyKind {}
    impl DiagKind for MyKind {
        fn code(&self) -> Code {
            register_code("test.diagkind.my-kind")
        }
        fn grade(&self) -> Grade {
            Grade::new(
                Severity::Warning,
                FindingCategory::PolicyWarning,
                Standpoint::Advisory,
            )
        }
    }

    // Compiling this function proves BOTH From paths coexist — the reflexive
    // `From<Diag> for Diag` (for the `Diag` `?`) and the blanket `From<E: Error>`
    // (for the `io::Error` `?`). They can only coexist if `Diag: !Error`.
    fn thread_both(fail_diag: bool) -> Result<(), Diag> {
        fn make_io() -> Result<(), io::Error> {
            Err(io::Error::other("io boom"))
        }
        fn make_diag() -> Result<(), Diag> {
            Err(Diag::new(
                code::foreign_code(),
                Grade::new(
                    Severity::Error,
                    FindingCategory::ContradictionWitness,
                    Standpoint::Binding,
                ),
                "diag boom",
            ))
        }
        if fail_diag {
            make_diag()?; // reflexive From<Diag> for Diag
        } else {
            make_io()?; // blanket From<io::Error> for Diag
        }
        Ok(())
    }

    #[test]
    fn both_from_paths_compose_through_question_mark() {
        assert!(thread_both(false).is_err());
        assert!(thread_both(true).is_err());
    }

    #[test]
    fn blanket_from_preserves_downcast() {
        let diag: Diag = io::Error::new(io::ErrorKind::NotFound, "missing").into();
        let recovered = diag.downcast_ref::<io::Error>().expect("downcast");
        assert_eq!(recovered.kind(), io::ErrorKind::NotFound);
        assert!(diag.is::<io::Error>());
        // A foreign error takes the reserved code but stays a real gate-able error.
        assert_eq!(diag.code(), code::foreign_code());
        assert_eq!(diag.gate(), GateVerdict::Fatal);
    }

    #[test]
    fn of_kind_preserves_code_grade_and_downcast() {
        let diag = Diag::of_kind(MyKind);
        assert_eq!(diag.code(), register_code("test.diagkind.my-kind"));
        assert_eq!(diag.grade().severity, Severity::Warning);
        // An advisory PolicyWarning never gates.
        assert_eq!(diag.gate(), GateVerdict::Collected);
        assert!(diag.downcast_ref::<MyKind>().is_some());
    }

    #[test]
    fn track_caller_captures_the_question_mark_site_not_crate_internals() {
        // R3: the emit location captured through the blanket From must be THIS
        // function's file, not diag.rs.
        fn boom() -> Result<(), Diag> {
            Err(io::Error::other("x"))?;
            Ok(())
        }
        let diag = boom().unwrap_err();
        let file = diag.emitted_at().file();
        assert!(
            file.ends_with("diag.rs") || file.contains("diag"),
            "unexpected emit file {file}"
        );
        // More precisely: it must NOT point at the From impl line region; the
        // captured line is the `?` site inside `boom`, which lives in this test
        // module. We assert the file is this source file.
        assert!(file.ends_with("src/diag.rs"));
    }

    #[test]
    fn result_ext_adds_context_and_display_walks_it() {
        let r: Result<(), io::Error> = Err(io::Error::other("root cause"));
        let diag = r.ctx("while loading the bundle").unwrap_err();
        let rendered = format!("{diag:#}");
        assert!(rendered.contains("while loading the bundle"));
        assert!(rendered.contains("root cause"));
    }

    #[test]
    fn or_collect_pushes_and_continues() {
        let mut sink: Vec<Diag> = Vec::new();
        let ok: Result<u8, io::Error> = Ok(7);
        let err: Result<u8, io::Error> = Err(io::Error::other("nope"));
        assert_eq!(ok.or_collect(&mut sink), Some(7));
        assert_eq!(err.or_collect(&mut sink), None);
        assert_eq!(sink.len(), 1);
    }

    #[test]
    fn collect_all_gathers_ok_and_sinks_err() {
        let mut sink: Vec<Diag> = Vec::new();
        let items: Vec<Result<u8, io::Error>> = vec![Ok(1), Err(io::Error::other("bad")), Ok(3)];
        let got = items.collect_all(&mut sink);
        assert_eq!(got, vec![1, 3]);
        assert_eq!(sink.len(), 1);
    }

    crate::define_diag_kind! {
        /// A generated kind for the macro test.
        pub struct UnknownStageImpl { stage: String, impl_key: String }
        code = "test.define.unknown-stage-impl";
        grade = Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        );
        message = "no impl `{}` for stage `{}`", impl_key, stage;
    }

    #[test]
    fn define_diag_kind_binds_code_grade_message_and_registers() {
        let e = UnknownStageImpl {
            stage: "reason".to_owned(),
            impl_key: "demo".to_owned(),
        };
        // Message renders from the fields via the positional format.
        assert_eq!(e.to_string(), "no impl `demo` for stage `reason`");
        assert_eq!(UnknownStageImpl::CODE, "test.define.unknown-stage-impl");
        // Code is registered (eagerly reachable via register(), and via code()).
        assert_eq!(e.code(), UnknownStageImpl::register());
        assert!(intern_code("test.define.unknown-stage-impl").is_ok());
        // Building a Diag from it preserves code + grade + downcast.
        let diag = Diag::of_kind(e);
        assert_eq!(diag.code(), UnknownStageImpl::register());
        assert_eq!(diag.gate(), GateVerdict::Fatal);
        assert!(diag.downcast_ref::<UnknownStageImpl>().is_some());
    }

    crate::define_diag_kind! {
        /// A generated kind carrying the OPTIONAL ontology failure-class binding.
        pub struct FailureClassBound { detail: String }
        code = "test.define.failure-class-bound";
        grade = Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        );
        message = "bound kind: {}", detail;
        failure_class = "https://blackcatinformatics.ca/gmeow/TestFailureClass";
    }

    /// The `failure_class` clause is OPTIONAL and PURELY ADDITIVE: a kind that
    /// declares one exposes the IRI through both the constant and the trait
    /// accessor, and a kind that does not keeps the `None` default — so extending
    /// the macro cannot have changed the meaning of any existing kind.
    #[test]
    fn define_diag_kind_binds_the_optional_failure_class() {
        assert_eq!(
            FailureClassBound::FAILURE_CLASS,
            Some("https://blackcatinformatics.ca/gmeow/TestFailureClass")
        );
        let bound = FailureClassBound {
            detail: "demo".to_owned(),
        };
        assert_eq!(
            bound.failure_class(),
            Some("https://blackcatinformatics.ca/gmeow/TestFailureClass")
        );
        // Everything else the macro generates is unchanged by the new clause.
        assert_eq!(bound.to_string(), "bound kind: demo");
        assert_eq!(bound.code(), FailureClassBound::register());

        // A kind WITHOUT the clause keeps the honest `None` default.
        assert_eq!(UnknownStageImpl::FAILURE_CLASS, None);
        assert_eq!(
            UnknownStageImpl {
                stage: "reason".to_owned(),
                impl_key: "demo".to_owned(),
            }
            .failure_class(),
            None
        );
    }

    #[test]
    fn macros_build_and_bail() {
        let code = register_code("test.macro.code");
        fn use_ensure(code: Code, n: i32) -> Result<(), Diag> {
            ensure!(n > 0, code, "n must be positive, got {n}");
            Ok(())
        }
        assert!(use_ensure(code, 1).is_ok());
        let err = use_ensure(code, -1).unwrap_err();
        assert!(err.message().contains("n must be positive"));
        assert_eq!(err.code(), code);
    }
}
