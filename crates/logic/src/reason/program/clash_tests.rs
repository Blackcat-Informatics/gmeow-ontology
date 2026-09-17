// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native DL contradictions must wake authored rules before any terminal DL pass.

use super::*;
use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
use purrdf::{RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const NOTHING: &str = "http://www.w3.org/2002/07/owl#Nothing";
const DIFFERENT: &str = "http://www.w3.org/2002/07/owl#differentFrom";
const SAME: &str = "http://www.w3.org/2002/07/owl#sameAs";
const DISJOINT: &str = "http://www.w3.org/2002/07/owl#propertyDisjointWith";
const FUNCTIONAL: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";
const P: &str = "urn:clash:p";
const Q: &str = "urn:clash:q";
const SUBJECT: &str = "urn:clash:subject";
const DONE: &str = "urn:clash:done";
const HAS_KEY: &str = "http://www.w3.org/2002/07/owl#hasKey";
const KEY_CLASS: &str = "https://blackcatinformatics.ca/logic/keyClass";
const KEY_PROPERTY: &str = "https://blackcatinformatics.ca/logic/keyProperty";
const KEY_ASSERTION: &str = "https://blackcatinformatics.ca/logic/KeyAssertion";
const CLASS: &str = "urn:key:class";
const RECORD: &str = "urn:key:record";
const OTHER: &str = "urn:key:other";

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

fn source(quads: Vec<RdfQuad>) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();
    for quad in quads {
        builder.push_owned_quad(&quad);
    }
    builder.freeze().unwrap()
}

fn atom(s: &str, p: &str, o: &str) -> Formula {
    let term = |value: &str| match value.strip_prefix('?') {
        Some(name) => Term::var(name).unwrap(),
        None => Term::iri(value).unwrap(),
    };
    Formula::atom(Term::iri(p).unwrap(), vec![term(s), term(o)]).unwrap()
}

fn program(extra: Vec<(Formula, Formula)>) -> LogicProgram {
    let rules = extra
        .into_iter()
        .chain([(atom("?x", TYPE, NOTHING), atom("?x", DONE, NOTHING))]);
    LogicProgram::new(vec![], vec![], vec![], None).with_formulas(
        rules
            .map(|(body, head)| Formula::Forall {
                vars: vec!["x".to_owned(), "y".to_owned()],
                body: Box::new(Formula::Implies(Box::new(body), Box::new(head))),
            })
            .collect(),
    )
}

fn run(program: &LogicProgram, data: &Arc<RdfDataset>) -> ProgramClosure {
    let prepared = crate::program_analysis::prepare_program(program).unwrap();
    // This is the shared physical closure itself. A later DL augmentation cannot
    // make this test pass or supply the authored consumer's missing consequence.
    execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .unwrap()
}

fn key_source(canonical: bool) -> Vec<RdfQuad> {
    let mut quads = if canonical {
        vec![
            fact(RECORD, TYPE, KEY_ASSERTION),
            fact(RECORD, KEY_CLASS, CLASS),
            fact(RECORD, KEY_PROPERTY, P),
        ]
    } else {
        vec![
            fact(CLASS, HAS_KEY, RECORD),
            fact(
                RECORD,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#first",
                P,
            ),
            fact(
                RECORD,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest",
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil",
            ),
        ]
    };
    quads.extend([
        fact(SUBJECT, TYPE, CLASS),
        fact(OTHER, TYPE, CLASS),
        fact(SUBJECT, DIFFERENT, OTHER),
        literal(SUBJECT, P, "01", "http://www.w3.org/2001/XMLSchema#integer"),
    ]);
    quads
}

