// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native well-founded-semantics evaluator.
//!
//! The well-founded model is computed by the **alternating fixpoint** of van Gelder,
//! on top of the reduct least
//! model in [`crate::rule_ir`].  Per world:
//!
//! 1. `K = edb` (the certainly-true under-estimate).
//! 2. Repeat: `U = lmr(reference = K)` (the over-estimate — facts that could be
//!    true), then `K2 = lmr(reference = U)` (the new under-estimate).  Stop when
//!    `K2` has the same fact-key set as `K`.  The fixpoint `W = K` is the
//!    well-founded model's set of true atoms.
//! 3. `final = lmr(reference = W)`.  In a two-valued (total) well-founded model
//!    `final.store == W`; if not (an atom is *undefined*), this is a hard error —
//!    no Phase-A corpus case is partial.
//!
//! `final.derivations` already excludes EDB facts, so the emitted derived rows are
//! exactly the non-asserted true atoms, each with the first-wins provenance the
//! reduct engine recorded.  Determinism mirrors `foundation.rs`: sorted EDB seed,
//! parse-order rules, insertion-order facts, and a final sort by
//! `(graph, subject, predicate, object)`.
//!
//! # Worked corpus case
//!
//! EDB `move(p1, p2)`, rule `win(?X,?X) :- move(?X,?Y), ~win(?Y,?Y)`.  `win(p2,p2)`
//! is unsupported (no `move` from `p2`), so `~win(p2,p2)` holds and `win(p1,p1)` is
//! TRUE.  The alternating fixpoint converges to `W = {move(p1,p2), win(p1,p1)}`;
//! the single derived row is `win(p1,p1)` with `rule_iri = …/ruleWin` and
//! `source_quad_ids = [reifier(move(p1,p2))]`.
//!
//! [`IncrementalWellFoundedSession`] is the production multi-shot boundary used by
//! the production materialization router. The low-level scratch
//! entry points remain crate-internal comparators for parity tests and benchmarks,
//! hence the crate-internal `dead_code` allowance.
#![allow(dead_code)]

use std::sync::Arc;

use crate::rule_ir::{
    DerivedRow, EvalRule, FactStore, echo_asserted, least_model_of_reduct, world_edb_facts,
};
use crate::{
    physical::{GroundingUpdate, IncrementalGroundProgram, SignedFact},
    reason::perf_ledger::{NonmonotoneSolveRun, NonmonotoneSolver, nonmonotone_solve_run},
    rule_ir::Fact,
};

/// Wrap a reasoning-driver condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn reason_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

/// The ordered intra-engine phases [`materialize`] runs per world — the runtime
/// twin of the authored `logic:wellFoundedMaterializerPlan`
/// (`slices/grounding/logic/module.ttl`).
///
/// Principle 12 BOUNDARY: the authored plan is checked against THIS const by the
/// dogfood parity gate `crates/pipeline/tests/wellfounded_plan_parity.rs`; the
/// reasoner NEVER parses that RDF at scheduling or runtime — the native loop
/// below is the runtime, the plan is its declared twin. The names match the local
/// names of the plan's `logic:ActionSchema` phase individuals one-to-one, so the
/// parity test can map them directly.
///
/// The middle phase (`wfResolveFixpoint`) is the alternating fixpoint — an
/// iteration (see [`WELL_FOUNDED_ITERATED_PHASE`]).
pub const WELL_FOUNDED_PHASES: [&str; 3] =
    ["wfMaterializeEdb", "wfResolveFixpoint", "wfConstructModel"];

/// The one phase of [`WELL_FOUNDED_PHASES`] that is an iteration (the alternating
/// fixpoint loop), modelled in the authored plan as a `logic:Iteration` whose
/// `logic:iterationBody` is this phase's `logic:ActionSchema`.
pub const WELL_FOUNDED_ITERATED_PHASE: &str = "wfResolveFixpoint";

/// One multi-shot WFS update: maintained-ground-program evidence, the explicit
/// non-incremental solver ledger row, and the current canonical result.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalWellFoundedShot {
    pub(crate) grounding: GroundingUpdate,
    pub(crate) solve: NonmonotoneSolveRun,
    pub(crate) rows: Arc<[DerivedRow]>,
}

/// Stateful WFS facade over an incrementally maintained ground program.
///
/// Only grounding is incremental.  A changed complete solver slice reruns the
/// existing alternating fixpoint from scratch; an unchanged slice reuses the
/// cached rows and records that disposition in the per-shot perf-ledger row.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalWellFoundedSession {
    world: String,
    ground: IncrementalGroundProgram,
    rows: Arc<[DerivedRow]>,
}

impl IncrementalWellFoundedSession {
    /// Build the initial ground program and solve it once from scratch.
    pub(crate) fn new(
        contract_hash: impl Into<String>,
        world: impl Into<String>,
        edb: impl IntoIterator<Item = Fact>,
        rules: &[EvalRule],
    ) -> gmeow_errors::Result<Self> {
        let world = world.into();
        let ground = IncrementalGroundProgram::new(contract_hash, edb, rules)?;
        let snapshot = ground.snapshot();
        let rows = Arc::from(materialize_ground_slice(
            &world,
            &snapshot.edb,
            &snapshot.rules,
        )?);
        Ok(Self {
            world,
            ground,
            rows,
        })
    }

