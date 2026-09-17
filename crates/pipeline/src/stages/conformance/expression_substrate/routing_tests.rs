// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::{BlankScope, RdfDatasetBuilder, RdfTerm};

use super::{
    APPLICATION_EXPRESSION, MIN_QUALIFIED_CARDINALITY, ON_PROPERTY, OPERATOR,
    expression_reasoning_context, probe_graph_dataset,
};

const PROBE: &str = "https://blackcatinformatics.ca/gmeow/graph/probe";

#[test]
fn expression_probe_keeps_both_complete_placements_and_shared_expression_identity() {
    let mut builder = RdfDatasetBuilder::new();
    let expression = builder.intern_blank("expression", BlankScope(7));
    let operator = builder.intern_iri("https://blackcatinformatics.ca/math/operator");
    let multiplication = builder.intern_iri("https://blackcatinformatics.ca/math/Multiplication");
    let claim = builder.intern_blank("claim", BlankScope(9));
    let source_graph = builder.intern_iri("urn:expression:source-world");
    let according_to = builder.intern_iri("https://blackcatinformatics.ca/gmeow/accordingTo");
    let triple = builder.intern_triple(expression, operator, multiplication);
    builder.push_quad(expression, operator, multiplication, None);
    builder.push_reifier_in_graph(claim, triple, None);
    builder.push_annotation_in_graph(claim, according_to, source_graph, None);
    builder.declare_named_graph(source_graph);
    let source = builder.freeze().unwrap();
    let original = source.owned_quads().next().unwrap();
    let output = probe_graph_dataset(&source).unwrap();
    assert_eq!(output.quad_count(), 2);
    assert_eq!(output.reifiers().count(), 2);
    assert_eq!(output.annotations().count(), 2);
    for graph in [None, Some(RdfTerm::iri(PROBE))] {
        let row = output
            .owned_quads()
            .find(|q| q.graph_name == graph)
            .unwrap();
        assert_eq!(row.subject, original.subject);
        assert_eq!(row.object, original.object);
        let reifier = output.owned_reifiers().find(|r| r.graph == graph).unwrap();
        assert_eq!(reifier.statement.subject, original.subject);
        let annotation = output
            .owned_annotations()
            .find(|a| a.graph == graph)
            .unwrap();
        assert_eq!(annotation.reifier, reifier.reifier);
        assert_eq!(
            annotation.object,
            RdfTerm::iri("urn:expression:source-world")
        );
    }
    let reifiers = output.owned_reifiers().collect::<Vec<_>>();
    assert_eq!(reifiers[0].reifier, reifiers[1].reifier);
    let graphs = output.owned_named_graphs().collect::<Vec<_>>();
    assert_eq!(graphs.len(), 2);
    assert!(graphs.contains(&RdfTerm::iri(PROBE)));
    assert!(graphs.contains(&RdfTerm::iri("urn:expression:source-world")));
    assert_eq!(source.owned_quads().next().unwrap(), original);
}

#[test]
fn expression_empty_probe_preserves_original_declarations_and_adds_the_selected_role() {
    let mut builder = RdfDatasetBuilder::new();
    let original = builder.intern_iri("urn:expression:empty-original");
    builder.declare_named_graph(original);
    let output = probe_graph_dataset(&builder.freeze().unwrap()).unwrap();
    assert_eq!(output.rdf_row_count(), 0);
    let graphs = output.owned_named_graphs().collect::<Vec<_>>();
    assert_eq!(graphs.len(), 2);
    assert!(graphs.contains(&RdfTerm::iri(PROBE)));
    assert!(graphs.contains(&RdfTerm::iri("urn:expression:empty-original")));
}

fn math_context(extra: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    let source = format!(
        r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix math: <https://blackcatinformatics.ca/math/> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

math:ApplicationExpression rdfs:subClassOf [
    a logic:Restriction ;
    logic:onProperty math:operator ;
    logic:minQualifiedCardinality "1"^^xsd:integer ;
    logic:onClass logic:Thing
] .
{extra}
"#
    );
    purrdf::parse_dataset(source.as_bytes(), "text/turtle", None).unwrap()
}

#[test]
fn expression_reasoning_context_projects_only_the_exact_authored_law() {
    let source = math_context(
        "<urn:unrelated> a logic:ContextualRuleRequest ; logic:onProperty <urn:noise> .",
    );
    let focused = expression_reasoning_context(&source).unwrap();
    let quads = focused.owned_quads().collect::<Vec<_>>();
    assert_eq!(quads.len(), 5, "one root edge and four restriction rows");
    assert!(
        quads
            .iter()
            .all(|quad| quad.subject != RdfTerm::iri("urn:unrelated"))
    );
    assert!(quads.iter().any(|quad| {
        quad.subject == RdfTerm::iri(APPLICATION_EXPRESSION)
            && matches!(quad.object, RdfTerm::BlankNode(_))
    }));
    assert!(
        quads
            .iter()
            .any(|quad| { quad.predicate == ON_PROPERTY && quad.object == RdfTerm::iri(OPERATOR) })
    );
    assert!(quads.iter().any(|quad| {
        quad.predicate == MIN_QUALIFIED_CARDINALITY
            && matches!(&quad.object, RdfTerm::Literal(literal) if literal.lexical_form == "1")
    }));
}

#[test]
fn expression_reasoning_context_rejects_ambiguous_authored_laws() {
    let source = math_context(
        r#"
math:ApplicationExpression rdfs:subClassOf [
    a logic:Restriction ;
    logic:onProperty math:operator ;
    logic:minQualifiedCardinality "1"^^xsd:integer ;
    logic:onClass logic:Thing
] .
"#,
    );
    let error = expression_reasoning_context(&source).unwrap_err();
    assert!(
        error.to_string().contains("exactly one"),
        "ambiguous canonical input must fail closed: {error}"
    );
}
