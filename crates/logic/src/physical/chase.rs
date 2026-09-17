// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native restricted (standard) existential-rule chase.
//!
//! The forward semi-naive core ([`crate::physical::seminaive`]) is a pure Datalog
//! engine: [`ground_head`] hard-errors on a head variable the body does not bind.
//! This module adds the missing capability — **value invention** for existential head
//! variables — as the Datalog± *restricted (standard) chase*.
//!
//! # Restricted, not oblivious
//!
//! An [`ExistentialRule`] `∃ȳ. H(x̄, ȳ) ← B(x̄)` fires on a frontier binding of the body
//! ONLY when the head is not *already* satisfied: if the store already contains an
//! extension of the frontier to witnesses making every head atom true, the firing is
//! **skipped** (the restricted-chase satisfaction check).  This is what distinguishes
//! the restricted chase from the oblivious chase, and — together with weak acyclicity
//! of the rule set — is what makes it terminate.
//!
//! # The witness is a Skolem function of the frontier
//!
//! When a firing does invent, each existential variable is bound to a deterministic
//! [`crate::physical::store::SkolemTerm`] witness addressed on the bound frontier
//! VALUES (never lexical variable names) — a genuine Skolem function `f(x̄)`.  Two
//! distinct frontier bindings mint two distinct witnesses. Re-firing on the same
//! frontier recovers the same witness (the registry is idempotent), so a converging
//! program reaches its fixpoint.
//!
//! # Termination is a certificate, not a hope
//!
//! This engine does NOT decide termination — it assumes the caller has certified the
//! program terminating (weak acyclicity) via `ChaseAdmission` and refuses/​budgets the
//! rest.  The [`StepGovernor`] budget is the backstop: an unbudgeted run of a
//! non-terminating program would loop, so the router only calls this unbudgeted on a
//! certified-terminating program, and budgeted otherwise (incomplete-never-wrong).
//!
//! # Routing
//!
//! [`chase_materialize`] is the native forward entry for a value-inventing program
//! (a rule with an existential head variable).

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{Finding, Severity};

use crate::physical::seminaive::{
    Budgeted, NativeOutcome, StepGovernor, StrataProgress, UnsupportedKind,
};
use crate::physical::store::{
    RelationStore, SkolemRegistry, SkolemTerm, WitnessContract, WitnessScope, metadata_identity,
};
use crate::provenance::{
    MinProofHeightSemiring, ProofHeight, mint_derivation_id, mint_nary_reifier, term_display,
};
use crate::rule_ir::{
    DerivedRow, EvalAtom, EvalTerm, Fact, FactKey, Solution, distinct_pairs_satisfied,
    echo_asserted, ground_head, sort_rows,
};
use crate::seam::BudgetStatus;

pub(super) mod join;

/// Shared DL witness materialization backstop, including dynamic native families.
/// A finite termination proof does not bound the size of its witness model.
pub(crate) const DL_CHASE_STEP_BACKSTOP: u64 = 20_000;

/// Wrap a physical-chase condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn physical_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical { detail })
}

/// A chase attempt's outcome: a decided budgeted derivation, or a declared gap.
pub(crate) type ChaseOutcome = NativeOutcome<Budgeted<Vec<DerivedRow>>>;

/// How an existential rule addresses witnesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum WitnessPolicy {
    /// Standard frontier-Skolem witnesses for general existential TGDs.
    FrontierSkolem,
    /// DL tableau blocking: preserve a distinct witness per root binding, then
    /// close recursive same-rule/ordinal obligations on the nearest ancestor.
    DlAncestorBlocking,
}

/// One not-yet-committed chase row and the provenance needed to publish it.
struct PendingRow {
    fact: Fact,
    source_quad_ids: Vec<String>,
    antecedents: Vec<Fact>,
    rule_iri: String,
}

/// One firing's borrowed premises. Standalone publication computes their reifiers
/// lazily once; joint publication uses its own shared provenance builder directly.
pub(super) struct ChasePremises<'a> {
    pub(super) rule_iri: &'a str,
    pub(super) source_facts: &'a [Fact],
    source_quad_ids: Option<Vec<String>>,
}

impl ChasePremises<'_> {
    fn source_quad_ids(&mut self) -> gmeow_errors::Result<&[String]> {
        if self.source_quad_ids.is_none() {
            self.source_quad_ids = Some(
                self.source_facts
                    .iter()
                    .map(Fact::reifier)
                    .collect::<gmeow_errors::Result<_>>()?,
            );
        }
        Ok(self
            .source_quad_ids
            .as_deref()
            .expect("initialized source reifiers"))
    }
}

/// A single existential (tuple-generating) rule: a conjunctive body implies a
/// conjunctive head that may quantify fresh existential variables.
///
/// The head is a conjunction so a `∃y. p(x,y) ∧ D(y)` obligation is ONE rule sharing
/// the invented witness `y` across its atoms.  `distinct` carries the pairwise
/// inequalities of a `≥n p.D` obligation (its `n` witnesses must be distinct), read
/// both by the satisfaction check and — since distinct existential ordinals already
/// mint distinct witnesses — honored by construction on a firing.
///
/// `distinct` is populated directly from the typed IR so witness distinctness is
/// preserved without an intermediate rule-language projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ExistentialRule {
    /// Canonical finite numeric bindings evaluated before witness allocation.
    pub(crate) numeric: Vec<gmeow_logic_compile::relational_core::RcNumeric>,
    /// The content-addressed firing rule IRI.
    pub(crate) rule_iri: String,
    /// The body atoms (positive; the DL-safe fragment binds every frontier var here).
    pub(crate) body: Vec<EvalAtom>,
    /// The conjunctive head atoms.
    pub(crate) head: Vec<EvalAtom>,
    /// Pairwise inequality guards over head/existential variables.
    pub(crate) distinct: Vec<(String, String)>,
    /// Optional explicit witness-address frontier. `None` derives the standard
    /// head/body intersection; `Some([])` gives a rule-scoped shared witness.
    pub(crate) witness_frontier: Option<Vec<String>>,
    /// Witness addressing policy for this rule family.
    pub(crate) witness_policy: WitnessPolicy,
}

impl ExistentialRule {
    /// Every variable occurring in the body (subject/object positions).
    fn body_vars(&self) -> BTreeSet<String> {
        let mut vars = BTreeSet::new();
        for atom in &self.body {
            collect_var(&atom.subject, &mut vars);
            collect_var(&atom.object, &mut vars);
        }
        vars.extend(
            self.numeric
                .iter()
                .filter_map(|call| call.output_var())
                .map(str::to_owned),
        );
        vars
    }

    /// Every variable occurring in the head.
    fn head_vars(&self) -> BTreeSet<String> {
        let mut vars = BTreeSet::new();
        for atom in &self.head {
            collect_var(&atom.subject, &mut vars);
            collect_var(&atom.object, &mut vars);
        }
        vars
    }

    /// The existential head variables: head vars the body does not bind, sorted.
    ///
    /// Sorted so the ordinal assigned to each (its index here) is deterministic —
    /// the ordinal disambiguates the `n` witnesses of a `≥n` head.
    pub(crate) fn existentials(&self) -> Vec<String> {
        let body = self.body_vars();
        self.head_vars()
            .into_iter()
            .filter(|v| !body.contains(v))
            .collect()
    }

    /// The frontier variables: head vars the body DOES bind, sorted.  These are the
    /// Skolem function's arguments — the witness depends on their bound values.
    fn frontier_vars(&self) -> Vec<String> {
        if let Some(explicit) = &self.witness_frontier {
            return explicit.clone();
        }
        self.copied_vars()
    }

    /// Values copied from body to head, independently of witness addressing.
    /// A reduced witness frontier cannot erase these ordinary data-flow edges.
    fn copied_vars(&self) -> Vec<String> {
        let body = self.body_vars();
        self.head_vars()
            .into_iter()
            .filter(|v| body.contains(v))
            .collect()
    }

    /// Whether this rule invents (has at least one existential head variable).
    pub(crate) fn is_existential(&self) -> bool {
        !self.existentials().is_empty()
    }
}

/// Push `term`'s variable name into `vars` if it is a variable.
fn collect_var(term: &EvalTerm, vars: &mut BTreeSet<String>) {
    if let EvalTerm::Var(name) = term {
        vars.insert(name.clone());
    }
}

/// Decide restricted-head satisfaction with one native witness search. A complete
/// extension proves existence immediately; only a complete traversal proves absence.
/// Exhaustion without a witness withholds, so it can never mint a redundant witness
/// based on an unfinished blocking probe. Native distinct guards prune partial paths
/// once both operands are bound, without building a cardinality cross-product.
fn head_satisfied(
    rule: &ExistentialRule,
    sol: &Solution,
    rel: &RelationStore,
    max_solutions: usize,
) -> gmeow_errors::Result<(bool, bool)> {
    let outcome = join::walk(
        &rule.head,
        rel,
        sol,
        join::Policy {
            max_matches: max_solutions,
            distinct: &rule.distinct,
            retain_sources: false,
        },
        |candidate| Ok(!distinct_pairs_satisfied(&rule.distinct, &candidate)?),
    )?;
    Ok((
        outcome == join::Outcome::Stopped,
        outcome == join::Outcome::Exhausted,
    ))
}

/// Exact source ownership for the selected native EDB record grammar. Evidence
/// retains original native terms; premises use the admitted execution term view.
#[derive(Debug, Clone)]
pub(crate) struct SourceExistentialRule {
    pub(crate) profile: String,
    pub(crate) world: String,
    pub(crate) graph: Option<purrdf::TermValue>,
    pub(crate) source: purrdf::TermValue,
    pub(crate) evidence: Vec<Fact>,
    pub(crate) premises: Vec<Fact>,
    pub(crate) rule: ExistentialRule,
}

#[derive(Debug)]
enum ChaseOwnership {
    Template,
    Source(SourceExistentialRule),
    Domain(crate::physical::SelectedLogicalWorld),
}

impl ChaseOwnership {
    fn owns_world(&self, world: &str) -> gmeow_errors::Result<bool> {
        Ok(match self {
            Self::Template => true,
            Self::Source(source) => source.world == world,
            Self::Domain(domain) => domain.world()? == world,
        })
    }
}

/// Immutable witness and tuple layout, shared across admitted execution schedules.
#[derive(Debug)]
pub(super) struct PreparedChaseRule {
    numeric: super::numeric::Plan,
    pub(super) rule: ExistentialRule,
    existentials: Vec<String>,
    frontier_vars: Vec<String>,
    reifier_groups: Vec<(String, String, Vec<EvalTerm>)>,
    source_identity: [u8; 32],
    ownership: ChaseOwnership,
}

impl PreparedChaseRule {
    /// Exact execution ownership also governs source/effect admission.
    pub(super) fn owns_world(&self, world: &str) -> gmeow_errors::Result<bool> {
        self.ownership.owns_world(world)
    }

    pub(super) fn new(rule: ExistentialRule) -> gmeow_errors::Result<Self> {
        Ok(Self {
            numeric: super::numeric::Plan::for_body(&rule.numeric, &rule.body)
                .map_err(|detail| super::numeric::error(&rule.rule_iri, &detail))?,
            existentials: rule.existentials(),
            frontier_vars: rule.frontier_vars(),
            reifier_groups: reified_nary_head_groups(&rule)?,
            source_identity: metadata_identity("gmeow-existential-source-v1", &rule),
            rule,
            ownership: ChaseOwnership::Template,
        })
    }

    pub(super) fn from_source(source: SourceExistentialRule) -> gmeow_errors::Result<Self> {
        let mut prepared = Self::new(source.rule.clone())?;
        prepared.source_identity = metadata_identity(
            "gmeow-owned-existential-source-v1",
            &(
                &source.profile,
                &source.world,
                &source.graph,
                &source.source,
                &source.evidence,
                &source.rule,
            ),
        );
        prepared.ownership = ChaseOwnership::Source(source);
        Ok(prepared)
    }

    pub(super) fn from_domain(
        domain: crate::physical::SelectedLogicalWorld,
    ) -> gmeow_errors::Result<Self> {
        domain.validate()?;
        let mut prepared = Self::new(domain.rule())?;
        prepared.source_identity = domain.identity();
        prepared.ownership = ChaseOwnership::Domain(domain);
        Ok(prepared)
    }
}

