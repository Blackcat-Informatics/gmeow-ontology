// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{Formula, Term};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const SCN: &str = "http://world/scenario";
const STANDPOINT: &str = "http://world/standpoint/alice";

// Vocabulary IRIs.
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const KNOWS: &str = "http://ex/knows";
const TRUSTS: &str = "http://ex/trusts";
const NAME: &str = "http://ex/name";
const ALICE: &str = "http://ex/alice";
const BOB: &str = "http://ex/bob";
const SAM_P: &str = "http://ex/sam";

const A_CLS: &str = "http://ex/A";
const B_CLS: &str = "http://ex/B";
const C_CLS: &str = "http://ex/C";
const D_CLS: &str = "http://ex/D";
const IND_A: &str = "http://ex/a";
const IND_X: &str = "http://ex/x";

/// A dataset of `(subject, predicate, object)` IRI triples all in the scenario world.
fn kb(triples: &[(&str, &str, &str)]) -> std::sync::Arc<purrdf::RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        let quad = RdfQuad::new(RdfTerm::iri(*s), *p, RdfTerm::iri(*o)).in_graph(RdfTerm::iri(SCN));
        builder.push_owned_quad(&quad);
    }
    builder.freeze().expect("valid test dataset")
}

fn binary_atom(rel: &str, s: &str, o: &str) -> Formula {
    Formula::atom(
        Term::iri(rel.to_owned()).unwrap(),
        vec![
            Term::iri(s.to_owned()).unwrap(),
            Term::iri(o.to_owned()).unwrap(),
        ],
    )
    .unwrap()
}

fn literal_atom(rel: &str, s: &str, lexical: &str, datatype: Option<&str>) -> Formula {
    Formula::atom(
        Term::iri(rel.to_owned()).unwrap(),
        vec![
            Term::iri(s.to_owned()).unwrap(),
            Term::literal(lexical, datatype.map(str::to_owned)).unwrap(),
        ],
    )
    .unwrap()
}

fn literal_kb(subject: &str, predicate: &str, literal: RdfLiteral) -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let quad = RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::literal(literal))
        .in_graph(RdfTerm::iri(SCN));
    builder.push_owned_quad(&quad);
    builder.freeze().expect("valid literal test dataset")
}

/// `∀x. rel₁(x, c₁) → rel₂(x, c₂)` — a universally-quantified Horn implication.
fn forall_horn(rel1: &str, c1: &str, rel2: &str, c2: &str) -> Formula {
    Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::iri(rel1.to_owned()).unwrap(),
                    vec![Term::var("x").unwrap(), Term::iri(c1.to_owned()).unwrap()],
                )
                .unwrap(),
            ),
            Box::new(
                Formula::atom(
                    Term::iri(rel2.to_owned()).unwrap(),
                    vec![Term::var("x").unwrap(), Term::iri(c2.to_owned()).unwrap()],
                )
                .unwrap(),
            ),
        )),
    }
}

// ── Test 1: ∀-Horn lowered & resolved (THE BINDING FIX) ──────────────────────

#[test]
fn forall_horn_already_satisfied_is_corroborated_not_unsupported() {
    // ∀x. knows(x, alice) → trusts(x, bob); KB has knows(sam, alice) AND trusts(sam, bob).
    // The rule is already satisfied (redundant) ⇒ has_proof, Corroborated. NOT Unsupported.
    let store = kb(&[(SAM_P, KNOWS, ALICE), (SAM_P, TRUSTS, BOB)]);
    let candidate = forall_horn(KNOWS, ALICE, TRUSTS, BOB);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::Completed,
        "a ∀-Horn implication MUST lower and RESOLVE, never be refused as Unsupported"
    );
    assert_ne!(ans.verdict.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(ans.verdict.information, InformationState::Supported);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Corroborated);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    assert!(ans.witness.is_none());
}

