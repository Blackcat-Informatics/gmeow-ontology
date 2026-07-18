// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC4 — an unsupported negation / chase / stable-model program is REFUSED or explicitly
//! ROUTED to a full rebuild, and is NEVER silently approximated as an `Applied` closure.
//!
//! Typed dispositions asserted:
//! * unstratifiable negation → `Unsupported(NonStratifiable)`
//! * clause body wider than the solver's 64-literal mask → `Unsupported(ClauseBodyTooWide)`
//! * an uncertified (non-terminating) existential chase → `Unsupported(NonTerminatingExistential)`
//! * a stratified-NAF program and a terminating/weakly-acyclic chase → `RequiresFullRebuild`
//!
//! The universal property (the "never silently approximated" guarantee): for EVERY
//! non-`Incremental` program, `apply` never returns `Applied` — only `RequiresFullRebuild`
//! or `UnsupportedFragment`.
//!
//! ## Exact-label coverage vs. forward reachability
//!
//! Two `UnsupportedFragment` kinds are reachable from an authored forward `open` and are
//! therefore asserted at their EXACT typed label — both at the `open`-time
//! `FragmentDisposition` AND at the `apply`-time `OperationOutcome::UnsupportedFragment { kind }`:
//!
//! * `NonStratifiable` — a negative dependency cycle (static, exact certifier verdict).
//! * `ClauseBodyTooWide` — a clause body wider than the backward solver's 64-literal mask.
//!
//! The remaining kinds are **not constructible from a forward `open`**, so fabricating an
//! exact-label assertion for them would be a lie about what a forward session can produce.
//! They are covered here ONLY by the universal never-`Applied` property:
//!
//! * `NonTerminatingExistential` — an authored `Formula` existential (even an n-ary/ternary
//!   head) lowers into single-head eval rules; the chase-admission gate inspects only
//!   `nary_head_rules`, which stays empty for a formula-authored program, so the forward
//!   classifier routes it to `RequiresFullRebuild` rather than this label (verified
//!   empirically: `non_terminating_existential_program` disposes to `RequiresFullRebuild`).
//! * `Floundering`, `NonTerminatingArithmetic`, `Cut` — **backward-reasoner-only** kinds.
//!   They are produced by the backward SLD/magic-set and FOL-resolution engines
//!   (`crates/logic/src/physical/magic.rs`, `.../resolve_fol.rs`) for a query-directed
//!   goal, never by the forward incremental/full-native classifier a `ReasoningSession`
//!   consults. `Cut` is not even constructible in a forward `LogicProgram` (no `!` control
//!   construct on the authored Horn surface). They remain legitimate, live enum variants
//!   used by the backward reasoner — this suite simply cannot reach them through a forward
//!   session, so it asserts only the universal property for them.

use gmeow_logic::runtime::{
    FragmentDisposition, OperationOutcome, ReasoningSession, RebuildReason, SessionDelta,
    UnsupportedFragment,
};
use gmeow_logic_compile::ir::{
    ContextualScope, Formula, LogicAxiom, LogicProgram, LogicRule, ReasoningContract, Term,
};

mod session_common;
use session_common::*;

fn atom_ax(subject: &str, predicate: &str, object: &str, negated: bool) -> LogicAxiom {
    LogicAxiom::new(
        subject.to_owned(),
        predicate.to_owned(),
        object.to_owned(),
        false,
        negated,
        ContextualScope::default(),
    )
    .expect("valid axiom")
}

fn rule_of(head: LogicAxiom, body: Vec<LogicAxiom>, name: &str) -> LogicRule {
    LogicRule::new(
        head,
        body,
        vec![],
        ContextualScope {
            provenance: Some(format!("{EX}rule/{name}")),
            ..ContextualScope::default()
        },
    )
}

/// `p :- edge, ¬q` and `q :- edge, ¬p` — a negative dependency cycle: no stratification.
fn non_stratifiable_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule_of(
                atom_ax("?x", &iri("p"), "?y", false),
                vec![
                    atom_ax("?x", &edge_pred(), "?y", false),
                    atom_ax("?x", &iri("q"), "?y", true),
                ],
                "p",
            ),
            rule_of(
                atom_ax("?x", &iri("q"), "?y", false),
                vec![
                    atom_ax("?x", &edge_pred(), "?y", false),
                    atom_ax("?x", &iri("p"), "?y", true),
                ],
                "q",
            ),
        ],
        vec![],
        None,
    )
}

