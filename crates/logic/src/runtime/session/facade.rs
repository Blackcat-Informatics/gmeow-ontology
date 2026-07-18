// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The public [`ReasoningSession`] façade over the incremental maintenance engine.
//!
//! All `apply`-family methods are **total** — they never panic and always return an
//! [`OperationOutcome`]. Construction is only via the constructors below; the fields
//! are private and the type carries no authority-write method (it references an
//! authorized commit but never mints a new authorized generation).

use gmeow_logic_compile::ir::{LogicProgram, ReasoningContract};
use purrdf::{
    FallibleDatasetView, PagedDataset, PagedQueryError, PagedQueryEvidence, PagedQueryLimits,
    RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, ViewOperationStatus,
};

use crate::annotation::AnnotationContract;
use crate::cost::{
    DerivedProvenance, ForwardRows, IncrementalForwardSession, NativeIncrementalRun,
};
use crate::seam::{
    BudgetStatus, RdfViewFactSource, WorldFactPattern, WorldFactSource, WorldSourceIdentity,
    WorldSourceMetrics,
};

use super::CERTIFIED_FRAGMENT;
use super::checkpoint::Checkpoint;
use super::delta::SessionDelta;
use super::identity::{SessionIdentity, dataset_rows, mint_edb_generation};
use super::journal::TransitionEntry;
use super::outcome::{
    FragmentDisposition, IncompleteCause, IntegrityFault, OperationOutcome, OutcomeTag,
    RebuildReason, UnsupportedFragment,
};

/// The forward step budget the disposition probe drives the full native reasoner
/// under. It only bounds the probe's runtime: the Tier-2/Tier-3 split is decided by
/// whether the reasoner accepts the program (`Ok`, decidable — possibly budget-cut) or
/// refuses it (`Err`, a hard lowering/planning gap), never by the budget value.
const DISPOSITION_PROBE_BUDGET: u64 = 4_096;

/// The backward solver represents the not-yet-selected body literals of a clause as a
/// `u64` bitmask (one bit per literal), so a body wider than this is a typed refusal.
const CLAUSE_BODY_MASK_WIDTH: usize = 64;

/// The profile a paged EDB scan is admitted under (asserted facts, positive Horn).
const PAGED_SCAN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";

/// The page-fault and source-access accounting of a paged-composed `open_paged` — the
/// evidence an AC6 composition test asserts (cross-view fingerprint equality plus
/// page-fault accounting).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PagedCompositionMetrics {
    /// Structural source-access metrics (delivered quads, pattern/cardinality probes).
    pub source: WorldSourceMetrics,
    /// Backend page-fault accounting (requested/consumed pages, generation) — the
    /// per-page evidence a cross-view page-fault-accounting test asserts.
    pub backend: PagedQueryEvidence,
}

/// A stable, content-addressed operational reasoning session.
///
/// See the [module docs](super) for the seven bound identities, the total six-way
/// outcome, the hash-linked journal, and the content-addressed checkpoint contract.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReasoningSession {
    /// The maintenance engine — `Some` only for a [`FragmentDisposition::Incremental`]
    /// program (the sole path that drives `Applied`); `None` for the routed/refused
    /// dispositions.
    inner: Option<IncrementalForwardSession>,
    /// The three-way classification of the FIXED program, decided once at `open`.
    disposition: FragmentDisposition,
    /// The seven-axis identity this session is pinned to.
    identity: SessionIdentity,
    /// The hash-linked transition journal.
    journal: Vec<TransitionEntry>,
    /// The current journal head (genesis = `identity.descriptor_hash`).
    head: String,
    /// The maintained least-model closure — the reader `facts()` surfaces.
    closure: ForwardRows,
    /// The full evidence of the most recent committed transaction (cost vector, signed
    /// changes, per-fact derivations, consumed steps), for the provenance reader.
    latest_run: Option<NativeIncrementalRun>,
    /// One canonical witness for EVERY derived fact currently in [`Self::closure`] —
    /// the full-closure provenance `provenance()` surfaces, maintained from the initial
    /// settle across every committed transaction.
    closure_provenance: Vec<DerivedProvenance>,
    /// The paged-source composition metrics, when this session was opened via
    /// [`Self::open_paged`]; `None` for a resident `open`.
    paged_metrics: Option<PagedCompositionMetrics>,
}

