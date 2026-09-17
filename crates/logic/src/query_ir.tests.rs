// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// ── Variable vs constant classification ───────────────────────────────────

#[test]
fn variable_uppercase_first() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, ex:a).\n\
             ?- ex:p(X, Y).\n",
    )
    .unwrap();
    let fact = &prog.rules[0];
    assert_eq!(fact.head.args[0], QTerm::Var("X".to_owned()));
    assert_eq!(
        fact.head.args[1],
        QTerm::Const("<https://example.org/a>".to_owned())
    );
}

// ── RDF 1.2 quoted-triple goal argument ───────────────────────────────────

#[test]
fn quoted_triple_goal_argument_parses_ground_components() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p ex:o0 )>>, C).\n",
    )
    .unwrap();
    let arg = &prog.goal.atoms[0].args[0];
    let QTerm::Triple { s, p, o } = arg else {
        panic!("expected a quoted-triple term, got {arg:?}");
    };
    assert_eq!(**s, QTerm::Const("<https://example.org/s0>".to_owned()));
    assert_eq!(**p, QTerm::Const("<https://example.org/p>".to_owned()));
    assert_eq!(**o, QTerm::Const("<https://example.org/o0>".to_owned()));
    // The unbound candidate stays a variable.
    assert_eq!(prog.goal.atoms[0].args[1], QTerm::Var("C".to_owned()));
}

#[test]
fn quoted_triple_nested_object_parses() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p <<( ex:a ex:q ex:b )>> )>>, C).\n",
    )
    .unwrap();
    let QTerm::Triple { o, .. } = &prog.goal.atoms[0].args[0] else {
        panic!("expected a quoted-triple term");
    };
    assert!(matches!(**o, QTerm::Triple { .. }), "nested object triple");
}

#[test]
fn quoted_triple_with_embedded_variable_is_rejected() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( X ex:p ex:o0 )>>, C).\n",
    )
    .unwrap_err();
    assert!(
        format!("{err:?}").contains("ground"),
        "embedded variable must be rejected as non-ground: {err:?}"
    );
}

#[test]
fn quoted_triple_with_non_iri_predicate_is_rejected() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 \"lit\" ex:o0 )>>, C).\n",
    )
    .unwrap_err();
    assert!(
        format!("{err:?}").contains("predicate must be an IRI"),
        "non-IRI predicate must be rejected: {err:?}"
    );
}

#[test]
fn quoted_triple_wrong_arity_is_rejected() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:vector(<<( ex:s0 ex:p )>>, C).\n",
    )
    .unwrap_err();
    assert!(
        format!("{err:?}").contains("exactly 3 components"),
        "a 2-component quoted triple must be rejected: {err:?}"
    );
}

#[test]
fn variable_underscore_first() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(_Z, ex:b).\n\
             ?- ex:p(_Z, ex:b).\n",
    )
    .unwrap();
    assert_eq!(prog.rules[0].head.args[0], QTerm::Var("_Z".to_owned()));
}

// ── Prefix expansion correctness ──────────────────────────────────────────

#[test]
fn prefix_expansion_is_correct() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/profiles/positive-horn/').\n\
             ex:parentOf(ex:alice, ex:bob).\n\
             ?- ex:parentOf(ex:alice, Y).\n",
    )
    .unwrap();
    let fact = &prog.rules[0];
    assert_eq!(
        fact.head.pred,
        "https://example.org/profiles/positive-horn/parentOf"
    );
    assert_eq!(
        fact.head.args[0],
        QTerm::Const("<https://example.org/profiles/positive-horn/alice>".to_owned())
    );
    assert_eq!(
        fact.head.args[1],
        QTerm::Const("<https://example.org/profiles/positive-horn/bob>".to_owned())
    );
}

// ── Malformed prefix IRI: must Err, never panic on the quote-strip slice ───

#[test]
fn prefix_lone_single_quote_errs_not_panics() {
    // A lone `'` satisfies both starts_with/ends_with; without the len guard
    // the `iri_part[1..len-1]` strip is `[1..0]` and panics. Must be a clean Err.
    let err = parse_prefix_directive("prefix(ex, ')").unwrap_err();
    assert!(
        err.message().contains("single-quoted"),
        "unexpected error: {err}"
    );
}