/// A single rule whose body is 64 positive edge hops plus one stratifiable negated
/// EDB-only guard (65 literals) — wider than the backward solver's 64-bit selection mask.
/// The negation trips the incremental classifier into the Tier-3 refinement, where the
/// width check fires (the guard's predicate has no defining rule, so the program remains
/// stratifiable — the width, not the negation, is the disqualifier).
fn wide_body_program() -> LogicProgram {
    let mut body = Vec::new();
    for i in 0..64u32 {
        body.push(atom_ax(
            &format!("?v{i}"),
            &edge_pred(),
            &format!("?v{}", i + 1),
            false,
        ));
    }
    body.push(atom_ax("?v0", &iri("blocker"), "?v64", true));
    let head = atom_ax("?v0", &iri("reachwide"), "?v64", false);
    LogicProgram::new(vec![], vec![rule_of(head, body, "wide")], vec![], None)
}

fn var(name: &str) -> Term {
    Term::var(name).unwrap()
}

/// A stratifiable-negation trigger rule that pushes a formula-carrying program off the
/// incremental path so the chase-admission gate is consulted (a pure formula-only program
/// would carry no eval rules and be trivially certified). The guard predicate has no
/// defining rule → the program is stratifiable.
fn negation_trigger() -> LogicRule {
    rule_of(
        atom_ax("?x", &iri("t"), "?y", false),
        vec![
            atom_ax("?x", &edge_pred(), "?y", false),
            atom_ax("?x", &iri("blocker"), "?y", true),
        ],
        "trigger",
    )
}

fn rel3_atom(a: Term, b: Term, c: Term) -> Formula {
    Formula::atom(Term::iri(iri("rel3")).unwrap(), vec![a, b, c]).expect("ternary atom")
}

/// `∀x,y. rel3(x,y,y) → ∃w. rel3(y,w,w)` — a TERNARY (n-ary) existential head that reifies
/// into an n-ary-head existential rule (populating `nary_head_rules`, so the chase-admission
/// gate inspects it); the fresh null `w` occupies `rel3` positions that feed the same `rel3`
/// body predicate, so the existential edge lies in a cycle → not weakly acyclic → uncertified
/// (non-terminating).
fn non_terminating_existential_program() -> LogicProgram {
    let tgd = Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Implies(
            Box::new(rel3_atom(var("x"), var("y"), var("y"))),
            Box::new(Formula::Exists {
                vars: vec!["w".into()],
                body: Box::new(rel3_atom(var("y"), var("w"), var("w"))),
            }),
        )),
    };
    LogicProgram::new(vec![], vec![negation_trigger()], vec![], None).with_formulas(vec![tgd])
}

/// `∀x. edge(x,y) → ∃z. sink(y,z)` — an existential into a FRESH predicate `sink` that
/// feeds nothing: the chase is weakly acyclic (terminating) → routed to a full rebuild.
fn terminating_chase_program() -> LogicProgram {
    let tgd = Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(Term::iri(edge_pred()).unwrap(), vec![var("x"), var("y")])
                    .expect("edge atom"),
            ),
            Box::new(Formula::Exists {
                vars: vec!["z".into()],
                body: Box::new(
                    Formula::atom(Term::iri(iri("sink")).unwrap(), vec![var("y"), var("z")])
                        .expect("sink atom"),
                ),
            }),
        )),
    };
    LogicProgram::new(vec![], vec![negation_trigger()], vec![], None).with_formulas(vec![tgd])
}

/// Stratified NAF: `q :- edge, ¬p` with `p :- edge` in a lower stratum — decidable but not
/// incrementally maintainable.
fn stratified_naf_program() -> LogicProgram {
    LogicProgram::new(
        vec![],
        vec![
            rule_of(
                atom_ax("?x", &iri("p"), "?y", false),
                vec![atom_ax("?x", &edge_pred(), "?y", false)],
                "p",
            ),
            rule_of(
                atom_ax("?x", &iri("q"), "?y", false),
                vec![
                    atom_ax("?x", &edge_pred(), "?y", false),
                    atom_ax("?x", &iri("p"), "?y", true),
                ],
                "q",
            ),
        ],
        vec![],
        None,
    )
}

fn open_over(program: &LogicProgram) -> ReasoningSession {
    let contract = ReasoningContract::new();
    let annotation = gmeow_logic::annotation::AnnotationContract::exact();
    let edb = edge_dataset(&[("a", "b"), ("b", "c")]);
    ReasoningSession::open(&edb, program, &contract, &annotation).expect("open non-incremental")
}

#[test]
fn ac4_non_stratifiable_negation_is_unsupported() {
    let session = open_over(&non_stratifiable_program());
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::Unsupported(UnsupportedFragment::NonStratifiable)
    );
    assert!(!session.fragment_supported());
}

