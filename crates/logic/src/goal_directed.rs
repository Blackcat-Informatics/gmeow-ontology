// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The goal-directed (backward) demonstrator façade — the production surface that makes
//! the proof-carrying full-FOL backward engine non-dark.
//!
//! The proof-carrying backward engine (`crate::physical::resolve_fol`) and its
//! Curry–Howard proof checker (`crate::physical::proof::check`) are `pub(crate)` behind
//! the private `physical` module, so no other crate can reach them. This module is the
//! single thin, honest `pub` façade over them: it holds a corpus of shipped
//! *goal-directed demonstrator programs* (structured — function-symbol — logic programs
//! the flat query text-parser cannot express, so they are built directly against the
//! resolver's `TermDag`), evaluates each through [`resolve_fol`], validates every answer's
//! proof with [`check`], and projects the checked answers + their content-addressed
//! derivation IRIs into RDF-serializable data the `gmeow-pipeline`
//! `stage-goal-directed` folds into `graph/goal-directed` of `gmeow.gts`.
//!
//! It is NOT a fork of the engine: it constructs programs and reads back the engine's own
//! [`FolOutcome`], never re-implementing resolution. Task 8 appends the substantial
//! demonstrators (append/member, WFS negation, math sub-sort) to
//! [`shipped_demonstrators`]; this module ships the minimal Peano-addition demonstrator so
//! the stage has a real, proof-checked answer to fold.

use std::collections::BTreeMap;

use purrdf::TermValue;

use crate::physical::proof::{check, structured_derivation_iri};
use crate::physical::resolve_fol::{
    FolClause, FolControl, FolLit, FolProgram, render, resolve_fol,
};
use crate::physical::term_dag::TermDag;
use crate::physical::unify::SortContext;
use crate::query_ir::Budget;

/// The gmeow namespace every projected goal-directed IRI/predicate lives under.
const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
/// The XSD boolean datatype IRI for the proof-checked flag.
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `rdf:type`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// One checked answer to a demonstrator's goal: the ground answer atom surface, the goal
/// variable bindings, the content-addressed derivation (proof) IRI, and whether the proof
/// [`check`]s to exactly that atom. Every field is RDF-serializable (strings), so the
/// pipeline can fold it without reaching into the engine's private term handles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedAnswer {
    /// The ground answer atom rendered to its functional surface, e.g.
    /// `add(s(s(zero)),s(zero),s(s(s(zero))))`.
    pub atom: String,
    /// The goal variable → resolved sub-term surface map (deterministic, sorted keys).
    pub bindings: BTreeMap<String, String>,
    /// The content-addressed derivation IRI of this answer's proof
    /// ([`derivation_iri`] — byte-identical to the forward reasoner's rule-application id).
    pub derivation_iri: String,
    /// Whether the proof [`check`]ed and re-derived exactly [`Self::atom`]. Always `true`
    /// for a shipped answer (a proof that fails to check HARD-fails the evaluation).
    pub proof_checks: bool,
}

/// One evaluated goal-directed demonstrator: its stable name, prose description, rendered
/// goal template, budget status, and every proof-checked answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalDirectedEvaluation {
    /// The stable demonstrator name (a URI path segment; also the query IRI local part).
    pub name: String,
    /// The prose description of what the demonstrator demonstrates.
    pub description: String,
    /// The rendered goal template (free metavariables shown as `?n`), e.g.
    /// `add(s(s(zero)),s(zero),?0)`.
    pub goal: String,
    /// The budget status of the resolution (`ok` / `partial` / `exhausted`).
    pub status: String,
    /// The proof-checked answers, sorted by [`GoalDirectedAnswer::atom`] for determinism.
    pub answers: Vec<GoalDirectedAnswer>,
}

/// Evaluate every shipped goal-directed demonstrator: run each through the proof-carrying
/// backward resolver, [`check`] every answer's proof (a proof that does not re-derive its
/// answer atom HARD-fails — no unchecked answer ever ships), and return the deterministic,
/// RDF-serializable evaluations. This is the pipeline stage's single entry point.
pub fn evaluate_shipped_demonstrators() -> gmeow_errors::Result<Vec<GoalDirectedEvaluation>> {
    let mut evals = Vec::new();
    for builder in shipped_demonstrators() {
        evals.push(evaluate_demonstrator(builder)?);
    }
    evals.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(evals)
}

/// A shipped demonstrator: its stable name, description, and a builder that interns the
/// structured program into a fresh [`TermDag`] and returns it alongside its [`FolProgram`].
struct Demonstrator {
    name: &'static str,
    description: &'static str,
    build: fn() -> (TermDag, FolProgram),
}

/// The shipped demonstrator corpus. Task 8 appends the substantial structured / WFS /
/// math-sub-sort demonstrators here; this is deliberately a SET so a second demonstrator is
/// a one-line addition, never a stage rewrite.
fn shipped_demonstrators() -> Vec<Demonstrator> {
    vec![Demonstrator {
        name: "peano-add",
        description: "Peano addition by structural recursion — the minimal structured \
                      goal-directed demonstrator: one fact clause add(zero,Y,Y), one rule \
                      clause add(s(X),Y,s(Z)) :- add(X,Y,Z), and the query \
                      ?- add(s(s(zero)),s(zero),R), backward-resolved to R = s(s(s(zero))) \
                      with a Curry–Howard-checkable proof.",
        build: build_peano_add,
    }]
}

