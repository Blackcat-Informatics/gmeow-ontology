// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The total, six-way classification of a session operation, plus its typed evidence.
//!
//! [`OperationOutcome`] is the single value every `apply`-family method returns. The
//! six variants are **disjoint and total**: an operation is exactly one of applied,
//! routed-to-rebuild, outside the maintainable fragment, budget-incomplete, integrity-
//! invalid, or a genuine engine failure. Each variant carries the typed evidence that
//! justifies its classification (the run, the budget status, the diagnostic), so a
//! consumer never re-derives the reason from a string.

use crate::cost::NativeIncrementalRun;
use crate::seam::BudgetStatus;

/// The disjoint classification of one session operation (`apply`/`restore`/`restart`).
///
/// Total and never-panicking: an `apply`-family method always returns exactly one of
/// these. Adding a variant is an additive (minor) semver change, hence
/// `#[non_exhaustive]`. Not `Clone`: the `EngineFailure` diagnostic is move-only.
#[derive(Debug)]
#[non_exhaustive]
pub enum OperationOutcome {
    /// The delta was applied as a genuine incremental maintenance; the session state
    /// advanced. Carries the full [`NativeIncrementalRun`] evidence (closure rows,
    /// signed changes, cost vector, consumed steps) and the new hash-linked
    /// journal head.
    Applied {
        /// The final incremental run committed by this operation.
        run: NativeIncrementalRun,
        /// The advanced journal state-hash (the new [`super::ReasoningSession::head`]).
        new_state_hash: String,
    },
    /// The operation is sound but cannot be served incrementally; the caller must
    /// rebuild from scratch. The session state is unchanged.
    RequiresFullRebuild {
        /// Why the incremental path declined.
        reason: RebuildReason,
    },
    /// The FIXED program is outside the incrementally-maintainable fragment; no
    /// incremental application is possible and no approximate closure is presented.
    UnsupportedFragment {
        /// The typed fragment condition that disqualified the program.
        kind: UnsupportedFragment,
    },
    /// The operation ran under a budget that cut it before fixpoint. The session state
    /// is unchanged (the maintainer commits only on a complete `Ok` run).
    Incomplete {
        /// The budget status at the cut (`Partial` or `Exhausted`).
        status: BudgetStatus,
        /// The resource dimension that governed the cut.
        cause: IncompleteCause,
    },
    /// A precondition/integrity gate refused the operation. The session state is
    /// unchanged; this is how double-apply and mismatched restores are refused.
    Invalid {
        /// The specific integrity fault.
        fault: IntegrityFault,
    },
    /// The maintenance engine reported a genuine failure (distinct from an unsupported
    /// fragment, which is classified typed at `open`). Carries the raw diagnostic.
    EngineFailure {
        /// The engine diagnostic.
        diagnostic: gmeow_errors::Diag,
    },
}

/// Why a sound operation was routed to a full rebuild instead of served incrementally.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RebuildReason {
    /// A step-budgeted delta also carried retirements: bounded retraction has no sound
    /// partial-delete frontier, so the whole transaction must be rebuilt.
    BoundedRetractionUnsupported,
    /// The additions fall outside the incremental fragment (they would require a
    /// program the maintainer does not admit).
    AdditionsOutsideIncrementalFragment,
    /// The contract or engine descriptor drifted since the checkpoint, so the cached
    /// incremental state can no longer be trusted.
    ContractOrEngineDriftSinceCheckpoint,
}

