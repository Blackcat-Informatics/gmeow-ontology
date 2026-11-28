// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A whole-graph completed statement read is withheld when any one native owner retains an
//! obstruction in the world, so every cardinality definition that waits on it stays undecided.
//! Each source below carries one independent maximum-cardinality clash plus exactly one
//! construct of one obstruction kind. The clash does not depend on the construct, so the
//! native closure must complete and decide it. These inputs are inline and never build or
//! read the repository corpus.

use super::*;
use crate::reason::refute::native::{
    NativeClosureStatus, NativeFamilyCompletion, NativeRefutationFamily,
};

const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const NAMED: &str = "urn:wg:Named";
const EXPRESSION: &str = "urn:wg:Expression";
const OPERAND: &str = "urn:wg:Operand";
const LIST: &str = "urn:wg:list";
const WITNESS: &str = "urn:wg:member";

/// One maximum-cardinality clash: the subject has two distinct fillers where one is allowed.
fn clash() -> Vec<RdfQuad> {
    let mut quads = maximum_source(MAXIMUM, "1", INTEGER);
    quads.extend([
        fact(SUBJECT, P, "urn:wg:a"),
        fact(SUBJECT, P, "urn:wg:b"),
        fact("urn:wg:a", DIFFERENT, "urn:wg:b"),
    ]);
    quads
}

/// A one-element RDF list bound to `owner` through `predicate`.
fn list_of(owner: &str, predicate: &str, member: &str) -> Vec<RdfQuad> {
    vec![
        fact(owner, predicate, LIST),
        fact(LIST, RDF_FIRST, member),
        fact(LIST, RDF_REST, RDF_NIL),
    ]
}

/// Every retained obstruction of the closure, by owner, so a failure names its cause.
fn obstructions(result: &ProgramClosure) -> Vec<String> {
    let admissions = result
        .source_coverage
        .worlds
        .values()
        .flat_map(|world| &world.admissions)
        .flat_map(|admission| &admission.obstructions);
    let classes = result.classes.iter().flat_map(|class| &class.obstructions);
    let outcomes = result
        .native_families
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .flat_map(|outcome| &outcome.obstructions);
    admissions
        .chain(classes)
        .chain(outcomes)
        .map(|obstruction| format!("{:?}: {}", obstruction.kind, obstruction.detail))
        .collect()
}

/// The independent clash plus `construct` must reach a completed native closure that has
/// decided the cardinality definition.
fn assert_clash_is_decided(construct: Vec<RdfQuad>) {
    let mut quads = clash();
    quads.extend(construct);
    let result = run(&program(Vec::new()), &source(quads));
    let waiting: Vec<_> = result
        .native_families
        .iter()
        .flat_map(|ledger| &ledger.outcomes)
        .filter(|outcome| outcome.family == NativeRefutationFamily::Cardinality)
        .filter_map(|outcome| match &outcome.completion {
            NativeFamilyCompletion::Blocked { reads } => Some(reads),
            _ => None,
        })
        .collect();
    assert!(
        waiting.is_empty(),
        "a cardinality definition waits on a withheld read {waiting:?}: {:?}",
        obstructions(&result)
    );
    assert_eq!(
        result.native_status,
        NativeClosureStatus::Completed,
        "the closure is blocked: {:?}",
        obstructions(&result)
    );
    assert!(
        result.inferred.iter().any(|row| row.predicate == DONE),
        "the independent maximum-cardinality clash was not decided"
    );
}

/// The baseline every obstructed case is measured against: the clash alone completes.
#[test]
fn an_unobstructed_maximum_cardinality_clash_completes_the_closure() {
    assert_clash_is_decided(Vec::new());
}

/// A class expression whose operands are all named classes is decided by the class model and
/// retains no obstruction of its own, so it is not itself what withholds the read.
#[test]
fn a_named_operand_expression_alone_does_not_withhold_the_whole_graph_read() {
    assert_clash_is_decided(list_of(EXPRESSION, &format!("{OWL}unionOf"), OPERAND));
}