/// Evaluate one demonstrator: resolve its goal, then validate + project each answer.
fn evaluate_demonstrator(demo: Demonstrator) -> gmeow_errors::Result<GoalDirectedEvaluation> {
    let (mut dag, program) = (demo.build)();
    // Render the goal template BEFORE resolution (free metavariables still present).
    let goal = render(&dag, program.goal);
    let outcome = match resolve_fol(
        &mut dag,
        &program,
        &SortContext::default(),
        &Budget::default(),
    )? {
        FolControl::Decided(outcome) => outcome,
        FolControl::Unsupported(kind) => {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed demonstrator {:?} is unsupported by the backward engine: {kind:?}",
                    demo.name
                ),
            }));
        }
    };
    let status = outcome.status.as_str().to_owned();
    let mut answers = Vec::with_capacity(outcome.answers.len());
    for ans in &outcome.answers {
        // Curry–Howard check: the proof MUST re-derive exactly the answer atom. A proof
        // that fails to check, or checks to a different atom, is a hard fail — the whole
        // point of shipping proof objects is that every shipped answer is proof-carrying.
        let checked = check(&mut dag, ans.proof, &outcome.rule_ctx).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed demonstrator {:?} answer proof failed to check: {e:?}",
                    demo.name
                ),
            })
        })?;
        if checked != ans.atom {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Physical {
                detail: format!(
                    "goal-directed demonstrator {:?} proof re-derives a different atom than its answer",
                    demo.name
                ),
            }));
        }
        let derivation_iri = structured_derivation_iri(&dag, ans.proof)?;
        answers.push(GoalDirectedAnswer {
            atom: render(&dag, ans.atom),
            bindings: ans.bindings.clone(),
            derivation_iri,
            proof_checks: true,
        });
    }
    answers.sort_by(|a, b| a.atom.cmp(&b.atom));
    Ok(GoalDirectedEvaluation {
        name: demo.name.to_owned(),
        description: demo.description.to_owned(),
        goal,
        status,
        answers,
    })
}

/// The Peano-addition demonstrator: `add(zero,Y,Y). add(s(X),Y,s(Z)) :- add(X,Y,Z).`
/// with the goal `?- add(s(s(zero)),s(zero),R).` interned into a fresh [`TermDag`]. The
/// function symbols (`add`/`s`/`zero`) are program-local surfaces, not dereferenceable
/// terms; the rule IRIs are gmeow-namespaced so the derivation identity is stable.
fn build_peano_add() -> (TermDag, FolProgram) {
    let mut dag = TermDag::new();
    let leaf = |dag: &mut TermDag, s: &str| dag.intern_leaf(TermValue::iri(s.to_owned()));
    let app = |dag: &mut TermDag, pred: &str, args: Vec<_>| {
        let op = dag.intern_leaf(TermValue::iri(pred.to_owned()));
        dag.intern_app(op, args)
    };

    let zero = leaf(&mut dag, "zero");

    // Fact clause: add(zero, Y, Y).
    let (_, y) = dag.fresh_meta();
    let fact_head = app(&mut dag, "add", vec![zero, y, y]);
    let fact_rule = dag.intern_atom(&TermValue::iri(rule_iri("peano-add", 0)));

    // Rule clause: add(s(X), Y, s(Z)) :- add(X, Y, Z).
    let (_, x) = dag.fresh_meta();
    let (_, y2) = dag.fresh_meta();
    let (_, z) = dag.fresh_meta();
    let sx = app(&mut dag, "s", vec![x]);
    let sz = app(&mut dag, "s", vec![z]);
    let rule_head = app(&mut dag, "add", vec![sx, y2, sz]);
    let rule_body = app(&mut dag, "add", vec![x, y2, z]);
    let step_rule = dag.intern_atom(&TermValue::iri(rule_iri("peano-add", 1)));

    // Goal: add(s(s(zero)), s(zero), R).
    let s_zero = app(&mut dag, "s", vec![zero]);
    let ss_zero = app(&mut dag, "s", vec![s_zero]);
    let s_zero_g = app(&mut dag, "s", vec![zero]);
    let (_, r) = dag.fresh_meta();
    let goal = app(&mut dag, "add", vec![ss_zero, s_zero_g, r]);

    let program = FolProgram {
        clauses: vec![
            FolClause {
                head: fact_head,
                body: vec![],
                rule_iri: fact_rule,
            },
            FolClause {
                head: rule_head,
                body: vec![FolLit::Pos(rule_body)],
                rule_iri: step_rule,
            },
        ],
        goal,
        goal_vars: vec![(r, "R".to_owned())],
        meta_sorts: std::collections::HashMap::new(),
    };
    (dag, program)
}

/// The gmeow-namespaced content-addressing anchor for a demonstrator clause's rule IRI.
fn rule_iri(name: &str, idx: usize) -> String {
    format!("{GMEOW}goal-directed/{name}/rule/{idx}")
}

