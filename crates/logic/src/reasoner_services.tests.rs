// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDatasetBuilder;

/// Reserved vocabulary the fixtures below assert over.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASSOF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const CAT: &str = "http://example.org/Cat";
const DOG: &str = "http://example.org/Dog";
const ANIMAL: &str = "http://example.org/Animal";
const TOM: &str = "http://example.org/tom";

/// A tiny, consistent A-Box: `Cat ⊑ Animal`, `tom a Cat`.
fn consistent_dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let cat = builder.intern_iri(CAT);
    let animal = builder.intern_iri(ANIMAL);
    let tom = builder.intern_iri(TOM);
    let sub = builder.intern_iri(RDFS_SUBCLASSOF);
    let ty = builder.intern_iri(RDF_TYPE);
    builder.push_quad(cat, sub, animal, None);
    builder.push_quad(tom, ty, cat, None);
    builder.freeze().expect("freeze the consistent fixture")
}

/// A tiny, INCONSISTENT A-Box: `Cat` and `Dog` are disjoint, yet `tom` is both.
fn inconsistent_dataset() -> std::sync::Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    let cat = builder.intern_iri(CAT);
    let dog = builder.intern_iri(DOG);
    let tom = builder.intern_iri(TOM);
    let disjoint = builder.intern_iri(OWL_DISJOINT_WITH);
    let ty = builder.intern_iri(RDF_TYPE);
    builder.push_quad(cat, disjoint, dog, None);
    builder.push_quad(tom, ty, cat, None);
    builder.push_quad(tom, ty, dog, None);
    builder.freeze().expect("freeze the inconsistent fixture")
}

#[test]
fn consistency_true_on_a_satisfiable_abox() {
    let dataset = consistent_dataset();
    let reasoner = DlReasoner::new(&dataset).expect("reverse-map the consistent ontology");
    let certified = reasoner.consistency();
    assert_eq!(certified.answer, Verdict::True);
    // The whole ontology was read and every search finished: an exact answer.
    assert!(certified.is_decided());
    assert!(certified.is_exact());
    assert!(certified.boundaries.is_empty());
}

#[test]
fn consistency_false_on_a_disjointness_violation() {
    let dataset = inconsistent_dataset();
    let reasoner = DlReasoner::new(&dataset).expect("reverse-map the inconsistent ontology");
    let certified = reasoner.consistency();
    // Detected, not errored: consistency is the one service that reports an
    // unsatisfiable ontology as a verdict rather than as a refusal.
    assert_eq!(certified.answer, Verdict::False);
    assert!(certified.is_decided());
}

#[test]
fn entailed_subsumption_is_certified_true() {
    let dataset = consistent_dataset();
    let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
    let axiom = DlAxiom::ClassAssertion {
        individual: TermValue::iri(TOM),
        class: TermValue::iri(ANIMAL),
    };
    // `tom a Animal` is not asserted but IS entailed through `Cat ⊑ Animal`.
    let certified = reasoner.entails(&axiom).expect("consistent ontology");
    assert_eq!(certified.answer, Verdict::True);
    assert!(certified.is_exact());
}

#[test]
fn class_satisfiability_on_the_unsatisfiable_ontology_is_an_error() {
    let dataset = inconsistent_dataset();
    let mut reasoner = DlReasoner::new(&dataset).expect("reverse-map");
    // Every class is vacuously unsatisfiable in an ontology with no model, so
    // the service refuses rather than answering, retaining the no-model category.
    let error = reasoner
        .class_satisfiability(&TermValue::iri(CAT))
        .expect_err("an unsatisfiable ontology has no meaningful class answer");
    assert_eq!(error.kind(), DlServiceFailureKind::NoModel);
    assert!(error.detail().contains("unsatisfiable"), "{error}");
}

#[test]
fn a_narrowed_step_cap_reports_unknown_rather_than_a_fabricated_verdict() {
    let dataset = consistent_dataset();

    // Under the size-derived ceiling the same ontology is DECIDED exactly: this is
    // the control that makes the exhausted arm below falsifiable — the ontology is
    // trivially consistent, so a non-conclusion can only come from the ceiling, not
    // from the input being genuinely undecidable.
    let decided = DlReasoner::new(&dataset)
        .expect("reverse-map the consistent ontology")
        .consistency();
    assert_eq!(decided.answer, Verdict::True);
    assert!(decided.is_exact());
    assert_eq!(decided.completeness, DlCompleteness::Decided);

    // Now narrow the per-decision step cap to one round — the one ceiling this
    // repository can move, and only downward. One round decides nothing, so the
    // hypertableau search must exhaust.
    let starved =
        DlReasoner::with_step_cap(&dataset, 1).expect("reverse-map under a narrowed step cap");
    assert_eq!(starved.step_cap(), 1, "the cap was narrowed to one round");

    let answer = starved.consistency();

    // The honest contract: exhaustion is a NON-CONCLUSION, never a fabricated
    // verdict and never a panic. A boolean service reports `Unknown`, the
    // certificate reports `BudgetExhausted`, and both completeness predicates read
    // `false` — the answer is not presented as if it were decided or exact.
    assert_eq!(
        answer.answer,
        Verdict::Unknown,
        "an exhausted search is Unknown, never True/False as if decided"
    );
    assert_ne!(answer.answer, Verdict::True);
    assert_ne!(answer.answer, Verdict::False);
    assert_eq!(answer.completeness, DlCompleteness::BudgetExhausted);
    assert!(!answer.is_decided());
    assert!(!answer.is_exact());
}
