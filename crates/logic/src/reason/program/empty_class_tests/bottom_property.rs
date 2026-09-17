// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Empty-property obligations and source evidence on the shared native closure.

use super::*;

const OBJECT_BOTTOM: &str = "http://www.w3.org/2002/07/owl#bottomObjectProperty";
const DATA_BOTTOM: &str = "http://www.w3.org/2002/07/owl#bottomDataProperty";
const EMPTY_RULE: &str = "dl:bottom-property-empty-class";

fn count_source(canonical: bool, property: &str, predicate: &str, count: &str) -> Vec<RdfQuad> {
    let mut rows = vec![
        fact(R, &vocabulary(canonical, "onProperty"), property),
        RdfQuad::new(
            RdfTerm::iri(R),
            vocabulary(canonical, predicate),
            RdfTerm::Literal(RdfLiteral::typed(count, INTEGER)),
        ),
    ];
    if predicate.contains("Qualified") || predicate == "qualifiedCardinality" {
        let (qualifier, target) = if property == DATA_BOTTOM {
            ("onDataRange", INTEGER)
        } else {
            ("onClass", C)
        };
        rows.push(fact(R, &vocabulary(canonical, qualifier), target));
    }
    rows
}

#[test]
fn asserted_empty_property_values_wake_authored_consumers_with_exact_evidence() {
    for (property, object) in [
        (OBJECT_BOTTOM, RdfTerm::iri(C)),
        (
            DATA_BOTTOM,
            RdfTerm::Literal(RdfLiteral::typed("+0002", INTEGER)),
        ),
    ] {
        let row = RdfQuad::new(RdfTerm::iri(S), property, object);
        let result = run(std::slice::from_ref(&row), Vec::new()).unwrap();
        assert!(contains(&result, S, CLASH));
        let contradiction = result
            .inferred
            .iter()
            .find(|derived| {
                derived.subject == S
                    && derived.predicate == TYPE
                    && derived.rule_name.as_deref() == Some("dl:bottom-property-clash")
            })
            .unwrap();
        assert_eq!(
            contradiction.premises,
            vec![(
                S.to_owned(),
                property.to_owned(),
                crate::provenance::term_display(&super::super::super::dataset::value(&row.object)),
            )]
        );
    }
}

#[test]
fn positive_restriction_bounds_retain_every_defining_field_without_population() {
    for canonical in [false, true] {
        for property in [OBJECT_BOTTOM, DATA_BOTTOM] {
            for predicate in [
                "minCardinality",
                "cardinality",
                "minQualifiedCardinality",
                "qualifiedCardinality",
            ] {
                let rows = count_source(canonical, property, predicate, "+0002");
                let result = run(&rows, Vec::new()).unwrap();
                assert!(contains(&result, R, SEEN));
                assert_premises(&result, R, EMPTY_RULE, &rows);
                assert!(result.witnesses.is_empty());
                assert!(!result.inferred.iter().any(|row| row.predicate == CLASH));
            }
        }
    }
}

#[test]
fn late_restriction_emptiness_reaches_authored_rules_before_negation() {
    for canonical in [false, true] {
        for property in [OBJECT_BOTTOM, DATA_BOTTOM] {
            let some = vocabulary(canonical, "someValuesFrom");
            let rows = [
                fact(R, &vocabulary(canonical, "onProperty"), property),
                fact(R, SEED, C),
            ];
            let result = run(
                &rows,
                vec![
                    (atom("?x", SEED, "?y"), atom("?x", &some, "?y")),
                    (
                        Formula::And(vec![
                            atom("?x", SEED, "?y"),
                            Formula::Not(Box::new(atom("?x", SUBCLASS, NOTHING))),
                        ]),
                        atom("?x", ABSENT, NOTHING),
                    ),
                ],
            )
            .unwrap();
            assert!(contains(&result, R, SEEN));
            assert!(!contains(&result, R, ABSENT));
            assert_premises(
                &result,
                R,
                EMPTY_RULE,
                &[rows[0].clone(), fact(R, &some, C)],
            );
        }
    }
}

