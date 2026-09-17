// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native equality and local reflexivity must wake authored consumers, with no
//! terminal DL pass or repository corpus capable of filling a missing result.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const SAME: &str = "http://www.w3.org/2002/07/owl#sameAs";
const DIFFERENT: &str = "https://blackcatinformatics.ca/logic/differentFrom";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const ON_PROPERTY: &str = "https://blackcatinformatics.ca/logic/onProperty";
const ON_CLASS: &str = "https://blackcatinformatics.ca/logic/onClass";
const MAX: &str = "https://blackcatinformatics.ca/logic/maxCardinality";
const QUALIFIED: &str = "https://blackcatinformatics.ca/logic/maxQualifiedCardinality";
const SELF: &str = "https://blackcatinformatics.ca/logic/hasSelf";
const P: &str = "urn:equality:p";
const S: &str = "urn:equality:subject";
const A: &str = "urn:equality:a";
const B: &str = "urn:equality:b";
const R: &str = "urn:equality:restriction";
const C: &str = "urn:equality:class";
const SEEN: &str = "urn:equality:seen";
const SEED: &str = "urn:equality:seed";
const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

fn fact(s: &str, p: &str, o: &str) -> RdfQuad {
    RdfQuad::new(RdfTerm::iri(s), p, RdfTerm::iri(o))
}

fn literal(s: &str, p: &str, text: &str, datatype: &str) -> RdfQuad {
    RdfQuad::new(
        RdfTerm::iri(s),
        p,
        RdfTerm::Literal(RdfLiteral::typed(text, datatype)),
    )
}

fn source(rows: &[RdfQuad]) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for row in rows {
        builder.push_owned_quad(row);
    }
    builder.freeze().unwrap()
}

fn copying(edges: &[(&str, &str)]) -> LogicProgram {
    let atom = |predicate: &str| {
        Formula::atom(
            Term::iri(predicate).unwrap(),
            vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
        )
        .unwrap()
    };
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(
        edges
            .iter()
            .map(|(from, to)| Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(Formula::Implies(Box::new(atom(from)), Box::new(atom(to)))),
            })
            .collect(),
    )
}

fn run(
    rows: &[RdfQuad],
    edges: &[(&str, &str)],
    budget: Option<u64>,
) -> gmeow_errors::Result<ProgramClosure> {
    let prepared = crate::program_analysis::prepare_program(&copying(edges))?;
    execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        budget,
    )
}

fn contains(result: &ProgramClosure, s: &str, p: &str, o: &str) -> bool {
    result
        .inferred
        .iter()
        .any(|row| row.subject == s && row.predicate == p && row.object == TermValue::iri(o))
}

fn characteristic(inverse: bool, record: bool) -> Vec<RdfQuad> {
    let marker = if inverse {
        "https://blackcatinformatics.ca/logic/inverseFunctionalProperty"
    } else {
        "https://blackcatinformatics.ca/logic/functionalProperty"
    };
    if record {
        vec![
            fact(R, "https://blackcatinformatics.ca/logic/characterizes", P),
            fact(
                R,
                "https://blackcatinformatics.ca/logic/characteristicSort",
                marker,
            ),
        ]
    } else {
        vec![fact(P, TYPE, marker)]
    }
}

#[test]
fn functional_equality_wakes_authored_consumers_and_retains_late_value_evidence() {
    for record in [false, true] {
        let mut rows = characteristic(false, record);
        rows.extend([fact(S, P, A), fact(S, SEED, B)]);
        let result = run(&rows, &[(SEED, P), (SAME, SEEN)], None).unwrap();
        assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
        assert!(contains(&result, A, SEEN, B));
        let equality = result
            .inferred
            .iter()
            .find(|row| {
                row.subject == A
                    && row.object == TermValue::iri(B)
                    && row.rule_name.as_deref() == Some("dl:functional-equality")
            })
            .unwrap();
        assert_eq!(equality.premises.len(), if record { 4 } else { 3 });
        for value in [A, B] {
            assert!(
                equality
                    .premises
                    .iter()
                    .any(|(s, p, o)| s == S && p == P && o.contains(value))
            );
        }
        assert!(equality.premises.iter().any(|(_, p, _)| p
            == if record {
                "https://blackcatinformatics.ca/logic/characteristicSort"
            } else {
                TYPE
            }));
    }
}