    /// Current cached rows.
    pub(crate) fn rows(&self) -> &[DerivedRow] {
        &self.rows
    }

    /// Full-grounding candidate probes paid when this session was built.
    pub(crate) fn scratch_ground_rule_probe_rows(&self) -> gmeow_errors::Result<u64> {
        self.ground.scratch_ground_rule_probe_rows()
    }

    /// Active fully-ground rules in the current solver slice.
    pub(crate) fn active_ground_rule_count(&self) -> usize {
        self.ground.active_ground_rule_count()
    }

    /// Falsifiable maintenance oracle for tests / deterministic benchmark lanes.
    pub(crate) fn check_grounding_scratch_parity(&self) -> gmeow_errors::Result<()> {
        self.ground.check_scratch_parity()
    }

    /// Apply one signed EDB shot.  The session is atomic across grounding and
    /// solving: a solver failure leaves both the old ground program and old rows.
    pub(crate) fn apply(
        &mut self,
        changes: impl IntoIterator<Item = SignedFact>,
    ) -> gmeow_errors::Result<IncrementalWellFoundedShot> {
        let mut next_ground = self.ground.clone();
        let grounding = next_ground.apply(changes)?;
        let solve = nonmonotone_solve_run(
            NonmonotoneSolver::WellFounded,
            grounding.slice_changed,
            grounding.edb_changes.len(),
            grounding.rule_changes.len(),
        );
        let next_rows = if solve.solver_reran() {
            let snapshot = next_ground.snapshot();
            Arc::from(materialize_ground_slice(
                &self.world,
                &snapshot.edb,
                &snapshot.rules,
            )?)
        } else {
            self.rows.clone()
        };
        self.ground = next_ground;
        self.rows = next_rows.clone();
        Ok(IncrementalWellFoundedShot {
            grounding,
            solve,
            rows: next_rows,
        })
    }
}