#[test]
fn key_values_join_late_native_values_with_actual_definition_and_value_evidence() {
    for canonical in [false, true] {
        let mut quads = key_source(canonical);
        quads.push(literal(
            OTHER,
            "urn:seed",
            "1.0",
            "http://www.w3.org/2001/XMLSchema#decimal",
        ));
        let data = source(quads);
        let p = program(vec![(atom("?x", "urn:seed", "?y"), atom("?x", P, "?y"))]);
        let result = run(&p, &data);
        assert!(
            result
                .inferred
                .iter()
                .any(|row| row.subject == SUBJECT && row.predicate == DONE)
        );
        let clash = result
            .inferred
            .iter()
            .find(|row| row.rule_name.as_deref() == Some("dl:has-key-clash"))
            .unwrap();
        assert_eq!(clash.premises.len(), 8);
        assert!(
            clash
                .premises
                .iter()
                .any(|(s, p, o)| s == OTHER && p == P && o.contains("\"1.0\""))
        );
        assert!(
            clash
                .premises
                .iter()
                .any(|(s, p, o)| s == SUBJECT && p == P && o.contains("\"01\""))
        );
        assert!(
            clash
                .premises
                .iter()
                .any(|(_, p, _)| p == if canonical { KEY_PROPERTY } else { HAS_KEY })
        );
        let prepared = crate::program_analysis::prepare_program(&p).unwrap();
        let cut = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&data).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            Some(0),
        )
        .unwrap();
        assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
        assert!(!cut.inferred.iter().any(|row| row.predicate == DONE));
    }
}

#[test]
fn canonical_composite_keys_wait_for_late_definition_writers() {
    for agrees in [false, true] {
        let mut quads = key_source(true);
        quads.extend([
            literal(OTHER, P, "1.0", "http://www.w3.org/2001/XMLSchema#decimal"),
            fact(RECORD, "urn:key:seed", Q),
            fact(SUBJECT, Q, "urn:key:shared"),
            fact(
                OTHER,
                Q,
                if agrees {
                    "urn:key:shared"
                } else {
                    "urn:key:different"
                },
            ),
        ]);
        let p = program(vec![
            (
                atom("?x", "urn:key:seed", "?y"),
                atom("?x", "urn:key:staged", "?y"),
            ),
            (
                atom("?x", "urn:key:staged", "?y"),
                atom("?x", KEY_PROPERTY, "?y"),
            ),
        ]);
        let result = run(&p, &source(quads));
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            agrees
        );
        if agrees {
            let clash = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:has-key-clash"))
                .unwrap();
            assert_eq!(clash.premises.len(), 11);
            assert!(
                clash
                    .premises
                    .iter()
                    .any(|(s, p, o)| s == RECORD && p == KEY_PROPERTY && o == "<urn:clash:q>")
            );
        }
    }
}

#[test]
fn key_agreement_needs_shared_values_distinctness_and_one_world() {
    for canonical in [false, true] {
        for case in ["absent", "different", "unrelated-world", "no-distinctness"] {
            let mut quads = key_source(canonical);
            if case == "no-distinctness" {
                quads.retain(|quad| quad.predicate.as_str() != DIFFERENT);
            }
            if case != "absent" {
                let value = literal(
                    OTHER,
                    P,
                    if case == "different" { "2" } else { "1" },
                    "http://www.w3.org/2001/XMLSchema#integer",
                );
                quads.push(if case == "unrelated-world" {
                    value.in_graph(RdfTerm::iri("urn:isolated"))
                } else {
                    value
                });
            }
            assert!(
                !run(&program(Vec::new()), &source(quads))
                    .inferred
                    .iter()
                    .any(|row| row.predicate == DONE),
                "{case}"
            );
        }
    }
}

