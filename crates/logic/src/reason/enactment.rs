// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The enactment-kernel gate.
//!
//! # What this gate does
//!
//! Two things, both live on the production `verify()` path.
//!
//! **First, it materializes the kernel's integrity findings from the authored laws.**
//! [`enactment_gate_markers`] compiles every enactment-kernel `logic:Constraint` in the
//! embedded `slices/grounding/logic/module.ttl` into a VIOLATION-EMITTING forward
//! `EvalRule` ([`crate::relational_core::lower_violation_rules`]) — the law's antecedent
//! as an ordinary positive body, its consequent NEGATED and joined on — and drives them
//! through the native forward semi-naive chase over the SAME reasoned closure `verify()`
//! is checking. A body that fully matches is therefore exactly a record that meets the
//! law's guard and breaks its obligation, and the rule's head types that record
//! `logic:EnactmentIntegrityViolation`. The marker is an ordinary `rdf:type` triple the
//! caller splices into the reasoned graph, so the `enactment-integrity-violation.rq`
//! verify query renders it like any other row: the finding is REASONER-DERIVED from the
//! authored law, never a Rust side-channel decision. Structurally this is
//! [`super::math_gate`] with the arithmetic-decided consequent replaced by a
//! negation-as-failure one, which is what the ordinary (non-builtin-bound) corpus needs.
//!
//! Not every authored constraint compiles, and the ones that do not are named rather than
//! quietly dropped: the lowering is a total function into `⟨ rules ⊕ flagged residue ⟩`
//! ([`gmeow_logic_compile::relational_core::RcViolationGap`]), and
//! [`compiled_law_report`] exposes both halves so a caller — or a test — can state the
//! compiled fraction and audit the shortfall. A gate reporting only the numerator is
//! reporting a number nobody can check.
//!
//! **Second, it holds the observed-not-derived boundary.** [`reject_banned_heads`] is
//! enforced on the real `verify()` path over the REASONED CLOSURE — the derived (non-EDB)
//! edges `verify()` layers onto the asserted graph — and again over this module's own
//! marker output, because a gate that materializes markers from authored laws is itself a
//! derivation and is held to the same boundary.
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
//! it ONLY over this module's marker output would be the weaker wiring: the markers are a
//! narrow, law-shaped derivation, and the closure is where an arbitrary one appears.
//!
//! # Determinism contract
//!
//! 1. **Insertion-order enumeration** — every join / scan walks the world's quads in the
//!    deterministic, content-sorted order the store produces.
//! 2. **Canonical marker sort** — the emitted `(subject, failure_class)` pairs are sorted
//!    and deduplicated before return, so a marker set is a function of the closure alone.
//! 3. **Chase-derived only** — a row whose `rule_iri` is [`crate::provenance::ASSERT_RULE_IRI`]
//!    is echoed EDB and is dropped: a violation marker is always DERIVED, and a caller's
//!    data asserting `a logic:EnactmentIntegrityViolation` by hand is not a finding this
//!    gate made.
//!
//! # No-optionality
//!
//! A malformed law (a constraint with no `logic:integrity` formula, a lowering that
//! hard-fails on an arity mismatch) is an authoring bug in the shipped module, never a
//! runtime condition a caller could recover from — hence the loud failure, exactly as
//! [`super::math_gate`] does for its own embedded asset.

pub(crate) mod search;

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

use crate::physical::{NativeOutcome, compile_cached, materialize_native};
use gmeow_logic_compile::relational_core::RcViolationGap;

use crate::relational_core::ViolationLowering;
use crate::rule_ir::{EvalRule, EvalTerm};
use crate::store::WorldStore;

/// The `rdf:type` predicate the guard keys its class check on.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// The `logic:` slice module, embedded at compile time (the same convention
/// `crates/logic/build.rs` uses for the verify query set, and the same one
/// [`super::math_gate`] uses for `math/module.ttl`): production `verify()` never reads
/// `slices/` off disk at runtime.
const LOGIC_MODULE_TTL: &str = include_str!("../../../../slices/grounding/logic/module.ttl");

