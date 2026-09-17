// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

/// The flagship fixture: a real R statistics script.
const MTCARS: &str = include_str!("../../fixtures/mtcars.R");
/// A syntactically valid script with no statistical content at all.
const UNLIFTABLE: &str = include_str!("../../fixtures/unliftable.R");

fn turtle(src: &str) -> String {
    lift(src.as_bytes(), BASE)
        .unwrap_or_else(|e| panic!("`{src}` must lift: {e}"))
        .turtle
}

fn count(ttl: &str, needle: &str) -> usize {
    ttl.matches(needle).count()
}

/// How many subjects the graph types as `math:{class}`.
///
/// Exact rather than substring: `Estimate` must not be counted by `Estimator`, nor
/// `Distribution` by `DistributionFamily`.
fn typed(ttl: &str, class: &str) -> usize {
    let suffix = format!("{RDF_TYPE_LINE} <{}> .", math(class));
    ttl.lines().filter(|line| line.ends_with(&suffix)).count()
}

/// The subjects carrying `predicate`.
fn subjects_with(ttl: &str, predicate: &str) -> BTreeSet<String> {
    let marker = format!(" <{predicate}> ");
    ttl.lines()
        .filter(|line| line.contains(&marker))
        .filter_map(|line| line.split(' ').next())
        .map(str::to_owned)
        .collect()
}

const RDF_TYPE_LINE: &str = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";

#[test]
fn a_non_utf8_source_is_a_typed_encoding_failure() {
    let err = lift(&[0x66, 0x69, 0x74, 0xff, 0xfe], BASE).expect_err("must not lift");
    assert!(format!("{err}").contains("not valid UTF-8"), "{err}");
}

#[test]
fn a_malformed_script_is_an_rparse_failure() {
    let err = lift(b"fit <- lm(mpg ~ wt, data = mtcars", BASE).expect_err("must not lift");
    assert!(
        format!("{err}").contains("R parse failure at line"),
        "{err}"
    );
}

#[test]
fn the_unliftable_fixture_hard_fails_rather_than_degrading() {
    let err = lift(UNLIFTABLE.as_bytes(), BASE).expect_err("must not lift");
    let text = format!("{err}");
    assert!(text.contains("no statistical content"), "{text}");
    assert!(
        text.contains("routed to logic:"),
        "the diagnostic must say what it DID do: {text}"
    );
}

#[test]
fn a_model_call_without_data_fits_the_calling_environment() {
    // `lm(y ~ x)` with no `data =` is ordinary R: the fit resolves its variables from the
    // calling environment, which IS a data source. So the fit is real, math:FittedModel's
    // min-1 math:fittedToData is satisfied by naming that environment rather than by
    // inventing a data frame, and what the script does not state — which frame, which
    // columns — is enumerated as residue.
    let ttl = turtle("fit <- lm(y ~ x)\n");
    assert_eq!(typed(&ttl, "FittedModel"), 1, "{ttl}");
    assert!(
        ttl.contains("the calling environment"),
        "the data source is named, not invented:\n{ttl}"
    );
    assert!(
        ttl.contains("no `data =` binding"),
        "…and the ungrounded binding is enumerated as residue:\n{ttl}"
    );
}

#[test]
fn a_two_sample_test_estimates_a_contrast_rather_than_fitting_a_model() {
    // `t.test(x, y)` is R's most common shape for the test and carries no formula, so
    // there is no specification to fit and NO math:FittedModel — minting one would
    // fabricate the restriction that class exists to pin. What the call does determine
    // is an estimand (the contrast) and the procedure that estimated it, which is
    // exactly math:Estimate's obligation.
    let ttl = turtle("r <- t.test(x, y)\n");
    assert_eq!(typed(&ttl, "Estimate"), 1, "{ttl}");
    assert_eq!(typed(&ttl, "FittedModel"), 0, "no formula, no fit:\n{ttl}");
    assert!(ttl.contains("Welch two-sample mean contrast"), "{ttl}");
    assert!(ttl.contains("the contrast between x and y"), "{ttl}");

    // The SAME function in its formula form is a fit — one name, two shapes, two
    // codomains.
    let fitted = turtle("t <- t.test(y ~ g, data = d)\n");
    assert_eq!(typed(&fitted, "FittedModel"), 1, "{fitted}");
}