#[test]
fn forall_horn_fires_new_fact_is_open_not_unsupported() {
    // Same rule, but KB has knows(sam, alice) and NOT trusts(sam, bob). The rule FIRES,
    // deriving trusts(sam, bob) (a new, consistent fact) ⇒ not already entailed ⇒ Open.
    let store = kb(&[(SAM_P, KNOWS, ALICE)]);
    let candidate = forall_horn(KNOWS, ALICE, TRUSTS, BOB);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert_eq!(ans.verdict.evaluation, EvaluationStatus::Completed);
    assert_ne!(ans.verdict.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(
        ans.verdict.information,
        InformationState::Neither,
        "the KB does not already entail the head, so neither proof nor counterproof"
    );
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    // The Horn formula evaluated exactly — no residue disclosed.
    assert!(
        ans.verdict.preservation.unsupported_constructs.is_empty(),
        "a fully-Horn formula discloses no residue: {:?}",
        ans.verdict.preservation
    );
}

// ── Test 2: ground atom already derivable ⇒ Supported/Corroborated ───────────

#[test]
fn ground_atom_already_derivable_is_supported() {
    // candidate rdf:type(a, B); KB: a:A, A ⊑ B ⇒ DL derives a:B, so φ is redundant ⇒ Supported.
    let store = kb(&[(IND_A, TYPE, A_CLS), (A_CLS, SUBCLASS, B_CLS)]);
    let candidate = binary_atom(TYPE, IND_A, B_CLS);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert_eq!(ans.verdict.information, InformationState::Supported);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Corroborated);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    assert!(ans.witness.is_none(), "a corroboration carries no witness");
}

// ── Test 3: KB REFUTES φ ⇒ Opposed/Both, RefutedInStandpoint, witness ─────────

#[test]
fn kb_refutes_candidate_is_refuted_in_standpoint_with_witness() {
    // candidate rdf:type(a, B); KB: a:A, A disjointWith B ⇒ asserting a:B forces a into
    // owl:Nothing ⇒ inconsistent ⇒ Opposed, RefutedInStandpoint, with a sorted witness.
    let store = kb(&[(IND_A, TYPE, A_CLS), (A_CLS, DISJOINT, B_CLS)]);
    let candidate = binary_atom(TYPE, IND_A, B_CLS);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert!(
        matches!(
            ans.verdict.information,
            InformationState::Opposed | InformationState::Both
        ),
        "a refutation is Opposed or Both, got {:?}",
        ans.verdict.information
    );
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::RefutedInStandpoint);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    let witness = ans.witness.expect("a refutation MUST name a witness");
    assert_eq!(witness.individual, IND_A);
    assert!(
        !witness.premises.is_empty(),
        "the witness carries the premises that entailed the clash"
    );
    // ContradictionWitness derives Ord over sorted premises; run_reasoning sorts them.
    let mut sorted = witness.premises.clone();
    sorted.sort();
    assert_eq!(witness.premises, sorted, "premises must be sorted");
}

// ── Test 3b: KB supports BOTH φ and ¬φ ⇒ genuine Both glut ────────────────────

#[test]
fn kb_supporting_both_phi_and_not_phi_is_a_genuine_both_glut() {
    // The promised Both-quadrant hard surface. The KB commits, from ONE standpoint, to BOTH
    // the candidate φ = rdf:type(a, B) — it directly asserts a:B (told TRUE) — AND its
    // negation ¬φ — a:A with A disjointWith B refutes a:B (told FALSE). Because the φ leg
    // (redundant: a:B already entailed) and the ¬φ leg (asserting a:B clashes) are computed
    // INDEPENDENTLY, both fire ⇒ classify → Both. This is a within-standpoint contradiction
    // LOCALIZED to the candidate proposition, which is testable (a genuine glut about φ),
    // NOT a foreign inconsistency (contrast already_inconsistent_kb_is_hard_error, candidate
    // a:C, which the base neither entails nor genuinely refutes → hard error).
    let store = kb(&[
        (IND_A, TYPE, A_CLS),
        (IND_A, TYPE, B_CLS),
        (A_CLS, DISJOINT, B_CLS),
    ]);
    let candidate = binary_atom(TYPE, IND_A, B_CLS);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("a glut localized to the candidate is testable, not a hard error");

    assert_eq!(
        ans.verdict.information,
        InformationState::Both,
        "the KB entails φ (a:B redundant) AND refutes φ (a:A ⊓ A⊓B disjoint) ⇒ Belnap glut"
    );
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::RefutedInStandpoint);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    let witness = ans
        .witness
        .expect("a Both glut MUST name its contradiction witness (validate() invariant)");
    assert_eq!(witness.individual, IND_A);
    assert!(
        !witness.premises.is_empty(),
        "the glut witness carries the premises that entailed the clash"
    );
    // The result invariants hold: a Both must carry a witness or a proof+counterproof pair.
    ans.verdict
        .validate()
        .expect("a Both carrying its witness satisfies the ReasoningResult invariants");
}

