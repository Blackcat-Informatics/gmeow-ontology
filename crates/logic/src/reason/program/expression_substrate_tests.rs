// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The expression-substrate conformance scenes reason over the single authored
//! `math:ApplicationExpression ⊑ ≥1 math:operator.logic:Thing` restriction
//! (`slices/grounding/math/module.ttl`) plus one small scene, in one default
//! world under the nonempty-object-domain profile and without a runtime budget.
//! Their witnesses are only ever `logic:Thing`, never members of the restricted
//! class, so the restricted chase terminates after at most one witness per
//! application. The input-specific termination certificate must admit them: a
//! refusal here is a certifier precision defect, not a non-terminating input.
//! These inputs are inline; they never build or read the repository corpus.

use super::*;

const OPERATOR: &str = "https://blackcatinformatics.ca/math/operator";

/// The exact five-triple reasoning context the conformance stage selects from
/// the canonical math TBox (the authored restriction and its subclass edge).
const CONTEXT: &str = r#"
@prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
math:ApplicationExpression rdfs:subClassOf [
    a logic:Restriction ;
    logic:onProperty math:operator ;
    logic:minQualifiedCardinality 1 ;
    logic:onClass logic:Thing
] .
"#;

/// The operator-less refutation control: the one scene that must invent a filler.
const OPERATOR_LESS: &str = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <http://example.org/math/refuted/> .
ex:noOperator a math:ApplicationExpression ;
    math:argumentSlot ex:slot0 .
ex:slot0 a math:ArgumentSlot ;
    math:slotIndex 0 ;
    math:slotExpression ex:leaf .
ex:leaf a math:NumberLiteral ;
    math:literalValue 1 .
"#;

/// The independent-wrapper twins control: every application already names its
/// operator, so the lower bound is satisfied without invention.
const TWINS: &str = r#"
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix ex:   <https://example.org/twin/> .
ex:symL a math:MathematicalSymbol .
ex:symR a math:MathematicalSymbol .
ex:appA a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sA0 , ex:sA1 .
ex:sA0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refA0 .
ex:sA1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refA1 .
ex:refA0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refA1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
ex:appB a math:ApplicationExpression ; math:operator math:Multiplication ;
    math:argumentSlot ex:sB0 , ex:sB1 .
ex:sB0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:refB0 .
ex:sB1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:refB1 .
ex:refB0 a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .
ex:refB1 a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .
"#;

/// Reason the context plus `scene` exactly as the conformance action does: one
/// default world, `NonemptyObjectDomainV1`, the empty authored program, no budget.
fn reason_scene(scene: &str) -> (usize, gmeow_errors::Result<ProgramClosure>) {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for text in [CONTEXT, scene] {
        let parsed = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
        for quad in parsed.owned_quads() {
            builder.push_owned_quad(&quad);
        }
    }
    let dataset = builder.freeze().unwrap();
    let quads = dataset.quad_count();
    let input = prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains =
        crate::physical::SelectedDomains::new([crate::physical::SelectedLogicalWorld::new(
            crate::physical::LogicalGraph::Default,
            crate::physical::DomainProfile::NonemptyObjectDomainV1,
            "gmeow.pipeline.expression-substrate.v1".to_owned(),
            *input.ingress_contract(),
        )
        .unwrap()])
        .unwrap();
    let prepared = crate::program_analysis::prepare_program(
        &gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None),
    )
    .unwrap();
    (quads, execute(&prepared, input, &domains, None))
}

fn operator_fillers(
    result: &ProgramClosure,
    subject: &str,
) -> std::collections::BTreeSet<TermValue> {
    result
        .inferred
        .iter()
        .filter(|row| row.subject == subject && row.predicate == OPERATOR)
        .map(|row| row.object.clone())
        .collect()
}

fn operator_witnesses(result: &ProgramClosure) -> usize {
    result
        .witnesses
        .iter()
        .filter(|witness| witness.rule_iri.starts_with("dl:minimum-witness:"))
        .count()
}

#[test]
fn the_operator_less_control_certifies_and_mints_exactly_one_operator_witness() {
    let (quads, result) = reason_scene(OPERATOR_LESS);
    assert_eq!(quads, 12, "the conformance control is the 12-quad scene");
    let result = result.expect("a Thing-only witness chase must be admitted without a budget");
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    // The nonempty-object-domain law invents its own domain element; exactly one
    // operator witness is minted by the minimum law.
    assert_eq!(operator_witnesses(&result), 1);
    assert_eq!(
        operator_fillers(&result, "http://example.org/math/refuted/noOperator").len(),
        1
    );
    assert!(
        result
            .certificates
            .iter()
            .all(|certificate| certificate.admission.admits_native())
    );
    // The witness is a Thing filler, never a member of the restricted class.
    assert!(!result.inferred.iter().any(|row| {
        row.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && row.object
                == TermValue::iri("https://blackcatinformatics.ca/math/ApplicationExpression")
            && !row.is_edb
    }));
}

#[test]
fn the_twins_control_certifies_without_inventing_an_operator() {
    let (quads, result) = reason_scene(TWINS);
    assert_eq!(quads, 35);
    let result = result.expect("a satisfied Thing-only lower bound must be admitted");
    assert_eq!(result.status, crate::seam::BudgetStatus::Ok);
    assert_eq!(operator_witnesses(&result), 0);
    for application in [
        "https://example.org/twin/appA",
        "https://example.org/twin/appB",
    ] {
        assert_eq!(
            operator_fillers(&result, application),
            [TermValue::iri(
                "https://blackcatinformatics.ca/math/Multiplication"
            )]
            .into_iter()
            .collect()
        );
    }
}
