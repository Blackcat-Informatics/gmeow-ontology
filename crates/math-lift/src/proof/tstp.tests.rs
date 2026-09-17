// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const FIXTURE: &str = include_str!("../../fixtures/theorem-subclass.tstp");
const EPROVER_FOF: &str = include_str!("../../fixtures/eprover-fof.tstp");
const VAMPIRE_CNF: &str = include_str!("../../fixtures/vampire-cnf-refutation.tstp");
const EPROVER_CLAUSIFY: &str = include_str!("../../fixtures/eprover-clausify-status.tstp");

fn one(src: &str) -> Derivation {
    parse(src.as_bytes()).unwrap_or_else(|e| panic!("must parse: {e}"))
}

fn err(src: &str) -> String {
    format!(
        "{}",
        parse(src.as_bytes()).expect_err("this derivation must not parse")
    )
}

/// A minimal well-founded derivation: one asserted leaf, one inference.
const MINIMAL: &str = "cnf(a0, axiom, p(x)).\n\
                           cnf(d1, plain, q(x), inference(r, [status(thm)], [a0])).\n";

// -- the committed fixture -------------------------------------------------

#[test]
fn the_committed_reasoner_fixture_parses_into_its_three_steps() {
    let derivation = one(FIXTURE);
    assert_eq!(derivation.steps().len(), 3);
    let names: Vec<&str> = derivation.steps().iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "d_b17bff2379a308d9d6b50b0f9a073ef5e051b853",
            "d_0e9d1c683cc59b3da5626f16bd835a646aefd1ac",
            "d_2f2cb9320e4a9d0302cb5797a1e21f10d26a3e26",
        ]
    );
    assert_eq!(derivation.steps()[0].role, Role::Axiom);
    assert!(!derivation.steps()[0].is_derived());
    assert_eq!(derivation.steps()[1].role, Role::Plain);
    assert_eq!(
        derivation.steps()[1].parents,
        vec!["d_b17bff2379a308d9d6b50b0f9a073ef5e051b853".to_owned()]
    );
    assert_eq!(
        derivation.steps()[1].status(),
        &["status(thm)".to_owned()][..]
    );
}

#[test]
fn a_quoted_atom_carries_its_full_iri_unshortened() {
    let derivation = one(FIXTURE);
    let step = &derivation.steps()[0];
    let Conclusion::Clause(clause) = &step.conclusion else {
        panic!("the reasoner fixture is CNF");
    };
    let Term::Apply { functor, args } = &clause.literals[0].atom else {
        panic!("the leaf concludes an application");
    };
    assert_eq!(functor, "https://blackcatinformatics.ca/gmeow/tptp#a");
    assert_eq!(
        args,
        &[Term::Apply {
            functor: "https://blackcatinformatics.ca/logic/entail/reserved#witness-\
                          d4a1e02579180296"
                .to_owned(),
            args: Vec::new(),
        }]
    );
    assert!(!clause.literals[0].negated);
}

#[test]
fn the_inference_rule_is_the_content_addressed_firing_iri() {
    let derivation = one(FIXTURE);
    let rule = derivation.steps()[2].rule().expect("a rule");
    assert_eq!(
        rule,
        "https://blackcatinformatics.ca/gmeow/goal-directed/rule/\
             772035b62784cf12dce87129ba59bf14365477017a1c8c5fed58ada7d4ad35be"
    );
}

#[test]
fn the_terminal_step_is_the_derivations_conclusion() {
    let derivation = one(FIXTURE);
    assert_eq!(
        derivation.conclusion().name,
        "d_2f2cb9320e4a9d0302cb5797a1e21f10d26a3e26"
    );
    assert!(derivation.conclusion().is_derived());
}

#[test]
fn a_step_is_reachable_by_name() {
    let derivation = one(MINIMAL);
    assert_eq!(derivation.step("a0").expect("a0").role, Role::Axiom);
    assert!(derivation.step("nope").is_none());
}

// -- the term / clause grammar --------------------------------------------

fn clause_of<'d>(derivation: &'d Derivation, name: &str) -> &'d Clause {
    let Conclusion::Clause(clause) = &derivation.step(name).expect("the step").conclusion else {
        panic!("`{name}` concludes a clause");
    };
    clause
}

