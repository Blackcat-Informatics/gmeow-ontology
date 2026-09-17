// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Datatype contradictions reach authored consumers in the shared native closure.
//! No terminal DL pass or repository corpus can supply these consequences.

use super::*;

const DATATYPE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#DatatypeProperty";
const ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const RANGE: &str = "http://www.w3.org/2000/01/rdf-schema#range";
const SOME: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const ALL: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const BYTE: &str = "http://www.w3.org/2001/XMLSchema#byte";
const DATATYPE: &str = "urn:datatype:constraint";
const BASE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const LOWER: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const UPPER: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

fn base() -> Vec<RdfQuad> {
    vec![
        fact(P, TYPE, DATATYPE_PROPERTY),
        fact(SUBJECT, TYPE, RECORD),
        fact(RECORD, ON_PROPERTY, P),
    ]
}

fn definition(minimum: &str) -> Vec<RdfQuad> {
    vec![
        fact(DATATYPE, BASE, INTEGER),
        fact(DATATYPE, RESTRICTIONS, "urn:list"),
        fact("urn:list", FIRST, "urn:facet"),
        fact("urn:list", REST, NIL),
        literal("urn:facet", LOWER, minimum, INTEGER),
        literal("urn:facet", UPPER, "2", INTEGER),
    ]
}

fn is_done(result: &ProgramClosure) -> bool {
    result.inferred.iter().any(|row| row.predicate == DONE)
}

#[test]
fn late_native_range_values_wake_authored_consumers_with_actual_source_spellings() {
    for canonical in [false, true] {
        let type_predicate = if canonical {
            "https://blackcatinformatics.ca/logic/instanceOf"
        } else {
            TYPE
        };
        let property_class = if canonical {
            "https://blackcatinformatics.ca/logic/DatatypeProperty"
        } else {
            DATATYPE_PROPERTY
        };
        let range_predicate = if canonical {
            "https://blackcatinformatics.ca/logic/range"
        } else {
            RANGE
        };
        let data = source(vec![
            fact(P, type_predicate, property_class),
            fact(P, range_predicate, INTEGER),
            literal(SUBJECT, "urn:seed", "not an integer", STRING),
        ]);
        let authored = program(vec![
            (atom("?x", "urn:seed", "?y"), atom("?x", "urn:stage", "?y")),
            (atom("?x", "urn:stage", "?y"), atom("?x", P, "?y")),
        ]);
        let result = run(&authored, &data);
        assert!(is_done(&result));
        let proof = &result
            .inferred
            .iter()
            .find(|row| row.rule_name.as_deref() == Some("dl:datatype-membership-clash"))
            .unwrap()
            .premises;
        assert!(proof.iter().any(|(s, p, _)| s == P && p == type_predicate));
        assert!(proof.iter().any(|(s, p, _)| s == P && p == range_predicate));
        assert!(
            proof
                .iter()
                .any(|(s, p, o)| s == SUBJECT && p == P && o.contains("not an integer"))
        );
    }
}

#[test]
fn datatype_property_literal_domain_and_local_universals_are_actual_clash_producers() {
    for (value, expected) in [
        (fact(SUBJECT, P, "urn:resource"), true),
        (literal(SUBJECT, P, "literal", STRING), false),
    ] {
        let mut facts = base();
        facts.push(value);
        assert_eq!(
            is_done(&run(&program(Vec::new()), &source(facts))),
            expected
        );
    }
    let mut facts = base();
    facts.extend([
        fact(RECORD, ALL, INTEGER),
        literal(SUBJECT, P, "literal", STRING),
    ]);
    assert!(is_done(&run(&program(Vec::new()), &source(facts))));
}

#[test]
fn independent_existential_fillers_do_not_constrain_existing_values_or_each_other() {
    let mut facts = base();
    facts.extend([
        fact(RECORD, SOME, INTEGER),
        fact(RECORD, SOME, STRING),
        literal(SUBJECT, P, "unrelated existing value", STRING),
    ]);
    assert!(!is_done(&run(&program(Vec::new()), &source(facts))));
}