impl ReasoningSession {
    /// Open a session over an authorized resident EDB, program, contract, and
    /// annotation.
    ///
    /// The FIXED program's fragment is classified ONCE here: a certified fragment opens
    /// a live maintainer; a non-certified one opens a session whose every `apply`
    /// returns [`OperationOutcome::UnsupportedFragment`] (so the refusal is observable
    /// on the public surface without a panic). The seven-axis [`SessionIdentity`] is
    /// bound and the genesis journal head is `identity.descriptor_hash`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the EDB cannot be content-addressed, the program cannot be
    /// lowered, or (for a certified fragment) the maintainer cannot be prepared (e.g.
    /// the EDB is not exactly one named world).
    pub fn open(
        edb: &RdfDataset,
        program: &LogicProgram,
        contract: &ReasoningContract,
        annotation: &AnnotationContract,
    ) -> gmeow_errors::Result<Self> {
        let data_generation = mint_edb_generation(edb)?;
        let (disposition, inner) = classify_disposition(program, edb, annotation)?;
        let closure = inner.as_ref().map_or_else(
            ForwardRows::default,
            IncrementalForwardSession::closure_rows,
        );
        let closure_provenance = match inner.as_ref() {
            Some(session) => session.closure_provenance()?,
            None => Vec::new(),
        };

        let identity = SessionIdentity::bind(
            data_generation,
            program,
            contract,
            annotation,
            CERTIFIED_FRAGMENT,
        );
        let head = identity.descriptor_hash.clone();

        Ok(Self {
            inner,
            disposition,
            identity,
            journal: Vec::new(),
            head,
            closure,
            latest_run: None,
            closure_provenance,
            paged_metrics: None,
        })
    }