fn formula_of<'d>(derivation: &'d Derivation, name: &str) -> &'d Formula {
    let Conclusion::Formula(formula) = &derivation.step(name).expect("the step").conclusion else {
        panic!("`{name}` concludes a formula");
    };
    formula
}

#[test]
fn a_nested_term_structure_survives_to_the_ast() {
    let derivation = one("cnf(a0, axiom, p(f(g(a), X), b)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
    let clause = clause_of(&derivation, "a0");
    assert_eq!(clause.literals.len(), 1);
    assert_eq!(clause.render(), "p(f(g(a), X), b)");
    let Term::Apply { args, .. } = &clause.literals[0].atom else {
        panic!("an application");
    };
    let Term::Apply { functor, args: f } = &args[0] else {
        panic!("a nested application");
    };
    assert_eq!(functor, "f");
    assert_eq!(f[1], Term::Variable("X".to_owned()));
}

#[test]
fn a_disjunctive_clause_keeps_every_literal_and_its_polarity() {
    let derivation = one("cnf(a0, axiom, ( ~p(X) | q(X) | ~r )).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
    let clause = clause_of(&derivation, "a0");
    assert_eq!(clause.literals.len(), 3);
    assert!(clause.literals[0].negated);
    assert!(!clause.literals[1].negated);
    assert!(clause.literals[2].negated);
    assert_eq!(clause.render(), "~p(X) | q(X) | ~r");
}

#[test]
fn a_cnf_equality_literal_is_an_equation_not_a_predicate_named_equals() {
    let derivation = one("cnf(a0, axiom, ( f(X) = X | a != b )).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
    let clause = clause_of(&derivation, "a0");
    assert_eq!(clause.literals.len(), 2);
    assert!(!clause.literals[0].negated);
    assert_eq!(
        clause.literals[0].equated.as_ref().map(Term::render),
        Some("X".to_owned())
    );
    assert!(clause.literals[1].negated, "`!=` is a negative equation");
    assert_eq!(clause.render(), "f(X) = X | a != b");
    assert_eq!(one(&derivation.render()), derivation);
}

#[test]
fn a_tilde_negated_equation_canonicalizes_to_the_infix_disequality() {
    let derivation = one("cnf(a0, axiom, ~ f(a) = b).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
    assert_eq!(clause_of(&derivation, "a0").render(), "f(a) != b");
    assert_eq!(one(&derivation.render()), derivation);
}

#[test]
fn the_empty_clause_rides_as_the_defined_atom_false() {
    let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n");
    assert_eq!(derivation.conclusion().conclusion.render(), "$false");
}

#[test]
fn comments_and_the_shipped_header_are_skipped() {
    let derivation = one("% a line comment\n\
             /* a block\n comment */\n\
             cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])). % trailing\n");
    assert_eq!(derivation.steps().len(), 2);
}

#[test]
fn a_multi_parent_inference_keeps_every_parent_in_source_order() {
    let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(a1, axiom, q(a)).\n\
             cnf(d2, plain, r(a), inference(res, [status(thm), foo], [a0, a1])).\n");
    assert_eq!(
        derivation.conclusion().parents,
        vec!["a0".to_owned(), "a1".to_owned()]
    );
    assert_eq!(
        derivation.conclusion().status(),
        &["status(thm)".to_owned(), "foo".to_owned()][..]
    );
}

#[test]
fn an_inference_with_no_parent_is_a_derived_step_all_the_same() {
    // A prover may derive a tautology from nothing; it is still not an asserted leaf.
    let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(taut, [status(thm)], [])).\n\
             cnf(d2, plain, r(a), inference(res, [status(thm)], [a0, d1])).\n");
    assert!(derivation.step("d1").expect("d1").is_derived());
    assert!(derivation.step("d1").expect("d1").parents.is_empty());
}

#[test]
fn dependency_order_places_every_parent_before_the_step_that_cites_it() {
    // Source order is deliberately BACKWARDS here: the conclusion is written first.
    let derivation = one(
        "cnf(d2, plain, r(a), inference(res, [status(thm)], [d1, a1])).\n\
             cnf(d1, plain, q(a), inference(res, [status(thm)], [a0])).\n\
             cnf(a0, axiom, p(a)).\n\
             cnf(a1, axiom, s(a)).\n",
    );
    let order = derivation.dependency_order();
    assert_eq!(order.len(), 4, "every step is placed exactly once");
    let position: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(slot, &i)| (derivation.steps()[i].name.as_str(), slot))
        .collect();
    for step in derivation.steps() {
        for parent in &step.parents {
            assert!(
                position[parent.as_str()] < position[step.name.as_str()],
                "`{parent}` must be placed before `{}`",
                step.name
            );
        }
    }
}

// -- the FOF grammar -------------------------------------------------------

/// Wrap a `fof` body in a minimal well-founded derivation.
fn fof(body: &str) -> Derivation {
    one(&format!(
        "fof(a0, axiom, {body}).\n\
             cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
    ))
}

#[test]
fn a_quantified_implication_parses_into_a_binder_over_a_connective() {
    let derivation = fof("! [X] : (p(X) => q(X))");
    let Formula::Quantified {
        quantifier,
        variables,
        body,
    } = formula_of(&derivation, "a0")
    else {
        panic!("a quantified formula");
    };
    assert_eq!(*quantifier, Quantifier::ForAll);
    assert_eq!(variables, &["X".to_owned()]);
    let Formula::Binary { connective, .. } = body.as_ref() else {
        panic!("an implication body");
    };
    assert_eq!(*connective, Connective::Imply);
}

#[test]
fn every_tptp_binary_connective_parses_and_renders_back() {
    for (surface, connective) in [
        ("(p & q)", Connective::And),
        ("(p | q)", Connective::Or),
        ("(p => q)", Connective::Imply),
        ("(p <= q)", Connective::RevImply),
        ("(p <=> q)", Connective::Iff),
        ("(p <~> q)", Connective::Xor),
        ("(p ~| q)", Connective::Nor),
        ("(p ~& q)", Connective::Nand),
    ] {
        let derivation = fof(surface);
        let formula = formula_of(&derivation, "a0");
        let Formula::Binary { connective: c, .. } = formula else {
            panic!("{surface} is a binary formula");
        };
        assert_eq!(*c, connective, "{surface}");
        assert_eq!(formula.render(), surface, "the surface round-trips");
    }
}

#[test]
fn a_quantifier_list_binds_every_variable_in_source_order() {
    let derivation = fof("? [X, Y, Z] : p(X, Y, Z)");
    let Formula::Quantified {
        quantifier,
        variables,
        ..
    } = formula_of(&derivation, "a0")
    else {
        panic!("a quantified formula");
    };
    assert_eq!(*quantifier, Quantifier::Exists);
    assert_eq!(variables, &["X".to_owned(), "Y".to_owned(), "Z".to_owned()]);
}

#[test]
fn equality_and_disequality_are_structured_not_read_as_predicates() {
    let derivation = fof("! [X] : (f(X) = g(X))");
    let Formula::Quantified { body, .. } = formula_of(&derivation, "a0") else {
        panic!("quantified");
    };
    let Formula::Equation {
        negated,
        left,
        right,
    } = body.as_ref()
    else {
        panic!("an equation");
    };
    assert!(!negated);
    assert_eq!(left.render(), "f(X)");
    assert_eq!(right.render(), "g(X)");

    let derivation = fof("a != b");
    let Formula::Equation { negated, .. } = formula_of(&derivation, "a0") else {
        panic!("a disequation");
    };
    assert!(negated, "`!=` is a disequation, not a predicate named `!=`");
    assert_eq!(formula_of(&derivation, "a0").render(), "a != b");
}

#[test]
fn a_negated_quantified_formula_nests_rather_than_flattening() {
    let derivation = fof("~! [X] : p(X)");
    let Formula::Not(inner) = formula_of(&derivation, "a0") else {
        panic!("a negation");
    };
    assert!(matches!(inner.as_ref(), Formula::Quantified { .. }));
}

#[test]
fn an_associative_chain_is_left_nested_and_re_renders_identically() {
    let derivation = fof("((p & q) & r)");
    let formula = formula_of(&derivation, "a0");
    let Formula::Binary { left, .. } = formula else {
        panic!("a conjunction");
    };
    assert!(matches!(left.as_ref(), Formula::Binary { .. }));
    assert_eq!(formula.render(), "((p & q) & r)");
}

#[test]
fn mixing_the_associative_connectives_without_parentheses_is_a_syntax_error() {
    let text = err("fof(a0, axiom, p & q | r).\n");
    assert!(text.contains("without parentheses"), "{text}");
    assert!(text.contains("line "), "{text}");
}

#[test]
fn a_chained_non_associative_connective_is_a_syntax_error() {
    let text = err("fof(a0, axiom, p => q => r).\n");
    assert!(text.contains("exactly two unitary operands"), "{text}");
}

#[test]
fn a_fof_step_renders_under_the_fof_keyword_and_a_cnf_step_under_cnf() {
    let derivation = fof("! [X] : p(X)");
    assert!(
        derivation
            .step("a0")
            .expect("a0")
            .render()
            .starts_with("fof(")
    );
    assert!(
        derivation
            .step("d1")
            .expect("d1")
            .render()
            .starts_with("cnf(")
    );
}

// -- the full role set -----------------------------------------------------

#[test]
fn every_tptp_formula_role_parses_as_itself() {
    for word in [
        "axiom",
        "hypothesis",
        "definition",
        "assumption",
        "lemma",
        "theorem",
        "corollary",
        "conjecture",
        "negated_conjecture",
        "plain",
        "type",
        "fi_domain",
        "fi_functors",
        "fi_predicates",
        "unknown",
    ] {
        let derivation = one(&format!(
            "cnf(a0, {word}, p(a)).\n\
                 cnf(d1, plain, $false, inference(r, [status(thm)], [a0])).\n"
        ));
        let role = derivation.step("a0").expect("a0").role;
        assert_eq!(role.as_str(), word, "the raw role word survives the parse");
        assert_eq!(Role::from_word(word), Some(role));
    }
}

#[test]
fn only_the_three_foundation_roles_are_foundational() {
    for word in ["axiom", "hypothesis", "assumption"] {
        assert!(
            Role::from_word(word).expect("a role").is_foundational(),
            "{word}"
        );
    }
    for word in [
        "negated_conjecture",
        "conjecture",
        "plain",
        "lemma",
        "theorem",
        "definition",
        "unknown",
    ] {
        assert!(
            !Role::from_word(word).expect("a role").is_foundational(),
            "`{word}` must never be lifted as a law"
        );
    }
}

#[test]
fn a_role_no_longer_dictates_whether_a_step_is_derived() {
    // A real prover writes `cnf(c, negated_conjecture, …, inference(…))` and
    // `cnf(c, plain, …, file(…))`; coupling the role to the source would refuse both.
    let derivation = one(
        "cnf(a0, negated_conjecture, ~p(a), file('problem.p', goal)).\n\
             cnf(d1, negated_conjecture, q(a), inference(r, [status(thm)], [a0])).\n",
    );
    assert!(!derivation.step("a0").expect("a0").is_derived());
    assert!(derivation.step("d1").expect("d1").is_derived());
}

// -- the source forms ------------------------------------------------------

#[test]
fn a_file_source_is_an_external_reference_carried_verbatim() {
    let derivation = one("cnf(a0, axiom, p(a), file('SET001-1.p', ax7)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
    let Source::External(external) = &derivation.step("a0").expect("a0").source else {
        panic!("a file(…) source is external");
    };
    assert_eq!(external.functor, "file");
    assert_eq!(external.rendered, "file('SET001-1.p', ax7)");
    assert!(
        derivation.step("a0").expect("a0").parents.is_empty(),
        "an external reference cites no parent inside this document"
    );
}

#[test]
fn theory_introduced_creator_and_unknown_are_all_external_references() {
    for (surface, functor) in [
        ("theory(equality)", "theory"),
        (
            "introduced(definition, [new_symbols(definition, [esk1_0])])",
            "introduced",
        ),
        ("creator(eprover, [version('3.0')])", "creator"),
        ("unknown", "unknown"),
    ] {
        let derivation = one(&format!(
            "cnf(a0, axiom, p(a), {surface}).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n"
        ));
        let Source::External(external) = &derivation.step("a0").expect("a0").source else {
            panic!("`{surface}` must be an external reference");
        };
        assert_eq!(external.functor, functor, "{surface}");
        assert_eq!(external.rendered, surface, "{surface} rides verbatim");
    }
}

#[test]
fn a_bare_name_source_is_a_derived_step_citing_that_parent_with_no_rule() {
    let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, p(a), a0).\n");
    let step = derivation.step("d1").expect("d1");
    assert_eq!(step.source, Source::Parent);
    assert!(step.is_derived(), "a DAG source is a derivation edge");
    assert_eq!(step.parents, vec!["a0".to_owned()]);
    assert_eq!(
        step.rule(),
        None,
        "a bare DAG source names a parent, not a calculus rule"
    );
}

#[test]
fn a_useful_info_field_is_read_rather_than_refused() {
    let derivation = one("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0]), [iquote('0:Res:1,2')]).\n");
    assert_eq!(
        derivation.step("d1").expect("d1").useful_info,
        vec!["iquote('0:Res:1,2')".to_owned()]
    );
}

#[test]
fn a_non_thm_status_is_carried_rather_than_refused() {
    for token in ["cth", "esa", "sab", "ceq"] {
        let derivation = one(&format!(
            "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status({token})], [a0])).\n"
        ));
        let step = derivation.step("d1").expect("d1");
        assert_eq!(step.status(), &[format!("status({token})")][..]);
        assert!(!step.declares_thm_status());
    }
}

#[test]
fn an_empty_status_list_is_a_stated_absence_not_a_refusal() {
    let derivation = one("fof(f1, axiom, p(a)).\n\
             cnf(f2, plain, p(a), inference(cnf_transformation, [], [f1])).\n");
    assert!(derivation.step("f2").expect("f2").status().is_empty());
    assert!(!derivation.step("f2").expect("f2").declares_thm_status());
}

// -- rendering round-trips -------------------------------------------------

#[test]
fn a_rendered_derivation_re_parses_to_the_same_ast() {
    for source in [FIXTURE, MINIMAL, EPROVER_FOF, VAMPIRE_CNF, EPROVER_CLAUSIFY] {
        let first = one(source);
        let second = one(&first.render());
        assert_eq!(first, second, "rendering must be a faithful TSTP surface");
    }
}

#[test]
fn every_source_form_round_trips_through_the_rendered_surface() {
    let source = "fof(a0, axiom, ! [X] : (p(X) => q(X)), file('problem.p', ax1)).\n\
                      cnf(a1, negated_conjecture, ~q(sk1), theory(equality)).\n\
                      cnf(a2, axiom, p(sk1), introduced(definition)).\n\
                      cnf(a3, plain, p(sk1), a2).\n\
                      cnf(d1, plain, $false, \
                          inference(sr, [status(thm)], [a0, a1, a3]), [iquote('x')]).\n";
    let first = one(source);
    assert_eq!(one(&first.render()), first);
}

#[test]
fn an_atom_is_quoted_exactly_when_it_is_not_a_bare_word() {
    assert_eq!(render_atom("plain_word9"), "plain_word9");
    assert_eq!(render_atom("$false"), "$false");
    assert_eq!(render_atom("42"), "42");
    assert_eq!(render_atom("https://e.org/a#b"), "'https://e.org/a#b'");
    assert_eq!(render_atom("Upper"), "'Upper'");
    assert_eq!(render_atom("it's"), r"'it\'s'");
    assert_eq!(render_atom(r"back\slash"), r"'back\\slash'");
}

#[test]
fn a_quoted_atom_with_escapes_round_trips_through_the_lexer() {
    let derivation = one("cnf(a0, axiom, 'it\\'s'(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
    let Term::Apply { functor, .. } = &clause_of(&derivation, "a0").literals[0].atom else {
        panic!("an application");
    };
    assert_eq!(functor, "it's");
    assert_eq!(one(&derivation.render()), derivation);
}

// -- the prover fixtures ---------------------------------------------------

#[test]
fn the_eprover_fof_fixture_lifts_its_quantifiers_roles_and_file_sources() {
    let derivation = one(EPROVER_FOF);
    let roles: BTreeSet<&str> = derivation.steps().iter().map(|s| s.role.as_str()).collect();
    assert!(roles.contains("axiom"), "{roles:?}");
    assert!(roles.contains("negated_conjecture"), "{roles:?}");
    assert!(roles.contains("plain"), "{roles:?}");
    assert!(
        derivation
            .steps()
            .iter()
            .any(|s| matches!(&s.source, Source::External(e) if e.functor == "file")),
        "the fixture carries file(…) sources"
    );
    assert!(
        derivation.steps().iter().any(|s| matches!(
            &s.conclusion,
            Conclusion::Formula(Formula::Quantified { .. })
        )),
        "the fixture carries a quantified fof conclusion"
    );
    assert_eq!(derivation.conclusion().conclusion.render(), "$false");
}

#[test]
fn the_vampire_fixture_declares_empty_status_lists_and_still_parses() {
    let derivation = one(VAMPIRE_CNF);
    assert!(
        derivation
            .steps()
            .iter()
            .filter(|s| s.is_derived())
            .any(|s| s.status().is_empty()),
        "Vampire writes `inference(rule,[],[parent])`"
    );
    assert_eq!(derivation.conclusion().conclusion.render(), "$false");
}

#[test]
fn the_eprover_clausification_fixture_declares_cth_and_esa_statuses() {
    let derivation = one(EPROVER_CLAUSIFY);
    let statuses: BTreeSet<&str> = derivation
        .steps()
        .iter()
        .flat_map(|s| s.status().iter().map(String::as_str))
        .collect();
    assert!(statuses.contains("status(cth)"), "{statuses:?}");
    assert!(statuses.contains("status(esa)"), "{statuses:?}");
    assert!(statuses.contains("status(thm)"), "{statuses:?}");
}

// -- syntax hard failures --------------------------------------------------

#[test]
fn every_syntax_failure_carries_a_line_and_a_column() {
    for (source, needle) in [
        ("cnf(a0, axiom, p(a))\n", "unexpected end"),
        ("cnf(a0, axiom, p(a) .\n", "expected `)`"),
        ("cnf(a0 axiom, p(a)).\n", "expected `,`"),
        ("cnf(a0, axiom, 'unterminated).\n", "unterminated"),
        ("/* never closed\ncnf(a0, axiom, p(a)).\n", "block comment"),
        ("cnf(a0, axiom, p(a)).\n@\n", "annotated formula, found `@`"),
        ("cnf(a0, axiom, p(_x)).\n", "starting with `_`"),
        ("cnf(a0, bogus_role, p(a)).\n", "not a TPTP formula role"),
        ("fmt(a0, axiom, p(a)).\n", "expected `cnf` or `fof`"),
        ("cnf(a0, axiom, ''(a)).\n", "empty single-quoted atom"),
        ("fof(a0, axiom, ! [x] : p(x)).\n", "quantified variable"),
        ("fof(a0, axiom, ! [X] p(X)).\n", "expected `:`"),
    ] {
        let text = err(source);
        assert!(text.contains("line "), "{source:?} → {text}");
        assert!(text.contains("column "), "{source:?} → {text}");
        assert!(text.contains(needle), "{source:?} → {text}");
    }
}

#[test]
fn the_reported_position_is_the_offending_token_not_the_document_start() {
    let text = err("cnf(a0, axiom, p(a)).\ncnf(a1, axiom, p(&)).\n");
    assert!(text.contains("line 2, column 18"), "{text}");
}

#[test]
fn a_duplicate_formula_name_is_a_parse_failure() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(a0, axiom, q(a)).\n\
             cnf(d1, plain, r(a), inference(x, [status(thm)], [a0])).\n");
    assert!(text.contains("twice"), "{text}");
    assert!(text.contains("`a0`"), "{text}");
}

#[test]
fn a_malformed_inference_arity_is_a_parse_failure() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)])).\n");
    assert!(
        text.contains("exactly (rule, status-list, parent-list)"),
        "{text}"
    );
}

