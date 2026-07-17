// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `goal-directed` stage: run the native proof-carrying full-FOL backward engine over
//! the shipped goal-directed demonstrator corpus and fold its checked answers + proof
//! derivations into the bundle's `graph/goal-directed` named graph.
//!
//! This is the PRODUCTION consumer that makes the backward engine non-dark: without it the
//! engine (`gmeow_logic::physical::resolve_fol` + its Curry–Howard `check`) would only ever
//! run in tests. The stage calls the thin `gmeow_logic::goal_directed` façade — which
//! evaluates each shipped structured demonstrator, validates every answer's proof, and
//! returns RDF-serializable data — then routes the projected N-Triples into
//! [`GRAPH_GOAL_DIRECTED`]. `stage-snapshot`'s `assemble_carrier`
//! folds that named graph into `gmeow.gts` (the shippable deliverable), so a repo-free
//! consumer reads every proof-checked backward answer straight out of the bundle.
//!
//! The demonstrator corpus is a SET (`gmeow_logic::goal_directed::evaluate_shipped_demonstrators`),
//! not a single hardcoded program: Task 8 appends the substantial append/member, WFS
//! negation, and math sub-sort demonstrators to that corpus, and they reach the bundle
//! through this same stage with no stage change.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::goal_directed::{evaluate_shipped_demonstrators, project_goal_directed};

use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};

/// The bundle-internal named graph the checked goal-directed answers + proof derivations are
/// folded into (dual carriage with no committed byte artifact in this task — the
/// `generated/goal-directed/` fanout goldens are produced by Task 9's regenerate). A sibling
/// of `graph/reasoning`: a queryable projection of a native engine's result that ships inside
/// `gmeow.gts`, excluded from the object-level EDB (it asserts derived answers, not axioms).
pub const GRAPH_GOAL_DIRECTED: &str = "https://blackcatinformatics.ca/gmeow/graph/goal-directed";

/// The `goal-directed` pipeline stage.
pub struct GoalDirectedStage {
    consumes: Vec<String>,
}

impl GoalDirectedStage {
    /// Construct the stage. It consumes `stage-compile-logic` (the compiled EDB + rules): it
    /// sits downstream of the logic compiler in the DAG so the compiled program is available
    /// in memory, and future demonstrators that pull world facts from the compiled EDB need
    /// no new edge. The minimal shipped demonstrator is self-contained (its facts, rules, and
    /// goal are authored in `gmeow_logic::goal_directed`), so this run reads no upstream
    /// graph, but the dependency keeps the stage a proper build-DAG citizen ordered after the
    /// compiler.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-compile-logic".to_string()],
        }
    }
}

impl Default for GoalDirectedStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GoalDirectedStage {
    fn id(&self) -> &str {
        "stage-goal-directed"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn cache_policy(&self) -> CachePolicy {
        // The backward engine rebuilds this small proof-carrying result faster than a
        // structural cache hydrate would reparse + re-key its named graph, and Recompute
        // keeps the proof-check gate live on every run (an unchecked answer HARD-fails in
        // the façade). Mirrors stage-reason's Recompute rationale.
        CachePolicy::Recompute
    }
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        "goal-directed.v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Evaluate the shipped demonstrator corpus through the proof-carrying backward
        // engine. Every answer's proof is `check`-validated inside the façade, so a proof
        // that does not re-derive its answer atom HARD-fails here (fail-closed).
        let evals = evaluate_shipped_demonstrators()?;
        let nt = project_goal_directed(&evals);
        // Route the projected N-Triples into the bundle-internal graph/goal-directed named
        // graph (the stage's sole attach delta).
        let dataset = crate::stages::carrier::parse_into_graph(
            nt.as_bytes(),
            "application/n-triples",
            GRAPH_GOAL_DIRECTED,
        )?;
        let bundle = crate::bundle::bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            Arc::new(bundle),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_directed_stage_attaches_a_nonempty_goal_directed_graph() {
        let stage = GoalDirectedStage::new();
        let upstream = BTreeMap::new();
        let root = std::path::Path::new(".");
        let out = stage
            .run(StageInput {
                root,
                upstream: &upstream,
            })
            .expect("goal-directed run");
        let dataset = out.product.bundle().dataset();
        let graph = dataset.project_named_graph(GRAPH_GOAL_DIRECTED);
        let quads: Vec<_> = graph.owned_quads().collect();
        assert!(
            !quads.is_empty(),
            "the stage attaches a non-empty graph/goal-directed"
        );
        // The graph carries the minimal Peano demonstrator's ground answer atom + a
        // proof-derivation IRI (the proof reached the bundle, not just the answer).
        let has_atom = quads.iter().any(|q| {
            matches!(&q.object, purrdf::RdfTerm::Literal(l)
                if l.lexical_form == "add(s(s(zero)),s(zero),s(s(s(zero))))")
        });
        assert!(has_atom, "the ground answer atom is in graph/goal-directed");
        let has_derivation = quads
            .iter()
            .any(|q| q.predicate == "https://blackcatinformatics.ca/gmeow/goalDirectedDerivation");
        assert!(
            has_derivation,
            "a proof-derivation IRI is in graph/goal-directed"
        );

        // The structured member/append demonstrator's cons-list answer atom rode through.
        let has_structured = quads.iter().any(|q| {
            matches!(&q.object, purrdf::RdfTerm::Literal(l)
                if l.lexical_form == "member(a,cons(a,cons(b,cons(c,nil))))")
        });
        assert!(
            has_structured,
            "a structured cons-list membership answer is in graph/goal-directed"
        );

        // The three-valued SLG-WFS negation demonstrator: an `undefined` loop verdict AND both
        // founded verdicts reached the graph — SLG-WFS is observable (non-dark).
        let verdict_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedVerdict";
        let verdict_values: Vec<&str> = quads
            .iter()
            .filter(|q| q.predicate == verdict_pred)
            .filter_map(|q| match &q.object {
                purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            verdict_values.contains(&"undefined"),
            "an undefined WFS verdict is in graph/goal-directed: {verdict_values:?}"
        );
        assert!(
            verdict_values.contains(&"true") && verdict_values.contains(&"false"),
            "founded true/false WFS verdicts are in graph/goal-directed: {verdict_values:?}"
        );

        // The order-sorted (ℤ ⊑ ℝ) demonstrator's subsort-unified answer atom rode through.
        let has_subsort = quads.iter().any(
            |q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == "p(one)"),
        );
        assert!(
            has_subsort,
            "the order-sorted subsort-unified answer p(one) is in graph/goal-directed"
        );
    }
}