#[test]
fn prefix_empty_quotes_errs() {
    // `''` strips to the empty IRI — caught by the empty-IRI check, also an Err.
    let err = parse_prefix_directive("prefix(ex, '')").unwrap_err();
    assert!(err.message().contains("empty"), "unexpected error: {err}");
}

// ── Prefix + 2 rules + goal parse ─────────────────────────────────────────

#[test]
fn parse_prefix_two_rules_and_goal() {
    let src = "\
:- prefix(ex, 'https://example.org/').\
\n\
ex:parentOf(ex:alice, ex:bob).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Y).\
\n\
ex:ancestorOf(X, Y) :- ex:parentOf(X, Z), ex:ancestorOf(Z, Y).\
\n\
?- ex:ancestorOf(ex:alice, Y).\
";
    let prog = parse_query_program(src).unwrap();
    assert_eq!(prog.rules.len(), 3, "1 fact + 2 rules");
    assert_eq!(prog.goal.atoms.len(), 1);
    assert_eq!(prog.goal.atoms[0].pred, "https://example.org/ancestorOf");
}

// ── Fact parse ────────────────────────────────────────────────────────────

#[test]
fn parse_fact_no_body() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n\
             ?- ex:p(ex:a, ex:b).\n",
    )
    .unwrap();
    assert_eq!(prog.rules.len(), 1);
    let fact = &prog.rules[0];
    assert!(fact.body.is_empty(), "fact must have empty body");
    assert_eq!(fact.head.pred, "https://example.org/p");
}

// ── Cut in body ───────────────────────────────────────────────────────────

#[test]
fn parse_cut_in_body() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:q(X, Y), !, ex:r(X, Y).\n\
             ?- ex:p(X, Y).\n",
    )
    .unwrap();
    let rule = &prog.rules[0];
    assert_eq!(rule.body.len(), 3);
    assert_eq!(
        rule.body[0],
        QBodyLit::Atom(rule.body[0].clone().into_atom().unwrap())
    );
    assert_eq!(rule.body[1], QBodyLit::Cut);
    assert_eq!(
        rule.body[2],
        QBodyLit::Atom(rule.body[2].clone().into_atom().unwrap())
    );
}

// ── Negation-as-failure in body ───────────────────────────────────────────

#[test]
fn parse_backslash_plus_and_not_negation() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:q(X, Y), \\+ ex:r(X, Y), not ex:s(X, Y).\n\
             ?- ex:p(X, Y).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 3, "one positive + two negated literals");
    assert!(matches!(body[0], QBodyLit::Atom(_)), "q is positive");
    match &body[1] {
        QBodyLit::Neg(a) => assert_eq!(a.pred, "https://example.org/r"),
        other => panic!("expected Neg(r), got {other:?}"),
    }
    match &body[2] {
        QBodyLit::Neg(a) => assert_eq!(a.pred, "https://example.org/s"),
        other => panic!("expected Neg(s), got {other:?}"),
    }
}

#[test]
fn parse_not_prefixed_predicate_is_not_mistaken_for_negation() {
    // A predicate whose local name merely starts with `not` (here `notation`) must NOT
    // be parsed as a negation operator — the `not` keyword requires a following space.
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- ex:notation(X, Y).\n\
             ?- ex:p(X, Y).\n",
    )
    .unwrap();
    match &prog.rules[0].body[0] {
        QBodyLit::Atom(a) => assert_eq!(a.pred, "https://example.org/notation"),
        other => panic!("expected a positive notation atom, got {other:?}"),
    }
}

// ── Reject: no goal ───────────────────────────────────────────────────────

#[test]
fn reject_program_with_no_goal() {
    let result = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b).\n",
    );
    assert!(result.is_err(), "must reject program with no goal");
    assert!(result.unwrap_err().message().contains("no ?- goal"));
}

// ── Reject: malformed clause ──────────────────────────────────────────────

#[test]
fn reject_malformed_atom_missing_parens() {
    let result = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p ex:a ex:b.\n\
             ?- ex:p(ex:a, ex:b).\n",
    );
    assert!(result.is_err(), "must reject atom missing parentheses");
}