#[test]
fn a_useful_info_field_that_is_not_a_list_is_a_parse_failure() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0]), iquote('x')).\n");
    assert!(text.contains("bracketed general list"), "{text}");
}

// -- unliftable hard failures ---------------------------------------------

#[test]
fn a_dangling_parent_is_unliftable_because_there_is_no_well_founded_proof() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [ghost])).\n");
    assert!(text.contains("`ghost`"), "{text}");
    assert!(text.contains("never introduces"), "{text}");
    assert!(text.contains("well-founded"), "{text}");
}

#[test]
fn a_dangling_bare_name_source_is_unliftable_too() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), ghost).\n");
    assert!(text.contains("`ghost`"), "{text}");
    assert!(text.contains("never introduces"), "{text}");
}

#[test]
fn a_cycle_is_unliftable_and_the_diagnostic_names_it() {
    let text = err("cnf(d1, plain, p(a), inference(r, [status(thm)], [d2])).\n\
             cnf(d2, plain, q(a), inference(r, [status(thm)], [d1])).\n");
    assert!(text.contains("cycle"), "{text}");
    assert!(text.contains("d1"), "{text}");
    assert!(text.contains("d2"), "{text}");
}

#[test]
fn a_cycle_through_bare_name_sources_is_caught_the_same_way() {
    let text = err("cnf(d1, plain, p(a), d2).\ncnf(d2, plain, p(a), d1).\n");
    assert!(text.contains("cycle"), "{text}");
}