#[test]
fn the_formula_binder_indexes_the_response_at_zero() {
    let ttl = turtle("fit <- lm(mpg ~ wt + hp, data = mtcars)\n");
    assert!(ttl.contains("ModelFormula"));
    assert!(ttl.contains("BindingExpression"), "the ~ is a binder");
    assert!(ttl.contains("boundVariable"));
    assert_eq!(typed(&ttl, "ArgumentSlot"), 3, "mpg, wt, hp");
    assert_eq!(count(&ttl, "slotIndex"), 3);
    assert!(ttl.contains(r#""0"^^"#), "the response sits at index 0");
    assert_eq!(typed(&ttl, "VariableExpression"), 3);
    // Index 0 holds the response.
    let response_slot = ttl
        .lines()
        .find(|l| l.contains("slotIndex") && l.contains(r#""0"^^"#))
        .expect("a slot at index 0");
    let slot_iri = response_slot.split(' ').next().unwrap_or_default();
    let expression = ttl
        .lines()
        .find(|l| l.starts_with(slot_iri) && l.contains("slotExpression"))
        .and_then(|l| l.split(' ').nth(2))
        .expect("slot 0 has an expression");
    assert!(
        ttl.contains(&format!(
            "{expression} <http://www.w3.org/2000/01/rdf-schema#label> \"mpg\" ."
        )),
        "index 0 must carry the response `mpg`"
    );
}

#[test]
fn a_suppressed_intercept_is_recorded_as_an_explicit_slot() {
    let with = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
    let without = turtle("fit <- lm(mpg ~ wt - 1, data = mtcars)\n");
    assert_eq!(typed(&with, "ArgumentSlot"), 2);
    assert_eq!(
        typed(&without, "ArgumentSlot"),
        3,
        "`- 1` adds an explicit zero intercept slot rather than vanishing"
    );
    assert!(without.contains("NumberLiteral"));
}

#[test]
fn an_interaction_term_lifts_as_an_application_over_its_factors() {
    let ttl = turtle("fit <- lm(y ~ a * b, data = d)\n");
    // Response + a + b + a:b.
    assert_eq!(typed(&ttl, "ModelFormula"), 1);
    assert_eq!(
        typed(&ttl, "ApplicationExpression"),
        1,
        "a:b is an application"
    );
    assert_eq!(
        typed(&ttl, "VariableExpression"),
        3,
        "y, a, b interned once"
    );
    assert_eq!(
        typed(&ttl, "ArgumentSlot"),
        4 + 2,
        "4 formula slots + 2 in a:b"
    );
}

#[test]
fn a_dot_formula_lifts_the_removal_structurally() {
    let ttl = turtle("fit <- lm(y ~ . - x3, data = d)\n");
    assert!(ttl.contains("ApplicationExpression"));
    assert!(
        ttl.contains("dot-expansion") || ttl.contains("Operation"),
        "the `.` is an operator, never a string"
    );
    assert!(
        ttl.contains("VariableExpression"),
        "the removed x3 survives as structure"
    );
}

#[test]
fn the_fitted_model_carries_both_min_one_restrictions() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
    assert!(ttl.contains("FittedModel"));
    assert!(ttl.contains("modelFormula"));
    assert!(ttl.contains("fittedToData"));
    assert!(ttl.contains("DatasetMatrix"));
}

#[test]
fn the_dataset_is_held_by_reference_and_never_inlined() {
    let ttl =
        turtle("d <- data.frame(x = c(1, 2, 3), y = c(4, 5, 6))\nfit <- lm(y ~ x, data = d)\n");
    assert!(ttl.contains("DatasetMatrix"));
    assert!(
        !ttl.contains("\"4\"") && !ttl.contains("\"5\""),
        "no column payload may reach the graph:\n{ttl}"
    );
}

#[test]
fn a_distribution_call_lifts_family_parameterization_and_roles() {
    let ttl = turtle("draws <- rnorm(100, mean = 0, sd = 1)\n");
    assert!(ttl.contains("Distribution"));
    assert!(ttl.contains("DistributionFamily"));
    assert!(ttl.contains("DistributionParameterization"));
    assert_eq!(typed(&ttl, "Distribution"), 1);
    assert_eq!(typed(&ttl, "DistributionParameterRole"), 2, "mean and sd");
    assert_eq!(
        typed(&ttl, "DistributionParameter"),
        2,
        "n is not a parameter"
    );
    assert!(ttl.contains("requiresPositiveValue"));
    assert!(ttl.contains("hasDimension"));
}

#[test]
fn r_supplies_its_own_documented_defaults_rather_than_dropping_a_role() {
    let ttl = turtle("draws <- rnorm(100)\n");
    assert_eq!(
        typed(&ttl, "DistributionParameterRole"),
        2,
        "mean = 0, sd = 1 are R language semantics, not invention"
    );
}

#[test]
fn a_distribution_missing_a_defaultless_parameter_refuses() {
    let err = lift(b"x <- rpois(10)\n", BASE).expect_err("must not lift");
    assert!(format!("{err}").contains("lambda"), "{err}");
}

#[test]
fn coefficients_lift_to_estimates_with_a_parameter_and_an_estimator() {
    let ttl = turtle("fit <- lm(mpg ~ wt + hp, data = mtcars)\nb <- coef(fit)\n");
    assert_eq!(typed(&ttl, "Estimate"), 3, "(Intercept), wt, hp");
    assert_eq!(typed(&ttl, "Estimator"), 1, "one shared OLS estimator");
    assert_eq!(count(&ttl, "estimatedParameter"), 3);
    assert_eq!(
        count(&ttl, "> <https://blackcatinformatics.ca/math/estimator>"),
        3
    );
    assert!(
        !ttl.contains("estimatesEstimand"),
        "an estimand needs six framing coordinates an R script never states"
    );
}

#[test]
fn every_broom_shaped_coefficient_accessor_reaches_the_same_estimates() {
    for accessor in [
        "b <- coef(fit)",
        "b <- summary(fit)$coefficients",
        "b <- fit$coefficients",
        "b <- broom::tidy(fit)",
    ] {
        let ttl = turtle(&format!("fit <- lm(mpg ~ wt, data = mtcars)\n{accessor}\n"));
        assert_eq!(
            typed(&ttl, "Estimate"),
            2,
            "`{accessor}` produced no estimate"
        );
        assert_eq!(
            typed(&ttl, "Estimator"),
            1,
            "`{accessor}` named no estimator"
        );
    }
}

#[test]
fn residual_accessors_lift_to_a_residual_of_the_fit() {
    for accessor in [
        "r <- residuals(fit)",
        "r <- resid(fit)",
        "r <- fit$residuals",
    ] {
        let ttl = turtle(&format!("fit <- lm(mpg ~ wt, data = mtcars)\n{accessor}\n"));
        assert_eq!(typed(&ttl, "Residual"), 1, "`{accessor}`");
        assert_eq!(count(&ttl, "residualOf"), 1, "`{accessor}`");
    }
}

#[test]
fn a_summary_read_is_a_vantage_held_observation() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\ns <- summary(fit)\n");
    assert!(ttl.contains("Observation"));
    assert!(ttl.contains("observedFeature"));
    assert!(ttl.contains("vantage"));
    assert!(ttl.contains("Standpoint"));
}

#[test]
fn arithmetic_lifts_to_application_expressions() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt) * 2\n");
    assert!(ttl.contains("ApplicationExpression"));
    assert!(ttl.contains("Multiplication"), "* is math:Multiplication");
    assert!(ttl.contains("Logarithm"), "log is math:Logarithm");
    assert!(ttl.contains("NumberLiteral"));
}

