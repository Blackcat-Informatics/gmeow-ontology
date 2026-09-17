// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! GMEOW result-component roles survive contextual naming without losing evidence.

use super::*;
use purrdf::{RdfTextDirection, TermValue};

/// An explicit synthetic result exercising each linked component role. This
/// tests the projection contract; it makes no claim to execute a source theory.
fn component_assessment() -> crate::contextual::ContextualAssessment {
    let mut provenance = ResultProvenance::native("contract:resource-components", "urn:world");
    provenance.query = "urn:formula".into();
    provenance.conclusion = "urn:formula".into();
    provenance.context.standpoint = Some("urn:standpoint".into());
    provenance.context.attributed = Some("urn:context".into());
    provenance.context.path = Some("urn:path".into());
    provenance.context.time = Some("2026-09-11T00:00:00Z".into());
    provenance.proof = Some(DerivationRef {
        derivation_id: "urn:proof".into(),
        cited_iris: BTreeSet::from(["urn:support".into(), "urn:source".into()]),
    });
    provenance.counterproof = Some(DerivationRef {
        derivation_id: "urn:counterproof".into(),
        cited_iris: BTreeSet::from(["urn:opposition".into()]),
    });
    provenance
        .contradiction_witnesses
        .push(ContradictionWitness {
            individual: "urn:individual".into(),
            world: "urn:world".into(),
            premises: vec![(
                "urn:individual".into(),
                "urn:predicate".into(),
                "literal with spaces".into(),
            )],
        });
    provenance.consumed_budget = crate::result::BudgetUsage {
        consumed: 7,
        allowance: Some(11),
        limit: None,
    };
    provenance.certified_fragment = Some(crate::contextual::FRAGMENT.into());
    provenance.projection_class = PreservationClaim::exact();
    provenance.assumptions.insert(Assumption::OpenWorld);
    let directional = TermValue::Literal {
        lexical_form: "complete evidence".into(),
        datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into(),
        language: Some("ar".into()),
        direction: Some(RdfTextDirection::Rtl),
    };
    let quoted = TermValue::Triple {
        s: Box::new(TermValue::iri("urn:statement-subject")),
        p: Box::new(TermValue::iri("urn:statement-predicate")),
        o: Box::new(directional.clone()),
    };
    let axioms = [directional, quoted]
        .into_iter()
        .map(|object| InferredAxiom {
            modal_evaluation: None,
            subject: "urn:derived-subject".into(),
            predicate: "urn:derived-predicate".into(),
            object,
            world: "urn:world".into(),
            is_edb: false,
            rule_name: Some("urn:rule:synthetic-receipt".into()),
            premises: vec![(
                "urn:source".into(),
                "urn:premise-predicate".into(),
                "urn:premise-object".into(),
            )],
        })
        .collect();
    crate::contextual::ContextualAssessment {
        request: "urn:request".into(),
        formula: "urn:formula".into(),
        result: ReasoningResult::new(
            InputStatus::Valid,
            EvaluationStatus::Completed,
            CompletenessStatus::CompleteForFragment,
            PreservationClaim::exact(),
            InformationState::Both,
            provenance,
            ResultPayload::Inferred(axioms),
        ),
        interrupted: None,
        inferences: Vec::new(),
        anchors: Vec::new(),
        temporal_prefixes: Vec::new(),
        native_evidence: Vec::new(),
        diagnostics: gmeow_errors::DiagLedger::new(),
    }
}

/// Ordinary blank components and contextual IRI components use the same role
/// reader, preserving complete provenance and verified native axiom receipts.
#[test]
fn contextual_resource_components_keep_proof_witness_and_native_receipts() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    let ordinary = project_reasoning_dataset(&assessment.result).unwrap();
    let contextual = project_contextual_assessment_dataset(&assessment).unwrap();
    assert!(contextual.owned_quads().any(
        |row| matches!(row.subject, purrdf::RdfTerm::Iri(ref iri) if iri.contains("/component/"))
    ));
    assert!(
        contextual
            .owned_quads()
            .all(|row| !matches!(row.subject, purrdf::RdfTerm::BlankNode(_)))
    );
    for dataset in [&ordinary, &contextual] {
        let restored = parse_reasoning_dataset(dataset, GraphMatch::Default).unwrap();
        assert_eq!(restored.provenance, assessment.result.provenance);
        assert_eq!(restored.information, InformationState::Both);
        assert_eq!(
            restored.inferred().iter().collect::<BTreeSet<_>>(),
            assessment.result.inferred().iter().collect::<BTreeSet<_>>()
        );
    }
    let output = project_contextual_dataset(&assessment).expect("admitted contextual projection");
    let named = purrdf::parse_dataset(output.as_bytes(), "application/n-quads", None).unwrap();
    let graph = named
        .term_id_by_value(&TermValue::iri(GRAPH_REASONING))
        .unwrap();
    let restored = parse_reasoning_dataset(&named, GraphMatch::Named(graph)).unwrap();
    assert_eq!(restored.provenance, assessment.result.provenance);
    assert_eq!(
        restored.inferred().iter().collect::<BTreeSet<_>>(),
        assessment.result.inferred().iter().collect::<BTreeSet<_>>()
    );
    assert!(parse_reasoning_dataset(&named, GraphMatch::Default).is_err());
}