#[test]
fn a_step_that_is_its_own_parent_is_a_cycle() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [d1])).\n");
    assert!(text.contains("cycle"), "{text}");
}

#[test]
fn a_document_with_no_derived_step_is_unliftable() {
    let text = err("cnf(a0, axiom, p(a)).\n");
    assert!(text.contains("no derived step"), "{text}");
}

#[test]
fn neutral_document_parser_accepts_a_problem_without_claiming_a_proof() {
    let document = parse_document(
        b"fof(a0, axiom, ! [X] : (p(X) => q(X))).\n\
          fof(a1, axiom, p(a)).\n",
    )
    .expect("neutral FOF document");
    assert_eq!(document.steps().len(), 2);
    assert_eq!(document.steps()[0].name, "a0");
    assert!(!document.steps()[0].is_derived());
}

#[test]
fn a_document_of_external_leaves_only_is_still_unliftable() {
    let text = err("cnf(a0, axiom, p(a), file('problem.p', ax)).\n");
    assert!(text.contains("no derived step"), "{text}");
}

#[test]
fn a_document_with_several_terminal_steps_is_several_proofs() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n\
             cnf(d2, plain, s(a), inference(r, [status(thm)], [a0])).\n");
    assert!(text.contains("2 terminal steps"), "{text}");
    assert!(text.contains("d1"), "{text}");
}