/// The `logic:` slice module's canonical source IRI (provenance only).
const LOGIC_MODULE_SOURCE_IRI: &str = "https://blackcatinformatics.ca/gmeow/slices/logic";

/// The canonical scratch named graph every caller-supplied quad (default OR named graph)
/// is promoted into before the forward chase runs — a single world, so the world-indexed
/// engine (which only reasons over named-graph worlds) sees the whole dataset regardless
/// of how its source graphs were named. Never leaks into a caller's data: it exists only
/// inside this module's own transient [`WorldStore`].
const ENACTMENT_GATE_WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/enactment-gate";

/// The plan-cache contract-hash namespace for the compiled violation rules — distinct from
/// [`crate::reason::native_contract_hash`]'s DL/EL/RL contract and from the math gate's, so
/// none of the three collide in the shared process-wide plan cache.
const ENACTMENT_GATE_CONTRACT: &str =
    "https://blackcatinformatics.ca/gmeow/reason/enactment-gate/v1";

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

/// The kernel's authored constraint laws, compiled once per process from the embedded
/// `logic/module.ttl` and cached for every subsequent `verify()` call — together with the
/// flagged residue naming every constraint that did NOT compile.
///
/// The embedded module is a fixed, always-valid compile-time asset — its law census is
/// pinned by this module's own `tests::every_enactment_kernel_law_compiles_into_a_violation_rule`
/// and its behaviour on the production surface by
/// [`enactment_gate.rs`](../../../tests/enactment_gate.rs) — so a build failure here is a
/// genuine authoring/build bug, not a runtime condition a caller could recover from. Hence
/// the loud panic, exactly as [`super::math_gate`] does for its own embedded asset and
/// `crates/logic/build.rs` does for a malformed embedded query.
fn compiled_law_report() -> &'static ViolationLowering {
    static LOWERING: OnceLock<ViolationLowering> = OnceLock::new();
    LOWERING.get_or_init(|| {
        build_rules().unwrap_or_else(|e| {
            panic!(
                "enactment gate: failed to compile the embedded logic/module.ttl \
                 logic:Constraint laws into violation rules: {e}"
            )
        })
    })
}

/// The compiled violation rules alone — the hot path's view of [`compiled_law_report`].
fn compiled_rules() -> &'static [EvalRule] {
    &compiled_law_report().rules
}

/// Parse the embedded `logic/module.ttl`, compile it into a [`LogicProgram`], and lower
/// every ordinary `logic:Constraint` whose consequent is a stored-relation obligation or
/// prohibition into violation `EvalRule`s.
///
/// No `rdfs:seeAlso` reflection-substitution map is built, deliberately. [`super::math_gate`]
/// needs one because the `math:` dimension laws predicate over HiLog REFLECTION relations
/// (`math:hasDimensionRel`, `math:homogeneousOperandRel`, …) that no data ever asserts, so
/// its lowered bodies must be bridged to the object-level properties real triples carry.
/// The kernel's laws do not: their authored `logic:Formula` ASTs name the object-level
/// properties directly — `rdf:type`, `logic:receiptOfAttempt`, `logic:fencingIdentity`,
/// `logic:journalPrevHead`, `logic:derivationIdentifier` — which is exactly the predicate
/// the asserted data uses. Threading a substitution map through here would not be
/// harmlessly redundant: `rdfs:seeAlso` is used across `logic/module.ttl` for ordinary
/// documentary cross-references, so a blanket map would silently REWRITE an authored law's
/// predicate to a relation the law never mentioned, pointing the body at data the law does
/// not govern.
///
/// # Errors
///
/// Returns `Err` if the embedded Turtle fails to parse, if the `logic:` frontend cannot
/// compile it into a [`LogicProgram`], or if a lowered body atom carries a term with no
/// engine form — an authoring bug in the shipped module, never silently swallowed.
///
/// [`LogicProgram`]: gmeow_logic_compile::ir::LogicProgram
fn build_rules() -> gmeow_errors::Result<ViolationLowering> {
    let source = purrdf::parse_dataset(LOGIC_MODULE_TTL.as_bytes(), "text/turtle", None)
        .map_err(|e| enactment_gate_err(format!("parse the embedded logic/module.ttl: {e}")))?;
    let (program, _diagnostics) = gmeow_logic_compile::frontend::parse_logic_dataset(
        source.as_ref(),
        Some(LOGIC_MODULE_SOURCE_IRI.to_owned()),
    )
    .map_err(|e| {
        enactment_gate_err(format!(
            "compile the embedded logic/module.ttl into a LogicProgram: {e}"
        ))
    })?;
    let lowering = crate::relational_core::lower_violation_rules(&program)?;
    reject_unenforced_laws(&lowering)?;
    Ok(lowering)
}