    /// Compose over a paged world-source: page in the authorized facts (paying page
    /// faults through the demand provider), thread the paged
    /// [`WorldSourceIdentity`] into [`SessionIdentity::data_generation`], and prepare
    /// the resident incremental maintainer over the collected single-world EDB.
    ///
    /// Returns `Ok(session)` on a fully-paged open, or `Err(outcome)` mapping a paged
    /// failure typed: an OPERATIONAL failure (page/byte budget, cancellation, deadline,
    /// stale generation) surfaced during the scan →
    /// [`OperationOutcome::Incomplete`]; any other engine/materialization failure →
    /// [`OperationOutcome::EngineFailure`]. The page-fault/source metrics are exposed
    /// via [`Self::paged_metrics`] so a cross-view (resident == paged == pack)
    /// composition test can assert closure-fingerprint equality AND page accounting.
    ///
    /// `world` names the single named-graph world the incremental maintainer folds
    /// (the fragment is single-world). `limits` bounds the demand scan.
    #[allow(clippy::result_large_err)]
    pub fn open_paged(
        source: &PagedDataset,
        identity: WorldSourceIdentity,
        world: &str,
        program: &LogicProgram,
        contract: &ReasoningContract,
        annotation: &AnnotationContract,
        limits: PagedQueryLimits,
    ) -> Result<Self, OperationOutcome> {
        let view = source.query_view(limits);
        // A pre-scan operational failure (e.g. provider-wide stale-generation drift).
        if let ViewOperationStatus::Failed { error, .. } = view.operation_status() {
            return Err(map_paged_error(&error));
        }
        let fact_source = RdfViewFactSource::new(&view, PAGED_SCAN_PROFILE, identity.clone());

        // Page in every quad of the authorized world (each demand read may fault).
        let mut builder = RdfDatasetBuilder::new();
        let scan = fact_source.visit_world(world, &WorldFactPattern::ANY, &mut |quad| {
            let subject = crate::reason::term_value_to_rdf_term(&quad.subject)?;
            let object = crate::reason::term_value_to_rdf_term(&quad.object)?;
            let rdf_quad = RdfQuad::new(subject, quad.predicate.clone(), object)
                .in_graph(RdfTerm::iri(world.to_owned()));
            builder.push_owned_quad(&rdf_quad);
            Ok(())
        });
        // A page fault surfaces through the fallible view's operation status (not
        // necessarily the visitor Result), so the post-scan status is authoritative: an
        // OPERATIONAL failure (page/byte budget, cancellation, deadline, stale
        // generation) is Incomplete; a data-corruption failure is EngineFailure. A
        // Ready status carries the backend page-fault evidence.
        let backend = match view.operation_status() {
            ViewOperationStatus::Failed { error, .. } => return Err(map_paged_error(&error)),
            ViewOperationStatus::Ready { evidence } => evidence,
        };
        if let Err(diagnostic) = scan {
            return Err(OperationOutcome::EngineFailure { diagnostic });
        }
        let metrics = PagedCompositionMetrics {
            source: fact_source.metrics(),
            backend,
        };

        let edb = match builder.freeze() {
            Ok(edb) => edb,
            Err(error) => {
                return Err(OperationOutcome::EngineFailure {
                    diagnostic: gmeow_errors::Diag::of_kind(crate::error::Engine {
                        detail: format!(
                            "open_paged: paged EDB failed the freeze contract: {error}"
                        ),
                    }),
                });
            }
        };

        // Classify + prepare over the collected resident EDB.
        let (disposition, inner) = match classify_disposition(program, &edb, annotation) {
            Ok(pair) => pair,
            Err(diagnostic) => return Err(OperationOutcome::EngineFailure { diagnostic }),
        };
        let closure = inner.as_ref().map_or_else(
            ForwardRows::default,
            IncrementalForwardSession::closure_rows,
        );
        let closure_provenance = match inner.as_ref() {
            Some(session) => match session.closure_provenance() {
                Ok(provenance) => provenance,
                Err(diagnostic) => return Err(OperationOutcome::EngineFailure { diagnostic }),
            },
            None => Vec::new(),
        };

        // Thread the PAGED source identity into the data-generation axis.
        let session_identity =
            SessionIdentity::bind(identity, program, contract, annotation, CERTIFIED_FRAGMENT);
        let head = session_identity.descriptor_hash.clone();

        Ok(Self {
            inner,
            disposition,
            identity: session_identity,
            journal: Vec::new(),
            head,
            closure,
            latest_run: None,
            closure_provenance,
            paged_metrics: Some(metrics),
        })
    }

    /// The seven-axis identity this session is pinned to.
    #[must_use]
    pub fn identity(&self) -> &SessionIdentity {
        &self.identity
    }

    /// The current journal head (the transition precondition anchor).
    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    /// The hash-linked transition journal, oldest-first.
    #[must_use]
    pub fn journal(&self) -> &[TransitionEntry] {
        &self.journal
    }

    /// The maintained least-model closure (the derived + asserted rows), in canonical
    /// order — the production reader an operational consumer surfaces after each
    /// operation. Empty for a session opened on a non-certified fragment.
    #[must_use]
    pub fn facts(&self) -> &ForwardRows {
        &self.closure
    }

    /// The full evidence of the most recent committed transaction — the signed closure
    /// changes, the decomposable `(rule, predicate, stratum)` cost vector, the
    /// per-fact derivations, the committed-derivation count, and the budget status. This
    /// is the per-operation provenance a `facts`/`query` reader projects alongside
    /// [`Self::facts`]. `None` before the first committed `apply`.
    #[must_use]
    pub fn latest_run(&self) -> Option<&NativeIncrementalRun> {
        self.latest_run.as_ref()
    }