#[test]
fn control_flow_routes_to_logic_with_both_co_required_declarations() {
    // Every graph this crate can produce, checked subject by subject: a
    // math:compilesToLogicFormula edge without BOTH declarations is
    // math:UndeclaredLogicLowering.
    for src in [
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (z > 0) {\n  z <- z\n}\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nfor (i in 1:3) print(i)\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nwhile (TRUE) break\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nf <- function(a) a\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nlabel <- paste0(\"a\", \"b\")\n",
        MTCARS,
    ] {
        let ttl = turtle(src);
        let lowered = subjects_with(&ttl, &math("compilesToLogicFormula"));
        assert!(!lowered.is_empty(), "`{src}` routed nothing to logic:");
        assert_eq!(
            lowered,
            subjects_with(&ttl, &math("denotationKind")),
            "a lowering with no math:denotationKind: {src}"
        );
        assert_eq!(
            lowered,
            subjects_with(&ttl, &math("logicLoweringPreservation")),
            "a lowering with no math:logicLoweringPreservation: {src}"
        );
        assert!(ttl.contains(&logic("SoundUnderApproximation")));
        assert!(ttl.contains(&math("denotesProposition")));
    }
}

#[test]
fn every_lowered_formula_selects_exactly_one_constructor() {
    // logic:FormulaConstructorConstraint: every logic:Formula must select EXACTLY ONE of
    // {and, antecedent, exists, forall, iff, not, or, relation}. A node carrying none is
    // as malformed as one carrying two, and a bare `a logic:Formula` carries none — the
    // shape this lift originally emitted, copied from the hand-authored bridges.ttl
    // template, and caught only once a validate lane finally consumed the output.
    const CONSTRUCTORS: [&str; 8] = [
        "and",
        "antecedent",
        "exists",
        "forall",
        "iff",
        "not",
        "or",
        "relation",
    ];
    for src in [
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (z > 0) {\n  z <- z\n}\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nfor (i in 1:3) print(i)\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nwhile (TRUE) break\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nf <- function(a) a\n",
        MTCARS,
    ] {
        let ttl = turtle(src);
        let formulas = subjects_with(&ttl, &logic("Formula"));
        assert!(!formulas.is_empty(), "`{src}` lowered nothing");
        for formula in &formulas {
            let selected: Vec<&str> = CONSTRUCTORS
                .iter()
                .copied()
                .filter(|c| {
                    ttl.lines().any(|l| {
                        l.starts_with(formula.as_str()) && l.contains(&format!("<{}>", logic(c)))
                    })
                })
                .collect();
            assert_eq!(
                selected.len(),
                1,
                "<{formula}> selected {selected:?}; exactly one constructor is required\n{ttl}"
            );
        }
    }
}

#[test]
fn a_lowered_atom_carries_a_typed_relation_and_an_indexed_argument() {
    // logic:relation's range is a reified logic:Type; logic:TermCarrierIndexConstraint
    // requires an index on every carrier; logic:TermCarrierValueConstraint requires
    // exactly one term-value kind on it.
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nfor (i in 1:3) print(i)\n");
    assert!(ttl.contains(&format!("<{}> .", logic("Type"))), "{ttl}");
    assert!(
        ttl.contains(&format!("<{}> .", logic("TermCarrier"))),
        "{ttl}"
    );
    assert!(ttl.contains(&format!("<{}>", logic("termIndex"))), "{ttl}");
    assert!(
        ttl.contains(&format!("<{}>", logic("termVariable"))),
        "the loop variable is a real logic: variable term:\n{ttl}"
    );
    // The relation individuals are named for R's own constructs, not invented
    // categories: the loop's membership guard, and the call it makes.
    assert!(
        ttl.contains("r-for-in"),
        "the loop guard names R's own `in`:\n{ttl}"
    );
    assert!(
        ttl.contains("r-call:print"),
        "the call's relation names the callee:\n{ttl}"
    );
}

/// Every `logic:Formula` subject reachable in `ttl` that carries `constructor`.
fn formulas_with_constructor(ttl: &str, constructor: &str) -> BTreeSet<String> {
    subjects_with(ttl, &logic(constructor))
}

/// The objects of `<subject> <predicate> ?o`, as raw Turtle terms.
fn objects_of(ttl: &str, subject: &str, predicate: &str) -> Vec<String> {
    let marker = format!(" <{predicate}> ");
    ttl.lines()
        .filter(|line| line.starts_with(subject) && line.contains(&marker))
        .filter_map(|line| line.split(' ').nth(2))
        .map(str::to_owned)
        .collect()
}

/// Every `logic:TermCarrier` value the atom `formula` predicates over, as raw terms.
fn atom_arguments(ttl: &str, formula: &str) -> Vec<String> {
    let mut values = Vec::new();
    for carrier in objects_of(ttl, formula, &logic("argument")) {
        let carrier = carrier.trim_matches(['<', '>']);
        for kind in [
            "termIri",
            "termVariable",
            "termLiteral",
            "termApplication",
            "termSequenceMarker",
        ] {
            values.extend(objects_of(ttl, &format!("<{carrier}>"), &logic(kind)));
        }
    }
    values
}

#[test]
fn an_if_lowers_to_a_real_implication_over_the_lowered_condition() {
    // The flagship: `if (c) t else e` is `(c → t) ∧ (¬c → e)`, not an opaque node
    // tagged `r-if` whose only content is a truncated label.
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (nrow(cars) > 10) {\n  message(\"plenty\")\n} else {\n  \
             warning(\"sparse\")\n}\n",
    );
    let implications = formulas_with_constructor(&ttl, "antecedent");
    assert_eq!(implications.len(), 2, "one implication per arm:\n{ttl}");
    assert_eq!(
        formulas_with_constructor(&ttl, "consequent").len(),
        2,
        "an implication carries both halves"
    );
    assert_eq!(
        formulas_with_constructor(&ttl, "not").len(),
        1,
        "the else arm is guarded by the NEGATED condition"
    );
    assert_eq!(
        formulas_with_constructor(&ttl, "and").len(),
        1,
        "the two arms are conjoined"
    );

    // The condition itself is present as a real comparison over a real nested call —
    // `nrow(cars) > 10`, not a string.
    assert!(ttl.contains("\"r-greater\""), "{ttl}");
    assert!(ttl.contains("\"r-call:nrow\""), "{ttl}");
    assert!(
        ttl.contains(&format!("<{}>", logic("termApplication"))),
        "the nested `nrow(cars)` rides as a compound function term:\n{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{}> .", logic("FunctionTerm"))),
        "{ttl}"
    );
    assert!(
        ttl.contains("\"cars\""),
        "the condition's operand survives:\n{ttl}"
    );
    assert!(ttl.contains("\"10.0\""), "the threshold survives:\n{ttl}");
}

