// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Advisory-tier emission seam — the D1 keystone for EPIC #759 / issue #760.
//!
//! # Dual-projection contract
//!
//! One [`Advisory`] value produces, via a single [`Advisory::project`] call,
//! BOTH projections simultaneously:
//!
//! 1. A flat [`gmeow_diagnostics::Finding`] at [`Severity::Note`] or
//!    [`Severity::Info`] — the linter/SARIF/CLI surface consumed by every
//!    existing diagnostics renderer.
//! 2. An in-memory [`AdvisoryClaim`] hook carrying the vantage IRI, the advised
//!    proposition, and the deontic-modality IRI.  Issue #763 (D4) will later
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
//! D4 (#763) reconciles the string IRI with a real `gmeow:Standpoint` individual
//! when it materialises the RDF claim.

use gmeow_diagnostics::{Finding, Location, Rule, Severity};

// ── Standpoint & modality constants ─────────────────────────────────────────

/// The canonical best-practice standpoint IRI an advisory is issued from — the
/// `gmeow:vantage` of the recommendation claim (P9: advice is one standpoint's
/// perspectival claim, never a global verdict).  D1 carries it as a string only;
/// D4 (#763) reconciles it with a real `gmeow:Standpoint` individual.
pub const BEST_PRACTICE_STANDPOINT_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/standpoint/gmeowBestPractice";

/// The `gmeow:deonticRecommendation` modality individual IRI ("the issuer
/// advises the conduct without requiring it").  The soft-tier mirror of
/// `gmeow:deonticObligation`.
pub const DEONTIC_RECOMMENDATION_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/deonticRecommendation";

// ── Core types ───────────────────────────────────────────────────────────────

/// A best-practice advisory ready to emit as a flat [`Finding`] and an
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

/// The dual projection of one advisory event (P4/P17): a flat [`Finding`] and
/// the in-memory claim hook D4 (#763) fills.  Produced together by
/// [`Advisory::project`].
///
/// # Invariant
///
/// `finding.code == claim.code` — the flat finding and the claim hook always
/// refer to the same diagnostic rule.
#[derive(Debug, Clone, PartialEq)]
pub struct AdvisoryProjection {
    /// The flat linter/SARIF/CLI surface for this advisory event.
    pub finding: Finding,
    /// The in-memory claim hook for D4 (#763) to materialise as RDF.
    pub claim: AdvisoryClaim,
}

/// The vantage-indexed recommendation-claim HOOK (#760 keystone; filled by
/// #763).
///
/// Carries what a `gmeow:ComplianceAssessment` / `StandpointClaim` needs — the
/// issuing standpoint (vantage), the advised proposition (message text), and the
/// `deonticRecommendation` modality IRI — WITHOUT emitting RDF in D1.
///
/// # Lifecycle
///
/// D1 constructs this struct; D4 (#763) consumes it, resolves
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
    /// `project` yields exactly one [`Finding`] AND one [`AdvisoryClaim`].
    ///
    /// # Finding construction
    ///
    /// * `severity` / `code` / `message` forwarded verbatim.
    /// * Tool stamp set to `"validate"`.
    /// * Each `suggestion` pushed into `finding.suggestions`.
    /// * Each `location` pushed via [`Finding::add_location`] (empty locations
    ///   are filtered by that method).
    /// * `tags` cloned into `finding.tags`.
    ///
    /// # Claim construction
    ///
    /// * `standpoint_iri` carried from `self`.
    /// * `advised_proposition` = `self.message`.
    /// * `modality_iri` = [`DEONTIC_RECOMMENDATION_IRI`].
    /// * `code` = `self.code`.
    pub fn project(&self) -> AdvisoryProjection {
        let mut finding = Finding::new(self.severity, self.code.clone(), self.message.clone())
            .with_tool("validate");

        for suggestion in &self.suggestions {
            finding.suggestions.push(suggestion.clone());
        }
        for location in &self.locations {
            finding.add_location(location.clone());
        }
        finding.tags = self.tags.clone();

        let claim = AdvisoryClaim {
            standpoint_iri: self.standpoint_iri.clone(),
            advised_proposition: self.message.clone(),
            modality_iri: DEONTIC_RECOMMENDATION_IRI.to_owned(),
            code: self.code.clone(),
        };

        AdvisoryProjection { finding, claim }
    }

    /// The soft [`Rule`] to register on the [`gmeow_diagnostics::Report`] so
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

    /// Both projection wings are produced in one call and carry the expected
    /// field values — the core dual-projection-always contract.
    #[test]
    fn project_yields_both_flat_finding_and_claim() {
        let advisory = Advisory::note("advice.sample", "consider a more specific sortal")
            .with_suggestion("use gmeow:Kind")
            .with_help_uri("https://example.org/docs/advice#sample");

        let AdvisoryProjection { finding, claim } = advisory.project();

        // ── flat finding assertions ──────────────────────────────────────────
        assert_eq!(finding.severity, Severity::Note);
        assert_eq!(finding.code, "advice.sample");
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

    /// `project()` always returns exactly ONE finding and ONE claim — the 1:1
    /// structural invariant of the dual-projection-always contract.
    #[test]
    fn one_advisory_one_claim() {
        let advisory = Advisory::note("advice.sanity", "sanity check advisory");
        let projection = advisory.project();

        // Destructure to confirm both wings exist (would not compile otherwise).
        let AdvisoryProjection { finding, claim } = projection;

        // The codes must agree — the finding and claim refer to the same rule.
        assert_eq!(finding.code, claim.code);
    }
}