/// Stream a restricted-chase breadth layer against the caller's indexed store.
/// No fact is committed here; the caller merges candidates and charges one shared
/// governor. The same witness recipes serve standalone and joint native execution.
/// The callback borrows the firing's premises, so joint publication never creates
/// a second pending-row buffer or hashes premise reifiers before discarding them.
pub(super) fn chase_round<'a>(
    prepared: impl IntoIterator<Item = &'a PreparedChaseRule>,
    store: &RelationStore,
    solution_cap: usize,
    registry: &mut SkolemRegistry,
    context: (&str, WitnessContract),
    changed: Option<&BTreeSet<String>>,
    mut emit: impl FnMut(Fact, &mut ChasePremises<'_>) -> gmeow_errors::Result<()>,
) -> gmeow_errors::Result<bool> {
    let (world, contract) = context;
    let mut candidate_count = 0usize;
    let mut round_truncated = false;
    for prepared_rule in prepared {
        if !prepared_rule.ownership.owns_world(world)? {
            continue;
        }
        // Positive triggers can only appear when a body relation gains a row.
        // Head growth can only block a prior trigger. Bodyless rules run once.
        if let Some(changed) = changed
            && !prepared_rule.rule.body.iter().any(|atom| {
                changed.contains(&atom.predicate)
                    || store
                        .semantics
                        .alternate_predicate(&atom.predicate)
                        .is_some_and(|alternate| changed.contains(alternate))
            })
        {
            continue;
        }
        let PreparedChaseRule {
            numeric,
            rule,
            existentials,
            frontier_vars,
            reifier_groups,
            source_identity,
            ownership,
        } = prepared_rule;
        let outcome = join::walk(
            &rule.body,
            store,
            &empty_solution(),
            join::Policy {
                max_matches: solution_cap,
                distinct: &[],
                retain_sources: true,
            },
            |mut sol| {
                if !numeric.apply(&rule.rule_iri, &mut sol)? {
                    return Ok(true);
                }
                // The rule's `distinct` guards range over the EXISTENTIAL head vars
                // (the `≥n` distinctness), which are unbound in the body solution.  They
                // are enforced two ways: `head_satisfied` applies them to store
                // candidates (so `≥n` blocks only on n distinct existing witnesses), and
                // distinct existential ordinals mint distinct witnesses on a firing (so
                // the invented facts satisfy them by construction).
                //
                // Restricted-chase satisfaction: skip if the head already holds. A
                // blocking probe that exhausts without a witness withholds. A witnessed
                // extension needs no enumeration of alternative blockers.
                let (already_satisfied, block_truncated) =
                    head_satisfied(rule, &sol, store, solution_cap)?;
                if block_truncated {
                    round_truncated = true;
                    return Ok(false);
                }
                if already_satisfied {
                    return Ok(true);
                }
                let mut scope = contract.scope(world, *source_identity);
                if let ChaseOwnership::Domain(domain) = ownership {
                    scope.origin = crate::physical::WitnessOrigin::NonemptyDomain(domain.clone());
                }
                // Invent one witness per existential var (distinct ordinals ⇒ distinct
                // witnesses), addressed on the bound frontier values.
                let frontier: Vec<_> = frontier_vars
                    .iter()
                    .map(|v| bound_value(&sol, v))
                    .collect::<Result<_, _>>()?;
                let mut extended = sol.clone();
                let mut introduced = Vec::with_capacity(existentials.len());
                // A REIFIED n-ary head reifies each invented tuple `Rel(a₀,…,aₙ)` as
                // `instanceOf(R, Rel) ∧ naryArg{i}(R, aᵢ)` over its OWN existential reifier
                // subject `R`, and mints `R` by TUPLE IDENTITY via `mint_nary_reifier` —
                // content-addressed on the relation + ordered argument VALUES — so the same
                // derived tuple gets the same node regardless of derivation (parity with a
                // pre-reified ground fact). A MULTI-HEAD rule inventing two or more n-ary
                // tuples has two or more reifier subjects, each its own group and its own
                // tuple-identity mint. Every OTHER existential — a DL `some_values_from`
                // witness, or a SHARED value null that occurs as a tuple *argument* (never a
                // reifier subject) — keeps the default frontier-addressed `SkolemTerm` witness.
                if reifier_groups.is_empty() {
                    // No reified n-ary tuple in the head: every existential is a genuine value
                    // witness (DL `∃p.D`, `≥n p.D`, …), frontier-addressed `SkolemTerm`.
                    for (ordinal, evar) in existentials.iter().enumerate() {
                        let witness = mint_witness(rule, &scope, registry, ordinal, &frontier);
                        introduced.push(witness.clone());
                        extended.bindings.push((evar.clone(), witness));
                    }
                } else {
                    // Mint the VALUE-null existentials FIRST (a shared null that is a tuple
                    // ARGUMENT, not a reifier subject) — frontier-addressed `SkolemTerm` — so a
                    // reifier whose ordered argument list references such a null resolves it
                    // BEFORE the tuple-identity mint. Then mint each reifier group by
                    // content-addressed tuple identity over the (now fully bound) argument
                    // values. A single-reifier reified head has no value-null existentials, so
                    // this is byte-identical to the original single-`mint_nary_reifier` path.
                    let reifier_vars: BTreeSet<&str> =
                        reifier_groups.iter().map(|(v, _, _)| v.as_str()).collect();
                    let mut ordinal = 0usize;
                    for evar in existentials.iter() {
                        if reifier_vars.contains(evar.as_str()) {
                            continue;
                        }
                        let witness = mint_witness(rule, &scope, registry, ordinal, &frontier);
                        introduced.push(witness.clone());
                        extended.bindings.push((evar.clone(), witness));
                        ordinal += 1;
                    }
                    for (reifier_var, rel, arg_terms) in reifier_groups {
                        let mut arg_values = Vec::with_capacity(arg_terms.len());
                        for t in arg_terms {
                            arg_values.push(eval_term_value(t, &extended)?);
                        }
                        let witness_iri = mint_nary_reifier(rel, &arg_values)?;
                        extended
                            .bindings
                            .push((reifier_var.clone(), purrdf::TermValue::iri(witness_iri)));
                    }
                }
                // Definition evidence belongs to this exact source/world and is
                // committed alongside the actual matched premises, never invented.
                if let ChaseOwnership::Source(source) = ownership {
                    sol.source_facts.extend(source.premises.iter().cloned());
                    sol.source_facts.sort_by_key(Fact::key);
                    sol.source_facts.dedup_by_key(|fact| fact.key());
                }
                // Ground every head atom; each becomes a candidate new fact.
                let mut premises = ChasePremises {
                    rule_iri: &rule.rule_iri,
                    source_facts: &sol.source_facts,
                    source_quad_ids: None,
                };
                for hatom in &rule.head {
                    let fact = ground_head(hatom, &extended)?;
                    registry.record_head(&scope, &introduced, &fact, &sol.source_facts)?;
                    emit(fact, &mut premises)?;
                    candidate_count = candidate_count.saturating_add(1);
                }
                // Preserve the selected raw-candidate ceiling even when the caller
                // deduplicates rows immediately. Finish the conjunction before checking
                // this ceiling, exactly as buffered publication does.
                Ok(candidate_count < solution_cap)
            },
        )?;
        if outcome != join::Outcome::Complete {
            round_truncated = true;
            break;
        }
    }
    Ok(round_truncated)
}

/// Run the restricted chase for one world's EDB under `rules`.
///
/// Returns the derived rows (the asserted-EDB echo plus every chase-invented head fact),
/// budget status, and completion frontier — the same [`Budgeted`] surface the
/// semi-naive forward core returns, so the router treats a chase result identically.
///
/// # Errors
///
/// Propagates provenance/​grounding failures from the shared `rule_ir` helpers.
pub(crate) fn chase_world(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
) -> gmeow_errors::Result<ChaseOutcome> {
    let (outcome, _registry) = chase_world_explained(world, edb_facts, rules, max_steps)?;
    Ok(outcome)
}

/// Run the restricted chase for one world like [`chase_world`], but ALSO return the
/// [`SkolemRegistry`] of invented witnesses so a caller can EXPLAIN an invented null —
/// recover its decomposable Skolem-function recipe (rule, ordinal, frontier binding) via
/// [`SkolemRegistry::explain`].  [`chase_world`] delegates here and discards the registry;
/// the "explain invented individual" surface keeps it.
///
/// # Errors
///
/// Propagates provenance/​grounding failures from the shared `rule_ir` helpers.
pub(crate) fn chase_world_explained(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ChaseOutcome, SkolemRegistry)> {
    let mut registry = SkolemRegistry::new();
    let outcome = chase_world_with_registry(world, edb_facts, rules, max_steps, &mut registry)?;
    Ok((outcome, registry))
}

/// Run one world while retaining witness recipes across repeated outer fixed-point
/// invocations. The DL reasoner uses this to make ancestor blocking span its
/// alternating ordinary-rule/chase rounds; general chase entry points use a fresh
/// registry and therefore keep standard one-shot behavior.
pub(crate) fn chase_world_with_registry(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
    registry: &mut SkolemRegistry,
) -> gmeow_errors::Result<ChaseOutcome> {
    let mut governor = StepGovernor::new(max_steps);
    let mut out: Vec<DerivedRow> = Vec::new();
    let status = chase_world_into(world, edb_facts, rules, &mut governor, registry, &mut out)?;
    sort_rows(&mut out);
    let progress = StrataProgress {
        completed: usize::from(status == BudgetStatus::Ok),
        total: 1,
        saturated_preds: BTreeSet::new(),
    };
    Ok(NativeOutcome::Decided(Budgeted {
        rows: out,
        status,
        progress,
        consumed_steps: governor.consumed,
    }))
}

/// Chase ONE world into a shared output buffer under a shared step governor.
///
/// Factored out of [`chase_world`] so [`chase_materialize`] can run a single global
/// budget across the sorted worlds (matching `materialize_native`'s discipline).  Echoes
/// the world's asserted EDB, runs the restricted-chase fixpoint, and appends the derived
/// rows (each stamped with `world`) to `out`.  Returns the world's budget status.
///
/// The caller owns the [`SkolemRegistry`] so the invented witnesses survive the run and
/// can be EXPLAINED afterward (and, in [`chase_materialize`], so ONE registry spans the
/// sorted worlds. Witness IRIs bind world, native contract, source rule and frontier;
/// sharing the registry never identifies witnesses from independent worlds).
///
/// Both callers ([`chase_world_explained`] and [`chase_materialize`]) RETAIN the derived
/// rows — the existential chase has no closure-only, provenance-discarding lane (the backward
/// leg uses [`crate::physical::seminaive::evaluate`], not the chase). If a discarding caller
/// is ever added here, thread a
/// [`ProvenanceMode`](crate::physical::seminaive::ProvenanceMode)-style skip through the round
/// loop rather than accumulating `out` it will throw away.
fn chase_world_into(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    governor: &mut StepGovernor,
    registry: &mut SkolemRegistry,
    out: &mut Vec<DerivedRow>,
) -> gmeow_errors::Result<BudgetStatus> {
    // Seed the columnar store from the EDB; echo the asserted facts as derived rows so
    // the native fact set is directly comparable to an oracle's closure (which includes
    // the EDB).
    let mut store = RelationStore::new();
    for f in edb_facts {
        store.insert(&f.predicate, &f.subject, &f.object);
    }
    out.extend(echo_asserted(world, edb_facts)?);

    let mut committed: BTreeSet<FactKey> = edb_facts.iter().map(Fact::key).collect();
    let mut status = BudgetStatus::Ok;
    let mut prior_round_height = ProofHeight::ASSERTED;

    // A rule's existential/frontier variable sets are loop-invariant (they depend only on
    // the rule's shape, not the store), so compute them ONCE rather than re-deriving —
    // with their allocations and string clones — every fixpoint round.
    let prepared = rules
        .iter()
        .cloned()
        .map(PreparedChaseRule::new)
        .collect::<gmeow_errors::Result<Vec<_>>>()?;

    let contract = WitnessContract::native(crate::native_semantics::SemanticVocabulary::Exact);

    // Naive restricted-chase fixpoint: each round re-derives against the full store,
    // the restricted-satisfaction check skips already-witnessed obligations, and the
    // SkolemRegistry collapses repeat firings — so a weakly-acyclic program converges.
    // (Incrementality is out of scope: the perf ledger flags the chase non-incremental.)
    'fixpoint: loop {
        // The restricted chase commits one breadth layer per round. The first
        // appearance of a fact is therefore its minimal proof-height layer.
        let round_height = MinProofHeightSemiring.derive([prior_round_height])?;
        // Gather this round's new facts with their provenance, keyed for deterministic
        // FactKey-sorted commit (the columnar-store determinism doctrine).
        // A budgeted run bounds every intermediate to the same ceiling as the whole
        // derivation (it can never commit more), so a single super-polynomial round
        // becomes a sound `Exhausted` withhold instead of an OOM. Unbudgeted ⇒ `usize::MAX`.
        let mut round = BTreeMap::new();
        let round_truncated = chase_round(
            &prepared,
            &store,
            governor.solution_cap(),
            registry,
            (world, contract),
            None,
            |fact, premises| {
                let key = fact.key();
                if !committed.contains(&key)
                    && let std::collections::btree_map::Entry::Vacant(entry) = round.entry(key)
                {
                    entry.insert(PendingRow {
                        fact,
                        source_quad_ids: premises.source_quad_ids()?.to_vec(),
                        antecedents: premises.source_facts.to_vec(),
                        rule_iri: premises.rule_iri.to_owned(),
                    });
                }
                Ok(())
            },
        )?;

        // Ordered vacant-entry insertion retains the former stable-sort winner:
        // the first candidate for each new FactKey, with its actual provenance.
        let mut progressed = false;
        for (
            key,
            PendingRow {
                fact,
                source_quad_ids,
                antecedents,
                rule_iri,
            },
        ) in round
        {
            if governor.spent() {
                status = BudgetStatus::Exhausted;
                break;
            }
            let src_refs: Vec<&str> = source_quad_ids.iter().map(String::as_str).collect();
            let derivation_id = mint_derivation_id(&rule_iri, &src_refs);
            store.insert(&fact.predicate, &fact.subject, &fact.object);
            out.push(DerivedRow {
                // Restricted chase heads retain their witness receipt in the registry.
                cross_world: None,
                graph: world.to_owned(),
                subject: fact.subject,
                predicate: fact.predicate,
                object: fact.object,
                rule_iri,
                source_quad_ids,
                derivation_id,
                proof_height: round_height,
                antecedents,
            });
            committed.insert(key);
            governor.charge();
            progressed = true;
        }
        registry.commit_heads(world, &store);
        if status == BudgetStatus::Exhausted {
            break;
        }
        // A round whose working set was capped is an incomplete layer: the chase cannot
        // certify a fixpoint, so it withholds as `Exhausted` (incomplete-never-wrong)
        // rather than looping on a super-polynomial materialization.
        if round_truncated {
            status = BudgetStatus::Exhausted;
            break 'fixpoint;
        }
        if !progressed {
            break; // natural fixpoint — the chase terminated
        }
        prior_round_height = round_height;
    }

    Ok(status)
}