/// The typed condition disqualifying a program from incremental maintenance — the
/// public mirror of the crate-internal fragment refusals.
///
/// The first eight variants mirror
/// [`crate::physical::UnsupportedKind`](the seminaive declared-gap kinds) one-for-one
/// (see [`From`] below). The incremental-circuit classifier's own refusals
/// ([`crate::physical::UnsupportedFragmentReason`]) map onto these variants through
/// [`UnsupportedFragment::from_incremental_reason`] with the documented table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedFragment {
    /// A negative dependency-graph edge inside a cycle — no stratification exists.
    NonStratifiable,
    /// A `!`/cut control construct (no declarative bottom-up meaning).
    Cut,
    /// An arithmetic / comparison builtin the incremental circuit does not evaluate.
    Arithmetic,
    /// A non-binary atom (arity ≠ 2 after the world slot is dropped).
    NonBinaryAtom,
    /// A negation/inequality/head variable not range-restricted by a positive body
    /// atom (an unsafe, "floundering" rule).
    Floundering,
    /// An existential-rule program whose termination could not be certified.
    NonTerminatingExistential,
    /// A program whose only path to divergence is unbounded arithmetic self-drive.
    NonTerminatingArithmetic,
    /// A clause body wider than the backward solver's 64-literal bitmask.
    ClauseBodyTooWide,
}

impl From<crate::physical::UnsupportedKind> for UnsupportedFragment {
    fn from(kind: crate::physical::UnsupportedKind) -> Self {
        use crate::physical::UnsupportedKind as K;
        match kind {
            K::NonStratifiable => Self::NonStratifiable,
            K::Cut => Self::Cut,
            K::Arithmetic => Self::Arithmetic,
            K::NonBinaryAtom => Self::NonBinaryAtom,
            K::Floundering => Self::Floundering,
            K::NonTerminatingExistential => Self::NonTerminatingExistential,
            K::NonTerminatingArithmetic => Self::NonTerminatingArithmetic,
            K::ClauseBodyTooWide => Self::ClauseBodyTooWide,
        }
    }
}

impl UnsupportedFragment {
    /// Map an incremental-circuit fragment refusal onto the public surface.
    ///
    /// The incremental classifier checks a narrower set of conditions than the
    /// seminaive core, so several refusals share a public variant. The documented
    /// table is:
    ///
    /// | [`crate::physical::UnsupportedFragmentReason`] | [`UnsupportedFragment`] |
    /// |---|---|
    /// | `Negation`            | `NonStratifiable` (unsupported negation in the circuit) |
    /// | `Builtins`            | `Arithmetic`                                            |
    /// | `UnsafeHeadVar`       | `Floundering` (unsafe, unbound head variable)           |
    /// | `UnsafeInequalityVar` | `Floundering` (unsafe, unbound inequality variable)     |
    /// | `Bodyless`            | `NonBinaryAtom` (a bodyless clause is not an admissible rule) |
    #[must_use]
    pub(crate) fn from_incremental_reason(
        reason: &crate::physical::UnsupportedFragmentReason,
    ) -> Self {
        use crate::physical::UnsupportedFragmentReason as R;
        match reason {
            R::Negation => Self::NonStratifiable,
            R::Builtins => Self::Arithmetic,
            R::UnsafeHeadVar | R::UnsafeInequalityVar => Self::Floundering,
            R::Bodyless => Self::NonBinaryAtom,
        }
    }
}

/// The three-way classification of a session's FIXED program, decided once at `open`.
///
/// This is the typed disposition the AC4 surface reads: a program is exactly one of
/// incrementally maintainable, decidable-but-not-incremental (routed to a full
/// rebuild), or hard-unsupported (refused). It is the single classification `apply`
/// consults to choose between the [`OperationOutcome::Applied`],
/// [`OperationOutcome::RequiresFullRebuild`], and [`OperationOutcome::UnsupportedFragment`]
/// paths.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FragmentDisposition {
    /// Tier 1 — within finite positive binary Datalog: a live incremental maintainer
    /// drives `Applied` operations.
    Incremental,
    /// Tier 2 — outside the incremental fragment but decidable by the full native
    /// reasoner (e.g. stratified NAF, terminating/weakly-acyclic existential chase):
    /// `apply` returns [`OperationOutcome::RequiresFullRebuild`] with this reason.
    RequiresFullRebuild(RebuildReason),
    /// Tier 3 — hard-unsupported (non-stratifiable negation, non-terminating
    /// existential chase, unsafe/floundering, clause-body-too-wide, …): `apply` returns
    /// [`OperationOutcome::UnsupportedFragment`] with this kind.
    Unsupported(UnsupportedFragment),
}

