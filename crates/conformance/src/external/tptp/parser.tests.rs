// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn one(src: &str) -> AnnotatedFormula {
    let mut v = parse_tptp(src).expect("parse ok");
    assert_eq!(v.len(), 1, "expected exactly one formula");
    v.pop().unwrap()
}

#[test]
fn parses_a_universal_implication() {
    let af = one("fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n");
    assert_eq!(af.name, "a_sub_b");
    assert_eq!(af.role, TptpRole::Premise);
    match &af.formula {
        Formula::Forall { vars, body } => {
            assert_eq!(vars, &["X".to_string()]);
            assert!(matches!(**body, Formula::Implies(_, _)));
        }
        other => panic!("expected Forall, got {other:?}"),
    }
}

#[test]
fn quantifier_binds_only_a_unit_body_not_a_binary_formula() {
    // TPTP BNF: `<fof_quantified_formula> ::= <quantifier> [vars] : <fof_unit_formula>`.
    // The body is unitary/unary, so an un-parenthesized `=>` binds OUTSIDE the
    // quantifier: `![X] : a(X) => b(X)` == `(![X] : a(X)) => b(X)`. This pins the
    // BNF-correct precedence against a "fix" that would swallow the whole
    // implication as the body (which would bind `X` free in `b(X)`).
    let af = one("fof(prec, axiom, ![X] : a(X) => b(X)).\n");
    match &af.formula {
        Formula::Implies(l, r) => {
            assert!(
                matches!(**l, Formula::Forall { .. }),
                "lhs should be the quantified `a(X)`, got {l:?}"
            );
            assert!(
                matches!(**r, Formula::Atom { .. }),
                "rhs should be the bare `b(X)`, got {r:?}"
            );
        }
        other => panic!(
            "expected a top-level Implies (quantifier binds only its unit body), got {other:?}"
        ),
    }
}

#[test]
fn parses_disjointness_as_negated_conjunction() {
    let af = one("fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).\n");
    match &af.formula {
        Formula::Forall { body, .. } => match &**body {
            Formula::Not(inner) => assert!(matches!(**inner, Formula::And(_))),
            other => panic!("expected Not, got {other:?}"),
        },
        other => panic!("expected Forall, got {other:?}"),
    }
}

#[test]
fn parses_ground_atom() {
    let af = one("fof(x_is_a, axiom, a(x)).\n");
    match &af.formula {
        Formula::Atom { relation, args } => {
            assert_eq!(*relation, Term::Iri(format!("{TPTP_NS}a")));
            assert_eq!(args, &[Term::Iri(format!("{TPTP_NS}x"))]);
        }
        other => panic!("expected Atom, got {other:?}"),
    }
}

#[test]
fn parses_conjecture_role() {
    let af = one("fof(goal, conjecture, ![X] : (a(X) => b(X))).\n");
    assert_eq!(af.role, TptpRole::Conjecture);
}

#[test]
fn parses_negated_conjecture_role() {
    let af = one("fof(goal, negated_conjecture, a(x)).\n");
    assert_eq!(af.role, TptpRole::NegatedConjecture);
}

#[test]
fn parses_cnf_clause_with_implicit_universal() {
    // ~b(X) | ~c(X) is disjointness in CNF form.
    let af = one("cnf(b_disj_c, axiom, ( ~b(X) | ~c(X) )).\n");
    match &af.formula {
        Formula::Forall { vars, body } => {
            assert_eq!(vars, &["X".to_string()]);
            assert!(matches!(**body, Formula::Or(_)));
        }
        other => panic!("expected universally-closed Or, got {other:?}"),
    }
}

