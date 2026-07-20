// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Advisory-tier emission seam — the D1 keystone.
//!
//! # Dual-projection contract
//!
//! One [`Advisory`] value produces, via a single [`Advisory::project`] call,
//! BOTH projections simultaneously:
//!
//! 1. A graded [`gmeow_errors::Diag`] at [`Severity::Note`] or
//!    [`Severity::Info`], carried at [`Standpoint::Advisory`] — it interns onto
//!    the run [`gmeow_errors::DiagLedger`] and projects to the linter/SARIF/CLI
//!    surface every existing diagnostics renderer consumes.
//! 2. An in-memory [`AdvisoryClaim`] hook carrying the vantage IRI, the advised
//!    proposition, and the deontic-modality IRI.  (D4) will later
//!    materialise this hook into a `gmeow:ComplianceAssessment` /
//!    `deonticRecommendation` RDF claim.  **D1 emits no RDF** — the claim lives
//!    in memory only until D4 consumes it.
//!
//! The projection is *unconditional*: there is no opt-in flag.  Every advisory
//! always produces both wings.  This is the "dual-projection-always" contract
//! (P4/P17).
//!
//! # Standpoint vantage
//!
//! Advice is a perspectival claim, never a global verdict (P9).  Every advisory
//! is issued from an explicit `gmeow:Standpoint`.  The default is
//! [`BEST_PRACTICE_STANDPOINT_IRI`]; callers may substitute another IRI to
//! represent a different advisory vantage (e.g. a domain-specific style guide).
//! D4 reconciles the string IRI with a real `gmeow:Standpoint` individual
//! when it materialises the RDF claim.

use gmeow_errors::render::nq_escape;
use gmeow_errors::{
    Advice, Diag, FindingCategory, Grade, Location, Rule, Severity, Standpoint, register_code,
};
use purrdf::shapes::report::{Severity as ShaclSeverity, ValidationReport, ValidationResult};
use purrdf::shapes::term::Term as ShaclTerm;
use sha2::{Digest, Sha256};

// ── Standpoint & modality constants ─────────────────────────────────────────

/// The canonical best-practice standpoint IRI an advisory is issued from — the
/// `gmeow:vantage` of the recommendation claim (P9: advice is one standpoint's
/// perspectival claim, never a global verdict).  This is the REAL in-graph
/// `gmeow:Standpoint` individual (`gmeow:gmeowBestPractice`) materialised by
/// (D4); D1 already carried the string form, and (D4)'s emitter resolves it
/// to the individual's RDF triples.
pub const BEST_PRACTICE_STANDPOINT_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/gmeowBestPractice";

/// The `gmeow:deonticRecommendation` modality individual IRI ("the issuer
/// advises the conduct without requiring it").  The soft-tier mirror of
/// `gmeow:deonticObligation`.
pub const DEONTIC_RECOMMENDATION_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/deonticRecommendation";

/// The advisor's STATED certainty in the recommendation — NOT a tuned or
/// calibrated metric (per the project's quality-metrics discipline: this is a
/// declared confidence the advisory carries, never a score fitted to a
/// target).  [`Advisory::note`] seeds every advisory with this value; callers
/// override it via [`Advisory::with_confidence`] when the advisor has a
/// different stated certainty.
pub const ADVISORY_DEFAULT_CONFIDENCE: f64 = 1.0;

/// The `gmeow:verdictNotHeld` individual IRI — the default
/// `gmeow:complianceVerdict` for an advisory claim.  Advice is a
/// recommendation, not an assertion that a norm IS held, so the un-overridden
/// verdict is "not (yet) held" rather than any positive compliance verdict.
pub const VERDICT_NOT_HELD_IRI: &str = "https://blackcatinformatics.ca/gmeow/verdictNotHeld";

/// The base IRI under which (D4) mints the per-claim `gmeow:Norm` /
/// `gmeow:Event` / `gmeow:ComplianceAssessment` triple of individuals, keyed
/// by the claim's diagnostic `code`: `{NORM_CLAIMS_BASE_IRI}{code}/{norm,event,assessment}`.
pub const NORM_CLAIMS_BASE_IRI: &str = "https://blackcatinformatics.ca/gmeow/norm-claims/";

/// The `gmeow:NormativeSystem` individual every best-practice recommendation
/// norm is `gmeow:partOf`.
pub const BEST_PRACTICE_NORMATIVE_SYSTEM_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/gmeowBestPracticeRecommendations";

/// The `gmeow:temporalFrameUTCGregorian` individual every advisory-claim
/// `gmeow:Event` carries as its exactly-one `gmeow:eventTemporalFrame`.
/// Deliberately NOT a wall-clock `gmeow:eventTime` — the advisory-conduct
/// event is a claim anchor, not a timestamped occurrence, and a fixed
/// temporal-frame individual keeps the projected N-Quads byte-deterministic.
const EVENT_TEMPORAL_FRAME_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/temporalFrameUTCGregorian";

/// The `gmeow:EventType` the advisory-conduct event carries — `gmeow:eventTypeAudit`.
/// A ComplianceAssessment is a compliance review of the assessed conduct, so an audit
/// event type is the apt kind; it also satisfies the advisory "an event should carry a
/// gmeow:eventType or a temporal placement" modeling shape without a synthetic timestamp.
const EVENT_TYPE_IRI: &str = "https://blackcatinformatics.ca/gmeow/eventTypeAudit";

/// A pinned, stable `xsd:decimal` lexical form for a confidence value:
/// exactly one fractional digit (`"1.0"`, `"0.5"`, `"0.0"`).  The confidence
/// literal rides the base-quad CBOR fold, and the superset/fold gate compares
/// projected bytes — `f64`'s default `Display`/`{}` formatting is NOT pinned
/// across Rust versions/inputs (e.g. it can vary trailing-zero behaviour), so
/// this helper is the single, deterministic formatter every emission site
/// must go through.
fn confidence_decimal(c: f64) -> String {
    format!("{c:.1}")
}

// ── Core types ───────────────────────────────────────────────────────────────

/// A best-practice advisory ready to emit as a graded [`Diag`] and an
/// in-memory [`AdvisoryClaim`] hook.
///
/// # Contract
///
/// * `severity` MUST be [`Severity::Note`] or [`Severity::Info`].  Errors and
///   warnings are violations, not advice; they live in the hard-error and
///   warning tiers.
/// * `standpoint_iri` defaults to [`BEST_PRACTICE_STANDPOINT_IRI`] when
///   constructed via [`Advisory::note`].  Override it for domain-specific
///   advisory vantages.
/// * Call [`Advisory::project`] exactly once per emission event; do not cache
///   the result and reuse it across different contexts (the claim hook is
///   per-event).
#[derive(Debug, Clone, PartialEq)]
pub struct Advisory {
    /// Stable dot-separated diagnostic code, e.g. `"advice.sortal.specific"`.
    pub code: String,
    /// Human-readable advisory message — also becomes the `advised_proposition`
    /// in the [`AdvisoryClaim`].
    pub message: String,
    /// Severity of the flat finding.  Must be [`Severity::Note`] or
    /// [`Severity::Info`]; advisory semantics do not apply to harder tiers.
    pub severity: Severity,
    /// Concrete corrective suggestions surfaced in SARIF / CLI output.
    pub suggestions: Vec<String>,
    /// Optional URI to the rule's documentation page; carried into the
    /// [`Rule`] returned by [`Advisory::rule`].
    pub help_uri: Option<String>,
    /// The `gmeow:Standpoint` IRI from which the advice is issued (P9).
    /// Defaults to [`BEST_PRACTICE_STANDPOINT_IRI`].
    pub standpoint_iri: String,
    /// Source locations (file / GTS wire coordinates) relevant to this advisory.
    pub locations: Vec<Location>,
    /// Classifier tags forwarded to the flat finding's `tags` field.
    pub tags: Vec<String>,
    /// The advisor's stated certainty in the recommendation, in `[0.0, 1.0]`.
    /// Defaults to [`ADVISORY_DEFAULT_CONFIDENCE`]; NOT a tuned/calibrated
    /// metric — a declared value the advisor asserts.
    pub confidence: f64,
    /// The `gmeow:complianceVerdict` individual IRI.  Defaults to
    /// [`VERDICT_NOT_HELD_IRI`]; override via [`Advisory::with_verdict_iri`].
    pub verdict_iri: String,
    /// The advised subject's term IRI (`gmeow:observedFeature`), when the
    /// advisory concerns a specific in-graph term.  `None` for the
    /// demonstrator advisory (which has no single subject); set explicitly
    /// via [`Advisory::with_subject_iri`] — NEVER derived from `locations`,
    /// which are source POSITIONS (path/line/GTS coordinates), not IRIs;
    /// there is no defined position→IRI projection.
    pub subject_iri: Option<String>,
}