/// Materialize an existential-rule program over a multi-world store: certify termination,
/// then run the restricted chase world-by-world under ONE global step budget.
///
/// This is the forward entry `materialize::materialize_routed` calls for a value-inventing
/// program, mirroring `materialize_native`'s shape (sorted worlds, a single shared
/// governor, cross-world under-claiming frontier) so the router treats a chase result
/// identically to the Datalog one.
///
/// - Certified (`WeaklyAcyclic`) ⇒ run the chase (a declared budget still applies).
/// - Uncertified WITH a budget ⇒ budgeted-partial (incomplete-never-wrong).
/// - Uncertified with NO budget ⇒ `Unsupported(NonTerminatingExistential)` — the router
///   demotes it to the oracle rather than looping.
///
/// # Errors
///
/// Propagates grounding/​provenance failures and EDB extraction errors.
pub(crate) fn chase_materialize(
    store: &crate::store::WorldStore,
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ChaseAdmission, ChaseOutcome)> {
    let admission = ChaseAdmission::certify(rules);
    if !admits_or_budgeted(&admission, max_steps) {
        // Surface the certificate alongside the refusal rather than discarding it: the
        // caller reads its `Uncertified` violations off the returned admission (as a
        // counted `reason::ledger` capability-gap via `ChaseAdmission::capability_gap_rows`
        // and as a `gmeow:Finding` via `ChaseAdmission::to_finding`).
        return Ok((
            admission,
            NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential),
        ));
    }

    let mut worlds = store.worlds();
    worlds.sort();

    let mut governor = StepGovernor::new(max_steps);
    // ONE registry spans the sorted worlds with explicit world-scoped addresses, so any
    // invented witness stays explainable across the whole materialization.
    let mut registry = SkolemRegistry::new();
    let mut out: Vec<DerivedRow> = Vec::new();
    let mut status = BudgetStatus::Ok;
    for world in &worlds {
        let edb_facts = crate::rule_ir::world_edb_facts(store, world)?;
        // The budget governs DERIVED steps, not the input: once it is spent, later worlds
        // run no derivations, but their ASSERTED (EDB) facts are already known and must
        // still be echoed — dropping them would silently lose input, not just derivations.
        if status == BudgetStatus::Exhausted {
            out.extend(echo_asserted(world, &edb_facts)?);
            continue;
        }
        let world_status = chase_world_into(
            world,
            &edb_facts,
            rules,
            &mut governor,
            &mut registry,
            &mut out,
        )?;
        if world_status == BudgetStatus::Exhausted {
            status = BudgetStatus::Exhausted;
        }
    }

    sort_rows(&mut out);
    let progress = StrataProgress {
        // The chase has no strata; a value-inventing round can always, in principle,
        // extend any head predicate, so saturate none (under-claim, never over).
        completed: usize::from(status == BudgetStatus::Ok),
        total: 1,
        saturated_preds: BTreeSet::new(),
    };
    Ok((
        admission,
        NativeOutcome::Decided(Budgeted {
            rows: out,
            status,
            progress,
            consumed_steps: governor.consumed,
        }),
    ))
}

/// The firing IRI stamped/// The firing IRI stamped on a chase-derived row.
const CHASE_RULE_IRI: &str = "https://blackcatinformatics.ca/gmeow/logic/chase/exists";

/// The empty seed solution.
fn empty_solution() -> Solution {
    Solution {
        bindings: Vec::new(),
        source_facts: Vec::new(),
    }
}

/// The `TermValue` a frontier variable is bound to under `sol` (a hard error if
/// unbound — a frontier var is bound by the body by construction).
fn bound_value(sol: &Solution, var: &str) -> gmeow_errors::Result<purrdf::TermValue> {
    let value = sol.get(var).ok_or_else(|| {
        physical_err(format!(
            "chase: frontier variable {var:?} unbound after body join"
        ))
    })?;
    Ok(value.clone())
}

fn mint_witness(
    rule: &ExistentialRule,
    scope: &WitnessScope,
    registry: &mut SkolemRegistry,
    ordinal: usize,
    frontier: &[purrdf::TermValue],
) -> purrdf::TermValue {
    let recipe = SkolemTerm {
        scope: scope.clone(),
        rule_iri: rule.rule_iri.clone(),
        ordinal,
        frontier: frontier.to_vec(),
    };
    match rule.witness_policy {
        WitnessPolicy::FrontierSkolem => registry.mint(recipe),
        WitnessPolicy::DlAncestorBlocking => registry.mint_dl_blocked(recipe),
    }
}

/// The LOGIC `instanceOf` predicate IRI (the reified-n-ary typing atom) — the single
/// canonical surface in [`crate::provenance`], shared with the n-ary ingestion path so
/// pre-reified EDB tuples and chase-derived tuples agree on the exact predicate IRIs.
fn instance_of_iri() -> String {
    crate::provenance::instance_of_iri()
}

/// Parse a `logic:naryArg{i}` predicate IRI to its positional index, or `None` if the
/// predicate is not a positional n-ary argument predicate (the shared canonical parser).
fn nary_arg_index(predicate: &str) -> Option<usize> {
    crate::provenance::nary_arg_index(predicate)
}

/// Recognize the REIFIED-n-ary head shape and extract `(reifier_var, relation, args_by_index)`.
///
/// The shape is a single existential head variable `R` whose head atoms are EXACTLY
/// `logic:instanceOf(R, Rel)` (predicate == LOGIC `instanceOf`, object a constant relation IRI
/// `Rel`) plus `logic:naryArg{i}(R, aᵢ)` atoms (predicate == LOGIC `naryArg{i}`), all sharing
/// the subject `R`. The arguments are returned ordered by their positional index `i` (NOT by
/// [`ExistentialRule::frontier_vars`], which is lexical). Returns `Ok(None)` for any other
/// existential (a DL `some_values_from` witness, …), which keeps the default `SkolemTerm`
/// witness. Returns `Err` when the shape IS reified but its positional indices are not the
/// contiguous set `{0..n-1}` (a gap or duplicate `naryArg{i}` would mint a wrong reifier).
fn reified_nary_head(
    rule: &ExistentialRule,
) -> gmeow_errors::Result<Option<(String, String, Vec<EvalTerm>)>> {
    let existentials = rule.existentials();
    // Exactly one existential — the shared tuple reifier `R`.
    let [reifier] = existentials.as_slice() else {
        return Ok(None);
    };
    let instance_of = instance_of_iri();
    let mut rel: Option<String> = None;
    let mut args: Vec<(usize, EvalTerm)> = Vec::new();
    for atom in &rule.head {
        // Every head atom of the reified shape has the reifier as its subject.
        let EvalTerm::Var(subj) = &atom.subject else {
            return Ok(None);
        };
        if subj != reifier {
            return Ok(None);
        }
        if atom.predicate == instance_of {
            // The typing atom `instanceOf(R, Rel)` — Rel is a constant relation IRI.
            let EvalTerm::ConstNamed(r) = &atom.object else {
                return Ok(None);
            };
            if rel.is_some() {
                return Ok(None); // more than one typing atom is not the reified shape
            }
            rel = Some(r.clone());
        } else {
            // A head atom outside the reified `naryArg{i}` vocabulary rules the shape out.
            let Some(i) = nary_arg_index(&atom.predicate) else {
                return Ok(None);
            };
            args.push((i, atom.object.clone()));
        }
    }
    let Some(rel) = rel else {
        return Ok(None);
    };
    if args.is_empty() {
        return Ok(None);
    }
    args.sort_by_key(|(i, _)| *i);
    // The positional indices of a REIFIED head MUST be the contiguous set `{0..n-1}` with no
    // duplicate: the ordered arg vector feeds `mint_nary_reifier`, so a gap or a duplicate
    // `naryArg{i}` would mint a wrong (or colliding) content-addressed reifier IRI. Once the
    // shape is confirmed reified (single existential, `instanceOf` typing, `naryArg{i}` args),
    // malformed indices are a HARD ERROR — never a silent mis-addressing (no-optionality).
    for (position, (i, _)) in args.iter().enumerate() {
        if *i != position {
            return Err(physical_err(format!(
                "reified n-ary head for relation {rel:?} has non-contiguous or duplicate \
                 positional arguments (naryArg indices {:?}, expected 0..{})",
                args.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
                args.len()
            )));
        }
    }
    let ordered: Vec<EvalTerm> = args.into_iter().map(|(_, t)| t).collect();
    Ok(Some((reifier.clone(), rel, ordered)))
}

/// Recognize a REIFIED-n-ary head with ANY NUMBER of invented tuples, returning one
/// `(reifier_var, relation, args_by_index)` group per invented tuple.
///
/// This is the multi-tuple generalization of [`reified_nary_head`]. A multi-head TGD may
/// invent two or more n-ary tuples in ONE firing — `m1(?a,?e,?c) ∧ m2(?e,?d) ← …` — each
/// reifying onto its OWN existential reifier subject (`R₁` for `m1`, `R₂` for `m2`), and
/// each must be minted by tuple identity (`mint_nary_reifier`), NOT the frontier-addressed
/// Skolem fallback. The head is partitioned into reifier groups by existential subject var:
/// a group's atoms are exactly `logic:instanceOf(Rₖ, Relₖ)` (one typing atom, `Relₖ` a
/// constant relation IRI) plus `logic:naryArg{i}(Rₖ, aᵢ)` with a contiguous index set
/// `{0..n-1}`. Groups are returned in sorted-reifier-var order (deterministic).
///
/// Returns `Ok(vec![])` for an existential head that carries NO reified-n-ary vocabulary at
/// all (a plain DL `some_values_from` / `≥n` head), which keeps every existential on the
/// default `SkolemTerm` witness. A single-tuple reified head returns exactly one group whose
/// mint is byte-identical to [`reified_nary_head`]'s.
///
/// Hard-fails (no-optionality) on a head that uses the reified vocabulary but is malformed:
/// a non-variable reifier subject, a reifier subject the body binds (not existential — an
/// invented reifier is always fresh), a head atom mixing reified and non-reified predicates,
/// a group missing its `instanceOf` typing atom or its arguments, a duplicate typing atom,
/// or non-contiguous / duplicate positional indices (which would mint a wrong reifier).
fn reified_nary_head_groups(
    rule: &ExistentialRule,
) -> gmeow_errors::Result<Vec<(String, String, Vec<EvalTerm>)>> {
    let instance_of = instance_of_iri();
    // Positional `naryArg{i}` is the discriminant for a reified tuple. `instanceOf`
    // alone cannot be one: ordinary DL existential heads legitimately type an invented
    // witness (for example `domain(P,C) -> instanceOf(P,Thing)`). Once an argument atom
    // selects this shape, the checks below require its matching `instanceOf` relation.
    let uses_reified_vocab = rule
        .head
        .iter()
        .any(|atom| nary_arg_index(&atom.predicate).is_some());
    if !uses_reified_vocab {
        return Ok(Vec::new());
    }

    let existentials: BTreeSet<String> = rule.existentials().into_iter().collect();
    // reifier subject var → (relation from its `instanceOf` typing atom, positional args):
    // the per-reifier accumulator gathered in one head pass, drained into ordered groups below.
    type ReifierAcc = (Option<String>, Vec<(usize, EvalTerm)>);
    let mut groups: std::collections::BTreeMap<String, ReifierAcc> =
        std::collections::BTreeMap::new();
    for atom in &rule.head {
        let EvalTerm::Var(subj) = &atom.subject else {
            return Err(physical_err(format!(
                "reified n-ary head atom on predicate <{}> has a non-variable subject — a \
                 reified tuple's subject must be its existential reifier variable",
                atom.predicate
            )));
        };
        if !existentials.contains(subj) {
            return Err(physical_err(format!(
                "reified n-ary head reifier {subj:?} is bound by the body (not existential) — \
                 an invented tuple's reifier node must be a fresh existential, never a \
                 frontier variable"
            )));
        }
        let entry = groups.entry(subj.clone()).or_default();
        if atom.predicate == instance_of {
            let EvalTerm::ConstNamed(r) = &atom.object else {
                return Err(physical_err(format!(
                    "reified n-ary typing atom instanceOf({subj:?}, …) has a non-IRI object — \
                     the typed relation must be a constant relation IRI"
                )));
            };
            if entry.0.is_some() {
                return Err(physical_err(format!(
                    "reified n-ary reifier {subj:?} carries more than one instanceOf typing \
                     atom — a tuple reifies onto exactly one relation"
                )));
            }
            entry.0 = Some(r.clone());
        } else if let Some(i) = nary_arg_index(&atom.predicate) {
            entry.1.push((i, atom.object.clone()));
        } else {
            return Err(physical_err(format!(
                "reified n-ary head mixes a non-reified predicate <{}> with reified-tuple \
                 atoms — a reified head atom must be instanceOf or naryArg{{i}}",
                atom.predicate
            )));
        }
    }

    let mut out: Vec<(String, String, Vec<EvalTerm>)> = Vec::with_capacity(groups.len());
    for (reifier, (rel, mut args)) in groups {
        let Some(rel) = rel else {
            return Err(physical_err(format!(
                "reified n-ary reifier {reifier:?} has argument atoms but no instanceOf typing \
                 atom — the reified tuple's relation is unknown"
            )));
        };
        if args.is_empty() {
            return Err(physical_err(format!(
                "reified n-ary reifier {reifier:?} for relation {rel:?} carries no naryArg \
                 argument — a fixed-arity n-ary tuple has at least one argument"
            )));
        }
        args.sort_by_key(|(i, _)| *i);
        for (position, (i, _)) in args.iter().enumerate() {
            if *i != position {
                return Err(physical_err(format!(
                    "reified n-ary head for relation {rel:?} has non-contiguous or duplicate \
                     positional arguments (naryArg indices {:?}, expected 0..{})",
                    args.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
                    args.len()
                )));
            }
        }
        let ordered: Vec<EvalTerm> = args.into_iter().map(|(_, t)| t).collect();
        out.push((reifier, rel, ordered));
    }
    Ok(out)
}