#[test]
fn parses_iff_and_reverse_implication() {
    let af = one("fof(e, axiom, a(x) <=> b(x)).\n");
    assert!(matches!(af.formula, Formula::Iff(_, _)));
    let af = one("fof(r, axiom, a(x) <= b(x)).\n");
    // `a <= b` normalizes to `b => a`.
    match af.formula {
        Formula::Implies(l, r) => {
            assert_eq!(
                *l,
                Formula::Atom {
                    relation: Term::Iri(format!("{TPTP_NS}b")),
                    args: vec![Term::Iri(format!("{TPTP_NS}x"))]
                }
            );
            assert_eq!(
                *r,
                Formula::Atom {
                    relation: Term::Iri(format!("{TPTP_NS}a")),
                    args: vec![Term::Iri(format!("{TPTP_NS}x"))]
                }
            );
        }
        other => panic!("expected Implies, got {other:?}"),
    }
}

#[test]
fn skips_comments_including_szs_status() {
    let src = "% a comment\n\
                   /* block\n comment */\n\
                   fof(x_is_a, axiom, a(x)).\n\
                   % SZS status Unsatisfiable for foo\n";
    let v = parse_tptp(src).unwrap();
    assert_eq!(v.len(), 1);
}

#[test]
fn function_symbol_in_argument_is_a_capability_gap() {
    let err = parse_tptp("fof(f, axiom, p(f(x))).\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
    assert!(err.to_string().contains("function symbol"), "{err}");
}

#[test]
fn equality_is_a_capability_gap() {
    let err = parse_tptp("fof(e, axiom, x = y).\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn defined_atom_is_a_capability_gap() {
    let err = parse_tptp("fof(t, axiom, $true).\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn tff_dialect_is_a_capability_gap() {
    let err = parse_tptp("tff(t, type, a : $i > $o).\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn include_directive_is_a_capability_gap() {
    let err = parse_tptp("include('Axioms/SET001-0.ax').\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn unknown_role_is_a_syntax_error() {
    let err = parse_tptp("fof(x, bogus_role, a(x)).\n").unwrap_err();
    assert!(matches!(err, TptpError::Syntax(_)), "{err}");
}

#[test]
fn missing_trailing_dot_is_a_syntax_error() {
    let err = parse_tptp("fof(x, axiom, a(x))\n").unwrap_err();
    assert!(matches!(err, TptpError::Syntax(_)), "{err}");
}

#[test]
fn leading_underscore_identifier_is_a_syntax_error() {
    // A word starting with `_` is neither a valid variable (`<upper_word>`) nor a
    // valid functor (`<lower_word>`), so it must be rejected — never silently
    // admitted as a constant (which would change the problem's meaning).
    let err = parse_tptp("fof(u, axiom, p(_x)).\n").unwrap_err();
    assert!(matches!(err, TptpError::Syntax(_)), "{err}");
}

#[test]
fn parses_nested_alternating_quantifiers() {
    // `![X] : ?[Y] : r(X, Y)` — the outer body is itself a quantified unit
    // formula, so the parse is Forall(Exists(atom)) with the inner binder distinct.
    let af = one("fof(nest, axiom, ![X] : ?[Y] : r(X, Y)).\n");
    match &af.formula {
        Formula::Forall { vars, body } => {
            assert_eq!(vars, &["X".to_string()]);
            match &**body {
                Formula::Exists { vars, body } => {
                    assert_eq!(vars, &["Y".to_string()]);
                    assert!(matches!(**body, Formula::Atom { .. }));
                }
                other => panic!("expected inner Exists, got {other:?}"),
            }
        }
        other => panic!("expected outer Forall, got {other:?}"),
    }
}

#[test]
fn parses_multiple_formulas_in_order() {
    let src = "fof(a1, axiom, a(x)).\nfof(a2, axiom, b(y)).\n";
    let v = parse_tptp(src).unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].name, "a1");
    assert_eq!(v[1].name, "a2");
}

// --- TSTP derivation grammar ---------------------------------------------

#[test]
fn a_plain_problem_formula_carries_no_annotation() {
    // Backwards shape guarantee: the 3-field form still parses and reports an
    // ABSENT source, so `lower_problem`'s premise/conjecture routing is unchanged.
    let af = one("fof(x_is_a, axiom, a(x)).\n");
    assert_eq!(af.role, TptpRole::Premise);
    assert_eq!(af.source, None);
    assert_eq!(af.useful_info, None);
}

#[test]
fn plain_role_is_a_derived_step_not_a_premise() {
    let af = one("cnf(d_1, plain, b(x), inference(r, [status(thm)], [d_0])).\n");
    assert_eq!(
        af.role,
        TptpRole::Derived,
        "a `plain` TSTP step must not masquerade as an asserted premise"
    );
}

#[test]
fn parses_an_inference_source_with_status_and_parents() {
    let af = one("cnf(c3, plain, b(x), inference(resolution, [status(thm)], [c1, c2])).\n");
    match af.source.expect("a source is present") {
        TptpSource::Inference {
            rule,
            status,
            parents,
        } => {
            assert_eq!(rule, "resolution");
            assert_eq!(
                status,
                vec![TstpTerm::Func(
                    "status".into(),
                    vec![TstpTerm::Name("thm".into())]
                )]
            );
            assert_eq!(parents, vec!["c1".to_string(), "c2".to_string()]);
        }
        other => panic!("expected an inference source, got {other:?}"),
    }
}

#[test]
fn parses_an_inference_with_no_parents_and_a_quoted_rule_iri() {
    let af = one(
        "cnf(c1, plain, 'https://example.org/p'('https://example.org/a'), \
             inference('https://example.org/rule/1', [status(thm)], [])).\n",
    );
    match af.source.expect("source") {
        TptpSource::Inference { rule, parents, .. } => {
            assert_eq!(rule, "https://example.org/rule/1");
            assert!(parents.is_empty());
        }
        other => panic!("expected an inference source, got {other:?}"),
    }
}

#[test]
fn parses_file_bare_name_and_theory_sources() {
    let af = one("cnf(c1, axiom, a(x), file('problem.p', x_is_a)).\n");
    assert_eq!(
        af.source,
        Some(TptpSource::File {
            path: "problem.p".into(),
            name: Some("x_is_a".into())
        })
    );

    let af = one("cnf(c1, axiom, a(x), file('problem.p')).\n");
    assert_eq!(
        af.source,
        Some(TptpSource::File {
            path: "problem.p".into(),
            name: None
        })
    );

    let af = one("cnf(c1, axiom, a(x), x_is_a).\n");
    assert_eq!(af.source, Some(TptpSource::Name("x_is_a".into())));

    let af = one("cnf(c1, axiom, a(x), theory(equality)).\n");
    assert_eq!(
        af.source,
        Some(TptpSource::Theory {
            name: "equality".into(),
            args: vec![]
        })
    );
}

#[test]
fn parses_the_useful_info_field() {
    let af = one("cnf(c1, plain, a(x), inference(r, [], [c0]), [iquote('foo')]).\n");
    assert_eq!(
        af.useful_info,
        Some(TstpTerm::List(vec![TstpTerm::Func(
            "iquote".into(),
            vec![TstpTerm::Name("foo".into())]
        )]))
    );
}

#[test]
fn a_nested_inference_parent_is_a_capability_gap() {
    // An inline parent derivation would need a second, anonymous step identity —
    // an honest gap, never a silently dropped provenance edge.
    let err = parse_tptp("cnf(c2, plain, a(x), inference(r, [], [inference(s, [], [c0])])).\n")
        .unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn an_unrecognized_source_functor_is_a_capability_gap() {
    let err = parse_tptp("cnf(c1, plain, a(x), introduced(definition)).\n").unwrap_err();
    assert!(matches!(err, TptpError::Unsupported(_)), "{err}");
}

#[test]
fn a_malformed_inference_arity_is_a_syntax_error() {
    let err = parse_tptp("cnf(c1, plain, a(x), inference(r, [])).\n").unwrap_err();
    assert!(matches!(err, TptpError::Syntax(_)), "{err}");
}