/// The dual projection of one advisory event (P4/P17): a graded [`Diag`] and
/// the in-memory claim hook D4 fills.  Produced together by
/// [`Advisory::project`].
///
/// The diagnostic wing is a first-class graded [`Diag`] at
/// [`Standpoint::Advisory`] — the perspectival vantage is carried on the grade,
/// not stringly as an IRI — so it interns onto the run [`gmeow_errors::DiagLedger`]
/// like every other producer.  An Advisory-standpoint witness never gates,
/// whatever its severity (that theorem lives in the gate morphism), so the
/// advisory demonstrator stays a non-failing Note.
///
/// # Invariant
///
/// `diag.code() == claim.code` — the diagnostic and the claim hook always refer
/// to the same advisory rule.
///
/// [`Diag`] is a move-only carrier (no `Clone`), so this projection is consumed
/// once — destructured into its two wings — never cached and reused.
#[derive(Debug)]
pub struct AdvisoryProjection {
    /// The graded advisory diagnostic, ready to intern onto the run ledger.
    pub diag: Diag,
    /// The in-memory claim hook for D4 to materialise as RDF.
    pub claim: AdvisoryClaim,
}

/// The vantage-indexed recommendation-claim HOOK (keystone; filled later).
///
/// Carries what a `gmeow:ComplianceAssessment` / `StandpointClaim` needs — the
/// issuing standpoint (vantage), the advised proposition (message text), and the
/// `deonticRecommendation` modality IRI — WITHOUT emitting RDF in D1.
///
/// # Lifecycle
///
/// D1 constructs this struct; D4 consumes it, resolves
/// `standpoint_iri` to an in-graph `gmeow:Standpoint` individual, and emits the
/// corresponding RDF triples into the validation output graph.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryClaim {
    /// The `gmeow:Standpoint` IRI from which the advice is issued.
    /// D4 resolves this to an in-graph individual.
    pub standpoint_iri: String,
    /// The natural-language text of the advised proposition (= [`Advisory::message`]).
    pub advised_proposition: String,
    /// The deontic modality IRI — always [`DEONTIC_RECOMMENDATION_IRI`] for
    /// advisory-tier claims.
    pub modality_iri: String,
    /// The diagnostic code linking this claim back to its flat finding's rule.
    pub code: String,
    /// The advisor's stated certainty in the recommendation, in `[0.0, 1.0]`.
    pub confidence: f64,
    /// The `gmeow:complianceVerdict` individual IRI.
    pub verdict_iri: String,
    /// The advised subject's term IRI (`gmeow:observedFeature`), when set.
    pub subject_iri: Option<String>,
}

// ── Advisory impl ────────────────────────────────────────────────────────────

impl Advisory {
    /// Convenience constructor: a [`Severity::Note`]-severity advisory from the
    /// [`BEST_PRACTICE_STANDPOINT_IRI`] vantage.
    ///
    /// All optional fields are empty / `None`; use the builder methods to fill
    /// them in before calling [`project`](Advisory::project).
    pub fn note(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: Severity::Note,
            suggestions: Vec::new(),
            help_uri: None,
            standpoint_iri: BEST_PRACTICE_STANDPOINT_IRI.to_owned(),
            locations: Vec::new(),
            tags: Vec::new(),
            confidence: ADVISORY_DEFAULT_CONFIDENCE,
            verdict_iri: VERDICT_NOT_HELD_IRI.to_owned(),
            subject_iri: None,
        }
    }

    /// Append a corrective suggestion (builder-style; chainable).
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestions.push(suggestion.into());
        self
    }

    /// Set the rule documentation URI (builder-style; chainable).
    #[must_use]
    pub fn with_help_uri(mut self, uri: impl Into<String>) -> Self {
        self.help_uri = Some(uri.into());
        self
    }

    /// Append a source location (builder-style; chainable).
    #[must_use]
    pub fn with_location(mut self, location: Location) -> Self {
        self.locations.push(location);
        self
    }

    /// Append a classifier tag (builder-style; chainable).
    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Override the advisor's stated confidence (builder-style; chainable).
    /// Must land in `[0.0, 1.0]`; the RDF emitter
    /// [`project_compliance_assessment`] hard-fails (panics) on an out-of-range
    /// value rather than silently clamp it. [`Advisory::project`] itself only
    /// carries the value onto the [`AdvisoryClaim`]; the range is enforced when
    /// the claim is projected to N-Quads.
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence;
        self
    }

    /// Override the `gmeow:complianceVerdict` IRI (builder-style; chainable).
    #[must_use]
    pub fn with_verdict_iri(mut self, verdict_iri: impl Into<String>) -> Self {
        self.verdict_iri = verdict_iri.into();
        self
    }

    /// Set the advised subject's term IRI (builder-style; chainable).
    #[must_use]
    pub fn with_subject_iri(mut self, subject_iri: impl Into<String>) -> Self {
        self.subject_iri = Some(subject_iri.into());
        self
    }

    /// Produce BOTH projections from one call.
    ///
    /// The dual-projection-always contract: no opt-in flag; every call to
    /// `project` yields exactly one graded [`Diag`] AND one [`AdvisoryClaim`].
    ///
    /// # Diagnostic construction
    ///
    /// * `severity` / `code` / `message` forwarded verbatim.
    /// * Grade = (`self.severity`, [`FindingCategory::PolicyWarning`],
    ///   [`Standpoint::Advisory`]) — advice is a non-gating, perspectival
    ///   recommendation, so the vantage is first-class on the grade (never a
    ///   Binding gate contribution).
    /// * Each `suggestion` becomes an [`Advice`] at the Advisory standpoint,
    ///   projected back onto `finding.suggestions` by the ledger.
    /// * The first `location` (if any) becomes the diagnostic's source context.
    /// * `tags` carried onto the diagnostic.
    ///
    /// # Claim construction
    ///
    /// * `standpoint_iri` carried from `self`.
    /// * `advised_proposition` = `self.message`.
    /// * `modality_iri` = [`DEONTIC_RECOMMENDATION_IRI`].
    /// * `code` = `self.code`.
    /// * `confidence` / `verdict_iri` / `subject_iri` forwarded verbatim from
    ///   `self` (see [`Advisory::with_confidence`], [`Advisory::with_verdict_iri`],
    ///   [`Advisory::with_subject_iri`]).
    pub fn project(&self) -> AdvisoryProjection {
        let mut diag = Diag::new(
            register_code(&self.code),
            Grade::new(
                self.severity,
                FindingCategory::PolicyWarning,
                Standpoint::Advisory,
            ),
            self.message.clone(),
        );

        for suggestion in &self.suggestions {
            diag = diag.with_advice(Advice {
                standpoint: Standpoint::Advisory,
                text: suggestion.clone(),
                help_uri: self.help_uri.clone(),
            });
        }
        // A `Diag` carries a single source context; the advisory demonstrator has
        // none, and callers attach at most one relevant location. The first
        // location is the source anchor.
        if let Some(location) = self.locations.first() {
            diag = diag.with_location(location.clone());
        }
        for tag in &self.tags {
            diag = diag.with_tag(tag.clone());
        }

        let claim = AdvisoryClaim {
            standpoint_iri: self.standpoint_iri.clone(),
            advised_proposition: self.message.clone(),
            modality_iri: DEONTIC_RECOMMENDATION_IRI.to_owned(),
            code: self.code.clone(),
            confidence: self.confidence,
            verdict_iri: self.verdict_iri.clone(),
            subject_iri: self.subject_iri.clone(),
        };

        AdvisoryProjection { diag, claim }
    }

    /// The soft [`Rule`] to register on the [`gmeow_errors::Report`] so
    /// SARIF/text/HTML renderers can surface the `help_uri`.
    ///
    /// `default_severity` mirrors `self.severity`; `help_uri` is forwarded from
    /// `self.help_uri`.  `title` and `description` are left `None` — callers may
    /// set them on the returned value if needed.
    pub fn rule(&self) -> Rule {
        let mut rule = Rule::new(self.code.clone(), self.severity);
        rule.help_uri = self.help_uri.clone();
        rule
    }
}