// ── Test 4: conclusively independent ⇒ Neither, Open, Discharged ──────────────

#[test]
fn conclusively_independent_is_neither_open_discharged() {
    // candidate rdf:type(a, C); KB: a:A only (C unrelated). φ adds a new consistent fact,
    // neither entailed nor refuted ⇒ Neither (conclusive) ⇒ Open, Discharged.
    let store = kb(&[(IND_A, TYPE, A_CLS)]);
    let candidate = binary_atom(TYPE, IND_A, C_CLS);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert_eq!(ans.verdict.information, InformationState::Neither);
    assert!(ans.verdict.is_conclusive());
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Discharged);
    assert!(ans.witness.is_none());
}

// ── Test 5: budget-truncated ⇒ BudgetExhausted, Open, Unknown ─────────────────

#[test]
fn budget_truncation_is_budget_exhausted_open_unknown() {
    // A subclass chain A ⊑ B ⊑ C ⊑ D with x:A derives a closure far larger than max=1.
    let store = kb(&[
        (A_CLS, SUBCLASS, B_CLS),
        (B_CLS, SUBCLASS, C_CLS),
        (C_CLS, SUBCLASS, D_CLS),
        (IND_X, TYPE, A_CLS),
    ]);
    // A redundant ground atom candidate — the verdict is dominated by the budget ceiling.
    let candidate = binary_atom(TYPE, IND_X, A_CLS);
    let budget = Budget {
        max_answers: Some(1),
        max_steps: Some(1),
    };
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &budget)
        .expect("conjecture_test ok");

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::BudgetExhausted,
        "the derived closure exceeds the declared budget ceiling"
    );
    assert_eq!(ans.verdict.information, InformationState::Undetermined);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Unknown);
    assert_ne!(ans.lifecycle, ConjectureLifecycleState::RefutedInStandpoint);
    assert_ne!(ans.verdict.information, InformationState::Neither);
}

#[test]
fn ground_candidate_step_budget_cuts_incremental_recursion_inline() {
    // The stable base contains only the schema edge A ⊑ B. Adding x:A would derive
    // x:B in the first incremental round, but max_steps=0 cuts before that sorted
    // derived-fact commit. The asserted candidate remains visible and the cached base
    // is not recharged.
    let store = kb(&[(A_CLS, SUBCLASS, B_CLS)]);
    let candidate = binary_atom(TYPE, IND_X, A_CLS);
    let budget = Budget {
        max_answers: None,
        max_steps: Some(0),
    };
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &budget)
        .expect("inline-governed conjecture test");

    assert_eq!(ans.verdict.evaluation, EvaluationStatus::BudgetExhausted);
    assert_eq!(ans.verdict.information, InformationState::Undetermined);
    assert_eq!(ans.verdict.provenance.consumed_budget.consumed, 0);
    let triples = triple_set(ans.verdict.inferred());
    assert!(triples.contains(&(
        IND_X.to_owned(),
        TYPE.to_owned(),
        format!("<{A_CLS}>"),
        SCN.to_owned(),
    )));
    assert!(!triples.contains(&(
        IND_X.to_owned(),
        TYPE.to_owned(),
        format!("<{B_CLS}>"),
        SCN.to_owned(),
    )));
}

#[test]
fn new_literal_candidate_is_not_falsely_corroborated() {
    // Literal objects are outside the fixed class/property rule EDB, but they are still
    // first-class signed facts. The candidate assertion must therefore make the adjusted
    // closure differ from the base. The former scratch fallback dropped the literal and
    // falsely treated this new fact as redundant/Supported.
    let store = kb(&[]);
    let candidate = literal_atom(NAME, IND_X, "Alice", None);
    let ans = conjecture_test(
        &store,
        SCN,
        &candidate,
        STANDPOINT,
        &[],
        &Budget {
            max_answers: None,
            max_steps: Some(0),
        },
    )
    .expect("literal candidate uses the inline signed-fact path");

    assert_eq!(ans.verdict.evaluation, EvaluationStatus::Completed);
    assert_eq!(ans.verdict.information, InformationState::Neither);
    assert_eq!(ans.verdict.provenance.consumed_budget.consumed, 0);
    assert!(ans.verdict.inferred().iter().any(|axiom| {
        axiom.subject == IND_X
            && axiom.predicate == NAME
            && axiom.object.contains("Alice")
            && axiom.is_edb
    }));
}

