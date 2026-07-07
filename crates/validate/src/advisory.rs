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

use gmeow_errors::{
    Advice, Diag, FindingCategory, Grade, Location, Rule, Severity, Standpoint, register_code,
};

// ── Standpoint & modality constants ─────────────────────────────────────────

/// The canonical best-practice standpoint IRI an advisory is issued from — the
/// `gmeow:vantage` of the recommendation claim (P9: advice is one standpoint's
/// perspectival claim, never a global verdict).  D1 carries it as a string only;
/// D4 reconciles it with a real `gmeow:Standpoint` individual.
pub const BEST_PRACTICE_STANDPOINT_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/standpoint/gmeowBestPractice";

/// The `gmeow:deonticRecommendation` modality individual IRI ("the issuer
/// advises the conduct without requiring it").  The soft-tier mirror of
/// `gmeow:deonticObligation`.
pub const DEONTIC_RECOMMENDATION_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/deonticRecommendation";

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
}