/// Hard-fail when an authored law ASKED to be enforced as a marker and the lowering could
/// not deliver it.
///
/// A `logic:Constraint` carrying `gmeow:enforcesFailureClass` is an author's statement that
/// its violations are to surface as typed, queryable objects. If such a law then falls
/// outside the Horn+NAF fragment, it is enforced by nothing: not by this gate, and not by
/// the derived-SHACL surface either, since a marker-bearing law is exactly the procedural
/// kind that surface cannot decide. Shipping it would be the failure this whole module was
/// rebuilt to end — a law that reads as enforced and is not — so it is refused loudly at the
/// first `verify()` call rather than passing silently on every one.
///
/// The two legitimate declines are excluded by construction:
///
/// * [`RcViolationGap::NoFailureClass`] — the constraint never asked; its enforcement
///   surface is the derived SHACL that `make validate` runs.
/// * [`RcViolationGap::BuiltinBoundConsequent`] — the constraint asked, and
///   [`super::math_gate`] answers, by exact-rational arithmetic rather than by NAF.
///
/// # Errors
///
/// Returns `Err` naming every law that asked for marker enforcement and did not get it.
fn reject_unenforced_laws(lowering: &ViolationLowering) -> gmeow_errors::Result<()> {
    let unenforced: Vec<String> = lowering
        .residue
        .iter()
        .filter(|r| {
            !matches!(
                r.gap,
                RcViolationGap::NoFailureClass | RcViolationGap::BuiltinBoundConsequent
            )
        })
        .map(|r| format!("<{}> ({})", r.constraint_iri, r.gap.as_str()))
        .collect();
    if unenforced.is_empty() {
        return Ok(());
    }
    Err(enactment_gate_err(format!(
        "enactment gate: {} authored logic:Constraint(s) carry a gmeow:enforcesFailureClass \
         but did not lower into a violation rule, so nothing enforces them — neither this \
         gate nor the derived-SHACL surface, which cannot decide a procedural law: {}",
        unenforced.len(),
        unenforced.join(", ")
    )))
}

/// Build a kernel-gate diagnostic.
fn enactment_gate_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The EDB projection the compiled laws read: every predicate named by a rule body, split
/// out from the `rdf:type` CLASS constants those bodies test against.
///
/// Derived from the compiled `rules` rather than hardcoded, so a newly-authored kernel law
/// over a new property is projected correctly without touching this file.
///
/// The `rdf:type` split matters for more than tidiness. Nearly every kernel law's guard is
/// an `rdf:type` test against ONE named class, and `rdf:type` is the largest relation in a
/// whole-bundle graph by a wide margin; promoting all of it would turn a bounded chase into
/// a pathological one for no gain, since a type triple naming a class no law mentions can
/// never match a body atom.
///
/// Both halves are load-bearing for CORRECTNESS, not only cost: a law's negated literal is
/// decided by negation-as-failure over the promoted world, so dropping the negated
/// predicate's rows would make every record look like it was missing that property and
/// fabricate violations wholesale. Every body predicate is kept, negated ones included.
fn gate_read_predicates(rules: &[EvalRule]) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut preds: BTreeSet<String> = BTreeSet::new();
    let mut type_objects: BTreeSet<String> = BTreeSet::new();
    for rule in rules {
        for atom in &rule.body {
            // An `rdf:type` atom against a NAMED class narrows the projection to that
            // class; one against a variable object does not, so it falls through and keeps
            // `rdf:type` wholesale — correctness before cost.
            if atom.predicate == RDF_TYPE
                && let EvalTerm::ConstNamed(class) = &atom.object
            {
                type_objects.insert(class.clone());
                continue;
            }
            preds.insert(atom.predicate.clone());
        }
    }
    (preds, type_objects)
}

