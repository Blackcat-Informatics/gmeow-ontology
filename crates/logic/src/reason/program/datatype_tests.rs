// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Completed native datatype definitions participate in the authored fixed point.

use super::*;

const DATATYPE: &str = "urn:datatype:root";
const BASE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const MIN: &str = "http://www.w3.org/2001/XMLSchema#minInclusive";
const MAX: &str = "http://www.w3.org/2001/XMLSchema#maxInclusive";
const FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";
const UNION: &str = "http://www.w3.org/2002/07/owl#unionOf";
const INTERSECTION: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const COMPLEMENT: &str = "http://www.w3.org/2002/07/owl#datatypeComplementOf";
const STRING: &str = "http://www.w3.org/2001/XMLSchema#string";

fn qualified() -> Vec<RdfQuad> {
    let mut quads = maximum_source(QUALIFIED_MAXIMUM, "0", INTEGER);
    quads.push(fact(RECORD, ON_DATA_RANGE, DATATYPE));
    quads
}

fn restriction() -> Vec<RdfQuad> {
    let mut quads = qualified();
    quads.extend([
        fact(DATATYPE, BASE, INTEGER),
        fact(DATATYPE, RESTRICTIONS, "urn:datatype:list"),
        fact("urn:datatype:list", FIRST, "urn:datatype:facet"),
        fact("urn:datatype:list", REST, NIL),
    ]);
    quads
}

fn clashes(quads: Vec<RdfQuad>) -> bool {
    run(&program(Vec::new()), &source(quads))
        .inferred
        .iter()
        .any(|row| row.predicate == DONE)
}

#[test]
fn native_datatype_dag_composes_membership_and_preserves_lexical_proofs() {
    // (decimal or string) and not {1}: duplicate references share one DAG node.
    for (value, datatype, expected) in [
        ("1.0", DECIMAL, false),
        ("2", INTEGER, true),
        ("text", STRING, true),
    ] {
        let mut quads = qualified();
        quads.extend([
            fact(DATATYPE, INTERSECTION, "urn:outer:1"),
            fact("urn:outer:1", FIRST, "urn:choice"),
            fact("urn:outer:1", REST, "urn:outer:2"),
            fact("urn:outer:2", FIRST, "urn:complement"),
            fact("urn:outer:2", REST, "urn:outer:3"),
            fact("urn:outer:3", FIRST, "urn:choice"),
            fact("urn:outer:3", REST, NIL),
            fact("urn:choice", UNION, "urn:choice:1"),
            fact("urn:choice:1", FIRST, DECIMAL),
            fact("urn:choice:1", REST, "urn:choice:2"),
            fact("urn:choice:2", FIRST, STRING),
            fact("urn:choice:2", REST, NIL),
            fact("urn:complement", COMPLEMENT, "urn:enumeration"),
            fact("urn:enumeration", ONE_OF, "urn:enum:1"),
            literal("urn:enum:1", FIRST, "01", INTEGER),
            fact("urn:enum:1", REST, NIL),
            literal(SUBJECT, P, value, datatype),
        ]);
        let result = run(&program(Vec::new()), &source(quads));
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            expected
        );
        if expected {
            let proof = &result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
                .unwrap()
                .premises;
            assert!(
                proof
                    .iter()
                    .any(|(s, p, o)| s == "urn:enum:1" && p == FIRST && o.contains("\"01\""))
            );
            assert!(
                proof
                    .iter()
                    .any(|(s, p, _)| s == DATATYPE && p == INTERSECTION)
            );
            assert!(proof.iter().any(|(s, p, _)| s == SUBJECT && p == P));
        }
    }
}

#[test]
fn datatype_facets_are_conjunctive_and_preserve_every_definition_premise() {
    for (minimum, expected) in [("3", false), ("2", true)] {
        let mut quads = restriction();
        quads.extend([
            literal("urn:datatype:facet", MIN, "1", INTEGER),
            literal("urn:datatype:facet", MIN, minimum, INTEGER),
            literal("urn:datatype:facet", MAX, "2", INTEGER),
            literal(SUBJECT, P, "2.0", DECIMAL),
        ]);
        let result = run(&program(Vec::new()), &source(quads));
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            expected
        );
        if expected {
            let proof = &result
                .inferred
                .iter()
                .find(|row| row.rule_name.as_deref() == Some("dl:max-cardinality-clash"))
                .unwrap()
                .premises;
            assert_eq!(
                proof
                    .iter()
                    .filter(|(s, p, _)| s == "urn:datatype:facet" && p == MIN)
                    .count(),
                2
            );
            assert!(
                proof
                    .iter()
                    .any(|(s, p, _)| s == "urn:datatype:facet" && p == MAX)
            );
            for predicate in [BASE, RESTRICTIONS] {
                assert!(
                    proof
                        .iter()
                        .any(|(s, p, _)| s == DATATYPE && p == predicate)
                );
            }
        }
    }
}

