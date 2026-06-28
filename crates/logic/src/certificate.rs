// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The scoped coherence certificate (ME13).
//!
//! A coherence check over a bundle yields a CONTRACT-SCOPED assertion:
//!
//! > No forbidden integrity violation and no undisclosed contradiction was found
//! > under contract **C**, over certified fragment **F**, within budget **B**,
//! > against bundle hash **H**.
//!
//! Two artifacts record this outcome, differing precisely in how complete the
//! inspection was — a **completeness gate**, because only a conclusive check can
//! *certify* coherence (design/LOGIC-CONFORMANCE.md, §The coherence certificate):
//!
//! * a [`CoherenceOutcome::Certificate`] — issued ONLY from a CONCLUSIVE check
//!   ([`ReasoningResult::is_conclusive`]: a completed run, or a complete-for-the-
//!   fragment answer) that found no forbidden violation. A complete check is the
//!   only thing entitled to the word *certify*;
//! * a [`CoherenceOutcome::Attestation`] — issued for a BOUNDED/INCOMPLETE check;
//!   it records only that none was found *within the completed search*. A
//!   budget-exhausted run produces an attestation, NEVER a certificate.
//!
//! A third outcome, [`CoherenceOutcome::Refused`], is neither: a forbidden
//! integrity violation (an unpermitted glut) was found, so coherence is REFUTED —
//! no certificate or attestation of coherence issues, and the violation is
//! surfaced as an error finding by the validator.
//!
//! **Permitted vs forbidden.** Whether a within-world glut is a forbidden
//! violation or a permitted, disclosed conflict is decided by the contract's
//! [`ContradictionPolicy`] (the `admissible_valuation` facet). A glut under a
//! glut-admitting policy is coherent — exactly the behaviour the paraconsistent
//! contract anticipates — and does NOT block a certificate.
//!
//! This is a pure-data builder (Principle 17), mirroring [`crate::result`]; the
//! authored source is Rust and the RDF emission is one lossy projection.

use std::collections::BTreeSet;

use gmeow_logic_compile::compat::ContradictionPolicy;
use gmeow_logic_compile::ir::LOGIC_NAMESPACE;

use crate::result::{
    Assumption, BudgetUsage, CompletenessStatus, ContradictionWitness, EngineId, EvaluationStatus,
    ReasoningResult,
};

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
/// The slice IRI the `logic:` coherence vocabulary is defined by.
const LOGIC_SLICE: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";

/// The full scoped payload a coherence outcome carries — the signed evidence the
/// certificate vouches over. Every field is load-bearing for the assertion; the
/// builder reads them off a [`ReasoningResult`] plus the bundle-level identity the
/// caller supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoherencePayload {
    /// The content-addressed identity of the exact bundle that was checked (`H`).
    pub bundle_hash: String,
    /// Content-addressed digests of the axiom sets inspected (one per fragment part).
    pub axiom_hashes: BTreeSet<String>,
    /// The reasoning-contract identity the check ran under (`C`).
    pub contract_hash: String,
    /// The engine identity + version that produced the result (`E`).
    pub engine: EngineId,
    /// The certified-complete fragment the assertion ranges over (`F`).
    pub certified_fragment: Option<String>,
    /// The closure/identity/revision/witness assumptions the check rests on.
    pub assumptions: BTreeSet<Assumption>,
    /// The resource budget consumed against the contract's allowance (`B`).
    pub consumed_budget: BudgetUsage,
    /// The answer-completeness axis (drives the certificate-vs-attestation gate).
    pub completeness: CompletenessStatus,
    /// The computation axis (drives the certificate-vs-attestation gate).
    pub evaluation: EvaluationStatus,
    /// The policy that classified each glut as permitted or forbidden.
    pub contradiction_policy: ContradictionPolicy,
    /// Constructs the lowering could not carry exactly (the loss ledger).
    pub projection_losses: BTreeSet<String>,
    /// Disclosed contradictions the contract PERMITS (do not block a certificate).
    pub permitted_conflicts: Vec<ContradictionWitness>,
    /// Contradictions the contract FORBIDS (a non-empty set refutes coherence).
    pub forbidden_violations: Vec<ContradictionWitness>,
    /// The issue timestamp — INJECTED, never sampled, so the fold is deterministic.
    pub issued_at: String,
}

