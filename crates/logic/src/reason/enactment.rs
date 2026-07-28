// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The enactment-kernel gate.
//!
//! # What is implemented here today
//!
//! The observed-not-derived guard, and nothing else. [`reject_banned_heads`] is live and
//! is enforced on the real `verify()` path over the REASONED CLOSURE — the derived
//! (non-EDB) edges `verify()` layers onto the asserted graph — not over this module's own
//! output. That distinction is the whole point of the guard: the closure is where a real
//! derivation of an effect record would appear, and it is populated on every run.
//! [`enactment_gate_markers`] is a seam: it compiles no laws and derives no markers yet,
//! so it returns an empty marker set, and `verify()` runs the guard over that empty set as
//! a SECOND, currently-vacuous call that becomes meaningful the moment laws are wired in.
//!
//! The frontier derivation, the dispatch/reconciliation/compensation classifications and
//! the typed-outcome fold are NOT implemented. When they are, they follow
//! [`super::math_gate`]: compile the authored `logic:Constraint` laws into
//! violation-emitting forward `EvalRule`s
//! ([`crate::relational_core::lower_constraint_violation_rules`]) and drive them through
//! the native forward semi-naive chase over the SAME dataset `verify()` checks, so a
//! kernel finding is reasoner-derived from the authored laws rather than a Rust
//! side-channel decision.
//!
//! Saying this plainly matters more than usual here: a gate that returns an empty finding
//! set is indistinguishable, from the caller's side, from a gate that ran and found
//! nothing.
//!
//! # The one thing this module may never do
//!
//! The kernel's hardest safety boundary is that the engine DESCRIBES, validates and
//! certifies external-effect records but never DERIVES or CAUSES them. A reasoner that
//! could conclude an attempt happened could conclude the world changed. That boundary is
//! authored as `logic:EffectRecordsAreObservedNotDerivedConstraint`, but a constraint only
//! binds if it is actually run, so this module carries the same rule as a Rust-side guard:
//! [`reject_banned_heads`] refuses any row typing its subject as one of
//! [`BANNED_DERIVED_HEADS`] and hard-fails rather than dropping the row silently.
//!
//! A guard only guards what it is given, so where it is called matters as much as what it
//! checks. `verify()` runs it over the DERIVED (non-EDB) edges of the reasoned closure,
//! unconditionally and before any gate marker work — the closure is the one place a real
//! derivation of an effect record can surface, and it is non-empty on every run. Running
//! it only over this module's own (currently empty) marker output would be a guard with no
//! input: indistinguishable, from a passing test's side, from a guard that ran.
//!
//! # Determinism contract (golden-pinned, identical to [`crate::teleology`])
//!
//! 1. **Insertion-order enumeration** — every join / scan walks the world's quads in the
//!    deterministic, content-sorted order the store produces.
//! 2. **First-wins dedup** — a derived quad whose `(s, p, o, g)` key already exists is
//!    dropped, keeping the first record's provenance.
//! 3. **Provenance** — reifiers via `mint_reifier` and derivation IRIs via
//!    `mint_derivation_id`, reused verbatim from [`crate::provenance`]; no new provenance
//!    scheme is invented.
//! 4. **Canonical row sort** — the emitted quads are sorted by `(graph, subject,
//!    predicate, object)` before return.
//!
//! # No-optionality
//!
//! A malformed law (a constraint with no `logic:integrity` formula, a lowering that
//! hard-fails on an arity mismatch) is an authoring bug in the shipped module, never a
//! runtime condition a caller could recover from — hence the loud failure, exactly as
//! [`super::math_gate`] does for its own embedded asset.

pub(crate) mod search;

use std::sync::OnceLock;

use purrdf::{RdfDataset, RdfQuad};

use crate::rule_ir::EvalRule;

/// The `rdf:type` predicate the guard keys its class check on.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Predicates whose SUBJECT the engine may never derive.
///
/// An effect attempt and an external effect receipt are records of what happened in the
/// world. They are asserted by the organ that performed the dispatch and observed the
/// outcome; deriving one would mean the reasoner concluded that an external effect
/// occurred. This is the inference the whole commitment layer exists to forbid, so it is
/// refused here as well as constrained in the module.
pub(crate) const BANNED_DERIVED_HEADS: [&str; 2] = [
    "https://blackcatinformatics.ca/logic/EffectAttempt",
    "https://blackcatinformatics.ca/logic/ExternalEffectReceipt",
];

/// The derivation-provenance predicate whose presence on an effect record is exactly what
/// distinguishes a record the engine produced from one the world did.
pub(crate) const DERIVATION_IDENTIFIER: &str =
    "https://blackcatinformatics.ca/logic/derivationIdentifier";

/// True when `class_iri` names a record kind the engine may never derive.
#[must_use]
pub(crate) fn is_banned_derived_head(class_iri: &str) -> bool {
    BANNED_DERIVED_HEADS.contains(&class_iri)
}

