// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use crate::derivation_graph::{FactKey, RuleApplication};

const P: &str = "https://example.org/p";
const Q: &str = "https://example.org/q";
const A: &str = "https://example.org/a";
const B: &str = "https://example.org/b";
const C: &str = "https://example.org/c";
const MUL: &str = "https://example.org/mul";
const RULE: &str = "https://blackcatinformatics.ca/logic/rules/p_from_q";

fn iri(s: &str) -> TermValue {
    TermValue::iri(s)
}

/// Build the rule `p(X) :- q(X)` in `dag`: its rule-IRI handle, head atom, and body
/// atoms, sharing one metavariable `X` between head and body.
fn build_rule(dag: &mut TermDag) -> (TermId, NodeId, Vec<NodeId>) {
    let (_x_meta, x) = dag.fresh_meta();
    let p = dag.intern_leaf(iri(P));
    let q = dag.intern_leaf(iri(Q));
    let head = dag.intern_app(p, vec![x]);
    let body = dag.intern_app(q, vec![x]);
    let rule_tid = dag.intern_atom(&iri(RULE));
    (rule_tid, head, vec![body])
}

/// Build the ground atom `rel(args…)` in `dag`.
fn ground_atom(dag: &mut TermDag, rel: &str, args: &[&str]) -> NodeId {
    let op = dag.intern_leaf(iri(rel));
    let arg_nodes: Vec<NodeId> = args.iter().map(|a| dag.intern_leaf(iri(a))).collect();
    dag.intern_app(op, arg_nodes)
}

/// A leaf handle for `iri_str`, so it can be carried as a proof reifier argument.
fn reifier_handle(dag: &mut TermDag, iri_str: &str) -> TermId {
    dag.intern_atom(&iri(iri_str))
}

// ── Test 1: check accepts a valid proof ─────────────────────────────────────────

#[test]
fn check_accepts_a_valid_rule_application() {
    let mut dag = TermDag::new();
    let (rule_tid, head, body) = build_rule(&mut dag);
    let q_a = ground_atom(&mut dag, Q, &[A]);
    let p_a = ground_atom(&mut dag, P, &[A]);

    let mut ctx = RuleCtx::default();
    ctx.rules.insert(
        rule_tid,
        GroundClause {
            head,
            pos: body,
            neg: vec![],
        },
    );
    ctx.asserted.insert(q_a);

    let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
    let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
    let assert_qa = proof_assert(&mut dag, q_a, reifier_tid);

    // by_rule(p(a); rule=p(X):-q(X); [assert(q(a))]) re-derives p(a).
    let proof = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qa]);
    assert_eq!(
        check(&mut dag, proof, &ctx),
        Ok(p_a),
        "a valid proof checks to its goal"
    );
}

// ── Test 2: check rejects tampered proofs ───────────────────────────────────────

#[test]
fn check_rejects_tampered_proofs() {
    let mut dag = TermDag::new();
    let (rule_tid, head, body) = build_rule(&mut dag);
    let q_a = ground_atom(&mut dag, Q, &[A]);
    let p_a = ground_atom(&mut dag, P, &[A]);
    let p_b = ground_atom(&mut dag, P, &[B]);
    let q_b = ground_atom(&mut dag, Q, &[B]);

    let mut ctx = RuleCtx::default();
    ctx.rules.insert(
        rule_tid,
        GroundClause {
            head,
            pos: body,
            neg: vec![],
        },
    );
    ctx.asserted.insert(q_a); // ONLY q(a) is asserted.

    let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
    let q_a_reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
    let assert_qa = proof_assert(&mut dag, q_a, q_a_reifier_tid);

    // (a) Tampered goal: by_rule(p(b); rule; [assert(q(a))]) re-derives p(a) ≠ p(b).
    let tampered_goal = proof_by_rule(&mut dag, p_b, rule_tid, &[assert_qa]);
    assert!(
        matches!(
            check(&mut dag, tampered_goal, &ctx),
            Err(ProofError::HeadMismatch { .. })
        ),
        "a proof whose stated goal is not the re-derived head is rejected"
    );

    // (b1) Missing premise: assert(q(b)) where only q(a) is asserted.
    let q_b_reifier = provenance::mint_nary_reifier(Q, &[iri(B)]).unwrap();
    let q_b_reifier_tid = reifier_handle(&mut dag, &q_b_reifier);
    let assert_qb = proof_assert(&mut dag, q_b, q_b_reifier_tid);
    let wrong_premise = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qb]);
    assert!(
        matches!(
            check(&mut dag, wrong_premise, &ctx),
            Err(ProofError::NotAsserted { .. })
        ),
        "a premise appealing to a non-asserted EDB fact is rejected"
    );

    // (b2) Empty subproofs where the rule demands one premise → arity mismatch.
    let no_premise = proof_by_rule(&mut dag, p_a, rule_tid, &[]);
    assert!(
        matches!(
            check(&mut dag, no_premise, &ctx),
            Err(ProofError::ArityMismatch { .. })
        ),
        "a premise count differing from the rule body arity is rejected"
    );

    // (c) Unknown rule IRI (not in the RuleCtx).
    let unknown_tid = reifier_handle(&mut dag, "https://example.org/no_such_rule");
    let unknown_rule = proof_by_rule(&mut dag, p_a, unknown_tid, &[assert_qa]);
    assert!(
        matches!(
            check(&mut dag, unknown_rule, &ctx),
            Err(ProofError::UnknownRule { .. })
        ),
        "a proof citing a rule absent from the context is rejected"
    );
}

