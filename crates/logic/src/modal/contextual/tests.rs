// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

mod native_execution;
mod temporal;

const SOURCE: &str = r#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <urn:example:> .
ex:c a logic:AttributedContext ; logic:contextWorld ex:w ;
  logic:contextStandpoint ex:s ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:open a logic:AttributedContext ; logic:contextWorld ex:w ;
  logic:contextStandpoint ex:s ; logic:evidenceClosure logic:OpenWorldClosure .
ex:rival a logic:AttributedContext ; logic:contextWorld ex:w ;
  logic:contextStandpoint ex:t ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:other a logic:AttributedContext ; logic:contextWorld ex:otherWorld ;
  logic:contextStandpoint ex:s ; logic:evidenceClosure logic:ClosedWorldClosure .
ex:empty a logic:ContextSuccessorSet ; logic:successorContext ex:c ;
  logic:successorAxis logic:epistemicallyPossible ;
  logic:successorClosure logic:ClosedWorldClosure .
ex:w {
  ex:yes rdf:reifies <<( ex:item ex:predicate ex:value )>> ;
    gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportSupported .
  ex:no rdf:reifies <<( ex:item ex:predicate ex:value )>> ;
    gmeow:accordingTo ex:t ; gmeow:standpointSupportStatus gmeow:supportOpposed .
  ex:item ex:unattributed ex:value .
}
ex:otherWorld {
  ex:elsewhere rdf:reifies <<( ex:item ex:predicate ex:value )>> ;
    gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportBoth .
}
"#;

const QUERY: &str = r#"
ex:request a logic:ContextualEvaluationRequest ;
  logic:queryFormula ex:formula ; logic:queryContext ex:c .
ex:formula a logic:Formula ; logic:relation ex:predicate ;
  logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ],
                 [ logic:termIndex 1 ; logic:termIri ex:value ] .
"#;

fn parse(source: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(source.as_bytes(), "application/trig", None)
        .expect("synthetic RDF 1.2 input")
}

/// These synthetic requests explicitly select the two evidence worlds in SOURCE;
/// the default and named metadata graphs remain configuration, not logical domains.
fn evidence_domains(
    input: &crate::reason::PreparedReasoningInput,
) -> crate::physical::SelectedDomains {
    use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};

    SelectedDomains::new(["urn:example:w", "urn:example:otherWorld"].map(|world| {
        SelectedLogicalWorld::new(
            LogicalGraph::Named(purrdf::TermValue::iri(world)),
            DomainProfile::NonemptyObjectDomainV1,
            "urn:test:contextual-request:evidence-worlds".to_owned(),
            *input.ingress_contract(),
        )
        .expect("explicit synthetic evidence world")
    }))
    .expect("distinct synthetic evidence worlds")
}

fn named_metadata_source() -> String {
    let (prefixes, body) = SOURCE.split_once("ex:c a").expect("context start");
    let (contexts, evidence) = body.split_once("ex:w {").expect("evidence worlds");
    format!("{prefixes} ex:metadata {{ ex:c a {contexts} {QUERY} }} ex:w {{ {evidence}")
}