    /// One canonical proof witness for EVERY derived fact in the CURRENT maintained
    /// closure (firing rule + premises + signed Z-weight), rendered so it is comparable
    /// field-for-field against the full-recompute oracle
    /// ([`crate::reason::reason_program`] → [`crate::reason::InferredAxiom`]).
    ///
    /// This covers the full closure `facts()` reports — including the base facts
    /// materialized at the initial settle, before any delta — not merely the last
    /// transaction's newly-derived facts. For the last transaction's *delta* witnesses
    /// alone, read [`Self::latest_run`]'s `derivations`. Empty for a non-incremental
    /// session (no maintained closure).
    #[must_use]
    pub fn provenance(&self) -> &[DerivedProvenance] {
        &self.closure_provenance
    }

    /// The three-way classification of this session's FIXED program, decided at `open`.
    #[must_use]
    pub fn fragment_disposition(&self) -> &FragmentDisposition {
        &self.disposition
    }

    /// The paged-source composition metrics, when opened via [`Self::open_paged`].
    #[must_use]
    pub fn paged_metrics(&self) -> Option<&PagedCompositionMetrics> {
        self.paged_metrics.as_ref()
    }

    /// Whether this session's fixed program is within the incrementally-maintainable
    /// fragment ([`FragmentDisposition::Incremental`]).
    #[must_use]
    pub fn fragment_supported(&self) -> bool {
        matches!(self.disposition, FragmentDisposition::Incremental)
    }

    /// Apply a content-addressed delta, classifying the result into the total six-way
    /// [`OperationOutcome`]. Never panics.
    ///
    /// Two preconditions are checked first: (1) authorization — `base_commit` must equal
    /// the bound data-generation; (2) transition — `expected_head` must equal the
    /// current head (the structural double-apply guard). The whole transaction is
    /// atomic: it is driven against a cheap `Arc`-backed clone of the maintainer and the
    /// session state advances only on a complete `Applied` run.
    pub fn apply(&mut self, delta: &SessionDelta) -> OperationOutcome {
        // Precondition (1): authorization anchor.
        if delta.base_commit != self.identity.data_generation {
            return OperationOutcome::Invalid {
                fault: IntegrityFault::PreconditionMismatch {
                    expected_state_hash: self.head.clone(),
                    delta_base: delta.base_commit.generation.clone(),
                },
            };
        }
        // Precondition (2): transition anchor (double-apply guard).
        if delta.expected_head != self.head {
            return OperationOutcome::Invalid {
                fault: IntegrityFault::PreconditionMismatch {
                    expected_state_hash: self.head.clone(),
                    delta_base: delta.expected_head.clone(),
                },
            };
        }
        // Disposition: a non-incremental program routes or refuses every apply, typed.
        match &self.disposition {
            FragmentDisposition::Incremental => {}
            FragmentDisposition::RequiresFullRebuild(reason) => {
                return OperationOutcome::RequiresFullRebuild {
                    reason: reason.clone(),
                };
            }
            FragmentDisposition::Unsupported(kind) => {
                return OperationOutcome::UnsupportedFragment { kind: *kind };
            }
        }
        // Bounded retraction has no sound partial-delete frontier: a step-budgeted delta
        // that also retires state is routed to a full rebuild rather than approximated.
        if delta.max_steps.is_some() && !delta.retirements.is_empty() {
            return OperationOutcome::RequiresFullRebuild {
                reason: RebuildReason::BoundedRetractionUnsupported,
            };
        }

        // Drive against an atomic working clone (cheap: Arc-backed).
        let mut working = self
            .inner
            .as_ref()
            .expect("a certified-fragment session carries a maintainer")
            .clone();
        let mut last_run: Option<NativeIncrementalRun> = None;

        // Insertions (skip an empty additions dataset — a suppression-only delta).
        match dataset_rows(&delta.additions) {
            Ok(rows) if !rows.is_empty() => {
                match working.insert(&delta.additions, delta.max_steps) {
                    Ok(run) => match run.status {
                        BudgetStatus::Ok => last_run = Some(run),
                        status @ (BudgetStatus::Partial | BudgetStatus::Exhausted) => {
                            // `insert` commits only on `Ok`; the working clone (and the
                            // session) are left unchanged.
                            return OperationOutcome::Incomplete {
                                status,
                                cause: IncompleteCause::StepBudget,
                            };
                        }
                    },
                    Err(diag) => return classify_engine_error(diag),
                }
            }
            Ok(_) => {}
            Err(diag) => return OperationOutcome::EngineFailure { diagnostic: diag },
        }

        // Retirements (unbounded retraction; skip an empty suppression dataset).
        for suppression in &delta.retirements {
            match dataset_rows(&suppression.row) {
                Ok(rows) if rows.is_empty() => continue,
                Ok(_) => {}
                Err(diag) => return OperationOutcome::EngineFailure { diagnostic: diag },
            }
            match working.retract(&suppression.row) {
                Ok(run) => last_run = Some(run),
                Err(diag) => return classify_engine_error(diag),
            }
        }

        let Some(run) = last_run else {
            // A delta that moved nothing (neither additions nor retirements resolved to
            // rows) is an illegal transaction, not a silent no-op advance.
            return OperationOutcome::Invalid {
                fault: IntegrityFault::IllegalSignedTransaction {
                    detail: "delta carries neither additions nor retirements".to_owned(),
                },
            };
        };

        // Re-derive the full-closure provenance from the post-transaction maintainer
        // BEFORE committing, so a re-descent failure (unreachable for the certified
        // fragment — it has no builtins) leaves the session state unchanged.
        let closure_provenance = match working.closure_provenance() {
            Ok(provenance) => provenance,
            Err(diagnostic) => return OperationOutcome::EngineFailure { diagnostic },
        };

        // Commit the atomic transaction and advance the hash-linked journal.
        self.inner = Some(working);
        self.closure = run.rows.clone();
        self.closure_provenance = closure_provenance;
        let entry = TransitionEntry::advance(
            self.head.clone(),
            delta.delta_identity.clone(),
            OutcomeTag::Applied,
        );
        self.head = entry.new_state_hash.clone();
        let new_state_hash = self.head.clone();
        self.journal.push(entry);
        self.latest_run = Some(run.clone());

        OperationOutcome::Applied {
            run,
            new_state_hash,
        }
    }