#[test]
fn inverse_functional_equality_uses_shared_literal_values_without_erasing_lexical_evidence() {
    for record in [false, true] {
        let mut rows = characteristic(true, record);
        rows.extend([
            literal(A, P, "01", INTEGER),
            literal(B, SEED, "1.0", "http://www.w3.org/2001/XMLSchema#decimal"),
            fact(B, DIFFERENT, A),
        ]);
        let result = run(
            &rows,
            &[(SEED, P), (SAME, SEEN), (TYPE, "urn:equality:types")],
            None,
        )
        .unwrap();
        assert!(contains(&result, A, SEEN, B));
        assert!(contains(&result, B, "urn:equality:types", NOTHING));
        let equality = result
            .inferred
            .iter()
            .find(|row| row.rule_name.as_deref() == Some("dl:inverse-functional-equality"))
            .unwrap();
        for lexical in ["\"01\"", "\"1.0\""] {
            assert!(
                equality
                    .premises
                    .iter()
                    .any(|(_, p, o)| p == P && o.contains(lexical))
            );
        }
    }
}

fn restriction(predicate: &str, count: &str) -> Vec<RdfQuad> {
    vec![
        literal(R, predicate, count, INTEGER),
        fact(R, ON_PROPERTY, P),
        fact(S, TYPE, R),
        fact(S, P, A),
        fact(S, P, B),
    ]
}

#[test]
fn maximum_one_equality_requires_both_qualifiers_and_keeps_every_premise() {
    let mut rows = restriction(QUALIFIED, "01");
    rows.extend([
        fact(R, ON_CLASS, C),
        fact(A, TYPE, C),
        fact(B, SEED, C),
        fact(S, P, "urn:equality:outside"),
    ]);
    let result = run(&rows, &[(SEED, TYPE), (SAME, SEEN)], None).unwrap();
    assert!(contains(&result, A, SEEN, B));
    assert!(
        !result.inferred.iter().any(
            |row| row.predicate == SAME && row.object == TermValue::iri("urn:equality:outside")
        )
    );
    let equality = result
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:maximum-one-equality"))
        .unwrap();
    assert_eq!(equality.premises.len(), 8);
    assert!(
        equality
            .premises
            .iter()
            .any(|(s, p, _)| s == B && p == TYPE)
    );
    assert!(
        equality
            .premises
            .iter()
            .any(|(s, p, _)| s == R && p == QUALIFIED)
    );
}

#[test]
fn missing_qualifier_or_larger_maximum_cannot_choose_an_equality() {
    for (predicate, count) in [(QUALIFIED, "1"), (MAX, "2")] {
        let mut rows = restriction(predicate, count);
        rows.push(fact(S, P, "urn:equality:third"));
        let result = run(&rows, &[(SAME, SEEN)], None).unwrap();
        assert!(
            !result
                .inferred
                .iter()
                .any(|row| row.predicate == SAME || row.predicate == SEEN)
        );
    }
}

#[test]
fn universal_qualified_and_unqualified_exact_one_need_no_invented_membership() {
    for predicate in [
        QUALIFIED,
        "https://blackcatinformatics.ca/logic/cardinality",
    ] {
        let mut rows = restriction(predicate, "1");
        if predicate == QUALIFIED {
            rows.push(fact(
                R,
                ON_CLASS,
                "https://blackcatinformatics.ca/logic/Thing",
            ));
        }
        let result = run(&rows, &[(SAME, SEEN)], None).unwrap();
        assert!(contains(&result, A, SEEN, B));
        let equality = result
            .inferred
            .iter()
            .find(|row| row.rule_name.as_deref() == Some("dl:maximum-one-equality"))
            .unwrap();
        assert_eq!(
            equality.premises.len(),
            if predicate == QUALIFIED { 6 } else { 5 }
        );
        assert!(
            !equality
                .premises
                .iter()
                .any(|(s, p, _)| (s == A || s == B) && p == TYPE)
        );
    }
}

