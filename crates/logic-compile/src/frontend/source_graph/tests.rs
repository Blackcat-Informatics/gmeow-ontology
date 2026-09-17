// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW structural ownership and original-source boundaries.

use super::*;
use crate::frontend::{PreparedLogicSource, Severity, parse_logic_str};

const PREFIXES: &str = "@prefix logic: <https://blackcatinformatics.ca/logic/> .
    @prefix math: <https://blackcatinformatics.ca/math/> .
    @prefix ex: <https://example.org/> . ";
const FORMULA: &str = "ex:law a logic:Formula ; logic:not ex:atom .
    ex:atom a logic:Formula ; logic:relation ex:p ;
        logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .";

fn parse(source: &str) -> (crate::ir::LogicProgram, Vec<Diagnostic>) {
    parse_logic_str(
        &format!("{PREFIXES}{source}"),
        Some("selected-source".into()),
    )
    .unwrap()
}

#[test]
fn undeclared_recovery_owner_cannot_hide_an_orphan_or_assert_its_transform() {
    let (program, diagnostics) = parse(&format!(
        "{FORMULA}
        ex:fake logic:recoveryCase ex:case .
        ex:case a logic:RecoveryCase ; logic:recoveryTransform ex:law ."
    ));
    assert!(program.formulas.is_empty());
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "ORPHAN_RECOVERY_CASE" && d.severity == Severity::Error)
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "UNDECLARED_SEMANTIC_OWNER"
                && d.subject.as_deref() == Some("https://example.org/fake"))
    );
}

#[test]
fn malformed_constraint_owner_keeps_the_formula_owned_and_fails_explicitly() {
    let (program, diagnostics) = parse(&format!("{FORMULA} ex:fake logic:integrity ex:law ."));
    assert!(program.formulas.is_empty());
    assert!(program.constraints.is_empty());
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "UNDECLARED_SEMANTIC_OWNER")
    );
}

#[test]
fn mathematical_backing_and_member_definitions_have_distinct_roles() {
    let (backed, diagnostics) = parse(&format!(
        "{FORMULA}
        ex:concept math:definingLaw ex:law . ex:morphism math:preservesStructure ex:law ."
    ));
    assert_eq!(backed.formulas.len(), 1);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let (defined, diagnostics) = parse(&format!(
        "{FORMULA}
        ex:set math:memberCondition ex:law . ex:otherSet math:memberCondition ex:law ."
    ));
    assert!(defined.formulas.is_empty());
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.code == "OWNER_SCOPED_MEMBER_CONDITION")
            .count(),
        2
    );
}

#[test]
fn literal_target_cannot_steal_a_same_spelled_resource_identity() {
    let (program, diagnostics) = parse(&format!(
        "{FORMULA}
        ex:constraint a logic:Constraint ; logic:integrity \"https://example.org/law\" ."
    ));
    assert_eq!(
        program.formulas.len(),
        1,
        "the unrelated resource remains an assertion"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "MALFORMED_SEMANTIC_TARGET")
    );
}

#[test]
fn graph_module_and_standpoint_are_independent_ownership_coordinates() {
    let source = purrdf::parse_dataset(
        format!(
            "{PREFIXES}{FORMULA}
        ex:g {{ ex:law a logic:Formula ; logic:inModule ex:theory ; logic:standpoint ex:observer .
            ex:c a logic:Constraint ; logic:integrity ex:law . }}"
        )
        .as_bytes(),
        "application/trig",
        None,
    )
    .unwrap();
    let prepared = PreparedLogicSource::new(&source).unwrap();
    let law = prepared
        .dataset()
        .term_id_by_iri("https://example.org/law")
        .unwrap();
    let graph = prepared
        .dataset()
        .term_id_by_iri("https://example.org/g")
        .unwrap();
    let index = prepared.source_graph();
    for node in [
        SourceNode {
            term: law,
            graph: None,
        },
        SourceNode {
            term: law,
            graph: Some(graph),
        },
    ] {
        let expected: Vec<_> = index
            .edges()
            .iter()
            .filter(|edge| edge.source == node)
            .collect();
        let actual = index.outgoing(node);
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(
                std::ptr::eq(actual, expected),
                "source lookup must borrow the original edge"
            );
        }
    }
    assert!(!index.formula_is_owned(SourceNode {
        term: law,
        graph: None
    }));
    assert!(index.formula_is_owned(SourceNode {
        term: law,
        graph: Some(graph)
    }));
    assert!(
        index
            .edges()
            .iter()
            .any(|e| e.role == SourceEdgeRole::Module && e.source.graph == Some(graph))
    );
    assert!(
        index
            .edges()
            .iter()
            .any(|e| e.role == SourceEdgeRole::Standpoint && e.source.graph == Some(graph))
    );
    let (program, diagnostics) = prepared.compile(None).unwrap();
    assert_eq!(program.formulas.len(), 1);
    assert!(
        diagnostics.is_empty(),
        "named contexts were not selected as default assertions: {diagnostics:?}"
    );
}