#[test]
fn datatype_definition_producers_complete_before_any_membership_witness() {
    for (minimum, expected) in [("3", false), ("2", true)] {
        let mut quads = restriction();
        quads.extend([
            literal("urn:datatype:facet", MIN, "1", INTEGER),
            literal("urn:datatype:facet", "urn:facet:seed", minimum, INTEGER),
            literal(SUBJECT, P, "2", INTEGER),
        ]);
        let p = program(vec![
            (
                atom("?x", "urn:facet:seed", "?y"),
                atom("?x", "urn:facet:stage", "?y"),
            ),
            (atom("?x", "urn:facet:stage", "?y"), atom("?x", MIN, "?y")),
        ]);
        let result = run(&p, &source(quads));
        assert_eq!(
            result.inferred.iter().any(|row| row.predicate == DONE),
            expected
        );
    }
    // Complete definitions may themselves need multiple rounds to produce lists.
    let mut quads = qualified();
    quads.extend([
        fact(DATATYPE, ONE_OF, "urn:enum:1"),
        literal("urn:enum:1", "urn:list:seed", "1", INTEGER),
        fact("urn:enum:1", REST, NIL),
        literal(SUBJECT, P, "1.0", DECIMAL),
    ]);
    let p = program(vec![
        (
            atom("?x", "urn:list:seed", "?y"),
            atom("?x", "urn:list:stage", "?y"),
        ),
        (atom("?x", "urn:list:stage", "?y"), atom("?x", FIRST, "?y")),
    ]);
    assert!(
        run(&p, &source(quads))
            .inferred
            .iter()
            .any(|row| row.predicate == DONE)
    );
}

#[test]
fn datatype_definition_feedback_cycle_refuses_a_transient_clash() {
    let mut quads = restriction();
    quads.extend([
        literal("urn:datatype:facet", MIN, "1", INTEGER),
        literal(SUBJECT, P, "2", INTEGER),
    ]);
    let p = program(vec![(
        atom("?x", DONE, "?y"),
        atom("urn:datatype:facet", MIN, "?y"),
    )]);
    let prepared = crate::program_analysis::prepare_program(&p).unwrap();
    let error = match execute(
        &prepared,
        crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
        &crate::physical::SelectedDomains::new([]).unwrap(),
        None,
    ) {
        Ok(_) => panic!("a datatype cannot justify a producer that strengthens its own definition"),
        Err(error) => error.to_string(),
    };
    assert!(error.contains("NonStratifiable"), "{error}");
}

#[test]
fn datatype_definitions_with_identical_names_remain_world_local() {
    let mut quads = Vec::new();
    for (world, minimum) in [("urn:world:excluded", "3"), ("urn:world:included", "1")] {
        let mut local = restriction();
        local.extend([
            literal("urn:datatype:facet", MIN, minimum, INTEGER),
            literal(SUBJECT, P, "2", INTEGER),
        ]);
        quads.extend(
            local
                .into_iter()
                .map(|quad| quad.in_graph(RdfTerm::iri(world))),
        );
    }
    let result = run(&program(Vec::new()), &source(quads));
    let rows: Vec<_> = result
        .inferred
        .iter()
        .filter(|row| row.predicate == DONE)
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].world, "urn:world:included");
}

#[test]
fn datatype_definition_admission_refuses_malformed_or_unsupported_sources() {
    let cases = [
        vec![],
        vec![fact(DATATYPE, BASE, INTEGER)],
        vec![fact(DATATYPE, COMPLEMENT, DATATYPE)],
        vec![fact(DATATYPE, UNION, "urn:missing:list")],
        vec![
            fact(DATATYPE, COMPLEMENT, INTEGER),
            fact(DATATYPE, COMPLEMENT, DECIMAL),
        ],
        vec![
            fact(DATATYPE, COMPLEMENT, INTEGER),
            fact(DATATYPE, UNION, NIL),
        ],
        vec![
            fact(DATATYPE, ONE_OF, "urn:bad:list"),
            fact("urn:bad:list", FIRST, INTEGER),
            fact("urn:bad:list", REST, NIL),
        ],
        vec![
            fact(DATATYPE, BASE, INTEGER),
            fact(DATATYPE, RESTRICTIONS, "urn:bad:list"),
            fact("urn:bad:list", FIRST, "urn:facet"),
            fact("urn:bad:list", REST, NIL),
            literal(
                "urn:facet",
                "http://www.w3.org/2001/XMLSchema#pattern",
                ".*",
                STRING,
            ),
        ],
    ];
    for (case, definition) in cases.into_iter().enumerate() {
        let mut quads = qualified();
        quads.extend(definition);
        quads.push(literal(SUBJECT, P, "2", INTEGER));
        let prepared = crate::program_analysis::prepare_program(&program(Vec::new())).unwrap();
        assert!(
            execute(
                &prepared,
                crate::reason::program::prepare_reasoning_input(&source(quads)).unwrap(),
                &crate::physical::SelectedDomains::new([]).unwrap(),
                None
            )
            .is_err(),
            "case {case}"
        );
    }
}

#[test]
fn datatype_length_restriction_uses_shared_native_values_in_cardinality() {
    for (base, value, expected) in [
        (STRING, "é🐈", true),
        (STRING, "é", false),
        ("http://www.w3.org/2001/XMLSchema#hexBinary", "CAFE", true),
    ] {
        let mut quads = restriction();
        quads.retain(|quad| quad.predicate.as_str() != BASE);
        quads.extend([
            fact(DATATYPE, BASE, base),
            literal(
                "urn:datatype:facet",
                "http://www.w3.org/2001/XMLSchema#length",
                "2",
                INTEGER,
            ),
            literal(SUBJECT, P, value, base),
        ]);
        assert_eq!(clashes(quads), expected);
    }
}