/// An equivalence between a named class and a class expression leaves the world's
/// cardinality definitions decidable: the class model decides the equivalence, and the
/// clash is independent of it.
#[test]
fn class_expression_equivalence_does_not_withhold_the_whole_graph_read() {
    let mut construct = list_of(EXPRESSION, &format!("{OWL}unionOf"), OPERAND);
    construct.push(fact(NAMED, &format!("{OWL}equivalentClass"), EXPRESSION));
    construct.push(fact(WITNESS, TYPE, NAMED));
    assert_clash_is_decided(construct);
}

/// A restriction predicate on a class connected to a selected class expression leaves the
/// world's cardinality definitions decidable.
///
/// Obstruction kind: restriction predicate. This is a known defect: the class model does
/// not complete a restriction predicate on a selected class, so the world's completed
/// statement read is withheld. The change that gives the class model support for this kind
/// removes the `should_panic` attribute, which turns the test green.
#[test]
#[should_panic(expected = "a cardinality definition waits on a withheld read")]
fn a_restriction_predicate_in_a_selected_class_model_does_not_withhold_the_whole_graph_read() {
    let mut construct = list_of(EXPRESSION, &format!("{OWL}unionOf"), OPERAND);
    construct.push(fact(OPERAND, &format!("{OWL}onProperty"), "urn:wg:other"));
    assert_clash_is_decided(construct);
}

/// A nominal class (an enumeration of individuals) leaves the world's cardinality
/// definitions decidable.
///
/// Obstruction kind: nominal class. This is a known defect: the class model requires
/// complete set-equality admission for an enumeration, so the world's completed statement
/// read is withheld. The change that gives the class model support for this kind removes
/// the `should_panic` attribute, which turns the test green.
#[test]
#[should_panic(expected = "a cardinality definition waits on a withheld read")]
fn a_nominal_class_does_not_withhold_the_whole_graph_read() {
    assert_clash_is_decided(list_of(NAMED, &format!("{OWL}oneOf"), WITNESS));
}

/// An asserted literal in a datatype with no admitted interpretation leaves an unrelated
/// cardinality definition decidable.
///
/// Obstruction kind: datatype. This is a known defect: the asserted value has no admitted
/// interpretation, so the world's completed statement read is withheld. The change that
/// resolves this kind removes the `should_panic` attribute, which turns the test green.
#[test]
#[should_panic(expected = "a cardinality definition waits on a withheld read")]
fn an_uninterpreted_literal_datatype_does_not_withhold_the_whole_graph_read() {
    assert_clash_is_decided(vec![
        fact("urn:wg:dp", TYPE, &format!("{OWL}DatatypeProperty")),
        fact(
            "urn:wg:dp",
            "http://www.w3.org/2000/01/rdf-schema#range",
            INTEGER,
        ),
        literal("urn:wg:v", "urn:wg:dp", "unknown", "urn:wg:uninterpreted"),
    ]);
}

/// A maximum bound on a datatype property leaves an unrelated cardinality definition
/// decidable.
///
/// Obstruction kind: datatype upper-bound consistency. It depends on the cardinality
/// obligation: the datatype owner retains an obstruction until the complete native
/// cardinality obligation decides the bound, so this kind clears only with the cardinality
/// support. This is a known defect: the world's completed statement read is withheld. The
/// change that gives the cardinality obligation that support removes the `should_panic`
/// attribute, which turns the test green.
#[test]
#[should_panic(expected = "a cardinality definition waits on a withheld read")]
fn a_datatype_upper_bound_does_not_withhold_the_whole_graph_read() {
    assert_clash_is_decided(vec![
        fact("urn:wg:dp", TYPE, &format!("{OWL}DatatypeProperty")),
        fact("urn:wg:bounded", &format!("{OWL}onProperty"), "urn:wg:dp"),
        literal("urn:wg:bounded", MAXIMUM, "1", INTEGER),
        fact("urn:wg:holder", TYPE, "urn:wg:bounded"),
    ]);
}
