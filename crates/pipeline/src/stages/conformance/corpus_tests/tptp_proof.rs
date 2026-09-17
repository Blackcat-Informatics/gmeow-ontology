// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Grade the authenticated native proof and terminal export of the authored TPTP cases.

use gmeow_conformance::external::tptp::{TptpRole, TptpSource, TstpTerm};

/// The committed `tptp-mini` problems, by case name.
const THEOREM_SUBCLASS: &str = "theorem-subclass";
const THEOREM_GROUND: &str = "theorem-ground";
const COUNTERSATISFIABLE: &str = "countersatisfiable";
const SATISFIABLE_OPEN: &str = "satisfiable-open";
const CONTRADICTORY_AXIOMS: &str = "contradictory-axioms";
const CNF_DISJOINT_CLASH: &str = "cnf-disjoint-clash";

fn source_observation(slug: &str) -> &'static gmeow_conformance::observations::TptpObservation {
    let key = format!("conformance/logic/cases/external/tptp-mini/{slug}/source/problem.p");
    super::common::supplemental().tptp[&key]
        .as_ref()
        .expect("source admitted")
        .as_ref()
        .expect("well-formed supported TPTP syntax")
}

fn prove(slug: &str) -> &'static gmeow_conformance::observations::ProofObservation {
    source_observation(slug)
        .proof
        .as_ref()
        .expect("native proof production succeeded")
}

fn gap(slug: &str) -> &gmeow_conformance::external::tptp::LoweringGap {
    match source_observation(slug).proof.as_ref().unwrap_err() {
        gmeow_conformance::external::tptp::DecisionError::Gap(gap) => gap,
        error => panic!("execution failure is not a capability gap: {error}"),
    }
}

#[test]
fn the_committed_tptp_mini_problem_corpus_still_parses() {
    // Inspect the exact producer-selected parse shared with the native proof and
    // decision lanes. Tests never parse the authored corpus a second time.
    const CORPUS: &[(&str, usize)] = &[
        (CNF_DISJOINT_CLASH, 4),
        (CONTRADICTORY_AXIOMS, 4),
        (COUNTERSATISFIABLE, 2),
        (SATISFIABLE_OPEN, 2),
        (THEOREM_GROUND, 3),
        (THEOREM_SUBCLASS, 3),
    ];
    for (case, expected) in CORPUS {
        let parsed = &source_observation(case).formulas;
        assert_eq!(parsed.len(), *expected, "{case} formula count");
        for af in parsed {
            assert_eq!(af.source, None, "{case}/{} carries no source", af.name);
            assert_eq!(af.useful_info, None, "{case}/{}", af.name);
            assert!(
                matches!(af.role, TptpRole::Premise | TptpRole::Conjecture),
                "{case}/{} role {:?}",
                af.name,
                af.role
            );
        }
    }
}

#[test]
fn theorem_subclass_lowers_to_a_proof_carrying_derivation() {
    // a ⊑ b, b ⊑ c ⊢ a ⊑ c. Negating the conjecture mints a witness w with a(w);
    // the Horn derivation c(w) ← b(w) ← a(w) IS the refutation of ¬c(w).
    let proved = prove(THEOREM_SUBCLASS);
    assert_eq!(proved.status, "ok");
    assert_eq!(proved.answers.len(), 1, "one derived goal instance");
    let tree = &proved.answers[0];
    assert_eq!(tree.steps.len(), 3, "c(w) ← b(w) ← a(w)");
    assert!(!tree.steps[0].asserted, "the root is a rule application");
    assert_eq!(tree.steps[0].premises, vec![1]);
    assert!(
        tree.steps.as_slice()[2].asserted,
        "the witness membership a(w) is the asserted leaf"
    );
    // Every step's identity is a genuine content-addressed derivation IRI.
    for step in tree.steps.as_slice() {
        assert!(
            step.derivation_iri
                .starts_with("https://blackcatinformatics.ca/gmeow/derivation/"),
            "{}",
            step.derivation_iri
        );
    }
}

#[test]
fn theorem_ground_lowers_to_a_two_step_derivation() {
    // a ⊑ b, a(x) ⊢ b(x): one rule application over one asserted fact.
    let proved = prove(THEOREM_GROUND);
    assert_eq!(proved.answers.len(), 1);
    let tree = &proved.answers[0];
    assert_eq!(tree.steps.len(), 2);
    assert!(tree.steps.as_slice()[1].asserted);
}

