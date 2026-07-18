// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Termination-class ladder demonstrators shipped into `gmeow.gts`.
//!
//! Each demonstrator is a small general existential program, authored in the
//! `logic:existential#` rule surface, that witnesses one chase-termination class the
//! constant-refined OWL-restriction fragment cannot express. Placed in its own reasoning
//! world, each program's per-world [`crate::physical::ChaseAdmission`] certificate ships
//! as a `chase.certificate.{jointly,super-weakly,model-summarizing}-acyclic` finding —
//! so the shipped bundle carries the reasoner's full termination-certification power, not
//! just the weakly-acyclic class its own foundational restrictions need (the engine
//! dogfooding its own reasoning capability into the deliverable).
//!
//! The programs are exactly the witnesses pinned by the certifier unit tests in
//! [`crate::physical`]: joint-acyclicity's guard-split, super-weak-acyclicity's
//! symmetric head (whose occurs-check breaks a cycle joint acyclicity reports), and
//! model-summarizing-acyclicity's swap-diagonal (which every structural class refuses but
//! the engine's own fixpoint certifies). Each must certify to its class or `stage-reason`
//! hard-fails, so they are frozen against the certifier tests.

use crate::reasoning_graphs::{
    GRAPH_DEMO_JOINTLY_ACYCLIC, GRAPH_DEMO_MODEL_SUMMARIZING, GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC,
};

/// `type(x,C) ∧ type(x,D) → ∃y. p(x,y)` and `p(x,y) → type(y,C)`.
/// Weak acyclicity refuses (position cycle); joint acyclicity certifies (the p-object
/// null becomes C but never D, so it can never re-bind the C∧D guard).
const JOINTLY_ACYCLIC_TTL: &str = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix lx:  <https://blackcatinformatics.ca/gmeow/logic/existential#> .
@prefix demo: <https://blackcatinformatics.ca/gmeow/demo/termination#> .

demo:ja-guard rdf:type lx:ExistentialRule ;
    lx:body demo:ja-guard-b0, demo:ja-guard-b1 ;
    lx:head demo:ja-guard-h0 .
demo:ja-guard-b0 lx:s "?x" ; lx:p demo:type ; lx:o demo:C .
demo:ja-guard-b1 lx:s "?x" ; lx:p demo:type ; lx:o demo:D .
demo:ja-guard-h0 lx:s "?x" ; lx:p demo:p ; lx:o "?y" .

demo:ja-feedback rdf:type lx:ExistentialRule ;
    lx:body demo:ja-feedback-b0 ;
    lx:head demo:ja-feedback-h0 .
demo:ja-feedback-b0 lx:s "?x" ; lx:p demo:p ; lx:o "?y" .
demo:ja-feedback-h0 lx:s "?y" ; lx:p demo:type ; lx:o demo:C .
"#;

/// `type(x,C) → ∃y. p(x,y) ∧ p(y,x)` and `p(x,x) → type(x,C)`.
/// Weak and joint acyclicity both refuse; super-weak acyclicity certifies (the null is
/// placed directly at both `p` slots, so the occurs-check blocks both head atoms from
/// unifying with the diagonal body `p(x,x)`).
const SUPER_WEAKLY_ACYCLIC_TTL: &str = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix lx:  <https://blackcatinformatics.ca/gmeow/logic/existential#> .
@prefix demo: <https://blackcatinformatics.ca/gmeow/demo/termination#> .

demo:swa-invent rdf:type lx:ExistentialRule ;
    lx:body demo:swa-invent-b0 ;
    lx:head demo:swa-invent-h0, demo:swa-invent-h1 .
demo:swa-invent-b0 lx:s "?x" ; lx:p demo:type ; lx:o demo:C .
demo:swa-invent-h0 lx:s "?x" ; lx:p demo:p ; lx:o "?y" .
demo:swa-invent-h1 lx:s "?y" ; lx:p demo:p ; lx:o "?x" .

demo:swa-diagonal rdf:type lx:ExistentialRule ;
    lx:body demo:swa-diagonal-b0 ;
    lx:head demo:swa-diagonal-h0 .
demo:swa-diagonal-b0 lx:s "?x" ; lx:p demo:p ; lx:o "?x" .
demo:swa-diagonal-h0 lx:s "?x" ; lx:p demo:type ; lx:o demo:C .
"#;

/// `p(x,x) → ∃y. p(x,y)` and `p(x,y) → p(y,x)`.
/// Every structural class refuses (the swap rule launders the null past super-weak
/// acyclicity's unification); the self-hosted model-summarizing check certifies it — on
/// the critical instance the summarizing null never forms the diagonal `p(n,n)`.
const MODEL_SUMMARIZING_TTL: &str = r#"
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix lx:  <https://blackcatinformatics.ca/gmeow/logic/existential#> .
@prefix demo: <https://blackcatinformatics.ca/gmeow/demo/termination#> .

demo:msa-invent rdf:type lx:ExistentialRule ;
    lx:body demo:msa-invent-b0 ;
    lx:head demo:msa-invent-h0 .
demo:msa-invent-b0 lx:s "?x" ; lx:p demo:p ; lx:o "?x" .
demo:msa-invent-h0 lx:s "?x" ; lx:p demo:p ; lx:o "?y" .

demo:msa-swap rdf:type lx:ExistentialRule ;
    lx:body demo:msa-swap-b0 ;
    lx:head demo:msa-swap-h0 .
demo:msa-swap-b0 lx:s "?x" ; lx:p demo:p ; lx:o "?y" .
demo:msa-swap-h0 lx:s "?y" ; lx:p demo:p ; lx:o "?x" .
"#;

/// The three termination-ladder demonstrators, each as `(reasoning-world IRI, Turtle)`,
/// for the pipeline to root into its own named graph in the object-level reasoning EDB.
pub fn termination_ladder_demonstrators() -> [(&'static str, &'static str); 3] {
    [
        (GRAPH_DEMO_JOINTLY_ACYCLIC, JOINTLY_ACYCLIC_TTL),
        (GRAPH_DEMO_SUPER_WEAKLY_ACYCLIC, SUPER_WEAKLY_ACYCLIC_TTL),
        (GRAPH_DEMO_MODEL_SUMMARIZING, MODEL_SUMMARIZING_TTL),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::ChaseAdmission;

    #[test]
    fn each_demonstrator_parses_and_certifies_to_its_class() {
        // Freeze each demonstrator turtle against its intended termination class — a typo
        // or a drifted witness would hard-fail `stage-reason` at `make sync`, so catch it
        // here on the fast path.
        for (i, (graph, ttl)) in termination_ladder_demonstrators().iter().enumerate() {
            let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
                .unwrap_or_else(|e| panic!("{graph}: demonstrator turtle must parse: {e}"));
            let rules: Vec<_> = crate::reason::dl::authored_existential_rules(dataset.as_ref())
                .unwrap_or_else(|e| panic!("{graph}: demonstrator rules must assemble: {e}"))
                .into_values()
                .flatten()
                .collect();
            assert!(!rules.is_empty(), "{graph}: demonstrator rules must parse");
            let cert = ChaseAdmission::certify(&rules);
            let ok = match i {
                0 => matches!(cert, ChaseAdmission::JointlyAcyclic { .. }),
                1 => matches!(cert, ChaseAdmission::SuperWeaklyAcyclic { .. }),
                2 => matches!(cert, ChaseAdmission::ModelSummarizingAcyclic { .. }),
                _ => unreachable!(),
            };
            assert!(
                ok,
                "{graph}: demonstrator must certify to its class, got {cert:?}"
            );
        }
    }
}