#[test]
fn both_branches_of_the_mtcars_conditional_survive_the_lowering() {
    // The finding this test closes: `grep -c "enough observations" lifted-r.ttl` was 0,
    // and so was `grep -c "message\|warning"`. The whole else arm had vanished.
    let ttl = turtle(MTCARS);
    assert!(
        ttl.contains("enough observations for the interaction term"),
        "the `message(…)` argument must survive:\n{ttl}"
    );
    assert!(
        ttl.contains("the interaction term is underpowered"),
        "the ELSE arm's `warning(…)` argument must survive"
    );
    assert!(ttl.contains("\"r-call:message\""));
    assert!(ttl.contains("\"r-call:warning\""));
}

#[test]
fn no_lowered_label_truncates_the_construct_it_names() {
    // A `…` in a label was the old lowering's only carrier of meaning. Labels remain
    // for readability, but nothing is elided out of one any more.
    let ttl = turtle(MTCARS);
    assert!(
        !ttl.contains('…'),
        "an ellipsis is a truncation, not a lowering:\n{ttl}"
    );
    assert!(
        ttl.contains(
            "if (nrow(cars) > 10) { message(\\\"enough observations for the \
                          interaction term\\\") } else"
        ),
        "the computation node's label renders the WHOLE construct:\n{ttl}"
    );
}

#[test]
fn distinct_callees_never_share_one_relation() {
    // `library(stats)` and `set.seed(20260725)` both used to predicate ONE `r-call`
    // relation, so the formula's predicate said nothing about which call it was.
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nlibrary(stats)\nset.seed(20260725)\n",
    );
    let library = subjects_with(&ttl, &logic("relation"))
        .into_iter()
        .filter_map(|f| {
            let relation = objects_of(&ttl, &f, &logic("relation")).pop()?;
            Some((f, relation))
        })
        .collect::<BTreeMap<_, _>>();
    let relations: BTreeSet<&String> = library.values().collect();
    assert_eq!(
        relations.len(),
        2,
        "two calls, two relations — not one shared `r-call`:\n{ttl}"
    );
    assert!(ttl.contains("\"r-call:library\""), "{ttl}");
    assert!(ttl.contains("\"r-call:set.seed\""), "{ttl}");
    // …and each atom carries its own argument, in order.
    for (formula, _) in library {
        assert_eq!(
            atom_arguments(&ttl, &formula).len(),
            1,
            "each call predicates over its own single argument"
        );
    }
    assert!(ttl.contains("\"stats\""), "{ttl}");
    assert!(ttl.contains("\"20260725.0\""), "{ttl}");
}

#[test]
fn a_nested_call_argument_is_a_real_term_application() {
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nprint(paste0(toupper(label), \"!\"))\n",
    );
    assert!(ttl.contains("\"r-call:print\""));
    assert!(ttl.contains("\"r-call:paste0\""));
    assert!(ttl.contains("\"r-call:toupper\""));
    assert_eq!(
        ttl.matches(&format!("<{}>", logic("FunctionTerm"))).count(),
        2,
        "paste0(…) and toupper(…) are function terms; print(…) is the atom:\n{ttl}"
    );
    assert!(ttl.contains(&format!("<{}>", logic("functionSymbol"))));
    assert!(
        ttl.contains("\"label\""),
        "the innermost variable survives three levels of nesting"
    );
}

#[test]
fn a_for_loop_lowers_to_a_guarded_universal_quantification() {
    // Carries a real fit: arithmetic alone is not statistical content, so a script
    // without one is an unliftable ingest and never reaches the lowering under test.
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\n\
             for (predictor in c(\"disp\", \"wt\")) cat(predictor)\n",
    );
    assert_eq!(
        formulas_with_constructor(&ttl, "forall").len(),
        1,
        "the traversal IS a universal quantification:\n{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{}>", logic("quantifiedVariable"))),
        "the loop variable is bound, not free:\n{ttl}"
    );
    assert_eq!(
        formulas_with_constructor(&ttl, "antecedent").len(),
        1,
        "the body is guarded by sequence membership"
    );
    assert!(ttl.contains("\"r-for-in\""));
    assert!(ttl.contains("\"disp\"") && ttl.contains("\"wt\""), "{ttl}");
}