#[test]
fn a_non_theorem_lowers_and_derives_nothing() {
    // a(x) does NOT entail b(x): the goal is decided with an EMPTY answer set — no
    // proof exists, and none is fabricated.
    let proved = prove(COUNTERSATISFIABLE);
    assert_eq!(proved.status, "ok");
    assert!(proved.answers.is_empty());
}

#[test]
fn non_horn_and_goal_free_problems_are_honest_gaps() {
    // No conjecture ⇒ no goal to derive.
    let no_goal = gap(SATISFIABLE_OPEN);
    assert!(no_goal.reason.contains("no conjecture"), "{no_goal}");

    // `∀X.¬(b(X) ∧ c(X))` is a disjointness constraint, not a Horn clause.
    let disjointness = gap(CONTRADICTORY_AXIOMS);
    assert!(disjointness.reason.contains("Horn"), "{disjointness}");

    // `¬b(X) ∨ ¬c(X)` is an all-negative (goal) clause with no Horn head.
    let all_negative = gap(CNF_DISJOINT_CLASH);
    assert!(
        all_negative.reason.contains("all-negative"),
        "{all_negative}"
    );
}

#[test]
fn the_tstp_derivation_round_trips_through_the_parser() {
    use gmeow_logic::proof_tree::{tstp_step_derivation_iri, tstp_step_name};

    let proved = prove(THEOREM_SUBCLASS);
    let tree = &proved.answers[0];
    let parsed = &tree.parsed;
    assert_eq!(
        parsed.len(),
        tree.steps.len(),
        "one annotated formula per step"
    );

    // Names round-trip to the step identities, and the emitted (reverse) order lines up
    // with the tree's step table read backwards.
    for (i, af) in parsed.iter().enumerate() {
        let step = &tree.steps.as_slice()[tree.steps.len() - 1 - i];
        assert_eq!(
            af.name,
            tstp_step_name(&step.derivation_iri).expect("name"),
            "step name"
        );
        assert_eq!(
            tstp_step_derivation_iri(&af.name).expect("inverse"),
            step.derivation_iri,
            "name → derivation IRI is the exact inverse"
        );
        match (&step.rule_iri, &af.source, af.role) {
            (None, None, TptpRole::Premise) => {
                assert!(step.asserted, "an axiom line is an asserted leaf");
            }
            (
                Some(rule),
                Some(TptpSource::Inference {
                    rule: parsed_rule,
                    status,
                    parents,
                }),
                TptpRole::Derived,
            ) => {
                assert_eq!(parsed_rule, rule, "the cited firing rule survives");
                assert_eq!(
                    status,
                    &vec![TstpTerm::Func(
                        "status".into(),
                        vec![TstpTerm::Name("thm".into())]
                    )]
                );
                let expected: Vec<String> = step
                    .premises
                    .iter()
                    .map(|&p| {
                        tstp_step_name(&tree.steps.as_slice()[p].derivation_iri).expect("name")
                    })
                    .collect();
                assert_eq!(parents, &expected, "the parent SET survives");
            }
            other => panic!("step {i} did not round-trip: {other:?}"),
        }
    }
}

#[test]
fn the_committed_tstp_fixture_is_exactly_what_our_reasoner_produces() {
    // The shipped derivation fixture is a PRODUCT of the pipeline above, not a
    // hand-written artifact: compare its producer-selected export and require a
    // byte match of its derivation lines (the `%` header is prose). This also parses
    // the fixture as it ships, header and all.
    const FIXTURE: &str = include_str!("../../../../../math-lift/fixtures/theorem-subclass.tstp");

    let regenerated = &prove(THEOREM_SUBCLASS).answers[0].tstp;
    let committed: String = FIXTURE
        .lines()
        .filter(|l| !l.starts_with('%'))
        .map(|l| format!("{l}\n"))
        .collect();
    assert_eq!(
        &committed, regenerated,
        "the committed TSTP fixture drifted from what the reasoner now produces"
    );
    assert_eq!(
        super::common::supplemental()
            .tstp_fixture
            .as_ref()
            .expect("the shipped fixture must parse")
            .len(),
        3
    );
}