#[test]
fn key_definition_feedback_cycle_is_refused_before_any_clash() {
    let mut quads = key_source(true);
    quads.push(literal(
        OTHER,
        P,
        "1",
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    let p = program(vec![(
        atom("?x", DONE, "?y"),
        atom(RECORD, KEY_PROPERTY, Q),
    )]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let error = match execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    ) {
        Ok(_) => panic!("a key cannot justify a producer that strengthens its own definition"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("NonStratifiable"), "{error}");
}

#[test]
fn malformed_or_undefined_keys_cannot_certify_a_complete_native_result() {
    for defect in [
        "two-classes",
        "no-property",
        "literal-property",
        "undefined-value",
    ] {
        let mut quads = key_source(true);
        if defect == "no-property" || defect == "literal-property" {
            quads.retain(|quad| quad.predicate.as_str() != KEY_PROPERTY);
        }
        match defect {
            "two-classes" => quads.push(fact(RECORD, KEY_CLASS, "urn:another-class")),
            "literal-property" => quads.push(literal(
                RECORD,
                KEY_PROPERTY,
                "bad",
                "http://www.w3.org/2001/XMLSchema#string",
            )),
            "undefined-value" => quads.push(literal(OTHER, P, "opaque", "urn:uninterpreted")),
            _ => {}
        }
        let p = program(Vec::new());
        let prepared = crate::program_analysis::prepare_program(&p).unwrap();
        assert!(
            execute(
                &prepared,
                crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
                &crate::physical::SelectedDomains::new([]).unwrap(),
                None
            )
            .is_err(),
            "{defect}"
        );
    }
}

#[test]
fn universal_keys_need_no_explicit_thing_memberships() {
    for canonical in [false, true] {
        let mut quads = key_source(canonical);
        quads
            .retain(|quad| quad.predicate.as_str() != TYPE || quad.subject == RdfTerm::iri(RECORD));
        quads.retain(|quad| {
            quad.predicate.as_str() != KEY_CLASS && quad.predicate.as_str() != HAS_KEY
        });
        let thing = "http://www.w3.org/2002/07/owl#Thing";
        quads.push(if canonical {
            fact(RECORD, KEY_CLASS, thing)
        } else {
            fact(thing, HAS_KEY, RECORD)
        });
        quads.push(literal(
            OTHER,
            P,
            "1",
            "http://www.w3.org/2001/XMLSchema#integer",
        ));
        assert!(
            run(&program(Vec::new()), &source(quads))
                .inferred
                .iter()
                .any(|row| row.predicate == DONE)
        );
    }
}

#[test]
fn late_negative_assertion_values_wake_consumers_with_exact_worlds_and_premises() {
    let p = program(vec![(atom("?x", "urn:seed", "?y"), atom("?x", P, "?y"))]);
    let world = "urn:clash:world";
    let data = source(
        vec![
            fact(
                "urn:npa",
                "http://www.w3.org/2002/07/owl#sourceIndividual",
                SUBJECT,
            ),
            fact(
                "urn:npa",
                "http://www.w3.org/2002/07/owl#assertionProperty",
                P,
            ),
            literal(
                "urn:npa",
                "http://www.w3.org/2002/07/owl#targetValue",
                "1.0",
                "http://www.w3.org/2001/XMLSchema#decimal",
            ),
            literal(
                SUBJECT,
                "urn:seed",
                "01",
                "http://www.w3.org/2001/XMLSchema#integer",
            ),
        ]
        .into_iter()
        .map(|quad| quad.in_graph(RdfTerm::iri(world)))
        .chain([literal(
            SUBJECT,
            "urn:seed",
            "01",
            "http://www.w3.org/2001/XMLSchema#integer",
        )
        .in_graph(RdfTerm::iri("urn:other-world"))])
        .collect(),
    );
    let result = run(&p, &data);
    let done: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == DONE)
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].world, world);
    let clash = result
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:negative-property-assertion-clash"))
        .unwrap();
    assert_eq!(clash.premises.len(), 4);
    assert!(
        clash
            .premises
            .iter()
            .any(|(s, p, o)| s == SUBJECT && p == P && o.contains("\"01\""))
    );
    assert!(
        clash
            .premises
            .iter()
            .any(|(s, _, o)| s == "urn:npa" && o.contains("\"1.0\""))
    );
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let cut = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        Some(0),
    )
    .unwrap();
    assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
    assert!(!cut.inferred.iter().any(|row| row.predicate == DONE));
}

#[test]
fn disjoint_properties_compare_values_and_feed_authored_rules() {
    for (right, expected) in [("1.0", true), ("2.0", false)] {
        let p = program(vec![(atom("?x", "urn:seed", "?y"), atom("?x", Q, "?y"))]);
        let data = source(vec![
            fact(P, DISJOINT, Q),
            literal(SUBJECT, P, "01", "http://www.w3.org/2001/XMLSchema#integer"),
            literal(
                SUBJECT,
                "urn:seed",
                right,
                "http://www.w3.org/2001/XMLSchema#decimal",
            ),
        ]);
        let result = run(&p, &data);
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            expected
        );
        if expected {
            let clash = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:property-disjoint-clash"))
                .unwrap();
            assert_eq!(clash.premises.len(), 3);
        }
    }
}