    /// Mint a content-addressed checkpoint of the current state.
    ///
    /// The checkpoint stores the identity, the authorized EDB generation, and the
    /// journal head; it carries no circuit state (restore re-materializes).
    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint::new(
            self.identity.clone(),
            self.identity.data_generation.generation.clone(),
            self.head.clone(),
        )
    }

    /// Restore a session from a checkpoint by deterministic re-materialization.
    ///
    /// Returns `Ok(session)` on a fully-gated restore, or `Err(outcome)` carrying the
    /// typed refusal (never a panic, never a silently-coerced rebuild). The gates are,
    /// in order: (1) checkpoint content-integrity (→ [`IntegrityFault::CorruptCheckpoint`]);
    /// (2) re-materialization from the authorized EDB (→ [`OperationOutcome::EngineFailure`]
    /// on an engine fault); (3) identity — the checkpoint's `descriptor_hash` must equal
    /// the freshly-reconstructed one, so a mismatch on ANY of the seven axes is
    /// [`IntegrityFault::IdentityMismatch`]; (4) the minted EDB generation must equal the
    /// checkpoint's. On success the durable `journal_head` is adopted as the head, so a
    /// re-submitted already-committed delta is refused after a restart.
    ///
    /// This takes the full `open` inputs (incl. `contract`/`annotation`) so ALL seven
    /// identity axes are re-derivable and gate-checked; `contract_hash` and
    /// `annotation_identity` cannot be reconstructed from `(edb, program)` alone.
    // The error carrier is deliberately the API's own `OperationOutcome` (the same
    // value `apply` returns by value): a restore refusal IS an operation outcome, so
    // boxing only this path would make the surface inconsistent for no ergonomic gain.
    #[allow(clippy::result_large_err)]
    pub fn restore(
        cp: &Checkpoint,
        authorized_edb: &RdfDataset,
        program: &LogicProgram,
        contract: &ReasoningContract,
        annotation: &AnnotationContract,
    ) -> Result<Self, OperationOutcome> {
        // (1) content integrity.
        if let Err(fault) = cp.verify() {
            return Err(OperationOutcome::Invalid { fault });
        }
        // (2) deterministic re-materialization + fresh identity.
        let session = match Self::open(authorized_edb, program, contract, annotation) {
            Ok(session) => session,
            Err(diagnostic) => return Err(OperationOutcome::EngineFailure { diagnostic }),
        };
        // (3) identity gate — every axis folds into descriptor_hash.
        if cp
            .identity
            .assert_matches(&session.identity.descriptor_hash)
            .is_err()
        {
            return Err(OperationOutcome::Invalid {
                fault: IntegrityFault::IdentityMismatch {
                    expected: cp.identity.descriptor_hash.clone(),
                    found: session.identity.descriptor_hash.clone(),
                },
            });
        }
        // (4) explicit data-generation gate (a precise IdentityMismatch on the EDB axis).
        if session.identity.data_generation.generation != cp.edb_generation {
            return Err(OperationOutcome::Invalid {
                fault: IntegrityFault::IdentityMismatch {
                    expected: cp.edb_generation.clone(),
                    found: session.identity.data_generation.generation.clone(),
                },
            });
        }
        // (5) adopt the durable journal head.
        let mut restored = session;
        restored.head = cp.journal_head.clone();
        restored.journal = Vec::new();
        Ok(restored)
    }

    /// Restore from a checkpoint and resume at its durable head.
    ///
    /// In the re-materialization checkpoint model this is behaviourally identical to
    /// [`Self::restore`] (both re-materialize, gate all seven identity axes and the
    /// checkpoint integrity, and adopt the durable `journal_head`); the distinct name is
    /// the crash-recovery/resume entry point. It is total and never panics.
    #[allow(clippy::result_large_err)]
    pub fn restart(
        cp: &Checkpoint,
        authorized_edb: &RdfDataset,
        program: &LogicProgram,
        contract: &ReasoningContract,
        annotation: &AnnotationContract,
    ) -> Result<Self, OperationOutcome> {
        Self::restore(cp, authorized_edb, program, contract, annotation)
    }
}

