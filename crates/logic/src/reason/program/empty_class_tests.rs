// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Empty classes are positive schema consequences in the authored fixed point.
//! These tiny independent sources never invoke the terminal DL postpass or corpus.

use std::collections::BTreeSet;

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

mod bottom_property;
mod universal_class;

const C: &str = "urn:empty:class";
const D: &str = "urn:empty:superclass";
const R: &str = "urn:empty:restriction";
const P: &str = "urn:empty:property";
const S: &str = "urn:empty:subject";
const SEED: &str = "urn:empty:seed";
const SEEN: &str = "urn:empty:seen";
const CLASH: &str = "urn:empty:clash";
const ABSENT: &str = "urn:empty:absent";
const SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

fn vocabulary(canonical: bool, local: &str) -> String {
    let namespace = if canonical {
        "https://blackcatinformatics.ca/logic/"
    } else if local == "subClassOf" {
        "http://www.w3.org/2000/01/rdf-schema#"
    } else {
        "http://www.w3.org/2002/07/owl#"
    };
    format!("{namespace}{local}")
}

fn fact(subject: &str, predicate: &str, object: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
}

fn source(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}

fn atom(subject: &str, predicate: &str, object: &str) -> Formula {
    let term = |value: &str| match value.strip_prefix('?') {
        Some(name) => Term::var(name).unwrap(),
        None => Term::iri(value).unwrap(),
    };
    Formula::atom(
        Term::iri(predicate).unwrap(),
        vec![term(subject), term(object)],
    )
    .unwrap()
}

fn program(extra: Vec<(Formula, Formula)>) -> LogicProgram {
    let rules = extra.into_iter().chain([
        (atom("?x", SUBCLASS, NOTHING), atom("?x", SEEN, NOTHING)),
        (atom("?x", TYPE, NOTHING), atom("?x", CLASH, NOTHING)),
    ]);
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(
        rules
            .map(|(body, head)| Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
            })
            .collect(),
    )
}

fn run(rows: &[RdfQuad], extra: Vec<(Formula, Formula)>) -> gmeow_errors::Result<ProgramClosure> {
    let prepared = crate::program_analysis::prepare_program(&program(extra))?;
    execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
}

fn contains(result: &ProgramClosure, subject: &str, predicate: &str) -> bool {
    result.inferred.iter().any(|row| {
        row.subject == subject && row.predicate == predicate && row.object.as_iri() == Some(NOTHING)
    })
}

fn assert_premises(result: &ProgramClosure, subject: &str, rule: &str, rows: &[RdfQuad]) {
    let derived = result
        .inferred
        .iter()
        .find(|row| {
            row.subject == subject
                && row.predicate == SUBCLASS
                && row.object.as_iri() == Some(NOTHING)
                && row.rule_name.as_deref() == Some(rule)
        })
        .expect("the native schema law must publish its own evidence");
    let expected: BTreeSet<_> = rows
        .iter()
        .map(|row| {
            let RdfTerm::Iri(subject) = &row.subject else {
                panic!("synthetic premise subject is an IRI")
            };
            (
                subject.clone(),
                row.predicate.clone(),
                crate::provenance::term_display(&super::super::dataset::value(&row.object)),
            )
        })
        .collect();
    assert_eq!(derived.premises.len(), expected.len());
    assert_eq!(
        derived.premises.iter().cloned().collect::<BTreeSet<_>>(),
        expected
    );
    assert!(!derived.is_edb);
}

#[test]
fn self_disjointness_has_no_fabricated_reflexive_premise_or_individual() {
    for canonical in [false, true] {
        let rows = [fact(C, &vocabulary(canonical, "disjointWith"), C)];
        let result = run(&rows, Vec::new()).unwrap();
        assert!(contains(&result, C, SEEN));
        assert_premises(&result, C, "dl:self-disjoint-empty-class", &rows);
        assert!(result.witnesses.is_empty());
        assert!(!result.inferred.iter().any(|row| row.predicate == CLASH));
    }
}

#[test]
fn disjoint_superclass_emptiness_wakes_consumers_with_the_actual_source_orientation() {
    for canonical in [false, true] {
        for reversed in [false, true] {
            let disjoint = vocabulary(canonical, "disjointWith");
            let rows = [
                fact(C, &vocabulary(canonical, "subClassOf"), D),
                if reversed {
                    fact(D, &disjoint, C)
                } else {
                    fact(C, &disjoint, D)
                },
            ];
            let result = run(&rows, Vec::new()).unwrap();
            assert!(contains(&result, C, SEEN));
            assert_premises(&result, C, "dl:disjoint-superclass-empty-class", &rows);
            assert!(!result.inferred.iter().any(|row| row.predicate == CLASH));
        }
    }
}

#[test]
fn late_empty_existential_fillers_wake_class_and_instance_consumers_before_absence() {
    for canonical in [false, true] {
        let disjoint = vocabulary(canonical, "disjointWith");
        let some = vocabulary(canonical, "someValuesFrom");
        let on_property = vocabulary(canonical, "onProperty");
        let member = if canonical {
            "https://blackcatinformatics.ca/logic/instanceOf"
        } else {
            TYPE
        };
        let rows = [
            fact(C, SEED, C),
            fact(R, &on_property, P),
            fact(R, &some, C),
            fact(S, member, R),
        ];
        let result = run(
            &rows,
            vec![
                (atom("?x", SEED, "?y"), atom("?x", &disjoint, "?y")),
                (
                    Formula::And(vec![
                        atom("?x", member, R),
                        Formula::Not(Box::new(atom(R, SUBCLASS, NOTHING))),
                    ]),
                    atom("?x", ABSENT, NOTHING),
                ),
            ],
        )
        .unwrap();
        assert!(contains(&result, R, SEEN));
        assert!(contains(&result, S, CLASH));
        assert!(!result.inferred.iter().any(|row| row.predicate == ABSENT));
        assert_premises(
            &result,
            R,
            "dl:someValuesFrom-unsat-filler",
            &[rows[1].clone(), rows[2].clone(), fact(C, SUBCLASS, NOTHING)],
        );
    }
}