#[test]
fn functionality_uses_canonical_records_and_requires_resource_distinctness() {
    for explicit in [false, true] {
        let p = program(vec![(
            atom("?x", "urn:distinct", "?y"),
            atom("?x", DIFFERENT, "?y"),
        )]);
        let mut quads = vec![
            fact(
                "urn:characteristic",
                "https://blackcatinformatics.ca/logic/characterizes",
                P,
            ),
            fact(
                "urn:characteristic",
                "https://blackcatinformatics.ca/logic/characteristicSort",
                "https://blackcatinformatics.ca/logic/functionalProperty",
            ),
            fact(SUBJECT, P, "urn:first"),
            fact(SUBJECT, P, "urn:second"),
        ];
        if explicit {
            quads.push(fact("urn:second", "urn:distinct", "urn:first"));
        }
        let result = run(&p, &source(quads));
        assert_eq!(
            result
                .inferred
                .iter()
                .any(|row| row.subject == SUBJECT && row.predicate == DONE),
            explicit
        );
        if explicit {
            let clash = result
                .inferred
                .iter()
                .find(|row| {
                    row.subject == SUBJECT
                        && row.rule_name.as_deref() == Some("dl:functional-property-clash")
                })
                .unwrap();
            assert_eq!(clash.premises.len(), 5);
            assert!(
                clash
                    .premises
                    .iter()
                    .any(|(s, p, o)| s == "urn:second" && p == DIFFERENT && o == "<urn:first>")
            );
        }
    }
    for (second, expected) in [("1.0", false), ("2.0", true)] {
        let result = run(
            &program(Vec::new()),
            &source(vec![
                fact(P, TYPE, FUNCTIONAL),
                literal(SUBJECT, P, "01", "http://www.w3.org/2001/XMLSchema#integer"),
                literal(
                    SUBJECT,
                    P,
                    second,
                    "http://www.w3.org/2001/XMLSchema#decimal",
                ),
            ]),
        );
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            expected
        );
    }
}

#[test]
fn structural_property_and_identity_clashes_join_the_same_round_graph() {
    let cases = [
        vec![
            fact(P, TYPE, "http://www.w3.org/2002/07/owl#AsymmetricProperty"),
            fact(SUBJECT, P, "urn:other"),
            fact("urn:other", P, SUBJECT),
        ],
        vec![
            fact(P, TYPE, "http://www.w3.org/2002/07/owl#IrreflexiveProperty"),
            fact(SUBJECT, P, SUBJECT),
        ],
        vec![
            fact(SUBJECT, SAME, "urn:middle"),
            fact("urn:other", SAME, "urn:middle"),
            fact(SUBJECT, DIFFERENT, "urn:other"),
        ],
        vec![fact(SUBJECT, DIFFERENT, SUBJECT)],
    ];
    for quads in cases {
        let result = run(&program(Vec::new()), &source(quads));
        assert!(
            result
                .inferred
                .iter()
                .any(|row| row.subject == SUBJECT && row.predicate == DONE)
        );
    }
}

#[test]
fn undefined_value_comparison_cannot_become_a_complete_native_result() {
    let p = program(Vec::new());
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let data = source(vec![
        fact(P, DISJOINT, Q),
        literal(SUBJECT, P, "first", "urn:uninterpreted-datatype"),
        literal(SUBJECT, Q, "second", "urn:uninterpreted-datatype"),
    ]);
    let error = match execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&data).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    ) {
        Ok(_) => panic!("unknown values cannot silently decide the selected law"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("undefined datatype value comparison")
    );
}

const MAXIMUM: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const QUALIFIED_MAXIMUM: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

fn maximum_source(predicate: &str, count: &str, datatype: &str) -> Vec<RdfQuad> {
    vec![
        fact(SUBJECT, TYPE, RECORD),
        fact(RECORD, "http://www.w3.org/2002/07/owl#onProperty", P),
        literal(RECORD, predicate, count, datatype),
    ]
}

