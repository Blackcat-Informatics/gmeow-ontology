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
        s: TermValue::iri("urn:statement-subject").into(),
        p: TermValue::iri("urn:statement-predicate").into(),
        o: directional.clone().into(),
    };
    let direct_blank = TermValue::Blank {
        label: "shared".into(),
        scope: purrdf::BlankScope::DEFAULT,
    };
    let composite_blank = TermValue::Literal {
        lexical_form: "[_:shared]".into(),
        datatype: "http://w3id.org/awslabs/neptune/SPARQL-CDTs/List".into(),
        language: None,
        direction: None,
    };
    let axioms = [directional, quoted, direct_blank, composite_blank]
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
        request: "urn:purrdf:rdfc:request".into(),
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
/// Caller-owned reserved-looking IRIs and shared direct/CDT blanks survive GTS.
#[test]
fn contextual_resource_components_keep_proof_witness_and_native_receipts() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    let (ordinary, _receipt) = project_reasoning_dataset(&assessment.result).unwrap();
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
        let restored =
            parse_reasoning_dataset(dataset, GraphMatch::Default, &NoNativeReceipts).unwrap();
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
        .as_ref()
        .term_id_by_value(&TermValue::iri(GRAPH_REASONING))
        .unwrap();
    let restored =
        parse_reasoning_dataset(&named, GraphMatch::Named(graph), &NoNativeReceipts).unwrap();
    assert_eq!(restored.provenance, assessment.result.provenance);
    assert_eq!(
        restored.inferred().iter().collect::<BTreeSet<_>>(),
        assessment.result.inferred().iter().collect::<BTreeSet<_>>()
    );
    assert!(parse_reasoning_dataset(&named, GraphMatch::Default, &NoNativeReceipts).is_err());
    let artifact = project_contextual_artifact(&assessment)
        .expect("the contextual result publishes its complete GTS carrier");
    gmeow_gts_profile::validate_mandated_frames(&artifact.bytes)
        .expect("the contextual carrier uses mandated GTS frames");
    let admitted = admit_contextual_artifact(&artifact.bytes, &assessment.request)
        .expect("the exact request selects its admitted contextual result");
    let graph = admitted
        .as_ref()
        .term_id_by_value(&TermValue::iri(GRAPH_REASONING))
        .expect("the contextual result retains its named graph");
    let restored = parse_reasoning_dataset(&admitted, GraphMatch::Named(graph), &NoNativeReceipts)
        .expect("the admitted contextual carrier retains its result");
    assert_eq!(restored.provenance, assessment.result.provenance);
    assert_eq!(restored.information, assessment.result.information);
    assert!(admit_contextual_artifact(&artifact.bytes, "urn:missing:request").is_err());
    assert!(parse_reasoning_artifact_graph(&artifact.bytes, Some(GRAPH_REASONING)).is_err());
    for graph in [GRAPH_REASONING, "urn:wrong:contextual-graph"] {
        let (mut sink, subject) = contextual_assessment_sink(&assessment)
            .expect("the synthetic contextual carrier is valid before perturbation");
        if graph == GRAPH_REASONING {
            sink.push(
                Node::iri(&assessment.request),
                logic("contextualResult"),
                Node::iri(format!("{subject}/another-result")),
            );
        }
        let mut builder = purrdf::RdfDatasetBuilder::new();
        append_native_dataset(sink, &mut builder, Some(graph), purrdf::BlankScope::DEFAULT)
            .expect("the perturbed carrier remains valid RDF");
        let dataset = builder.freeze().expect("the perturbed carrier freezes");
        // gmeow-test-input: synthetic-only
        let altered = gmeow_gts_profile::GmeowGtsWriter::new("runtime")
            .finish_view_scoped(dataset.as_ref(), "contextual-selection-test")
            .expect("the tiny perturbed carrier has valid governed frames");
        assert!(
            admit_contextual_artifact(&altered.bytes, &assessment.request).is_err(),
            "a wrong graph or ambiguous request cannot select a contextual result"
        );
    }
}