/// The [`purrdf::TermValue`] an [`EvalTerm`] denotes under solution `sol`: a named/literal
/// constant directly, or a variable's native bound value (a hard error if
/// the variable is unbound — a range-restricted head argument is bound by the body by
/// construction).
fn eval_term_value(term: &EvalTerm, sol: &Solution) -> gmeow_errors::Result<purrdf::TermValue> {
    match term {
        EvalTerm::ConstNamed(iri) => Ok(purrdf::TermValue::iri(iri)),
        EvalTerm::ConstLit(value) => Ok(value.clone()),
        EvalTerm::Var(name) => {
            let value = sol.get(name).ok_or_else(|| {
                physical_err(format!(
                    "chase: n-ary head argument variable {name:?} unbound after body join"
                ))
            })?;
            Ok(value.clone())
        }
    }
}

/// The firing rule IRI recorded for provenance — a fixed chase IRI (the chase is one
/// engine, not a per-rule reduct), kept separate from `CHASE_RULE_IRI` only so a future
/// per-rule attribution can refine it without touching the derivation-id recipe.
fn fact_rule_iri(_sources: &[String]) -> String {
    CHASE_RULE_IRI.to_owned()
}

// ── ChaseAdmission: the termination certificate (constant-refined weak acyclicity) ──
//
// The chase does not decide termination; the router certifies the program FIRST and
// only runs a certified-terminating program unbudgeted.  Termination of the restricted
// chase is decided by a **position dependency graph**: normal edges track how a
// frontier value flows body→head, special (existential) edges track where a fresh null
// is placed; the program terminates when no special edge lies inside a cycle (weak
// acyclicity).  This is the `ExistentialRule`-native port of `certify.rs`'s
// `certify_weak_acyclicity` — computed on the SAME rules the chase runs, so the
// existential head vars are actually visible (a text re-projection would hard-error on
// them and the certifier would stay vacuous).
//
// # Constant refinement (why plain positions are too coarse)
//
// The binary `type(individual, class)` encoding puts every class in ONE object slot, so
// plain weak acyclicity collapses `type(?x, C)` and `type(?y, D)` into the same subject
// position and spuriously reports the terminating `C ⊑ ∃p.D` as cyclic.  A position is
// therefore **refined by the constant co-occurring in the other slot**: a null typed `D`
// lives at `(type, S | D)` and can only be consumed by a body atom matching class `D`, so
// it never triggers the `type(?x, C)` rule — the refinement tracks class-typed null flow
// precisely.  The refinement is sound: it only SPLITS positions by a constant that
// genuinely partitions which body atoms can consume the null; a variable in the other
// slot stays the wildcard `*`, and where both a wildcard and constants occur for one
// `(predicate, slot)` they are conservatively connected (over-approximating reachability,
// never under — so a non-terminating program is never wrongly certified).

/// The class refinement of a position: the constant co-occurring in the atom's other
/// slot, or the wildcard when that slot is a variable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum ClassKey {
    /// The other slot is this constant surface.
    Const(String),
    /// The other slot is a variable — matches any class.
    Wildcard,
}

/// Which column of a binary atom a variable occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Slot {
    Subject,
    Object,
}

/// A node in the position dependency graph: a `(predicate, slot)` refined by the
/// co-occurring constant class.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Position {
    predicate: String,
    slot: Slot,
    class: ClassKey,
}

impl Position {
    fn render(&self) -> String {
        let slot = match self.slot {
            Slot::Subject => "S",
            Slot::Object => "O",
        };
        let class = match &self.class {
            ClassKey::Const(k) => k.as_str(),
            ClassKey::Wildcard => "*",
        };
        format!("{}[{slot}|{class}]", self.predicate)
    }
}

/// The refined positions at which `var` occurs across `atoms`.
fn refined_positions(atoms: &[EvalAtom], var: &str) -> Vec<Position> {
    let mut out = Vec::new();
    for atom in atoms {
        if matches!(&atom.subject, EvalTerm::Var(v) if v == var) {
            out.push(Position {
                predicate: atom.predicate.clone(),
                slot: Slot::Subject,
                class: class_key(&atom.object),
            });
        }
        if matches!(&atom.object, EvalTerm::Var(v) if v == var) {
            out.push(Position {
                predicate: atom.predicate.clone(),
                slot: Slot::Object,
                class: class_key(&atom.subject),
            });
        }
    }
    out
}

/// The class key contributed by the OTHER slot's term.
fn class_key(other: &EvalTerm) -> ClassKey {
    match other {
        EvalTerm::ConstNamed(iri) => ClassKey::Const(format!("<{iri}>")),
        EvalTerm::ConstLit(t) => ClassKey::Const(term_display(t)),
        EvalTerm::Var(_) => ClassKey::Wildcard,
    }
}

/// The termination certificate for the restricted chase — a certified-terminating class
/// on an explicit **escalation order** ([`Self::rank`]):
///
/// ```text
/// Uncertified ⊏ WeaklyAcyclic ⊏ JointlyAcyclic ⊏ SuperWeaklyAcyclic ⊏ ModelSummarizingAcyclic
/// ```
///
/// This `⊏` is the escalation order [`Self::certify`] tries cheapest-first, NOT a subset
/// chain: weak acyclicity is strictly contained in each broader class, and
/// model-summarizing acyclicity strictly contains all, but **joint and super-weak
/// acyclicity are incomparable siblings** (Cuenca Grau et al., JAIR 47, 2013) — neither
/// certifies a superset of the other.  Joint acyclicity is a constant-refined
/// existential-dependency graph; super-weak acyclicity a Skolem place graph with
/// unification (its occurs-check catches nulls joint acyclicity's positions cannot, and
/// vice versa).  `certify` reports the **first (least-cost sufficient)** class that
/// holds; every certified class `admits_native`, and `Uncertified` refuses or budgets.
///
/// The broader classes prove termination of the **skolem/oblivious** chase, which
/// soundly bounds the **restricted** chase the engine runs (skolem-chase termination ⟹
/// restricted-chase termination), so the witness-addressing [`WitnessPolicy`] is
/// unchanged: the certificate selects the proof variant, the runtime keeps its
/// restricted chase.
///
/// The order is implemented explicitly ([`Self::rank`]), never derived: a derived `Ord`
/// would order by declaration, not by the certified-strength meaning.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChaseAdmission {
    /// Certified terminating by constant-refined **weak acyclicity**: no existential
    /// edge lies inside a cycle.  `evidence` records the proof shape (position /
    /// existential-edge counts).
    WeaklyAcyclic {
        /// Human-readable proof summary folded into the divergence ledger.
        evidence: String,
    },
    /// Certified terminating by **joint acyclicity** (strictly broader than weak):
    /// the existential-dependency graph over existential variables is acyclic.
    JointlyAcyclic {
        /// Human-readable proof summary folded into the divergence ledger.
        evidence: String,
    },
    /// Certified terminating by **super-weak acyclicity** (strictly broader than
    /// joint): the place/trigger moving relation over existentials is acyclic.
    SuperWeaklyAcyclic {
        /// Human-readable proof summary folded into the divergence ledger.
        evidence: String,
    },
    /// Certified terminating by **model-summarizing acyclicity** (strictly broader
    /// than super-weak): the engine's own Datalog fixpoint over the critical instance
    /// derives no cyclic-null dependency.  The certifier is a self-hosted reasoning
    /// program — the engine dogfooding its fixpoint as its termination analysis.
    ModelSummarizingAcyclic {
        /// Human-readable proof summary folded into the divergence ledger.
        evidence: String,
    },
    /// Not certified terminating; `violations` names each existential-edge-in-cycle,
    /// deterministically sorted — the router refuses or budgets it.
    Uncertified {
        /// The offending special edges, sorted.
        violations: Vec<String>,
    },
}

/// Native statement analysis only. Executable layouts are never rewritten by
/// source specialization or by the binary termination projection.
#[derive(Debug, Clone)]
pub(crate) struct StatementRule {
    pub(crate) name: String,
    pub(crate) body: Vec<[EvalTerm; 3]>,
    pub(crate) heads: Vec<[EvalTerm; 3]>,
    pub(crate) frontier: Option<Vec<String>>,
    /// The head represents positions of an arbitrary finite witness family,
    /// rather than a complete tuple-generating rule with fixed multiplicity.
    pub(crate) position_only: bool,
}

impl StatementRule {
    pub(crate) fn from_binary(rule: &ExistentialRule) -> Self {
        let statement = |atom: &EvalAtom| {
            [
                atom.subject.clone(),
                EvalTerm::named(&atom.predicate),
                atom.object.clone(),
            ]
        };
        Self {
            name: rule.rule_iri.clone(),
            body: rule.body.iter().map(statement).collect(),
            heads: rule.head.iter().map(statement).collect(),
            frontier: if rule.numeric.is_empty() {
                rule.witness_frontier.clone()
            } else {
                Some(
                    super::numeric::input_variables(&rule.body)
                        .into_iter()
                        .collect(),
                )
            },
            position_only: !rule.numeric.is_empty(),
        }
    }

    pub(crate) fn from_property(property: &crate::physical::PreparedPropertyRule) -> Self {
        Self {
            name: property.source.rule_iri.clone(),
            body: property
                .analysis_body
                .iter()
                .map(|atom| atom.0.clone())
                .collect(),
            heads: property
                .analysis_heads
                .iter()
                .map(|atom| atom.0.clone())
                .collect(),
            frontier: property.witness_frontier.clone(),
            position_only: matches!(
                property.source.operation.as_ref(),
                Some(crate::physical::PropertyOperation::Minimum(_))
            ),
        }
    }
}

