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

use std::collections::BTreeSet;

use gmeow_errors::{Finding, Severity};

use crate::physical::cursor::LendingIterator;
use crate::physical::seminaive::{
    Budgeted, NativeOutcome, StepGovernor, StrataProgress, UnsupportedKind,
};
use crate::physical::store::{Bound, RelationStore, SkolemRegistry, SkolemTerm};
use crate::provenance::{
    MinProofHeightSemiring, ProofHeight, mint_derivation_id, mint_nary_reifier, term_display,
};
use crate::rule_ir::{
    DerivedRow, EvalAtom, EvalTerm, Fact, FactKey, Solution, distinct_pairs_satisfied,
    echo_asserted, ground, ground_head, match_atom, sort_rows,
};
use crate::seam::BudgetStatus;

/// Wrap a physical-chase condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn physical_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Physical { detail })
}

/// A chase attempt's outcome: a decided budgeted derivation, or a declared gap.
pub(crate) type ChaseOutcome = NativeOutcome<Budgeted<Vec<DerivedRow>>>;

/// How an existential rule addresses witnesses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WitnessPolicy {
    /// Standard frontier-Skolem witnesses for general existential TGDs.
    FrontierSkolem,
    /// DL tableau blocking: preserve a distinct witness per root binding, then
    /// close recursive same-rule/ordinal obligations on the nearest ancestor.
    DlAncestorBlocking,
}