#[test]
fn parse_ternary_atom_is_accepted() {
    // Arity is now arbitrary (≥1): n-ary IDB predicates like get/3 are valid
    // (G2a). EDB RDF atoms remain binary naturally.
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(ex:a, ex:b, ex:c).\n\
             ?- ex:p(ex:a, ex:b, ex:c).\n",
    )
    .unwrap();
    assert_eq!(prog.rules[0].head.args.len(), 3);
    assert_eq!(prog.goal.atoms[0].args.len(), 3);
}

// ── Arithmetic / comparison builtin parsing (G2a) ───────────────────

#[test]
fn parse_is_arith_builtin_roundtrip() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:len(L, N) :- ex:rest(L, R), ex:len(R, M), N is M + 1.\n\
             ?- ex:len(ex:l0, N).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 3, "two atoms + one builtin");
    match &body[2] {
        QBodyLit::Builtin(QBuiltin::Is {
            target,
            lhs,
            op,
            rhs,
        }) => {
            assert_eq!(*target, QTerm::Var("N".to_owned()));
            assert_eq!(*lhs, QTerm::Var("M".to_owned()));
            assert_eq!(*op, ArithOp::Add);
            assert_eq!(*rhs, QTerm::Num(1));
        }
        other => panic!("expected Is builtin, got {other:?}"),
    }
}

#[test]
fn parse_distinguishes_truncating_and_exact_division_operators() {
    // `//` parses to the truncating-integer operator; a lone `/` to exact ℚ
    // division. `//` (multi-char, checked first) always wins over `/`.
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(A, B) :- A is 6 // 4, B is 6 / 4.\n\
             ?- ex:p(A, B).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 2);
    assert_eq!(
        body[0],
        QBodyLit::Builtin(QBuiltin::Is {
            target: QTerm::Var("A".to_owned()),
            lhs: QTerm::Num(6),
            op: ArithOp::Div,
            rhs: QTerm::Num(4),
        })
    );
    assert_eq!(
        body[1],
        QBodyLit::Builtin(QBuiltin::Is {
            target: QTerm::Var("B".to_owned()),
            lhs: QTerm::Num(6),
            op: ArithOp::ExactDiv,
            rhs: QTerm::Num(4),
        })
    );
    assert_eq!(ArithOp::Div.token(), "//");
    assert_eq!(ArithOp::ExactDiv.token(), "/");
}

#[test]
fn parse_is_single_operand_lowers_to_canonical_additive_identity() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:p(X, Y) :- X is 1, Y is 2.\n\
             ?- ex:p(X, Y).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 2);
    assert_eq!(
        body[0],
        QBodyLit::Builtin(QBuiltin::Is {
            target: QTerm::Var("X".to_owned()),
            lhs: QTerm::Num(1),
            op: ArithOp::Add,
            rhs: QTerm::Num(0),
        })
    );
    assert_eq!(
        body[1],
        QBodyLit::Builtin(QBuiltin::Is {
            target: QTerm::Var("Y".to_owned()),
            lhs: QTerm::Num(2),
            op: ArithOp::Add,
            rhs: QTerm::Num(0),
        })
    );
}

#[test]
fn parse_compare_builtin_roundtrip() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:pos(L, N, X) :- N > 0, ex:rest(L, X).\n\
             ?- ex:pos(ex:l0, 1, X).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 2, "one builtin + one atom");
    match &body[0] {
        QBodyLit::Builtin(QBuiltin::Compare { lhs, op, rhs }) => {
            assert_eq!(*lhs, QTerm::Var("N".to_owned()));
            assert_eq!(*op, CmpOp::Gt);
            assert_eq!(*rhs, QTerm::Num(0));
        }
        other => panic!("expected Compare builtin, got {other:?}"),
    }
}

#[test]
fn parse_combined_builtins_split_on_top_comma() {
    // `N is M + 1, N > 0` must split into two body literals.
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ex:r(M, N) :- N is M + 1, N > 0.\n\
             ?- ex:r(ex:a, N).\n",
    )
    .unwrap();
    let body = &prog.rules[0].body;
    assert_eq!(body.len(), 2);
    assert!(matches!(body[0], QBodyLit::Builtin(QBuiltin::Is { .. })));
    assert!(matches!(
        body[1],
        QBodyLit::Builtin(QBuiltin::Compare { .. })
    ));
}

// ── Answer-set canonicalization ───────────────────────────────────────────

// ── Counterfactual directive parsing ───────────────────────────────