impl ChaseAdmission {
    /// Certify the combined ordinary, existential and native schema producers.
    ///
    /// The binary certifier sees each statement `(s, p, o)` through three fixed
    /// relations: `(s, p)`, `(s, o)` and `(p, o)`. Predicate variables therefore
    /// participate in the SAME value-flow proof as subject/object variables.
    /// This is a conservative analysis abstraction, never an execution rewrite:
    /// projecting every concrete fact maps each concrete Skolem firing to a firing
    /// of the abstract rule with identical variables and witness frontier. Pair
    /// joins may admit additional combinations, but cannot remove a concrete firing.
    /// A finite abstract Skolem closure consequently bounds the concrete closure.
    ///
    /// Each head remains one conjunction and no reifier or fresh variable is added.
    /// Constants retain their typed identity. Abstract relation names cannot collide
    /// with authored predicates: ALL input predicates become terms in the analysis.
    /// The immutable joint plan retains the result, so no world data is projected.
    pub(crate) fn certify_with_properties(
        rules: &[ExistentialRule],
        properties: &[crate::physical::PreparedPropertyRule],
        semantics: crate::native_semantics::SemanticVocabulary,
    ) -> Self {
        if properties.is_empty() && rules.iter().all(|rule| rule.numeric.is_empty()) {
            return Self::certify(rules);
        }
        let analysis: Vec<_> = rules
            .iter()
            .map(StatementRule::from_binary)
            .chain(properties.iter().map(StatementRule::from_property))
            .collect();
        Self::certify_statements(&analysis, semantics)
    }

    /// Certify a complete over-approximation of native producers. Source-bound
    /// specializations may use this only after proving every substituted relation
    /// immutable and enumerating all its bindings within the admitted input.
    pub(crate) fn certify_statements(
        rules: &[StatementRule],
        semantics: crate::native_semantics::SemanticVocabulary,
    ) -> Self {
        let fixed_relations = rules
            .iter()
            .flat_map(|rule| rule.body.iter().chain(&rule.heads))
            .all(|atom| {
                matches!(
                    &atom[1],
                    EvalTerm::ConstNamed(_) | EvalTerm::ConstLit(purrdf::TermValue::Iri(_))
                )
            });
        let project = |terms: [&EvalTerm; 3]| {
            [
                (0, 1, "urn:gmeow:termination:subject-predicate"),
                (0, 2, "urn:gmeow:termination:subject-object"),
                (1, 2, "urn:gmeow:termination:predicate-object"),
            ]
            .map(|(left, right, relation)| {
                EvalAtom::positive(terms[left].clone(), relation, terms[right].clone())
            })
        };
        // Abstract only the terms entering the termination proof. The executable
        // property layouts remain shared and retain their exact native spellings.
        let property_atom = |terms: &[EvalTerm; 3]| {
            let terms = terms.each_ref().map(|term| {
                let mut term = term.clone();
                semantics.abstract_term(&mut term);
                term
            });
            // When metadata fixes every operator, retain the original binary
            // relations and their class-refined positions. Dropping predicate
            // correlation into statement pairs would create spurious cycles.
            if fixed_relations {
                let predicate = match &terms[1] {
                    EvalTerm::ConstNamed(iri) | EvalTerm::ConstLit(purrdf::TermValue::Iri(iri)) => {
                        iri
                    }
                    _ => unreachable!("fixed native predicate"),
                };
                vec![EvalAtom::positive(
                    terms[0].clone(),
                    predicate,
                    terms[2].clone(),
                )]
            } else {
                project(terms.each_ref()).to_vec()
            }
        };
        let analysis: Vec<_> = rules
            .iter()
            .map(|rule| ExistentialRule {
                numeric: Vec::new(),
                rule_iri: rule.name.clone(),
                body: rule.body.iter().flat_map(property_atom).collect(),
                head: rule.heads.iter().flat_map(property_atom).collect(),
                // Dropping inequality guards only adds possible firings.
                distinct: Vec::new(),
                witness_frontier: rule.frontier.clone(),
                witness_policy: WitnessPolicy::FrontierSkolem,
            })
            .collect();
        // Two symbolic ordinals preserve position dependencies for arbitrary
        // finite counts. They do NOT preserve every nonlinear tuple join (e.g.
        // a triangle requiring three distinct siblings). Only the position proof
        // may certify this abstraction; MSA requires a complete rule expansion.
        let mut admission = if rules.iter().any(|rule| rule.position_only) {
            Self::certify_weakly_acyclic(&analysis)
                .unwrap_or_else(|violations| Self::Uncertified { violations })
        } else {
            Self::certify(&analysis)
        };
        let context = format!(
            "joint statement value-flow abstraction of {} native producer(s)",
            rules.len()
        );
        match &mut admission {
            Self::WeaklyAcyclic { evidence }
            | Self::JointlyAcyclic { evidence }
            | Self::SuperWeaklyAcyclic { evidence }
            | Self::ModelSummarizingAcyclic { evidence } => {
                *evidence = format!("{context}; {evidence}");
            }
            Self::Uncertified { violations } => {
                violations.insert(0, format!("{context} has no joint termination certificate"));
            }
        }
        admission
    }

    /// Certify `rules` by the termination-class ladder: escalate cheapest-first
    /// (weak → joint → super-weak → model-summarizing acyclicity) and report the
    /// **least-cost sufficient** certificate. The polynomial rungs run before the
    /// EXPTIME model-summarizing check, which is reached only when the structural
    /// rungs all refuse. When no class certifies, return `Uncertified` carrying the
    /// weak-acyclicity position-graph violations (the canonical diagnostic).
    pub(crate) fn certify(rules: &[ExistentialRule]) -> Self {
        if rules.iter().any(|rule| !rule.numeric.is_empty()) {
            // Arithmetic can collapse or reuse values. Only the conservative
            // position proof applies; critical-instance tuple proofs do not.
            let mut abstraction = rules.to_vec();
            for rule in &mut abstraction {
                if !rule.numeric.is_empty() {
                    rule.witness_frontier = Some(
                        super::numeric::input_variables(&rule.body)
                            .into_iter()
                            .collect(),
                    );
                }
                rule.numeric.clear();
            }
            return Self::certify_weakly_acyclic(&abstraction)
                .unwrap_or_else(|violations| Self::Uncertified { violations });
        }
        match Self::certify_weakly_acyclic(rules) {
            Ok(admission) => admission,
            Err(violations) => Self::certify_joint_acyclic(rules)
                .or_else(|| Self::certify_super_weak_acyclic(rules))
                .or_else(|| Self::certify_model_summarizing(rules))
                .unwrap_or(Self::Uncertified { violations }),
        }
    }

    /// Constant-refined **weak acyclicity** of the position graph: `Ok(WeaklyAcyclic)`
    /// when no existential edge lies in a cycle, else `Err(violations)` (the sorted
    /// edge-in-cycle diagnostics, reused as the `Uncertified` fallback).
    fn certify_weakly_acyclic(rules: &[ExistentialRule]) -> Result<Self, Vec<String>> {
        // Adjacency (normal ∪ special) and the special-edge list.
        let mut adj: std::collections::BTreeMap<Position, BTreeSet<Position>> =
            std::collections::BTreeMap::new();
        let mut special: Vec<(Position, Position)> = Vec::new();
        let mut all_nodes: BTreeSet<Position> = BTreeSet::new();

        for rule in rules {
            let body_vars = rule.body_vars();
            let existentials: BTreeSet<String> = rule.existentials().into_iter().collect();

            // Normal edges: a frontier var's body positions → its head positions.
            for hv in rule.head_vars() {
                if !body_vars.contains(&hv) {
                    continue;
                }
                let bpos = refined_positions(&rule.body, &hv);
                let hpos = refined_positions(&rule.head, &hv);
                for b in &bpos {
                    for h in &hpos {
                        all_nodes.insert(b.clone());
                        all_nodes.insert(h.clone());
                        adj.entry(b.clone()).or_default().insert(h.clone());
                    }
                }
            }

            // Special edges: every frontier-var body position → every existential head
            // position (the fresh null depends on the frontier binding).
            if !existentials.is_empty() {
                let mut frontier_bpos: Vec<Position> = Vec::new();
                for fv in rule.frontier_vars() {
                    frontier_bpos.extend(refined_positions(&rule.body, &fv));
                }
                for e in &existentials {
                    for h in refined_positions(&rule.head, e) {
                        for b in &frontier_bpos {
                            all_nodes.insert(b.clone());
                            all_nodes.insert(h.clone());
                            adj.entry(b.clone()).or_default().insert(h.clone());
                            special.push((b.clone(), h.clone()));
                        }
                    }
                }
            }
        }

        add_wildcard_subsumption(&mut adj, &all_nodes);

        // A special edge (u → v) violates weak acyclicity iff v can reach u (the edge
        // lies in a cycle → the chase may not terminate).
        let mut violations: Vec<String> = Vec::new();
        for (u, v) in &special {
            if reaches(&adj, v, u) {
                violations.push(format!(
                    "weak-acyclicity: existential edge {} -> {} lies in a cycle (the restricted chase may not terminate)",
                    u.render(),
                    v.render()
                ));
            }
        }
        violations.sort();
        violations.dedup();

        if violations.is_empty() {
            Ok(Self::WeaklyAcyclic {
                evidence: format!(
                    "weakly acyclic: {} refined position(s), {} existential edge(s), none in a cycle",
                    all_nodes.len(),
                    special.len()
                ),
            })
        } else {
            Err(violations)
        }
    }

    /// **Joint acyclicity** (Cuenca Grau et al., JAIR 47, 2013): strictly broader than
    /// weak acyclicity. `Some(JointlyAcyclic)` when the existential-dependency graph
    /// over existential variables is acyclic, else `None`.
    ///
    /// For each existential `e` of each rule, [`move_set`] computes the refined positions
    /// a null created for `e` can occupy (closing null-flow through frontier variables).
    /// The existential-dependency graph has an edge from `e1` to every existential of
    /// rule `r_j` when some frontier variable of `r_j` has *all* its refined body
    /// positions inside `Move(e1)` — i.e. `e1`'s null can bind that frontier and so
    /// trigger `r_j`'s invention.  JA holds iff that graph is acyclic (no existential
    /// transitively depends on itself).  Weak acyclicity conflates positions and so
    /// reports a spurious cycle whenever a null merely *touches* a position on a
    /// position-graph cycle; JA is exact about which frontier a null can actually bind.
    fn certify_joint_acyclic(rules: &[ExistentialRule]) -> Option<Self> {
        let universe = all_program_positions(rules);
        // Existential nodes: (rule index, existential var name).
        let mut existentials: Vec<(usize, String)> = Vec::new();
        for (i, r) in rules.iter().enumerate() {
            for e in r.existentials() {
                existentials.push((i, e));
            }
        }
        if existentials.is_empty() {
            // No existential to certify — weak acyclicity already handled this shape.
            return None;
        }
        // Precompute each rule's frontier flows — (refined body positions, refined head
        // positions) per frontier var — ONCE, instead of re-deriving them on every
        // iteration of every existential's `move_set` fixpoint.
        let precomputed_flows: Vec<Vec<(BTreeSet<Position>, Vec<Position>)>> = rules
            .iter()
            .map(|r| {
                r.copied_vars()
                    .into_iter()
                    .map(|v| {
                        (
                            refined_positions(&r.body, &v).into_iter().collect(),
                            refined_positions(&r.head, &v),
                        )
                    })
                    .collect()
            })
            .collect();
        let moves: Vec<BTreeSet<Position>> = existentials
            .iter()
            .map(|(i, e)| move_set(&precomputed_flows, &rules[*i], e, &universe))
            .collect();

        // Existential-dependency graph.
        let mut edges: std::collections::BTreeMap<usize, BTreeSet<usize>> =
            std::collections::BTreeMap::new();
        let mut edge_count = 0usize;
        for (a, _) in existentials.iter().enumerate() {
            let mv = &moves[a];
            for (j, r_j) in rules.iter().enumerate() {
                if !r_j.is_existential() {
                    continue;
                }
                // Can a1's null bind a frontier of r_j (all that frontier's body
                // positions lie within Move)? Then it can trigger r_j's invention.
                let triggers = r_j.frontier_vars().into_iter().any(|v| {
                    let bpos = refined_positions(&r_j.body, &v);
                    !bpos.is_empty() && bpos.iter().all(|p| move_contains(mv, p, &universe))
                });
                if !triggers {
                    continue;
                }
                for (b, (bi, _)) in existentials.iter().enumerate() {
                    if *bi == j && edges.entry(a).or_default().insert(b) {
                        edge_count += 1;
                    }
                }
            }
        }

        // JA holds iff no existential node lies on a cycle (reaches itself).
        let acyclic = (0..existentials.len()).all(|n| !node_reaches_self(&edges, n));
        acyclic.then(|| Self::JointlyAcyclic {
            evidence: format!(
                "jointly acyclic: {} existential variable(s), {} dependency edge(s), no existential depends on itself",
                existentials.len(),
                edge_count
            ),
        })
    }