// ── Test 2b: check rejects a forged (wrong) assert reifier ──────────────────────

#[test]
fn check_rejects_forged_assert_reifier() {
    // G2 regression: `assert(valid_goal, arbitrary_iri)` must be REJECTED by check(),
    // even though the goal genuinely IS a member of the asserted EDB — the reifier is
    // a pure function of the goal, and a caller-supplied handle that does not match
    // the recomputed one is forged provenance.
    let mut dag = TermDag::new();
    let q_a = ground_atom(&mut dag, Q, &[A]);

    let mut ctx = RuleCtx::default();
    ctx.asserted.insert(q_a);

    // An arbitrary IRI, NOT the reifier `mint_nary_reifier` mints for q(a).
    let arbitrary_tid = reifier_handle(&mut dag, "https://example.org/not-a-real-reifier");
    let forged = proof_assert(&mut dag, q_a, arbitrary_tid);
    assert!(
        matches!(
            check(&mut dag, forged, &ctx),
            Err(ProofError::ForgedReifier { .. })
        ),
        "assert(valid_goal, arbitrary_iri) must be rejected as a forged reifier"
    );

    // The correctly-minted reifier for the SAME goal passes.
    let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
    let q_a_reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
    let genuine = proof_assert(&mut dag, q_a, q_a_reifier_tid);
    assert_eq!(
        check(&mut dag, genuine, &ctx),
        Ok(q_a),
        "the correctly-minted reifier for the same goal must check"
    );
}

// ── Test 2c: G5 — check rejects a `not Undefined` negation justification ───────

#[test]
fn check_rejects_undefined_negative_premise() {
    // G5 regression: a `by_rule` proof of `p(a) :- not q(a)` whose negative premise
    // `q(a)` is merely NON-TRUE (Undefined — a member of Γ(W), not proven
    // well-founded-FALSE) must be REJECTED. Negation-as-failure is sound only over a
    // genuinely FALSE atom; "not proven true" is not the same as "proven false".
    let mut dag = TermDag::new();
    let q_a = ground_atom(&mut dag, Q, &[A]);
    let p_a = ground_atom(&mut dag, P, &[A]);
    let rule_tid = reifier_handle(&mut dag, RULE);

    // by_rule(p(a); rule; []) with the rule's sole body atom NEGATIVE (`not q(a)`).
    let proof = proof_by_rule(&mut dag, p_a, rule_tid, &[]);

    // (a) q(a) is Undefined (∈ Γ(W)): present in `not_false` ⇒ rejected.
    let mut ctx_undefined = RuleCtx::default();
    ctx_undefined.rules.insert(
        rule_tid,
        GroundClause {
            head: p_a,
            pos: vec![],
            neg: vec![q_a],
        },
    );
    ctx_undefined.not_false.insert(q_a);
    assert!(
        matches!(
            check(&mut dag, proof, &ctx_undefined),
            Err(ProofError::NegativePremiseNotFalse { .. })
        ),
        "a `not q(a)` justification where q(a) is merely Undefined must be rejected"
    );

    // (b) Control: q(a) is genuinely FALSE (absent from `not_false`) ⇒ accepted.
    let mut ctx_false = RuleCtx::default();
    ctx_false.rules.insert(
        rule_tid,
        GroundClause {
            head: p_a,
            pos: vec![],
            neg: vec![q_a],
        },
    );
    assert_eq!(
        check(&mut dag, proof, &ctx_false),
        Ok(p_a),
        "a genuinely well-founded-FALSE negative premise is a valid justification"
    );
}

// ── Test 3: derivation_iri byte-parity with mint_derivation_id / RuleApplication ─

