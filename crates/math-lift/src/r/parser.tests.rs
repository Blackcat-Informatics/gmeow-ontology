// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn script(src: &str) -> RScript {
    parse(src).expect("parses")
}

fn only(src: &str) -> RExpr {
    let s = script(src);
    assert_eq!(s.statements.len(), 1, "expected one statement in `{src}`");
    s.statements[0].expression().clone()
}

fn formula(src: &str) -> Formula {
    match only(src) {
        RExpr::Formula(f) => *f,
        other => panic!("expected a formula, got {other:?}"),
    }
}

fn term_names(f: &Formula) -> Vec<String> {
    f.terms
        .iter()
        .map(|t| {
            t.factors
                .iter()
                .map(|e| match e.unparenthesized() {
                    RExpr::Ident(n) => n.clone(),
                    other => other.structure_key(),
                })
                .collect::<Vec<_>>()
                .join(":")
        })
        .collect()
}

#[test]
fn every_assignment_form_parses_to_one_statement() {
    for (src, kind) in [
        ("x <- 1", AssignKind::Left),
        ("x <<- 1", AssignKind::SuperLeft),
        ("x = 1", AssignKind::Equals),
        ("1 -> x", AssignKind::Right),
        ("1 ->> x", AssignKind::SuperRight),
    ] {
        let s = script(src);
        match &s.statements[0].kind {
            RStmtKind::Assign {
                target, kind: got, ..
            } => {
                assert_eq!(*got, kind, "{src}");
                assert_eq!(*target, RExpr::Ident("x".to_owned()), "{src}");
            }
            other => panic!("{src} did not parse as an assignment: {other:?}"),
        }
    }
}

#[test]
fn a_call_carries_positional_and_named_arguments_in_order() {
    let RExpr::Call { callee, args } = only("lm(mpg ~ wt, data = mtcars)") else {
        panic!("expected a call")
    };
    assert_eq!(*callee, RExpr::Ident("lm".to_owned()));
    assert_eq!(args.len(), 2);
    assert!(args[0].name.is_none());
    assert!(matches!(args[0].value, Some(RExpr::Formula(_))));
    assert_eq!(args[1].name.as_deref(), Some("data"));
    assert_eq!(args[1].value, Some(RExpr::Ident("mtcars".to_owned())));
}

#[test]
fn an_empty_subscript_argument_is_kept_not_dropped() {
    let RExpr::Index { args, double, .. } = only("m[, 1]") else {
        panic!("expected a subscript")
    };
    assert!(!double);
    assert_eq!(args.len(), 2);
    assert!(args[0].value.is_none(), "the empty subscript survives");
    assert!(args[1].value.is_some());
}

#[test]
fn double_bracket_and_component_accessors_parse() {
    assert!(matches!(only("x[[1]]"), RExpr::Index { double: true, .. }));
    let RExpr::Component { name, slot, .. } = only("fit$residuals") else {
        panic!("expected a `$` component")
    };
    assert_eq!(name, "residuals");
    assert!(!slot);
    assert!(matches!(
        only("obj@slot"),
        RExpr::Component { slot: true, .. }
    ));
}

#[test]
fn namespaced_names_parse() {
    assert_eq!(
        only("stats::lm"),
        RExpr::Namespace {
            package: "stats".to_owned(),
            name: "lm".to_owned(),
            internal: false
        }
    );
    assert!(matches!(
        only("broom:::tidy"),
        RExpr::Namespace { internal: true, .. }
    ));
}

#[test]
fn arithmetic_honours_r_precedence_and_associativity() {
    // `-2^2` is `-(2^2)`; `^` is right-associative; `*` binds tighter than `+`.
    assert_eq!(only("-2^2").structure_key(), only("-(2^2)").structure_key());
    assert_eq!(
        only("2^3^2").structure_key(),
        only("2^(3^2)").structure_key()
    );
    assert_eq!(
        only("a + b * c").structure_key(),
        only("a + (b * c)").structure_key()
    );
    assert_eq!(
        only("a:b + c").structure_key(),
        only("(a:b) + c").structure_key()
    );
    assert_eq!(
        only("!a == b").structure_key(),
        only("!(a == b)").structure_key()
    );
}

#[test]
fn a_two_sided_formula_indexes_its_terms_in_source_order() {
    let f = formula("mpg ~ wt + hp");
    assert_eq!(f.response, Some(RExpr::Ident("mpg".to_owned())));
    assert_eq!(term_names(&f), vec!["wt", "hp"]);
    assert!(f.intercept);
    assert!(f.terms.iter().all(|t| t.kind == TermKind::Main));
}

#[test]
fn a_one_sided_formula_has_no_response() {
    let f = formula("~ x");
    assert!(f.response.is_none());
    assert_eq!(term_names(&f), vec!["x"]);
}

#[test]
fn crossing_expands_to_main_effects_plus_the_interaction() {
    let f = formula("y ~ x1 * x2");
    assert_eq!(term_names(&f), vec!["x1", "x2", "x1:x2"]);
    assert_eq!(f.terms[2].kind, TermKind::Interaction);
}

#[test]
fn the_colon_operator_is_interaction_inside_a_formula() {
    let f = formula("y ~ x1:x2");
    assert_eq!(term_names(&f), vec!["x1:x2"]);
    assert_eq!(f.terms[0].kind, TermKind::Interaction);
}

#[test]
fn nesting_expands_to_the_outer_term_plus_the_interaction() {
    let f = formula("y ~ a / b");
    assert_eq!(term_names(&f), vec!["a", "a:b"]);
}