#[test]
fn native_maximum_waits_for_explicit_resource_inequality_and_feeds_consumers() {
    for explicit in [false, true] {
        let mut quads = maximum_source(MAXIMUM, "1", INTEGER);
        quads.extend([fact(SUBJECT, P, "urn:a"), fact(SUBJECT, P, "urn:b")]);
        if explicit {
            quads.push(fact("urn:b", "urn:distinct-seed", "urn:a"));
        }
        let p = program(vec![(
            atom("?x", "urn:distinct-seed", "?y"),
            atom("?x", DIFFERENT, "?y"),
        )]);
        let data = source(quads);
        let result = run(&p, &data);
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            explicit
        );
        if explicit {
            let clash = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
                .unwrap();
            assert_eq!(clash.premises.len(), 6);
            assert!(
                clash
                    .premises
                    .iter()
                    .any(|(s, p, o)| s == "urn:b" && p == DIFFERENT && o == "<urn:a>")
            );
            let prepared = crate::program_analysis::prepare_program(&p).unwrap();
            let cut = execute(
                &prepared,
                crate::reason::program::prepare_reasoning_input(&data).unwrap(),
                &crate::physical::SelectedDomains::new([]).unwrap(),
                Some(0),
            )
            .unwrap();
            assert_eq!(cut.status, crate::seam::BudgetStatus::Exhausted);
            assert!(!cut.inferred.iter().any(|row| row.predicate == DONE));
        }
    }
}

#[test]
fn qualified_maximum_counts_late_membership_in_its_own_world_only() {
    for main_world in [false, true] {
        let mut quads = maximum_source(QUALIFIED_MAXIMUM, "1", INTEGER);
        quads.extend([
            fact(RECORD, ON_CLASS, CLASS),
            fact(SUBJECT, P, "urn:a"),
            fact(SUBJECT, P, "urn:b"),
            fact(SUBJECT, P, "urn:untyped"),
            fact("urn:a", TYPE, CLASS),
            fact("urn:a", DIFFERENT, "urn:b"),
        ]);
        let seed = fact("urn:b", "urn:type-seed", CLASS);
        quads.push(if main_world {
            seed
        } else {
            seed.in_graph(RdfTerm::iri("urn:other-world"))
        });
        let result = run(
            &program(vec![(
                atom("?x", "urn:type-seed", "?y"),
                atom("?x", TYPE, "?y"),
            )]),
            &source(quads),
        );
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            main_world
        );
        if main_world {
            let clash = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
                .unwrap();
            assert_eq!(clash.premises.len(), 9);
            assert!(
                clash
                    .premises
                    .iter()
                    .any(|(s, p, o)| s == "urn:b" && p == TYPE && o == "<urn:key:class>")
            );
            assert!(!clash.premises.iter().any(|(_, _, o)| o == "<urn:untyped>"));
        }
    }
}

#[test]
fn native_cardinality_uses_literal_values_and_each_asserted_upper_bound() {
    for (second, clashes) in [("1.0", false), ("2.0", true)] {
        let mut quads = maximum_source(MAXIMUM, "1", INTEGER);
        quads.extend([
            literal(SUBJECT, P, "01", INTEGER),
            literal(SUBJECT, P, second, DECIMAL),
        ]);
        assert_eq!(
            run(&program(Vec::new()), &source(quads))
                .inferred
                .iter()
                .any(|row| row.predicate == DONE),
            clashes
        );
    }
    for predicate in [MAXIMUM, "http://www.w3.org/2002/07/owl#cardinality"] {
        let mut quads = maximum_source(predicate, "0", INTEGER);
        quads.extend([
            literal(RECORD, predicate, "10", INTEGER),
            literal(SUBJECT, P, "one", "http://www.w3.org/2001/XMLSchema#string"),
        ]);
        let result = run(&program(Vec::new()), &source(quads));
        assert!(result.inferred.iter().any(|row| row.predicate == DONE));
        let clash = result
            .inferred
            .iter()
            .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
            .unwrap();
        assert!(
            clash
                .premises
                .iter()
                .any(|(s, p, o)| s == RECORD && p == predicate && o.contains("\"0\""))
        );
    }
}