#[test]
fn functional_literal_clashes_never_publish_resource_equalities() {
    let mut rows = characteristic(false, false);
    rows.extend([literal(S, P, "1", INTEGER), literal(S, P, "2", INTEGER)]);
    let result = run(&rows, &[(SAME, SEEN), (TYPE, "urn:equality:types")], None).unwrap();
    assert!(contains(&result, S, "urn:equality:types", NOTHING));
    assert!(!result.inferred.iter().any(|row| row.predicate == SAME));
}

#[test]
fn native_equality_refuses_unknown_datatype_comparison() {
    let mut rows = characteristic(true, false);
    rows.extend([
        literal(A, P, "one", "urn:equality:opaque"),
        literal(B, P, "uno", "urn:equality:opaque"),
    ]);
    let error = run(&rows, &[(SAME, SEEN)], None)
        .err()
        .expect("unknown datatype equality must refuse execution");
    assert!(
        error
            .to_string()
            .contains("undefined datatype value comparison")
    );
}

#[test]
fn shared_preparation_does_not_reuse_equality_across_worlds_or_inputs() {
    let prepared = crate::program_analysis::prepare_program(&copying(&[(SAME, SEEN)])).unwrap();
    let rows: Vec<_> = characteristic(false, false)
        .into_iter()
        .chain([fact(S, P, A), fact(S, P, B)])
        .collect();
    let first = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(contains(&first, A, SEEN, B));
    let mut split = rows.clone();
    split.last_mut().unwrap().graph_name = Some(RdfTerm::iri("urn:equality:other-world"));
    let second = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&split)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(
        !second
            .inferred
            .iter()
            .any(|row| row.predicate == SAME || row.predicate == SEEN)
    );
    let third = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(contains(&third, A, SEEN, B));
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(0),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!cut.inferred.iter().any(|row| row.predicate == SEEN));
}

#[test]
fn true_local_self_restrictions_feed_native_rules_in_both_directions() {
    for membership in [false, true] {
        for flag in ["true", "1"] {
            let rows = [
                literal(R, SELF, flag, BOOLEAN),
                fact(R, ON_PROPERTY, P),
                if membership {
                    fact(S, P, S)
                } else {
                    fact(S, TYPE, R)
                },
            ];
            let result = run(&rows, &[(P, SEEN), (TYPE, "urn:equality:types")], None).unwrap();
            assert!(contains(&result, S, SEEN, S));
            assert!(contains(&result, S, "urn:equality:types", R));
            let name = if membership {
                "dl:hasSelf-membership"
            } else {
                "dl:hasSelf-assertion"
            };
            let row = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some(name))
                .unwrap();
            assert_eq!(row.premises.len(), 3);
            assert!(
                row.premises
                    .iter()
                    .any(|(_, p, o)| p == SELF && o.contains(flag))
            );
        }
    }
}

#[test]
fn false_or_string_flags_and_nonself_edges_do_not_enable_local_reflexivity() {
    for (flag, datatype) in [
        ("false", BOOLEAN),
        ("0", BOOLEAN),
        ("true", "http://www.w3.org/2001/XMLSchema#string"),
    ] {
        let rows = [
            literal(R, SELF, flag, datatype),
            fact(R, ON_PROPERTY, P),
            fact(S, TYPE, R),
        ];
        let result = run(&rows, &[(P, SEEN)], None).unwrap();
        assert!(!contains(&result, S, SEEN, S));
    }
    let rows = [
        literal(R, SELF, "true", BOOLEAN),
        fact(R, ON_PROPERTY, P),
        fact(S, P, A),
    ];
    let result = run(&rows, &[(TYPE, SEEN)], None).unwrap();
    assert!(!contains(&result, S, SEEN, R));
}