#[test]
fn existential_empty_space_reaches_authored_rules_with_all_facet_premises() {
    let mut facts = base();
    facts.push(fact(RECORD, SOME, DATATYPE));
    facts.extend(definition("3"));
    let result = run(&program(Vec::new()), &source(facts));
    assert!(is_done(&result));
    let proof = &result
        .inferred
        .iter()
        .find(|row| row.rule_name.as_deref() == Some("dl:datatype-capacity-clash"))
        .unwrap()
        .premises;
    for predicate in [
        LOWER,
        UPPER,
        BASE,
        RESTRICTIONS,
        FIRST,
        REST,
        SOME,
        ON_PROPERTY,
    ] {
        assert!(
            proof.iter().any(|(_, p, _)| p == predicate),
            "missing {predicate}: {proof:?}"
        );
    }
}

#[test]
fn finite_datatype_lower_and_exact_counts_are_conjunctive_and_keep_large_source_values() {
    for (predicate, qualified) in [
        ("http://www.w3.org/2002/07/owl#minCardinality", false),
        ("http://www.w3.org/2002/07/owl#cardinality", false),
        (
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            true,
        ),
        ("http://www.w3.org/2002/07/owl#qualifiedCardinality", true),
    ] {
        for (count, expected) in [(256u128, false), (257, true), (1u128 << 64, true)] {
            let mut facts = base();
            let bound = count.to_string();
            facts.push(literal(RECORD, predicate, &bound, INTEGER));
            facts.push(if qualified {
                fact(RECORD, ON_DATA_RANGE, BYTE)
            } else {
                fact(P, RANGE, BYTE)
            });
            let result = run(&program(Vec::new()), &source(facts));
            assert_eq!(is_done(&result), expected);
            if expected {
                let proof = &result
                    .inferred
                    .iter()
                    .find(|row| row.rule_name.as_deref() == Some("dl:datatype-capacity-clash"))
                    .unwrap()
                    .premises;
                assert!(
                    proof
                        .iter()
                        .any(|(s, p, o)| s == RECORD && p == predicate && o.contains(&bound))
                );
                assert!(
                    proof
                        .iter()
                        .any(|(_, p, _)| p == if qualified { ON_DATA_RANGE } else { RANGE })
                );
            }
        }
    }
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, BYTE),
        literal(
            RECORD,
            "http://www.w3.org/2002/07/owl#minCardinality",
            "257",
            INTEGER,
        ),
        literal(
            RECORD,
            "http://www.w3.org/2002/07/owl#minCardinality",
            "1",
            INTEGER,
        ),
    ]);
    assert!(is_done(&run(&program(Vec::new()), &source(facts))));
}

#[test]
fn missing_qualified_datatype_is_never_guessed_from_a_property_range() {
    let mut facts = base();
    facts.extend([
        fact(P, RANGE, BYTE),
        literal(
            RECORD,
            "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
            "257",
            INTEGER,
        ),
    ]);
    assert!(!is_done(&run(&program(Vec::new()), &source(facts))));
}

#[test]
fn a_universal_empty_space_only_clashes_when_a_value_is_required() {
    for witness in [false, true] {
        let mut facts = base();
        facts.push(fact(RECORD, ALL, DATATYPE));
        facts.extend(definition("3"));
        if witness {
            facts.extend([
                fact(SUBJECT, TYPE, "urn:other:restriction"),
                fact("urn:other:restriction", ON_PROPERTY, P),
                fact("urn:other:restriction", SOME, INTEGER),
            ]);
        }
        assert_eq!(is_done(&run(&program(Vec::new()), &source(facts))), witness);
    }
}