// ── Advisory bridge: data-matched Info constraints → Note advisories ────────

/// The `logic:formalizes` back-reference every derived constraint shape carries — the
/// gmeow-domain term the advisory constraint concerns (the advice's provenance).
const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// The help page every harvested advisory rule links to.
const ADVICE_HELP_URI: &str = "https://blackcatinformatics.ca/gmeow/advice";
/// The formalized term's positive how-to prose — surfaced on the advisory as a corrective
/// `suggestion` (D3 acceptance criterion: `gmeow:howToUse` populates `suggestions`).
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
/// The formalized term's applicability prose — surfaced on the advisory as contextual
/// guidance (D3 acceptance criterion: `gmeow:useWhen` surfaces as guidance).
const GMEOW_USE_WHEN: &str = "https://blackcatinformatics.ca/gmeow/useWhen";
/// The canonical source language every authored guidance literal carries; the public
/// `@en`/`@zh`/`@fr` projections are never the surfaced text.
const ADVICE_SOURCE_LANG: &str = "x-gmeow-english";

/// The IRI string of a SHACL term, or `None` for a blank node / literal.
fn shacl_iri(term: &ShaclTerm) -> Option<String> {
    match term {
        ShaclTerm::NamedNode(n) => Some(n.as_str().to_owned()),
        _ => None,
    }
}