/// The outcome of a scoped coherence check (the completeness gate + the permitted/
/// forbidden partition).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoherenceOutcome {
    /// A CONCLUSIVE check that found no forbidden violation: a real certificate.
    Certificate(CoherencePayload),
    /// A BOUNDED/INCOMPLETE check that found no forbidden violation: the strictly
    /// weaker attestation.
    Attestation(CoherencePayload),
    /// A forbidden integrity violation was found: coherence is REFUTED. No
    /// certificate or attestation of coherence issues.
    Refused(CoherencePayload),
}

impl CoherenceOutcome {
    /// Build the outcome from a [`ReasoningResult`] and the bundle-level identity.
    ///
    /// The contradiction witnesses are partitioned into permitted vs forbidden by
    /// `contradiction_policy`. The gate is then:
    /// * any forbidden violation ⇒ [`Self::Refused`] (regardless of completeness);
    /// * else a conclusive check ⇒ [`Self::Certificate`];
    /// * else ⇒ [`Self::Attestation`].
    ///
    /// `issued_at` is injected (never sampled) so re-running with the same inputs is
    /// byte-identical.
    pub fn from_reasoning_result(
        result: &ReasoningResult,
        bundle_hash: impl Into<String>,
        axiom_hashes: impl IntoIterator<Item = impl Into<String>>,
        contradiction_policy: ContradictionPolicy,
        issued_at: impl Into<String>,
    ) -> Self {
        // Partition the witnesses purely on the policy: a glut is permitted iff the
        // valuation admits a glut, otherwise it is a forbidden violation.
        let glut_permitted = contradiction_policy.glut_permitted();
        let (permitted_conflicts, forbidden_violations): (Vec<_>, Vec<_>) = if glut_permitted {
            (
                result.provenance.contradiction_witnesses.clone(),
                Vec::new(),
            )
        } else {
            (
                Vec::new(),
                result.provenance.contradiction_witnesses.clone(),
            )
        };

        let payload = CoherencePayload {
            bundle_hash: bundle_hash.into(),
            axiom_hashes: axiom_hashes.into_iter().map(Into::into).collect(),
            contract_hash: result.provenance.contract_hash.clone(),
            engine: result.provenance.engine.clone(),
            certified_fragment: result.provenance.certified_fragment.clone(),
            assumptions: result.provenance.assumptions.clone(),
            consumed_budget: result.provenance.consumed_budget,
            completeness: result.completeness,
            evaluation: result.evaluation,
            contradiction_policy,
            projection_losses: result.preservation.unsupported_constructs.clone(),
            permitted_conflicts,
            forbidden_violations,
            issued_at: issued_at.into(),
        };

        if !payload.forbidden_violations.is_empty() {
            Self::Refused(payload)
        } else if result.is_conclusive() {
            Self::Certificate(payload)
        } else {
            Self::Attestation(payload)
        }
    }

    /// The carried payload, whatever the outcome.
    pub fn payload(&self) -> &CoherencePayload {
        match self {
            Self::Certificate(p) | Self::Attestation(p) | Self::Refused(p) => p,
        }
    }

    /// `true` iff a `logic:CoherenceCertificate` issues (a conclusive check with no
    /// forbidden violation).
    pub fn issues_certificate(&self) -> bool {
        matches!(self, Self::Certificate(_))
    }

    /// `true` iff coherence was refuted by a forbidden integrity violation.
    pub fn is_refused(&self) -> bool {
        matches!(self, Self::Refused(_))
    }