    /// **Super-weak acyclicity** (Marnette, 2009): strictly broader than weak acyclicity,
    /// an incomparable sibling of joint acyclicity below model-summarizing acyclicity.
    ///
    /// Builds the **place graph** of the Skolemized program: within a rule, values flow
    /// body→head through frontier variables (and a frontier body place feeds each
    /// existential head place); across rules, a producer head place feeds a consumer body
    /// place ONLY when the Skolemized head atom **unifies** with the consumer body atom
    /// (most-general unifier with occurs-check).  That unification gate is the precision
    /// weak acyclicity lacks: a null minted at `R(x, f(x))` cannot flow into a diagonal
    /// body atom `R(x, x)` (the occurs-check `f(x) = x` fails), so a position-graph cycle
    /// that weak acyclicity reports is broken here.  SWA holds iff no existential head
    /// place lies on a cycle (a null never feeds back to re-mint itself).
    fn certify_super_weak_acyclic(rules: &[ExistentialRule]) -> Option<Self> {
        let (places, edges, existential_out) = build_swa_place_graph(rules);
        if existential_out.is_empty() {
            // No invented null — weak acyclicity already handled this shape.
            return None;
        }
        let acyclic = existential_out
            .iter()
            .all(|&p| !node_reaches_self(&edges, p));
        let cross_edges: usize = edges.values().map(|s| s.len()).sum();
        acyclic.then(|| Self::SuperWeaklyAcyclic {
            evidence: format!(
                "super-weakly acyclic: {} place(s), {} existential output place(s), {} flow edge(s), no null re-mints itself",
                places,
                existential_out.len(),
                cross_edges
            ),
        })
    }

    /// **Model-summarizing acyclicity** (Cuenca Grau et al., JAIR 47, 2013): strictly
    /// broader than joint and super-weak acyclicity, and **self-hosted** — the check *is*
    /// Datalog entailment over the critical instance, so the engine dogfoods its own
    /// fixpoint as its own termination analysis (doctrine: `LOGIC-PERFORMANCE.md`
    /// §"Chase doctrine").
    ///
    /// Each existential is *summarized* by a single fresh constant `n_{r,∃}`.  The rules
    /// become Datalog (existentials → their summarizing constants), a marker `isNull(n)`
    /// tags each, and a dependency rule fires `dep(v, n)` whenever a summarizing null `v`
    /// binds a **frontier** position of the rule minting `n`.  Running the engine's own
    /// [`seminaive::evaluate`] fixpoint over the critical instance (every predicate fully
    /// populated over the program's constants plus one special constant `*`) materializes
    /// the whole `dep` relation; MSA holds iff **no null depends on itself** (no cycle in
    /// `dep`) — a self-dependency is the summarized signature of an unbounded skolem term.
    fn certify_model_summarizing(rules: &[ExistentialRule]) -> Option<Self> {
        const STAR: &str = "https://blackcatinformatics.ca/gmeow/msa#star";
        const IS_NULL: &str = "https://blackcatinformatics.ca/gmeow/msa#isNull";
        const MSA_TRUE: &str = "https://blackcatinformatics.ca/gmeow/msa#true";
        const DEP: &str = "https://blackcatinformatics.ca/gmeow/msa#dep";
        let null_iri =
            |i: usize, e: &str| format!("https://blackcatinformatics.ca/gmeow/msa#null/{i}/{e}");

        // Negation is outside the summarizable positive-existential fragment.
        if rules.iter().any(|r| r.body.iter().any(|a| a.negated)) {
            return None;
        }

        // Summarizing null constants, one per (rule, existential).
        let mut nulls: Vec<String> = Vec::new();
        for (i, r) in rules.iter().enumerate() {
            for e in r.existentials() {
                nulls.push(null_iri(i, &e));
            }
        }
        if nulls.is_empty() {
            return None;
        }

        // Predicates and the constant domain (constants of Σ ∪ the special constant `*`).
        // Dedup the constant domain directly on `TermValue` (no `term_display` String
        // allocation per term), keeping `domain_terms` in deterministic first-seen order.
        let mut predicates: BTreeSet<String> = BTreeSet::new();
        let mut seen: std::collections::HashSet<purrdf::TermValue> =
            std::collections::HashSet::new();
        let mut domain_terms: Vec<purrdf::TermValue> = Vec::new();
        {
            let star = purrdf::TermValue::iri(STAR);
            seen.insert(star.clone());
            domain_terms.push(star);
        }
        for r in rules {
            for atom in r.body.iter().chain(r.head.iter()) {
                predicates.insert(atom.predicate.clone());
                for term in [&atom.subject, &atom.object] {
                    let tv = match term {
                        EvalTerm::ConstNamed(iri) => Some(purrdf::TermValue::iri(iri)),
                        EvalTerm::ConstLit(v) => Some(v.clone()),
                        EvalTerm::Var(_) => None,
                    };
                    if let Some(tv) = tv
                        && seen.insert(tv.clone())
                    {
                        domain_terms.push(tv);
                    }
                }
            }
        }

        // Transform Σ into the MSA Datalog program.
        let mut program: Vec<crate::rule_ir::EvalRule> = Vec::new();
        for (i, r) in rules.iter().enumerate() {
            let existentials: BTreeSet<String> = r.existentials().into_iter().collect();
            // Production: one Datalog rule per head atom, existentials → summarizing nulls.
            for (k, h) in r.head.iter().enumerate() {
                let msa_term = |t: &EvalTerm| -> EvalTerm {
                    match t {
                        EvalTerm::Var(v) if existentials.contains(v) => {
                            EvalTerm::ConstNamed(null_iri(i, v))
                        }
                        other => other.clone(),
                    }
                };
                let head_atom =
                    EvalAtom::positive(msa_term(&h.subject), &h.predicate, msa_term(&h.object));
                program.push(crate::rule_ir::EvalRule::positive(
                    &format!("urn:gmeow:msa:rule:{i}:head:{k}"),
                    head_atom,
                    r.body.clone(),
                ));
            }
            // Dependency: a summarizing null binding a frontier position of `r` depends
            // into every null `r` mints.
            for (frontier_index, v) in r.frontier_vars().into_iter().enumerate() {
                for (existential_index, e) in r.existentials().into_iter().enumerate() {
                    let mut body = r.body.clone();
                    body.push(EvalAtom::positive(
                        EvalTerm::Var(v.clone()),
                        IS_NULL,
                        EvalTerm::ConstNamed(MSA_TRUE.to_owned()),
                    ));
                    let head_atom = EvalAtom::positive(
                        EvalTerm::Var(v.clone()),
                        DEP,
                        EvalTerm::ConstNamed(null_iri(i, &e)),
                    );
                    program.push(crate::rule_ir::EvalRule::positive(
                        &format!(
                            "urn:gmeow:msa:rule:{i}:dependency:{frontier_index}:{existential_index}"
                        ),
                        head_atom,
                        body,
                    ));
                }
            }
        }

        // Bound the analysis BEFORE materializing the critical instance. The critical
        // instance is `predicates × domain²` facts, and BOTH counts are controlled entirely
        // by the authored program, so a pathological (and typically *rejected*) program
        // could allocate an enormous store — exhausting memory or hanging certification —
        // before the fixpoint step budget below ever applies. If the projected size exceeds
        // the cap we conservatively return `None` (the ladder falls through to
        // `Uncertified` → budget) WITHOUT allocating: a HARD refusal, never a silent admit
        // and never an OOM. The cap is generous for real authored programs (a handful of
        // predicates over a handful of constants) and only trips on pathological blow-ups.
        const MSA_CRITICAL_INSTANCE_CAP: usize = 1 << 20;
        let projected_facts = predicates
            .len()
            .saturating_mul(domain_terms.len())
            .saturating_mul(domain_terms.len());
        if projected_facts > MSA_CRITICAL_INSTANCE_CAP {
            return None;
        }

        let executable =
            crate::physical::plan::compile_cached("gmeow-msa-critical-v1", program).executable?;

        // Seek a concrete obstruction on a small SUBSET of the critical instance first.
        // Each original body atom is seeded with all variables bound to STAR; authored
        // constants stay distinct and typed. These are actual critical-instance facts.
        // Positive Datalog is monotone, so a null-dependency cycle derived here also
        // exists in the full model and conclusively defeats MSA. Absence of a cycle,
        // exhaustion, or any execution gap NEVER admits: the full check below remains
        // mandatory. This probe cannot turn bounded-corpus evidence into a certificate.
        let mut probe = RelationStore::new();
        let probe_term = |term: &EvalTerm| match term {
            EvalTerm::Var(_) => purrdf::TermValue::iri(STAR),
            EvalTerm::ConstNamed(iri) => purrdf::TermValue::iri(iri),
            EvalTerm::ConstLit(value) => value.clone(),
        };
        for atom in rules.iter().flat_map(|rule| &rule.body) {
            probe.insert(
                &atom.predicate,
                &probe_term(&atom.subject),
                &probe_term(&atom.object),
            );
        }
        let msa_true = purrdf::TermValue::iri(MSA_TRUE);
        for null in &nulls {
            probe.insert(IS_NULL, &purrdf::TermValue::iri(null), &msa_true);
        }
        const MSA_OBSTRUCTION_PROBE_STEPS: u64 = 4096;
        if let Ok(NativeOutcome::Decided(result)) = crate::physical::seminaive::evaluate(
            probe,
            executable.as_ref(),
            Some(MSA_OBSTRUCTION_PROBE_STEPS),
        ) && msa_dependency_summary(&result.rows, DEP).1
        {
            return None;
        }

        // The critical instance: every predicate over the whole constant domain, plus the
        // null markers.
        let mut store = crate::physical::store::RelationStore::new();
        for p in &predicates {
            for s in &domain_terms {
                for o in &domain_terms {
                    store.insert(p, s, o);
                }
            }
        }
        let msa_true = purrdf::TermValue::iri(MSA_TRUE);
        for n in &nulls {
            store.insert(IS_NULL, &purrdf::TermValue::iri(n), &msa_true);
        }
        let critical_facts = store.row_count();

        // Run the engine's own fixpoint under an explicit step budget. The MSA Datalog
        // program is pure positive Datalog over a finite domain (the summarizing nulls are
        // constants, never invented), so its least model is finite; the budget bounds the
        // cost of a large authored program during `stage-reason`. A non-stratifiable
        // program, an engine error, OR a budget cut is a conservative `None` (the ladder
        // falls through to `Uncertified` → budget) — never a silent admit. Crucially, a
        // budget-`Exhausted` run leaves a PARTIAL least model: a `dep(n,n)` edge it never
        // reached would read as absent and MIS-certify, so `Exhausted` must REFUSE, never
        // certify on an incomplete fixpoint.
        let budget = (critical_facts as u64).saturating_mul(16).max(1 << 20);
        let facts =
            match crate::physical::seminaive::evaluate(store, executable.as_ref(), Some(budget)) {
                Ok(crate::physical::NativeOutcome::Decided(budgeted))
                    if matches!(budgeted.status, crate::seam::BudgetStatus::Ok) =>
                {
                    budgeted.rows
                }
                _ => return None,
            };

        // Only the complete full critical model may authorize this sufficient class.
        let (edge_count, cyclic) = msa_dependency_summary(&facts, DEP);
        (!cyclic).then(|| Self::ModelSummarizingAcyclic {
            evidence: format!(
                "model-summarizing acyclic: {critical_facts} critical-instance fact(s), {} summarizing null(s), {edge_count} dependency edge(s), no null re-mints itself",
                nulls.len()
            ),
        })
    }

    /// Whether the native chase may run this program unbudgeted (it terminates). Every
    /// certified class on the ladder admits; only `Uncertified` does not.
    pub(crate) fn admits_native(&self) -> bool {
        matches!(
            self,
            Self::WeaklyAcyclic { .. }
                | Self::JointlyAcyclic { .. }
                | Self::SuperWeaklyAcyclic { .. }
                | Self::ModelSummarizingAcyclic { .. }
        )
    }

    /// The COUNTED capability-gap rows for this certificate: one
    /// [`crate::reason::ledger::DivergenceKind::DlGap`] row per weak-acyclicity violation
    /// when [`Self::Uncertified`], and NONE when [`Self::WeaklyAcyclic`] (a certified
    /// program has no gap).
    ///
    /// A refused existential program is a native coverage defect, so its gap is routed to
    /// the counted `reason::ledger` DlGap surface (reusing the existing kind — never the
    /// uncounted `physical::parity::ParityLedger`) and categorized
    /// [`crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY`] so it stays out of the
    /// committed DL/EL crosscheck `gapCount == 0` gate.
    pub(crate) fn capability_gap_rows(&self) -> Vec<crate::reason::ledger::LedgerRow> {
        match self {
            Self::Uncertified { violations } => {
                crate::reason::ledger::existential_gap_rows(violations)
            }
            Self::WeaklyAcyclic { .. }
            | Self::JointlyAcyclic { .. }
            | Self::SuperWeaklyAcyclic { .. }
            | Self::ModelSummarizingAcyclic { .. } => Vec::new(),
        }
    }