#[test]
fn a_while_loop_lowers_to_an_implication_over_its_guard() {
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nwhile (remaining > 0) remaining <- remaining - 1\n",
    );
    assert_eq!(formulas_with_constructor(&ttl, "antecedent").len(), 1);
    assert!(ttl.contains("\"r-greater\""), "the guard survives:\n{ttl}");
    assert!(
        ttl.contains("\"r-assign\""),
        "the body's assignment survives:\n{ttl}"
    );
    assert!(ttl.contains("\"remaining\""));
}

#[test]
fn a_function_literal_lowers_over_its_formals_and_its_body() {
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nbanner <- function(text, width = 8) strrep(text, width)\n",
    );
    assert!(ttl.contains("\"r-function\""), "{ttl}");
    assert!(
        ttl.contains("\"r-parameter-default\""),
        "a formal's default is structure, not a dropped token:\n{ttl}"
    );
    assert!(
        ttl.contains("\"r-call:strrep\""),
        "the body survives:\n{ttl}"
    );
    assert!(ttl.contains("\"text\"") && ttl.contains("\"width\""));
    assert!(
        ttl.contains("\"8.0\""),
        "the default value survives:\n{ttl}"
    );
}

#[test]
fn logical_connectives_lower_to_the_logic_connectives() {
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (a > 1 && !(b < 2)) print(a)\n",
    );
    assert_eq!(formulas_with_constructor(&ttl, "and").len(), 1, "{ttl}");
    assert_eq!(formulas_with_constructor(&ttl, "not").len(), 1, "{ttl}");
    assert!(ttl.contains("\"r-greater\"") && ttl.contains("\"r-less\""));
}

#[test]
fn every_lowered_term_carrier_selects_exactly_one_value_kind() {
    // logic:TermCarrierValueConstraint: exactly one of termIri / termVariable /
    // termLiteral / termSequenceMarker / termApplication per carrier.
    const KINDS: [&str; 5] = [
        "termIri",
        "termVariable",
        "termLiteral",
        "termSequenceMarker",
        "termApplication",
    ];
    for src in [
        MTCARS,
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nf <- function(a, ...) a[[1]]\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nx <- d[, 1]\nplot(y ~ x)\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (is.null(v)) v <- NA else repeat break\n",
    ] {
        let ttl = turtle(src);
        let carriers = subjects_with(&ttl, &logic("termIndex"));
        assert!(!carriers.is_empty(), "`{src}` produced no carrier");
        for carrier in &carriers {
            let selected: Vec<&str> = KINDS
                .iter()
                .copied()
                .filter(|kind| {
                    ttl.lines().any(|l| {
                        l.starts_with(carrier.as_str()) && l.contains(&format!("<{}>", logic(kind)))
                    })
                })
                .collect();
            assert_eq!(
                selected.len(),
                1,
                "<{carrier}> selected {selected:?}; exactly one value kind is required\n{ttl}"
            );
        }
    }
}

#[test]
fn every_lowered_connective_meets_the_typed_ir_arity_invariant() {
    // crates/logic-compile/src/frontend.rs: `not` exactly one child, `and`/`or` at least
    // two, an implication exactly one antecedent AND one consequent, a quantifier at
    // least one bound variable, an atom at least one argument.
    for src in [
        MTCARS,
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (a) b else c\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nwhile (a && b) { p(); q() }\n",
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (a || b) next\n",
    ] {
        let ttl = turtle(src);
        for formula in subjects_with(&ttl, &logic("Formula")) {
            for (link, minimum) in [("and", 2), ("or", 2)] {
                let operands = objects_of(&ttl, &formula, &logic(link)).len();
                assert!(
                    operands == 0 || operands >= minimum,
                    "<{formula}> logic:{link} has {operands} operand(s)\n{ttl}"
                );
            }
            for link in ["not", "antecedent", "consequent", "forall", "exists"] {
                let operands = objects_of(&ttl, &formula, &logic(link)).len();
                assert!(operands <= 1, "<{formula}> logic:{link} × {operands}");
            }
            let antecedents = objects_of(&ttl, &formula, &logic("antecedent")).len();
            let consequents = objects_of(&ttl, &formula, &logic("consequent")).len();
            assert_eq!(
                antecedents, consequents,
                "<{formula}> is half an implication\n{ttl}"
            );
            if !objects_of(&ttl, &formula, &logic("relation")).is_empty() {
                assert!(
                    !objects_of(&ttl, &formula, &logic("argument")).is_empty(),
                    "<{formula}> is a nullary atomic predication\n{ttl}"
                );
            }
            if !objects_of(&ttl, &formula, &logic("forall")).is_empty() {
                assert!(
                    !objects_of(&ttl, &formula, &logic("quantifiedVariable")).is_empty(),
                    "<{formula}> is a vacuous binder\n{ttl}"
                );
            }
        }
        // Every argument carrier family is zero-based and contiguous.
        for parent in subjects_with(&ttl, &logic("argument")) {
            let mut indexes: Vec<i64> = objects_of(&ttl, &parent, &logic("argument"))
                .into_iter()
                .filter_map(|carrier| {
                    let carrier = carrier.trim_matches(['<', '>']);
                    objects_of(&ttl, &format!("<{carrier}>"), &logic("termIndex"))
                        .pop()?
                        .trim_matches('"')
                        .parse()
                        .ok()
                })
                .collect();
            indexes.sort_unstable();
            let expected: Vec<i64> = (0..i64::try_from(indexes.len()).unwrap_or(0)).collect();
            assert_eq!(indexes, expected, "<{parent}> argument indexes\n{ttl}");
        }
    }
}