#[test]
fn plain_program_has_no_counterfactual() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             ?- ex:p(ex:a, Y).\n",
    )
    .unwrap();
    assert!(
        prog.counterfactual.is_none(),
        "a plain v4 goal must not be a counterfactual"
    );
}

#[test]
fn parse_counterfactual_with_assume_and_depth() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/wc/').\n\
             :- counterfactual('http://world/cf', 'http://world/base').\n\
             :- assume(ex:mitigation(ex:x, ex:failed)).\n\
             :- assume(ex:control(ex:y, ex:absent)).\n\
             :- depth_budget(3).\n\
             ?- ex:harm(ex:y, Z).\n",
    )
    .unwrap();
    let cf = prog.counterfactual.expect("counterfactual must be parsed");
    assert_eq!(cf.cf_world, "<http://world/cf>");
    assert_eq!(cf.base_world, "<http://world/base>");
    assert_eq!(cf.antecedent.len(), 2, "two assume(...) atoms");
    assert_eq!(cf.antecedent[0].pred, "https://example.org/wc/mitigation");
    assert_eq!(
        cf.antecedent[0].args[1],
        QTerm::Const("<https://example.org/wc/failed>".to_owned())
    );
    assert_eq!(cf.depth_budget, Some(3));
    // The goal φ is an ordinary atom resolved inside W_cf.
    assert_eq!(prog.goal.atoms[0].pred, "https://example.org/wc/harm");
}

#[test]
fn counterfactual_defaults_depth_budget_to_none() {
    let prog = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap();
    let cf = prog.counterfactual.unwrap();
    assert_eq!(cf.depth_budget, None);
    assert_eq!(cf.cf_world, "<https://example.org/cf>");
}

#[test]
fn reject_variable_in_antecedent() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, O)).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(err.message().contains("ground"), "unexpected error: {err}");
}

#[test]
fn reject_assume_without_counterfactual() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("without a counterfactual"),
        "unexpected error: {err}"
    );
}

#[test]
fn reject_depth_budget_without_counterfactual() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- depth_budget(2).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("without a counterfactual"),
        "unexpected error: {err}"
    );
}

#[test]
fn reject_duplicate_counterfactual_directive() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- counterfactual(ex:cf2, ex:base).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("more than one"),
        "unexpected error: {err}"
    );
}

#[test]
fn reject_counterfactual_same_world() {
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:w, ex:w).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("must differ"),
        "unexpected error: {err}"
    );
}

#[test]
fn reject_unrecognized_directive() {
    // A typo'd directive (here `depth_buget`) must fail loudly rather than be
    // silently dropped — otherwise the intended guardrail vanishes unnoticed.
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             :- assume(ex:a(ex:s, ex:o)).\n\
             :- depth_buget(2).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("unrecognized directive"),
        "unexpected error: {err}"
    );
}

#[test]
fn reject_counterfactual_with_empty_antecedent() {
    // A counterfactual with no assume(...) admits nothing hypothetical: a
    // no-op revision, rejected as malformed.
    let err = parse_query_program(
        ":- prefix(ex, 'https://example.org/').\n\
             :- counterfactual(ex:cf, ex:base).\n\
             ?- ex:p(ex:s, Y).\n",
    )
    .unwrap_err();
    assert!(
        err.message().contains("at least one assume"),
        "unexpected error: {err}"
    );
}

#[test]
fn answer_set_canonicalize_sorts_bindings() {
    let mut b1 = BTreeMap::new();
    b1.insert("Y".to_owned(), "<https://example.org/c>".to_owned());
    let mut b2 = BTreeMap::new();
    b2.insert("Y".to_owned(), "<https://example.org/a>".to_owned());
    let mut b3 = BTreeMap::new();
    b3.insert("Y".to_owned(), "<https://example.org/b>".to_owned());

    let mut ans = AnswerSet {
        bindings: vec![b1.clone(), b3.clone(), b2.clone()],
        status: BudgetStatus::Ok,
        preservation: crate::result::PreservationClaim::exact(),
        frontier: CompletionFrontier::empty(),
    };
    ans.canonicalize();
    assert_eq!(ans.bindings[0]["Y"], "<https://example.org/a>");
    assert_eq!(ans.bindings[1]["Y"], "<https://example.org/b>");
    assert_eq!(ans.bindings[2]["Y"], "<https://example.org/c>");
}