#[test]
fn an_uncited_asserted_leaf_is_a_second_terminal() {
    let text = err("cnf(a0, axiom, p(a)).\n\
             cnf(spare, axiom, z(a)).\n\
             cnf(d1, plain, q(a), inference(r, [status(thm)], [a0])).\n");
    assert!(text.contains("terminal steps"), "{text}");
    assert!(text.contains("spare"), "{text}");
}

#[test]
fn every_out_of_fragment_construct_is_refused_by_name() {
    for (source, needle) in [
        ("tff(a0, type, a: $i).\n", "TYPED TPTP dialect `tff`"),
        ("thf(a0, axiom, p).\n", "TYPED TPTP dialect `thf`"),
        ("tcf(a0, axiom, p).\n", "TYPED TPTP dialect `tcf`"),
        ("include('Axioms/SET001-0.ax').\n", "`include` directive"),
        (
            "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), mystery(x)).\n",
            "is not a TPTP <source> form",
        ),
        (
            "cnf(a0, axiom, p(a)).\ncnf(d1, plain, q(a), [file('p', a), theory(equality)]).\n",
            "<sources> LIST",
        ),
        (
            "cnf(a0, axiom, p(a)).\n\
                 cnf(d1, plain, q(a), inference(r, [status(thm)], [inference(s, [], [a0])])).\n",
            "nested parent",
        ),
    ] {
        let text = err(source);
        assert!(text.contains(needle), "{source:?} → {text}");
    }
}

