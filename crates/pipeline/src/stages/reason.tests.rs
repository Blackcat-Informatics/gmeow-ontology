// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use purrdf::RdfTerm;

use super::*;
use crate::bundle::bundle_from_artifacts_over;

#[test]
fn object_level_domains_keep_exact_roles_across_data_edits() {
    let stage = ReasonStage::new();
    let upstream = |digest: &str| {
        stage
            .consumes()
            .iter()
            .map(|producer| (producer.clone(), StageProduct::new(producer, digest)))
            .collect::<BTreeMap<_, _>>()
    };
    let before = upstream("source-before");
    let after = upstream("source-after");
    fn input(upstream: &BTreeMap<String, StageProduct>) -> StageInput<'_> {
        StageInput {
            root: std::path::Path::new("."),
            upstream,
        }
    }
    let selected = object_level_domains(&stage, &input(&before)).unwrap();
    assert_eq!(
        selected,
        object_level_domains(&stage, &input(&after)).unwrap()
    );
    let actual: BTreeSet<_> = selected
        .worlds()
        .iter()
        .map(|domain| domain.graph().clone())
        .collect();
    let expected: BTreeSet<_> = std::iter::once(LogicalGraph::Default)
        .chain(
            gmeow_logic::reasoning_graphs::OBJECT_LEVEL_NAMED_GRAPHS
                .into_iter()
                .map(|graph| LogicalGraph::Named(purrdf::TermValue::iri(graph))),
        )
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        actual.len(),
        9,
        "required empty theories remain selected before assembly"
    );
    assert!(
        !actual.contains(&LogicalGraph::Named(purrdf::TermValue::iri(
            crate::stages::carrier::GRAPH_DIAGNOSTICS,
        )))
    );
    let mut missing = upstream("missing-source");
    missing.remove("stage-source-load");
    assert!(object_level_domains(&stage, &input(&missing)).is_err());
    let mut wrong = upstream("wrong-owner");
    wrong.get_mut("stage-statements").unwrap().stage_id = "stage-diagnostics".to_owned();
    assert!(object_level_domains(&stage, &input(&wrong)).is_err());
    let mut released = upstream("released-source");
    released
        .get_mut("stage-source-load")
        .unwrap()
        .carrier_released = true;
    assert!(object_level_domains(&stage, &input(&released)).is_err());
}

#[test]
fn explicit_theory_selection_does_not_mint_support_graph_domains() {
    let source = purrdf::parse_dataset(
        format!(
            "<urn:selected-theory> {{}} <{}> {{ <urn:report> <urn:message> \"retained\" . }}",
            crate::stages::carrier::GRAPH_DIAGNOSTICS,
        )
        .as_bytes(),
        "application/trig",
        None,
    )
    .unwrap();
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(purrdf::TermValue::iri("urn:selected-theory")),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.pipeline.test.selected-theory.v1".to_owned(),
        *blake3::hash(
            b"gmeow.pipeline.test.selected-theory.v1/urn:selected-theory/nonempty-object-domain-v1",
        )
        .as_bytes(),
    )
    .unwrap()])
    .unwrap();
    let reasoned = reason_over_dataset(&source, &domains).unwrap();
    let native = reasoned.result.native_execution().unwrap();
    assert_eq!(native.selected_domains, domains);
    assert_eq!(
        native.class_admission.source_worlds[crate::stages::carrier::GRAPH_DIAGNOSTICS].assertions,
        1
    );
    assert!(!native.witness_derivations.is_empty());
    assert!(
        native
            .witness_derivations
            .iter()
            .all(|witness| witness.scope.world == "urn:selected-theory")
    );
    assert!(native.class_admission.source_worlds["urn:selected-theory"].assertions == 0);
}

#[test]
fn selected_blank_graph_cannot_silently_split_at_pipeline_relabelling() {
    let source = purrdf::parse_dataset(
        b"_:theory { <urn:a> <urn:p> <urn:b> . }",
        "application/trig",
        None,
    )
    .unwrap();
    let input = prepare_reasoning_input(source.as_ref()).unwrap();
    let graph = input
        .source_contexts()
        .values()
        .find_map(|graph| match graph {
            Some(graph @ purrdf::TermValue::Blank { .. }) => Some(graph.clone()),
            _ => None,
        })
        .unwrap();
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(graph),
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.pipeline.test.blank-theory.v1".to_owned(),
        [3; 32],
    )
    .unwrap()])
    .unwrap();
    let error = reason_over_dataset(&source, &domains)
        .err()
        .expect("issuer mapping is mandatory");
    assert!(error.to_string().contains("issuer mapping"));
}