#[test]
fn already_asserted_literal_candidate_is_corroborated_without_recharge() {
    let store = literal_kb(IND_X, NAME, RdfLiteral::simple("Alice"));
    let candidate = literal_atom(NAME, IND_X, "Alice", None);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("already-asserted literal candidate is a zero-delta shot");

    assert_eq!(ans.verdict.evaluation, EvaluationStatus::Completed);
    assert_eq!(ans.verdict.information, InformationState::Supported);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Corroborated);
    assert_eq!(ans.verdict.provenance.consumed_budget.consumed, 0);
}

#[test]
fn literal_candidate_reaches_literal_aware_dl_refutation() {
    const FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
    const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

    let mut builder = RdfDatasetBuilder::new();
    for quad in [
        RdfQuad::new(RdfTerm::iri(NAME), TYPE, RdfTerm::iri(FUNCTIONAL))
            .in_graph(RdfTerm::iri(SCN)),
        RdfQuad::new(
            RdfTerm::iri(IND_X),
            NAME,
            RdfTerm::literal(RdfLiteral::typed("Alice", XSD_STRING)),
        )
        .in_graph(RdfTerm::iri(SCN)),
    ] {
        builder.push_owned_quad(&quad);
    }
    let store = builder.freeze().expect("valid functional-literal KB");
    let candidate = literal_atom(NAME, IND_X, "Bob", Some(XSD_STRING));
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("literal candidate must reach the DL post-pass");

    assert_eq!(ans.verdict.information, InformationState::Opposed);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::RefutedInStandpoint);
    assert!(ans.witness.is_some());
}