    /// Project this termination certificate into a [`gmeow_errors::Finding`] — the
    /// certificate class AND its evidence as a first-class surfaced diagnostic, reusing the
    /// canonical Finding machinery the divergence ledger uses, never an internal boolean.
    ///
    /// A [`Self::WeaklyAcyclic`] certificate is an informational finding carrying its proof
    /// evidence; an [`Self::Uncertified`] one is an error finding carrying the joined
    /// weak-acyclicity violations.
    pub fn to_finding(&self) -> Finding {
        match self {
            Self::WeaklyAcyclic { evidence } => Finding::new(
                Severity::Info,
                "chase.certificate.weakly-acyclic".to_owned(),
                evidence.clone(),
            )
            .with_tool("chase"),
            Self::JointlyAcyclic { evidence } => Finding::new(
                Severity::Info,
                "chase.certificate.jointly-acyclic".to_owned(),
                evidence.clone(),
            )
            .with_tool("chase"),
            Self::SuperWeaklyAcyclic { evidence } => Finding::new(
                Severity::Info,
                "chase.certificate.super-weakly-acyclic".to_owned(),
                evidence.clone(),
            )
            .with_tool("chase"),
            Self::ModelSummarizingAcyclic { evidence } => Finding::new(
                Severity::Info,
                "chase.certificate.model-summarizing-acyclic".to_owned(),
                evidence.clone(),
            )
            .with_tool("chase"),
            Self::Uncertified { violations } => Finding::new(
                Severity::Error,
                "chase.certificate.uncertified".to_owned(),
                violations.join("; "),
            )
            .with_tool("chase"),
        }
    }

    /// The **escalation-cost** rank — explicit, NOT a derived `Ord`, and NOT a
    /// certificate-*strength* total order: it is the cheapest-first probing order
    /// [`Self::certify`] escalates through (Uncertified < WA < JA < SWA < MSA by *cost*).
    /// Certificate strength is only a PARTIAL order — JA and SWA are incomparable siblings
    /// (neither certifies a superset of the other), so `rank` must not be used to decide
    /// which of two certificates is stronger; the certificate-strength meet lives in
    /// [`Self::meet_certified`] / [`Self::combine`].
    fn rank(&self) -> u8 {
        match self {
            Self::Uncertified { .. } => 0,
            Self::WeaklyAcyclic { .. } => 1,
            Self::JointlyAcyclic { .. } => 2,
            Self::SuperWeaklyAcyclic { .. } => 3,
            Self::ModelSummarizingAcyclic { .. } => 4,
        }
    }

    /// The human-readable proof evidence a certified certificate carries (`None` for
    /// `Uncertified`, which carries violations instead).
    fn evidence(&self) -> Option<&str> {
        match self {
            Self::WeaklyAcyclic { evidence }
            | Self::JointlyAcyclic { evidence }
            | Self::SuperWeaklyAcyclic { evidence }
            | Self::ModelSummarizingAcyclic { evidence } => Some(evidence),
            Self::Uncertified { .. } => None,
        }
    }

    /// The greatest lower bound of two **certified** classes in the certificate-strength
    /// poset `WA ⊏ {JA ∥ SWA} ⊏ MSA`. The incomparable siblings JA and SWA meet to their
    /// glb, `WeaklyAcyclic` — never a linearization to whichever the escalation probed
    /// first; every other pair is comparable and meets to the lower (more conservative)
    /// class by escalation rank.
    fn meet_certified(lhs: Self, rhs: Self) -> Self {
        match (&lhs, &rhs) {
            (Self::JointlyAcyclic { .. }, Self::SuperWeaklyAcyclic { .. })
            | (Self::SuperWeaklyAcyclic { .. }, Self::JointlyAcyclic { .. }) => {
                Self::WeaklyAcyclic {
                    evidence: format!(
                        "meet of incomparable certificates [{}] and [{}] → greatest lower bound \
                     weakly-acyclic",
                        lhs.evidence().unwrap_or_default(),
                        rhs.evidence().unwrap_or_default()
                    ),
                }
            }
            _ if lhs.rank() <= rhs.rank() => lhs,
            _ => rhs,
        }
    }

    /// The certificate-strength lattice **meet** — NOT a recertification of the rule-set
    /// union (chase termination is not compositional under union; a caller needing a union
    /// certificate must recertify the combined rules). A program is admitted only when
    /// every part is, so an `Uncertified` part forces the whole to `Uncertified` (keeping
    /// EVERY violation, merged/sorted/deduped, so no termination-failure diagnostic is lost
    /// to the meet); two certified parts meet to their greatest lower bound
    /// ([`Self::meet_certified`]) — in particular the incomparable JA∥SWA pair meets to
    /// `WeaklyAcyclic`, never to whichever sibling was probed first.
    pub(crate) fn combine(self, other: Self) -> Self {
        match (self, other) {
            (
                Self::Uncertified {
                    violations: mut merged,
                },
                Self::Uncertified { violations: rhs },
            ) => {
                merged.extend(rhs);
                merged.sort();
                merged.dedup();
                Self::Uncertified { violations: merged }
            }
            // Any Uncertified part forces the whole to Uncertified, keeping its violations.
            (u @ Self::Uncertified { .. }, _) | (_, u @ Self::Uncertified { .. }) => u,
            // Two certified classes: the greatest lower bound in the strength poset.
            (lhs, rhs) => Self::meet_certified(lhs, rhs),
        }
    }
}

/// The certify→(chase | refuse/budget) DECISION, authored ONCE and shared by both forward
/// entry points ([`route_chase`], single-world, and [`chase_materialize`], multi-world):
/// the native chase may run iff the program is certified terminating OR a step budget
/// bounds it (an uncertified program stays incomplete-never-wrong under the governor).
/// A certified-negative, unbudgeted program is `Unsupported` rather than looped. Only
/// this DECISION is unified; each entry point keeps its own chase scoping.
fn admits_or_budgeted(admission: &ChaseAdmission, max_steps: Option<u64>) -> bool {
    admission.admits_native() || max_steps.is_some()
}

/// Route an existential-rule program over ONE world: certify termination, then chase or
/// refuse.  The single-world sibling of [`chase_materialize`]; both share the
/// [`admits_or_budgeted`] decision, differing only in chase scope (one world here, a
/// governed sweep of all worlds there).
///
/// The decision is a deterministic function of the certificate and the declared budget
/// (never a runtime knob):
/// - **Certified** (`WeaklyAcyclic`) ⇒ run the native chase.  A declared budget still
///   applies (it just never trips on a terminating program).
/// - **Uncertified** with a budget ⇒ run the chase **budgeted-partial** — the budget
///   governor caps it, returning an incomplete-never-wrong prefix.
/// - **Uncertified** with no budget ⇒ `Unsupported(NonTerminatingExistential)`, refusing
///   the program rather than looping.
///
/// [`chase_materialize`] is the multi-world sibling used by the production forward
/// path; this router exposes the same admission decision for one-world callers.
///
/// # Errors
///
/// Propagates grounding/​provenance failures from [`chase_world`].
pub(crate) fn route_chase(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
) -> gmeow_errors::Result<(ChaseAdmission, ChaseOutcome)> {
    let admission = ChaseAdmission::certify(rules);
    let outcome = if admits_or_budgeted(&admission, max_steps) {
        chase_world(world, edb_facts, rules, max_steps)?
    } else {
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    };
    Ok((admission, outcome))
}

/// The production-DL sibling of [`route_chase`] that preserves one witness registry
/// across alternating fixed-point rounds.
pub(crate) fn route_chase_with_registry(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    max_steps: Option<u64>,
    registry: &mut SkolemRegistry,
) -> gmeow_errors::Result<(ChaseAdmission, ChaseOutcome)> {
    let admission = ChaseAdmission::certify(rules);
    let outcome = if admits_or_budgeted(&admission, max_steps) {
        chase_world_with_registry(world, edb_facts, rules, max_steps, registry)?
    } else {
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    };
    Ok((admission, outcome))
}

/// The production-DL router that keeps [`route_chase_with_registry`]'s admission
/// decision but always caps a CERTIFIED program with a hard step **backstop**.
///
/// Weak/joint acyclicity certifies *termination*, not *tractable size*: a certified
/// program can still materialize a super-polynomial (up to double-exponential) model,
/// and running it unbudgeted exhausts memory rather than deciding. This sibling runs a
/// certified program under `backstop_steps` so such a case returns
/// [`BudgetStatus::Exhausted`] (incomplete-never-wrong) instead of OOMing. An
/// uncertified program still refuses with `Unsupported` exactly as
/// [`route_chase_with_registry`] does with no budget — the backstop is a memory
/// ceiling on the terminating path, never a licence to run a non-terminating one.
pub(crate) fn route_chase_with_registry_backstopped(
    world: &str,
    edb_facts: &[Fact],
    rules: &[ExistentialRule],
    backstop_steps: u64,
    registry: &mut SkolemRegistry,
) -> gmeow_errors::Result<(ChaseAdmission, ChaseOutcome)> {
    let admission = ChaseAdmission::certify(rules);
    let outcome = if admission.admits_native() {
        chase_world_with_registry(world, edb_facts, rules, Some(backstop_steps), registry)?
    } else {
        NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
    };
    Ok((admission, outcome))
}

/// Connect wildcard and constant refinements of the same `(predicate, slot)` when BOTH
/// occur — a conservative over-approximation (a wildcard-typed null could be any class,
/// and a wildcard consumer reads any class), so reachability is never under-counted.
fn add_wildcard_subsumption(
    adj: &mut std::collections::BTreeMap<Position, BTreeSet<Position>>,
    nodes: &BTreeSet<Position>,
) {
    use std::collections::BTreeMap;
    // Group nodes by (predicate, slot).
    let mut groups: BTreeMap<(String, Slot), (Vec<Position>, bool)> = BTreeMap::new();
    for n in nodes {
        let entry = groups.entry((n.predicate.clone(), n.slot)).or_default();
        if n.class == ClassKey::Wildcard {
            entry.1 = true;
        } else {
            entry.0.push(n.clone());
        }
    }
    for ((predicate, slot), (consts, has_wildcard)) in groups {
        if !has_wildcard || consts.is_empty() {
            continue; // refinement stays precise unless both a wildcard and consts occur
        }
        let wildcard = Position {
            predicate,
            slot,
            class: ClassKey::Wildcard,
        };
        for c in consts {
            adj.entry(c.clone()).or_default().insert(wildcard.clone());
            adj.entry(wildcard.clone()).or_default().insert(c);
        }
    }
}

/// Whether `to` is reachable from `from` in `adj` (BFS over ≥1 edges; a self-edge on
/// `from` therefore counts).
fn reaches(
    adj: &std::collections::BTreeMap<Position, BTreeSet<Position>>,
    from: &Position,
    to: &Position,
) -> bool {
    let mut stack: Vec<&Position> = adj.get(from).into_iter().flatten().collect();
    let mut seen: BTreeSet<&Position> = BTreeSet::new();
    while let Some(node) = stack.pop() {
        if node == to {
            return true;
        }
        if !seen.insert(node) {
            continue;
        }
        if let Some(succs) = adj.get(node) {
            stack.extend(succs.iter());
        }
    }
    false
}

// ── Joint-acyclicity support: null-flow `Move` sets over refined positions ──────────

/// Every refined position occurring anywhere in `rules` (body or head).  The universe
/// for conservative wildcard/const linkage in [`move_contains`].
fn all_program_positions(rules: &[ExistentialRule]) -> BTreeSet<Position> {
    let mut universe = BTreeSet::new();
    for r in rules {
        let vars: BTreeSet<String> = r.body_vars().into_iter().chain(r.head_vars()).collect();
        for v in &vars {
            universe.extend(refined_positions(&r.body, v));
            universe.extend(refined_positions(&r.head, v));
        }
    }
    universe
}

/// Conservative Move membership: `p ∈ mv`, OR — when the program has BOTH a wildcard and
/// a constant refinement for `p`'s `(predicate, slot)` — any sibling of `p` at that
/// `(predicate, slot)` is in `mv`.  This over-approximates a null's reach (wildcard nulls
/// could be any class, constant consumers read any class), never under — so a real
/// existential cycle is never hidden (soundness: JA never wrongly certifies).
fn move_contains(mv: &BTreeSet<Position>, p: &Position, universe: &BTreeSet<Position>) -> bool {
    if mv.contains(p) {
        return true;
    }
    let same_slot = |q: &Position| q.predicate == p.predicate && q.slot == p.slot;
    let has_wildcard = universe
        .iter()
        .any(|q| same_slot(q) && q.class == ClassKey::Wildcard);
    let has_const = universe
        .iter()
        .any(|q| same_slot(q) && matches!(q.class, ClassKey::Const(_)));
    has_wildcard && has_const && mv.iter().any(same_slot)
}

/// The `Move` set of existential `e` (of `rule_i`): the least set of refined positions a
/// null minted for `e` can occupy, closing null-flow through every rule's frontier
/// variables (a frontier `v` whose refined body positions all lie within Move carries the
/// null to `v`'s head positions).  Grows monotonically within the finite position
/// universe, so the fixpoint terminates.
fn move_set(
    precomputed_flows: &[Vec<(BTreeSet<Position>, Vec<Position>)>],
    rule_i: &ExistentialRule,
    e: &str,
    universe: &BTreeSet<Position>,
) -> BTreeSet<Position> {
    let mut mv: BTreeSet<Position> = refined_positions(&rule_i.head, e).into_iter().collect();
    loop {
        let before = mv.len();
        for rule_flow in precomputed_flows {
            for (bpos, hpos) in rule_flow {
                if !bpos.is_empty() && bpos.iter().all(|p| move_contains(&mv, p, universe)) {
                    mv.extend(hpos.iter().cloned());
                }
            }
        }
        if mv.len() == before {
            break;
        }
    }
    mv
}