#[test]
fn the_declared_loss_is_enumerated_on_the_source_witness() {
    // Rung::lossy_vague_with_witness() declares a LossyLens. math:unmappedConstruct is
    // what makes that declaration have content: "a lift that declares a rung weaker than
    // logic:ExactPreservation and enumerates nothing is asserting a loss it cannot name".
    let ttl = turtle(MTCARS);
    let witness = subjects_with(&ttl, &math("unmappedConstruct"));
    assert_eq!(witness.len(), 1, "the residue rides on ONE witness:\n{ttl}");
    assert!(
        witness.iter().all(|s| s.contains("r-src-")),
        "…and that witness is math:parseSource's"
    );
    let residue = count(&ttl, &math("unmappedConstruct"));
    assert!(
        residue >= 6,
        "only {residue} construct(s) enumerated:\n{ttl}"
    );
    for expected in [
        "R `for` iteration order",
        // math:unmappedConstruct's own skos:example names exactly these two.
        "R call `message`",
        "R call `warning`",
        "R call `library`",
        "R call `set.seed`",
        "R call `cat`",
    ] {
        assert!(ttl.contains(expected), "residue is missing `{expected}`");
    }
    // …and the rung it qualifies is unchanged.
    assert!(ttl.contains("LossyLens") && ttl.contains("Vague"));
}

#[test]
fn a_multi_statement_block_names_the_ordering_it_loses() {
    let ttl =
        turtle("fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nif (a) {\n  p(1)\n  q(2)\n}\n");
    assert_eq!(
        formulas_with_constructor(&ttl, "and").len(),
        1,
        "the block's statements are conjoined:\n{ttl}"
    );
    assert!(
        ttl.contains("R `{ }` statement sequencing"),
        "and the ordering it loses is named:\n{ttl}"
    );
}

#[test]
fn a_script_with_nothing_to_lose_enumerates_no_residue() {
    // An exact-for-this-script lift emits no math:unmappedConstruct, which is itself the
    // claim that nothing was lost — so the property is not a constant.
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
    assert_eq!(count(&ttl, &math("unmappedConstruct")), 0, "{ttl}");
}

#[test]
fn an_r_language_constant_rides_as_a_named_individual_with_its_loss_named() {
    let ttl = turtle(
        "fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\nhandle <- ifelse(is.na(x), NULL, x)\n",
    );
    assert!(
        ttl.contains("\"NULL\"") && ttl.contains("\"r-call:is.na\""),
        "{ttl}"
    );
    assert!(
        ttl.contains(&format!("<{}>", logic("termIri"))),
        "a 0-ary constant is an individual, not a nullary function term:\n{ttl}"
    );
    assert!(ttl.contains("zero-length-vector semantics"), "{ttl}");
}

#[test]
fn an_empty_subscript_argument_is_a_sequence_marker_rather_than_a_dropped_slot() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt)\ncolumn <- frame[, 2]\n");
    assert!(
        ttl.contains(&format!("<{}>", logic("termSequenceMarker"))),
        "{ttl}"
    );
    assert!(ttl.contains("\"r-subscript\""), "{ttl}");
    assert!(
        ttl.contains("R empty argument"),
        "the loss is named:\n{ttl}"
    );
}

#[test]
fn a_logic_lowering_is_always_generated_by_the_run() {
    // The r-bridge-source-fidelity-and-loss competency query joins on exactly this.
    let lifted = lift(MTCARS.as_bytes(), BASE).expect("the fixture lifts");
    let lowered = subjects_with(&lifted.turtle, &math("compilesToLogicFormula"));
    assert!(!lowered.is_empty());
    let generated = subjects_with(&lifted.turtle, &gmeow("wasGeneratedBy"));
    assert!(
        lowered.is_subset(&generated),
        "the competency query joins ?comp gmeow:wasGeneratedBy ?run with \
             math:compilesToLogicFormula, so every lowering must carry the back edge"
    );
    assert!(
        lifted
            .turtle
            .contains(&format!("{RDF_TYPE_LINE} <{}> .", logic("Formula"))),
        "the lowering target must be a logic:Formula"
    );
}

#[test]
fn the_pipe_forms_reach_the_same_lift_as_the_plain_call() {
    let plain = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
    let piped = turtle("fit <- mtcars %>% lm(mpg ~ wt, data = .)\n");
    assert_eq!(
        count(&plain, "FittedModel"),
        count(&piped, "FittedModel"),
        "a pipe is R syntax and carries no math: content of its own"
    );
    assert!(piped.contains("DatasetMatrix"));
}

#[test]
fn the_mtcars_fixture_lifts_every_expected_codomain_class() {
    let lifted = lift(MTCARS.as_bytes(), BASE).expect("the flagship fixture lifts");
    for class in [
        "RIngestRun",
        "ModelFormula",
        "BindingExpression",
        "ArgumentSlot",
        "VariableExpression",
        "VariableOccurrence",
        "FreeVariableDeclaration",
        "NumberLiteral",
        "FittedModel",
        "DatasetMatrix",
        "Distribution",
        "DistributionFamily",
        "DistributionParameterization",
        "Estimate",
        "Estimator",
        "Residual",
        "ApplicationExpression",
        "Operation",
        // The lowered R statements are math:MathematicalStatement, not
        // math:MathematicalExpression: they denote propositions and carry no structural
        // child of their own, which as a math:MathematicalExpression is exactly
        // math:StringOnlyComputableExpression.
        "MathematicalStatement",
        // The run-scoped math:Set every free-variable declaration is declared over.
        "Set",
    ] {
        assert!(
            typed(&lifted.turtle, class) > 0,
            "the mtcars fixture must produce a math:{class}"
        );
    }
    assert!(
        typed(&lifted.turtle, "RIngestRun") == 1,
        "exactly one ingest run"
    );
    assert!(
        lifted
            .turtle
            .contains(&format!("{RDF_TYPE_LINE} <{}> .", logic("Formula")))
    );
    assert!(
        lifted
            .turtle
            .contains(&format!("{RDF_TYPE_LINE} <{}> .", gmeow("Observation")))
    );
    assert!(lifted.codomain_nodes > 20, "a real script is dense");
    assert!(lifted.run_iri.contains("r-run-"));
}