/// Materialize the well-founded model of `rules` over every world in `store`.
///
/// Returns the asserted-EDB rows plus the derived true-atom rows, sorted by
/// `(graph, subject.to_string(), predicate, object.to_string())`.
///
/// # Errors
///
/// Returns `Err` for an invalid input IRI, an unbound head/guard variable, a
/// provenance-recipe failure, or a *partial* (non-total) well-founded model — the
/// last is a hard error in v1 (no Phase-A corpus case is undefined).
pub(crate) fn materialize(
    store: &crate::store::WorldStore,
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<DerivedRow>> {
    let mut worlds = store.worlds();
    worlds.sort();

    let mut out: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb_facts = world_edb_facts(store, world)?;
        out.extend(materialize_ground_slice(world, &edb_facts, rules)?);
    }

    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

/// Benchmark-only adapter for canonical typed rules.
#[doc(hidden)]
pub fn bench_wf_materialize(
    store: &crate::store::WorldStore,
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> gmeow_errors::Result<usize> {
    let rules = crate::lower::lower_eval_rules(program)?;
    materialize(store, &rules).map(|rows| rows.len())
}

/// Run the deliberately non-incremental alternating fixpoint over one complete
/// solver slice. `rules` may be the original variable program or the exact active
/// ground program maintained by [`IncrementalGroundProgram`].
fn materialize_ground_slice(
    world: &str,
    edb_facts: &[Fact],
    rules: &[EvalRule],
) -> gmeow_errors::Result<Vec<DerivedRow>> {
    let mut out = echo_asserted(world, edb_facts)?;
    let mut edb = FactStore::new();
    for fact in edb_facts {
        edb.insert(fact.clone());
    }

    let mut k = edb.clone();
    loop {
        let u = least_model_of_reduct(&edb, rules, &k)?.store;
        let k2 = least_model_of_reduct(&edb, rules, &u)?.store;
        if k2.key_set() == k.key_set() {
            k = k2;
            break;
        }
        k = k2;
    }
    let well_founded = k;
    let final_res = least_model_of_reduct(&edb, rules, &well_founded)?;
    if final_res.store.key_set() != well_founded.key_set() {
        return Err(reason_err(
            "well-founded model is partial (undefined atoms) — not supported in \
             gmeow-logic v1 (no Phase-A corpus case is non-total)"
                .to_owned(),
        ));
    }
    for mut row in final_res.derivations {
        row.graph = world.to_owned();
        out.push(row);
    }
    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{mint_derivation_id, mint_reifier};
    use crate::rule_ir::{EvalAtom, EvalTerm};
    use crate::store::WorldStore;
    use purrdf::TermValue;

    const WF: &str = "https://example.org/profiles/well-founded/";

    fn wf_rules() -> Vec<EvalRule> {
        let atom = |subject: &str, predicate: &str, object: &str, negated| EvalAtom {
            subject: EvalTerm::var(subject),
            predicate: format!("{WF}{predicate}"),
            object: EvalTerm::var(object),
            negated,
        };
        vec![EvalRule {
            head: atom("?X", "win", "?X", false),
            body: vec![
                atom("?X", "move", "?Y", false),
                atom("?Y", "win", "?Y", true),
            ],
            rule_iri: format!("{WF}ruleWin"),
            distinct_pairs: Vec::new(),
            builtins: Vec::new(),
            constraint_tag: None,
        }]
    }

    fn wf_store() -> WorldStore {
        let store = WorldStore::new();
        store.insert_quad(
            &format!("{WF}world-game"),
            &format!("{WF}p1"),
            &format!("{WF}move"),
            &format!("{WF}p2"),
        );
        store
    }

    fn wf_fact(subject: &str, predicate: &str, object: &str) -> Fact {
        Fact {
            subject: TermValue::iri(format!("{WF}{subject}")),
            predicate: format!("{WF}{predicate}"),
            object: TermValue::iri(format!("{WF}{object}")),
        }
    }

    fn row_key(row: &DerivedRow) -> (String, String, String, String, String) {
        (
            row.graph.clone(),
            crate::provenance::term_display(&row.subject),
            row.predicate.clone(),
            crate::provenance::term_display(&row.object),
            row.rule_iri.clone(),
        )
    }

    #[test]
    fn well_founded_derives_exactly_win_p1_p1() {
        let rules = wf_rules();
        let store = wf_store();
        let rows = materialize(&store, &rules).expect("materialize");

        // Partition asserted vs derived.
        let world = format!("{WF}world-game");
        let derived: Vec<&DerivedRow> = rows
            .iter()
            .filter(|r| r.rule_iri != crate::provenance::ASSERT_RULE_IRI)
            .collect();
        assert_eq!(derived.len(), 1, "exactly one derived quad: {rows:#?}");
        let row = derived[0];

        // win(p1, p1) in world-game.
        assert_eq!(row.graph, world);
        assert_eq!(row.subject, TermValue::iri(format!("{WF}p1")));
        assert_eq!(row.predicate.as_str(), format!("{WF}win"));
        assert_eq!(row.object, TermValue::iri(format!("{WF}p1")));

        // Provenance: rule_iri = …/ruleWin, source = reifier(move(p1,p2)).
        assert_eq!(row.rule_iri, format!("{WF}ruleWin"));
        let move_reifier = mint_reifier(
            &TermValue::iri(format!("{WF}p1")),
            &format!("{WF}move"),
            &TermValue::iri(format!("{WF}p2")),
        )
        .unwrap();
        assert_eq!(row.source_quad_ids, vec![move_reifier.clone()]);
        assert_eq!(
            row.derivation_id,
            mint_derivation_id(&format!("{WF}ruleWin"), &[move_reifier.as_str()])
        );

        // The asserted move(p1,p2) row is also present.
        assert!(
            rows.iter()
                .any(|r| r.rule_iri == crate::provenance::ASSERT_RULE_IRI
                    && r.predicate.as_str() == format!("{WF}move")),
            "asserted move row present"
        );
    }

    #[test]
    fn incremental_grounding_reuses_only_an_unchanged_complete_wfs_slice() {
        let world = format!("{WF}world-game");
        let rules = wf_rules();
        let mut session = IncrementalWellFoundedSession::new(
            "contract",
            &world,
            [wf_fact("p1", "move", "p2")],
            &rules,
        )
        .expect("initial incremental WFS session");
        let direct = materialize(&wf_store(), &rules).expect("direct WFS");
        assert_eq!(
            session.rows().iter().map(row_key).collect::<Vec<_>>(),
            direct.iter().map(row_key).collect::<Vec<_>>(),
            "ground-program solve preserves direct WFS rows"
        );

        let initial_rows = session.rows.clone();
        let cancelled = session
            .apply([
                SignedFact {
                    fact: wf_fact("p2", "move", "p3"),
                    weight: 1,
                },
                SignedFact {
                    fact: wf_fact("p2", "move", "p3"),
                    weight: -1,
                },
            ])
            .expect("cancelled shot");
        assert!(!cancelled.grounding.slice_changed);
        assert!(!cancelled.solve.solver_reran());
        assert_eq!(cancelled.solve.edb_changes, 0);
        assert_eq!(cancelled.solve.ground_rule_changes, 0);
        assert!(Arc::ptr_eq(&initial_rows, &cancelled.rows));
        assert!(Arc::ptr_eq(&cancelled.rows, &session.rows));

        let changed = session
            .apply([SignedFact {
                fact: wf_fact("p2", "move", "p3"),
                weight: 1,
            }])
            .expect("changed shot");
        assert!(changed.grounding.slice_changed);
        assert!(changed.solve.solver_reran());
        assert!(!Arc::ptr_eq(&cancelled.rows, &changed.rows));
        assert_eq!(
            changed.solve.solver.as_str(),
            "well-founded alternating fixpoint"
        );
        assert!(changed.rows.iter().any(|row| {
            row.predicate == format!("{WF}win")
                && crate::provenance::term_display(&row.subject) == format!("<{WF}p2>")
        }));
        session
            .check_grounding_scratch_parity()
            .expect("changed WFS grounding matches scratch");
    }
}
