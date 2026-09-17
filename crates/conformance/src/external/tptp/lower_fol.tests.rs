// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::external::tptp::parser::parse_tptp;

fn decide(src: &str) -> Result<ExternalOutcome, DecisionError> {
    let fs = parse_tptp(src).expect("parse ok");
    lower_and_decide(&fs, "https://gmeow.example/tptp-test/w").map(|(o, _)| o)
}

#[test]
fn disjointness_clash_is_inconsistent() {
    // unsat-clash: a⊑b, a⊑c, b⊥c, a(x).
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(a_sub_c, axiom, ![X] : (a(X) => c(X))).\n\
            fof(b_disj_c, axiom, ![X] : ~(b(X) & c(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
}

#[test]
fn open_model_is_consistent() {
    // satisfiable-open: a⊑b, a(x) — no clash.
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
}

#[test]
fn implication_to_negation_is_disjointness() {
    // a⊑b, b⊑¬c (as C→¬D), a(x), c(x) → x∈b and x∈¬c but also x∈c → clash.
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(b_disj_c, axiom, ![X] : (b(X) => ~c(X))).\n\
            fof(x_is_a, axiom, a(x)).\n\
            fof(x_is_c, axiom, c(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
}

#[test]
fn ground_unary_theorem_refutes_to_inconsistent() {
    // Premises a⊑b, a(x) ⊢ conjecture b(x): refutation is UNSAT.
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n\
            fof(goal, conjecture, b(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
}

#[test]
fn negated_ground_conjecture_is_an_honest_gap() {
    // A conjecture whose shape is `~C(a)` (a `Not(Atom)`) is not expressed by the
    // EL refutation lowerer, so it must surface as an honest capability gap
    // (LoweringGap) — never a silently-decided verdict. Extending the fragment to
    // cover it is a separate, soundness-reviewed change, not a silent approximation.
    let src = "\
            fof(prem, axiom, c(a)).\n\
            fof(goal, conjecture, ~c(a)).\n";
    let DecisionError::Gap(err) = decide(src).unwrap_err() else {
        panic!("expected a semantic gap, not an execution failure");
    };
    assert!(
        err.reason.contains("refutable") || err.reason.contains("conjecture shape"),
        "{}",
        err.reason
    );
}

#[test]
fn ground_unary_non_theorem_refutes_to_consistent() {
    // Premises a(x) do NOT entail b(x): refutation stays satisfiable.
    let src = "\
            fof(x_is_a, axiom, a(x)).\n\
            fof(goal, conjecture, b(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
}

#[test]
fn subclass_theorem_refutes_via_fresh_witness() {
    // a⊑b, b⊑c ⊢ a⊑c: negate → ∃X.(a(X) ∧ ¬c(X)); witness w∈a ⇒ w∈b ⇒ w∈c, clash w∈c̄.
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(b_sub_c, axiom, ![X] : (b(X) => c(X))).\n\
            fof(goal, conjecture, ![X] : (a(X) => c(X))).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
}

#[test]
fn subclass_non_theorem_is_consistent() {
    // a⊑b does NOT entail a⊑c.
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(goal, conjecture, ![X] : (a(X) => c(X))).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Consistent);
}

#[test]
fn cnf_disjointness_clash_is_inconsistent() {
    // Same unsat-clash, authored in CNF: ¬a∨b, ¬a∨c, ¬b∨¬c, a(x).
    let src = "\
            cnf(a_sub_b, axiom, ( ~a(X) | b(X) )).\n\
            cnf(a_sub_c, axiom, ( ~a(X) | c(X) )).\n\
            cnf(b_disj_c, axiom, ( ~b(X) | ~c(X) )).\n\
            cnf(x_is_a, axiom, a(x)).\n";
    assert_eq!(decide(src).unwrap(), ExternalOutcome::Inconsistent);
}

#[test]
fn cnf_two_positive_clause_is_a_capability_gap() {
    // `a(X) | b(X)` = ⊤ ⊑ a ⊔ b, a genuine disjunction outside EL.
    let src = "cnf(c, axiom, ( a(X) | b(X) )).\n";
    let DecisionError::Gap(err) = decide(src).unwrap_err() else {
        panic!("expected a semantic gap, not an execution failure");
    };
    assert!(err.reason.contains("disjunction"), "{err}");
}

#[test]
fn disjunctive_premise_is_a_capability_gap() {
    // A genuine disjunction in a premise body is outside the EL fragment.
    let src = "fof(d, axiom, ![X] : (a(X) => (b(X) | c(X)))).\n";
    let DecisionError::Gap(err) = decide(src).unwrap_err() else {
        panic!("expected a semantic gap, not an execution failure");
    };
    assert!(err.reason.contains("disjunction"), "{err}");
}

#[test]
fn binary_predicate_conjecture_is_a_capability_gap() {
    let src = "\
            fof(edge, axiom, r(a, b)).\n\
            fof(goal, conjecture, r(a, b)).\n";
    let DecisionError::Gap(err) = decide(src).unwrap_err() else {
        panic!("expected a semantic gap, not an execution failure");
    };
    assert!(err.reason.contains("role"), "{err}");
}

// -----------------------------------------------------------------------
// The Horn / backward-resolution (proof-minting) lowering
// -----------------------------------------------------------------------

#[test]
fn the_lowered_program_identity_is_content_addressed_and_stable() {
    let source = "fof(rule, axiom, ![X] : (first(X) => last(X))).\nfof(goal, conjecture, ![X] : (first(X) => last(X))).\n";
    let formulas = parse_tptp(source).unwrap();
    let a = lower_to_fol_program(&formulas).unwrap();
    let b = lower_to_fol_program(&formulas).unwrap();
    assert_eq!(a.iri, b.iri);
    let different =
        parse_tptp("fof(fact, axiom, first(alice)).\nfof(goal, conjecture, first(alice)).\n")
            .unwrap();
    let other = lower_to_fol_program(&different).unwrap();
    assert_ne!(a.iri, other.iri);
}

#[test]
fn world_scoped_edb_shape_matches_seed() {
    let src = "\
            fof(a_sub_b, axiom, ![X] : (a(X) => b(X))).\n\
            fof(x_is_a, axiom, a(x)).\n";
    let fs = parse_tptp(src).unwrap();
    let lowered = lower_problem(&fs, "https://gmeow.example/t/w").unwrap();
    assert_eq!(lowered.quad_count(), 2);
    // Every quad is scoped under the single world IRI.
    for line in lowered.to_nquads().unwrap().lines() {
        assert!(
            line.ends_with("<https://gmeow.example/t/w> ."),
            "quad not world-scoped: {line}"
        );
    }
}

#[test]
fn malformed_native_world_is_failure_not_capability_gap() {
    let formulas = parse_tptp("fof(fact, axiom, person(alice)).\n").unwrap();
    assert!(matches!(
        lower_problem(&formulas, "relative-world"),
        Err(DecisionError::Failure { .. })
    ));
}

#[test]
fn native_evaluation_failure_has_no_semantic_verdict() {
    let formulas = parse_tptp("fof(fact, axiom, person(alice)).\n").unwrap();
    let lowered = lower_problem(&formulas, "urn:world").unwrap();
    let error = decide_lowered_with(&lowered, &|_, _| {
        Err(gmeow_errors::Diag::of_kind(crate::error::RunFailed {
            detail: "synthetic native execution failure".to_owned(),
        }))
    })
    .unwrap_err();
    assert!(
        matches!(&error, DecisionError::Failure { detail } if detail.contains("synthetic native execution failure"))
    );
}

#[test]
fn direct_lowering_deduplicates_and_retains_the_selected_world() {
    let formulas =
        parse_tptp("fof(one, axiom, person(alice)).\nfof(two, axiom, person(alice)).\n").unwrap();
    let lowered = lower_problem(&formulas, "urn:selected-world").unwrap();
    assert_eq!(lowered.quad_count(), 1);
    assert!(
        lowered
            .dataset()
            .owned_quads()
            .all(|quad| quad.graph_name
                == Some(purrdf::RdfTerm::Iri("urn:selected-world".to_owned())))
    );
    let terminal = lowered.to_nquads().unwrap();
    assert_eq!(terminal.lines().count(), 1);
    assert!(terminal.ends_with("<urn:selected-world> .\n"));
}