/// A selected evidence link cannot silently disappear merely because its
/// object is a literal, quotation or a resource missing the declared role.
#[test]
fn contextual_resource_components_refuse_nonresources_and_untyped_targets() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    for predicate in [
        "resultProof",
        "resultCounterproof",
        "resultContradiction",
        "resultDerivedAxiom",
    ] {
        for replacement in [
            Node::string("not a component address"),
            Node::Value(TermValue::Triple {
                s: Box::new(TermValue::iri("urn:s")),
                p: Box::new(TermValue::iri("urn:p")),
                o: Box::new(TermValue::iri("urn:o")),
            }),
            Node::iri("urn:untyped-component"),
        ] {
            let (mut sink, _) =
                contextual_assessment_sink(&assessment).expect("admitted contextual components");
            sink.triples
                .iter_mut()
                .find(|row| row.predicate == logic(predicate))
                .unwrap()
                .object = replacement;
            let dataset = native_dataset(sink).unwrap();
            let error = parse_reasoning_dataset(&dataset, GraphMatch::Default).unwrap_err();
            assert!(error.message().contains(predicate), "{predicate}: {error}");
        }
    }
}

/// The selected named graph must carry its component's role. A correct type in
/// another graph does not authorize reading an incomplete proof component.
#[test]
fn contextual_resource_components_do_not_borrow_a_role_from_another_graph() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    let (mut sink, _) =
        contextual_assessment_sink(&assessment).expect("admitted contextual components");
    let proof = sink
        .triples
        .iter()
        .find(|row| row.predicate == logic("resultProof"))
        .unwrap()
        .object
        .render();
    let position = sink
        .triples
        .iter()
        .position(|row| row.subject.render() == proof && row.predicate == RDF_TYPE)
        .unwrap();
    let role = sink.triples.remove(position);
    let mut foreign = Sink::default();
    foreign.push(role.subject, role.predicate, role.object);
    let mut lines = sink.render_lines_in_graph(Some(GRAPH_REASONING));
    lines.extend(foreign.render_lines_in_graph(Some("urn:unselected-graph")));
    let dataset =
        purrdf::parse_dataset(lines.join("\n").as_bytes(), "application/n-quads", None).unwrap();
    let graph = dataset
        .term_id_by_value(&TermValue::iri(GRAPH_REASONING))
        .unwrap();
    let error = parse_reasoning_dataset(&dataset, GraphMatch::Named(graph)).unwrap_err();
    assert!(
        error
            .message()
            .contains("typed logic:Derivation in the selected graph"),
        "{error}"
    );
}

/// Component bodies retain their complete typed fields; neither a malformed
/// body nor competing proof links can degrade to an absent proof or witness.
#[test]
fn contextual_resource_components_refuse_missing_ambiguous_or_erased_fields() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    for mutation in 0..6 {
        let (mut sink, _) =
            contextual_assessment_sink(&assessment).expect("admitted contextual components");
        match mutation {
            0 => sink
                .triples
                .retain(|row| row.predicate != logic("derivationId")),
            1 => {
                let subject = sink
                    .triples
                    .iter()
                    .find(|row| row.predicate == logic("resultProof"))
                    .unwrap()
                    .subject
                    .clone();
                sink.push(subject, logic("resultProof"), Node::iri("urn:second-proof"));
            }
            2 => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate == logic("citesIri"))
                    .unwrap()
                    .object = Node::string("urn:literal-citation")
            }
            3 => sink
                .triples
                .retain(|row| row.predicate != logic("witnessWorld")),
            4 => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate == logic("witnessPremise"))
                    .unwrap()
                    .object = Node::string("missing-components")
            }
            _ => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate == logic("derivationId"))
                    .unwrap()
                    .object = Node::Value(TermValue::Literal {
                    lexical_form: "urn:proof".into(),
                    datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
                    language: Some("en".into()),
                    direction: None,
                })
            }
        }
        let dataset = native_dataset(sink).unwrap();
        let error = parse_reasoning_dataset(&dataset, GraphMatch::Default).unwrap_err();
        let expected = [
            "derivationId",
            "single-valued",
            "citesIri",
            "witnessWorld",
            "three source components",
            "xsd:string",
        ][mutation];
        assert!(
            error.message().contains(expected),
            "mutation {mutation}: {error}"
        );
    }
}