    /// The `module.ttl` class local name of the issued artifact, or `None` for a
    /// refusal (no coherence artifact issues).
    pub fn class_local_name(&self) -> Option<&'static str> {
        match self {
            Self::Certificate(_) => Some("CoherenceCertificate"),
            Self::Attestation(_) => Some("CoherenceCheckAttestation"),
            Self::Refused(_) => None,
        }
    }

    /// Project the issued coherence artifact into N-Quads in `graph_iri`, typed
    /// `logic:CoherenceCertificate` / `logic:CoherenceCheckAttestation` and carrying
    /// the full scoped payload. Returns an empty string for a [`Self::Refused`]
    /// outcome — no coherence artifact issues; the violation rides as findings.
    ///
    /// Deterministic: a content-addressed subject IRI, sorted multi-valued
    /// properties, and the injected timestamp make re-runs byte-identical.
    pub fn to_nquads(&self, graph_iri: &str) -> String {
        let Some(class_local) = self.class_local_name() else {
            return String::new();
        };
        let payload = self.payload();
        let graph = format!("<{graph_iri}>");
        let id = content_id(class_local, payload);
        let subject = format!("<{GMEOW_NS}coherence/{id}>");
        let mut lines: Vec<String> = Vec::new();

        let mut triple = |s: &str, p: &str, o: &str| {
            lines.push(format!("{s} <{p}> {o} {graph} ."));
        };

        // Assertional-tier self-description (so the folded named graph passes the
        // structural lint, mirroring the release-attestation authoring style).
        triple(
            &subject,
            RDF_TYPE,
            &format!("<{LOGIC_NAMESPACE}{class_local}>"),
        );
        triple(
            &subject,
            RDFS_LABEL,
            &format!("\"{}\"", nq_escape(&self.label())),
        );
        triple(&subject, RDFS_IS_DEFINED_BY, &format!("<{LOGIC_SLICE}>"));
        triple(
            &subject,
            &format!("{GMEOW_NS}graphBoxRole"),
            &format!("<{GMEOW_NS}boxABox>"),
        );

        // The scoped identity payload.
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}bundleHash"),
            &format!("\"{}\"", nq_escape(&payload.bundle_hash)),
        );
        for axiom_hash in &payload.axiom_hashes {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}axiomHash"),
                &format!("\"{}\"", nq_escape(axiom_hash)),
            );
        }
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}contractHash"),
            &format!("\"{}\"", nq_escape(&payload.contract_hash)),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}engine"),
            &format!(
                "\"{}\"",
                nq_escape(&format!(
                    "{} {}",
                    payload.engine.name, payload.engine.version
                ))
            ),
        );
        if let Some(fragment) = &payload.certified_fragment {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}certifiedFragment"),
                &format!("\"{}\"", nq_escape(fragment)),
            );
        }
        for assumption in &payload.assumptions {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}resultAssumption"),
                &format!("\"{}\"", nq_escape(assumption.wire())),
            );
        }
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}consumedBudget"),
            &format!("\"{}\"", nq_escape(&budget_text(&payload.consumed_budget))),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}contradictionPolicy"),
            &format!("<{}>", payload.contradiction_policy.iri()),
        );
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}checkIssuedAt"),
            &format!("\"{}\"^^<{XSD_DATETIME}>", nq_escape(&payload.issued_at)),
        );
        for loss in &payload.projection_losses {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}projectionLoss"),
                &format!("\"{}\"", nq_escape(loss)),
            );
        }
        // Disclosed permitted conflicts (sorted by their clash individual) point at
        // the individual forced into the glut. forbidden_violations is empty here
        // (a non-empty set would have made this a Refused outcome).
        let mut permitted: Vec<&str> = payload
            .permitted_conflicts
            .iter()
            .map(|w| w.individual.as_str())
            .collect();
        permitted.sort_unstable();
        permitted.dedup();
        for individual in permitted {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}permittedConflictWitness"),
                &format!("<{individual}>"),
            );
        }

        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }

    /// A concise human label for the issued artifact.
    fn label(&self) -> String {
        let payload = self.payload();
        match self {
            Self::Certificate(_) => format!(
                "Coherence certificate over bundle {} under contract {}",
                short_hash(&payload.bundle_hash),
                short_hash(&payload.contract_hash)
            ),
            Self::Attestation(_) => format!(
                "Coherence check attestation over bundle {} under contract {}",
                short_hash(&payload.bundle_hash),
                short_hash(&payload.contract_hash)
            ),
            Self::Refused(_) => String::new(),
        }
    }
}

/// Render a budget usage as a stable, human-readable string.
fn budget_text(budget: &BudgetUsage) -> String {
    let mut text = format!("consumed={}", budget.consumed);
    if let Some(allowance) = budget.allowance {
        text.push_str(&format!(" allowance={allowance}"));
    }
    if let Some(limit) = budget.limit {
        text.push_str(&format!(" limit={}", limit.wire()));
    }
    text
}

/// The first 12 characters of a hash (past any `algo:` prefix), for labels.
fn short_hash(hash: &str) -> String {
    let bare = hash.split_once(':').map_or(hash, |(_, rest)| rest);
    bare.chars().take(12).collect()
}

/// A deterministic content-addressed local id for the artifact's subject IRI: a
/// stable FNV-1a hash (hex) over the class + the scoping identity, so two
/// certificates over distinct bundles/contracts/timestamps never collide and the
/// same inputs always reproduce the same IRI. Dependency-free and platform-stable.
fn content_id(class_local: &str, payload: &CoherencePayload) -> String {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    let mut absorb = |part: &str| {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    };
    absorb(class_local);
    absorb(&payload.bundle_hash);
    absorb(&payload.contract_hash);
    absorb(&payload.issued_at);
    format!("{hash:016x}")
}