#[test]
fn reason_product_reuses_the_already_projected_native_carrier() {
    let reasoned = reason_artifacts(br#"
<urn:projection:a> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:projection:b> <urn:projection:world> .
<urn:projection:b> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:projection:c> <urn:projection:world> .
"#).unwrap();
    let native = Arc::clone(&reasoned.dataset);
    let product = reason_product_from_artifacts(reasoned).unwrap();
    assert!(
        std::ptr::eq(product.dataset(), native.as_ref()),
        "the product must retain the native carrier instead of reparsing or copying it"
    );
}

#[test]
fn reason_produces_nonempty_artifacts_over_tiny_graph() {
    let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
    let reasoned = reason_artifacts(nq).expect("reason");

    // Wiring check: the native reasoner ran end-to-end and the three
    // builders produced their artifacts (each carries at least its generated
    // header), and the closure contains a concrete derived transitive
    // subclass axiom.
    for (name, ttl) in [
        ("closure", &reasoned.closure),
        ("explanations", &reasoned.explanations),
        ("ledger", &reasoned.ledger),
        ("perf_ledger", &reasoned.perf_ledger),
    ] {
        assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
    }
    assert!(reasoned.closure.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
    assert!(reasoned.chase_report.findings.iter().any(|finding| {
        finding.code == "reason.native-contract"
            && finding
                .message
                .contains(&gmeow_logic::reason::native_contract_hash())
    }));
    // The perf ledger flags the deferred / non-incremental levers (static content).
    assert!(
        reasoned
            .perf_ledger
            .contains("https://blackcatinformatics.ca/gmeow/FlaggedNonIncremental"),
        "the perf ledger flags the non-incremental hard parts"
    );
}

// The synthetic in-crate fold test that hand-built an `example.org` existential EDB
// and asserted the certificate's free-text edge message was RETIRED (AC3):
// its fold demonstration is now the non-vacuous golden over REAL sources
// (`tests/chase_certificate_golden.rs`, structured), the witness projection is
// covered by `invented_witness_skeletons_land_in_diagnostics_and_certificate_cites_them`
// below, and `finding_nodes` node-count rendering by `diag_render`'s own tests.

#[test]
fn invented_witness_skeletons_land_in_diagnostics_and_certificate_cites_them() {
    // A `C ⊑ ∃p.D` obligation on an individual `x:R` mints exactly one chase
    // witness null. The stage projects its minting head quad p(x, null) into
    // graph/diagnostics as standard RDF reification + types the null a
    // gmeow:InventedWitness, and the weakly-acyclic certificate finding cites
    // the null-minting reifier through gmeow:findingDerivedFromQuad.
    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RDF_OBJECT: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#object";
    const INVENTED_WITNESS: &str = "https://blackcatinformatics.ca/gmeow/InventedWitness";
    const EXISTENTIAL_ORDINAL: &str = "https://blackcatinformatics.ca/gmeow/existentialOrdinal";
    const VIA_RULE: &str = "https://blackcatinformatics.ca/gmeow/viaRule";

    let nq = br#"
<http://example.org/R> <http://www.w3.org/2002/07/owl#onProperty> <http://example.org/p> <http://gmeow.example/w> .
<http://example.org/R> <http://www.w3.org/2002/07/owl#someValuesFrom> <http://example.org/D> <http://gmeow.example/w> .
<http://example.org/x> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.org/R> <http://gmeow.example/w> .
"#;
    let reasoned = reason_artifacts(nq).expect("production existential reason");
    assert!(
        !reasoned
            .result
            .native_execution()
            .unwrap()
            .witness_derivations
            .is_empty(),
        "the existential obligation must mint at least one witness"
    );
    let dataset = Arc::clone(&reasoned.dataset);
    let diagnostics = dataset.project_named_graph(crate::stages::carrier::GRAPH_DIAGNOSTICS);
    let quads: Vec<_> = diagnostics.owned_quads().collect();

    let iri = |term: &RdfTerm| match term {
        RdfTerm::Iri(iri) => Some(iri.clone()),
        _ => None,
    };

    // (1) The source obligation's value null is projected as InventedWitness.
    // The same run also retains its intrinsic nonempty-domain witness; that
    // subject-only head must not replace the property/filler control below.
    let witness = reasoned
        .result
        .native_execution()
        .unwrap()
        .witness_derivations
        .iter()
        .find(|witness| {
            witness
                .heads
                .iter()
                .any(|head| head.statement.predicate == "http://example.org/p")
        })
        .expect("the authored source obligation minted its property witness")
        .witness
        .clone();
    assert!(
        quads.iter().any(|quad| {
            quad.subject == RdfTerm::iri(&witness)
                && quad.predicate == RDF_TYPE
                && matches!(&quad.object, RdfTerm::Iri(iri) if iri == INVENTED_WITNESS)
        }),
        "the source witness is projected into graph/diagnostics"
    );

    // (2) that witness carries gmeow:existentialOrdinal.
    assert!(
        quads.iter().any(|quad| {
            iri(&quad.subject).as_deref() == Some(witness.as_str())
                && quad.predicate == EXISTENTIAL_ORDINAL
        }),
        "the invented witness must carry its gmeow:existentialOrdinal"
    );

    // (3) a reifier whose rdf:object IS the null AND that carries gmeow:viaRule.
    let reifier = quads
        .iter()
        .find(|quad| {
            quad.predicate == RDF_OBJECT
                && matches!(&quad.object, RdfTerm::Iri(iri) if iri == &witness)
        })
        .and_then(|quad| iri(&quad.subject))
        .expect("a head-quad reifier with rdf:object = <null> is present");
    assert!(
        quads.iter().any(|quad| {
            iri(&quad.subject).as_deref() == Some(reifier.as_str()) && quad.predicate == VIA_RULE
        }),
        "the null-minting reifier must carry gmeow:viaRule"
    );

    // (4) the certificate finding rehydrated via the offline reader carries a
    // non-empty derived_from_quads (the null-minting reifier it cites).
    let index = crate::diagnostics_reader::read_findings(&dataset)
        .expect("rehydrate the diagnostics graph");
    let certificate = index
        .findings
        .values()
        .find(|finding| finding.code == "chase.certificate.weakly-acyclic")
        .expect("the weakly-acyclic certificate finding rehydrates");
    assert!(
        !certificate.derived_from_quads.is_empty(),
        "the certificate must cite its null-minting reifiers via findingDerivedFromQuad: {certificate:?}"
    );
    assert!(
        certificate.derived_from_quads.contains(&reifier),
        "the certificate must cite the head-quad reifier whose object is the null"
    );
}

#[test]
fn corpus_contextual_assessments_ship_in_the_reasoning_graph_with_the_aggregate_handle() {
    let source = br#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <urn:contextual-artifact:> .
ex:context a logic:AttributedContext ; logic:contextWorld ex:world ;
  logic:contextStandpoint ex:observer ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:request a logic:ContextualEvaluationRequest ;
  logic:queryFormula ex:formula ; logic:queryContext ex:context .
ex:formula a logic:Formula ; logic:relation ex:predicate ;
  logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ],
                 [ logic:termIndex 1 ; logic:termIri ex:value ] .
ex:world {
  ex:reportedBy <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> gmeow:accordingTo .
  ex:claim rdf:reifies <<( ex:item ex:predicate ex:value )>> ;
    ex:reportedBy ex:observer ; gmeow:standpointSupportStatus gmeow:supportSupported .
}
"#;
    let source = purrdf::parse_dataset(source, "application/trig", None).unwrap();
    let reasoned = reason_test_dataset(&source).expect("synthetic contextual reasoning");
    let gates = crate::fixture::authenticated_reasoned_gates(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    )
    .expect("selected producer-prepared native laws");
    let verification = gmeow_logic::verify::PreparedVerification::new(&[], &gates).unwrap();
    let gmeow_logic::verify::ReasonedGraphOutcome::Ready(verified) = verification
        .materialize_reasoned_graph(&source, &reasoned.result)
        .expect("the downstream verifier consumes the contextual result")
    else {
        panic!("the finite contextual example has no DL gap");
    };
    assert!(verified.dataset.owned_quads().any(|quad| {
        quad.subject == RdfTerm::iri("urn:contextual-artifact:request")
            && quad.predicate == "https://blackcatinformatics.ca/logic/contextualResult"
    }));
    let dataset = &reasoned.dataset;
    let graph = dataset.project_named_graph(GRAPH_REASONING);
    assert!(
        graph.owned_reifiers().next().is_some(),
        "native source receipts retain their quoted conclusions"
    );
    assert!(
        graph.owned_annotations().next().is_some(),
        "native source receipt annotations follow their graph"
    );
    assert!(
        graph.owned_quads().any(|quad| {
            quad.subject == RdfTerm::iri("urn:contextual-artifact:request")
                && quad.predicate == "https://blackcatinformatics.ca/logic/contextualResult"
        }),
        "the result link is directly queryable in graph/reasoning"
    );
    let restored =
        gmeow_logic::result_rdf::parse_reasoning_dataset(&graph, purrdf::GraphMatch::Default)
            .unwrap();
    assert_eq!(restored.provenance, reasoned.result.provenance);
    assert!(
        dataset.owned_quads().all(|quad| {
            quad.subject != RdfTerm::iri("urn:contextual-artifact:item")
                || quad.predicate != "urn:contextual-artifact:predicate"
        }),
        "projection cannot assert the attributed proposition"
    );
}

#[test]
fn reason_stage_pins_a_reasoning_handle_to_graph_reasoning() {
    // The dual-carriage dataset folds the graph/reasoning projection as a named
    // graph and the typed handle pins to it (the digest invariant must hold).
    let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
    let reasoned = reason_artifacts(nq).expect("reason");
    let dataset = Arc::clone(&reasoned.dataset);
    let mut bundle = bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    let pinned = bundle.graph_digest(GRAPH_REASONING);
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(reasoned.result.clone())),
            pinned,
        )
        .expect("pin Reasoning handle to its backing graph");
    let entry = bundle.handle(GRAPH_REASONING).expect("handle attached");
    let PipelineHandle::Reasoning(r) = &entry.payload else {
        panic!("the handle arm is Reasoning");
    };
    assert_eq!(
        r.as_ref(),
        &reasoned.result,
        "the typed result is carried verbatim"
    );
    // The graph/reasoning named graph is non-empty (the projection landed).
    assert_ne!(
        bundle.graph_digest(GRAPH_REASONING),
        bundle.graph_digest("https://blackcatinformatics.ca/gmeow/graph/absent"),
        "graph/reasoning carries the projection"
    );
    assert_ne!(
        bundle.graph_digest(crate::stages::carrier::GRAPH_DIAGNOSTICS),
        bundle.graph_digest("https://blackcatinformatics.ca/gmeow/graph/absent"),
        "graph/diagnostics always carries the run's native contract evidence"
    );
}