#[test]
fn direct_nothing_filler_needs_no_asserted_bottom_reflexivity() {
    for canonical in [false, true] {
        let rows = [
            fact(R, &vocabulary(canonical, "onProperty"), P),
            fact(
                R,
                &vocabulary(canonical, "someValuesFrom"),
                &vocabulary(canonical, "Nothing"),
            ),
        ];
        let result = run(&rows, Vec::new()).unwrap();
        assert!(contains(&result, R, SEEN));
        assert_premises(&result, R, "dl:someValuesFrom-unsat-filler", &rows);
    }
}

fn bound_rows(
    canonical: bool,
    predicate: &str,
    lexical: &str,
    datatype: &str,
    direct: bool,
) -> Vec<RdfQuad> {
    let filler = if direct {
        vocabulary(canonical, "Nothing")
    } else {
        C.to_owned()
    };
    let mut rows = vec![
        fact(R, &vocabulary(canonical, "onProperty"), P),
        RdfQuad::new(
            RdfTerm::iri(R),
            vocabulary(canonical, predicate),
            RdfTerm::Literal(RdfLiteral::typed(lexical, datatype)),
        ),
        fact(R, &vocabulary(canonical, "onClass"), &filler),
    ];
    if !direct {
        rows.push(fact(
            C,
            &vocabulary(canonical, "subClassOf"),
            &vocabulary(canonical, "Nothing"),
        ));
    }
    rows
}

#[test]
fn positive_qualified_bounds_keep_every_original_schema_premise() {
    for canonical in [false, true] {
        for predicate in ["minQualifiedCardinality", "qualifiedCardinality"] {
            for direct in [false, true] {
                let rows = bound_rows(canonical, predicate, "+0002", INTEGER, direct);
                let result = run(&rows, Vec::new()).unwrap();
                assert!(contains(&result, R, SEEN));
                assert_premises(&result, R, "dl:min-cardinality-unsat-filler", &rows);
                assert!(
                    result.witnesses.is_empty(),
                    "class emptiness does not populate a restriction"
                );
            }
        }
    }
}

#[test]
fn zero_unqualified_and_missing_object_qualifiers_cannot_force_empty_class() {
    for predicate in ["minQualifiedCardinality", "qualifiedCardinality"] {
        let zero = bound_rows(true, predicate, "0", INTEGER, true);
        assert!(!contains(&run(&zero, Vec::new()).unwrap(), R, SEEN));
        let mut no_class = bound_rows(true, predicate, "1", INTEGER, true);
        no_class.retain(|row| row.predicate != vocabulary(true, "onClass"));
        assert!(!contains(&run(&no_class, Vec::new()).unwrap(), R, SEEN));
    }
    for predicate in ["minCardinality", "cardinality"] {
        let rows = bound_rows(true, predicate, "1", INTEGER, true);
        assert!(
            !contains(&run(&rows, Vec::new()).unwrap(), R, SEEN),
            "onClass cannot qualify an unqualified bound"
        );
    }
}

#[test]
fn invalid_active_qualified_bounds_refuse_instead_of_becoming_zero() {
    for (lexical, datatype) in [
        ("-1", INTEGER),
        ("1.5", "http://www.w3.org/2001/XMLSchema#decimal"),
        ("one", "urn:empty:opaque-datatype"),
    ] {
        let rows = bound_rows(true, "minQualifiedCardinality", lexical, datatype, true);
        let error = run(&rows, Vec::new())
            .err()
            .expect("an invalid selected cardinality is not an absent obligation");
        assert!(error.to_string().contains("non-negative integer fields"));
    }
}

#[test]
fn repeated_bounds_remain_conjunctive_and_a_budget_cut_withholds_consumers() {
    let mut rows = bound_rows(true, "minQualifiedCardinality", "0", INTEGER, true);
    let positive = RdfQuad::new(
        RdfTerm::iri(R),
        vocabulary(true, "minQualifiedCardinality"),
        RdfTerm::Literal(RdfLiteral::typed("+0002", INTEGER)),
    );
    rows.push(positive.clone());
    let result = run(&rows, Vec::new()).unwrap();
    assert!(contains(&result, R, SEEN));
    assert_premises(
        &result,
        R,
        "dl:min-cardinality-unsat-filler",
        &[rows[0].clone(), positive, rows[2].clone()],
    );
    let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(0),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!contains(&cut, R, SEEN));
    assert!(!cut.frontier.saturated_preds.contains(SEEN));
}

#[test]
fn empty_filler_evidence_cannot_cross_source_worlds_or_cached_inputs() {
    // gmeow-test-input: synthetic-only
    let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
    let mut rows = bound_rows(true, "minQualifiedCardinality", "1", INTEGER, false);
    for row in &mut rows {
        row.graph_name = Some(RdfTerm::iri("urn:empty:world-a"));
    }
    let together = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(contains(&together, R, SEEN));
    assert!(
        together
            .inferred
            .iter()
            .filter(|row| row.predicate == SEEN)
            .all(|row| row.world == "urn:empty:world-a")
    );
    rows.last_mut().unwrap().graph_name = Some(RdfTerm::iri("urn:empty:world-b"));
    let split = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(!contains(&split, R, SEEN));
}