#[test]
fn named_datatype_maximum_counts_cross_datatype_values_with_late_feedback() {
    for second in ["1", "2"] {
        let mut quads = maximum_source(QUALIFIED_MAXIMUM, "1", INTEGER);
        quads.extend([
            fact(RECORD, ON_DATA_RANGE, INTEGER),
            literal(SUBJECT, P, "01", INTEGER),
            literal(SUBJECT, P, "1.0", DECIMAL),
            literal(SUBJECT, "urn:seed", second, INTEGER),
            literal(SUBJECT, P, "2", "http://www.w3.org/2001/XMLSchema#float"),
            literal(
                SUBJECT,
                P,
                "unrelated",
                "http://www.w3.org/2001/XMLSchema#string",
            ),
        ]);
        let result = run(
            &program(vec![(atom("?x", "urn:seed", "?y"), atom("?x", P, "?y"))]),
            &source(quads),
        );
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            second == "2"
        );
        if second == "2" {
            let clash = result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
                .unwrap();
            assert_eq!(clash.premises.len(), 6);
            assert!(
                clash
                    .premises
                    .iter()
                    .any(|(s, p, _)| s == RECORD && p == ON_DATA_RANGE)
            );
        }
    }
    for (rational, clashes) in [("1/3", false), ("1/2", true)] {
        let mut quads = maximum_source(QUALIFIED_MAXIMUM, "0", INTEGER);
        quads.extend([
            fact(RECORD, ON_DATA_RANGE, DECIMAL),
            literal(
                SUBJECT,
                P,
                rational,
                "http://www.w3.org/2002/07/owl#rational",
            ),
        ]);
        assert_eq!(
            run(&program(Vec::new()), &source(quads))
                .inferred
                .iter()
                .any(|row| row.predicate == DONE),
            clashes
        );
    }
}

#[test]
fn maximum_never_guesses_a_missing_qualifier_or_invents_thing_membership() {
    let mut quads = maximum_source(QUALIFIED_MAXIMUM, "0", INTEGER);
    quads.push(fact(SUBJECT, P, "urn:a"));
    assert!(
        !run(&program(Vec::new()), &source(quads.clone()))
            .inferred
            .iter()
            .any(|row| row.predicate == DONE)
    );
    quads.push(fact(
        RECORD,
        ON_CLASS,
        "http://www.w3.org/2002/07/owl#Thing",
    ));
    assert!(
        run(&program(Vec::new()), &source(quads))
            .inferred
            .iter()
            .any(|row| row.predicate == DONE)
    );
}

#[test]
fn maximum_bound_admission_matches_the_typed_projection_and_has_no_usize_truncation() {
    for (count, datatype) in [
        ("-1", INTEGER),
        ("1.0", DECIMAL),
        ("256", "http://www.w3.org/2001/XMLSchema#byte"),
        ("many", "http://www.w3.org/2001/XMLSchema#string"),
    ] {
        let mut quads = maximum_source(MAXIMUM, count, datatype);
        quads.push(fact(SUBJECT, P, "urn:a"));
        let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
        assert!(
            execute(
                &prepared,
                crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
                &crate::physical::SelectedDomains::new([]).unwrap(),
                None
            )
            .is_err(),
            "{count} {datatype}"
        );
    }
    for (count, datatype) in [
        ("1", "http://www.w3.org/2001/XMLSchema#string"),
        ("18446744073709551616", INTEGER),
    ] {
        let mut quads = maximum_source(MAXIMUM, count, datatype);
        quads.push(fact(SUBJECT, P, "urn:a"));
        assert!(
            !run(&program(Vec::new()), &source(quads))
                .inferred
                .iter()
                .any(|row| row.predicate == DONE)
        );
    }
}

#[test]
fn maximum_search_can_find_a_witness_after_unproved_or_undefined_candidates() {
    let mut quads = maximum_source(MAXIMUM, "2", INTEGER);
    for value in ["urn:a", "urn:b", "urn:c", "urn:d"] {
        quads.push(fact(SUBJECT, P, value));
    }
    quads.extend([
        fact("urn:b", DIFFERENT, "urn:c"),
        fact("urn:b", DIFFERENT, "urn:d"),
        fact("urn:c", DIFFERENT, "urn:d"),
    ]);
    assert!(
        run(&program(Vec::new()), &source(quads))
            .inferred
            .iter()
            .any(|row| row.predicate == DONE)
    );
    for known_witness in [false, true] {
        let mut quads = maximum_source(MAXIMUM, "1", INTEGER);
        quads.extend([
            literal(SUBJECT, P, "unknown", "urn:uninterpreted"),
            literal(SUBJECT, P, "1", INTEGER),
        ]);
        if known_witness {
            quads.push(literal(SUBJECT, P, "2", INTEGER));
        }
        let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
        let result = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        );
        if known_witness {
            assert!(
                result
                    .unwrap()
                    .inferred
                    .iter()
                    .any(|row| row.predicate == DONE)
            );
        } else {
            assert!(result.is_err());
        }
    }
}

#[path = "datatype_tests.rs"]
mod datatype_tests;

#[path = "datatype_constraints_tests.rs"]
mod datatype_constraints_tests;