#[test]
fn ground_candidate_incremental_closure_matches_scratch_with_real_premises() {
    let store = kb(&[(A_CLS, SUBCLASS, B_CLS), (B_CLS, SUBCLASS, C_CLS)]);
    let base_edb = build_scenario_edb(&store, SCN, &[], None).unwrap();
    let base = reason_all(&base_edb).unwrap();
    let with_candidate = build_scenario_edb(
        &store,
        SCN,
        &[],
        Some((IND_X.to_owned(), TYPE.to_owned(), RdfTerm::iri(A_CLS))),
    )
    .unwrap();
    let scratch = reason_all(&with_candidate).unwrap();
    let object = RdfTerm::iri(A_CLS);
    let incremental = crate::reason::reason_ground_fact_insert_incremental(
        crate::reason::GroundFactIncrementalRequest {
            base_edb: &base_edb,
            with_candidate_edb: &with_candidate,
            base: &base,
            scenario_world: SCN,
            subject: IND_X,
            predicate: TYPE,
            object: &object,
            max_steps: None,
        },
    )
    .unwrap();

    assert_eq!(incremental.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(
        triple_set(incremental.result.inferred()),
        triple_set(scratch.inferred()),
        "incremental and scratch closures must be fact-identical"
    );
    assert_eq!(incremental.result.is_consistent(), scratch.is_consistent());
    let derived_b = incremental
        .result
        .inferred()
        .iter()
        .find(|axiom| {
            axiom.subject == IND_X
                && axiom.predicate == TYPE
                && axiom.object == format!("<{B_CLS}>")
        })
        .expect("x:B is incrementally derived");
    assert!(derived_b.rule_name.is_some());
    assert!(
        !derived_b.premises.is_empty(),
        "incremental derivations carry their real immediate premises"
    );
}

#[test]
fn asserting_an_already_derived_candidate_promotes_it_to_edb_provenance() {
    let store = kb(&[(A_CLS, SUBCLASS, B_CLS), (B_CLS, SUBCLASS, C_CLS)]);
    let base_edb = build_scenario_edb(&store, SCN, &[], None).unwrap();
    let base = reason_all(&base_edb).unwrap();
    assert!(base.inferred().iter().any(|axiom| {
        axiom.subject == A_CLS
            && axiom.predicate == SUBCLASS
            && axiom.object == format!("<{C_CLS}>")
            && !axiom.is_edb
    }));

    let with_candidate = build_scenario_edb(
        &store,
        SCN,
        &[],
        Some((A_CLS.to_owned(), SUBCLASS.to_owned(), RdfTerm::iri(C_CLS))),
    )
    .unwrap();
    let object = RdfTerm::iri(C_CLS);
    let adjusted = crate::reason::reason_ground_fact_insert_incremental(
        crate::reason::GroundFactIncrementalRequest {
            base_edb: &base_edb,
            with_candidate_edb: &with_candidate,
            base: &base,
            scenario_world: SCN,
            subject: A_CLS,
            predicate: SUBCLASS,
            object: &object,
            max_steps: None,
        },
    )
    .unwrap();

    let candidate = adjusted
        .result
        .inferred()
        .iter()
        .find(|axiom| {
            axiom.subject == A_CLS
                && axiom.predicate == SUBCLASS
                && axiom.object == format!("<{C_CLS}>")
                && axiom.world == SCN
        })
        .expect("candidate remains in the adjusted closure");
    assert!(
        candidate.is_edb,
        "the newly asserted candidate owns EDB provenance"
    );
    assert!(candidate.rule_name.is_none());
    assert!(candidate.premises.is_empty());
}

// ── Test 6: beyond-fragment candidate ⇒ Unsupported + disclosed residue ───────

#[test]
fn disjunctive_head_is_unsupported_with_disclosed_residue() {
    // ∀x. knows(x, alice) → (trusts(x, bob) ∨ trusts(x, sam)) — a disjunctive head does NOT
    // lower to a rule ⇒ Unsupported, NotEvaluated, residue disclosed.
    let candidate = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(
                Formula::atom(
                    Term::iri(KNOWS.to_owned()).unwrap(),
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(ALICE.to_owned()).unwrap(),
                    ],
                )
                .unwrap(),
            ),
            Box::new(Formula::Or(vec![
                Formula::atom(
                    Term::iri(TRUSTS.to_owned()).unwrap(),
                    vec![Term::var("x").unwrap(), Term::iri(BOB.to_owned()).unwrap()],
                )
                .unwrap(),
                Formula::atom(
                    Term::iri(TRUSTS.to_owned()).unwrap(),
                    vec![
                        Term::var("x").unwrap(),
                        Term::iri(SAM_P.to_owned()).unwrap(),
                    ],
                )
                .unwrap(),
            ])),
        )),
    };
    let store = kb(&[(SAM_P, KNOWS, ALICE)]);
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::Unsupported,
        "a fully beyond-fragment candidate was never evaluated"
    );
    assert_eq!(ans.verdict.information, InformationState::NotEvaluated);
    assert!(
        !ans.verdict.preservation.unsupported_constructs.is_empty(),
        "the beyond-fragment residue MUST be disclosed, not silently absent: {:?}",
        ans.verdict.preservation
    );
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Unknown);
}

// ── Test 7: already-inconsistent KB ⇒ hard Err ───────────────────────────────

#[test]
fn already_inconsistent_kb_is_hard_error() {
    // KB already forces a into owl:Nothing (a:A, a:B, A disjointWith B), but the candidate
    // a:C is UNRELATED to that glut: the base neither entails a:C (not redundant) nor
    // genuinely refutes it. Testing against a world contradictory for foreign reasons is
    // ex-falso-meaningless ⇒ hard Err. (Contrast candidate a:B, which the base both entails
    // and refutes → a genuine Both glut; see kb_supporting_both_phi_and_not_phi_*.)
    let store = kb(&[
        (IND_A, TYPE, A_CLS),
        (IND_A, TYPE, B_CLS),
        (A_CLS, DISJOINT, B_CLS),
    ]);
    let candidate = binary_atom(TYPE, IND_A, C_CLS);
    let err =
        conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default()).unwrap_err();
    assert!(
        err.message().contains("ALREADY") && err.message().contains("inconsistent"),
        "the hard foreign-inconsistency surface must be reported: {err}"
    );
}

// ── Test 7b: same foreign inconsistency, but budget-tripped ⇒ non-conclusion, NOT Err ──

