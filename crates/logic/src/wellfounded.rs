// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native well-founded-semantics evaluator (issue #651, Phase A).
//!
//! Nemo rejects non-stratifiable programs, so the well-founded model is computed
//! here by the **alternating fixpoint** of van Gelder, on top of the reduct least
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
//! Phase-A note: [`materialize`] is the entry point `py.rs` will call in Phase B of
//! #651; until that routing lands it is consumed only by this module's tests, hence
//! the crate-internal `dead_code` allowance.
#![allow(dead_code)]

use crate::rule_ir::{
    echo_asserted, least_model_of_reduct, world_edb_facts, DerivedRow, EvalRule, FactStore,
};

/// The ordered intra-engine phases [`materialize`] runs per world — the runtime
/// twin of the authored `logic:wellFoundedMaterializerPlan`
/// (`slices/core/logic/module.ttl`).
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
) -> Result<Vec<DerivedRow>, String> {
    let mut worlds = store.worlds();
    worlds.sort();

    let mut out: Vec<DerivedRow> = Vec::new();
    for world in &worlds {
        let edb_facts = world_edb_facts(store, world)?;

        // Asserted-EDB echo.
        out.extend(echo_asserted(world, &edb_facts)?);

        // Seed the EDB store (sorted-key order already, from world_edb_facts).
        let mut edb = FactStore::new();
        for f in &edb_facts {
            edb.insert(f.clone());
        }

        // Alternating fixpoint.
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

        // Final reduct against the well-founded model.  In a total model the least
        // model of the reduct reproduces W exactly; a mismatch means an undefined
        // atom (hard error in v1).
        let final_res = least_model_of_reduct(&edb, rules, &well_founded)?;
        if final_res.store.key_set() != well_founded.key_set() {
            return Err(
                "well-founded model is partial (undefined atoms) — not supported in \
                 gmeow-logic v1 (no Phase-A corpus case is non-total)"
                    .to_owned(),
            );
        }

        // Emit derived rows (already excludes EDB), stamping the world graph.
        for mut row in final_res.derivations {
            row.graph = world.clone();
            out.push(row);
        }
    }

    crate::rule_ir::sort_rows(&mut out);
    Ok(out)
}

/// Benchmarking shim: run the well-founded materializer over a pre-built store + rules.
///
/// Accepts a reference to a [`crate::store::WorldStore`] (already populated, built once
/// outside `b.iter`) and a Nemo `.rls` rule string.  Parses the rules, calls
/// [`materialize`], and returns the number of output rows.  Used by `benches/reduct.rs`
/// to exercise `rule_ir::least_model_of_reduct` through the public API with N-Quad
/// loading amortised outside the hot loop.  Not part of the production surface.
///
/// # Errors
///
/// Propagates errors from rule parsing or `materialize`.
#[doc(hidden)]
pub fn bench_wf_materialize(
    store: &crate::store::WorldStore,
    rules_text: &str,
) -> Result<usize, String> {
    use crate::rule_ir::parse_eval_rules;
    let rules = parse_eval_rules(rules_text)?;
    let rows = materialize(store, &rules)?;
    Ok(rows.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::{mint_derivation_id, mint_reifier};
    use crate::rule_ir::parse_eval_rules;
    use crate::store::WorldStore;
    use oxigraph::model::{NamedNode, Term};

    const WF: &str = "https://example.org/profiles/well-founded/";

    fn wf_rules() -> Vec<EvalRule> {
        let rls = format!(
            "#[name(\"{WF}ruleWin\")]\n\
             <{WF}win>(?X, ?X, ?W) :-\n\
                 <{WF}move>(?X, ?Y, ?W),\n\
                 ~<{WF}win>(?Y, ?Y, ?W) .\n"
        );
        parse_eval_rules(&rls).expect("parse WF rules")
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
        assert_eq!(
            row.subject,
            Term::NamedNode(NamedNode::new(format!("{WF}p1")).unwrap())
        );
        assert_eq!(row.predicate.as_str(), format!("{WF}win"));
        assert_eq!(
            row.object,
            Term::NamedNode(NamedNode::new(format!("{WF}p1")).unwrap())
        );

        // Provenance: rule_iri = …/ruleWin, source = reifier(move(p1,p2)).
        assert_eq!(row.rule_iri, format!("{WF}ruleWin"));
        let move_reifier = mint_reifier(
            &Term::NamedNode(NamedNode::new(format!("{WF}p1")).unwrap()),
            &NamedNode::new(format!("{WF}move")).unwrap(),
            &Term::NamedNode(NamedNode::new(format!("{WF}p2")).unwrap()),
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
}
