// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The scoped coherence certificate.
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

use std::collections::{BTreeMap, BTreeSet};

use gmeow_logic_compile::ir::LOGIC_NAMESPACE;
use purrdf::prelude::{RdfDataset, TermRef};

// Re-export so consumers (e.g. the validator) reach the contradiction policy
// through the coherence module rather than depending on logic-compile directly.
pub use gmeow_logic_compile::compat::ContradictionPolicy;

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
    /// Intentional serialization/projection losses from the static loss ledger
    /// (genuine `gts → projection-codec` losses, NOT DL-reasoner constructs).
    pub projection_losses: BTreeSet<String>,
    /// DL constructs the native reasoner could not decide — sourced from
    /// `ReasoningResult::preservation.unsupported_constructs`. Distinct from
    /// `projection_losses` (which are serialization losses, not reasoning gaps).
    pub unsupported_constructs: BTreeSet<String>,
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

/// The bare trichotomy the completeness gate ([`CoherenceOutcome::classify`])
/// decides — payload-free, so it can back both the payload-carrying
/// [`CoherenceOutcome`] variant and the payload-free
/// [`CoherenceOutcome::class_local_name_for`] lookup from the ONE decision.
enum Gate {
    Certificate,
    Attestation,
    Refused,
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
    ///
    /// # Certificate-requires-fragment gate
    /// A conclusive, violation-free check would issue a [`Self::Certificate`], but a
    /// scoped certificate that names no certified fragment `F` cannot honour the
    /// scoped-certificate assertion (the claim ranges over `F`). When the conclusive
    /// result carries no `certified_fragment`, the outcome is DOWNGRADED to a
    /// [`Self::Attestation`] (the honest weaker claim) rather than a fragment-less
    /// certificate — a certificate is NEVER silently emitted with no fragment.
    ///
    /// # Errors
    /// Returns `Result` for forward-compatibility with stricter future gates; the
    /// current builder is infallible.
    pub fn from_reasoning_result(
        result: &ReasoningResult,
        bundle_hash: impl Into<String>,
        axiom_hashes: impl IntoIterator<Item = impl Into<String>>,
        contradiction_policy: ContradictionPolicy,
        issued_at: impl Into<String>,
        projection_loss_codes: BTreeSet<String>,
    ) -> gmeow_errors::Result<Self> {
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
            // The caller-supplied projection-loss codes ARE the sorted set the content
            // hash folds over — sourced directly from the `BTreeSet`, no ceremonial
            // round-trip through a throwaway loss store (which was a byte-identical no-op:
            // interning each code under `preservation.rung.<code>` then stripping the
            // prefix back recovers the same set).
            projection_losses: projection_loss_codes.clone(),
            unsupported_constructs: result.preservation.unsupported_constructs.clone(),
            permitted_conflicts,
            forbidden_violations,
            issued_at: issued_at.into(),
        };