/// Hard-fail if any derived row would introduce a banned effect record.
///
/// `rows` are `(subject, predicate, object)` triples that the ENGINE PRODUCED — on the
/// production path, the derived (non-EDB) edges of the reasoned closure `verify()` layers
/// onto the asserted graph. Asserted effect records are legitimate and must never be
/// handed to this function: an `logic:EffectAttempt` the dispatching organ wrote down is
/// exactly the observation the kernel exists to reason ABOUT.
///
/// Two shapes are refused:
///
/// 1. **A banned `rdf:type` head** — a row typing its subject as a member of
///    [`BANNED_DERIVED_HEADS`]. This is the direct form of the forbidden inference.
/// 2. **Derivation provenance on an effect record** — a [`DERIVATION_IDENTIFIER`] row
///    whose subject `rows` also types as a banned effect record (see
///    [`is_banned_effect_subject`]). This is the indirect form: the engine stamping its
///    own derivation identity onto a record that is only ever supposed to be observed.
///
/// Returns `Ok(())` when the derivation is clean; the rows are never mutated or filtered.
/// A violation is an engine bug of the most serious kind available in this layer — it
/// means a reasoning step concluded that the world changed — so it is surfaced as an error
/// rather than filtered away, which would hide the defect while appearing to preserve the
/// invariant.
///
/// # Errors
///
/// Returns `Err` when a derived quad types its subject as a banned effect record, or
/// carries the derivation-provenance predicate on such a record.
pub(crate) fn reject_banned_heads(rows: &[(String, String, String)]) -> gmeow_errors::Result<()> {
    for (subject, predicate, object) in rows {
        let types_banned = predicate == RDF_TYPE && is_banned_derived_head(object);
        let stamps_derivation = predicate == DERIVATION_IDENTIFIER;
        if types_banned {
            return Err(enactment_gate_err(format!(
                "enactment gate: a derivation would type <{subject}> as <{object}>, but effect \
                 attempts and receipts are OBSERVED, never derived — a reasoner that can \
                 conclude an attempt happened can conclude the world changed"
            )));
        }
        if stamps_derivation && is_banned_effect_subject(rows, subject) {
            return Err(enactment_gate_err(format!(
                "enactment gate: a derivation would stamp derivation provenance on the effect \
                 record <{subject}>, which is precisely the shape of an inferred rather than \
                 observed effect"
            )));
        }
    }
    Ok(())
}

/// Whether `rows` type `subject_iri` as one of the [`BANNED_DERIVED_HEADS`].
///
/// Effect-record identity is decided by TYPING, never by the shape of the IRI. The kernel
/// publishes a great many `logic:`-namespaced terms the engine is REQUIRED to derive —
/// `logic:ActionableFrontier` and `logic:FrontierEntry` are the kernel's headline
/// capability, not a violation — so a namespace-prefix test would misfire on precisely the
/// derivations this module exists to produce. The only honest question is whether the
/// row-set at hand says the subject IS an effect attempt or an external effect receipt.
///
/// Scoped deliberately to `rows`: the caller hands over what the engine derived, so a
/// subject typed as an effect record inside that set was typed there BY THE ENGINE. A
/// record typed in the asserted graph is an observation, and stamping it is a question for
/// the authored `logic:EffectRecordsAreObservedNotDerivedConstraint` over the full
/// dataset, not for a guard whose entire input is the derivation.
fn is_banned_effect_subject(rows: &[(String, String, String)], subject_iri: &str) -> bool {
    rows.iter().any(|(subject, predicate, object)| {
        subject == subject_iri && predicate == RDF_TYPE && is_banned_derived_head(object)
    })
}

/// The kernel's authored constraint laws, compiled once per process.
///
/// EMPTY TODAY. No law is compiled yet, so the gate derives nothing. The compilation path
/// this will use is shared with [`super::math_gate`] and is exercised by that gate.
fn compiled_rules() -> &'static [EvalRule] {
    static RULES: OnceLock<Vec<EvalRule>> = OnceLock::new();
    RULES.get_or_init(Vec::new)
}