#[test]
fn crossing_to_an_order_produces_every_interaction_up_to_it() {
    let f = formula("y ~ (a + b + c)^2");
    assert_eq!(term_names(&f), vec!["a", "b", "c", "a:b", "a:c", "b:c"]);
    assert!(parse("y ~ (a + b)^99").is_err(), "an unbounded order fails");
}

#[test]
fn the_dot_term_and_a_removal_both_survive_structurally() {
    let f = formula("y ~ . - x3");
    assert_eq!(f.terms.len(), 1);
    assert_eq!(f.terms[0].kind, TermKind::Dot);
    assert_eq!(f.removed.len(), 1, "`- x3` is retained as a removal");
}

#[test]
fn a_removal_deletes_a_matching_expanded_term() {
    let f = formula("y ~ a + b - b");
    assert_eq!(term_names(&f), vec!["a"]);
}

#[test]
fn the_intercept_switch_is_a_flag_not_a_term() {
    assert!(!formula("y ~ x - 1").intercept);
    assert!(!formula("y ~ x + 0").intercept);
    assert!(formula("y ~ x + 1").intercept);
    assert_eq!(term_names(&formula("y ~ x - 1")), vec!["x"]);
}

#[test]
fn a_transformed_term_keeps_its_inner_expression() {
    let f = formula("y ~ I(x^2) + log(z)");
    assert_eq!(f.terms.len(), 2);
    assert!(f.terms.iter().all(|t| t.kind == TermKind::Transform));
    let RExpr::Call { callee, args } = f.terms[0].factors[0].unparenthesized() else {
        panic!("expected I(...)")
    };
    assert_eq!(**callee, RExpr::Ident("I".to_owned()));
    assert!(matches!(
        args[0].value.as_ref().map(RExpr::unparenthesized),
        Some(RExpr::Binary {
            op: BinaryOp::Power,
            ..
        })
    ));
}

#[test]
fn duplicate_terms_collapse() {
    assert_eq!(term_names(&formula("y ~ x + x")), vec!["x"]);
}

#[test]
fn control_flow_forms_parse_and_are_recognized() {
    for src in [
        "if (x > 1) y else z",
        "for (i in 1:10) f(i)",
        "while (x < 3) x <- x + 1",
        "repeat break",
        "function(a, b = 2) a + b",
        "{ a; b }",
    ] {
        let e = only(src);
        assert!(e.is_control_flow(), "`{src}` must route to logic:");
    }
}

#[test]
fn a_block_body_holds_its_own_statements() {
    let RExpr::Function { params, body } = only("function(x, y = 1) {\n  z <- x + y\n  z\n}")
    else {
        panic!("expected a function literal")
    };
    assert_eq!(params.len(), 2);
    assert_eq!(params[0].name, "x");
    assert!(params[1].default.is_some());
    let RExpr::Block(stmts) = *body else {
        panic!("expected a block body")
    };
    assert_eq!(stmts.len(), 2);
}

#[test]
fn both_pipes_parse_and_desugar_to_ordinary_calls() {
    let RExpr::Pipe { lhs, rhs, native } = only("mtcars %>% lm(mpg ~ wt, data = .)") else {
        panic!("expected a magrittr pipe")
    };
    assert!(!native);
    let desugared = desugar_pipe(&lhs, &rhs, native);
    assert_eq!(
        desugared.structure_key(),
        only("lm(mpg ~ wt, data = mtcars)").structure_key(),
        "the `.` placeholder receives the piped value"
    );

    let RExpr::Pipe { lhs, rhs, native } = only("x |> sum()") else {
        panic!("expected a native pipe")
    };
    assert!(native);
    assert_eq!(
        desugar_pipe(&lhs, &rhs, native).structure_key(),
        only("sum(x)").structure_key()
    );
}

#[test]
fn a_pipe_without_a_placeholder_inserts_first() {
    let RExpr::Pipe { lhs, rhs, native } = only("mtcars %>% summary()") else {
        panic!("expected a pipe")
    };
    assert_eq!(
        desugar_pipe(&lhs, &rhs, native).structure_key(),
        only("summary(mtcars)").structure_key()
    );
}

#[test]
fn a_newline_inside_a_call_continues_the_statement() {
    let s = script("fit <- lm(\n  mpg ~ wt,\n  data = mtcars\n)\n");
    assert_eq!(s.statements.len(), 1);
}

#[test]
fn a_multi_statement_script_splits_on_newlines_and_semicolons() {
    let s = script("a <- 1\nb <- 2; c <- 3\n\n# comment\nd <- 4\n");
    assert_eq!(s.statements.len(), 4);
    assert_eq!(s.statements[3].line, 5);
}

#[test]
fn a_malformed_script_is_a_positioned_hard_failure() {
    for src in [
        "lm(mpg ~ wt",
        "x <- ",
        "f(a b)",
        "{ a",
        "if (x",
        "for (1 in x) y",
    ] {
        let err = parse(src).expect_err("must not parse");
        assert!(
            format!("{err}").contains("R parse failure at line"),
            "`{src}` produced {err}"
        );
    }
}

#[test]
fn the_structure_key_separates_and_identifies_expressions() {
    assert_eq!(
        only("log(x)").structure_key(),
        only("log(x)").structure_key()
    );
    assert_ne!(
        only("log(x)").structure_key(),
        only("log(y)").structure_key()
    );
    assert_eq!(only("(a)").structure_key(), only("a").structure_key());
}