#[test]
fn every_codomain_node_carries_the_back_edge_the_native_lint_reads() {
    let lifted = lift(MTCARS.as_bytes(), BASE).expect("the fixture lifts");
    assert_eq!(
        count(&lifted.turtle, "wasGeneratedBy"),
        lifted.codomain_nodes,
        "exactly one gmeow:wasGeneratedBy per generated node"
    );
}

#[test]
fn a_relift_of_the_same_source_is_byte_identical() {
    let a = lift(MTCARS.as_bytes(), BASE).expect("lifts").turtle;
    let b = lift(MTCARS.as_bytes(), BASE).expect("lifts").turtle;
    assert_eq!(a, b, "the lift is idempotent: no clock, no counter");
}

#[test]
fn positional_data_binds_by_the_callees_own_signature() {
    // R signatures differ where it matters: `lm(formula, data, …)` puts data at
    // positional 1, `glm(formula, family, data, …)` puts FAMILY there. Reading position
    // 1 for every fitter bound `binomial` as the dataset and orphaned the real one — a
    // math:FittedModel whose math:fittedToData is a distribution family is a false
    // statement in the shipped graph.
    //
    // 298 tests passed with that bug: not one exercised a POSITIONAL data argument.
    for (src, expected) in [
        ("fit <- lm(mpg ~ wt, mtcars)\n", "mtcars"),
        ("fit <- glm(cured ~ dose, binomial, trial)\n", "trial"),
        ("fit <- gam(y ~ x, poisson, trial)\n", "trial"),
        ("fit <- gbm(y ~ x, \"bernoulli\", trial)\n", "trial"),
    ] {
        let ttl = turtle(src);
        let fitted: Vec<&str> = ttl
            .lines()
            .filter(|l| l.contains(&math("fittedToData")))
            .filter_map(|l| l.split_whitespace().nth(2))
            .collect();
        assert_eq!(fitted.len(), 1, "{src} → {ttl}");
        let label = ttl
            .lines()
            .find(|l| l.starts_with(fitted[0]) && l.contains(RDFS_LABEL))
            .unwrap_or_else(|| panic!("the dataset carries a label:\n{ttl}"));
        assert!(
            label.contains(expected),
            "{src} must fit to `{expected}`, not to its family argument:\n{label}"
        );
    }
}

#[test]
fn an_unlisted_model_family_lifts_by_shape() {
    // The whitelist was closed three times by appending one more name — the mechanism
    // was the gap, not the table. A call carrying a model formula AND the data it fits
    // is a fit, whatever it is called; the estimator is then named by the call rather
    // than guessed at, so nothing claims a method the lift did not determine.
    for (src, estimator) in [
        (
            "fit <- coxph(Surv(t, e) ~ x, data = d)\nb <- coef(fit)\n",
            "coxph",
        ),
        ("fit <- gam(y ~ s, data = d)\nb <- coef(fit)\n", "gam"),
        ("fit <- rlm(y ~ x, data = d)\nb <- coef(fit)\n", "rlm"),
    ] {
        let ttl = turtle(src);
        assert_eq!(typed(&ttl, "FittedModel"), 1, "{src} → {ttl}");
        assert!(
            ttl.contains(&format!("the estimator `{estimator}(…)` applies")),
            "the estimator is named by the CALL, not invented:\n{ttl}"
        );
    }
}

#[test]
fn a_formula_taking_call_that_fits_nothing_is_not_a_fit() {
    // math:FittedModel requires min-1 math:modelFormula AND min-1 math:fittedToData, so
    // a call that names no data cannot be one. `plot(y ~ x)` takes a formula and fits
    // nothing — the ontology's own restriction is the discriminator.
    let err = lift(b"plot(y ~ x)\n", BASE).expect_err("a graphics call is not a fit");
    assert!(format!("{err}").contains("no statistical content"), "{err}");
}

#[test]
fn the_core_stats_model_families_lift() {
    // This test previously exercised only the FORMULA form, which the shape rule catches
    // whether or not the family is named — so it passed with `aov` and `t.test` deleted
    // from the table entirely, and that is what hid the hard-refusal of `t.test(x, y)`.
    // It now asserts the two things the family list actually decides.
    //
    // (1) A formula+data call on the list IS a fit.
    for (src, label) in [
        ("a <- aov(y ~ g, data = d)\n", "aov(y ~ g)"),
        ("t <- t.test(y ~ g, data = d)\n", "t.test(y ~ g)"),
    ] {
        let ttl = turtle(src);
        assert_eq!(typed(&ttl, "FittedModel"), 1, "{src} → {ttl}");
        assert!(ttl.contains(label), "the fit names the call:\n{ttl}");
    }

    // (2) The SAME family called without a formula does NOT hard-fail the script. R's
    // `t.test(x, y)` is the two-sample form and is canonical on the broom surface;
    // refusing the whole ingest over it made a real statistics script unliftable.
    let ttl =
        turtle("x <- c(1, 2, 3)\ny <- c(4, 5, 6)\nr <- t.test(x, y)\nm <- lm(y ~ x, data = d)\n");
    assert_eq!(
        typed(&ttl, "FittedModel"),
        1,
        "only the lm is a fit; t.test(x, y) carries no formula:\n{ttl}"
    );
    assert!(
        ttl.contains("t.test(x, y)"),
        "…and the two-sample call still reaches the graph through logic::\n{ttl}"
    );
}