#[test]
fn reason_product_projects_modal_verdict_and_counterexample_into_stage_reason() {
    let nq = br#"
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/necessarily> <https://example.org/modal/B> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/overAccessibility> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalEvalWorld> <https://example.org/modal/w0> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomSubject> <https://example.org/modal/a> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomPredicate> <https://example.org/modal/knows> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomObject> <https://example.org/modal/b> <https://example.org/modal/frame> .
<https://example.org/modal/w0> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/w1> <https://example.org/modal/frame> .
<https://example.org/modal/w0> <https://blackcatinformatics.ca/logic/epistemicallyPossible> <https://example.org/modal/w2> <https://example.org/modal/frame> .
<https://example.org/modal/a> <https://example.org/modal/knows> <https://example.org/modal/b> <https://example.org/modal/w1> .
"#;
    let reasoned = reason_artifacts(nq).expect("reason modal frame");
    let modal = reasoned
        .result
        .inferred()
        .iter()
        .find(|axiom| axiom.predicate == "https://blackcatinformatics.ca/logic/modalNecessityFails")
        .expect("typed production result carries the modal verdict");
    assert_eq!(modal.world, "https://example.org/modal/frame");
    assert_eq!(modal.subject, "https://example.org/modal/F");
    assert_eq!(modal.object.as_iri(), Some("https://example.org/modal/B"));
    assert_eq!(
        modal.rule_name.as_deref(),
        Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
    );
    let expected = gmeow_logic::modal::ModalEvaluation {
        context: "https://example.org/modal/frame".into(),
        formula: "https://example.org/modal/F".into(),
        operator: gmeow_logic::modal::ModalOp::Box,
        body: "https://example.org/modal/B".into(),
        evaluation_world: "https://example.org/modal/w0".into(),
        accessibility_relation: "https://blackcatinformatics.ca/logic/epistemicallyPossible".into(),
        atom_subject: "https://example.org/modal/a".into(),
        atom_predicate: "https://example.org/modal/knows".into(),
        atom_object: "https://example.org/modal/b".into(),
        frontier: gmeow_logic::modal::ModalFrontier::CompletedFinitePredecessor {
            worlds: vec![
                gmeow_logic::modal::ModalWorldEvidence {
                    world: "https://example.org/modal/w1".into(),
                    atom_present: true,
                },
                gmeow_logic::modal::ModalWorldEvidence {
                    world: "https://example.org/modal/w2".into(),
                    atom_present: false,
                },
            ],
        },
        conclusion_predicate: "https://blackcatinformatics.ca/logic/modalNecessityFails".into(),
        conclusion_object: "https://example.org/modal/B".into(),
    };
    assert_eq!(modal.modal_evaluation.as_deref(), Some(&expected));
    let premises: Vec<_> = expected
        .positive_premises()
        .into_iter()
        .map(|p| (p.subject, p.predicate, p.object))
        .collect();
    assert_eq!(modal.premises, premises);
    assert_eq!(
        modal.premises.len(),
        9,
        "frame declarations, accessibility edges and positive atom are all premises"
    );
    let body_source = gmeow_logic::modal::ModalPremise {
        context: "https://example.org/modal/w1".into(),
        subject: "https://example.org/modal/a".into(),
        predicate: "https://example.org/modal/knows".into(),
        object: "<https://example.org/modal/b>".into(),
    }
    .occurrence_id();
    let access_source = gmeow_logic::modal::ModalPremise {
        context: "https://example.org/modal/frame".into(),
        subject: "https://example.org/modal/w0".into(),
        predicate: "https://blackcatinformatics.ca/logic/epistemicallyPossible".into(),
        object: "<https://example.org/modal/w2>".into(),
    }
    .occurrence_id();
    let verdict_derivation = expected.derivation_id();
    let mut expected_counterexample = expected.clone();
    expected_counterexample.conclusion_predicate =
        "https://blackcatinformatics.ca/logic/modalCounterexampleWorld".into();
    expected_counterexample.conclusion_object = "https://example.org/modal/w2".into();
    let counterexample_derivation = expected_counterexample.derivation_id();
    let counterexample = reasoned
        .result
        .inferred()
        .iter()
        .find(|axiom| {
            axiom.predicate == "https://blackcatinformatics.ca/logic/modalCounterexampleWorld"
        })
        .expect("typed production result carries the counterexample world");
    assert_eq!(counterexample.world, "https://example.org/modal/frame");
    assert_eq!(counterexample.subject, "https://example.org/modal/F");
    assert_eq!(
        counterexample.object.as_iri(),
        Some("https://example.org/modal/w2")
    );
    assert_eq!(
        counterexample.rule_name.as_deref(),
        Some("https://blackcatinformatics.ca/logic/rule/modal-evaluation")
    );
    assert_eq!(
        counterexample.modal_evaluation.as_deref(),
        Some(&expected_counterexample)
    );
    assert_eq!(counterexample.premises, premises);
    assert!(
            !reasoned.closure.contains(
                "<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalNecessityFails> <https://example.org/modal/B> ."
            ),
            "the contextual verdict must not become a default assertion"
        );
    assert!(
            !reasoned.closure.contains(
                "<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalCounterexampleWorld> <https://example.org/modal/w2> ."
            ),
            "the contextual counterexample must not become a default assertion"
        );
    for artifact in [&reasoned.closure, &reasoned.explanations] {
        assert!(artifact.contains("https://blackcatinformatics.ca/logic/rule/modal-evaluation"));
        assert!(artifact.contains(&body_source));
        assert!(artifact.contains(&access_source));
        assert!(artifact.contains(&verdict_derivation));
        assert!(artifact.contains(&counterexample_derivation));
        assert!(artifact.contains("https://example.org/modal/frame"));
    }
    let product = reason_product(nq).expect("stage-reason product");
    let reasoning_graph = product.dataset().project_named_graph(GRAPH_REASONING);
    let projected = reasoning_graph.owned_quads().collect::<Vec<_>>();
    assert!(projected.iter().any(|quad| {
        quad.predicate == "https://blackcatinformatics.ca/gmeow/viaRule"
            && matches!(
                &quad.object,
                RdfTerm::Iri(iri)
                    if iri == "https://blackcatinformatics.ca/logic/rule/modal-evaluation"
            )
    }));
    assert!(projected.iter().any(|quad| {
        quad.predicate == "https://blackcatinformatics.ca/logic/derivationIdentifier"
            && matches!(
                &quad.object,
                RdfTerm::Literal(literal) if literal.lexical_form == verdict_derivation
            )
    }));
    assert!(projected.iter().any(|quad| {
        quad.predicate == "https://blackcatinformatics.ca/logic/derivationIdentifier"
            && matches!(
                &quad.object,
                RdfTerm::Literal(literal) if literal.lexical_form == counterexample_derivation
            )
    }));
    assert!(projected.iter().any(|quad| {
        quad.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom"
            && matches!(&quad.object, RdfTerm::Iri(iri) if iri.as_str() == body_source.as_str())
    }));
    assert!(projected.iter().any(|quad| {
        quad.predicate == "http://www.w3.org/ns/prov#wasDerivedFrom"
            && matches!(&quad.object, RdfTerm::Iri(iri) if iri.as_str() == access_source.as_str())
    }));

    let handle = product
        .bundle()
        .handle(GRAPH_REASONING)
        .expect("stage-reason pins its typed result");
    let PipelineHandle::Reasoning(transported) = &handle.payload else {
        panic!("graph/reasoning must carry a Reasoning handle");
    };
    let transported_modal = transported
        .inferred()
        .iter()
        .find(|axiom| axiom.predicate == "https://blackcatinformatics.ca/logic/modalNecessityFails")
        .expect("stage-reason handle retains modal verdict");
    assert_eq!(transported_modal, modal);
    let transported_counterexample = transported
        .inferred()
        .iter()
        .find(|axiom| {
            axiom.predicate == "https://blackcatinformatics.ca/logic/modalCounterexampleWorld"
        })
        .expect("stage-reason handle retains the modal counterexample");
    assert_eq!(transported_counterexample, counterexample);
}

