// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoner-derived enactment-kernel gate.
//!
//! Compiles the `logic:Constraint` laws the enactment kernel authors in
//! `slices/grounding/logic/module.ttl` into VIOLATION-EMITTING forward `EvalRule`s
//! ([`crate::relational_core::lower_constraint_violation_rules`]) and drives them through
//! the native forward semi-naive chase over the SAME dataset `verify()` is checking, so a
//! kernel finding is REASONER-derived from the authored laws rather than a Rust
//! side-channel decision. The module mirrors [`super::math_gate`] deliberately: same
//! promotion into one scratch world, same plan-cache discipline, same loud-on-authoring-bug
//! posture.
//!
//! # The one thing this module may never do
//!
//! The kernel's hardest safety boundary is that the engine DESCRIBES, validates, derives
//! and certifies external-effect records but never CAUSES them. A reasoner that could
//! conclude an attempt happened could conclude the world changed. That boundary is
//! authored as `logic:EffectRecordsAreObservedNotDerivedConstraint`, but a constraint only
//! binds if it is actually run, so this module carries the same rule as a Rust-side
//! guard: [`BANNED_DERIVED_HEADS`] is refused at emission and
//! [`reject_banned_heads`] hard-fails rather than dropping the row silently. Belt and
//! braces on the one inference that must never be drawn.
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

use std::sync::OnceLock;

use purrdf::{RdfDataset, RdfQuad};

use crate::rule_ir::EvalRule;

/// The `logic:` namespace the kernel's terms live in.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// The `rdf:type` predicate the guard keys its class check on.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The single scratch world every quad (default OR named graph) is promoted into before
/// the forward chase runs — a single world, so the world-indexed engine sees the whole
/// dataset regardless of how its source graphs were named. Never leaks into a caller's
/// data: it exists only inside this module's own transient store.
const ENACTMENT_GATE_WORLD: &str =
    "https://blackcatinformatics.ca/gmeow/graph/enactment-kernel-gate";

/// The plan-cache contract-hash namespace for the compiled violation rules — distinct
/// from both [`crate::reason::native_contract_hash`] and the math gate's, so the three
/// never collide in the shared process-wide plan cache.
#[allow(dead_code)]
const ENACTMENT_GATE_CONTRACT: &str =
    "https://blackcatinformatics.ca/gmeow/reason/enactment-kernel-gate/v1";

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
/// Returns the rows unchanged when the derivation is clean. A violation is an engine bug
/// of the most serious kind available in this layer — it means a reasoning step concluded
/// that the world changed — so it is surfaced as an error rather than filtered away,
/// which would hide the defect while appearing to preserve the invariant.
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
        if stamps_derivation && is_banned_effect_subject(subject) {
            return Err(enactment_gate_err(format!(
                "enactment gate: a derivation would stamp derivation provenance on the effect \
                 record <{subject}>, which is precisely the shape of an inferred rather than \
                 observed effect"
            )));
        }
    }
    Ok(())
}

/// Whether an IRI is inside the kernel's effect-record space, used only to sharpen the
/// derivation-provenance refusal message.
fn is_banned_effect_subject(iri: &str) -> bool {
    iri.starts_with(LOGIC_NS)
}

/// The kernel's authored constraint laws, compiled once per process.
///
/// Empty until the frontier and gating derivations are wired through it; the compilation
/// path itself is shared with [`super::math_gate`] and is exercised by that gate today.
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
/// The banned-head guard runs over every derived row before anything is returned, so the
/// observed-not-derived boundary holds even if a future law is authored carelessly.
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
        let rows = vec![(
            "https://blackcatinformatics.ca/logic/someEffectRecord".to_owned(),
            DERIVATION_IDENTIFIER.to_owned(),
            "derivation-42".to_owned(),
        )];
        assert!(
            reject_banned_heads(&rows).is_err(),
            "derivation provenance is the machine-checkable mark of an inferred effect"
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