#[test]
fn budget_tripped_foreign_inconsistency_is_undetermined_not_ex_falso_error() {
    // Identical scenario KB to `already_inconsistent_kb_is_hard_error` — a:A, a:B, and
    // A disjointWith B force a into owl:Nothing, a glut UNRELATED to the candidate a:C — but
    // this time the budget is so tight (`max_answers: Some(0)`) that it trips BEFORE the `φ`
    // leg can be treated as having run to a genuine conclusion: the base's derived closure
    // already carries one non-EDB axiom (`a rdf:type owl:Nothing`), which alone exceeds a
    // zero answer ceiling. A truncated chase over an apparently-inconsistent base must NOT
    // be ex-falso'd into a hard `Err` — `has_proof`/`has_counterproof` being false could be an
    // artifact of the cut, not a decided absence of the glut relation — so the correct verdict
    // is the honest budget-exhausted non-conclusion (`Undetermined`/`Open`/`Unknown`), exactly
    // like any other budget-truncated run (contrast `budget_truncation_is_budget_exhausted_open_unknown`).
    let store = kb(&[
        (IND_A, TYPE, A_CLS),
        (IND_A, TYPE, B_CLS),
        (A_CLS, DISJOINT, B_CLS),
    ]);
    let candidate = binary_atom(TYPE, IND_A, C_CLS);
    let budget = Budget {
        max_answers: Some(0),
        max_steps: None,
    };
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &budget).expect(
        "a budget-tripped run over an apparently-inconsistent base must be a non-conclusion, \
         never the ex-falso hard error",
    );

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::BudgetExhausted,
        "the answer ceiling must be the reason this run did not reach a conclusion"
    );
    assert_eq!(
        ans.verdict.information,
        InformationState::Undetermined,
        "a budget-truncated leg over an inconsistent-looking base is undetermined, never a \
         decided Belnap quadrant"
    );
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Unknown);
}

// ── Test 8: isolation — the input KB dataset is never mutated ─────────────────

#[test]
fn input_kb_dataset_is_never_mutated() {
    let store = kb(&[(IND_A, TYPE, A_CLS), (A_CLS, DISJOINT, B_CLS)]);
    let before = store.owned_quads().count();
    // A refuting candidate (exercises witness + inconsistency inside the isolated copy).
    let candidate = binary_atom(TYPE, IND_A, B_CLS);
    let _ = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &Budget::default())
        .expect("conjecture_test ok");
    let after = store.owned_quads().count();
    assert_eq!(before, after, "the borrowed KB dataset must be unchanged");
}

// ── standpoint is required (Principle 9) ─────────────────────────────────────

#[test]
fn empty_standpoint_is_rejected() {
    let store = kb(&[(IND_A, TYPE, A_CLS)]);
    let candidate = binary_atom(TYPE, IND_A, C_CLS);
    let err = conjecture_test(&store, SCN, &candidate, "", &[], &Budget::default()).unwrap_err();
    assert!(err.message().contains("standpoint"), "got: {err}");
}

// ── assume_context is layered into the scenario EDB ──────────────────────────

#[test]
fn assume_context_facts_participate_in_the_scenario() {
    // KB has only the disjointness TBox; the assume-context supplies a:A. The candidate a:B
    // then clashes ⇒ refuted. Proves assume_context reaches the chase.
    let store = kb(&[(A_CLS, DISJOINT, B_CLS)]);
    let assume = vec![(IND_A.to_owned(), TYPE.to_owned(), A_CLS.to_owned())];
    let candidate = binary_atom(TYPE, IND_A, B_CLS);
    let ans = conjecture_test(
        &store,
        SCN,
        &candidate,
        STANDPOINT,
        &assume,
        &Budget::default(),
    )
    .expect("conjecture_test ok");
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::RefutedInStandpoint);
    assert!(ans.witness.is_some());
}

// ── the ¬φ leg genuinely constructs the strong negation ──────────────────────

#[test]
fn negate_candidate_wraps_the_formula_in_strong_negation() {
    // The second leg of the symmetric test constructs ¬φ as logic: strong negation, and its
    // content identity is the alpha-normalized negation of φ's — never negation-as-failure.
    let phi = binary_atom(KNOWS, ALICE, BOB);
    let neg = negate_candidate(&phi);
    match &neg {
        Formula::Not(inner) => assert_eq!(
            **inner, phi,
            "¬φ must wrap the untouched candidate so its content_key negates φ's"
        ),
        other => panic!("negate_candidate must build a strong negation, got {other:?}"),
    }
}