#[test]
fn reason_product_hard_fails_on_malformed_modal_frames() {
    let nq = br#"
<https://example.org/valid/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/valid/B> <https://example.org/valid/w> .
<https://example.org/valid/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.org/valid/C> <https://example.org/valid/w> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/necessarily> <https://example.org/modal/B> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/overAccessibility> <https://blackcatinformatics.ca/logic/accessibleFrom> <https://example.org/modal/frame> .
<https://example.org/modal/F> <https://blackcatinformatics.ca/logic/modalEvalWorld> <https://example.org/modal/w0> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomSubject> <https://example.org/modal/a> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomPredicate> <https://example.org/modal/knows> <https://example.org/modal/frame> .
<https://example.org/modal/B> <https://blackcatinformatics.ca/logic/atomObject> <https://example.org/modal/b> <https://example.org/modal/frame> .
"#;
    let err = reason_product(nq).expect_err("malformed modal frame must fail closed");
    assert!(
        err.message().contains("prose-only"),
        "the hard-fail should name the malformed modal accessibility: {err}"
    );
}

#[test]
fn pin_handle_hard_fails_on_a_digest_mismatch() {
    let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
"#;
    let reasoned = reason_artifacts(nq).expect("reason");
    let dataset = Arc::clone(&reasoned.dataset);
    let mut bundle = bundle_from_artifacts_over(
        dataset,
        BTreeMap::new(),
        purrdf::provenance::DatasetProvenance::new(),
    );
    // A WRONG pinned digest must be rejected (no silently-stale handle).
    let wrong = purrdf::ContentDigest::of(b"not the backing graph");
    let err = bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(reasoned.result)),
            wrong,
        )
        .expect_err("a mismatched pin must hard-fail");
    let _ = err;
}