/// Classify an engine diagnostic from `insert`/`retract` into an operational outcome.
///
/// The bounded-retraction refusal (a step-governed transaction that is not insert-only)
/// is routed to [`OperationOutcome::RequiresFullRebuild`]; every other diagnostic is a
/// genuine [`OperationOutcome::EngineFailure`].
fn classify_engine_error(diag: gmeow_errors::Diag) -> OperationOutcome {
    if diag.message().contains("must be insert-only") {
        OperationOutcome::RequiresFullRebuild {
            reason: RebuildReason::BoundedRetractionUnsupported,
        }
    } else {
        OperationOutcome::EngineFailure { diagnostic: diag }
    }
}

/// Decide the three-way [`FragmentDisposition`] of the FIXED program over `edb`, and
/// (for the incremental tier) prepare the live maintainer.
///
/// The classification is single-sourced against the existing engine certifiers and the
/// full native reasoner — it never re-implements a stratification/acyclicity checker:
/// * Tier 1 — [`crate::physical::classify_incremental_fragment`] accepts → build the
///   incremental session (a `prepare` failure for a non-fragment reason, e.g. a
///   multi-world EDB, is a genuine infrastructure `Err`, propagated).
/// * Tier 3 (static, exact, terminating) — a negated program the certifier cannot
///   stratify ([`crate::certify::is_stratifiable`] = false) → `NonStratifiable`; a
///   clause body wider than the solver's `u64` mask → `ClauseBodyTooWide`; an
///   existential program the chase certifier ([`crate::physical::ChaseAdmission::certify`])
///   cannot prove terminating → `NonTerminatingExistential`.
/// * Otherwise — split by whether the FULL native reasoner accepts the program under a
///   bounded probe (guaranteed terminating): `Ok` (decidable / progressing) → Tier 2
///   `RequiresFullRebuild`; `Err` (a hard lowering/planning gap) → Tier 3
///   `UnsupportedFragment`, its kind taken from the typed incremental refusal.
///
/// # Errors
///
/// Returns `Err` only for a genuine infrastructure failure preparing a certified
/// incremental session (e.g. a multi-world EDB), never for a fragment classification.
fn classify_disposition(
    program: &LogicProgram,
    edb: &RdfDataset,
    annotation: &AnnotationContract,
) -> gmeow_errors::Result<(FragmentDisposition, Option<IncrementalForwardSession>)> {
    match crate::lower::lower_eval_rules(program) {
        Ok(rules) => match crate::physical::classify_incremental_fragment(&rules) {
            // Binary-Datalog rules AND the program's forward-derivable semantics is fully
            // captured by `program.rules` (nothing the maintainer would drop) AND the bound
            // annotation contract selects an algebra the maintainer materializes exactly:
            // certify. A declared over-approximating annotation algebra is outside the
            // incrementally-maintained fragment (the maintainer only computes the exact
            // minimal-proof-height semiring), so it routes to a full rebuild rather than
            // silently substituting the exact annotation.
            Ok(())
                if derivable_semantics_fully_captured_by_rules(program)
                    && crate::cost::annotation_maintainable_incrementally(annotation) =>
            {
                let session = IncrementalForwardSession::prepare(edb, program, annotation)?;
                Ok((FragmentDisposition::Incremental, Some(session)))
            }
            // Binary-Datalog rules, but the program ALSO carries forward-derivation
            // content (`program.formulas`) the incremental maintainer would silently
            // drop, or the bound annotation algebra is outside the maintained fragment.
            // NEVER certify Incremental while dropping content or degrading the annotation:
            // route/refuse via the full-native probe.
            Ok(()) => Ok((classify_via_full_probe(program, edb, None), None)),
            Err(refusal) => Ok((
                classify_nonincremental(program, edb, &rules, &refusal),
                None,
            )),
        },
        // The program does not even lower to binary Datalog: it is not incremental. Let
        // the full-native probe split decidable (Tier 2) from a hard gap (Tier 3).
        Err(_lowering) => Ok((classify_via_full_probe(program, edb, None), None)),
    }
}

