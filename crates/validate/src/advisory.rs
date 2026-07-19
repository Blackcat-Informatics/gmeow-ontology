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

/// The single fixed demonstrator advisory (D1/D3/D4): emits a Note finding
/// distinct from compliance errors on every normal-completion run, so `gmeow
/// validate` surfaces the advisory tier before harvested advisory rules land
/// (D3 replaces this demonstrator; find it via the `"advisory-demonstrator"`
/// tag).
///
/// SINGLE-SOURCED (D4): the CLI path
/// ([`crate::validate_all`]) and the pipeline path
/// (`crates/pipeline/src/stages/validate.rs::ValidateStage::run`) both call
/// this function rather than each inlining their own `Advisory::note(...)`
/// builder, so the two consumer surfaces can never drift apart.
pub fn advisory_demonstrator() -> Advisory {
    Advisory::note(
        crate::codes::ADVICE_TIER_ACTIVE,
        "Advisory tier active — soft (deonticRecommendation) advice will surface here once advisory rules are harvested.",
    )
    .with_suggestion("Run `gmeow describe <term>` to see modeling guidance (avoidWhen / useWhen / howToUse).")
    .with_help_uri("https://blackcatinformatics.ca/gmeow/advice")
    .with_tag("advisory-demonstrator")
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
    if let Some(dup) = sorted_claims.windows(2).find_map(|w| {
        (w[0].code == w[1].code).then_some(&w[0].code)
    }) {
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

    // ── (D4) project_compliance_assessment ──────────────────────────────────

    const DEMO_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

    /// The demonstrator-style claim: code `advice.tier.active`, all defaults.
    fn demo_claim() -> AdvisoryClaim {
        Advisory::note(
            "advice.tier.active",
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
        let claim = Advisory::note("advice.tier.active", "prefer the active-voice phrasing")
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
        let a = Advisory::note("advice.tier.active", "first ruling")
            .project()
            .claim;
        let b = Advisory::note("advice.tier.active", "second, conflicting ruling")
            .with_confidence(0.25)
            .project()
            .claim;
        let _ = project_compliance_assessment(&[a, b], DEMO_GRAPH);
    }
}