/// Promote the kernel-relevant quads of the reasoned closure — every default-graph or
/// named-graph quad of `edb` PLUS the DL-`derived` non-EDB edges the reasoner layered onto
/// the reasoned graph — into the single canonical [`ENACTMENT_GATE_WORLD`], preserving
/// every term (literals included).
///
/// Both sources are filtered by the same projection and re-graphed identically, so a kernel
/// triple is gated whether it was asserted or derived — matching the verify queries that
/// evaluate the same closure. Taking the derived edges is what keeps a record whose kernel
/// TYPE is reached by subclass inference inside the gate's domain: a `logic:ResourceLease`
/// the closure concluded is as much a lease as one the data typed outright.
///
/// # Errors
///
/// Returns `Err` if the promoted dataset fails the freeze-time structural contract.
fn promote_to_single_world(
    edb: &RdfDataset,
    derived: &[RdfQuad],
    keep_predicates: &BTreeSet<String>,
    keep_type_objects: &BTreeSet<String>,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    // `rdf:type` is kept only for the classes some law names — UNLESS a law tests
    // `rdf:type` against a variable object, in which case `gate_read_predicates` puts
    // `rdf:type` itself into `keep_predicates` and the whole relation is promoted. The
    // narrow case must never silently swallow the wide one: a law reading types
    // open-endedly and receiving only a handful of them would decide on a truncated world.
    let keep = |predicate: &str, object: &RdfTerm| -> bool {
        if predicate == RDF_TYPE {
            return keep_predicates.contains(RDF_TYPE)
                || matches!(object, RdfTerm::Iri(o) if keep_type_objects.contains(o.as_str()));
        }
        keep_predicates.contains(predicate)
    };
    let mut builder = RdfDatasetBuilder::new();
    for quad in edb.owned_quads() {
        if !keep(&quad.predicate, &quad.object) {
            continue;
        }
        let promoted = RdfQuad::new(quad.subject, quad.predicate, quad.object)
            .in_graph(RdfTerm::iri(ENACTMENT_GATE_WORLD));
        builder.push_owned_quad(&promoted);
    }
    for quad in derived {
        if !keep(&quad.predicate, &quad.object) {
            continue;
        }
        let promoted = RdfQuad::new(
            quad.subject.clone(),
            quad.predicate.clone(),
            quad.object.clone(),
        )
        .in_graph(RdfTerm::iri(ENACTMENT_GATE_WORLD));
        builder.push_owned_quad(&promoted);
    }
    builder.freeze().map_err(|e| {
        enactment_gate_err(format!(
            "promote the reasoned closure into the enactment-gate scratch world: {e}"
        ))
    })
}

/// Decode a materialized row's subject to a bare IRI.
///
/// A violation marker's subject is the enactment record the law condemned, and the kernel's
/// records are identified individuals — a dispatch intent, a receipt, a lease all have
/// identity criteria (`design/LOGIC-ENACTMENT.md`), so an IRI is what the data carries. A
/// literal in that position means the chase bound the focus variable to something that
/// cannot be an enactment record, which is an engine-invariant failure and is surfaced as
/// one rather than dropped.
fn subject_iri(term: &TermValue) -> gmeow_errors::Result<String> {
    match term {
        TermValue::Iri(iri) => Ok(iri.clone()),
        other => Err(enactment_gate_err(format!(
            "enactment gate: a violation marker's subject must be an IRI, got {other:?}"
        ))),
    }
}