#[test]
fn dangling_formula_uses_and_explicit_boundaries_remain_in_the_inventory() {
    let source = purrdf::parse_dataset(
        format!(
            "{PREFIXES}
        ex:c a logic:Constraint ; logic:integrity ex:missing .
        ex:boundary a logic:ExpressivenessBoundary .
        ex:concept math:definingLaw ex:boundary .
        ex:theory logic:imports ex:importedTheory ."
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .unwrap();
    let prepared = PreparedLogicSource::new(&source).unwrap();
    let node = |name: &str| SourceNode {
        term: prepared
            .dataset()
            .term_id_by_iri(&format!("https://example.org/{name}"))
            .unwrap(),
        graph: None,
    };
    let missing = prepared.source_graph().unit(node("missing")).unwrap();
    assert!(missing.declarations.is_empty());
    assert!(missing.required_kinds.contains(&SourceUnitKind::Formula));
    assert!(
        prepared
            .source_graph()
            .declares(node("boundary"), SourceUnitKind::ExpressivenessBoundary)
    );
    assert!(
        prepared
            .source_graph()
            .edges()
            .iter()
            .any(|e| e.role == SourceEdgeRole::Import)
    );
    let (_, diagnostics) = prepared.compile(None).unwrap();
    assert!(diagnostics.iter().any(|d| d.code == "MALFORMED_CONSTRAINT"));
}

#[test]
fn undeclared_constructor_root_is_visible_and_its_children_are_not_asserted() {
    let (program, diagnostics) = parse(
        "ex:root logic:not ex:atom .
        ex:atom a logic:Formula ; logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .",
    );
    assert!(program.formulas.is_empty());
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == "UNDECLARED_FORMULA_ROOT")
    );
}

#[test]
fn conflicting_term_fields_retain_the_original_formula_anchor() {
    let (_, diagnostics) = parse(
        "ex:bad a logic:Formula ; logic:relation ex:p ; logic:argument
        [ logic:termIndex 0 ; logic:termLiteral \"conflicting value\" ; logic:termIri ex:subject ],
        [ logic:termIndex 1 ; logic:termIri ex:a ] .",
    );
    let failure = diagnostics
        .iter()
        .find(|d| d.code == "MALFORMED_FORMULA")
        .unwrap();
    assert_eq!(failure.subject.as_deref(), Some("https://example.org/bad"));
}

#[test]
fn native_statement_layer_keeps_reifier_module_and_standpoint_without_flattening() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let s = builder.intern_iri("https://example.org/s");
    let p = builder.intern_iri("https://blackcatinformatics.ca/logic/subClassOf");
    let o = builder.intern_iri("https://example.org/o");
    builder.push_quad(s, p, o, None);
    let triple = builder.intern_triple(s, p, o);
    let reifier = builder.intern_blank("claim", purrdf::BlankScope(7));
    let graph = builder.intern_iri("https://example.org/world");
    let module_predicate = builder.intern_iri("https://blackcatinformatics.ca/logic/inModule");
    let standpoint_predicate =
        builder.intern_iri("https://blackcatinformatics.ca/logic/standpoint");
    let module = builder.intern_iri("https://example.org/theory");
    let observer = builder.intern_iri("https://example.org/observer");
    builder.push_reifier_in_graph(reifier, triple, Some(graph));
    builder.push_annotation_in_graph(reifier, module_predicate, module, Some(graph));
    builder.push_annotation_in_graph(reifier, standpoint_predicate, observer, Some(graph));
    let source = builder.freeze().unwrap();
    let prepared = PreparedLogicSource::new(&source).unwrap();
    let source_node = SourceNode {
        term: prepared.source_term(reifier).unwrap(),
        graph: prepared.source_term(graph),
    };
    let edges: Vec<_> = prepared
        .source_graph()
        .edges()
        .iter()
        .filter(|edge| edge.source == source_node)
        .collect();
    let reification = edges
        .iter()
        .find(|edge| edge.carrier == SourceCarrier::Reifier)
        .unwrap();
    assert_eq!(reification.predicate.iri(prepared.dataset()), RDF_REIFIES);
    assert_eq!(reification.target, prepared.source_term(triple).unwrap());
    assert!(
        edges
            .iter()
            .any(|edge| edge.carrier == SourceCarrier::Annotation
                && edge.role == SourceEdgeRole::Module
                && edge.target == prepared.source_term(module).unwrap())
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.carrier == SourceCarrier::Annotation
                && edge.role == SourceEdgeRole::Standpoint
                && edge.target == prepared.source_term(observer).unwrap())
    );
    assert!(
        prepared
            .source_graph()
            .unit(SourceNode {
                term: reification.target,
                graph: source_node.graph
            })
            .unwrap()
            .required_kinds
            .contains(&SourceUnitKind::QuotedStatement)
    );
}

#[test]
fn one_invalid_use_in_two_native_carriers_is_one_defect_with_both_origins() {
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let owner = builder.intern_iri("https://example.org/undeclared");
    let predicate = builder.intern_iri("https://blackcatinformatics.ca/logic/integrity");
    let formula = builder.intern_iri("https://example.org/law");
    builder.push_quad(owner, predicate, formula, None);
    builder.push_annotation(owner, predicate, formula);
    let dataset = builder.freeze().unwrap();
    let graph = StructuralSourceGraph::new(&dataset);
    assert_eq!(
        graph.edges().len(),
        2,
        "both physical source positions are retained"
    );
    let failures = graph.ownership_diagnostics(&dataset);
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].code, "UNDECLARED_SEMANTIC_OWNER");
}
