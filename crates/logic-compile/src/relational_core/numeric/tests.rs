// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Canonical Formula lowering and loss-aware transport, not upstream arithmetic tests.

use super::*;
use crate::ir::LogicProgram;
use crate::relational_core::{
    lower_program_with_formulas, parse_relational_core, project_relational_core_dataset,
};
use purrdf::RdfLiteral;

fn var(name: &str) -> Term {
    Term::Var(name.to_owned())
}
fn atom(relation: &str, args: Vec<Term>) -> Formula {
    Formula::Atom {
        relation: Term::Iri(relation.to_owned()),
        args,
    }
}
fn lower_formula(formula: Formula) -> super::super::RelationalCoreProgram {
    lower_program_with_formulas(
        &LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![formula]),
    )
}
fn rule(mut numeric: Vec<Formula>) -> Formula {
    numeric.push(atom("urn:weight", vec![var("source"), var("weight")]));
    Formula::Implies(
        Box::new(Formula::And(numeric)),
        Box::new(atom("urn:result", vec![var("source"), var("result")])),
    )
}
fn literal(value: &str) -> Term {
    Term::Literal(RdfLiteral::typed(
        value,
        "http://www.w3.org/2001/XMLSchema#decimal",
    ))
}

#[test]
fn canonical_numeric_chain_is_scheduled_and_transport_is_lossless() {
    let calls = vec![
        atom(
            NumericOperator::Add.iri(),
            vec![var("product"), literal("0.1"), var("result")],
        ),
        atom(
            NumericOperator::Multiply.iri(),
            vec![var("weight"), literal("0.8"), var("product")],
        ),
    ];
    let compiled = lower_formula(rule(calls.clone()));
    assert!(compiled.residue.is_empty(), "{:?}", compiled.residue);
    let native = &compiled.rules[0];
    assert!(
        !native.has_existential_head(),
        "numeric outputs are bound, not witnesses"
    );
    assert_eq!(native.body.len(), 1, "no numeric tuple materialization");
    assert_eq!(native.numeric[0].operator, NumericOperator::Multiply);
    assert_eq!(native.numeric[1].operator, NumericOperator::Add);
    let restored =
        parse_relational_core(&project_relational_core_dataset(&compiled).unwrap()).unwrap();
    assert_eq!(restored, compiled);
    let reversed = lower_formula(rule(calls.into_iter().rev().collect()));
    assert_eq!(
        compiled, reversed,
        "conjunction order does not choose binding semantics"
    );
    let mut changed = compiled.clone();
    changed.rules[0].numeric[0].operator = NumericOperator::Divide;
    assert_ne!(compiled.rules[0].key(), changed.rules[0].key());
    assert_ne!(
        compiled.content_key().unwrap(),
        changed.content_key().unwrap()
    );
    assert_ne!(
        compiled.projection_key().unwrap(),
        changed.projection_key().unwrap()
    );
}

#[test]
fn numeric_modes_and_head_assertions_remain_honest_residue() {
    for formula in [
        rule(vec![atom(
            NumericOperator::Add.iri(),
            vec![var("missing"), literal("1"), var("result")],
        )]),
        rule(vec![atom(
            NumericOperator::Add.iri(),
            vec![var("result"), literal("1"), var("result")],
        )]),
        rule(vec![atom(
            NumericOperator::Add.iri(),
            vec![var("weight"), var("result")],
        )]),
        atom(
            NumericOperator::Add.iri(),
            vec![literal("1"), literal("1"), literal("2")],
        ),
    ] {
        let compiled = lower_formula(formula);
        assert!(compiled.rules.is_empty());
        assert!(!compiled.residue.is_empty());
    }
}

#[test]
fn numeric_output_and_existential_witness_keep_distinct_scope() {
    let formula = Formula::Implies(
        Box::new(Formula::And(vec![
            atom("urn:weight", vec![var("s"), var("w")]),
            atom(
                NumericOperator::Add.iri(),
                vec![var("w"), literal("1"), var("result")],
            ),
        ])),
        Box::new(Formula::Exists {
            vars: vec!["witness".into()],
            body: Box::new(Formula::And(vec![
                atom("urn:record", vec![var("s"), var("witness")]),
                atom("urn:value", vec![var("witness"), var("result")]),
            ])),
        }),
    );
    let compiled = lower_formula(formula);
    assert!(compiled.residue.is_empty(), "{:?}", compiled.residue);
    assert_eq!(compiled.rules.len(), 1);
    assert_eq!(compiled.rules[0].numeric.len(), 1);
    assert_eq!(compiled.rules[0].head_conjuncts.len(), 1);
    assert!(compiled.rules[0].has_existential_head());
    assert_eq!(
        parse_relational_core(&project_relational_core_dataset(&compiled).unwrap()).unwrap(),
        compiled
    );
}

#[test]
fn numeric_projection_cannot_drop_required_operands_or_invent_an_operator() {
    let source = lower_formula(rule(vec![atom(
        NumericOperator::Add.iri(),
        vec![var("weight"), literal("1"), var("result")],
    )]));
    let dataset = project_relational_core_dataset(&source).unwrap();
    for field in [
        "rcNumericResult",
        "rcNumericLeft",
        "rcNumericOperator",
        "rcIndex",
    ] {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        for quad in dataset.owned_quads() {
            if quad.predicate != format!("https://blackcatinformatics.ca/logic/{field}") {
                builder.push_owned_quad(&quad);
            }
        }
        assert!(
            parse_relational_core(&builder.freeze().unwrap()).is_err(),
            "missing {field} must fail"
        );
    }
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        if quad.predicate.ends_with("rcNumericOperator") {
            quad.object = purrdf::RdfTerm::iri("urn:unrecognized-numeric-relation");
        }
        builder.push_owned_quad(&quad);
    }
    assert!(parse_relational_core(&builder.freeze().unwrap()).is_err());
}