/// The resource dimension that governed a budget-incomplete operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IncompleteCause {
    /// A committed-derivation (`max_steps`) budget cut the run.
    StepBudget,
    /// A cooperative cancellation signal interrupted the operation.
    Cancelled,
    /// A wall-independent deadline evidence interrupted the operation.
    Deadline,
    /// A paged/demand world-source exhausted its page/byte budget.
    SourceBudgetExhausted,
}

/// A precondition or integrity fault that refuses an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum IntegrityFault {
    /// A delta's precondition did not match the current state: either the authorization
    /// anchor (`base_commit` vs the bound data-generation) or the transition anchor
    /// (`expected_head` vs the current journal head, the structural double-apply guard).
    PreconditionMismatch {
        /// The state-hash the session currently holds.
        expected_state_hash: String,
        /// The precondition value the delta carried (its `expected_head` or the
        /// `base_commit` generation, depending on which anchor failed).
        delta_base: String,
    },
    /// A bound identity (any of the seven axes, folded into `descriptor_hash`) did not
    /// match on restore — the checkpoint belongs to a different world/engine/contract.
    IdentityMismatch {
        /// The identity the checkpoint was minted under.
        expected: String,
        /// The identity reconstructed in the current environment.
        found: String,
    },
    /// A checkpoint's recomputed content address did not match its stored address —
    /// the checkpoint bytes were tampered with or corrupted.
    CorruptCheckpoint {
        /// The stored content address.
        expected_address: String,
        /// The address recomputed from the checkpoint's fields.
        computed_address: String,
    },
    /// A signed transaction was structurally illegal (e.g. an out-of-range membership
    /// change) at the session boundary.
    IllegalSignedTransaction {
        /// A human-readable description of the illegal transaction.
        detail: String,
    },
}

/// The data-only discriminant of an [`OperationOutcome`], folded into the transition
/// journal so the hash-linked chain records WHICH class of transition occurred without
/// carrying the (non-hashable) evidence payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum OutcomeTag {
    /// [`OperationOutcome::Applied`].
    Applied,
    /// [`OperationOutcome::RequiresFullRebuild`].
    RequiresFullRebuild,
    /// [`OperationOutcome::UnsupportedFragment`].
    UnsupportedFragment,
    /// [`OperationOutcome::Incomplete`].
    Incomplete,
    /// [`OperationOutcome::Invalid`].
    Invalid,
    /// [`OperationOutcome::EngineFailure`].
    EngineFailure,
}

impl OutcomeTag {
    /// The stable wire byte for this tag, framed into the transition state-hash.
    #[must_use]
    pub(crate) fn wire_byte(self) -> u8 {
        match self {
            OutcomeTag::Applied => 0,
            OutcomeTag::RequiresFullRebuild => 1,
            OutcomeTag::UnsupportedFragment => 2,
            OutcomeTag::Incomplete => 3,
            OutcomeTag::Invalid => 4,
            OutcomeTag::EngineFailure => 5,
        }
    }
}

impl OperationOutcome {
    /// The data-only discriminant of this outcome.
    #[must_use]
    pub fn tag(&self) -> OutcomeTag {
        match self {
            OperationOutcome::Applied { .. } => OutcomeTag::Applied,
            OperationOutcome::RequiresFullRebuild { .. } => OutcomeTag::RequiresFullRebuild,
            OperationOutcome::UnsupportedFragment { .. } => OutcomeTag::UnsupportedFragment,
            OperationOutcome::Incomplete { .. } => OutcomeTag::Incomplete,
            OperationOutcome::Invalid { .. } => OutcomeTag::Invalid,
            OperationOutcome::EngineFailure { .. } => OutcomeTag::EngineFailure,
        }
    }
}