/// The local name of an IRI (after the last `/` or `#`), sanitised to the IRI-safe
/// code alphabet so it can key a `deonticRecommendation` claim IRI.
fn code_local(iri: &str) -> String {
    let raw = iri.rsplit(['/', '#']).next().unwrap_or(iri);
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// A stable 12-hex-char digest of a focus-node IRI — makes each `(constraint, focus)`
/// match's advisory code unique (so [`project_compliance_assessment`] never sees a
/// duplicate code) and deterministic across runs (SHA-256, not a process hasher).
fn focus_digest(focus: &str) -> String {
    let digest = Sha256::digest(focus.as_bytes());
    let mut hex = String::with_capacity(12);
    for b in digest.iter().take(6) {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// The gmeow-domain term the derived `shape_iri` `logic:formalizes` (its advice
/// provenance), read from the `shapes` graph. `None` when the shape carries none.
fn shape_formalizes(shapes: &purrdf::RdfDataset, shape_iri: &str) -> Option<String> {
    use purrdf::{DatasetView, GraphMatch, TermRef};
    let (Some(subj), Some(pred)) = (
        graph_id(shapes, shape_iri),
        graph_id(shapes, LOGIC_FORMALIZES),
    ) else {
        return None;
    };
    shapes
        .quads_for_pattern(Some(subj), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match shapes.resolve(q.o) {
            TermRef::Iri(iri) => Some(iri.to_owned()),
            _ => None,
        })
}

/// Resolve an IRI to its interned dataset id (small local helper).
fn graph_id(ds: &purrdf::RdfDataset, iri: &str) -> Option<purrdf::TermId> {
    ds.term_id_by_value(&purrdf::TermValue::iri(iri))
}

/// The `term`'s canonical source-language (`@x-gmeow-english`) prose for the annotation
/// `prop` (e.g. `gmeow:howToUse` / `gmeow:useWhen`), read from `ontology`. `None` when the
/// term carries no such source-language literal — an `@en`/`@zh`/`@fr` projection is never
/// returned. The single reader both suggestion (`howToUse`) and guidance (`useWhen`)
/// surfacing route through, so an advisory carries the term's OWN prose, not a paraphrase.
fn term_source_prose(ontology: &purrdf::RdfDataset, term: &str, prop: &str) -> Option<String> {
    use purrdf::{DatasetView, GraphMatch, TermRef};
    let (Some(subj), Some(pred)) = (graph_id(ontology, term), graph_id(ontology, prop)) else {
        return None;
    };
    ontology
        .quads_for_pattern(Some(subj), Some(pred), None, GraphMatch::Any)
        .find_map(|q| match ontology.resolve(q.o) {
            TermRef::Literal {
                lexical,
                language: Some(lang),
                ..
            } if lang == ADVICE_SOURCE_LANG => Some(lexical.to_owned()),
            _ => None,
        })
}

/// Build one Note [`Advisory`] from a matched advisory constraint — the single source
/// both the pipeline (result-based) and CLI (finding-based) splits construct through.
///
/// The focus node (the individual whose data matched the anti-pattern guard) is the
/// advised subject (`gmeow:observedFeature`); `message` is the advice (the constraint's
/// formalized-term `gmeow:avoidWhen` prose); the code `advice.<shape-local>.<focus-digest>`
/// is injective per match and identifies the governing constraint (its provenance is the
/// shape's `logic:formalizes`, carried as a `formalizes:<term>` tag).
///
/// The whole prose surface of the formalized term is made machine-active here (D3): the
/// term's `gmeow:howToUse` becomes a corrective `suggestion` and its `gmeow:useWhen` becomes
/// a contextual-guidance `suggestion`, both read (source-language) from `ontology`. So a
/// harvested advisory carries `avoidWhen` (as the message) + `howToUse` + `useWhen` — the
/// term's own prose, never a paraphrase.
fn build_advisory(
    shape_iri: Option<&str>,
    focus_iri: Option<&str>,
    message: &str,
    shapes: &purrdf::RdfDataset,
    ontology: &purrdf::RdfDataset,
) -> Advisory {
    let shape_local = shape_iri.map_or_else(|| "constraint".to_owned(), code_local);
    let digest = focus_digest(focus_iri.unwrap_or_default());
    let code = format!("{}{}.{}", crate::codes::ADVICE_FAMILY, shape_local, digest);
    let mut advisory = Advisory::note(code, message.to_owned())
        .with_tag("advisory-harvested")
        .with_help_uri(ADVICE_HELP_URI);
    if let Some(focus) = focus_iri {
        advisory = advisory.with_subject_iri(focus.to_owned());
    }
    if let Some(term) = shape_iri.and_then(|s| shape_formalizes(shapes, s)) {
        advisory = advisory.with_tag(format!("formalizes:{term}"));
        // The formalized term's positive prose rides the advisory: howToUse as the corrective
        // suggestion, useWhen as contextual guidance (each surfaced only when the term authors
        // it — honest absence otherwise).
        if let Some(how_to_use) = term_source_prose(ontology, &term, GMEOW_HOW_TO_USE) {
            advisory = advisory.with_suggestion(how_to_use);
        }
        if let Some(use_when) = term_source_prose(ontology, &term, GMEOW_USE_WHEN) {
            advisory = advisory.with_suggestion(format!("Use when: {use_when}"));
        }
    }
    advisory
}

/// One matched advisory-constraint [`ValidationResult`] → a Note [`Advisory`].
fn advisory_from_result(
    r: &ValidationResult,
    shapes: &purrdf::RdfDataset,
    ontology: &purrdf::RdfDataset,
) -> Advisory {
    let message = r
        .message
        .clone()
        .unwrap_or_else(|| "advisory constraint matched".to_owned());
    build_advisory(
        shacl_iri(&r.source_shape).as_deref(),
        shacl_iri(&r.focus_node).as_deref(),
        &message,
        shapes,
        ontology,
    )
}

/// Split ADVISORY findings out of an already-projected [`gmeow_errors::Report`] — the
/// CLI twin of [`split_advisory_results`] (which works on raw results before projection).
///
/// The CLI interns all SHACL findings (including the Info-severity advisory ones as
/// `shacl.*`) through cached phases, so the split happens on the projected report: each
/// `Severity::Info`, `shacl.*` finding whose recorded source shape carries a
/// `logic:formalizes` is an advisory-constraint match — it is REMOVED from `report` (its
/// raw `shacl.*` form suppressed) and returned as a Note [`Advisory`] for the dual
/// projection. Fires from a DATA MATCH; a report with no such finding yields none.
///
/// `ontology` carries the formalized terms' `gmeow:howToUse` / `gmeow:useWhen` prose that each
/// advisory surfaces as suggestions/guidance (the CLI passes the validated bundle dataset).
#[must_use]
pub fn split_advisory_findings(
    report: &mut gmeow_errors::Report,
    shapes: &purrdf::RdfDataset,
    ontology: &purrdf::RdfDataset,
) -> Vec<Advisory> {
    let mut advisories = Vec::new();
    let mut retained = Vec::with_capacity(report.findings.len());
    for finding in std::mem::take(&mut report.findings) {
        let is_shacl = finding.code.starts_with(crate::codes::SHACL_FAMILY);
        let shape_iri = finding.detail.as_deref().and_then(|d| {
            d.strip_prefix("source shape: ")
                .map(|s| s.trim().to_owned())
        });
        let is_advisory = finding.severity == Severity::Info
            && is_shacl
            && shape_iri
                .as_deref()
                .is_some_and(|s| shape_formalizes(shapes, s).is_some());
        if is_advisory {
            let focus = finding
                .primary_location()
                .and_then(|l| l.logical.clone());
            advisories.push(build_advisory(
                shape_iri.as_deref(),
                focus.as_deref(),
                &finding.message,
                shapes,
                ontology,
            ));
        } else {
            retained.push(finding);
        }
    }
    report.findings = retained;
    advisories.sort_by(|a, b| a.code.cmp(&b.code));
    // One advice per (constraint, focus): collapse a shape matched more than once on the same
    // focus (identical `advice.<shape-local>.<focus-digest>` code) so the claim emitter never sees
    // a duplicate — mirrors `split_advisory_results`.
    advisories.dedup_by(|a, b| a.code == b.code);
    advisories
}

/// Split a SHACL [`ValidationReport`] into (retained hard/warning results, advisory
/// Note projections). An `Info`-severity result is ADVISORY — it comes from a
/// `logic:severity "Info"` advisory constraint (the only Info constraints authored),
/// so its raw `shacl.*` finding is SUPPRESSED (removed from `retained`) and re-projected
/// through the advisory dual-projection as a `Severity::Note` finding + a
/// `deonticRecommendation` `gmeow:ComplianceAssessment`. The advisory fires from a DATA
/// MATCH — the guard matched an individual — never merely because a rule exists.
///
/// Deterministic: advisories are sorted by code, and the retained results preserve the
/// engine's order. The `shapes` graph is read for each advisory shape's `logic:formalizes`
/// provenance term; `ontology` carries the formalized terms' `gmeow:howToUse` /
/// `gmeow:useWhen` prose each advisory surfaces (the pipeline passes the source graph).
#[must_use]
pub fn split_advisory_results(
    report: ValidationReport,
    shapes: &purrdf::RdfDataset,
    ontology: &purrdf::RdfDataset,
) -> (ValidationReport, Vec<Advisory>) {
    let mut retained = Vec::new();
    let mut advisories = Vec::new();
    for result in report.results {
        if matches!(result.severity, ShaclSeverity::Info) {
            advisories.push(advisory_from_result(&result, shapes, ontology));
        } else {
            retained.push(result);
        }
    }
    advisories.sort_by(|a, b| a.code.cmp(&b.code));
    // One advice per (constraint, focus): the SAME advisory constraint matching the SAME focus
    // node is ONE piece of advice, but the engine can surface it more than once (a shape present
    // twice in the shape union, or duplicate SPARQL solution rows), and the code
    // `advice.<shape-local>.<focus-digest>` is identical for those. Collapse them so the claim
    // emitter never sees a duplicate code — otherwise `project_compliance_assessment` hard-fails.
    advisories.dedup_by(|a, b| a.code == b.code);
    debug_assert!(
        advisories.windows(2).all(|w| w[0].code != w[1].code),
        "advisory codes must be unique per (constraint, focus) match after dedup"
    );
    // Recompute conformance for the RETAINED set: advisory Info matches were the reason
    // the run was non-conforming iff they were the only results, but they never gate — so
    // once they are lifted out, the retained report conforms unless a real Violation
    // remains. (Without this, suppressing the only result leaves conforms=false with an
    // empty result set, which the diagnostics fallback wrongly reports as a hard error.)
    let conforms = !retained
        .iter()
        .any(|r| matches!(r.severity, ShaclSeverity::Violation));
    (
        ValidationReport {
            conforms,
            results: retained,
        },
        advisories,
    )
}

// ── ComplianceAssessment RDF emitter (D4) ───────────────────────────────────

/// The GMEOW namespace IRI prefix (mirrors `crates/errors/src/render.rs`'s
/// `GMEOW` constant — kept crate-local since it is not exported).
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// Project (D4) `ComplianceAssessment` claims into `gmeow:` RDF as N-Quads, all
/// in the named graph `graph_iri`.
///
/// For each [`AdvisoryClaim`] this mints THREE individuals, IRI-keyed off the
/// claim's diagnostic `code` under [`NORM_CLAIMS_BASE_IRI`]:
///
/// * `{code}/norm` — a `gmeow:Norm` carrying the advised proposition as its
///   label, the `deonticRecommendation` modality, and the issuing standpoint
///   as `gmeow:normIssuer`, `gmeow:partOf` [`BEST_PRACTICE_NORMATIVE_SYSTEM_IRI`].
/// * `{code}/event` — a `gmeow:Event` carrying exactly one
///   `gmeow:eventTemporalFrame` ([`EVENT_TEMPORAL_FRAME_IRI`]), a
///   `gmeow:eventType` ([`EVENT_TYPE_IRI`]), and NO `gmeow:eventTime`
///   (deterministic output — no wall-clock).
/// * `{code}/assessment` — a `gmeow:ComplianceAssessment` linking the event
///   and norm, carrying the verdict, the vantage (= standpoint), the stated
///   confidence as a pinned `xsd:decimal` literal, and — when
///   `claim.subject_iri` is set — the advised subject as
///   `gmeow:observedFeature`.
///
/// All three individuals carry the shared A-Box annotation pattern (a
/// `rdfs:label`, `rdfs:isDefinedBy graph_iri`, `gmeow:graphBoxRole
/// gmeow:boxABox` — no `skos:definition` on instances), mirroring the
/// `gmeow:Finding` emitter in `crates/errors/src/render.rs::to_gmeow_rdf_in_graph`.
///
/// Output is deterministic: claims are sorted by `code` before emission, and
/// each claim emits its triples in a fixed order, so two calls on the same
/// input produce byte-identical strings.
///
/// # Panics
///
/// Every hard-fail below guards a producer bug — advisory-claim fields are
/// minted by this crate, never accepted from external input, so a violation is
/// a defect to surface, not data to tolerate (no silent degradation):
///
/// * a claim's `confidence` falls outside `[0.0, 1.0]` — a STATED value is
///   never silently clamped (mirrors the `class_coverage` range assert in
///   `crates/validate/src/coverage.rs`);
/// * a claim's `code` is not IRI-safe (outside `[A-Za-z0-9._-]+`) — it is
///   interpolated verbatim into the content-addressed IRIs and would otherwise
///   mint a malformed IRI / invalid N-Quad;
/// * two claims share a `code` — the code keys all three per-claim IRIs, so a
///   collision would emit conflicting triples on functional properties.
pub fn project_compliance_assessment(claims: &[AdvisoryClaim], graph_iri: &str) -> String {
    let graph = format!("<{graph_iri}>");
    let mut lines: Vec<String> = Vec::new();

    let triple = |s: &str, p: &str, o: &str, lines: &mut Vec<String>| {
        lines.push(format!("{s} <{p}> {o} {graph} ."));
    };

    let mut sorted_claims: Vec<&AdvisoryClaim> = claims.iter().collect();
    sorted_claims.sort_by(|a, b| a.code.cmp(&b.code));

    // Codes key all three per-claim IRIs; two claims sharing a code would collide onto
    // the same subjects and emit conflicting triples on functional properties. Codes are
    // sorted, so a duplicate is adjacent — a producer bug, hard-fail deterministically.
    if let Some(dup) = sorted_claims
        .windows(2)
        .find_map(|w| (w[0].code == w[1].code).then_some(&w[0].code))
    {
        panic!(
            "advisory claims contain a duplicate code {dup:?} — each code must be unique \
             (it keys the ComplianceAssessment / norm / event IRIs)"
        );
    }

    for claim in sorted_claims {
        assert!(
            (0.0..=1.0).contains(&claim.confidence),
            "advisory claim {:?} confidence out of range [0.0, 1.0]: {}",
            claim.code,
            claim.confidence
        );
        assert!(
            !claim.code.is_empty()
                && claim
                    .code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')),
            "advisory claim code {:?} is not IRI-safe (allowed: ASCII alphanumerics, '.', '_', '-') \
             — the code is interpolated verbatim into content-addressed IRIs and must not mint a \
             malformed IRI / invalid N-Quad",
            claim.code
        );

        let norm_iri = format!("{NORM_CLAIMS_BASE_IRI}{}/norm", claim.code);
        let event_iri = format!("{NORM_CLAIMS_BASE_IRI}{}/event", claim.code);
        let assessment_iri = format!("{NORM_CLAIMS_BASE_IRI}{}/assessment", claim.code);
        let norm = format!("<{norm_iri}>");
        let event = format!("<{event_iri}>");
        let assessment = format!("<{assessment_iri}>");

        // ── NORM (gmeow:Norm) ────────────────────────────────────────────
        // A plain gmeow:Norm, NOT gmeow:Rule: gmeow:Rule is the rights-graft Relator
        // that must name a gmeow:RightsAction via gmeow:ruleAction; a best-practice
        // recommendation is a bare social-convention norm (gmeow:deonticModality's own
        // domain is gmeow:Norm), so a gmeow:Norm is the faithful type.
        triple(&norm, RDF_TYPE, &format!("<{GMEOW}Norm>"), &mut lines);
        triple(
            &norm,
            RDFS_LABEL,
            &format!(
                "\"{}\"",
                nq_escape(&format!("recommendation: {}", claim.advised_proposition))
            ),
            &mut lines,
        );
        triple(&norm, RDFS_IS_DEFINED_BY, &graph, &mut lines);
        triple(
            &norm,
            &format!("{GMEOW}graphBoxRole"),
            &format!("<{GMEOW}boxABox>"),
            &mut lines,
        );
        triple(
            &norm,
            &format!("{GMEOW}partOf"),
            &format!("<{BEST_PRACTICE_NORMATIVE_SYSTEM_IRI}>"),
            &mut lines,
        );
        triple(
            &norm,
            &format!("{GMEOW}deonticModality"),
            &format!("<{}>", claim.modality_iri),
            &mut lines,
        );
        triple(
            &norm,
            &format!("{GMEOW}normIssuer"),
            &format!("<{}>", claim.standpoint_iri),
            &mut lines,
        );

        // ── EVENT (gmeow:Event) ──────────────────────────────────────────
        triple(&event, RDF_TYPE, &format!("<{GMEOW}Event>"), &mut lines);
        triple(
            &event,
            RDFS_LABEL,
            &format!(
                "\"{}\"",
                nq_escape(&format!("advice conduct for {}", claim.code))
            ),
            &mut lines,
        );
        triple(&event, RDFS_IS_DEFINED_BY, &graph, &mut lines);
        triple(
            &event,
            &format!("{GMEOW}graphBoxRole"),
            &format!("<{GMEOW}boxABox>"),
            &mut lines,
        );
        triple(
            &event,
            &format!("{GMEOW}eventTemporalFrame"),
            &format!("<{EVENT_TEMPORAL_FRAME_IRI}>"),
            &mut lines,
        );
        triple(
            &event,
            &format!("{GMEOW}eventType"),
            &format!("<{EVENT_TYPE_IRI}>"),
            &mut lines,
        );

        // ── ASSESSMENT (gmeow:ComplianceAssessment) ──────────────────────
        triple(
            &assessment,
            RDF_TYPE,
            &format!("<{GMEOW}ComplianceAssessment>"),
            &mut lines,
        );
        triple(
            &assessment,
            RDFS_LABEL,
            &format!(
                "\"{}\"",
                nq_escape(&format!("{}: {}", claim.code, claim.advised_proposition))
            ),
            &mut lines,
        );
        triple(&assessment, RDFS_IS_DEFINED_BY, &graph, &mut lines);
        triple(
            &assessment,
            &format!("{GMEOW}graphBoxRole"),
            &format!("<{GMEOW}boxABox>"),
            &mut lines,
        );
        triple(
            &assessment,
            &format!("{GMEOW}assessedEvent"),
            &event,
            &mut lines,
        );
        triple(
            &assessment,
            &format!("{GMEOW}assessedNorm"),
            &norm,
            &mut lines,
        );
        triple(
            &assessment,
            &format!("{GMEOW}complianceVerdict"),
            &format!("<{}>", claim.verdict_iri),
            &mut lines,
        );
        triple(
            &assessment,
            &format!("{GMEOW}vantage"),
            &format!("<{}>", claim.standpoint_iri),
            &mut lines,
        );
        triple(
            &assessment,
            &format!("{GMEOW}confidence"),
            &format!(
                "\"{}\"^^<{XSD_DECIMAL}>",
                confidence_decimal(claim.confidence)
            ),
            &mut lines,
        );
        if let Some(subject_iri) = &claim.subject_iri {
            triple(
                &assessment,
                &format!("{GMEOW}observedFeature"),
                &format!("<{subject_iri}>"),
                &mut lines,
            );
        }
    }

    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::{DiagLedger, StageId};

    /// Project the advisory diagnostic wing through a ledger to the wire
    /// [`gmeow_errors::Finding`] the renderers consume — the real intern path.
    fn project_diag_finding(diag: Diag) -> gmeow_errors::Finding {
        let mut ledger = DiagLedger::new();
        ledger.attach(diag, StageId::new("validate.advisory"));
        ledger
            .findings("validate")
            .pop()
            .expect("exactly one advisory finding")
    }

    /// Both projection wings are produced in one call and carry the expected
    /// field values — the core dual-projection-always contract.
    #[test]
    fn project_yields_both_graded_diag_and_claim() {
        let advisory = Advisory::note("advice.sample", "consider a more specific sortal")
            .with_suggestion("use gmeow:Kind")
            .with_help_uri("https://example.org/docs/advice#sample");

        let AdvisoryProjection { diag, claim } = advisory.project();

        // ── graded diagnostic assertions ─────────────────────────────────────
        assert_eq!(diag.grade().severity, Severity::Note);
        assert_eq!(diag.grade().category, FindingCategory::PolicyWarning);
        assert_eq!(diag.grade().standpoint, Standpoint::Advisory);
        assert_eq!(diag.message(), "consider a more specific sortal");

        let finding = project_diag_finding(diag);
        assert_eq!(finding.severity, Severity::Note);
        assert_eq!(finding.code, "advice.sample");
        assert_eq!(finding.standpoint, Some(Standpoint::Advisory));
        assert_eq!(finding.category, Some(FindingCategory::PolicyWarning));
        assert_eq!(finding.tool, Some("validate".to_owned()));
        assert_eq!(finding.suggestions, vec!["use gmeow:Kind".to_owned()]);
        assert_eq!(finding.message, "consider a more specific sortal");

        // ── claim hook assertions ────────────────────────────────────────────
        assert_eq!(claim.code, "advice.sample");
        assert_eq!(claim.standpoint_iri, BEST_PRACTICE_STANDPOINT_IRI);
        assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
        assert_eq!(claim.advised_proposition, "consider a more specific sortal");
    }

    /// The soft Rule carries the correct default_severity and help_uri so
    /// SARIF/text/HTML renderers surface the documentation link.
    #[test]
    fn rule_carries_help_and_note_default() {
        let advisory = Advisory::note("advice.sample", "consider a more specific sortal")
            .with_help_uri("https://example.org/docs/advice#sample");

        let rule = advisory.rule();

        assert_eq!(rule.default_severity, Severity::Note);
        assert_eq!(
            rule.help_uri,
            Some("https://example.org/docs/advice#sample".to_owned())
        );
    }

    /// `project()` always returns exactly ONE diagnostic and ONE claim — the 1:1
    /// structural invariant of the dual-projection-always contract.
    #[test]
    fn one_advisory_one_claim() {
        let advisory = Advisory::note("advice.sanity", "sanity check advisory");
        let projection = advisory.project();

        // Destructure to confirm both wings exist (would not compile otherwise).
        let AdvisoryProjection { diag, claim } = projection;

        // The codes must agree — the diagnostic and claim refer to the same rule.
        let finding = project_diag_finding(diag);
        assert_eq!(finding.code, claim.code);
    }

    /// `Advisory::note` seeds the new (D4) fields to their documented defaults.
    #[test]
    fn note_seeds_default_confidence_verdict_and_no_subject() {
        let advisory = Advisory::note("advice.defaults", "defaults check");
        assert_eq!(advisory.confidence, ADVISORY_DEFAULT_CONFIDENCE);
        assert_eq!(advisory.verdict_iri, VERDICT_NOT_HELD_IRI);
        assert_eq!(advisory.subject_iri, None);

        let claim = advisory.project().claim;
        assert_eq!(claim.confidence, ADVISORY_DEFAULT_CONFIDENCE);
        assert_eq!(claim.verdict_iri, VERDICT_NOT_HELD_IRI);
        assert_eq!(claim.subject_iri, None);
    }

    // ── Advisory bridge: data-matched Info constraints → Note advisories ─────

    fn result(
        severity: ShaclSeverity,
        shape: &str,
        focus: &str,
        message: &str,
    ) -> ValidationResult {
        use purrdf::shapes::term::{NamedNode, Term};
        ValidationResult {
            focus_node: Term::NamedNode(NamedNode::new_unchecked(focus)),
            result_path: None,
            path_structure: None,
            value: None,
            source_constraint_component: NamedNode::new_unchecked(
                "http://www.w3.org/ns/shacl#SPARQLConstraintComponent",
            ),
            source_shape: Term::NamedNode(NamedNode::new_unchecked(shape)),
            severity,
            message: Some(message.to_owned()),
            source_box_roles: Vec::new(),
            path_box_roles: Vec::new(),
            result_box_roles: Vec::new(),
            attributions: Vec::new(),
        }
    }

    /// An `Info`-severity result (from an advisory constraint) is lifted into a Note
    /// advisory carrying the focus node as subject and the shape's `logic:formalizes`
    /// provenance, its raw `shacl.*` finding SUPPRESSED; a `Violation` result is retained
    /// for the hard diagnostics report. The advisory fires from the DATA MATCH.
    #[test]
    fn split_advisory_lifts_info_results_and_retains_violations() {
        let shapes = purrdf::parse_dataset(
            b"@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
              @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              <https://ex/advShape> logic:formalizes gmeow:Entity .\n",
            "text/turtle",
            None,
        )
        .expect("shapes parse");
        // The ontology carries the formalized term's positive prose: howToUse → the advisory's
        // corrective suggestion, useWhen → contextual guidance (D3 acceptance criteria).
        let ontology = purrdf::parse_dataset(
            b"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
              gmeow:Entity gmeow:howToUse \"Type each instance with its most specific sortal.\"@x-gmeow-english ;\n\
                gmeow:useWhen \"Use for a genuinely category-neutral resource.\"@x-gmeow-english .\n",
            "text/turtle",
            None,
        )
        .expect("ontology parse");
        let report = ValidationReport {
            conforms: false,
            results: vec![
                result(
                    ShaclSeverity::Info,
                    "https://ex/advShape",
                    "https://data/thing",
                    "prefer a more specific sortal than bare gmeow:Entity",
                ),
                result(
                    ShaclSeverity::Violation,
                    "https://ex/hardShape",
                    "https://data/bad",
                    "a required value is missing",
                ),
            ],
        };

        let (retained, advisories) = split_advisory_results(report, &shapes, &ontology);

        // The hard violation is retained; the Info result was suppressed.
        assert_eq!(retained.results.len(), 1);
        assert_eq!(retained.results[0].severity, ShaclSeverity::Violation);

        // Exactly one Note advisory, subject = the matched focus node, provenance tag.
        assert_eq!(advisories.len(), 1);
        let advisory = &advisories[0];
        assert_eq!(advisory.severity, Severity::Note);
        assert!(advisory.code.starts_with(crate::codes::ADVICE_FAMILY));
        assert_eq!(advisory.subject_iri.as_deref(), Some("https://data/thing"));
        assert_eq!(advisory.message, "prefer a more specific sortal than bare gmeow:Entity");
        assert!(advisory.tags.iter().any(|t| t == "advisory-harvested"));
        assert!(
            advisory
                .tags
                .iter()
                .any(|t| t == "formalizes:https://blackcatinformatics.ca/gmeow/Entity"),
            "the advisory carries its constraint's logic:formalizes provenance: {:?}",
            advisory.tags
        );
        // howToUse populates the suggestions verbatim; useWhen is surfaced as guidance.
        assert!(
            advisory
                .suggestions
                .iter()
                .any(|s| s == "Type each instance with its most specific sortal."),
            "gmeow:howToUse must populate the advisory's suggestions: {:?}",
            advisory.suggestions
        );
        assert!(
            advisory
                .suggestions
                .iter()
                .any(|s| s == "Use when: Use for a genuinely category-neutral resource."),
            "gmeow:useWhen must surface as contextual guidance: {:?}",
            advisory.suggestions
        );

        // Its claim wing carries the deonticRecommendation modality and the subject.
        let claim = advisory.project().claim;
        assert_eq!(claim.modality_iri, DEONTIC_RECOMMENDATION_IRI);
        assert_eq!(claim.subject_iri.as_deref(), Some("https://data/thing"));
    }

    /// Two matches of the SAME advisory constraint at DIFFERENT focus nodes get distinct
    /// injective codes (so the claim emitter never sees a duplicate) and both project.
    #[test]
    fn split_advisory_distinct_foci_get_distinct_codes() {
        let shapes = purrdf::parse_dataset(
            b"<https://ex/advShape> <https://blackcatinformatics.ca/logic/formalizes> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
            "application/n-triples",
            None,
        )
        .expect("shapes parse");
        let report = ValidationReport {
            conforms: false,
            results: vec![
                result(ShaclSeverity::Info, "https://ex/advShape", "https://data/a", "advice"),
                result(ShaclSeverity::Info, "https://ex/advShape", "https://data/b", "advice"),
            ],
        };
        let (_retained, advisories) = split_advisory_results(report, &shapes, &shapes);
        assert_eq!(advisories.len(), 2);
        assert_ne!(advisories[0].code, advisories[1].code, "distinct foci → distinct codes");
        // No duplicate-code panic when both distinct-focus claims project to N-Quads.
        let claims: Vec<AdvisoryClaim> = advisories.iter().map(|a| a.project().claim).collect();
        let _ = project_compliance_assessment(&claims, "https://ex/graph");
    }

    /// The SAME advisory constraint matching the SAME focus more than once (a shape present twice
    /// in the shape union, or duplicate SPARQL solution rows) is ONE advice: the duplicate
    /// `advice.<shape>.<focus-digest>` results collapse to a single advisory, so the claim emitter
    /// never sees a duplicate code (which would hard-fail `project_compliance_assessment`).
    #[test]
    fn split_advisory_dedups_duplicate_shape_focus_matches() {
        let shapes = purrdf::parse_dataset(
            b"<https://ex/advShape> <https://blackcatinformatics.ca/logic/formalizes> <https://blackcatinformatics.ca/gmeow/Entity> .\n",
            "application/n-triples",
            None,
        )
        .expect("shapes parse");
        let report = ValidationReport {
            conforms: false,
            results: vec![
                result(ShaclSeverity::Info, "https://ex/advShape", "https://data/dup", "advice"),
                result(ShaclSeverity::Info, "https://ex/advShape", "https://data/dup", "advice"),
            ],
        };
        let (_retained, advisories) = split_advisory_results(report, &shapes, &shapes);
        assert_eq!(
            advisories.len(),
            1,
            "duplicate (shape, focus) Info matches must collapse to one advisory: {advisories:?}"
        );
        // And the collapsed set projects to N-Quads with no duplicate-code panic.
        let claims: Vec<AdvisoryClaim> = advisories.iter().map(|a| a.project().claim).collect();
        let _ = project_compliance_assessment(&claims, "https://ex/graph");
    }

    // ── (D4) project_compliance_assessment ──────────────────────────────────

    const DEMO_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

    /// The demonstrator-style claim: code `advice.sample.demo`, all defaults.
    fn demo_claim() -> AdvisoryClaim {
        Advisory::note(
            "advice.sample.demo",
            "prefer the active-voice recommendation phrasing",
        )
        .project()
        .claim
    }

    /// The emitter's output parses cleanly as N-Quads.
    #[test]
    fn emitter_output_parses_as_nquads() {
        let nquads = project_compliance_assessment(&[demo_claim()], DEMO_GRAPH);
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("emitted ComplianceAssessment N-Quads must parse cleanly");
    }

    /// The demonstrator claim's full expected triple shape: exactly one
    /// verdict/vantage/confidence, present event/norm links, the norm's
    /// deontic/issuer/partOf triples, and the event's temporal frame with NO
    /// eventTime — the exact contract Task 2 specifies.
    #[test]
    fn demonstrator_claim_emits_the_full_expected_shape() {
        let claim = demo_claim();
        let nquads = project_compliance_assessment(std::slice::from_ref(&claim), DEMO_GRAPH);
        purrdf::parse_dataset(nquads.as_bytes(), "application/n-quads", None)
            .expect("must parse cleanly");

        let norm = format!("<{NORM_CLAIMS_BASE_IRI}{}/norm>", claim.code);
        let event = format!("<{NORM_CLAIMS_BASE_IRI}{}/event>", claim.code);
        let assessment = format!("<{NORM_CLAIMS_BASE_IRI}{}/assessment>", claim.code);

        // Exactly one complianceVerdict, pointing at verdictNotHeld.
        let verdict_line = format!(
            "{assessment} <{GMEOW}complianceVerdict> <{VERDICT_NOT_HELD_IRI}> <{DEMO_GRAPH}> ."
        );
        assert_eq!(
            nquads.matches(&verdict_line).count(),
            1,
            "expected exactly one complianceVerdict triple:\n{nquads}"
        );

        // Exactly one vantage, pointing at the real gmeowBestPractice standpoint.
        let vantage_line = format!(
            "{assessment} <{GMEOW}vantage> <{BEST_PRACTICE_STANDPOINT_IRI}> <{DEMO_GRAPH}> ."
        );
        assert_eq!(
            nquads.matches(&vantage_line).count(),
            1,
            "expected exactly one vantage triple:\n{nquads}"
        );

        // assessedEvent / assessedNorm present.
        assert!(nquads.contains(&format!(
            "{assessment} <{GMEOW}assessedEvent> {event} <{DEMO_GRAPH}> ."
        )));
        assert!(nquads.contains(&format!(
            "{assessment} <{GMEOW}assessedNorm> {norm} <{DEMO_GRAPH}> ."
        )));

        // Exactly one confidence literal, lexical form "1.0", datatype xsd:decimal.
        let confidence_line =
            format!("{assessment} <{GMEOW}confidence> \"1.0\"^^<{XSD_DECIMAL}> <{DEMO_GRAPH}> .");
        assert_eq!(
            nquads.matches(&confidence_line).count(),
            1,
            "expected exactly one confidence triple with lexical form \"1.0\":\n{nquads}"
        );

        // The norm is typed gmeow:Norm and carries deonticModality / normIssuer / partOf.
        assert!(nquads.contains(&format!(
            "{norm} <{RDF_TYPE}> <{GMEOW}Norm> <{DEMO_GRAPH}> ."
        )));
        assert!(nquads.contains(&format!(
            "{norm} <{GMEOW}deonticModality> <{DEONTIC_RECOMMENDATION_IRI}> <{DEMO_GRAPH}> ."
        )));
        assert!(nquads.contains(&format!(
            "{norm} <{GMEOW}normIssuer> <{BEST_PRACTICE_STANDPOINT_IRI}> <{DEMO_GRAPH}> ."
        )));
        assert!(nquads.contains(&format!(
            "{norm} <{GMEOW}partOf> <{BEST_PRACTICE_NORMATIVE_SYSTEM_IRI}> <{DEMO_GRAPH}> ."
        )));

        // The event carries eventTemporalFrame + an eventType (satisfying the "type or
        // temporal placement" modeling shape) and NO eventTime (deterministic output).
        assert!(nquads.contains(&format!(
            "{event} <{GMEOW}eventTemporalFrame> <{EVENT_TEMPORAL_FRAME_IRI}> <{DEMO_GRAPH}> ."
        )));
        assert!(nquads.contains(&format!(
            "{event} <{GMEOW}eventType> <{EVENT_TYPE_IRI}> <{DEMO_GRAPH}> ."
        )));
        assert!(
            !nquads.contains("eventTime"),
            "advisory event must carry NO eventTime (deterministic output):\n{nquads}"
        );
        assert!(nquads.contains(&format!(
            "{event} <{RDF_TYPE}> <{GMEOW}Event> <{DEMO_GRAPH}> ."
        )));

        // The assessment IRI embeds the code, and is typed gmeow:ComplianceAssessment.
        assert!(assessment.contains(&claim.code));
        assert!(nquads.contains(&format!(
            "{assessment} <{RDF_TYPE}> <{GMEOW}ComplianceAssessment> <{DEMO_GRAPH}> ."
        )));
    }

    /// `with_verdict_iri` changes ONLY the verdict triple — a pure-function
    /// proof: every other emitted line is identical between the default and
    /// overridden projections.
    #[test]
    fn with_verdict_iri_changes_only_the_verdict_triple() {
        let base_claim = demo_claim();
        let mut overridden_claim = base_claim.clone();
        overridden_claim.verdict_iri = "https://example.org/verdict/held".to_owned();

        let base_nquads = project_compliance_assessment(&[base_claim], DEMO_GRAPH);
        let overridden_nquads = project_compliance_assessment(&[overridden_claim], DEMO_GRAPH);

        let base_lines: Vec<&str> = base_nquads.lines().collect();
        let overridden_lines: Vec<&str> = overridden_nquads.lines().collect();
        assert_eq!(base_lines.len(), overridden_lines.len());

        let mut differing = 0usize;
        for (a, b) in base_lines.iter().zip(overridden_lines.iter()) {
            if a != b {
                differing += 1;
                assert!(
                    a.contains("complianceVerdict") && b.contains("complianceVerdict"),
                    "the only differing line must be the complianceVerdict triple: {a:?} vs {b:?}"
                );
            }
        }
        assert_eq!(differing, 1, "exactly one line must differ");
    }

    /// `with_subject_iri` adds exactly one `observedFeature` triple, changing
    /// nothing else.
    #[test]
    fn with_subject_iri_adds_exactly_one_observed_feature_triple() {
        let base_claim = demo_claim();
        let mut subject_claim = base_claim.clone();
        subject_claim.subject_iri =
            Some("https://blackcatinformatics.ca/gmeow/SomeTerm".to_owned());

        let base_nquads = project_compliance_assessment(&[base_claim], DEMO_GRAPH);
        let subject_nquads = project_compliance_assessment(&[subject_claim], DEMO_GRAPH);

        assert!(!base_nquads.contains("observedFeature"));
        assert_eq!(subject_nquads.matches("observedFeature").count(), 1);

        let base_lines: std::collections::BTreeSet<&str> = base_nquads.lines().collect();
        let extra_lines: Vec<&str> = subject_nquads
            .lines()
            .filter(|line| !base_lines.contains(line))
            .collect();
        assert_eq!(
            extra_lines.len(),
            1,
            "exactly one new line: {extra_lines:?}"
        );
        assert!(extra_lines[0].contains("observedFeature"));
        assert!(extra_lines[0].contains("<https://blackcatinformatics.ca/gmeow/SomeTerm>"));
    }

    /// Determinism: two calls on the same claims produce byte-identical
    /// strings, and a 2-claim input is sorted by `code`.
    #[test]
    fn emitter_is_deterministic_and_sorts_by_code() {
        let claim_z = Advisory::note("advice.z.later", "z advisory")
            .project()
            .claim;
        let claim_a = Advisory::note("advice.a.first", "a advisory")
            .project()
            .claim;

        let claims = [claim_z.clone(), claim_a.clone()];
        let first = project_compliance_assessment(&claims, DEMO_GRAPH);
        let second = project_compliance_assessment(&claims, DEMO_GRAPH);
        assert_eq!(first, second, "emitter must be byte-deterministic");

        let a_pos = first
            .find("advice.a.first")
            .expect("advice.a.first present");
        let z_pos = first
            .find("advice.z.later")
            .expect("advice.z.later present");
        assert!(
            a_pos < z_pos,
            "claims must be sorted by code (a before z):\n{first}"
        );
    }

    /// An out-of-range confidence is a HARD FAIL: the emitter panics rather
    /// than silently clamp or ship a meaningless literal.
    #[test]
    #[should_panic(expected = "confidence out of range")]
    fn out_of_range_confidence_hard_fails() {
        let claim = Advisory::note("advice.sample.demo", "prefer the active-voice phrasing")
            .with_confidence(1.5)
            .project()
            .claim;
        let _ = project_compliance_assessment(&[claim], DEMO_GRAPH);
    }

    /// A code carrying an IRI-unsafe character is a HARD FAIL: it would be
    /// interpolated verbatim into the content-addressed IRIs and mint an
    /// invalid N-Quad, so the emitter rejects it rather than ship malformed RDF.
    #[test]
    #[should_panic(expected = "is not IRI-safe")]
    fn non_iri_safe_code_hard_fails() {
        let claim = Advisory::note("advice tier active", "a code with a space")
            .project()
            .claim;
        let _ = project_compliance_assessment(&[claim], DEMO_GRAPH);
    }

    /// Two claims sharing a code is a HARD FAIL: the code keys all three IRIs,
    /// so a collision would emit conflicting triples on functional properties.
    #[test]
    #[should_panic(expected = "duplicate code")]
    fn duplicate_code_hard_fails() {
        let a = Advisory::note("advice.sample.demo", "first ruling")
            .project()
            .claim;
        let b = Advisory::note("advice.sample.demo", "second, conflicting ruling")
            .with_confidence(0.25)
            .project()
            .claim;
        let _ = project_compliance_assessment(&[a, b], DEMO_GRAPH);
    }
}