#[test]
fn eprovers_hash_framed_szs_envelope_parses() {
    // `eprover --proof-object` frames its derivation in `#` lines. They are not TPTP
    // grammar, so the scanner refused them — and an unedited E proof could not be read
    // at all. The committed eprover fixtures had been written WITHOUT the framing, so
    // they passed a parser that could not read the tool they are named for.
    let derivation = parse(
        b"# SZS status Theorem\n\
              # SZS output start CNFRefutation\n\
              cnf(a1, axiom, (p(a))).\n\
              cnf(d1, plain, (q(a)), inference(spm,[status(thm)],[a1,theory(equality)])).\n\
              # SZS output end CNFRefutation\n\
              # Proof object total steps    : 2\n",
    )
    .expect("an unedited E proof object must parse");
    assert_eq!(derivation.steps().len(), 2);
    assert_eq!(
        derivation.step("d1").expect("the derived step").parents,
        vec!["a1".to_owned()]
    );
}

#[test]
fn an_external_source_in_a_parent_list_lifts_as_a_warrant() {
    // E cites theory(equality) in the parent list of EVERY equality-using inference
    // (rw, spm, sr, cn), so refusing it refused E's canonical output. It is a
    // grammatical <parent_info> and carries no sub-proof, so there is nothing to
    // flatten — it is a warrant, not a step.
    let derivation = parse(
        b"cnf(a1, axiom, (f(X) = g(X))).\n\
             cnf(a2, axiom, (p(f(a)))).\n\
             cnf(d1, plain, (p(g(a))), inference(rw,[status(thm)],[a2,a1,theory(equality)])).\n",
    )
    .expect("an E-shaped equality inference must lift");
    let step = derivation.step("d1").expect("the derived step");
    assert_eq!(
        step.parents,
        vec!["a2".to_owned(), "a1".to_owned()],
        "an external citation is NOT a step and must never enter the parent list the \
             well-foundedness walk resolves"
    );
    assert_eq!(step.external_parents.len(), 1);
    assert_eq!(step.external_parents[0].functor, "theory");
    assert_eq!(step.external_parents[0].rendered, "theory(equality)");
}