/// Whether the program's forward-DERIVABLE semantics is FULLY captured by
/// `program.rules` — the precondition for certifying [`FragmentDisposition::Incremental`]
/// without a silent drop.
///
/// The incremental maintainer lowers ONLY `program.rules` (`crate::lower::lower_eval_rules`).
/// The full forward reasoner (`crate::reason::reason_program`) additionally lowers
/// `program.formulas` — via `crate::relational_core::lower_formulas` into evaluable rules
/// plus n-ary existential head rules — which is the SOLE program-authored field beyond
/// `rules` that contributes to the forward closure (verified: `reason_program_budgeted`
/// consumes program content through exactly `lower_eval_rules` + `lower_formulas`;
/// `program.axioms` are expected in the EDB, and `correspondences` /
/// `transaction_programs` / `path_shapes` / `constraints` / `validation_shapes` /
/// `reasoning_programs` are consumed by other pipeline stages, not the forward closure).
/// Certifying Incremental while `formulas` is non-empty would present a closure that
/// silently omits the formula-derived tuples — a forbidden silent approximation — so it
/// is gated here.
fn derivable_semantics_fully_captured_by_rules(program: &LogicProgram) -> bool {
    program.formulas.is_empty()
}

/// Refine a non-incremental program into Tier 2 vs Tier 3 using the static single-source
/// certifiers first, then the bounded full-native probe.
fn classify_nonincremental(
    program: &LogicProgram,
    edb: &RdfDataset,
    rules: &[crate::rule_ir::EvalRule],
    refusal: &crate::physical::FragmentRefusal,
) -> FragmentDisposition {
    // Non-stratifiable negation — exact, static (`is_stratifiable` = the certifier's own
    // SCC/negative-edge check; a positive program is trivially stratifiable).
    let has_negation = rules
        .iter()
        .any(|rule| rule.body.iter().any(|atom| atom.negated));
    if has_negation && !crate::certify::is_stratifiable(rules) {
        return FragmentDisposition::Unsupported(UnsupportedFragment::NonStratifiable);
    }
    // Clause body wider than the backward solver's u64 selection mask — exact, static.
    if rules
        .iter()
        .any(|rule| rule.body.len() > CLAUSE_BODY_MASK_WIDTH)
    {
        return FragmentDisposition::Unsupported(UnsupportedFragment::ClauseBodyTooWide);
    }
    // Non-terminating existential chase — exact, static (the chase admission certifier).
    let lowering = crate::relational_core::lower_formulas(program);
    if !lowering.nary_head_rules.is_empty()
        && matches!(
            crate::physical::ChaseAdmission::certify(&lowering.nary_head_rules),
            crate::physical::ChaseAdmission::Uncertified { .. }
        )
    {
        return FragmentDisposition::Unsupported(UnsupportedFragment::NonTerminatingExistential);
    }
    classify_via_full_probe(program, edb, Some(refusal))
}