/// Escape a string literal for N-Triples/N-Quads (mirrors the diagnostics emitter).
fn nq_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::result::{
        InformationState, InputStatus, PreservationClaim, ResultPayload, ResultProvenance,
    };

    /// A consistent native result (no contradiction witnesses), with the given
    /// evaluation/completeness axes.
    fn consistent_result(
        evaluation: EvaluationStatus,
        completeness: CompletenessStatus,
    ) -> ReasoningResult {
        ReasoningResult {
            input: InputStatus::Valid,
            evaluation,
            completeness,
            preservation: PreservationClaim::exact(),
            information: InformationState::Supported,
            provenance: ResultProvenance::native("contract:abc", "world:default"),
            payload: ResultPayload::Empty,
            row_schema: None,
        }
    }

    /// A glut result carrying one contradiction witness.
    fn glut_result() -> ReasoningResult {
        let mut result = consistent_result(
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
        );
        result.information = InformationState::Both;
        result
            .provenance
            .contradiction_witnesses
            .push(ContradictionWitness {
                individual: "https://example.org/clash".to_owned(),
                world: "world:default".to_owned(),
                premises: vec![],
            });
        result
    }

    #[test]
    fn conclusive_consistent_yields_certificate() {
        let result = consistent_result(EvaluationStatus::Completed, CompletenessStatus::Unknown);
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            ["blake3:axioms"],
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
        );
        assert!(outcome.issues_certificate());
        assert_eq!(outcome.class_local_name(), Some("CoherenceCertificate"));
    }

    #[test]
    fn bounded_incomplete_yields_attestation_never_certificate() {
        // GENUINELY non-conclusive: budget-exhausted AND incomplete (not merely
        // budget-exhausted, which is_conclusive() can still pass via complete-for-
        // fragment).
        let result = consistent_result(
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::Incomplete,
        );
        assert!(!result.is_conclusive());
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
        );
        assert!(!outcome.issues_certificate());
        assert_eq!(
            outcome.class_local_name(),
            Some("CoherenceCheckAttestation")
        );
        assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
    }

    #[test]
    fn budget_exhausted_but_complete_for_fragment_still_certifies() {
        // is_conclusive() is true via complete-for-fragment even though the run hit
        // a budget — the gate keys on is_conclusive(), not on evaluation alone.
        let result = consistent_result(
            EvaluationStatus::BudgetExhausted,
            CompletenessStatus::CompleteForFragment,
        );
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
        );
        assert!(outcome.issues_certificate());
    }

    #[test]
    fn permitted_glut_keeps_the_certificate() {
        // A glut under a glut-admitting contract is a permitted, disclosed conflict:
        // the certificate still issues and the conflict is recorded in the payload.
        let outcome = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGap, // admits a glut
            "2026-06-28T00:00:00Z",
        );
        assert!(
            outcome.issues_certificate(),
            "permitted glut must still certify"
        );
        assert_eq!(outcome.payload().permitted_conflicts.len(), 1);
        assert!(outcome.payload().forbidden_violations.is_empty());
    }

    #[test]
    fn forbidden_glut_refuses_and_issues_no_certificate() {
        // The same glut under a glut-FORBIDDING contract refutes coherence: no
        // certificate, no attestation; the violation is recorded for the findings.
        let outcome = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut, // forbids a glut
            "2026-06-28T00:00:00Z",
        );
        assert!(outcome.is_refused());
        assert!(!outcome.issues_certificate());
        assert_eq!(outcome.class_local_name(), None);
        assert_eq!(outcome.payload().forbidden_violations.len(), 1);
        // A refusal emits no coherence artifact.
        assert!(outcome.to_nquads("https://example.org/g").is_empty());
    }

    #[test]
    fn nquads_are_well_formed_and_byte_stable() {
        let outcome = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            ["blake3:axioms"],
            ContradictionPolicy::ForbidGap,
            "2026-06-28T00:00:00Z",
        );
        let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
        let nquads = outcome.to_nquads(graph);
        // Re-running with the same inputs is byte-identical.
        assert_eq!(nquads, outcome.to_nquads(graph));
        // Typed as a certificate, carries the payload, lands in the named graph.
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/CoherenceCertificate>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/bundleHash>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/contradictionPolicy> <https://blackcatinformatics.ca/logic/ForbidGap>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/permittedConflictWitness> <https://example.org/clash>"));
        for line in nquads.lines() {
            assert!(
                line.ends_with(&format!("<{graph}> .")),
                "line not in graph: {line}"
            );
        }
    }
}