#[test]
fn a_file_reference_in_a_parent_list_lifts_too() {
    // The other <external_source> form a prover writes in the parent position.
    let derivation = parse(
        b"cnf(a1, axiom, (p(a))).\n\
             cnf(d1, plain, (q(a)), \
             inference(res,[status(thm)],[a1,file('SET001-1.p',ax7)])).\n",
    )
    .expect("a file-referenced premise must lift");
    let step = derivation.step("d1").expect("the derived step");
    assert_eq!(step.parents, vec!["a1".to_owned()]);
    assert_eq!(step.external_parents[0].rendered, "file('SET001-1.p', ax7)");
}

#[test]
fn a_non_utf8_source_is_refused_before_lexing() {
    let text = format!(
        "{}",
        parse(&[b'c', b'n', b'f', 0xff, 0xfe]).expect_err("invalid UTF-8 must not parse")
    );
    assert!(text.contains("not valid UTF-8"), "{text}");
}

#[test]
fn a_deep_chain_does_not_overflow_the_cycle_check() {
    // The dependency walk is explicit-stack, not recursive: a long derivation is
    // untrusted input and must not be able to blow the parser's own stack.
    let mut source = String::from("cnf(a0, axiom, p(a)).\n");
    for i in 1..5_000 {
        source.push_str(&format!(
            "cnf(d{i}, plain, q{i}(a), inference(r, [status(thm)], [{}])).\n",
            if i == 1 {
                "a0".to_owned()
            } else {
                format!("d{}", i - 1)
            }
        ));
    }
    let derivation = one(&source);
    assert_eq!(derivation.steps().len(), 5_000);
    assert_eq!(derivation.conclusion().name, "d4999");
}