#[test]
fn named_request_keeps_source_configuration_separate_from_evidence_worlds() {
    let source = named_metadata_source();
    let default_conflict =
        "ex:c logic:contextWorld ex:otherWorld . ex:formula logic:not ex:formula .";
    let dataset = parse(&format!("{source} {default_conflict}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", None, None)
        .expect("named metadata selects its own complete configuration");
    assert_eq!(assessment.result.information, InformationState::Supported);
    assert_eq!(
        assessment.result.completeness,
        CompletenessStatus::CompleteForFragment
    );
    let proof = assessment
        .result
        .provenance
        .proof
        .as_ref()
        .expect("support proof");
    assert!(proof.cited_iris.contains("urn:example:yes"));
    assert!(!proof.cited_iris.contains("urn:example:elsewhere"));
    let dataset_input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let dataset_domains = evidence_domains(&dataset_input);
    let reasoned = crate::reason::reason_all(dataset_input, &dataset_domains)
        .expect("native named request integration");
    assert_eq!(
        reasoned
            .inferred()
            .iter()
            .filter(|axiom| axiom.predicate == format!("{LOGIC_NAMESPACE}contextualResult"))
            .count(),
        1
    );
    assert!(
        !reasoned
            .inferred()
            .iter()
            .any(|axiom| axiom.predicate == "urn:example:predicate")
    );
}

#[test]
fn nested_review_selects_its_standpoint_before_following_epistemic_edges() {
    let source = format!("{SOURCE} {QUERY}",).replace(
        "logic:queryFormula ex:formula",
        "logic:queryFormula ex:combined",
    );
    let nested = r#"
ex:combined a logic:Formula ; logic:and ex:formula, ex:review .
ex:review a logic:Formula ; logic:inContext ex:rival ;
  logic:necessarily ex:opposition ; logic:overAccessibility logic:epistemicallyPossible .
ex:opposition a logic:Formula ; logic:not ex:formula .
ex:reviewPossibilities a logic:ContextSuccessorSet ; logic:successorContext ex:rival ;
  logic:successorAxis logic:epistemicallyPossible ;
  logic:successorClosure logic:ClosedWorldClosure ; logic:successorMember ex:rival .
"#;
    let dataset = parse(&format!("{source} {nested}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", None, None)
        .expect("explicitly selected rival context preserves its epistemic standpoint");
    assert_eq!(assessment.result.information, InformationState::Supported);
    assert_eq!(
        assessment.result.completeness,
        CompletenessStatus::CompleteForFragment
    );
    let proof = assessment
        .result
        .provenance
        .proof
        .as_ref()
        .expect("nested proof");
    for witness in [
        "urn:example:yes",
        "urn:example:no",
        "urn:example:reviewPossibilities",
    ] {
        assert!(proof.cited_iris.contains(witness), "{witness}");
    }

    let implicit_switch = nested.replace(
        "logic:successorMember ex:rival",
        "logic:successorMember ex:c",
    );
    let dataset = parse(&format!("{source} {implicit_switch}"));
    let error = evaluate_request(&dataset, "urn:example:request", None, None)
        .expect_err("an epistemic edge cannot substitute another standpoint");
    assert!(error.message().contains("unselected context coordinate"));
}

#[test]
fn selecting_one_request_does_not_select_unrelated_malformed_requests() {
    let source = named_metadata_source();
    let dataset = parse(&format!(
        "{source} [] a logic:ContextualEvaluationRequest ."
    ));
    assert!(evaluate_request(&dataset, "urn:example:request", None, None).is_ok());
    assert!(evaluate_requests(&dataset, None, None).is_err());
}

#[test]
fn named_request_cannot_borrow_a_missing_binding_or_choose_between_source_graphs() {
    let source = named_metadata_source();
    let ambiguous = parse(&format!(
        "{source} ex:request a logic:ContextualEvaluationRequest ."
    ));
    for result in [
        evaluate_requests(&ambiguous, None, None).map(|_| ()),
        evaluate_request(&ambiguous, "urn:example:request", None, None).map(|_| ()),
    ] {
        assert!(
            result
                .expect_err("ambiguous source selection")
                .message()
                .contains("multiple source graphs")
        );
    }
    let incomplete = source.replace("logic:queryContext ex:c", "ex:unrelated ex:c");
    let dataset = parse(&format!(
        "{incomplete} ex:request logic:queryContext ex:c ."
    ));
    assert!(
        evaluate_request(&dataset, "urn:example:request", None, None)
            .expect_err("default graph cannot complete named request metadata")
            .message()
            .contains("missing required logic:queryContext")
    );
}

#[test]
fn incremental_attribution_replaces_contextual_results_outside_the_changed_world() {
    let source = format!("{SOURCE}\n{QUERY}");
    let without_owner = source.replace(
        "gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportSupported",
        "gmeow:standpointSupportStatus gmeow:supportSupported",
    );
    assert_ne!(source, without_owner);
    let base_edb = parse(&without_owner);
    let with_candidate_edb = parse(&source);
    let base_edb_input = crate::reason::prepare_reasoning_input(base_edb.as_ref()).unwrap();
    let base_edb_domains = evidence_domains(&base_edb_input);
    let candidate = crate::rule_ir::Fact {
        subject: purrdf::TermValue::iri("urn:example:yes"),
        predicate: "https://blackcatinformatics.ca/gmeow/accordingTo".into(),
        object: purrdf::TermValue::iri("urn:example:s"),
    };
    let session = crate::reason::NativeReasoningSession::new(
        base_edb_input,
        &base_edb_domains,
        vec![("urn:example:w".into(), candidate.clone())],
    )
    .expect("unattributed base reasons");
    let base = session.base();
    let with_candidate_edb_input =
        crate::reason::prepare_reasoning_input(with_candidate_edb.as_ref()).unwrap();
    // Both runs select the same logical domains; the candidate changes their
    // assertions, not the intrinsic domain identity of this operation.
    let scratch = crate::reason::reason_all(with_candidate_edb_input, &base_edb_domains)
        .expect("attributed input reasons");
    let incremental = session
        .insert(
            crate::physical::LogicalGraph::Named(purrdf::TermValue::iri("urn:example:w")),
            candidate,
            None,
        )
        .expect("incremental attribution reasons");
    let assessments = |result: &ReasoningResult| {
        result
            .inferred()
            .iter()
            .filter(|axiom| axiom.rule_name.as_deref() == Some(RULE_IRI))
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let old = assessments(base);
    let expected = assessments(&scratch);
    assert!(!old.is_empty() && !expected.is_empty());
    assert_ne!(
        old, expected,
        "attribution changes the assessment and its identity"
    );
    assert_eq!(incremental.status, crate::seam::BudgetStatus::Ok);
    let actual = assessments(&incremental.result);
    assert_eq!(
        actual.len(),
        expected.len(),
        "no stale assessment rows remain"
    );
    for (actual, expected) in actual.iter().zip(&expected) {
        assert_eq!(actual, expected, "fresh and incremental proof rows agree");
    }
    for (result, information) in [
        (base, InformationState::Neither),
        (&incremental.result, InformationState::Supported),
    ] {
        let values = assessments(result)
            .into_iter()
            .filter(|axiom| axiom.predicate == format!("{LOGIC_NAMESPACE}resultInformation"))
            .map(|axiom| axiom.object.clone())
            .collect::<Vec<_>>();
        assert_eq!(values, vec![TermValue::iri(information.iri())]);
    }
}

fn evidence(frame: &RdfFrame<'_>, context: &str, relation: &str) -> Evidence {
    frame
        .atom(
            context,
            relation,
            &Term::Iri("urn:example:item".into()),
            &Term::Iri("urn:example:value".into()),
        )
        .expect("admitted query")
}

#[test]
fn rdf12_evidence_preserves_world_and_standpoint_without_default_support() {
    let dataset = parse(SOURCE);
    let frame = RdfFrame::load(dataset.as_ref()).expect("admitted attributed contexts");
    let supported = evidence(&frame, "urn:example:c", "urn:example:predicate");
    assert_eq!(supported.support.as_deref(), Some("urn:example:yes"));
    assert!(supported.opposition.is_none() && supported.complete);
    let opposed = evidence(&frame, "urn:example:rival", "urn:example:predicate");
    assert_eq!(opposed.opposition.as_deref(), Some("urn:example:no"));
    assert!(opposed.support.is_none() && opposed.complete);
    let both = evidence(&frame, "urn:example:other", "urn:example:predicate");
    assert!(both.support.is_some() && both.opposition.is_some() && both.complete);
    let bare = evidence(&frame, "urn:example:c", "urn:example:unattributed");
    assert_eq!(
        bare,
        Evidence {
            complete: true,
            ..Evidence::default()
        }
    );
    let open = evidence(&frame, "urn:example:open", "urn:example:missing");
    assert_eq!(open, Evidence::default());
}

#[test]
fn an_explicitly_empty_successor_set_differs_from_a_missing_inventory() {
    let dataset = parse(SOURCE);
    let frame = RdfFrame::load(dataset.as_ref()).unwrap();
    let closed = frame
        .successors(
            "urn:example:c",
            Accessibility::parse(crate::modal::TYPED_ACCESSIBILITY[0]).unwrap(),
        )
        .unwrap();
    assert!(closed.transitions.is_empty() && closed.closure_witness.is_some());
    assert!(
        frame
            .successors(
                "urn:example:c",
                Accessibility::parse(crate::modal::TYPED_ACCESSIBILITY[1]).unwrap()
            )
            .is_err()
    );
}

#[test]
fn conflicting_bindings_and_malformed_selected_claims_fail_admission() {
    for extra in [
        "ex:c logic:contextStandpoint ex:another .",
        "ex:c logic:contextJournal ex:journal .",
        "ex:c logic:contextNormScope ex:scope .",
        "ex:c logic:evidenceClosure ex:unknownClosure .",
        "ex:w { ex:yes gmeow:standpointSupportStatus gmeow:supportOpposed . }",
    ] {
        let dataset = parse(&format!("{SOURCE}\n{extra}"));
        assert!(
            matches!(
                RdfFrame::load(dataset.as_ref()),
                Err(AdmissionError::Malformed(_))
            ),
            "{extra}"
        );
    }
}

#[test]
fn public_assessment_preserves_the_shared_result_and_complete_proof_addresses() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", None, None).unwrap();
    assert_eq!(assessment.result.information, InformationState::Supported);
    assert_eq!(
        assessment.result.completeness,
        CompletenessStatus::CompleteForFragment
    );
    assert_eq!(
        assessment.result.provenance.context.attributed.as_deref(),
        Some("urn:example:c")
    );
    assert!(
        assessment
            .result
            .provenance
            .proof
            .as_ref()
            .unwrap()
            .cited_iris
            .contains("urn:example:yes")
    );
    assert!(
        !assessment
            .result
            .provenance
            .proof
            .as_ref()
            .unwrap()
            .cited_iris
            .contains("urn:example:no")
    );
    let anchors = assessment
        .anchors
        .iter()
        .map(|anchor| &anchor.identity)
        .collect::<BTreeSet<_>>();
    for inference in &assessment.inferences {
        for antecedent in &inference.antecedents {
            if antecedent.starts_with("urn:gmeow:modal-assessment:") {
                assert!(anchors.contains(antecedent));
            }
        }
    }
    let rdf = crate::result_rdf::project_contextual_assessment(&assessment)
        .expect("project the valid native contextual result");
    assert!(rdf.contains("urn:example:yes"));
    assert!(rdf.contains("ContextualJudgment"));
    let restored = crate::result_rdf::parse_reasoning_graph(&rdf).unwrap();
    assert_eq!(restored.provenance, assessment.result.provenance);
}

#[test]
fn public_budget_stop_is_typed_and_cannot_become_neither() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", Some(0), None).unwrap();
    assert_eq!(assessment.interrupted, Some(IncompleteCause::StepBudget));
    assert_eq!(
        assessment.result.information,
        InformationState::Undetermined
    );
    assert_eq!(
        assessment.result.evaluation,
        EvaluationStatus::BudgetExhausted
    );
    let rdf = crate::result_rdf::project_contextual_assessment(&assessment)
        .expect("project the valid native contextual result");
    assert!(rdf.contains("IncompleteStepBudget"));
    assert!(assessment.inferences.is_empty() && assessment.anchors.is_empty());
}

#[test]
fn native_reasoning_evaluates_requests_without_asserting_their_propositions() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let dataset_input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let dataset_domains = evidence_domains(&dataset_input);
    let result = crate::reason::reason_all(dataset_input, &dataset_domains).unwrap();
    let projection = crate::result_rdf::project_reasoning_result(&result)
        .expect("project the valid native contextual result");
    let restored = crate::result_rdf::parse_reasoning_graph(&projection).unwrap();
    assert_eq!(restored.provenance, result.provenance);
    assert_eq!(
        restored.inferred().iter().collect::<BTreeSet<_>>(),
        result
            .inferred()
            .iter()
            .filter(|axiom| !axiom.is_edb)
            .collect::<BTreeSet<_>>(),
        "the aggregate reader retains every derived receipt",
    );
    let graph =
        purrdf::parse_dataset(projection.as_bytes(), "application/n-triples", None).unwrap();
    assert!(graph.owned_quads().any(|quad| {
        quad.subject == purrdf::RdfTerm::iri("urn:example:request")
            && quad.predicate == format!("{LOGIC_NAMESPACE}contextualResult")
    }));
    crate::reason::inferred_axioms_to_dataset(result.inferred())
        .expect("the public closure consumer preserves contextual objects too");
    for artifact in [
        crate::reason::artifacts::build_inferred_closure_ttl(&result, None, &[]).unwrap(),
        crate::reason::artifacts::build_explanations_ttl(&result).unwrap(),
    ] {
        assert_contextual_artifact_receipt(&artifact, &result, "urn:example:request");
    }
    assert!(
        result
            .inferred()
            .iter()
            .any(|axiom| axiom.subject == "urn:example:request"
                && axiom.predicate == format!("{LOGIC_NAMESPACE}contextualResult")
                && !axiom.is_edb)
    );
    assert!(result.inferred().iter().any(|axiom| axiom.predicate
        == format!("{LOGIC_NAMESPACE}resultInformation")
        && axiom.object.as_iri() == Some("https://blackcatinformatics.ca/logic/InfoSupported")));
    assert!(
        !result
            .inferred()
            .iter()
            .any(|axiom| axiom.subject == "urn:example:item"
                && axiom.predicate == "urn:example:predicate"
                && !axiom.is_edb)
    );
}