#[test]
fn datatype_constraint_definition_writers_complete_before_clash_evaluation() {
    for (minimum, expected) in [("2", false), ("3", true)] {
        let mut facts = base();
        facts.push(fact(RECORD, SOME, DATATYPE));
        facts.extend(definition("1"));
        facts.push(literal("urn:facet", "urn:facet:seed", minimum, INTEGER));
        let authored = program(vec![
            (
                atom("?x", "urn:facet:seed", "?y"),
                atom("?x", "urn:facet:stage", "?y"),
            ),
            (atom("?x", "urn:facet:stage", "?y"), atom("?x", LOWER, "?y")),
        ]);
        assert_eq!(is_done(&run(&authored, &source(facts))), expected);
    }
}

#[test]
fn datatype_capacity_feedback_through_a_definition_is_not_stratifiable() {
    let mut facts = base();
    facts.push(fact(RECORD, SOME, DATATYPE));
    facts.extend(definition("3"));
    let authored = program(vec![(
        atom("?x", DONE, "?y"),
        atom("urn:facet", LOWER, "?y"),
    )]);
    let prepared = crate::program_analysis::prepare_program(&authored).unwrap();
    let error = execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(facts)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    )
    .err()
    .expect("a definition cannot depend on its own capacity decision")
    .to_string();
    assert!(error.contains("NonStratifiable"), "{error}");
}

#[test]
fn native_datatype_capacity_and_proofs_stay_in_the_asserting_world() {
    let mut facts = Vec::new();
    for (world, minimum) in [("urn:empty:world", "3"), ("urn:inhabited:world", "1")] {
        let mut local = base();
        local.push(fact(RECORD, SOME, DATATYPE));
        local.extend(definition(minimum));
        facts.extend(
            local
                .into_iter()
                .map(|quad| quad.in_graph(RdfTerm::iri(world))),
        );
    }
    let result = run(&program(Vec::new()), &source(facts));
    let worlds: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == DONE)
        .map(|row| row.world.as_str())
        .collect();
    assert_eq!(worlds, ["urn:empty:world"]);
}

#[test]
fn upstream_finite_datatype_capacity_reaches_the_authored_constraint_consumer() {
    for (count, expected) in [("2", false), ("3", true)] {
        let mut facts = base();
        facts.extend([
            fact(
                RECORD,
                ON_DATA_RANGE,
                "http://www.w3.org/2001/XMLSchema#boolean",
            ),
            literal(
                RECORD,
                "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
                count,
                INTEGER,
            ),
        ]);
        assert_eq!(
            is_done(&run(&program(Vec::new()), &source(facts))),
            expected
        );
    }
}

#[test]
fn undefined_membership_and_capacity_are_explicit_refusals_not_negative_decisions() {
    for extra in [
        vec![
            fact(P, RANGE, INTEGER),
            literal(SUBJECT, P, "unknown", "urn:uninterpreted"),
        ],
        vec![
            fact(RECORD, ON_DATA_RANGE, DATATYPE),
            fact(
                DATATYPE,
                "http://www.w3.org/2002/07/owl#oneOf",
                "urn:unknown:list",
            ),
            literal("urn:unknown:list", FIRST, "unknown", "urn:uninterpreted"),
            fact("urn:unknown:list", REST, NIL),
            literal(
                RECORD,
                "http://www.w3.org/2002/07/owl#minQualifiedCardinality",
                "3",
                INTEGER,
            ),
        ],
    ] {
        let mut facts = base();
        facts.extend(extra);
        let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
        let result = execute(
            &prepared,
            crate::reason::program::prepare_reasoning_input(&source(facts)).unwrap(),
            &crate::physical::SelectedDomains::new([]).unwrap(),
            None,
        )
        .expect("undefined value-space evidence remains a typed native result");
        assert!(
            matches!(
                &result.native_status,
                crate::reason::refute::NativeClosureStatus::Blocked { reads }
                    if !reads.is_empty()
            ),
            "an undefined proof premise must withhold its affected producer: {:?}",
            result.native_status
        );
        assert!(
            !is_done(&result),
            "an undecided premise cannot publish a clash"
        );
    }
}