// ── lifecycle / discharge enum round-trips ───────────────────────────────────

#[test]
fn lifecycle_and_discharge_local_names_round_trip() {
    for s in ConjectureLifecycleState::ALL {
        assert_eq!(
            ConjectureLifecycleState::from_local(s.local_name()),
            Some(*s)
        );
        assert_eq!(ConjectureLifecycleState::from_wire(s.wire()), Some(*s));
        assert!(s.iri().ends_with(s.local_name()));
    }
    for d in [
        ConjectureDischarge::Discharged,
        ConjectureDischarge::Unknown,
    ] {
        assert_eq!(ConjectureDischarge::from_local(d.local_name()), Some(d));
    }
    // The module.ttl individual names.
    assert_eq!(
        ConjectureLifecycleState::Corroborated.local_name(),
        "ConjectureCorroborated"
    );
    assert_eq!(
        ConjectureDischarge::Discharged.local_name(),
        "ObligationDischarged"
    );
}

// ── Rule-program candidate: the GOVERNED forward chase cuts it mid-flight ──────

#[test]
fn rule_program_candidate_step_budget_cuts_the_forward_chase_not_a_post_hoc_ceiling() {
    // A KB with a subclass chain gives the forward DL closure several committed derivations
    // (A⊑C, A⊑D, B⊑D, x:B, x:C, x:D). A NON-TRIVIAL formula candidate (a ∀-Horn implication)
    // takes the program path, which is now evaluated through the SAME forward-chase governor
    // as the ground path (`reason_program_budgeted`). A tiny max_steps must cut the chase
    // mid-flight and surface the non-conclusive budget-exhausted Open/Unknown lifecycle —
    // reported through the real governor status, never a post-hoc `derived_closure_size`
    // comparison after a full run.
    let store = kb(&[
        (A_CLS, SUBCLASS, B_CLS),
        (B_CLS, SUBCLASS, C_CLS),
        (C_CLS, SUBCLASS, D_CLS),
        (IND_X, TYPE, A_CLS),
    ]);
    // A formula (not a ground atom) → the rule-program branch of conjecture_test.
    let candidate = forall_horn(KNOWS, ALICE, TRUSTS, BOB);
    let budget = Budget {
        max_answers: None,
        max_steps: Some(1),
    };
    let ans = conjecture_test(&store, SCN, &candidate, STANDPOINT, &[], &budget)
        .expect("governed rule-program conjecture test");

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::BudgetExhausted,
        "the forward chase was cut mid-flight by the step governor"
    );
    assert_eq!(ans.verdict.information, InformationState::Undetermined);
    assert_eq!(ans.lifecycle, ConjectureLifecycleState::Open);
    assert_eq!(ans.discharge, ConjectureDischarge::Unknown);
    // The consumed budget is the governor's REAL committed-derivation count (≤ ceiling),
    // never the derived-closure-size fiction the deleted post-hoc ceiling used.
    assert!(
        ans.verdict.provenance.consumed_budget.consumed <= 1,
        "consumed steps ({}) must respect the max_steps ceiling",
        ans.verdict.provenance.consumed_budget.consumed
    );
}

#[test]
fn rule_program_candidate_with_ample_step_budget_completes_normally() {
    // The SAME formula candidate over a KB with no subclass chain has a trivially-small
    // forward closure, so a generous ceiling never trips the governor: the run completes and
    // the ∀-Horn's fired consequence corroborates it (identical to the ungoverned path).
    let store = kb(&[(SAM_P, KNOWS, ALICE)]);
    let candidate = forall_horn(KNOWS, ALICE, TRUSTS, BOB);
    let ans = conjecture_test(
        &store,
        SCN,
        &candidate,
        STANDPOINT,
        &[],
        &Budget {
            max_answers: None,
            max_steps: Some(1_000_000),
        },
    )
    .expect("governed rule-program conjecture test with an ample budget");

    assert_eq!(
        ans.verdict.evaluation,
        EvaluationStatus::Completed,
        "an ample ceiling never trips the governor"
    );
    assert_ne!(ans.verdict.information, InformationState::Undetermined);
}