#[test]
fn a_formula_bound_to_a_name_resolves_at_the_fit() {
    // Ordinary R: build the specification, then fit it. The fit was refused for
    // "carrying no model formula" while the script plainly stated one two lines up.
    let ttl = turtle("f <- y ~ x\nm <- lm(f, data = d)\n");
    assert_eq!(typed(&ttl, "FittedModel"), 1, "{ttl}");
    assert_eq!(typed(&ttl, "ModelFormula"), 1, "{ttl}");
}

#[test]
fn a_mixed_model_random_effect_lifts() {
    // lmer / glmer / lme were DECLARED estimators that no valid call could reach:
    // every mixed model carries a `( … | … )` term, and that term was refused as
    // having no math: image. A grouping structure is not an opaque string — the left
    // side is the effect that varies, the right the factor it varies by.
    for src in [
        "fit <- lmer(y ~ x + (1 | site), data = d)\n",
        "fit <- glmer(y ~ x + (1 | site), family = binomial, data = d)\n",
        "fit <- lme(y ~ x + (slope | group), data = d)\n",
    ] {
        let ttl = turtle(src);
        assert_eq!(typed(&ttl, "FittedModel"), 1, "{src} → {ttl}");
        assert!(
            ttl.contains("random effect | grouping factor"),
            "the grouping structure is lifted, not stringified:\n{ttl}"
        );
    }
}

#[test]
fn a_fit_inside_a_function_body_is_lifted() {
    // The most common real-world shape, and previously INVISIBLE: the whole form was
    // routed to logic: as control flow and its body was never walked, so a script whose
    // every model lived in a function lifted nothing at all.
    let ttl = turtle("analyse <- function(d) { lm(mpg ~ wt, data = d) }\nres <- analyse(mtcars)\n");
    assert_eq!(typed(&ttl, "FittedModel"), 1, "{ttl}");
    assert_eq!(typed(&ttl, "ModelFormula"), 1, "{ttl}");
    assert_eq!(typed(&ttl, "DatasetMatrix"), 1, "{ttl}");
}

#[test]
fn a_fit_inside_a_loop_body_is_lifted() {
    let ttl = turtle("for (v in c(\"wt\", \"hp\")) { fit <- lm(mpg ~ wt, data = mtcars) }\n");
    assert_eq!(typed(&ttl, "FittedModel"), 1, "{ttl}");
}

#[test]
fn incidental_arithmetic_is_not_statistical_content() {
    // The gate asks whether the script has STATISTICAL content, not whether any
    // math: node was emitted. `remaining - 1L` in a loop counter is arithmetic, and a
    // string-munging script that happens to decrement an index is still an unliftable
    // ingest — otherwise the gate would pass anything containing a minus sign.
    let err = lift(
        b"n <- 3\nwhile (n > 0) { writeLines(paste0(\"row \", n)); n <- n - 1L }\n",
        BASE,
    )
    .expect_err("incidental arithmetic must not satisfy the statistical gate");
    assert!(format!("{err}").contains("no statistical content"), "{err}");
}

#[test]
fn a_repeated_subexpression_interns_to_one_node() {
    // `log(wt)` twice: one variable node, one log application, two distinct sums.
    let shared = turtle("s <- rnorm(10, mean = 0, sd = 1)\nu <- log(wt) + 1\nv <- log(wt) + 2\n");
    let distinct = turtle("s <- rnorm(10, mean = 0, sd = 1)\nu <- log(wt) + 1\nv <- log(hp) + 2\n");
    assert_eq!(
        typed(&shared, "VariableExpression"),
        1,
        "one `wt`, mentioned twice:\n{shared}"
    );
    assert_eq!(
        typed(&shared, "ApplicationExpression"),
        3,
        "log(wt), log(wt)+1, log(wt)+2 — the repeated log(wt) collapses"
    );
    assert_eq!(typed(&distinct, "VariableExpression"), 2);
    assert_eq!(
        typed(&distinct, "ApplicationExpression"),
        4,
        "distinct structure DOES grow the fact count"
    );
}

#[test]
fn textual_repetition_alone_does_not_grow_the_graph() {
    let once = lift(
        b"fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt) * 2\n",
        BASE,
    )
    .expect("lifts");
    let thrice = lift(
            b"fit <- lm(mpg ~ wt, data = mtcars)\nz <- log(wt) * 2\ny <- log(wt) * 2\nx <- log(wt) * 2\n",
            BASE,
        )
        .expect("lifts");
    assert_eq!(
        once.codomain_nodes, thrice.codomain_nodes,
        "the fact count grows with DISTINCT structure, not textual repetition"
    );
}

#[test]
fn a_lifted_graph_carries_no_private_use_language_tag() {
    let lifted = lift(MTCARS.as_bytes(), BASE).expect("lifts");
    assert!(
        !lifted.turtle.contains("x-gmeow-"),
        "consumer output must not leak a private-use tag"
    );
}

#[test]
fn every_lifted_number_is_a_valid_xsd_decimal() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\nz <- wt * 1e5\n");
    assert!(ttl.contains("100000.0"), "no exponent form in xsd:decimal");
}

#[test]
fn the_run_frame_travels_with_every_lift() {
    let ttl = turtle("fit <- lm(mpg ~ wt, data = mtcars)\n");
    for required in [
        "RIngestRun",
        "parseSource",
        "instantiatesSchema",
        "instantiatesPlan",
        "ingestCorrespondence",
        "LossyLens",
        "mnemomorphic",
    ] {
        assert!(ttl.contains(required), "the frame is missing `{required}`");
    }
}