/// One not-yet-committed chase row and the provenance needed to publish it.
struct PendingRow {
    key: FactKey,
    fact: Fact,
    source_quad_ids: Vec<String>,
    antecedents: Vec<Fact>,
    rule_iri: String,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExistentialRule {
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

/// Join `atoms` against `rel` starting from `seed`, returning every extension.
///
/// A full (non-delta) index-selected conjunctive join: each atom computes a [`Bound`]
/// from the partial solution and scans only the matching rows via
/// [`RelationStore::select`], merging via [`match_atom`] (so a repeated variable must
/// agree and a constant must equal the fact surface).  Used both for the body frontier
/// join and for the restricted-chase head-satisfaction probe.
fn join_atoms(atoms: &[EvalAtom], rel: &RelationStore, seed: &Solution) -> Vec<Solution> {
    let interner = rel.interner();
    let mut solutions = vec![seed.clone()];
    for atom in atoms {
        let mut next: Vec<Solution> = Vec::new();
        for sol in &solutions {
            let subj = ground(&atom.subject, sol);
            let obj = ground(&atom.object, sol);
            let Some(bound) = atom_bound(rel, subj.as_deref(), obj.as_deref()) else {
                continue; // a bound term the store has never seen matches nothing
            };
            // Drive the lending cursor directly (no eager `Vec`): each `next()` yields
            // one id row (the delta-probe RowId is ignored by the chase) in row-id order.
            let mut cursor = rel.select(atom.predicate.as_str(), bound);
            while let Some((s_id, o_id, _row)) = cursor.next() {
                // Resolve the term ids to `TermValue` surfaces here.
                let f = Fact {
                    subject: interner.resolve(s_id).clone(),
                    predicate: atom.predicate.clone(),
                    object: interner.resolve(o_id).clone(),
                };
                if let Some(mut merged) = match_atom(atom, &f, sol) {
                    merged.source_facts.push(f);
                    next.push(merged);
                }
            }
        }
        solutions = next;
        if solutions.is_empty() {
            break;
        }
    }
    solutions
}

/// The selection [`Bound`] for a `(subject, object)` pair of ground surfaces.
///
/// `None` means a bound position's term has never entered `rel`, so no row can match.
fn atom_bound(rel: &RelationStore, subj: Option<&str>, obj: Option<&str>) -> Option<Bound> {
    Some(match (subj, obj) {
        (Some(s), Some(o)) => Bound::Both(rel.term_id(s)?, rel.term_id(o)?),
        (Some(s), None) => Bound::Subject(rel.term_id(s)?),
        (None, Some(o)) => Bound::Object(rel.term_id(o)?),
        (None, None) => Bound::Any,
    })
}

/// Whether the head is ALREADY satisfied under the frontier binding `sol`.
///
/// The restricted-chase blocking condition: does the store already contain an extension
/// of `sol` to the existential variables making every head atom true — with the
/// existential/​distinct inequalities honored?  If so the firing is skipped.  Realized
/// as a conjunctive query over the head atoms (existentials free), filtered by the
/// distinct guards; any surviving solution means satisfied.
fn head_satisfied(
    rule: &ExistentialRule,
    sol: &Solution,
    rel: &RelationStore,
) -> gmeow_errors::Result<bool> {
    for candidate in join_atoms(&rule.head, rel, sol) {
        if distinct_pairs_satisfied(&rule.distinct, &candidate)? {
            return Ok(true);
        }
    }
    Ok(false)
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
/// sorted worlds — witness IRIs are content-addressed on rule+frontier, world-independent,
/// so a shared registry only dedups the recipe map, never changes a fact).
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
    let prepared: Vec<(&ExistentialRule, Vec<String>, Vec<String>)> = rules
        .iter()
        .map(|rule| (rule, rule.existentials(), rule.frontier_vars()))
        .collect();

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
        let mut round = Vec::new();
        for (rule, existentials, frontier_vars) in &prepared {
            for sol in join_atoms(&rule.body, &store, &empty_solution()) {
                // The rule's `distinct` guards range over the EXISTENTIAL head vars
                // (the `≥n` distinctness), which are unbound in the body solution.  They
                // are enforced two ways: `head_satisfied` applies them to store
                // candidates (so `≥n` blocks only on n distinct existing witnesses), and
                // distinct existential ordinals mint distinct witnesses on a firing (so
                // the invented facts satisfy them by construction).
                //
                // Restricted-chase satisfaction: skip if the head already holds.
                if head_satisfied(rule, &sol, &store)? {
                    continue;
                }
                // Invent one witness per existential var (distinct ordinals ⇒ distinct
                // witnesses), addressed on the bound frontier values.
                let frontier: Vec<_> = frontier_vars
                    .iter()
                    .map(|v| bound_value(&sol, v))
                    .collect::<Result<_, _>>()?;
                let mut extended = sol.clone();
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
                let reifier_groups = reified_nary_head_groups(rule)?;
                if reifier_groups.is_empty() {
                    // No reified n-ary tuple in the head: every existential is a genuine value
                    // witness (DL `∃p.D`, `≥n p.D`, …), frontier-addressed `SkolemTerm`.
                    for (ordinal, evar) in existentials.iter().enumerate() {
                        let witness = mint_witness(rule, registry, ordinal, &frontier);
                        extended
                            .bindings
                            .push((evar.clone(), term_display(&witness)));
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
                        let witness = mint_witness(rule, registry, ordinal, &frontier);
                        extended
                            .bindings
                            .push((evar.clone(), term_display(&witness)));
                        ordinal += 1;
                    }
                    for (reifier_var, rel, arg_terms) in &reifier_groups {
                        let mut arg_values = Vec::with_capacity(arg_terms.len());
                        for t in arg_terms {
                            arg_values.push(eval_term_value(t, &extended)?);
                        }
                        let witness_iri = mint_nary_reifier(rel, &arg_values)?;
                        extended.bindings.push((
                            reifier_var.clone(),
                            term_display(&purrdf::TermValue::iri(witness_iri)),
                        ));
                    }
                }
                // Ground every head atom; each becomes a candidate new fact.
                let sources = reifiers_of(&sol)?;
                for hatom in &rule.head {
                    let fact = ground_head(hatom, &extended)?;
                    round.push(PendingRow {
                        key: fact.key(),
                        fact,
                        source_quad_ids: sources.clone(),
                        antecedents: sol.source_facts.clone(),
                        rule_iri: rule.rule_iri.clone(),
                    });
                }
            }
        }

        // Commit in FactKey-sorted order, deduped against what is already known.
        round.sort_by(|left, right| left.key.cmp(&right.key));
        let mut progressed = false;
        for PendingRow {
            key,
            fact,
            source_quad_ids,
            antecedents,
            rule_iri,
        } in round
        {
            if committed.contains(&key) {
                continue;
            }
            if governor.spent() {
                status = BudgetStatus::Exhausted;
                break 'fixpoint;
            }
            let src_refs: Vec<&str> = source_quad_ids.iter().map(String::as_str).collect();
            let derivation_id = mint_derivation_id(&rule_iri, &src_refs);
            store.insert(&fact.predicate, &fact.subject, &fact.object);
            out.push(DerivedRow {
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
    // ONE registry spans the sorted worlds (witness IRIs are world-independent), so any
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
    let surface = sol.get(var).ok_or_else(|| {
        physical_err(format!(
            "chase: frontier variable {var:?} unbound after body join"
        ))
    })?;
    crate::rule_ir::surface_to_value(surface)
}

fn mint_witness(
    rule: &ExistentialRule,
    registry: &mut SkolemRegistry,
    ordinal: usize,
    frontier: &[purrdf::TermValue],
) -> purrdf::TermValue {
    let recipe = SkolemTerm {
        rule_iri: rule.rule_iri.clone(),
        ordinal,
        frontier: frontier.to_vec(),
    };
    match rule.witness_policy {
        WitnessPolicy::FrontierSkolem => registry.mint(recipe),
        WitnessPolicy::DlAncestorBlocking => registry.mint_dl_blocked(recipe),
    }
}

/// The reifier IRIs of a solution's matched body facts, in body order.
fn reifiers_of(sol: &Solution) -> gmeow_errors::Result<Vec<String>> {
    sol.source_facts.iter().map(Fact::reifier).collect()
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
    // A head carries the reified vocabulary iff at least one atom is `instanceOf` or a
    // positional `naryArg{i}`. If none do, this is not a reified-n-ary head (a DL witness
    // head) and every existential stays on the default `SkolemTerm` path.
    let uses_reified_vocab = rule
        .head
        .iter()
        .any(|atom| atom.predicate == instance_of || nary_arg_index(&atom.predicate).is_some());
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
/// constant directly, or a variable's bound surface resolved back to a value (a hard error if
/// the variable is unbound — a range-restricted head argument is bound by the body by
/// construction).
fn eval_term_value(term: &EvalTerm, sol: &Solution) -> gmeow_errors::Result<purrdf::TermValue> {
    match term {
        EvalTerm::ConstNamed(iri) => Ok(purrdf::TermValue::iri(iri)),
        EvalTerm::ConstLit(value) => Ok(value.clone()),
        EvalTerm::Var(name) => {
            let surface = sol.get(name).ok_or_else(|| {
                physical_err(format!(
                    "chase: n-ary head argument variable {name:?} unbound after body join"
                ))
            })?;
            crate::rule_ir::surface_to_value(surface)
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
#[derive(Debug, Clone, PartialEq, Eq)]
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

impl ChaseAdmission {
    /// Certify `rules` by the termination-class ladder: escalate cheapest-first
    /// (weak → joint → super-weak → model-summarizing acyclicity) and report the
    /// **least-cost sufficient** certificate. The polynomial rungs run before the
    /// EXPTIME model-summarizing check, which is reached only when the structural
    /// rungs all refuse. When no class certifies, return `Uncertified` carrying the
    /// weak-acyclicity position-graph violations (the canonical diagnostic).
    pub(crate) fn certify(rules: &[ExistentialRule]) -> Self {
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
        let moves: Vec<BTreeSet<Position>> = existentials
            .iter()
            .map(|(i, e)| move_set(rules, &rules[*i], e, &universe))
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
        let mut predicates: BTreeSet<String> = BTreeSet::new();
        let mut domain_keys: BTreeSet<String> = BTreeSet::new();
        let mut domain_terms: Vec<purrdf::TermValue> = Vec::new();
        {
            let star = purrdf::TermValue::iri(STAR);
            domain_keys.insert(term_display(&star));
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
                        && domain_keys.insert(term_display(&tv))
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
                    &format!("{}::msa-prod::{k}", r.rule_iri),
                    head_atom,
                    r.body.clone(),
                ));
            }
            // Dependency: a summarizing null binding a frontier position of `r` depends
            // into every null `r` mints.
            for v in r.frontier_vars() {
                for e in r.existentials() {
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
                        &format!("{}::msa-dep::{v}::{e}", r.rule_iri),
                        head_atom,
                        body,
                    ));
                }
            }
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

        // Run the engine's own fixpoint. A non-stratifiable program or a budget/refusal
        // is a conservative `None` (the ladder falls through to `Uncertified` → budget) —
        // never a silent admit.
        let executable =
            crate::physical::plan::compile_cached("gmeow-msa-critical-v1", program).executable?;
        let facts = match crate::physical::seminaive::evaluate(store, executable.as_ref(), None) {
            Ok(crate::physical::NativeOutcome::Decided(budgeted)) => budgeted.rows,
            _ => return None,
        };

        // The materialized dependency relation; MSA holds iff no null reaches itself.
        let mut dep: std::collections::BTreeMap<String, BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let mut edge_count = 0usize;
        for f in &facts {
            if f.predicate == DEP
                && dep
                    .entry(term_display(&f.subject))
                    .or_default()
                    .insert(term_display(&f.object))
            {
                edge_count += 1;
            }
        }
        let cyclic = dep.keys().any(|n| msa_null_reaches_self(&dep, n));
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

    /// The certified-strength rank — explicit, NOT a derived `Ord`. Higher = broader
    /// certified-terminating class on the ladder.
    fn rank(&self) -> u8 {
        match self {
            Self::Uncertified { .. } => 0,
            Self::WeaklyAcyclic { .. } => 1,
            Self::JointlyAcyclic { .. } => 2,
            Self::SuperWeaklyAcyclic { .. } => 3,
            Self::ModelSummarizingAcyclic { .. } => 4,
        }
    }

    /// The lattice meet toward `Uncertified`: a program is admitted only when every
    /// part is, so combining two admissions keeps the weaker (lower-ranked) one — and
    /// when both parts are `Uncertified`, keeps EVERY violation so no termination-failure
    /// diagnostic is lost to the meet.
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
            (lhs, rhs) => {
                if lhs.rank() <= rhs.rank() {
                    lhs
                } else {
                    rhs
                }
            }
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
    rules: &[ExistentialRule],
    rule_i: &ExistentialRule,
    e: &str,
    universe: &BTreeSet<Position>,
) -> BTreeSet<Position> {
    let mut mv: BTreeSet<Position> = refined_positions(&rule_i.head, e).into_iter().collect();
    loop {
        let before = mv.len();
        for r in rules {
            for v in r.frontier_vars() {
                let bpos = refined_positions(&r.body, &v);
                if !bpos.is_empty() && bpos.iter().all(|p| move_contains(&mv, p, universe)) {
                    mv.extend(refined_positions(&r.head, &v));
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

/// Whether `node` reaches itself in the model-summarizing `dep` relation (a null
/// depending on itself — the summarized signature of an unbounded skolem term).
fn msa_null_reaches_self(
    dep: &std::collections::BTreeMap<String, BTreeSet<String>>,
    node: &str,
) -> bool {
    let mut stack: Vec<&String> = dep.get(node).into_iter().flatten().collect();
    let mut seen: BTreeSet<&String> = BTreeSet::new();
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
    rule_iri: &str,
    existentials: &BTreeSet<String>,
    frontier: &[SwaTerm],
) -> SwaTerm {
    match t {
        EvalTerm::Var(v) if existentials.contains(v) => {
            SwaTerm::Skolem(format!("{rule_iri}#{v}"), frontier.to_vec())
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
    swa_unify_term(&subj_p.scoped("P!"), &subj_c.scoped("C!"), &mut subst)
        && swa_unify_term(&obj_p.scoped("P!"), &obj_c.scoped("C!"), &mut subst)
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

    for rule in rules {
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
                let subj = swa_term(&atom.subject, &rule.rule_iri, &existentials, &frontier);
                let obj = swa_term(&atom.object, &rule.rule_iri, &existentials, &frontier);
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
                out.push(SwaAtom {
                    predicate: atom.predicate.clone(),
                    subject: subj,
                    object: obj,
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
    // Skolemized atoms unify (MGU with occurs-check).
    for producer in &head_atoms {
        for a in producer {
            for consumer in &body_atoms {
                for b in consumer {
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
    }

    (next_place, edges, existential_out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::LOGIC_NAMESPACE;
    use purrdf::TermValue;

    const W: &str = "https://blackcatinformatics.ca/gmeow/world/default";
    const P: &str = "http://ex/p";
    const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const C: &str = "http://ex/C";
    const D: &str = "http://ex/D";

    fn iri(s: &str) -> TermValue {
        TermValue::iri(s)
    }

    fn fact(s: &str, p: &str, o: &str) -> Fact {
        Fact {
            subject: iri(s),
            predicate: p.to_owned(),
            object: iri(o),
        }
    }

    fn var(name: &str) -> EvalTerm {
        EvalTerm::Var(name.to_owned())
    }

    fn atom(s: EvalTerm, p: &str, o: EvalTerm) -> EvalAtom {
        EvalAtom {
            subject: s,
            predicate: p.to_owned(),
            object: o,
            negated: false,
        }
    }

    /// `type(x, C) → ∃y. p(x, y) ∧ type(y, D)` — the EL `∃p.D` obligation as a TGD.
    fn some_values_from_rule() -> ExistentialRule {
        ExistentialRule {
            rule_iri: "http://ex/rule/svf".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y")),
                atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        }
    }

    fn decided(outcome: NativeOutcome<Budgeted<Vec<DerivedRow>>>) -> Budgeted<Vec<DerivedRow>> {
        match outcome {
            NativeOutcome::Decided(b) => b,
            NativeOutcome::Unsupported(k) => panic!("expected Decided, got Unsupported({k:?})"),
        }
    }

    /// Count derived rows for a predicate.
    fn count(rows: &[DerivedRow], predicate: &str) -> usize {
        rows.iter().filter(|r| r.predicate == predicate).count()
    }

    /// A well-formed reified head extracts its args in positional order regardless of the
    /// head-atom authoring order.
    #[test]
    fn reified_nary_head_accepts_a_contiguous_shape() {
        let rel = "http://ex/rel/op";
        let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
        let a1 = format!("{LOGIC_NAMESPACE}naryArg1");
        let rule = ExistentialRule {
            rule_iri: "http://ex/rule/nary".to_owned(),
            body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?b"))],
            // naryArg1 authored BEFORE naryArg0 — the extractor must sort to positional order.
            head: vec![
                atom(
                    var("?r"),
                    &instance_of_iri(),
                    EvalTerm::ConstNamed(rel.to_owned()),
                ),
                atom(var("?r"), &a1, var("?b")),
                atom(var("?r"), &a0, var("?a")),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let (reifier, got_rel, args) = reified_nary_head(&rule).unwrap().unwrap();
        assert_eq!(reifier, "?r");
        assert_eq!(got_rel, rel);
        assert_eq!(args, vec![var("?a"), var("?b")]);
    }

    /// A gapped positional index (naryArg0 + naryArg2, no naryArg1) is a HARD ERROR — the
    /// ordered arg vector feeds `mint_nary_reifier`, so a gap would mint a wrong reifier.
    #[test]
    fn reified_nary_head_rejects_a_gapped_positional_index() {
        let rel = "http://ex/rel/op";
        let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
        let a2 = format!("{LOGIC_NAMESPACE}naryArg2");
        let rule = ExistentialRule {
            rule_iri: "http://ex/rule/nary".to_owned(),
            body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?c"))],
            head: vec![
                atom(
                    var("?r"),
                    &instance_of_iri(),
                    EvalTerm::ConstNamed(rel.to_owned()),
                ),
                atom(var("?r"), &a0, var("?a")),
                atom(var("?r"), &a2, var("?c")),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let err = reified_nary_head(&rule).unwrap_err();
        assert!(
            err.message().contains("non-contiguous or duplicate"),
            "{err}"
        );
    }

    /// A duplicate positional index (two naryArg0) is a HARD ERROR for the same reason.
    #[test]
    fn reified_nary_head_rejects_a_duplicate_positional_index() {
        let rel = "http://ex/rel/op";
        let a0 = format!("{LOGIC_NAMESPACE}naryArg0");
        let rule = ExistentialRule {
            rule_iri: "http://ex/rule/nary".to_owned(),
            body: vec![atom(var("?x"), P, var("?a")), atom(var("?x"), P, var("?b"))],
            head: vec![
                atom(
                    var("?r"),
                    &instance_of_iri(),
                    EvalTerm::ConstNamed(rel.to_owned()),
                ),
                atom(var("?r"), &a0, var("?a")),
                atom(var("?r"), &a0, var("?b")),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let err = reified_nary_head(&rule).unwrap_err();
        assert!(
            err.message().contains("non-contiguous or duplicate"),
            "{err}"
        );
    }

    #[test]
    fn chase_invents_a_witness_for_some_values_from() {
        // Two individuals of type C ⇒ two distinct p-edges to two distinct D witnesses
        // (restricted chase = one fresh witness per frontier binding).
        let edb = vec![fact("http://ex/a", TYPE, C), fact("http://ex/b", TYPE, C)];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.status, BudgetStatus::Ok);
        assert_eq!(count(&b.rows, P), 2, "one p-edge per C individual");
        assert_eq!(count(&b.rows, TYPE), 4, "2 asserted C + 2 invented D");
        // The two witnesses are distinct nulls.
        let objs: BTreeSet<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P)
            .map(|r| term_display(&r.object))
            .collect();
        assert_eq!(objs.len(), 2);
    }

    #[test]
    fn chase_restricted_satisfaction_skips_when_witness_exists() {
        // `a` already has a p-edge to `w` typed D ⇒ the obligation is satisfied and no
        // fresh witness is invented; `b` still gets one.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/a", P, "http://ex/w"),
            fact("http://ex/w", TYPE, D),
            fact("http://ex/b", TYPE, C),
        ];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(
            count(&b.rows, P),
            2,
            "a's existing edge + b's invented edge"
        );
        // `a` invents nothing: its only p-edge is the pre-existing one to w.
        let a_targets: Vec<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
            .collect();
        assert_eq!(a_targets.len(), 1);
        assert_eq!(term_display(&a_targets[0].object), "<http://ex/w>");
    }

    #[test]
    fn chase_terminates_on_a_bounded_program() {
        // An acyclic EL restriction over three C individuals: the chase reaches its
        // natural fixpoint (status Ok) with a bounded, exact derived-row count.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/b", TYPE, C),
            fact("http://ex/c", TYPE, C),
        ];
        let b = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.status, BudgetStatus::Ok);
        // 3 echoed C + 3 invented p-edges + 3 invented D-types = 9 rows, and no more on
        // a second identical run (determinism).
        assert_eq!(b.rows.len(), 9);
        let again = decided(chase_world(W, &edb, &[some_values_from_rule()], None).unwrap());
        assert_eq!(b.rows.len(), again.rows.len());
        assert_eq!(b.consumed_steps, again.consumed_steps);
    }

    #[test]
    fn chase_budget_exhaustion_is_incomplete_not_wrong() {
        // A cyclic `D ⊑ ∃p.D` would not terminate unbudgeted; with a step budget the
        // chase stops early, reporting Exhausted with a sound committed prefix.
        let cyclic = ExistentialRule {
            rule_iri: "http://ex/rule/cyclic".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(D.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y")),
                atom(var("?y"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let edb = vec![fact("http://ex/a", TYPE, D)];
        let b = decided(chase_world(W, &edb, &[cyclic], Some(3)).unwrap());
        assert_eq!(b.status, BudgetStatus::Exhausted);
        assert_eq!(
            b.consumed_steps, 3,
            "exactly the budget of committed derivations"
        );
    }

    #[test]
    fn chase_at_least_two_requires_two_distinct_witnesses() {
        // `≥2 p.D`: a single existing typed p-edge does NOT satisfy the obligation; the
        // chase must invent a second, distinct witness.
        let ge2 = ExistentialRule {
            rule_iri: "http://ex/rule/ge2".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![
                atom(var("?x"), P, var("?y1")),
                atom(var("?y1"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
                atom(var("?x"), P, var("?y2")),
                atom(var("?y2"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            distinct: vec![("?y1".to_owned(), "?y2".to_owned())],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        // `a` has ONE existing typed witness — short of the two required.
        let edb = vec![
            fact("http://ex/a", TYPE, C),
            fact("http://ex/a", P, "http://ex/w"),
            fact("http://ex/w", TYPE, D),
        ];
        let b = decided(chase_world(W, &edb, &[ge2], None).unwrap());
        // a must end with ≥2 distinct D-typed p-targets.
        let targets: BTreeSet<_> = b
            .rows
            .iter()
            .filter(|r| r.predicate == P && term_display(&r.subject) == "<http://ex/a>")
            .map(|r| term_display(&r.object))
            .collect();
        assert!(
            targets.len() >= 2,
            "≥2 distinct witnesses, got {}",
            targets.len()
        );
    }

    #[test]
    fn chase_materialize_echoes_later_worlds_asserted_facts_after_budget_exhaustion() {
        // A step budget governs DERIVED steps, not input. When it is spent in an earlier
        // world, later worlds' ASSERTED (EDB) facts must still be echoed — never dropped
        // with the derivations.
        let w1 = "http://ex/world/1";
        let w2 = "http://ex/world/2";
        let store = crate::store::WorldStore::new();
        // Two obligations in world 1 exhaust a 1-step budget before world 2 is reached.
        store.insert_quad(w1, "http://ex/a1", TYPE, C);
        store.insert_quad(w1, "http://ex/a2", TYPE, C);
        store.insert_quad(w2, "http://ex/b", TYPE, C);
        let (_admission, outcome) =
            chase_materialize(&store, &[some_values_from_rule()], Some(1)).unwrap();
        let b = decided(outcome);
        assert_eq!(
            b.status,
            BudgetStatus::Exhausted,
            "the 1-step budget must exhaust before world 2"
        );
        assert!(
            b.rows.iter().any(|r| r.graph == w2
                && r.predicate == TYPE
                && term_display(&r.subject) == "<http://ex/b>"),
            "world 2's asserted EDB must survive world 1's budget exhaustion; rows: {:#?}",
            b.rows
        );
    }

    // ── ChaseAdmission termination certificate ───────────────────────────────────

    const E: &str = "http://ex/E";
    const Q: &str = "http://ex/q";

    /// `type(x, from) → ∃y. rel(x, y) ∧ type(y, to)`.
    fn restriction_rule(iri: &str, from: &str, rel: &str, to: &str) -> ExistentialRule {
        ExistentialRule {
            rule_iri: iri.to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(from.to_owned()))],
            head: vec![
                atom(var("?x"), rel, var("?y")),
                atom(var("?y"), TYPE, EvalTerm::ConstNamed(to.to_owned())),
            ],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        }
    }

    #[test]
    fn certify_acyclic_el_restriction_is_weakly_acyclic_and_non_vacuous() {
        // `C ⊑ ∃p.D` terminates (the D-witness never re-triggers the C-bodied rule).
        // The certifier must (a) certify it AND (b) actually SEE an existential edge —
        // the load-bearing non-vacuity check: if the ∃ head var were invisible the
        // certifier would trivially (vacuously) certify with ZERO special edges.
        let admission = ChaseAdmission::certify(&[some_values_from_rule()]);
        match &admission {
            ChaseAdmission::WeaklyAcyclic { evidence } => {
                assert!(admission.admits_native());
                assert!(
                    !evidence.contains("0 existential edge"),
                    "certifier must see ≥1 existential edge (non-vacuous): {evidence}"
                );
            }
            other => {
                panic!("acyclic C⊑∃p.D must certify as weakly-acyclic, got: {other:?}")
            }
        }
    }

    #[test]
    fn certify_cyclic_restriction_is_uncertified() {
        // `D ⊑ ∃p.D`: the witness is itself D-typed, re-triggering the rule forever.
        // No rung of the ladder may certify it — it must fall through to Uncertified.
        let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
        let admission = ChaseAdmission::certify(&[cyclic]);
        match admission {
            ChaseAdmission::Uncertified { violations } => {
                assert!(!violations.is_empty());
                assert!(violations[0].contains("lies in a cycle"));
            }
            certified => {
                panic!("cyclic D⊑∃p.D must NOT certify by any class, got: {certified:?}")
            }
        }
    }

    #[test]
    fn certify_acyclic_chain_certifies() {
        // `C ⊑ ∃p.D` and `D ⊑ ∃q.E`: a finite chain C→D→E, terminating.
        let r1 = restriction_rule("http://ex/rule/c", C, P, D);
        let r2 = restriction_rule("http://ex/rule/d", D, Q, E);
        assert!(ChaseAdmission::certify(&[r1, r2]).admits_native());
    }

    #[test]
    fn certify_two_rule_cycle_is_uncertified() {
        // `C ⊑ ∃p.D` and `D ⊑ ∃q.C`: C→D→C invents forever across two rules.
        let r1 = restriction_rule("http://ex/rule/c", C, P, D);
        let r2 = restriction_rule("http://ex/rule/d", D, Q, C);
        assert!(!ChaseAdmission::certify(&[r1, r2]).admits_native());
    }

    // ── Joint acyclicity (strictly broader than weak) ────────────────────────────

    /// `type(x,C) ∧ type(x,D) → ∃y. p(x,y)` and `p(x,y) → type(y,C)`.
    ///
    /// **Jointly acyclic but NOT weakly acyclic.** Weak acyclicity sees the position
    /// cycle `(type,S,C) → (p,O,*) → (type,S,C)` (the p-object null flows to `type,C`,
    /// which is a body position of the first rule) and refuses.  Joint acyclicity tracks
    /// that the null becomes `C` but never `D`, so it can never re-bind the `C∧D`-guarded
    /// frontier `x` of the first rule — no existential depends on itself.  The chase
    /// terminates: the C-only witness does not satisfy the `C∧D` guard, so no further
    /// invention fires.
    fn jointly_acyclic_not_weakly_acyclic() -> Vec<ExistentialRule> {
        let guarded = ExistentialRule {
            rule_iri: "http://ex/rule/ja-guard".to_owned(),
            body: vec![
                atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned())),
                atom(var("?x"), TYPE, EvalTerm::ConstNamed(D.to_owned())),
            ],
            head: vec![atom(var("?x"), P, var("?y"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let feedback = ExistentialRule {
            rule_iri: "http://ex/rule/ja-feedback".to_owned(),
            body: vec![atom(var("?x"), P, var("?y"))],
            head: vec![atom(var("?y"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        vec![guarded, feedback]
    }

    #[test]
    fn certify_jointly_acyclic_non_vacuous_beyond_weak() {
        // The rung is a REAL increment: weak acyclicity refuses this program, joint
        // acyclicity certifies it.
        let prog = jointly_acyclic_not_weakly_acyclic();
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "weak acyclicity must REFUSE the guard-split program (position-cycle)"
        );
        match ChaseAdmission::certify(&prog) {
            ChaseAdmission::JointlyAcyclic { .. } => {}
            other => panic!("ladder must certify as JointlyAcyclic, got {other:?}"),
        }
        assert!(ChaseAdmission::certify(&prog).admits_native());
    }

    #[test]
    fn certify_jointly_acyclic_evidence_is_non_vacuous() {
        // The certifier actually SAW the existential (≥1 existential variable): a vacuous
        // certificate would report zero and is a bug.
        match ChaseAdmission::certify(&jointly_acyclic_not_weakly_acyclic()) {
            ChaseAdmission::JointlyAcyclic { evidence } => assert!(
                !evidence.contains("0 existential variable"),
                "joint-acyclicity certificate must be non-vacuous (saw ≥1 ∃): {evidence}"
            ),
            other => panic!("expected JointlyAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn jointly_acyclic_program_runs_natively_unbudgeted_on_route_chase() {
        // The production-surface proof: a program weak acyclicity REFUSES today is now
        // admitted AND runs to a natural fixpoint UNBUDGETED on the real router.  A false
        // certification of a non-terminating program would loop/exhaust here.
        let prog = jointly_acyclic_not_weakly_acyclic();
        let edb = vec![fact("http://ex/a", TYPE, C), fact("http://ex/a", TYPE, D)];
        let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
        assert!(
            matches!(admission, ChaseAdmission::JointlyAcyclic { .. }),
            "route_chase must admit the program as jointly-acyclic, got {admission:?}"
        );
        // Runs to a fixpoint with NO budget (Decided, not Unsupported/Exhausted).
        let _ = decided(outcome);
        // Previously-refused leg: the WA-only certifier refuses P, so pre-change
        // route_chase(P, None) would have been Unsupported(NonTerminatingExistential).
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "the same program is refused by weak acyclicity alone (previously refused)"
        );
    }

    #[test]
    fn certify_cyclic_defeats_joint_acyclicity() {
        // A genuine two-rule invention cycle (`C ⊑ ∃p.D`, `D ⊑ ∃q.C`) is non-terminating:
        // joint acyclicity must NOT certify it, and the ladder falls through to refusal.
        let cyclic = vec![
            restriction_rule("http://ex/rule/c", C, P, D),
            restriction_rule("http://ex/rule/d", D, Q, C),
        ];
        assert!(
            ChaseAdmission::certify_joint_acyclic(&cyclic).is_none(),
            "joint acyclicity must refuse a genuine invention cycle"
        );
        assert!(!ChaseAdmission::certify(&cyclic).admits_native());
    }

    // ── Super-weak acyclicity (Skolem place graph, incomparable sibling of JA) ────

    /// `type(x,C) → ∃y. p(x,y)` and `p(x,x) → type(x,C)`.
    ///
    /// **Super-weakly acyclic but NOT weakly acyclic.** Weak acyclicity sees the position
    /// cycle `(type,S,C) → (p,O,*) → (type,S,C)` (the p-object null flows to `type(·,C)`,
    /// a body position of the invention rule) and refuses.  Super-weak acyclicity refuses
    /// that flow: the null minted at `p(x, f(x))` cannot unify into the **diagonal** body
    /// atom `p(x, x)` because the occurs-check `f(x) = x` fails, so no fact ever satisfies
    /// the diagonal rule on the null.  The chase terminates: `p(a, f)` is never a diagonal,
    /// so the second rule never re-types a witness.
    fn super_weakly_acyclic_diagonal() -> Vec<ExistentialRule> {
        let invent = ExistentialRule {
            rule_iri: "http://ex/rule/swa-invent".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![atom(var("?x"), P, var("?y"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let diagonal = ExistentialRule {
            rule_iri: "http://ex/rule/swa-diagonal".to_owned(),
            body: vec![atom(var("?x"), P, var("?x"))],
            head: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        vec![invent, diagonal]
    }

    #[test]
    fn certify_super_weakly_acyclic_non_vacuous_beyond_weak() {
        // Real increment over weak acyclicity (the issue's non-vacuity bar): WA refuses
        // the diagonal program, SWA certifies it via the occurs-check on `f(x) = x`.
        let prog = super_weakly_acyclic_diagonal();
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "weak acyclicity must REFUSE the diagonal program (position-cycle)"
        );
        match ChaseAdmission::certify_super_weak_acyclic(&prog) {
            Some(ChaseAdmission::SuperWeaklyAcyclic { .. }) => {}
            other => {
                panic!("super-weak acyclicity must certify the diagonal program, got {other:?}")
            }
        }
    }

    /// `type(x,C) → ∃y. p(x,y) ∧ p(y,x)` and `p(x,x) → type(x,C)`.
    ///
    /// **Reported by the ladder as SuperWeaklyAcyclic** (WA and JA both refuse it, SWA
    /// certifies it): the null is placed DIRECTLY at both `p` slots (`p(x,f)` and
    /// `p(f,x)`) — no datalog laundering — so the occurs-check blocks both head atoms from
    /// unifying with the diagonal body `p(x,x)`, breaking the cycle WA's position graph and
    /// JA's existential-dependency graph both report.  Terminating: `p(a,f)`/`p(f,a)` are
    /// never the diagonal, so the second rule never re-types a witness.
    fn super_weakly_acyclic_symmetric_head() -> Vec<ExistentialRule> {
        let invent = ExistentialRule {
            rule_iri: "http://ex/rule/swa-sym".to_owned(),
            body: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            head: vec![atom(var("?x"), P, var("?y")), atom(var("?y"), P, var("?x"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let diagonal = ExistentialRule {
            rule_iri: "http://ex/rule/swa-sym-diagonal".to_owned(),
            body: vec![atom(var("?x"), P, var("?x"))],
            head: vec![atom(var("?x"), TYPE, EvalTerm::ConstNamed(C.to_owned()))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        vec![invent, diagonal]
    }

    #[test]
    fn certify_super_weakly_acyclic_is_reported_by_the_ladder() {
        // WA and JA both refuse, SWA certifies — so the escalation ladder REPORTS
        // SuperWeaklyAcyclic (the rung is reachable, not merely a sound standalone check).
        let prog = super_weakly_acyclic_symmetric_head();
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "WA must refuse"
        );
        assert!(
            ChaseAdmission::certify_joint_acyclic(&prog).is_none(),
            "JA must refuse the symmetric-head program"
        );
        match ChaseAdmission::certify(&prog) {
            ChaseAdmission::SuperWeaklyAcyclic { .. } => {}
            other => panic!("ladder must report SuperWeaklyAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn certify_super_weak_evidence_is_non_vacuous() {
        // The certifier actually saw ≥1 invented null (existential output place).
        match ChaseAdmission::certify_super_weak_acyclic(&super_weakly_acyclic_diagonal()) {
            Some(ChaseAdmission::SuperWeaklyAcyclic { evidence }) => assert!(
                !evidence.contains("0 existential output place"),
                "super-weak-acyclicity certificate must be non-vacuous: {evidence}"
            ),
            other => panic!("expected SuperWeaklyAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn super_weakly_acyclic_program_runs_natively_unbudgeted_on_route_chase() {
        // Production-surface proof: a program weak acyclicity refuses runs to a natural
        // fixpoint UNBUDGETED on the real router.  (The ladder reports this particular
        // program as JointlyAcyclic — JA also accepts it and runs first — so we assert
        // `admits_native`, and separately pin the SWA certifier's beyond-WA property.)
        let prog = super_weakly_acyclic_diagonal();
        let edb = vec![fact("http://ex/a", TYPE, C)];
        let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
        assert!(
            admission.admits_native(),
            "route_chase must admit the SWA-certified program natively, got {admission:?}"
        );
        let _ = decided(outcome);
        assert!(
            ChaseAdmission::certify_super_weak_acyclic(&prog).is_some(),
            "the super-weak certifier certifies the program directly"
        );
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "the same program is refused by weak acyclicity alone (previously refused)"
        );
    }

    #[test]
    fn certify_cyclic_defeats_super_weak_acyclicity() {
        // Genuine invention cycles: the null unifies back into its own rule's body (no
        // occurs-check block), so super-weak acyclicity refuses both.
        let self_cycle = vec![restriction_rule("http://ex/rule/cyclic", D, P, D)];
        assert!(
            ChaseAdmission::certify_super_weak_acyclic(&self_cycle).is_none(),
            "super-weak acyclicity must refuse the self-cycle D ⊑ ∃p.D"
        );
        let two_rule = vec![
            restriction_rule("http://ex/rule/c", C, P, D),
            restriction_rule("http://ex/rule/d", D, Q, C),
        ];
        assert!(
            ChaseAdmission::certify_super_weak_acyclic(&two_rule).is_none(),
            "super-weak acyclicity must refuse the two-rule invention cycle"
        );
        assert!(!ChaseAdmission::certify(&two_rule).admits_native());
    }

    // ── Model-summarizing acyclicity (self-hosted, the engine's own fixpoint) ─────

    /// `p(x,x) → ∃y. p(x,y)` and `p(x,y) → p(y,x)`.
    ///
    /// Terminating, but **every structural class refuses it**: weak acyclicity sees a
    /// self special edge, joint acyclicity sees a self existential-dependency (the null's
    /// positions cover the diagonal frontier), and super-weak acyclicity's cross-rule
    /// unification is defeated by the swap rule (`p(y,x)` unifies with the diagonal
    /// `p(x,x)` at the variable level).  Model-summarizing acyclicity certifies it: run on
    /// the critical instance, the summarizing null `p(*, n)` never forms the diagonal
    /// `p(n, n)`, so no `dep(n, n)` is derived — the engine's own fixpoint proves its own
    /// termination.
    fn model_summarizing_beyond_structural() -> Vec<ExistentialRule> {
        let invent = ExistentialRule {
            rule_iri: "http://ex/rule/msa-invent".to_owned(),
            body: vec![atom(var("?x"), P, var("?x"))],
            head: vec![atom(var("?x"), P, var("?y"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let swap = ExistentialRule {
            rule_iri: "http://ex/rule/msa-swap".to_owned(),
            body: vec![atom(var("?x"), P, var("?y"))],
            head: vec![atom(var("?y"), P, var("?x"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        vec![invent, swap]
    }

    #[test]
    fn certify_model_summarizing_non_vacuous_beyond_structural() {
        // Real increment: WA, JA, and SWA all refuse, MSA certifies — and the ladder
        // reports it as ModelSummarizingAcyclic (all cheaper rungs fell through).
        let prog = model_summarizing_beyond_structural();
        assert!(
            ChaseAdmission::certify_weakly_acyclic(&prog).is_err(),
            "weak acyclicity must refuse the swap-diagonal program"
        );
        assert!(
            ChaseAdmission::certify_joint_acyclic(&prog).is_none(),
            "joint acyclicity must refuse the swap-diagonal program"
        );
        assert!(
            ChaseAdmission::certify_super_weak_acyclic(&prog).is_none(),
            "super-weak acyclicity must refuse the swap-diagonal program"
        );
        match ChaseAdmission::certify_model_summarizing(&prog) {
            Some(ChaseAdmission::ModelSummarizingAcyclic { .. }) => {}
            other => panic!("MSA must certify the swap-diagonal program, got {other:?}"),
        }
        match ChaseAdmission::certify(&prog) {
            ChaseAdmission::ModelSummarizingAcyclic { .. } => {}
            other => panic!("the ladder must report ModelSummarizingAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn certify_msa_runs_the_engine_fixpoint() {
        // The self-hosting actually executed the engine's own fixpoint over a non-empty
        // critical instance (not a syntactic shortcut).
        match ChaseAdmission::certify_model_summarizing(&model_summarizing_beyond_structural()) {
            Some(ChaseAdmission::ModelSummarizingAcyclic { evidence }) => assert!(
                !evidence.contains("0 critical-instance fact"),
                "MSA must run the engine fixpoint over a non-empty critical instance: {evidence}"
            ),
            other => panic!("expected ModelSummarizingAcyclic, got {other:?}"),
        }
    }

    #[test]
    fn model_summarizing_program_runs_natively_unbudgeted_on_route_chase() {
        // Production-surface proof: a program every structural class refuses is admitted
        // by the MSA rung and runs to a natural fixpoint UNBUDGETED on the real router.
        // The diagonal `p(a,a)` fires the invent rule so a null is genuinely invented (an
        // `p(a,b)` seed would terminate without ever exercising existential invention).
        let prog = model_summarizing_beyond_structural();
        let edb = vec![fact("http://ex/a", P, "http://ex/a")];
        let (admission, outcome) = route_chase(W, &edb, &prog, None).unwrap();
        assert!(
            matches!(admission, ChaseAdmission::ModelSummarizingAcyclic { .. }),
            "route_chase must admit the program as model-summarizing-acyclic, got {admission:?}"
        );
        let _ = decided(outcome);
    }

    #[test]
    fn certify_cyclic_defeats_msa() {
        // A genuine self-cycle `D ⊑ ∃p.D`: on the critical instance the summarizing null
        // is re-typed D and re-triggers its own rule, so `dep(n, n)` is derived → MSA
        // refuses, and the ladder falls through to Uncertified.
        let cyclic = vec![restriction_rule("http://ex/rule/cyclic", D, P, D)];
        assert!(
            ChaseAdmission::certify_model_summarizing(&cyclic).is_none(),
            "MSA must refuse the self-cycle D ⊑ ∃p.D"
        );
        assert!(!ChaseAdmission::certify(&cyclic).admits_native());
    }

    // ── Nemo-free soundness self-oracle: every certified program terminates ────────

    #[test]
    fn certifier_soundness_differential_reaches_fixpoint() {
        // The soundness differential replacing the retired Nemo oracle: for every program
        // the ladder ADMITS (spanning all four classes), (b) the production router runs it
        // natively unbudgeted, and (c) the budgeted native chase reaches a NATURAL
        // fixpoint — a false certification of a non-terminating program would exhaust the
        // budget instead. Self-hosted, deterministic, on-gate.
        const BIG: u64 = 1_000;
        let certified: Vec<(&str, Vec<ExistentialRule>, Vec<Fact>)> = vec![
            (
                "weakly-acyclic",
                vec![some_values_from_rule()],
                vec![fact("http://ex/a", TYPE, C)],
            ),
            (
                "jointly-acyclic",
                jointly_acyclic_not_weakly_acyclic(),
                vec![fact("http://ex/a", TYPE, C), fact("http://ex/a", TYPE, D)],
            ),
            (
                // The genuinely-SWA-classified witness: `certify` reports
                // `super_weakly_acyclic_diagonal()` as JointlyAcyclic (JA accepts and runs
                // first), so the SWA row must use the symmetric-head fixture the ladder
                // actually reports as SuperWeaklyAcyclic, or this slot only re-tests JA.
                "super-weakly-acyclic",
                super_weakly_acyclic_symmetric_head(),
                vec![fact("http://ex/a", TYPE, C)],
            ),
            (
                // EDB seeds the diagonal `p(a,a)` so the invent rule `p(x,x) → ∃y. p(x,y)`
                // actually FIRES — with `p(a,b)` invention never triggers and the
                // fixpoint-soundness probe is vacuous (a false MSA certification could not
                // exhaust the budget if no null is ever invented).
                "model-summarizing-acyclic",
                model_summarizing_beyond_structural(),
                vec![fact("http://ex/a", P, "http://ex/a")],
            ),
        ];
        for (label, prog, edb) in &certified {
            let admission = ChaseAdmission::certify(prog);
            assert!(
                admission.admits_native(),
                "{label}: a certified program must admit natively, got {admission:?}"
            );
            let (_, unbudgeted) = route_chase(W, edb, prog, None).unwrap();
            let _ = decided(unbudgeted);
            let budgeted = decided(chase_world(W, edb, prog, Some(BIG)).unwrap());
            assert_eq!(
                budgeted.status,
                BudgetStatus::Ok,
                "{label}: a certified program must reach a NATURAL fixpoint (a false \
                 certification would exhaust the budget)"
            );
        }
    }

    #[test]
    fn certifier_refuses_non_terminating_programs() {
        // The sound fallback: genuinely non-terminating programs stay Uncertified, and the
        // unbudgeted router refuses them (Unsupported) rather than looping.
        let refused: Vec<(&str, Vec<ExistentialRule>, Vec<Fact>)> = vec![
            (
                "self-cycle",
                vec![restriction_rule("http://ex/rule/cyclic", D, P, D)],
                vec![fact("http://ex/a", TYPE, D)],
            ),
            (
                "two-rule-cycle",
                vec![
                    restriction_rule("http://ex/rule/c", C, P, D),
                    restriction_rule("http://ex/rule/d", D, Q, C),
                ],
                vec![fact("http://ex/a", TYPE, C)],
            ),
        ];
        for (label, prog, edb) in &refused {
            assert!(
                !ChaseAdmission::certify(prog).admits_native(),
                "{label}: a non-terminating program must stay Uncertified"
            );
            let (_, outcome) = route_chase(W, edb, prog, None).unwrap();
            assert!(
                matches!(
                    outcome,
                    NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
                ),
                "{label}: the unbudgeted router must refuse rather than loop"
            );
        }
    }

    #[test]
    fn certify_lattice_ranks_are_strictly_ordered() {
        // The explicit escalation order (never a derived `Ord`): the ranks strictly
        // increase Uncertified < WA < JA < SWA < MSA.
        let ev = |s: &str| s.to_owned();
        let ranks = [
            ChaseAdmission::Uncertified { violations: vec![] }.rank(),
            ChaseAdmission::WeaklyAcyclic { evidence: ev("wa") }.rank(),
            ChaseAdmission::JointlyAcyclic { evidence: ev("ja") }.rank(),
            ChaseAdmission::SuperWeaklyAcyclic {
                evidence: ev("swa"),
            }
            .rank(),
            ChaseAdmission::ModelSummarizingAcyclic {
                evidence: ev("msa"),
            }
            .rank(),
        ];
        for w in ranks.windows(2) {
            assert!(
                w[0] < w[1],
                "certificate ranks must strictly increase: {ranks:?}"
            );
        }
    }

    #[test]
    fn certify_non_existential_program_is_trivially_weakly_acyclic() {
        // A plain Datalog rule (no ∃ head var) has no special edges → certified.
        let datalog = ExistentialRule {
            rule_iri: "http://ex/rule/datalog".to_owned(),
            body: vec![atom(var("?x"), P, var("?y"))],
            head: vec![atom(var("?y"), P, var("?x"))],
            distinct: vec![],
            witness_frontier: None,
            witness_policy: WitnessPolicy::FrontierSkolem,
        };
        let admission = ChaseAdmission::certify(&[datalog]);
        assert!(admission.admits_native());
        assert!(matches!(
            admission,
            ChaseAdmission::WeaklyAcyclic { evidence } if evidence.contains("0 existential edge")
        ));
    }

    #[test]
    fn certify_lattice_combine_takes_the_weaker() {
        // The whole program is admitted only if every part is: combine → the weaker.
        let good = ChaseAdmission::certify(&[some_values_from_rule()]);
        let bad = ChaseAdmission::certify(&[restriction_rule("http://ex/r", D, P, D)]);
        assert!(!good.clone().combine(bad.clone()).admits_native());
        assert!(!bad.combine(good).admits_native());
    }

    #[test]
    fn certify_lattice_combine_merges_uncertified_violations() {
        // Two uncertified parts meet to Uncertified keeping EVERY violation — merged,
        // sorted, deduped — so no termination-failure diagnostic is dropped by the meet.
        let a = ChaseAdmission::Uncertified {
            violations: vec!["edge y -> z in cycle".to_owned(), "shared".to_owned()],
        };
        let b = ChaseAdmission::Uncertified {
            violations: vec!["edge p -> q in cycle".to_owned(), "shared".to_owned()],
        };
        match a.combine(b) {
            ChaseAdmission::Uncertified { violations } => assert_eq!(
                violations,
                vec![
                    "edge p -> q in cycle".to_owned(),
                    "edge y -> z in cycle".to_owned(),
                    "shared".to_owned(),
                ],
                "combine keeps every violation, sorted and deduped (no lost diagnostic)"
            ),
            other => panic!("two uncertified parts combine to Uncertified, got {other:?}"),
        }
    }

    #[test]
    fn certify_lattice_combine_orders_the_new_classes() {
        // The extended chain Uncertified ⊏ WA ⊏ JA ⊏ SWA ⊏ MSA is explicit (rank),
        // and combine keeps the weaker (lower-ranked) element for every new pair —
        // and all four certified classes admit_native.
        let ev = |s: &str| s.to_owned();
        let ladder = [
            ChaseAdmission::WeaklyAcyclic { evidence: ev("wa") },
            ChaseAdmission::JointlyAcyclic { evidence: ev("ja") },
            ChaseAdmission::SuperWeaklyAcyclic {
                evidence: ev("swa"),
            },
            ChaseAdmission::ModelSummarizingAcyclic {
                evidence: ev("msa"),
            },
        ];
        for cert in &ladder {
            assert!(
                cert.admits_native(),
                "every certified class admits: {cert:?}"
            );
        }
        // Adjacent pairs: combine keeps the weaker (the lower rung).
        for pair in ladder.windows(2) {
            let (weak, strong) = (pair[0].clone(), pair[1].clone());
            assert_eq!(
                weak.clone().combine(strong.clone()),
                weak.clone(),
                "combine keeps the weaker (lower-ranked) certificate"
            );
            assert_eq!(
                strong.combine(weak.clone()),
                weak,
                "combine is order-insensitive in which it keeps (the weaker)"
            );
        }
        // Any certified class meets Uncertified down to Uncertified.
        let uncertified = ChaseAdmission::Uncertified {
            violations: vec![ev("v")],
        };
        for cert in &ladder {
            assert!(
                !cert.clone().combine(uncertified.clone()).admits_native(),
                "certified ∧ Uncertified = Uncertified (not admitted)"
            );
            assert!(!uncertified.clone().combine(cert.clone()).admits_native());
        }
    }

    // ── route_chase: certify → chase / refuse / budget ───────────────────────────

    #[test]
    fn route_certified_program_runs_natively() {
        let edb = vec![fact("http://ex/a", TYPE, C)];
        let (admission, outcome) = route_chase(W, &edb, &[some_values_from_rule()], None).unwrap();
        assert!(admission.admits_native());
        let b = decided(outcome);
        assert_eq!(b.status, BudgetStatus::Ok);
        assert_eq!(count(&b.rows, P), 1);
    }

    #[test]
    fn route_uncertified_without_budget_refuses_to_the_oracle() {
        // Cyclic D⊑∃p.D, no budget ⇒ a first-class declared gap, never
        // a native loop.
        let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
        let edb = vec![fact("http://ex/a", TYPE, D)];
        let (admission, outcome) = route_chase(W, &edb, &[cyclic], None).unwrap();
        assert!(!admission.admits_native());
        assert!(matches!(
            outcome,
            NativeOutcome::Unsupported(UnsupportedKind::NonTerminatingExistential)
        ));
    }

    #[test]
    fn route_uncertified_with_budget_runs_partial() {
        // Cyclic program WITH a budget ⇒ budgeted-partial native run (incomplete, never
        // wrong), deterministically selected by budget config.
        let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
        let edb = vec![fact("http://ex/a", TYPE, D)];
        let (admission, outcome) = route_chase(W, &edb, &[cyclic], Some(2)).unwrap();
        assert!(!admission.admits_native());
        let b = decided(outcome);
        assert_eq!(b.status, BudgetStatus::Exhausted);
        assert_eq!(b.consumed_steps, 2);
    }

    // ── H3: capability-gap counting, invented-individual explain, certificate Finding ──

    #[test]
    fn refused_existential_program_counts_a_reason_ledger_dlgap() {
        // A cyclic `D ⊑ ∃p.D` is uncertified; its refusal is a COUNTED reason::ledger
        // DlGap carrying the weak-acyclicity violation evidence — never silently dropped.
        let cyclic = restriction_rule("http://ex/rule/cyclic", D, P, D);
        let admission = ChaseAdmission::certify(&[cyclic]);
        assert!(
            !admission.admits_native(),
            "cyclic program must be uncertified"
        );

        let rows = admission.capability_gap_rows();
        assert_eq!(rows.len(), 1, "one DlGap row per violation");
        assert_eq!(rows[0].kind, crate::reason::ledger::DivergenceKind::DlGap);
        assert_eq!(
            rows[0].category,
            crate::reason::ledger::EXISTENTIAL_CHASE_CATEGORY,
            "scoped out of the DL/EL crosscheck corpus by category"
        );
        assert!(
            rows[0].detail.contains("lies in a cycle"),
            "the violation evidence rides in detail: {:?}",
            rows[0].detail
        );

        // Routed into the counted divergence ledger it IS tallied and fails enforce…
        let ledger = crate::reason::ledger::build_ledger(Vec::new(), rows, Vec::new());
        assert_eq!(ledger.dl_gap, 1, "counted as a DL gap in reason::ledger");
        assert!(!crate::reason::ledger::enforce(&ledger).passed);

        // …but a CERTIFIED program contributes no gap rows.
        assert!(
            ChaseAdmission::certify(&[some_values_from_rule()])
                .capability_gap_rows()
                .is_empty(),
            "a weakly-acyclic program is not a capability-gap"
        );
    }

    #[test]
    fn explain_recovers_the_recipe_of_a_chase_invented_witness() {
        use crate::physical::store::WitnessDerivation;

        // Run the chase on `C ⊑ ∃p.D` for one C-individual, then EXPLAIN the invented null:
        // its recipe must name the firing rule and the frontier binding (the C-individual).
        let edb = vec![fact("http://ex/a", TYPE, C)];
        let (outcome, registry) =
            chase_world_explained(W, &edb, &[some_values_from_rule()], None).unwrap();
        let b = decided(outcome);

        // The one p-edge's object is the invented witness.
        let witness = b
            .rows
            .iter()
            .find(|r| r.predicate == P)
            .map(|r| term_display(&r.object))
            .expect("the chase must invent a p-target witness");
        let witness_iri = witness
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .expect("witness is an IRI display form");

        assert_eq!(registry.len(), 1, "exactly one witness invented");
        let derivation = registry
            .explain(witness_iri)
            .expect("the invented witness must be explainable from the registry");
        assert_eq!(
            derivation,
            WitnessDerivation {
                witness: witness_iri.to_owned(),
                rule_iri: "http://ex/rule/svf".to_owned(),
                ordinal: 0,
                frontier: vec![TermValue::iri("http://ex/a")],
            },
            "the recipe recovers the firing rule + the C-individual frontier binding"
        );

        // A never-invented term is not explainable.
        assert!(registry.explain("http://ex/a").is_none());
    }

    #[test]
    fn certificate_finding_carries_evidence_or_violations() {
        // WeaklyAcyclic ⇒ an informational Finding carrying the proof evidence.
        let good = ChaseAdmission::certify(&[some_values_from_rule()]);
        let good_finding = good.to_finding();
        assert_eq!(good_finding.severity, Severity::Info);
        assert_eq!(good_finding.code, "chase.certificate.weakly-acyclic");
        assert_eq!(good_finding.tool.as_deref(), Some("chase"));
        assert!(
            good_finding.message.contains("weakly acyclic"),
            "the WeaklyAcyclic finding carries its evidence: {}",
            good_finding.message
        );

        // Uncertified ⇒ an error Finding carrying the weak-acyclicity violations.
        let bad = ChaseAdmission::certify(&[restriction_rule("http://ex/rule/cyclic", D, P, D)]);
        let bad_finding = bad.to_finding();
        assert_eq!(bad_finding.severity, Severity::Error);
        assert_eq!(bad_finding.code, "chase.certificate.uncertified");
        assert!(
            bad_finding.message.contains("lies in a cycle"),
            "the Uncertified finding carries its violations: {}",
            bad_finding.message
        );
    }
}