#[test]
fn derivation_iri_is_byte_identical_to_mint_derivation_id() {
    let mut dag = TermDag::new();
    let (rule_tid, _head, _body) = build_rule(&mut dag);
    let q_a = ground_atom(&mut dag, Q, &[A]);
    let p_a = ground_atom(&mut dag, P, &[A]);

    // The reifier of q(a), content-addressed from its ground surface (no numeric ids).
    let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
    let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);
    let assert_qa = proof_assert(&mut dag, q_a, reifier_tid);
    let proof = proof_by_rule(&mut dag, p_a, rule_tid, &[assert_qa]);

    let got = derivation_iri(&dag, proof).unwrap();

    // (i) equals a hand-computed mint_derivation_id over the SAME string inputs — the
    // only inputs are the rule IRI and the source reifier IRI, never a NodeId/TermId.
    let expected = provenance::mint_derivation_id(RULE, &[q_a_reifier.as_str()]);
    assert_eq!(
        got, expected,
        "derivation_iri must reuse mint_derivation_id"
    );

    // (ii) equals the forward reasoner's RuleApplication id for the same firing.
    let app = RuleApplication::new(RULE, [FactKey::from(q_a_reifier.as_str())]);
    assert_eq!(
        got,
        app.derivation_id(),
        "proof and RuleApplication derivation ids must agree byte-for-byte"
    );
}

// ── Test 4: reify parity + non-IRI triple-term-predicate hard-fail ──────────────

#[test]
fn reify_matches_mint_nary_reifier_and_guards_non_iri_triple_predicate() {
    let mut dag = TermDag::new();
    let mul_abc = ground_atom(&mut dag, MUL, &[A, B, C]);

    let got = reify(&dag, mul_abc).unwrap();
    let expected = provenance::mint_nary_reifier(MUL, &[iri(A), iri(B), iri(C)]).unwrap();
    assert_eq!(
        got, expected,
        "reify must reuse mint_nary_reifier on the resolved args"
    );

    let op = dag.intern_leaf(iri(MUL));

    // An IRI-predicate RDF-star triple argument reifies: `term_n3` renders nested
    // triple terms recursively in RDF 1.2 non-asserting form (`<<( s p o )>>`), keeping
    // distinct nested statements distinct, so the reifier is well-defined.
    let iri_pred_triple = TermValue::Triple {
        s: Box::new(iri(A)),
        p: Box::new(iri(P)),
        o: Box::new(iri(B)),
    };
    let ok_leaf = dag.intern_leaf(iri_pred_triple);
    let with_ok_triple = dag.intern_app(op, vec![ok_leaf]);
    assert!(
        reify(&dag, with_ok_triple).is_ok(),
        "an IRI-predicate triple-term argument reifies (recursive RDF 1.2 rendering)"
    );

    // A NON-IRI triple-term predicate is the remaining hard fail
    // (`validate_triple_term_predicates` rejects a literal/blank/triple predicate).
    let non_iri_pred_triple = TermValue::Triple {
        s: Box::new(iri(A)),
        p: Box::new(TermValue::typed_literal(
            "not-an-iri".to_string(),
            crate::physical::XSD_INTEGER,
        )),
        o: Box::new(iri(B)),
    };
    let bad_leaf = dag.intern_leaf(non_iri_pred_triple);
    let with_bad_triple = dag.intern_app(op, vec![bad_leaf]);
    assert!(
        reify(&dag, with_bad_triple).is_err(),
        "a non-IRI triple-term predicate must hard-fail to reify"
    );
}

// ── Test 5: proofs hash-cons (maximal sharing) ──────────────────────────────────

#[test]
fn structurally_identical_proofs_share_one_node() {
    let mut dag = TermDag::new();
    let (rule_tid, _head, _body) = build_rule(&mut dag);
    let q_a = ground_atom(&mut dag, Q, &[A]);
    let p_a = ground_atom(&mut dag, P, &[A]);
    let q_a_reifier = provenance::mint_nary_reifier(Q, &[iri(A)]).unwrap();
    let reifier_tid = reifier_handle(&mut dag, &q_a_reifier);

    let assert1 = proof_assert(&mut dag, q_a, reifier_tid);
    let assert2 = proof_assert(&mut dag, q_a, reifier_tid);
    assert_eq!(
        assert1, assert2,
        "identical assert proofs intern to one NodeId"
    );

    let proof1 = proof_by_rule(&mut dag, p_a, rule_tid, &[assert1]);
    let proof2 = proof_by_rule(&mut dag, p_a, rule_tid, &[assert2]);
    assert_eq!(
        proof1, proof2,
        "identical by_rule proofs intern to one NodeId (maximal sharing)"
    );
}