/// The enactment-kernel gate entry point.
///
/// Runs the compiled kernel laws over the reasoned closure (`edb` UNIONED with the
/// DL-`derived` non-EDB edges the caller layered onto the reasoned graph) and returns every
/// materialized `(subject, failure_class)` marker pair — deduplicated, sorted — ready to be
/// inserted as `subject rdf:type failure_class` quads.
///
/// Returns an empty vector, ordinarily, when the closure violates no kernel law. It is NOT
/// a way of saying "nothing ran": the compiled law set is non-empty by construction (a
/// compile failure panics in [`compiled_law_report`], and the shipped module's law count is
/// pinned by `crates/logic/tests/enactment_gate.rs`), so an empty result means the laws ran
/// and found the closure clean.
///
/// # Errors
///
/// Returns `Err` when the promoted closure fails to freeze, when the compiled laws are not
/// stratifiable, when the native forward chase declines the program, or when a materialized
/// marker's subject is not an IRI — every case a genuine internal-invariant failure, never
/// a silent empty result standing in for an error.
pub(crate) fn enactment_gate_markers(
    edb: &RdfDataset,
    derived: &[RdfQuad],
) -> gmeow_errors::Result<Vec<(String, String)>> {
    let rules = compiled_rules();
    let (keep_predicates, keep_type_objects) = gate_read_predicates(rules);
    let promoted = promote_to_single_world(edb, derived, &keep_predicates, &keep_type_objects)?;
    let store = WorldStore::from_dataset(promoted.as_ref())?;

    let lookup = compile_cached(ENACTMENT_GATE_CONTRACT, rules.to_vec());
    let Some(executable) = lookup.executable else {
        return Err(enactment_gate_err(
            "enactment gate: the compiled violation rules are not stratifiable — a kernel \
             law's negated consequent literal shares a predicate with a violation head, so \
             no finite stratification exists"
                .to_owned(),
        ));
    };

    let outcome = materialize_native(&store, executable.as_ref(), None)?;
    let budgeted = match outcome {
        NativeOutcome::Decided(budgeted) => budgeted,
        NativeOutcome::Unsupported(kind) => {
            return Err(enactment_gate_err(format!(
                "enactment gate: the native forward chase declined the compiled violation \
                 rules ({kind:?})"
            )));
        }
    };

    let mut markers: Vec<(String, String)> = Vec::new();
    for row in budgeted.rows {
        // Drop the echoed-EDB rows: a violation marker is always chase-DERIVED, and a
        // caller's data typing a record `a logic:EnactmentIntegrityViolation` by hand is an
        // assertion about the world, not a finding this gate made.
        if row.rule_iri == crate::provenance::ASSERT_RULE_IRI {
            continue;
        }
        if row.predicate != RDF_TYPE {
            continue;
        }
        let TermValue::Iri(class) = &row.object else {
            continue;
        };
        markers.push((subject_iri(&row.subject)?, class.clone()));
    }
    markers.sort();
    markers.dedup();
    Ok(markers)
}

#[cfg(test)]
mod tests {
    use super::{
        BANNED_DERIVED_HEADS, DERIVATION_IDENTIFIER, RcViolationGap, compile_cached,
        compiled_law_report, is_banned_derived_head, reject_banned_heads,
    };
    use std::collections::BTreeSet;

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    /// The 23 enactment-kernel laws the gate MUST compile, by local name.
    ///
    /// Spelled out rather than counted, because the number alone would stay green if one
    /// law silently dropped out of the fragment and an unrelated one was added. This is the
    /// census the module doc's claim rests on: every enactment law authored in
    /// `slices/grounding/logic/module.ttl` is a law the chase actually runs.
    const ENACTMENT_LAWS: [&str; 23] = [
        "AdvisoryNeverAuthorityConstraint",
        "ApprovalCommitmentCompletenessConstraint",
        "CapabilityGapProposalCompletenessConstraint",
        "CheckpointRestoreIdentityConstraint",
        "ClockAttributionRequiredConstraint",
        "CompensationBindsExactForwardEffectConstraint",
        "CompensationNotInverseConstraint",
        "CompensationSuccessRequiresReceiptConstraint",
        "ContextAssemblyRecordsExclusionsConstraint",
        "ContinuationKindDisjointnessConstraint",
        "DispatchIntentCompletenessConstraint",
        "EffectRecordsAreObservedNotDerivedConstraint",
        "EnactmentPinsPrescriptionAndSnapshotConstraint",
        "FrontierClosureRequiresSaturationConstraint",
        "IdempotencyContractCompletenessConstraint",
        "JournalChainIntegrityConstraint",
        "LeaseExclusivityConstraint",
        "NoBlindRetryConstraint",
        "OperationalGapCarriesProposalConstraint",
        "PrescriptionVersionImmutabilityConstraint",
        "ReceiptRequiresAttemptConstraint",
        "ReconciliationResultCarriesVerdictConstraint",
        "RefinementCandidateSetClosureConstraint",
    ];

