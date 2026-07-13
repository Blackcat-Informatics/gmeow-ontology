// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit coverage for the reified n-ary lowering.

use purrdf::TermValue;

use super::{
    NaryArg, NaryAtom, NaryRule, certify_nary_termination, lower_nary_fact, lower_nary_rules,
};
use crate::physical::ChaseAdmission;
use crate::provenance::{instance_of_iri, mint_nary_reifier, nary_arg_predicate, term_display};

// ── The n-ary multi-head existential demonstrator program ─────────────────────
//
// An arity-4 EDB relation `m0` and ONE multi-head TGD that invents TWO n-ary tuples per
// body binding — `m1` (arity 3) and `m2` (arity 2) — sharing a SINGLE existential null
// `?e` across both heads (the ChaseBench/kr2024 family shape):
//
//     m1(?a, ?e, ?c) ∧ m2(?e, ?d)  ←  m0(?a, ?b, ?c, ?d)
//
// `?e` is a genuine restricted-chase shared null: not bound by the body, shared across the
// two invented tuples. Each invented tuple gets its OWN reifier existential, minted by
// tuple identity — so this exercises the multi-reifier generalization of `reified_nary_head`.

const M0: &str = "http://ex/nary/m0";
const M1: &str = "http://ex/nary/m1";
const M2: &str = "http://ex/nary/m2";
const RULE: &str = "http://ex/nary/rules/split";

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

fn v(name: &str) -> NaryArg {
    NaryArg::Var(name.to_owned())
}

/// The multi-head, shared-null n-ary TGD.
fn demo_rules() -> Vec<NaryRule> {
    vec![NaryRule {
        name: RULE.to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![
            NaryAtom {
                relation: M1.to_owned(),
                args: vec![v("?a"), v("?e"), v("?c")],
            },
            NaryAtom {
                relation: M2.to_owned(),
                args: vec![v("?e"), v("?d")],
            },
        ],
    }]
}

// ── Fact-lowering unit coverage ───────────────────────────────────────────────

#[test]
fn lower_nary_fact_reifies_onto_the_content_addressed_node() {
    let args = vec![iri("http://ex/x"), iri("http://ex/y"), iri("http://ex/z")];
    let facts = lower_nary_fact("http://ex/rel", &args).expect("ground reification");
    let reifier = mint_nary_reifier("http://ex/rel", &args).expect("mint");

    // Exactly one instanceOf typing atom + one naryArg{i} per argument, all on the reifier.
    assert_eq!(facts.len(), args.len() + 1);
    let typing = facts
        .iter()
        .find(|f| f.predicate == instance_of_iri())
        .expect("a typing atom");
    assert_eq!(term_display(&typing.subject), format!("<{reifier}>"));
    assert_eq!(typing.object, iri("http://ex/rel"));
    for (i, arg) in args.iter().enumerate() {
        let a = facts
            .iter()
            .find(|f| f.predicate == nary_arg_predicate(i))
            .expect("a positional argument atom");
        assert_eq!(term_display(&a.subject), format!("<{reifier}>"));
        assert_eq!(&a.object, arg);
    }
}

// ── Doctrinal refusals ────────────────────────────────────────────────────────

#[test]
fn lower_refuses_a_non_range_restricted_unshared_head_argument() {
    // `?e` occurs in a SINGLE head atom and no body atom — it can never be a shared null,
    // so it is a Skolem-function obligation, refused rather than mis-lowered as exact.
    let rules = vec![NaryRule {
        name: "http://ex/bad".to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![NaryAtom {
            relation: M1.to_owned(),
            args: vec![v("?a"), v("?e"), v("?c")],
        }],
    }];
    let err = lower_nary_rules(&rules).expect_err("a non-range-restricted arg must be refused");
    assert!(
        err.message().contains("not range-restricted") && err.message().contains("Skolem-function"),
        "the refusal must name the Skolem-function obligation it protects: {err}"
    );
    assert!(
        !err.message().contains('#'),
        "no process refs in the refusal message: {err}"
    );
}

#[test]
fn lower_refuses_an_empty_head() {
    let rules = vec![NaryRule {
        name: "http://ex/empty".to_owned(),
        body: vec![NaryAtom {
            relation: M0.to_owned(),
            args: vec![v("?a"), v("?b"), v("?c"), v("?d")],
        }],
        head: vec![],
    }];
    let err = lower_nary_rules(&rules).expect_err("an empty head must be refused");
    assert!(err.message().contains("empty head"), "{err}");
}

// ── Termination certificate ───────────────────────────────────────────────────

#[test]
fn demo_program_is_certified_weakly_acyclic() {
    // The relation-qualified certifier must certify the fresh-head, non-recursive program —
    // the canonical shared-`naryArg` certifier would spuriously see a cycle, which is
    // exactly why `certify_nary_termination` qualifies by relation.
    let admission = certify_nary_termination(&demo_rules()).expect("certify");
    assert!(
        matches!(admission, ChaseAdmission::WeaklyAcyclic { .. }),
        "the multi-head n-ary demonstrator must certify weakly acyclic, got {admission:?}"
    );
}