        let conclusive = result.is_conclusive();
        let forbidden_violation_present = !payload.forbidden_violations.is_empty();
        let fragment_present = payload.certified_fragment.is_some();
        Ok(
            match Self::classify(conclusive, forbidden_violation_present, fragment_present) {
                Gate::Refused => Self::Refused(payload),
                Gate::Certificate => Self::Certificate(payload),
                Gate::Attestation => Self::Attestation(payload),
            },
        )
    }

    /// The pure trichotomy decision, shared by [`Self::from_reasoning_result`] (which
    /// wraps the winning variant around a full [`CoherencePayload`]) and
    /// [`Self::class_local_name_for`] (a payload-free class lookup for callers that
    /// need only the label) — ONE gate, never re-derived ad hoc by a caller.
    ///
    /// * a forbidden violation refutes coherence. A CONCLUSIVE check that found one
    ///   is a flat refusal — no coherence artifact issues. A NON-conclusive check
    ///   that ran into one cannot refute coherence wholesale (it never completed the
    ///   search), but it must still DISCLOSE what it found: it issues an attestation
    ///   carrying the `logic:forbiddenViolationWitness` set. Only a bounded check can
    ///   carry a non-empty forbidden set on an issued artifact; a certificate's set
    ///   is necessarily empty;
    /// * else a conclusive, violation-free check certifies — but ONLY over a NAMED
    ///   certified fragment;
    /// * else (non-conclusive, or conclusive but fragment-less) the strongest honest
    ///   claim is the weaker attestation. A certificate is NEVER emitted
    ///   fragment-less.
    fn classify(
        conclusive: bool,
        forbidden_violation_present: bool,
        fragment_present: bool,
    ) -> Gate {
        if forbidden_violation_present {
            if conclusive {
                Gate::Refused
            } else {
                Gate::Attestation
            }
        } else if conclusive && fragment_present {
            Gate::Certificate
        } else {
            Gate::Attestation
        }
    }

    /// Payload-free class-local-name lookup over the SAME trichotomy gate as
    /// [`Self::from_reasoning_result`] — for callers (the MCP tool surface,
    /// `crates/pipeline/src/mcp.rs`) that need only the completeness-class LABEL, not
    /// the full scoped [`CoherencePayload`] (bundle hash, axiom hashes, issue
    /// timestamp, …) `from_reasoning_result` requires. Reusing this gate — rather than
    /// re-deriving `is_conclusive()`/fragment/violation checks per call site — is what
    /// guarantees `run_verify_graph` and `run_explain_quad` can never diverge on
    /// whether an inconsistent-but-conclusive closure earns a `CoherenceCertificate`.
    ///
    /// Returns `"Refused"` for a witnessed forbidden violation in a CONCLUSIVE check.
    /// This differs from [`Self::class_local_name`] (which returns `None` for a
    /// refusal, because no RDF artifact issues for one) — this is a tool-surface
    /// label a caller can render directly, not an RDF class name.
    pub fn class_local_name_for(
        result: &ReasoningResult,
        contradiction_policy: ContradictionPolicy,
    ) -> &'static str {
        let forbidden_violation_present = !contradiction_policy.glut_permitted()
            && !result.provenance.contradiction_witnesses.is_empty();
        let fragment_present = result.provenance.certified_fragment.is_some();
        match Self::classify(
            result.is_conclusive(),
            forbidden_violation_present,
            fragment_present,
        ) {
            Gate::Refused => "Refused",
            Gate::Certificate => "CoherenceCertificate",
            Gate::Attestation => "CoherenceCheckAttestation",
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

        // Link the artifact to the logic:ReasoningResult it summarizes (M2): a
        // deterministically minted result individual, content-addressed on the
        // result identity the certificate is scoped by (contract + fragment +
        // engine), so contract/fragment/budget are reachable from the certificate as
        // the ontology asserts (logic:summarizesResult, single-valued). The result
        // node carries its own type + the contract/fragment/budget facets it backs.
        let result_id = result_content_id(payload);
        let result_subject = format!("<{GMEOW_NS}coherence/result/{result_id}>");
        triple(
            &subject,
            &format!("{LOGIC_NAMESPACE}summarizesResult"),
            &result_subject,
        );
        triple(
            &result_subject,
            RDF_TYPE,
            &format!("<{LOGIC_NAMESPACE}ReasoningResult>"),
        );
        triple(
            &result_subject,
            RDFS_IS_DEFINED_BY,
            &format!("<{LOGIC_SLICE}>"),
        );
        triple(
            &result_subject,
            &format!("{GMEOW_NS}graphBoxRole"),
            &format!("<{GMEOW_NS}boxABox>"),
        );
        triple(
            &result_subject,
            &format!("{LOGIC_NAMESPACE}contractHash"),
            &format!("\"{}\"", nq_escape(&payload.contract_hash)),
        );
        if let Some(fragment) = &payload.certified_fragment {
            triple(
                &result_subject,
                &format!("{LOGIC_NAMESPACE}certifiedFragment"),
                &format!("\"{}\"", nq_escape(fragment)),
            );
        }
        triple(
            &result_subject,
            &format!("{LOGIC_NAMESPACE}consumedBudget"),
            &format!("\"{}\"", nq_escape(&budget_text(&payload.consumed_budget))),
        );
        // The two completeness-gate axes ride the result node as their `module.ttl`
        // status individuals (the SAME `logic:resultEvaluation` / `logic:resultCompleteness`
        // predicates + individuals the `graph/reasoning` projection uses), so the full
        // payload — including the axes that DECIDE certificate-vs-attestation — is
        // faithfully recoverable from the carried quads, not merely folded into the
        // content-addressed subject id.
        triple(
            &result_subject,
            &format!("{LOGIC_NAMESPACE}resultEvaluation"),
            &format!("<{}>", payload.evaluation.iri()),
        );
        triple(
            &result_subject,
            &format!("{LOGIC_NAMESPACE}resultCompleteness"),
            &format!("<{}>", payload.completeness.iri()),
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
        for construct in &payload.unsupported_constructs {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}unsupportedConstruct"),
                &format!("\"{}\"", nq_escape(construct)),
            );
        }
        // Disclosed permitted conflicts (sorted by their clash individual) point at
        // the individual forced into the glut.
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

        // Disclosed FORBIDDEN violations (the producer of logic:forbiddenViolationWitness):
        // non-empty only on a bounded CoherenceCheckAttestation that ran into a
        // forbidden glut without completing — a conclusive check with a forbidden
        // violation is Refused and serializes nothing. On an issued certificate this
        // set is necessarily empty.
        let mut forbidden: Vec<&str> = payload
            .forbidden_violations
            .iter()
            .map(|w| w.individual.as_str())
            .collect();
        forbidden.sort_unstable();
        forbidden.dedup();
        for individual in forbidden {
            triple(
                &subject,
                &format!("{LOGIC_NAMESPACE}forbiddenViolationWitness"),
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
    use std::fmt::Write as _;
    let mut text = String::new();
    write!(text, "consumed={}", budget.consumed).expect("write to String is infallible");
    if let Some(allowance) = budget.allowance {
        write!(text, " allowance={allowance}").expect("write to String is infallible");
    }
    if let Some(limit) = budget.limit {
        write!(text, " limit={}", limit.wire()).expect("write to String is infallible");
    }
    text
}

/// The first 12 characters of a hash (past any `algo:` prefix), for labels.
fn short_hash(hash: &str) -> String {
    let bare = hash.split_once(':').map_or(hash, |(_, rest)| rest);
    bare.chars().take(12).collect()
}

/// Build a deterministic canonical string for blake3 ingestion over the full
/// certificate payload. Field order is fixed; collections are sorted
/// (BTreeSet/BTreeMap preserve order; Vec witnesses are sorted below).
fn content_id_canonical(class_local: &str, payload: &CoherencePayload) -> String {
    use std::fmt::Write as _;
    let mut buf = String::new();
    // class / outcome kind
    writeln!(buf, "class={class_local}").unwrap();
    // bundle + contract
    writeln!(buf, "bundle_hash={}", payload.bundle_hash).unwrap();
    // axiom_hashes: BTreeSet is already sorted
    for h in &payload.axiom_hashes {
        writeln!(buf, "axiom_hash={h}").unwrap();
    }
    writeln!(buf, "contract_hash={}", payload.contract_hash).unwrap();
    // contradiction_policy
    writeln!(
        buf,
        "contradiction_policy={}",
        payload.contradiction_policy.iri()
    )
    .unwrap();
    // certified_fragment
    if let Some(fragment) = &payload.certified_fragment {
        writeln!(buf, "certified_fragment={fragment}").unwrap();
    }
    // budget
    writeln!(buf, "budget={}", budget_text(&payload.consumed_budget)).unwrap();
    // completeness / evaluation axes
    writeln!(buf, "completeness={:?}", payload.completeness).unwrap();
    writeln!(buf, "evaluation={:?}", payload.evaluation).unwrap();
    // projection_losses: BTreeSet is sorted
    for loss in &payload.projection_losses {
        writeln!(buf, "projection_loss={loss}").unwrap();
    }
    // unsupported_constructs: BTreeSet is sorted
    for construct in &payload.unsupported_constructs {
        writeln!(buf, "unsupported_construct={construct}").unwrap();
    }
    // permitted_conflicts: sort by individual IRI for determinism
    let mut permitted: Vec<&str> = payload
        .permitted_conflicts
        .iter()
        .map(|w| w.individual.as_str())
        .collect();
    permitted.sort_unstable();
    permitted.dedup();
    for individual in permitted {
        writeln!(buf, "permitted_conflict={individual}").unwrap();
    }
    // forbidden_violations: sort by individual IRI for determinism
    let mut forbidden: Vec<&str> = payload
        .forbidden_violations
        .iter()
        .map(|w| w.individual.as_str())
        .collect();
    forbidden.sort_unstable();
    forbidden.dedup();
    for individual in forbidden {
        writeln!(buf, "forbidden_violation={individual}").unwrap();
    }
    // issued_at (last, as a temporal anchor)
    writeln!(buf, "issued_at={}", payload.issued_at).unwrap();
    buf
}

/// A deterministic content-addressed local id for the artifact's subject IRI:
/// a blake3 digest (hex, first 16 bytes = 32 hex chars) over the FULL canonical
/// payload. Covers class/outcome kind, bundle_hash, all axiom_hashes (sorted),
/// contract_hash, contradiction_policy, certified_fragment, budget,
/// completeness/evaluation status, projection_losses (sorted),
/// unsupported_constructs (sorted), permitted-conflict and forbidden-violation
/// witness sets (sorted), and issued_at — eliminating the latent subject-IRI
/// collision of the former FNV-1a that hashed only four fields.
fn content_id(class_local: &str, payload: &CoherencePayload) -> String {
    let canonical = content_id_canonical(class_local, payload);
    let digest = blake3::hash(canonical.as_bytes());
    // 32 hex chars (first 16 bytes of the 32-byte blake3 output) — compact but
    // still far beyond collision probability for any realistic certificate volume.
    digest.to_hex()[..32].to_owned()
}

/// A deterministic content-addressed local id for the minted `logic:ReasoningResult`
/// individual the certificate's `logic:summarizesResult` points at. Keyed on the
/// result identity the certificate is scoped by — contract, certified fragment, and
/// engine — so the same result reproduces the same IRI and two distinct results never
/// collide. Uses blake3 for consistency with [`content_id`].
fn result_content_id(payload: &CoherencePayload) -> String {
    use std::fmt::Write as _;
    let mut buf = String::new();
    writeln!(buf, "kind=ReasoningResult").unwrap();
    writeln!(buf, "contract_hash={}", payload.contract_hash).unwrap();
    writeln!(
        buf,
        "certified_fragment={}",
        payload.certified_fragment.as_deref().unwrap_or("")
    )
    .unwrap();
    writeln!(buf, "engine_name={}", payload.engine.name).unwrap();
    writeln!(buf, "engine_version={}", payload.engine.version).unwrap();
    let digest = blake3::hash(buf.as_bytes());
    digest.to_hex()[..32].to_owned()
}

/// Compute a stable, deterministic digest per axiom-bearing named graph of a
/// dataset — the per-fragment axiom hashes a [`CoherencePayload`] signs over.
///
/// Each graph's quads are rendered as canonical, sorted, N-Quad-ish lines and hashed
/// with the caller-supplied `digest` primitive (the SAME blake3 `digest_string` used
/// for the bundle hash, injected so this PyO3-free crate takes no gmeow-gts edge).
/// The resulting Vec is sorted so re-runs are byte-identical. The default graph (no
/// graph name) is keyed under an empty string.
///
/// Shared by the validate `--deep` lane and the release lane so both pin axiom sets
/// identically (one construction, no drift).
pub fn per_graph_axiom_hashes(
    dataset: &RdfDataset,
    digest: impl Fn(&[u8]) -> String,
) -> Vec<String> {
    let render = |t: TermRef<'_>| -> String {
        match t {
            TermRef::Iri(iri) => format!("<{iri}>"),
            TermRef::Blank { label, .. } => format!("_:{label}"),
            TermRef::Literal {
                lexical, language, ..
            } => match language {
                Some(lang) => format!("\"{}\"@{lang}", nq_escape(lexical)),
                None => format!("\"{}\"", nq_escape(lexical)),
            },
            TermRef::Triple { .. } => "<<triple>>".to_owned(),
        }
    };

    // Group canonical per-quad lines by their graph name.
    let mut per_graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for q in dataset.quads() {
        let graph_key = match q.g {
            Some(g) => match dataset.resolve(g) {
                TermRef::Iri(iri) => iri.to_owned(),
                other => render(other),
            },
            None => String::new(),
        };
        let line = format!(
            "{} {} {} .",
            render(dataset.resolve(q.s)),
            render(dataset.resolve(q.p)),
            render(dataset.resolve(q.o)),
        );
        per_graph.entry(graph_key).or_default().push(line);
    }

    let mut hashes: Vec<String> = per_graph
        .into_values()
        .map(|mut lines| {
            lines.sort_unstable();
            digest(lines.join("\n").as_bytes())
        })
        .collect();
    hashes.sort_unstable();
    hashes
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
        let mut provenance = ResultProvenance::native("contract:abc", "world:default");
        // A named certified fragment: a conclusive, violation-free check is only
        // entitled to a CoherenceCertificate when it names the fragment it ranges over.
        provenance.certified_fragment = Some("fragment:test".to_owned());
        ReasoningResult {
            input: InputStatus::Valid,
            evaluation,
            completeness,
            preservation: PreservationClaim::exact(),
            information: InformationState::Supported,
            provenance,
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
            BTreeSet::new(),
        )
        .unwrap();
        assert!(outcome.issues_certificate());
        assert_eq!(outcome.class_local_name(), Some("CoherenceCertificate"));
    }

    #[test]
    fn conclusive_consistent_without_fragment_downgrades_to_attestation() {
        // A conclusive, violation-free check that names NO certified fragment cannot
        // certify (no F to range over): it DOWNGRADES to an attestation, never a
        // fragment-less certificate.
        let mut result = consistent_result(
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
        );
        result.provenance.certified_fragment = None;
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
            BTreeSet::new(),
        )
        .unwrap();
        assert!(
            !outcome.issues_certificate(),
            "no fragment ⇒ no certificate"
        );
        assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
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
            BTreeSet::new(),
        )
        .unwrap();
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
            BTreeSet::new(),
        )
        .unwrap();
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
            BTreeSet::new(),
        )
        .unwrap();
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
            BTreeSet::new(),
        )
        .unwrap();
        assert!(outcome.is_refused());
        assert!(!outcome.issues_certificate());
        assert_eq!(outcome.class_local_name(), None);
        assert_eq!(outcome.payload().forbidden_violations.len(), 1);
        // A refusal emits no coherence artifact.
        assert!(outcome.to_nquads("https://example.org/g").is_empty());
    }

    #[test]
    fn bounded_forbidden_glut_attests_and_discloses_the_forbidden_witness() {
        // A NON-conclusive (bounded/incomplete) check that ran into a glut under a
        // glut-FORBIDDING contract cannot refute coherence wholesale, but must
        // DISCLOSE what it found: an attestation carrying logic:forbiddenViolationWitness
        // (the property's producer — without this path the minted property is orphaned).
        let mut result = glut_result();
        result.evaluation = EvaluationStatus::BudgetExhausted;
        result.completeness = CompletenessStatus::Incomplete;
        assert!(!result.is_conclusive());
        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut, // forbids a glut
            "2026-06-28T00:00:00Z",
            BTreeSet::new(),
        )
        .unwrap();
        assert!(matches!(outcome, CoherenceOutcome::Attestation(_)));
        assert!(!outcome.issues_certificate());
        assert_eq!(outcome.payload().forbidden_violations.len(), 1);
        let nquads = outcome.to_nquads("https://example.org/g");
        assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/forbiddenViolationWitness> <https://example.org/clash>"
        ));
    }

    #[test]
    fn nquads_are_well_formed_and_byte_stable() {
        let outcome = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            ["blake3:axioms"],
            ContradictionPolicy::ForbidGap,
            "2026-06-28T00:00:00Z",
            BTreeSet::new(),
        )
        .unwrap();
        let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
        let nquads = outcome.to_nquads(graph);
        // Re-running with the same inputs is byte-identical (determinism).
        assert_eq!(nquads, outcome.to_nquads(graph));
        // Typed as a certificate, carries the payload, lands in the named graph.
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/CoherenceCertificate>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/bundleHash>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/contradictionPolicy> <https://blackcatinformatics.ca/logic/ForbidGap>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/permittedConflictWitness> <https://example.org/clash>"));
        // The certificate links to the logic:ReasoningResult it summarizes (M2).
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/summarizesResult>"));
        assert!(nquads.contains("<https://blackcatinformatics.ca/logic/ReasoningResult>"));
        // The two completeness-gate axes ride the result node as status individuals, so
        // the full payload (including the axes that decide certificate-vs-attestation) is
        // faithfully recoverable from the carried quads.
        assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/resultEvaluation> <https://blackcatinformatics.ca/logic/EvaluationCompleted>"
        ));
        assert!(nquads.contains(
            "<https://blackcatinformatics.ca/logic/resultCompleteness> <https://blackcatinformatics.ca/logic/CompleteForFragment>"
        ));
        for line in nquads.lines() {
            assert!(
                line.ends_with(&format!("<{graph}> .")),
                "line not in graph: {line}"
            );
        }
    }

    /// Two payloads differing ONLY in their axiom_hashes must produce different
    /// content_ids — the blake3 digest covers the full payload so the former
    /// FNV-1a collision (same class/bundle/contract/timestamp, different axioms)
    /// cannot recur.
    #[test]
    fn content_id_discriminates_on_axiom_hashes() {
        let outcome_a = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            ["blake3:axioms-set-A"],
            ContradictionPolicy::ForbidGap,
            "2026-06-28T00:00:00Z",
            BTreeSet::new(),
        )
        .unwrap();
        let outcome_b = CoherenceOutcome::from_reasoning_result(
            &glut_result(),
            "blake3:bundle",
            ["blake3:axioms-set-B"],
            ContradictionPolicy::ForbidGap,
            "2026-06-28T00:00:00Z",
            BTreeSet::new(),
        )
        .unwrap();
        let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
        let nquads_a = outcome_a.to_nquads(graph);
        let nquads_b = outcome_b.to_nquads(graph);
        // The subject IRIs must differ (different axiom hashes → different content_id).
        assert_ne!(
            nquads_a, nquads_b,
            "payloads differing only in axiom_hashes must produce different content_ids"
        );
        // Both still carry their respective axiom hashes.
        assert!(nquads_a.contains("\"blake3:axioms-set-A\""));
        assert!(nquads_b.contains("\"blake3:axioms-set-B\""));
    }

    /// The `projection_losses` payload field comes from the CALLER-SUPPLIED loss
    /// codes (genuine ledger codes from `pair_loss_ledger`), NOT from the DL
    /// reasoner's `unsupported_constructs`. The two must never be conflated.
    #[test]
    fn projection_losses_sourced_from_ledger_codes_not_unsupported_constructs() {
        let mut result = consistent_result(
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
        );
        // Inject a fake DL construct into preservation.unsupported_constructs.
        result
            .preservation
            .unsupported_constructs
            .insert("owl:someSpecialConstruct".to_owned());

        // The caller provides genuine ledger codes (what pair_loss_ledger returns).
        let ledger_codes: BTreeSet<String> = [
            "named-graph-dropped".to_owned(),
            "owl-dl-projection".to_owned(),
        ]
        .into_iter()
        .collect();

        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
            ledger_codes.clone(),
        )
        .unwrap();

        let payload = outcome.payload();

        // projection_losses must be exactly the caller-supplied ledger codes.
        assert_eq!(
            payload.projection_losses, ledger_codes,
            "projection_losses must equal the supplied ledger codes"
        );
        // projection_losses must NOT contain the DL unsupported construct.
        assert!(
            !payload
                .projection_losses
                .contains("owl:someSpecialConstruct"),
            "projection_losses must not be sourced from unsupported_constructs"
        );
        // unsupported_constructs must still carry the DL construct.
        assert!(
            payload
                .unsupported_constructs
                .contains("owl:someSpecialConstruct"),
            "unsupported_constructs must preserve the DL reasoner constructs"
        );
        // The two fields must not overlap in this scenario.
        assert!(
            payload
                .projection_losses
                .is_disjoint(&payload.unsupported_constructs),
            "projection_losses and unsupported_constructs must be disjoint here"
        );
    }

    /// The N-Quads projection emits `logic:unsupportedConstruct` for DL constructs
    /// the reasoner could not decide, and `logic:projectionLoss` for ledger codes —
    /// the two properties are distinct and carry the right values.
    #[test]
    fn nquads_emits_unsupported_construct_and_projection_loss_as_separate_properties() {
        let mut result = consistent_result(
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
        );
        result
            .preservation
            .unsupported_constructs
            .insert("owl:NominalClass".to_owned());

        let ledger_codes: BTreeSet<String> =
            ["named-graph-dropped".to_owned()].into_iter().collect();

        let outcome = CoherenceOutcome::from_reasoning_result(
            &result,
            "blake3:bundle",
            Vec::<String>::new(),
            ContradictionPolicy::ForbidGapAndGlut,
            "2026-06-28T00:00:00Z",
            ledger_codes,
        )
        .unwrap();

        let graph = "https://blackcatinformatics.ca/gmeow/graph/attestations";
        let nquads = outcome.to_nquads(graph);

        // logic:projectionLoss carries the ledger code, not the DL construct.
        assert!(
            nquads.contains(
                "<https://blackcatinformatics.ca/logic/projectionLoss> \"named-graph-dropped\""
            ),
            "projectionLoss must contain the ledger code: {nquads}"
        );
        assert!(
            !nquads.contains(
                "<https://blackcatinformatics.ca/logic/projectionLoss> \"owl:NominalClass\""
            ),
            "projectionLoss must NOT contain the DL construct: {nquads}"
        );
        // logic:unsupportedConstruct carries the DL construct.
        assert!(
            nquads.contains(
                "<https://blackcatinformatics.ca/logic/unsupportedConstruct> \"owl:NominalClass\""
            ),
            "unsupportedConstruct must contain the DL construct: {nquads}"
        );
        assert!(
            !nquads.contains(
                "<https://blackcatinformatics.ca/logic/unsupportedConstruct> \"named-graph-dropped\""
            ),
            "unsupportedConstruct must NOT contain the ledger code: {nquads}"
        );
    }
}