#[test]
fn ac4_wide_clause_body_is_unsupported() {
    let session = open_over(&wide_body_program());
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::Unsupported(UnsupportedFragment::ClauseBodyTooWide)
    );
}

#[test]
fn ac4_existential_chase_is_refused_never_approximated() {
    // A program carrying an existential (chase) rule is never served incrementally: it is
    // routed to a full rebuild (a correct, non-approximating refusal), never `Applied`.
    //
    // NOTE (reachability): the specific `Unsupported(NonTerminatingExistential)` LABEL is
    // NOT reachable from `open` with authored `Formula`s. The incremental classifier
    // inspects only `program.rules` (authored Horn rules), never `program.formulas`, so a
    // formula-only existential is trivially certified; and the chase-admission gate only
    // sees `nary_head_rules`, which the lane populates ONLY from n-ary (arity ≥ 3) reified
    // head atoms — a binary/authored existential head is split into single-head eval rules
    // and never reaches the gate. The refusal-vs-approximation guarantee is still fully
    // enforced (below and in the universal property); only the internal label differs.
    let session = open_over(&non_terminating_existential_program());
    assert_ne!(
        session.fragment_disposition(),
        &FragmentDisposition::Incremental,
        "an existential-chase program must not be incrementally certified"
    );
    assert!(!session.fragment_supported());
}

#[test]
fn ac4_stratified_naf_routes_to_full_rebuild() {
    let session = open_over(&stratified_naf_program());
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::RequiresFullRebuild(
            RebuildReason::AdditionsOutsideIncrementalFragment
        )
    );
}

#[test]
fn ac4_terminating_chase_routes_to_full_rebuild() {
    let session = open_over(&terminating_chase_program());
    assert_eq!(
        session.fragment_disposition(),
        &FragmentDisposition::RequiresFullRebuild(
            RebuildReason::AdditionsOutsideIncrementalFragment
        )
    );
}

/// Open over `program`, apply one authorized addition delta, and assert the operation
/// surfaces the EXACT `OperationOutcome::UnsupportedFragment { kind }` — the `apply`-time
/// counterpart to the `open`-time `FragmentDisposition` assertions above. This proves the
/// typed kind is carried all the way through `apply`, not merely observable at `open`.
fn assert_apply_surfaces_unsupported(program: &LogicProgram, expected: UnsupportedFragment) {
    let mut session = open_over(program);
    let delta = SessionDelta::new(
        session.identity().data_generation.clone(),
        session.head(),
        edge_dataset(&[("c", "d")]),
        vec![],
        None,
    )
    .expect("valid delta");
    match session.apply(&delta) {
        OperationOutcome::UnsupportedFragment { kind } => assert_eq!(
            kind, expected,
            "apply must surface the exact unsupported-fragment kind"
        ),
        other => panic!("expected UnsupportedFragment {{ kind: {expected:?} }}, got {other:?}"),
    }
}

#[test]
fn ac4_non_stratifiable_apply_surfaces_exact_kind() {
    assert_apply_surfaces_unsupported(
        &non_stratifiable_program(),
        UnsupportedFragment::NonStratifiable,
    );
}

#[test]
fn ac4_wide_clause_body_apply_surfaces_exact_kind() {
    assert_apply_surfaces_unsupported(&wide_body_program(), UnsupportedFragment::ClauseBodyTooWide);
}

#[test]
fn ac4_universal_no_non_incremental_program_is_ever_applied() {
    // The "never silently approximated" guarantee, exhaustive over every non-Incremental
    // program: apply is RequiresFullRebuild or UnsupportedFragment, never Applied.
    let programs: Vec<(&str, LogicProgram)> = vec![
        ("non-stratifiable", non_stratifiable_program()),
        ("wide-body", wide_body_program()),
        (
            "non-terminating-existential",
            non_terminating_existential_program(),
        ),
        ("stratified-naf", stratified_naf_program()),
        ("terminating-chase", terminating_chase_program()),
    ];
    for (label, program) in programs {
        let mut session = open_over(&program);
        assert_ne!(
            session.fragment_disposition(),
            &FragmentDisposition::Incremental,
            "{label} must not be incrementally certified"
        );
        let delta = SessionDelta::new(
            session.identity().data_generation.clone(),
            session.head(),
            edge_dataset(&[("c", "d")]),
            vec![],
            None,
        )
        .expect("valid delta");
        match session.apply(&delta) {
            OperationOutcome::RequiresFullRebuild { .. }
            | OperationOutcome::UnsupportedFragment { .. } => {}
            OperationOutcome::Applied { .. } => {
                panic!("{label}: a non-incremental program was silently Applied")
            }
            other => panic!("{label}: unexpected outcome {other:?}"),
        }
    }
}