/// Split Tier 2 vs Tier 3 by asking the full native reasoner (under a bounded,
/// guaranteed-terminating probe) whether it accepts the program.
fn classify_via_full_probe(
    program: &LogicProgram,
    edb: &RdfDataset,
    refusal: Option<&crate::physical::FragmentRefusal>,
) -> FragmentDisposition {
    match crate::reason::reason_program_budgeted(program, edb, Some(DISPOSITION_PROBE_BUDGET)) {
        // The full reasoner decided (or made governed progress) — decidable, not
        // incrementally maintainable → routed to a full rebuild.
        Ok(_) => FragmentDisposition::RequiresFullRebuild(
            RebuildReason::AdditionsOutsideIncrementalFragment,
        ),
        // The full reasoner refused (a hard lowering/planning gap) → unsupported. The
        // kind is taken from the typed incremental refusal (never a string match); a
        // program that failed even to lower defaults to the unsafe/floundering class.
        Err(_diag) => {
            let kind = refusal.map_or(UnsupportedFragment::Floundering, |refusal| {
                UnsupportedFragment::from_incremental_reason(&refusal.reason)
            });
            FragmentDisposition::Unsupported(kind)
        }
    }
}

/// Map a typed paged-source failure onto an operation outcome.
///
/// Every operational page-source failure (cancellation, deadline, page/byte budget,
/// stale generation, provider fault) is [`OperationOutcome::Incomplete`] with the
/// precise cause; only a data-corruption failure is a genuine
/// [`OperationOutcome::EngineFailure`]. The mapping is typed off the
/// [`PagedQueryError`] variants — never a string match.
fn map_paged_error(error: &PagedQueryError) -> OperationOutcome {
    let incomplete = |cause| OperationOutcome::Incomplete {
        status: BudgetStatus::Partial,
        cause,
    };
    match error {
        PagedQueryError::Cancelled { .. } => incomplete(IncompleteCause::Cancelled),
        PagedQueryError::DeadlineExceeded { .. } => incomplete(IncompleteCause::Deadline),
        PagedQueryError::PageBudgetExceeded { .. }
        | PagedQueryError::ByteBudgetExceeded { .. }
        | PagedQueryError::StaleGeneration { .. }
        | PagedQueryError::Provider { .. } => incomplete(IncompleteCause::SourceBudgetExhausted),
        PagedQueryError::InvalidData { .. } => OperationOutcome::EngineFailure {
            diagnostic: gmeow_errors::Diag::of_kind(crate::error::Engine {
                detail: format!("open_paged: paged source delivered invalid data: {error}"),
            }),
        },
    }
}