/// Build a kernel-gate diagnostic.
fn enactment_gate_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The enactment-kernel gate entry point.
///
/// Returns the reasoner-derived kernel findings over the dataset `verify()` is checking.
///
/// Returns an EMPTY set today: no law is compiled yet (see [`compiled_rules`]), so this
/// derives nothing and both parameters go unread. The caller runs the banned-head guard
/// over what comes back, which is therefore currently a vacuous check that starts biting
/// the moment laws are wired in; the boundary is held TODAY by the caller's separate,
/// unconditional [`reject_banned_heads`] pass over the reasoned closure itself.
///
/// # Errors
///
/// Returns `Err` when the compiled laws are not stratifiable, when the native chase
/// declines them, or when a derivation would produce a banned effect record.
pub(crate) fn enactment_gate_markers(
    _edb: &RdfDataset,
    _derived: &[RdfQuad],
) -> gmeow_errors::Result<Vec<(String, String)>> {
    let rules = compiled_rules();
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::{
        BANNED_DERIVED_HEADS, DERIVATION_IDENTIFIER, is_banned_derived_head, reject_banned_heads,
    };

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    #[test]
    fn both_effect_record_kinds_are_banned_heads() {
        assert_eq!(BANNED_DERIVED_HEADS.len(), 2);
        assert!(is_banned_derived_head(
            "https://blackcatinformatics.ca/logic/EffectAttempt"
        ));
        assert!(is_banned_derived_head(
            "https://blackcatinformatics.ca/logic/ExternalEffectReceipt"
        ));
    }

    #[test]
    fn a_kernel_class_that_is_not_an_effect_record_is_derivable() {
        // The guard must be narrow: the frontier and its labels are DERIVED by design,
        // and a guard that refused them would forbid the kernel's own headline capability.
        assert!(!is_banned_derived_head(
            "https://blackcatinformatics.ca/logic/ActionableFrontier"
        ));
        assert!(!is_banned_derived_head(
            "https://blackcatinformatics.ca/logic/FrontierEntry"
        ));
    }

    #[test]
    fn deriving_an_effect_attempt_is_refused() {
        let rows = vec![(
            "https://example.org/attempt-1".to_owned(),
            RDF_TYPE.to_owned(),
            "https://blackcatinformatics.ca/logic/EffectAttempt".to_owned(),
        )];
        let err = reject_banned_heads(&rows).expect_err("deriving an attempt must be refused");
        assert!(
            format!("{err:?}").contains("OBSERVED, never derived"),
            "the refusal must say WHY, not merely that it refused"
        );
    }

    #[test]
    fn deriving_an_external_effect_receipt_is_refused() {
        let rows = vec![(
            "https://example.org/receipt-1".to_owned(),
            RDF_TYPE.to_owned(),
            "https://blackcatinformatics.ca/logic/ExternalEffectReceipt".to_owned(),
        )];
        assert!(
            reject_banned_heads(&rows).is_err(),
            "deriving a receipt asserts an outcome nobody observed"
        );
    }

    #[test]
    fn stamping_derivation_provenance_on_a_kernel_effect_record_is_refused() {
        // The stamp row comes FIRST, so the guard reaches it before the typing row that
        // would independently condemn the same subject — proving the derivation-provenance
        // arm fires on its own terms and names the effect record in its message.
        let record = "https://example.org/attempt-7".to_owned();
        let rows = vec![
            (
                record.clone(),
                DERIVATION_IDENTIFIER.to_owned(),
                "derivation-42".to_owned(),
            ),
            (
                record.clone(),
                RDF_TYPE.to_owned(),
                "https://blackcatinformatics.ca/logic/EffectAttempt".to_owned(),
            ),
        ];
        let err = reject_banned_heads(&rows).expect_err(
            "derivation provenance is the machine-checkable mark of an inferred effect",
        );
        assert!(
            format!("{err:?}").contains("stamp derivation provenance"),
            "the derivation-provenance arm must be the one that fired, not the typing arm"
        );
    }

    /// Effect-record identity is decided by TYPING, not by IRI namespace.
    ///
    /// The retired predicate treated every `logic:`-namespaced subject as an effect
    /// record, which condemned derivation provenance on the kernel's own by-design
    /// derivations. A `logic:`-namespaced subject that nothing types as an effect attempt
    /// or receipt must carry derivation provenance freely — that IS what a derived
    /// frontier entry looks like.
    #[test]
    fn derivation_provenance_on_a_logic_subject_that_is_not_an_effect_record_passes() {
        let entry = "https://blackcatinformatics.ca/logic/frontierEntry-3".to_owned();
        let rows = vec![
            (
                entry.clone(),
                RDF_TYPE.to_owned(),
                "https://blackcatinformatics.ca/logic/FrontierEntry".to_owned(),
            ),
            (
                entry,
                DERIVATION_IDENTIFIER.to_owned(),
                "derivation-42".to_owned(),
            ),
        ];
        assert!(
            reject_banned_heads(&rows).is_ok(),
            "a derived frontier entry is the kernel's headline capability, not a violation"
        );
    }

    #[test]
    fn an_ordinary_derivation_passes_the_guard() {
        let rows = vec![(
            "https://example.org/frontier-1".to_owned(),
            RDF_TYPE.to_owned(),
            "https://blackcatinformatics.ca/logic/ActionableFrontier".to_owned(),
        )];
        assert!(
            reject_banned_heads(&rows).is_ok(),
            "the guard must not obstruct the derivations the kernel exists to produce"
        );
    }
}