    /// The IRIs of the constraints that actually lowered into violation rules.
    fn compiled_constraints() -> BTreeSet<String> {
        compiled_law_report()
            .rules
            .iter()
            .filter_map(|rule| rule.constraint_tag.clone())
            .collect()
    }

    /// Every enactment-kernel law compiles into at least one violation rule.
    ///
    /// The census that makes the module doc auditable. A law that stopped compiling — a
    /// consequent rewritten into a shape outside the Horn+NAF fragment, an accidentally
    /// dropped `gmeow:enforcesFailureClass` — would leave the gate quietly enforcing less
    /// than it says, which is the exact failure mode this whole module was rebuilt to end.
    #[test]
    fn every_enactment_kernel_law_compiles_into_a_violation_rule() {
        let compiled = compiled_constraints();
        let missing: Vec<&str> = ENACTMENT_LAWS
            .iter()
            .copied()
            .filter(|name| {
                !compiled
                    .iter()
                    .any(|iri| iri.ends_with(&format!("/{name}")))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "these authored enactment-kernel laws did not lower into violation rules, so the \
             gate does not enforce them: {missing:?}"
        );
    }

    /// Every declined constraint is declined for want of a FAILURE CLASS, never for want
    /// of expressiveness.
    ///
    /// The residue today is entirely `no-enforces-failure-class`: the `gmeow:` advice /
    /// term-completeness constraints and the `logic:` IR-shape constraints name no failure
    /// class, because their enforcement surface is the derived SHACL that `make validate`
    /// runs, not this gate. That is the one legitimate reason to decline (alongside a
    /// builtin-bound consequent, which [`super::super::math_gate`] owns and which this
    /// module's `logic/module.ttl` authors none of).
    ///
    /// [`super::build_rules`] already refuses to produce a report containing any other gap,
    /// so this test names the invariant explicitly rather than leaving a reader of the
    /// census to infer it from a panic message.
    #[test]
    fn declined_constraints_are_declined_only_for_want_of_a_failure_class() {
        let other_gaps: Vec<String> = compiled_law_report()
            .residue
            .iter()
            .filter(|r| r.gap != RcViolationGap::NoFailureClass)
            .map(|r| format!("{} :: {}", r.constraint_iri, r.gap.as_str()))
            .collect();
        assert!(
            other_gaps.is_empty(),
            "a constraint was declined for a reason other than naming no failure class: \
             {other_gaps:?}"
        );
    }

    /// The compiled law set is STRATIFIABLE, so the chase can actually run it.
    ///
    /// Every violation rule's head is `rdf:type` and several bodies carry NAF literals; if
    /// a law's negated literal were ever itself an `rdf:type` test, the predicate graph
    /// would carry a negative self-edge and no finite stratification would exist — the gate
    /// would then fail closed on EVERY dataset. Pinning it here catches that at the law's
    /// authoring rather than on the first `verify()` run that trips it.
    #[test]
    fn the_compiled_law_set_is_stratifiable() {
        let rules = compiled_law_report().rules.clone();
        assert!(!rules.is_empty(), "the gate must compile at least one law");
        let lookup = compile_cached(
            "https://blackcatinformatics.ca/gmeow/reason/enactment-gate/stratification-probe",
            rules,
        );
        assert!(
            lookup.executable.is_some(),
            "the compiled enactment laws must be stratifiable — otherwise the gate refuses \
             every dataset and its findings go permanently dark"
        );
    }

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