#[test]
fn has_value_empty_restrictions_preserve_literal_fields() {
    for canonical in [false, true] {
        for property in [OBJECT_BOTTOM, DATA_BOTTOM] {
            let value = if property == DATA_BOTTOM {
                RdfTerm::Literal(RdfLiteral::typed("+0002", INTEGER))
            } else {
                RdfTerm::iri(C)
            };
            let rows = [
                fact(R, &vocabulary(canonical, "onProperty"), property),
                RdfQuad::new(RdfTerm::iri(R), vocabulary(canonical, "hasValue"), value),
            ];
            let result = run(&rows, Vec::new()).unwrap();
            assert!(contains(&result, R, SEEN));
            assert_premises(&result, R, EMPTY_RULE, &rows);
        }
    }
}

#[test]
fn zero_and_unselected_qualifiers_do_not_assert_emptiness() {
    for property in [OBJECT_BOTTOM, DATA_BOTTOM] {
        for predicate in [
            "minCardinality",
            "cardinality",
            "minQualifiedCardinality",
            "qualifiedCardinality",
        ] {
            let rows = count_source(true, property, predicate, "0");
            assert!(!contains(&run(&rows, Vec::new()).unwrap(), R, SEEN));
        }
        for predicate in ["minQualifiedCardinality", "qualifiedCardinality"] {
            let mut rows = count_source(true, property, predicate, "1");
            rows.truncate(2);
            assert!(!contains(&run(&rows, Vec::new()).unwrap(), R, SEEN));
        }
        let rows = count_source(true, property, "maxQualifiedCardinality", "1");
        assert!(!contains(&run(&rows, Vec::new()).unwrap(), R, SEEN));
    }
}

#[test]
fn invalid_positive_obligation_fields_fail_instead_of_becoming_absent() {
    for property in [OBJECT_BOTTOM, DATA_BOTTOM] {
        for count in ["-1", "1.5", "unknown"] {
            let rows = count_source(true, property, "minCardinality", count);
            let error = run(&rows, Vec::new())
                .err()
                .expect("invalid source count must fail");
            assert!(error.to_string().contains("non-negative integer fields"));
        }
    }
}

#[test]
fn repeated_bounds_do_not_overwrite_the_positive_obligation() {
    let mut rows = count_source(true, OBJECT_BOTTOM, "minCardinality", "0");
    let positive = count_source(true, OBJECT_BOTTOM, "minCardinality", "2");
    rows.push(positive[1].clone());
    let result = run(&rows, Vec::new()).unwrap();
    assert!(contains(&result, R, SEEN));
    assert_premises(&result, R, EMPTY_RULE, &positive);
}

#[test]
fn source_worlds_budget_and_property_identity_bound_the_consequence() {
    let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
    let mut rows = count_source(true, OBJECT_BOTTOM, "minCardinality", "1");
    for row in &mut rows {
        row.graph_name = Some(RdfTerm::iri("urn:bottom:world-a"));
    }
    let complete = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap();
    assert!(contains(&complete, R, SEEN));
    assert!(
        complete
            .inferred
            .iter()
            .filter(|row| row.predicate == SEEN)
            .all(|row| row.world == "urn:bottom:world-a")
    );
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(0),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!contains(&cut, R, SEEN));
    rows[1].graph_name = Some(RdfTerm::iri("urn:bottom:world-b"));
    assert!(!contains(
        &execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source(&rows)).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None
        )
        .unwrap(),
        R,
        SEEN
    ));
    let mut unknown = count_source(
        true,
        "https://blackcatinformatics.ca/logic/bottomObjectProperty",
        "minCardinality",
        "1",
    );
    unknown.push(fact(
        R,
        "https://blackcatinformatics.ca/logic/sourceEndpoint",
        OBJECT_BOTTOM,
    ));
    assert!(
        !contains(&run(&unknown, Vec::new()).unwrap(), R, SEEN),
        "an undeclared spelling or data endpoint cannot select the empty property"
    );
}