/// Whether `node` lies on a cycle in the existential-dependency graph (reaches itself
/// over ≥1 edges; a self-edge therefore counts).
fn node_reaches_self(
    edges: &std::collections::BTreeMap<usize, BTreeSet<usize>>,
    node: usize,
) -> bool {
    let mut stack: Vec<usize> = edges.get(&node).into_iter().flatten().copied().collect();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == node {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(succ) = edges.get(&n) {
            stack.extend(succ.iter().copied());
        }
    }
    false
}

/// Count native dependency edges and detect an MSA obstruction without rendering terms.
fn msa_dependency_summary(facts: &[Fact], predicate: &str) -> (usize, bool) {
    let mut dependency: std::collections::BTreeMap<purrdf::TermValue, BTreeSet<purrdf::TermValue>> =
        std::collections::BTreeMap::new();
    for fact in facts.iter().filter(|fact| fact.predicate == predicate) {
        dependency
            .entry(fact.subject.clone())
            .or_default()
            .insert(fact.object.clone());
    }
    let edges = dependency.values().map(BTreeSet::len).sum();
    let cyclic = dependency
        .keys()
        .any(|node| msa_null_reaches_self(&dependency, node));
    (edges, cyclic)
}

fn msa_null_reaches_self(
    dep: &std::collections::BTreeMap<purrdf::TermValue, BTreeSet<purrdf::TermValue>>,
    node: &purrdf::TermValue,
) -> bool {
    let mut stack: Vec<&purrdf::TermValue> = dep.get(node).into_iter().flatten().collect();
    let mut seen: BTreeSet<&purrdf::TermValue> = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == node {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(succ) = dep.get(n) {
            stack.extend(succ.iter());
        }
    }
    false
}

// ── Super-weak-acyclicity support: the Skolemized place graph with unification ──────

/// A term in the super-weak-acyclicity analysis: a rule variable, a constant surface, or
/// a Skolem functional term `f_{rule,∃}(frontier…)` standing for an invented null.  The
/// functional structure is what lets the occurs-check refuse `f(x) = x`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SwaTerm {
    Var(String),
    Const(String),
    Skolem(String, Vec<SwaTerm>),
}

impl SwaTerm {
    /// Prefix every variable name (recursing through Skolem arguments) so a producer
    /// rule's variables and a consumer rule's variables live in disjoint scopes for one
    /// atom-pair unification.  Skolem function names already carry their rule IRI, so two
    /// distinct rules' nulls never share a function symbol.
    fn scoped(&self, tag: &str) -> SwaTerm {
        match self {
            SwaTerm::Var(v) => SwaTerm::Var(format!("{tag}{v}")),
            SwaTerm::Const(c) => SwaTerm::Const(c.clone()),
            SwaTerm::Skolem(f, args) => {
                SwaTerm::Skolem(f.clone(), args.iter().map(|a| a.scoped(tag)).collect())
            }
        }
    }
}

/// The Skolem-analysis view of an [`EvalTerm`]: existential head vars become Skolem
/// terms over the rule frontier; frontier vars stay variables; constants stay constants.
fn swa_term(
    t: &EvalTerm,
    rule_ordinal: usize,
    existentials: &BTreeSet<String>,
    frontier: &[SwaTerm],
) -> SwaTerm {
    match t {
        EvalTerm::Var(v) if existentials.contains(v) => {
            SwaTerm::Skolem(format!("{rule_ordinal}#{v}"), frontier.to_vec())
        }
        EvalTerm::Var(v) => SwaTerm::Var(v.clone()),
        EvalTerm::ConstNamed(iri) => SwaTerm::Const(format!("<{iri}>")),
        EvalTerm::ConstLit(t) => SwaTerm::Const(term_display(t)),
    }
}

/// Resolve `t` through the substitution to its current representative (walking variable
/// bindings).
fn swa_resolve(t: &SwaTerm, subst: &std::collections::BTreeMap<String, SwaTerm>) -> SwaTerm {
    let mut cur = t.clone();
    while let SwaTerm::Var(v) = &cur {
        match subst.get(v) {
            Some(next) => cur = next.clone(),
            None => break,
        }
    }
    cur
}

/// Occurs-check: does variable `v` occur inside `t` (after resolution)?  This is what
/// refuses `f(x) = x` — the heart of the unification precision.
fn swa_occurs(v: &str, t: &SwaTerm, subst: &std::collections::BTreeMap<String, SwaTerm>) -> bool {
    match swa_resolve(t, subst) {
        SwaTerm::Var(w) => w == v,
        SwaTerm::Const(_) => false,
        SwaTerm::Skolem(_, args) => args.iter().any(|a| swa_occurs(v, a, subst)),
    }
}

/// Most-general-unifier step for two terms under `subst`; returns `false` on clash.
fn swa_unify_term(
    a: &SwaTerm,
    b: &SwaTerm,
    subst: &mut std::collections::BTreeMap<String, SwaTerm>,
) -> bool {
    let a = swa_resolve(a, subst);
    let b = swa_resolve(b, subst);
    match (&a, &b) {
        (SwaTerm::Var(x), SwaTerm::Var(y)) if x == y => true,
        (SwaTerm::Var(x), other) | (other, SwaTerm::Var(x)) => {
            if swa_occurs(x, other, subst) {
                return false;
            }
            subst.insert(x.clone(), other.clone());
            true
        }
        (SwaTerm::Const(c1), SwaTerm::Const(c2)) => c1 == c2,
        (SwaTerm::Skolem(f1, a1), SwaTerm::Skolem(f2, a2)) => {
            f1 == f2
                && a1.len() == a2.len()
                && a1.iter().zip(a2).all(|(x, y)| swa_unify_term(x, y, subst))
        }
        _ => false,
    }
}

/// Whether the producer head atom (Skolemized, scoped `P!`) unifies with the consumer
/// body atom (scoped `C!`) — same predicate and a most-general unifier with occurs-check.
fn swa_atoms_unify(
    pred_p: &str,
    subj_p: &SwaTerm,
    obj_p: &SwaTerm,
    pred_c: &str,
    subj_c: &SwaTerm,
    obj_c: &SwaTerm,
) -> bool {
    if pred_p != pred_c {
        return false;
    }
    let mut subst = std::collections::BTreeMap::new();
    // Terms arrive PRE-SCOPED by role (`build_swa_place_graph` scopes producer heads `P!`
    // and consumer bodies `C!`), so the producer/consumer variable scopes are already
    // disjoint here — no per-attempt re-scoping.
    swa_unify_term(subj_p, subj_c, &mut subst) && swa_unify_term(obj_p, obj_c, &mut subst)
}

/// One (predicate, subject, object) atom in the Skolem-analysis view, tagged with the
/// place ids of its two slots for wiring the flow graph.
struct SwaAtom {
    predicate: String,
    subject: SwaTerm,
    object: SwaTerm,
    subject_place: usize,
    object_place: usize,
}

/// Build the Skolemized place graph.  Returns `(place_count, flow_edges, existential_out)`
/// where an edge `p → q` means a value at place `p` can flow to place `q`, and
/// `existential_out` are the head places holding an invented null.  SWA holds iff no
/// existential output place lies on a cycle.
fn build_swa_place_graph(
    rules: &[ExistentialRule],
) -> (
    usize,
    std::collections::BTreeMap<usize, BTreeSet<usize>>,
    Vec<usize>,
) {
    let mut next_place = 0usize;
    let mut edges: std::collections::BTreeMap<usize, BTreeSet<usize>> =
        std::collections::BTreeMap::new();
    let mut existential_out: Vec<usize> = Vec::new();
    // Per-rule Skolem-view atoms, split into body and head, for the cross-rule pass.
    let mut body_atoms: Vec<Vec<SwaAtom>> = Vec::with_capacity(rules.len());
    let mut head_atoms: Vec<Vec<SwaAtom>> = Vec::with_capacity(rules.len());

    for (rule_ordinal, rule) in rules.iter().enumerate() {
        let existentials: BTreeSet<String> = rule.existentials().into_iter().collect();
        let frontier: Vec<SwaTerm> = rule.frontier_vars().into_iter().map(SwaTerm::Var).collect();
        let frontier_set: BTreeSet<String> = rule.frontier_vars().into_iter().collect();
        // Variable → (body places, head places) for within-rule frontier flow.
        let mut var_body: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut var_head: std::collections::BTreeMap<String, Vec<usize>> =
            std::collections::BTreeMap::new();
        let mut frontier_body_places: Vec<usize> = Vec::new();

        let mut mk_atoms = |atoms: &[EvalAtom], is_head: bool| -> Vec<SwaAtom> {
            let mut out = Vec::with_capacity(atoms.len());
            for atom in atoms {
                let subj = swa_term(&atom.subject, rule_ordinal, &existentials, &frontier);
                let obj = swa_term(&atom.object, rule_ordinal, &existentials, &frontier);
                let sp = next_place;
                let op = next_place + 1;
                next_place += 2;
                for (term, place) in [(&subj, sp), (&obj, op)] {
                    match term {
                        SwaTerm::Var(v) => {
                            if is_head {
                                var_head.entry(v.clone()).or_default().push(place);
                            } else {
                                var_body.entry(v.clone()).or_default().push(place);
                                if frontier_set.contains(v) {
                                    frontier_body_places.push(place);
                                }
                            }
                        }
                        SwaTerm::Skolem(..) if is_head => existential_out.push(place),
                        _ => {}
                    }
                }
                // Pre-scope the stored terms by role ONCE (producer heads `P!`, consumer
                // bodies `C!`) so cross-rule unification never re-scopes per attempt. The
                // within-rule frontier flow above keys on the UNSCOPED `subj`/`obj`, so it
                // is unaffected.
                let tag = if is_head { "P!" } else { "C!" };
                out.push(SwaAtom {
                    predicate: atom.predicate.clone(),
                    subject: subj.scoped(tag),
                    object: obj.scoped(tag),
                    subject_place: sp,
                    object_place: op,
                });
            }
            out
        };

        let b = mk_atoms(&rule.body, false);
        let h = mk_atoms(&rule.head, true);

        // Within-rule frontier flow: a frontier variable carries its body value to its
        // head occurrences.
        for (v, body_places) in &var_body {
            if let Some(head_places) = var_head.get(v) {
                for &bp in body_places {
                    for &hp in head_places {
                        edges.entry(bp).or_default().insert(hp);
                    }
                }
            }
        }
        // Special flow: every frontier body place feeds every existential head place this
        // rule introduces (the fresh null depends on the frontier binding).
        let rule_existential_head_places: Vec<usize> = h
            .iter()
            .flat_map(|a| {
                let mut v = Vec::new();
                if matches!(a.subject, SwaTerm::Skolem(..)) {
                    v.push(a.subject_place);
                }
                if matches!(a.object, SwaTerm::Skolem(..)) {
                    v.push(a.object_place);
                }
                v
            })
            .collect();
        for &bp in &frontier_body_places {
            for &ep in &rule_existential_head_places {
                edges.entry(bp).or_default().insert(ep);
            }
        }

        body_atoms.push(b);
        head_atoms.push(h);
    }

    // Cross-rule flow: a producer head atom feeds a consumer body atom only when the
    // Skolemized atoms unify (MGU with occurs-check).  Group consumer body atoms by
    // predicate so each producer only probes same-predicate candidates — swa_atoms_unify
    // rejects a predicate mismatch immediately anyway, so this is O(H·B) → O(H·B_p) with
    // identical edges (edges is a set, so candidate order is irrelevant to the result).
    let mut body_by_pred: std::collections::BTreeMap<&str, Vec<&SwaAtom>> =
        std::collections::BTreeMap::new();
    for consumer in &body_atoms {
        for b in consumer {
            body_by_pred
                .entry(b.predicate.as_str())
                .or_default()
                .push(b);
        }
    }
    for producer in &head_atoms {
        for a in producer {
            let Some(candidates) = body_by_pred.get(a.predicate.as_str()) else {
                continue;
            };
            for b in candidates {
                if swa_atoms_unify(
                    &a.predicate,
                    &a.subject,
                    &a.object,
                    &b.predicate,
                    &b.subject,
                    &b.object,
                ) {
                    edges
                        .entry(a.subject_place)
                        .or_default()
                        .insert(b.subject_place);
                    edges
                        .entry(a.object_place)
                        .or_default()
                        .insert(b.object_place);
                }
            }
        }
    }

    (next_place, edges, existential_out)
}

#[path = "chase.tests.rs"]
#[cfg(test)]
mod tests;