/// A selected evidence link cannot silently disappear merely because its
/// object is a literal, quotation or a resource missing the declared role.
/// A contextual result also refuses native attachments and independent aggregate selection.
#[test]
fn contextual_resource_components_refuse_nonresources_and_untyped_targets() {
    // gmeow-test-input: synthetic-only
    let assessment = component_assessment();
    let mut forbidden = component_assessment();
    forbidden.result = super::tests::native_conflict_result(None);
    let Err(error) = contextual_assessment_sink(&forbidden) else {
        panic!("a contextual boundary rejects outer native execution before projection");
    };
    assert!(
        error
            .message()
            .contains("cannot carry native execution evidence")
    );
    let (sink, subject) =
        contextual_assessment_sink(&assessment).expect("admitted contextual assessment");
    let contextual = native_dataset(sink).expect("unmodified contextual dataset");
    parse_reasoning_dataset(&contextual, GraphMatch::Default, &NoNativeReceipts)
        .expect("the unmodified contextual assessment is valid as a contextual result");
    parse_reasoning_dataset_subject(
        &contextual,
        GraphMatch::Default,
        &subject,
        &NoNativeReceipts,
    )
    .expect_err("a valid contextual child cannot be selected as an artifact aggregate");
    for predicate in ["resultNativeExecution", "resultClosureBlob"] {
        let (mut sink, subject) = contextual_assessment_sink(&assessment)
            .expect("admitted contextual assessment before forbidden attachment");
        sink.push(
            Node::iri(&subject),
            logic(predicate),
            Node::string(format!("blake3:{}", "0".repeat(64))),
        );
        let dataset = native_dataset(sink)
            .expect("contextual dataset with a syntactically valid forbidden attachment");
        let error = parse_reasoning_dataset(&dataset, GraphMatch::Default, &NoNativeReceipts)
            .expect_err("contextual projections must reject native attachments");
        assert!(
            error
                .message()
                .contains("contextual projection cannot carry"),
            "{predicate}: {error}"
        );
        assert!(
            parse_reasoning_dataset_subject(
                &dataset,
                GraphMatch::Default,
                &subject,
                &NoNativeReceipts,
            )
            .is_err(),
            "a contextual child cannot become an artifact aggregate"
        );
    }
    for predicate in [
        "resultProof",
        "resultCounterproof",
        "resultContradiction",
        "resultDerivedAxiom",
    ] {
        for replacement in [
            Node::string("not a component address"),
            Node::Value(Box::new(TermValue::Triple {
                s: TermValue::iri("urn:s").into(),
                p: TermValue::iri("urn:p").into(),
                o: TermValue::iri("urn:o").into(),
            })),
            Node::iri("urn:untyped-component"),
        ] {
            let (mut sink, _) =
                contextual_assessment_sink(&assessment).expect("admitted contextual components");
            sink.triples
                .iter_mut()
                .find(|row| row.predicate.as_ref() == logic(predicate))
                .expect("the assessment declares the selected evidence component link")
                .object = replacement;
            let dataset = native_dataset(sink)
                .expect("contextual dataset with a syntactically valid replacement term");
            let error = parse_reasoning_dataset(&dataset, GraphMatch::Default, &NoNativeReceipts)
                .expect_err("evidence links must reject nonresources and untyped targets");
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
        .find(|row| row.predicate.as_ref() == logic("resultProof"))
        .unwrap()
        .object
        .to_string();
    let position = sink
        .triples
        .iter()
        .position(|row| row.subject.to_string() == proof && row.predicate.as_ref() == RDF_TYPE)
        .unwrap();
    let role = sink.triples.remove(position);
    let mut foreign = Sink::default();
    foreign.push(role.subject, role.predicate, role.object);
    let mut lines = sink.render_lines_in_graph(Some(GRAPH_REASONING));
    lines.extend(foreign.render_lines_in_graph(Some("urn:unselected-graph")));
    let dataset =
        purrdf::parse_dataset(lines.join("\n").as_bytes(), "application/n-quads", None).unwrap();
    let graph = dataset
        .as_ref()
        .term_id_by_value(&TermValue::iri(GRAPH_REASONING))
        .unwrap();
    let error =
        parse_reasoning_dataset(&dataset, GraphMatch::Named(graph), &NoNativeReceipts).unwrap_err();
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
                .retain(|row| row.predicate.as_ref() != logic("derivationId")),
            1 => {
                let subject = sink
                    .triples
                    .iter()
                    .find(|row| row.predicate.as_ref() == logic("resultProof"))
                    .unwrap()
                    .subject
                    .clone();
                sink.push(subject, logic("resultProof"), Node::iri("urn:second-proof"));
            }
            2 => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate.as_ref() == logic("citesIri"))
                    .unwrap()
                    .object = Node::string("urn:literal-citation")
            }
            3 => sink
                .triples
                .retain(|row| row.predicate.as_ref() != logic("witnessWorld")),
            4 => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate.as_ref() == logic("witnessPremise"))
                    .unwrap()
                    .object = Node::string("missing-components")
            }
            _ => {
                sink.triples
                    .iter_mut()
                    .find(|row| row.predicate.as_ref() == logic("derivationId"))
                    .unwrap()
                    .object = Node::Value(Box::new(TermValue::Literal {
                    lexical_form: "urn:proof".into(),
                    datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
                    language: Some("en".into()),
                    direction: None,
                }))
            }
        }
        let dataset = native_dataset(sink).unwrap();
        let error =
            parse_reasoning_dataset(&dataset, GraphMatch::Default, &NoNativeReceipts).unwrap_err();
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