#[test]
fn lexical_carriers_cannot_silently_erase_rdf12_literal_semantics() {
    // gmeow-test-input: synthetic-only
    // LOGIC-IR admits complete native literals. Exact value identity must select
    // the attributed claim without collapsing its datatype, language or direction.
    let mut contracts = BTreeSet::new();
    for (literal, different) in [
        ("\"bonjour\"@fr", "\"bonjour\"@en"),
        ("\"hello\"@en--ltr", "\"hello\"@en--rtl"),
        ("42", "\"42\""),
        ("true", "\"true\""),
    ] {
        let query = QUERY.replace(
            "logic:termIri ex:value",
            &format!("logic:termLiteral {literal}"),
        );
        let other_claims = format!(
            r#"
            ex:w {{
                ex:literalDifferent rdf:reifies <<( ex:item ex:predicate {different} )>> ;
                    gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportOpposed .
                ex:literalRival rdf:reifies <<( ex:item ex:predicate {literal} )>> ;
                    gmeow:accordingTo ex:t ; gmeow:standpointSupportStatus gmeow:supportOpposed .
            }}
            ex:otherWorld {{
                ex:literalElsewhere rdf:reifies <<( ex:item ex:predicate {literal} )>> ;
                    gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportBoth .
            }}
        "#
        );
        let unmatched = parse(&format!("{SOURCE}\n{query}\n{other_claims}"));
        let absent = evaluate_request(&unmatched, "urn:example:request", None, None).unwrap();
        assert_eq!(
            absent.result.information,
            InformationState::Neither,
            "{literal}"
        );
        assert!(
            absent.result.provenance.proof.is_none()
                && absent.result.provenance.counterproof.is_none()
        );

        let matching = format!(
            "ex:w {{ ex:literalMatch rdf:reifies <<( ex:item ex:predicate {literal} )>> ; gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportSupported . }}"
        );
        let dataset = parse(&format!("{SOURCE}\n{query}\n{other_claims}\n{matching}"));
        let assessment = evaluate_request(&dataset, "urn:example:request", None, None).unwrap();
        assert_eq!(
            assessment.result.information,
            InformationState::Supported,
            "{literal}"
        );
        assert!(assessment.result.provenance.counterproof.is_none());
        assert_eq!(assessment.result.provenance.context.world, "urn:example:w");
        assert_eq!(
            assessment.result.provenance.context.standpoint.as_deref(),
            Some("urn:example:s")
        );
        let proof = assessment.result.provenance.proof.as_ref().unwrap();
        assert!(proof.cited_iris.contains("urn:example:literalMatch"));
        for excluded in ["literalDifferent", "literalRival", "literalElsewhere"] {
            assert!(
                !proof
                    .cited_iris
                    .contains(&format!("urn:example:{excluded}")),
                "{literal}: excluded {excluded}"
            );
        }
        assert!(
            contracts.insert(assessment.result.provenance.contract_hash.clone()),
            "native literal identities must not collapse"
        );
        let native = crate::result_rdf::project_contextual_assessment_dataset(&assessment).unwrap();
        assert_eq!(
            crate::result_rdf::parse_reasoning_dataset(&native, GraphMatch::Default)
                .unwrap()
                .provenance,
            assessment.result.provenance
        );

        let opposite_query = QUERY.replace(
            "logic:termIri ex:value",
            &format!("logic:termLiteral {different}"),
        );
        let opposite = parse(&format!(
            "{SOURCE}\n{opposite_query}\n{other_claims}\n{matching}"
        ));
        let opposition = evaluate_request(&opposite, "urn:example:request", None, None).unwrap();
        assert_eq!(
            opposition.result.information,
            InformationState::Opposed,
            "{different}"
        );
        assert!(opposition.result.provenance.proof.is_none());
        let counterproof = opposition.result.provenance.counterproof.as_ref().unwrap();
        assert!(
            counterproof
                .cited_iris
                .contains("urn:example:literalDifferent")
        );
        assert!(!counterproof.cited_iris.contains("urn:example:literalMatch"));
        assert_ne!(proof.derivation_id, counterproof.derivation_id);
        assert_ne!(
            assessment.result.provenance.contract_hash,
            opposition.result.provenance.contract_hash
        );

        // Explicit authoring may add a datatype to a plain lexical value, but
        // cannot override any already typed or language-tagged native identity.
        let conflicting = QUERY.replace("logic:termIri ex:value", &format!("logic:termLiteral {literal} ; logic:termLiteralDatatype <http://www.w3.org/2001/XMLSchema#string>"));
        let dataset = parse(&format!("{SOURCE}\n{conflicting}"));
        let error = evaluate_request(&dataset, "urn:example:request", None, None).unwrap_err();
        assert!(
            error
                .message()
                .contains("disagrees with the native literal"),
            "{literal}: {error}"
        );
    }
    let query = QUERY.replace("logic:termIri ex:value", "logic:termLiteral \"42\" ; logic:termLiteralDatatype <http://www.w3.org/2001/XMLSchema#integer>");
    let claim = "ex:w { ex:integer rdf:reifies <<( ex:item ex:predicate 42 )>> ; gmeow:accordingTo ex:s ; gmeow:standpointSupportStatus gmeow:supportSupported . }";
    let dataset = parse(&format!("{SOURCE}\n{query}\n{claim}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", None, None).unwrap();
    assert_eq!(assessment.result.information, InformationState::Supported);
}

#[test]
fn result_components_remain_disjoint_when_multiple_requests_share_one_reasoning_graph() {
    let second = "ex:rivalRequest a logic:ContextualEvaluationRequest ;
        logic:queryFormula ex:formula ; logic:queryContext ex:rival .";
    let dataset = parse(&format!("{SOURCE}\n{QUERY}\n{second}"));
    let assessments = evaluate_requests(&dataset, None, None).unwrap();
    assert_eq!(assessments.len(), 2);
    let mut component_sets = Vec::new();
    for assessment in &assessments {
        let rdf = crate::result_rdf::project_contextual_assessment(assessment)
            .expect("project the valid native contextual result");
        let graph = purrdf::parse_dataset(rdf.as_bytes(), "application/n-triples", None).unwrap();
        let components = graph
            .quads()
            .filter_map(|quad| match graph.resolve(quad.s) {
                TermRef::Iri(iri) if iri.contains("/component/") => Some(iri.to_owned()),
                TermRef::Blank { .. } => {
                    panic!("contextual projection must scope all generated components")
                }
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert!(!components.is_empty());
        component_sets.push(components);
        let restored = crate::result_rdf::parse_reasoning_graph(&rdf).unwrap();
        assert_eq!(restored.provenance, assessment.result.provenance);
    }
    assert!(component_sets[0].is_disjoint(&component_sets[1]));
    let dataset_input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let dataset_domains = evidence_domains(&dataset_input);
    let reasoned = crate::reason::reason_all(dataset_input, &dataset_domains).unwrap();
    let aggregate = crate::result_rdf::project_reasoning_result(&reasoned)
        .expect("project the valid native contextual result");
    assert_eq!(
        crate::result_rdf::parse_reasoning_graph(&aggregate)
            .unwrap()
            .provenance,
        reasoned.provenance,
        "multiple assessment children cannot replace the aggregate handle",
    );
    assert_eq!(
        reasoned
            .inferred()
            .iter()
            .filter(|axiom| axiom.predicate == format!("{LOGIC_NAMESPACE}contextualResult"))
            .count(),
        2
    );
}

#[test]
fn native_derived_attribution_preserves_world_scopes_and_exact_receipts() {
    let source = SOURCE.replace("gmeow:accordingTo ex:s", "ex:reportedBy ex:s");
    let rules = "ex:w { ex:reportedBy <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> gmeow:accordingTo . }";
    let dataset = parse(&format!("{source}\n{rules}\n{QUERY}"));
    let source_only = evaluate_request(&dataset, "urn:example:request", None, None).unwrap();
    assert_eq!(source_only.result.information, InformationState::Neither);
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let closure = crate::reason::reason_closure_axioms(input, &domains).unwrap();
    let assessments = evaluate_requests_with_closure(&dataset, &closure, None, None).unwrap();
    let assessment = &assessments[0];
    assert_eq!(assessment.result.information, InformationState::Supported);
    assert_eq!(assessment.native_evidence.len(), 1);
    let native = &assessment.native_evidence[0];
    assert_eq!(native.row().graph, "urn:example:w");
    assert_eq!(native.row().subject, "urn:example:yes");
    let derived_fact = closure
        .iter()
        .find(|fact| {
            !fact.is_edb
                && fact.subject == "urn:example:yes"
                && fact.predicate == format!("{GMEOW_NS}accordingTo")
        })
        .unwrap();
    let exact = crate::explain::receipt_for_axiom(derived_fact);
    assert_eq!(native.row(), &exact.row);
    let proof = assessment.result.provenance.proof.as_ref().unwrap();
    assert!(proof.cited_iris.contains(native.identity()));
    assert!(proof.cited_iris.contains(&exact.row.derivation_id));
    assert!(
        exact
            .row
            .source_quad_ids
            .iter()
            .all(|premise| proof.cited_iris.contains(premise))
    );
    let projection = crate::result_rdf::project_contextual_assessment(assessment)
        .expect("project the valid native contextual result");
    let graph =
        purrdf::parse_dataset(projection.as_bytes(), "application/n-triples", None).unwrap();
    assert!(graph.reifiers_with_graph().any(|(reifier, _, _)| matches!(graph.resolve(reifier), TermRef::Iri(identity) if identity == native.identity())));
    assert!(projection.contains("receipt-rule-identity"));
    assert_eq!(
        crate::result_rdf::parse_reasoning_graph(&projection)
            .unwrap()
            .provenance,
        assessment.result.provenance
    );
    let dataset_input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let dataset_domains = evidence_domains(&dataset_input);
    let native_result = crate::reason::reason_all(dataset_input, &dataset_domains).unwrap();
    for artifact in [
        crate::reason::artifacts::build_inferred_closure_ttl(&native_result, None, &[]).unwrap(),
        crate::reason::artifacts::build_explanations_ttl(&native_result).unwrap(),
    ] {
        assert_contextual_artifact_receipt(&artifact, &native_result, native.identity());
    }
    let aggregate = crate::result_rdf::project_reasoning_result(&native_result)
        .expect("project the valid native contextual result");
    let restored = crate::result_rdf::parse_reasoning_graph(&aggregate).unwrap();
    assert_eq!(restored.provenance, native_result.provenance);
    assert_eq!(
        restored.inferred().iter().collect::<BTreeSet<_>>(),
        native_result
            .inferred()
            .iter()
            .filter(|axiom| !axiom.is_edb)
            .collect::<BTreeSet<_>>(),
        "quoted receipt objects retain their exact source spelling",
    );
    crate::reason::inferred_axioms_to_dataset(native_result.inferred())
        .expect("the public closure consumer preserves quoted native receipts");
    assert!(
        native_result
            .inferred()
            .iter()
            .any(|fact| fact.subject == native.identity()
                && fact.predicate == "http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies"
                && matches!(&fact.object, TermValue::Triple { .. }))
    );
    let rival_query = QUERY.replace("logic:queryContext ex:c", "logic:queryContext ex:other");
    let rival = parse(&format!("{source}\n{rules}\n{rival_query}"));
    let rival_assessments = evaluate_requests_with_closure(&rival, &closure, None, None).unwrap();
    assert_eq!(
        rival_assessments[0].result.information,
        InformationState::Neither
    );
    assert!(rival_assessments[0].native_evidence.is_empty());
}

#[test]
fn raw_inferred_facts_do_not_supply_attribution_and_interruption_claims_no_receipts() {
    let dataset = parse(&format!("{SOURCE}\n{QUERY}"));
    let bare = crate::reason::InferredAxiom {
        modal_evaluation: None,
        subject: "urn:example:item".into(),
        predicate: "urn:example:unattributed".into(),
        object: TermValue::iri("urn:example:value"),
        world: "urn:example:w".into(),
        is_edb: false,
        rule_name: Some("urn:example:rule".into()),
        premises: vec![(
            "urn:example:x".into(),
            "urn:example:p".into(),
            "<urn:example:y>".into(),
        )],
    };
    let frame = RdfFrame::load_with_closure(dataset.as_ref(), &[bare]).unwrap();
    let fact = evidence(&frame, "urn:example:c", "urn:example:unattributed");
    assert!(fact.support.is_none() && fact.opposition.is_none());
    let input = crate::reason::prepare_reasoning_input(dataset.as_ref()).unwrap();
    let domains = evidence_domains(&input);
    let closure = crate::reason::reason_closure_axioms(input, &domains).unwrap();
    let assessments = evaluate_requests_with_closure(&dataset, &closure, Some(0), None).unwrap();
    assert_eq!(
        assessments[0].result.evaluation,
        EvaluationStatus::BudgetExhausted
    );
    assert!(assessments[0].native_evidence.is_empty());
}

#[test]
fn unsupported_queries_export_the_shared_source_anchored_diagnostic_ledger() {
    let query = QUERY.replace(
        "logic:queryFormula ex:formula",
        "logic:queryFormula ex:quantified",
    );
    let quantified = "ex:quantified a logic:Formula ; logic:forall ex:formula ; logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .";
    let dataset = parse(&format!("{SOURCE}\n{query}\n{quantified}"));
    let assessment = evaluate_request(&dataset, "urn:example:request", None, None).unwrap();
    assert_eq!(assessment.result.evaluation, EvaluationStatus::Unsupported);
    assert_eq!(
        assessment.result.information,
        InformationState::NotEvaluated
    );
    let nodes = assessment.diagnostics.emit_sorted();
    assert_eq!(nodes.len(), 1);
    let finding = nodes[0].to_finding("gmeow-logic.contextual");
    assert_eq!(
        finding.category,
        Some(gmeow_errors::FindingCategory::UnsupportedSemanticFeature)
    );
    let exported = crate::result_rdf::project_contextual_dataset(&assessment)
        .expect("project the valid native contextual result");
    let graph = purrdf::parse_dataset(exported.as_bytes(), "application/n-quads", None).unwrap();
    assert!(exported.contains(finding.finding_iri.as_deref().unwrap()));
    assert!(exported.contains("urn:example:request"));
    let named_graphs = graph
        .named_graphs()
        .map(|graph_id| match graph.resolve(graph_id) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => panic!("named output graphs are IRIs"),
        })
        .collect::<BTreeSet<_>>();
    assert!(named_graphs.contains(crate::result_rdf::GRAPH_REASONING));
    assert!(named_graphs.contains("https://blackcatinformatics.ca/gmeow/graph/diagnostics"));

    // The production reasoner must retain the same capability refusal. It may
    // not relabel valid unsupported syntax as an ordinary reasoning failure.
    let native = crate::reason::prepare_reasoning_input(dataset.as_ref())
        .and_then(|input| {
            let domains = evidence_domains(&input);
            crate::reason::reason_all(input, &domains)
        })
        .unwrap_err();
    assert!(native.is::<crate::error::ContextualFragment>());
    assert_eq!(native.inner().failure_class, finding.failure_class);
    let mut ledger = DiagLedger::new();
    ledger.attach(native, StageId("native-contextual-refusal".into()));
    let native_finding = ledger.emit_sorted()[0].to_finding("gmeow-logic.contextual");
    assert_eq!(native_finding.category, finding.category);
    assert!(native_finding.message.contains("urn:example:request"));
}

/// Grade a GMEOW receipt and its exact conclusion on both terminal artifact surfaces.
fn assert_contextual_artifact_receipt(artifact: &str, result: &ReasoningResult, subject: &str) {
    let axiom = result
        .inferred()
        .iter()
        .find(|axiom| axiom.subject == subject && axiom.rule_name.as_deref() == Some(RULE_IRI))
        .expect("selected contextual conclusion");
    let receipt = crate::explain::receipt_for_axiom(axiom);
    let dataset = purrdf::parse_dataset(artifact.as_bytes(), "text/turtle", None).unwrap();
    let expected = purrdf::RdfTriple::new(
        purrdf::RdfTerm::iri(&axiom.subject),
        &axiom.predicate,
        crate::reason::term_value_to_rdf_term(&axiom.object).unwrap(),
    );
    assert!(
        dataset
            .owned_reifiers()
            .any(|row| row.statement == expected)
            || dataset
                .owned_quads()
                .any(|row| row.object == purrdf::RdfTerm::triple(expected.clone())),
        "the artifact retains this contextual conclusion as statement evidence"
    );
    assert!(dataset.flat_default_graph_quads().any(|row| {
        matches!(&row.p, TermValue::Iri(iri) if iri == &format!("{LOGIC_NAMESPACE}derivationIdentifier"))
            && matches!(&row.o, TermValue::Literal { lexical_form, .. } if lexical_form == &receipt.row.derivation_id)
    }), "the actual contextual firing identity survives terminal emission");
}