/// The query individual IRI of a demonstrator.
fn query_iri(name: &str) -> String {
    format!("{GMEOW}goal-directed/{name}")
}

/// The `n`-th answer individual IRI of a demonstrator.
fn answer_iri(name: &str, idx: usize) -> String {
    format!("{GMEOW}goal-directed/{name}/answer/{idx}")
}

/// Project evaluated demonstrators into deterministic (sorted) N-Triples for the
/// `graph/goal-directed` fold. Each demonstrator is a `gmeow:GoalDirectedQuery` carrying
/// its description, goal template, and status; each answer is a `gmeow:GoalDirectedAnswer`
/// carrying its ground atom, bindings, the proof-derivation IRI, and the proof-checked
/// flag. No new predicate is invented beyond this small self-consistent set; the goal /
/// atom / binding surfaces ride as plain string literals, the derivation as an IRI.
pub fn project_goal_directed(evals: &[GoalDirectedEvaluation]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let p = |pred: &str| format!("{GMEOW}{pred}");
    for eval in evals {
        let q = query_iri(&eval.name);
        lines.push(triple_iri(&q, RDF_TYPE, &p("GoalDirectedQuery")));
        lines.push(triple_lit(&q, &p("goalDirectedName"), &eval.name));
        lines.push(triple_lit(
            &q,
            &p("goalDirectedDescription"),
            &eval.description,
        ));
        lines.push(triple_lit(&q, &p("goalDirectedGoal"), &eval.goal));
        lines.push(triple_lit(&q, &p("goalDirectedStatus"), &eval.status));
        for (idx, ans) in eval.answers.iter().enumerate() {
            let a = answer_iri(&eval.name, idx);
            lines.push(triple_iri(&q, &p("hasGoalDirectedAnswer"), &a));
            lines.push(triple_iri(&a, RDF_TYPE, &p("GoalDirectedAnswer")));
            lines.push(triple_lit(&a, &p("goalDirectedAtom"), &ans.atom));
            for (var, surface) in &ans.bindings {
                lines.push(triple_lit(
                    &a,
                    &p("goalDirectedBinding"),
                    &format!("{var} = {surface}"),
                ));
            }
            lines.push(triple_iri(
                &a,
                &p("goalDirectedDerivation"),
                &ans.derivation_iri,
            ));
            lines.push(triple_typed(
                &a,
                &p("goalDirectedProofChecked"),
                if ans.proof_checks { "true" } else { "false" },
                XSD_BOOLEAN,
            ));
        }
    }
    lines.sort();
    lines.dedup();
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// One `<s> <p> <o> .` IRI-object triple line.
fn triple_iri(s: &str, p: &str, o: &str) -> String {
    format!("<{s}> <{p}> <{o}> .")
}

/// One `<s> <p> "lit" .` plain-string-literal triple line (with N-Triples escaping).
fn triple_lit(s: &str, p: &str, lit: &str) -> String {
    format!("<{s}> <{p}> \"{}\" .", escape_literal(lit))
}

/// One `<s> <p> "lex"^^<dt> .` typed-literal triple line.
fn triple_typed(s: &str, p: &str, lex: &str, dt: &str) -> String {
    format!("<{s}> <{p}> \"{}\"^^<{dt}> .", escape_literal(lex))
}

/// Escape a string for an N-Triples literal (backslash, quote, and the C0 controls that
/// have canonical escapes).
fn escape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peano_add_demonstrator_resolves_and_proof_checks() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let peano = evals
            .iter()
            .find(|e| e.name == "peano-add")
            .expect("the peano-add demonstrator is shipped");
        assert_eq!(peano.status, "ok");
        assert_eq!(peano.answers.len(), 1, "2 + 1 has exactly one answer");
        let ans = &peano.answers[0];
        assert_eq!(
            ans.bindings.get("R").map(String::as_str),
            Some("s(s(s(zero)))"),
            "2 + 1 = 3 in Peano successors"
        );
        assert_eq!(ans.atom, "add(s(s(zero)),s(zero),s(s(s(zero))))");
        assert!(ans.proof_checks, "the shipped answer is proof-checked");
        assert!(
            ans.derivation_iri.starts_with("https://"),
            "the answer carries a content-addressed derivation IRI: {}",
            ans.derivation_iri
        );
    }

    #[test]
    fn projection_carries_answer_atom_and_derivation_iri() {
        let evals = evaluate_shipped_demonstrators().expect("evaluate demonstrators");
        let nt = project_goal_directed(&evals);
        assert!(
            nt.contains("GoalDirectedQuery"),
            "the projection types the query"
        );
        assert!(
            nt.contains("add(s(s(zero)),s(zero),s(s(s(zero))))"),
            "the projection carries the ground answer atom:\n{nt}"
        );
        assert!(
            nt.contains("goalDirectedDerivation"),
            "the projection carries the proof-derivation IRI predicate"
        );
        // Deterministic: a second projection is byte-identical.
        let nt2 = project_goal_directed(&evals);
        assert_eq!(nt, nt2, "the projection is byte-stable");
    }
}
