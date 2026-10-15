// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::reason::el::InferredAxiom;
use crate::result::{
    CompletenessStatus, EvaluationStatus, InformationState, InputStatus, PreservationClaim,
    ResultPayload, ResultProvenance,
};

/// A native provenance bundle for the test results.
fn prov() -> ResultProvenance {
    ResultProvenance::native("test-contract", "")
}

fn axiom(s: &str, p: &str, o: &str, rule: Option<&str>) -> InferredAxiom {
    InferredAxiom {
        modal_evaluation: None,
        subject: s.to_owned(),
        predicate: p.to_owned(),
        object: purrdf::TermValue::iri(bare_iri(o)),
        world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
        is_edb: false,
        rule_name: rule.map(str::to_owned),
        premises: vec![(
            "http://example.org/A".to_owned(),
            RDFS_SUBCLASS_OF.to_owned(),
            "<http://example.org/B>".to_owned(),
        )],
    }
}

/// Compare RDF terms independently of the optional explicit xsd:string spelling.
fn native_term(term: &RdfTerm) -> TermValue {
    let mut builder = RdfDatasetBuilder::new();
    let id = builder.intern_owned_term(term);
    builder.freeze().unwrap().term_value(id)
}

fn native_typed_results() -> impl Iterator<Item = (RdfTerm, ReasoningResult)> {
    let quoted = RdfTerm::triple(RdfTriple::new(
        RdfTerm::iri("urn:inner:subject"),
        "urn:inner:predicate",
        RdfTerm::literal(RdfLiteral::typed(
            "quoted \"value\"\nline",
            "http://www.w3.org/2001/XMLSchema#string",
        )),
    ));
    [
        RdfTerm::literal(RdfLiteral::typed(
            "text \"with quotes\"\nnext line",
            "http://www.w3.org/2001/XMLSchema#string",
        )),
        RdfTerm::literal(RdfLiteral::typed("42", XSD_INTEGER)),
        RdfTerm::literal(RdfLiteral {
            lexical_form: "مرحبا".into(),
            datatype: Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString".into()),
            language: Some("ar".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        }),
        quoted.clone(),
        RdfTerm::triple(RdfTriple::new(
            RdfTerm::iri("urn:outer:subject"),
            "urn:outer:predicate",
            quoted,
        )),
    ]
    .into_iter()
    .map(|object| {
        use gmeow_logic_compile::ir::{Formula, LogicProgram, Term};
        let atom = |subject: &str| {
            Formula::atom(
                Term::iri("urn:property").unwrap(),
                vec![Term::iri(subject).unwrap(), Term::var("object").unwrap()],
            )
            .unwrap()
        };
        let program =
            LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![Formula::Forall {
                vars: vec!["object".into()],
                body: Box::new(Formula::Implies(
                    Box::new(atom("urn:source")),
                    Box::new(atom("urn:result")),
                )),
            }]);
        let result = native_program_result(
            &program,
            vec![RdfQuad::new(
                RdfTerm::iri("urn:source"),
                "urn:property",
                object.clone(),
            )],
        );
        (object, result)
    })
}

#[test]
fn native_derived_objects_and_premises_keep_rdf12_types_in_both_artifacts() {
    for (object, result) in native_typed_results() {
        let closure = build_inferred_closure_ttl(&result, None, &[]).unwrap();
        let closure = purrdf::parse_dataset(closure.as_bytes(), "text/turtle", None)
            .expect("the closure retains typed derived objects");
        assert!(closure.owned_quads().any(|quad| {
            quad.subject == RdfTerm::iri("urn:result")
                && quad.predicate == "urn:property"
                && native_term(&quad.object) == native_term(&object)
        }));
        let typed = closure
            .quads()
            .find(|quad| {
                matches!(closure.resolve(quad.s), purrdf::TermRef::Iri("urn:result"))
                    && matches!(
                        closure.resolve(quad.p),
                        purrdf::TermRef::Iri("urn:property")
                    )
            })
            .expect("the derived conclusion is present");
        assert_eq!(
            closure.term_value(typed.o),
            native_term(&object),
            "runtime delta publication retains the same RDF 1.2 term",
        );
        let explanations = build_explanations_ttl(&result).unwrap();
        let explanations = purrdf::parse_dataset(explanations.as_bytes(), "text/turtle", None)
            .expect("explanations retain recursive conclusion and premise terms");
        for (property, subject) in [("concludes", "urn:result"), ("hasPremise", "urn:source")] {
            let expected = RdfTerm::triple(RdfTriple::new(
                RdfTerm::iri(subject),
                "urn:property",
                object.clone(),
            ));
            assert!(
                explanations.owned_quads().any(|quad| {
                    quad.predicate == gmeow(property)
                        && native_term(&quad.object) == native_term(&expected)
                }),
                "{property}"
            );
        }
    }
}

#[test]
fn corpus_verifier_preserves_native_derived_rdf12_objects() {
    for (object, result) in native_typed_results() {
        let empty = RdfDataset::union(&[]);
        let crate::verify::ReasonedGraphOutcome::Ready(verified) =
            crate::verify::materialize_reasoned_graph(&empty, &result)
                .expect("the downstream verifier accepts typed derived objects")
        else {
            panic!("the native typed-copy rule must have a complete closure");
        };
        assert!(verified.dataset.owned_quads().any(|quad| {
            quad.subject == RdfTerm::iri("urn:result")
                && quad.predicate == "urn:property"
                && native_term(&quad.object) == native_term(&object)
        }));
        assert!(verified.derived_predicates.contains("urn:property"));
    }
}

#[test]
fn corpus_malformed_artifact_objects_fail_without_iri_reinterpretation() {
    for object in [
        "\"unterminated",
        "<\"invalid\">",
        "\"x\" . <urn:a> <urn:b> <urn:c>",
    ] {
        assert!(
            premise_object(object).is_err(),
            "malformed textual premise: {object}"
        );
        let result = projection_result(vec![axiom(
            "urn:result",
            "urn:property",
            object,
            Some("native:typed"),
        )]);
        assert!(
            build_inferred_closure_ttl(&result, None, &[]).is_err(),
            "{object}"
        );
        assert!(build_explanations_ttl(&result).is_err(), "{object}");
        assert!(
            crate::verify::materialize_reasoned_graph(&RdfDataset::union(&[]), &result).is_err(),
            "the verifier must reject malformed objects: {object}",
        );
    }
}

#[test]
fn premise_object_preserves_iris_and_literals() {
    // An IRI premise object round-trips to a bare IRI term.
    assert_eq!(
        premise_object("<http://example.org/B>").unwrap(),
        RdfTerm::iri("http://example.org/B")
    );
    // A typed literal stays a literal — emitting it as an IRI would produce
    // invalid Turtle in the proof skeleton.
    assert_eq!(
        emit_term(&premise_object("\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>").unwrap()),
        "\"42\"^^<http://www.w3.org/2001/XMLSchema#integer>"
    );
    // Language-tagged and simple (xsd:string) literals likewise round-trip.
    assert_eq!(
        emit_term(&premise_object("\"hi\"@en").unwrap()),
        "\"hi\"@en"
    );
    assert_eq!(
        native_term(&premise_object("\"plain\"").unwrap()),
        TermValue::simple_literal("plain")
    );
}

/// Serializer-only inputs are conclusions, never claims of an executed closure.
/// Deliberately malformed rows and duplicate proofs cannot be native results.
fn projection_result(inferred: Vec<InferredAxiom>) -> ReasoningResult {
    ReasoningResult::new(
        InputStatus::Valid,
        EvaluationStatus::Completed,
        CompletenessStatus::Incomplete,
        PreservationClaim::exact(),
        InformationState::Undetermined,
        prov(),
        ResultPayload::Inferred(inferred),
    )
}

/// Execute only explicit tiny inputs; this never loads or produces the corpus.
fn native_result(quads: Vec<RdfQuad>) -> ReasoningResult {
    native_program_result(
        &gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None),
        quads,
    )
}

fn native_program_result(
    program: &gmeow_logic_compile::ir::LogicProgram,
    quads: Vec<RdfQuad>,
) -> ReasoningResult {
    use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
    const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/imports";
    let mut source = RdfDatasetBuilder::new();
    for quad in quads {
        source.push_owned_quad(&quad.in_graph(RdfTerm::iri(WORLD)));
    }
    let source = source.freeze().unwrap();
    let input = crate::reason::prepare_reasoning_input(&source).unwrap();
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Named(TermValue::iri(WORLD)),
        DomainProfile::NonemptyObjectDomainV1,
        "urn:gmeow:test:artifact-domain".into(),
        *input.ingress_contract(),
    )
    .unwrap()])
    .unwrap();
    // gmeow-test-input: synthetic-only
    let result = crate::reason::reason_program(program, input, &domains).unwrap();
    result.validate_native_closure().unwrap();
    result
}

fn native_subclass_result() -> ReasoningResult {
    native_result(vec![
        RdfQuad::new(
            RdfTerm::iri("http://example.org/A"),
            RDFS_SUBCLASS_OF,
            RdfTerm::iri("http://example.org/B"),
        ),
        RdfQuad::new(
            RdfTerm::iri("http://example.org/B"),
            RDFS_SUBCLASS_OF,
            RdfTerm::iri("http://example.org/C"),
        ),
    ])
}

/// This checks GMEOW's two projections of one governed result, including its
/// proof graph; parsing is only the independent wire-side observation here.
fn paired_closure(
    result: &ReasoningResult,
    alpha_edges: &[(String, String)],
) -> (String, std::sync::Arc<RdfDataset>) {
    let mut builder = RdfDatasetBuilder::new();
    let text = build_inferred_closure_into(result, alpha_edges, &mut builder).unwrap();
    let dataset = builder.freeze().unwrap();
    assert_eq!(
        text,
        build_inferred_closure_ttl(result, None, alpha_edges).unwrap()
    );
    let wire = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None).unwrap();
    assert!(
        purrdf::datasets_isomorphic(&dataset, &wire),
        "native closure must preserve the entire GMEOW artifact, including proof reifiers"
    );
    (text, dataset)
}

#[test]
fn closure_emits_triple_and_reifier_with_provenance() {
    let result = native_subclass_result();
    let derived = result
        .inferred()
        .iter()
        .find(|axiom| {
            !axiom.is_edb
                && axiom.subject == "http://example.org/A"
                && axiom.predicate == RDFS_SUBCLASS_OF
                && axiom.object.as_iri() == Some("http://example.org/C")
        })
        .expect("native transitive derivation");
    let receipt = receipt_for_axiom(derived);
    let canonical_rule = canonical_rule_iri("el:subClassOf-transitive");
    assert_eq!(receipt.row.rule_iri, canonical_rule);
    assert_eq!(receipt.raw_rule_identity, "el:subClassOf-transitive");
    assert_eq!(
        receipt.row.derivation_id,
        crate::provenance::mint_derivation_id(
            "el:subClassOf-transitive",
            &receipt
                .row
                .source_quad_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
        "the receipt hash preserves the native firing identity bytes"
    );
    let (ttl, _) = paired_closure(&result, &[]);
    assert!(ttl.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
    assert!(ttl.contains("rdf-syntax-ns#reifies> <<( "));
    assert!(ttl.contains(&format!("<{}> <{canonical_rule}>", gmeow("viaRule"))));
    assert!(
        !ttl.contains("<el:subClassOf-transitive>"),
        "the raw firing label is receipt data, never a public rule resource"
    );
    assert!(ttl.contains("receipt-rule-identity"));
    assert!(ttl.contains("el:subClassOf-transitive"));
    assert!(ttl.contains(&receipt.row.derivation_id));
    assert!(ttl.contains(&receipt.row.source_quad_ids[0]));
    assert!(ttl.contains("gmeow/inferenceKind> <https://blackcatinformatics.ca/gmeow/Deduction>"));
    assert!(ttl.contains("gmeow/inWorld> <https://blackcatinformatics.ca/gmeow/graph/imports>"));
}

#[test]
fn native_artifact_fixture_rejects_missing_or_mismatched_execution_evidence() {
    let result = native_subclass_result();
    let mut missing = result.clone();
    missing.provenance.native_execution = None;
    assert!(
        missing
            .validate()
            .unwrap_err()
            .message()
            .contains("missing its native execution evidence")
    );
    let mut mismatched = result;
    let ResultPayload::Inferred(inferred) = &mut mismatched.payload else {
        panic!("native result carries its inferred closure");
    };
    inferred.clear();
    let error = mismatched.validate_native_closure().unwrap_err();
    assert!(
        error
            .message()
            .contains("native witness head or premise is absent from the committed closure"),
        "{error}"
    );
}

/// The α-equivalence section is the SHIPPED half of the expression-identity derivation:
/// two α-equivalent expressions must land on ONE class individual, that individual must be
/// typed exactly once however many expressions reach it, and every edge must carry the
/// derivation's own rule provenance rather than borrowing an EL/DL rule's.
#[test]
fn closure_emits_one_joinable_class_for_alpha_equivalent_expressions() {
    const CLASS: &str = "https://blackcatinformatics.ca/math/alphaClass/deadbeef";
    let result = projection_result(vec![axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        Some("el:subClassOf-transitive"),
    )]);
    let edges = vec![
        ("http://example.org/first".to_owned(), CLASS.to_owned()),
        ("http://example.org/second".to_owned(), CLASS.to_owned()),
    ];
    let (ttl, _) = paired_closure(&result, &edges);
    for expression in ["first", "second"] {
        assert!(
            ttl.contains(&format!(
                "<http://example.org/{expression}> <{MATH_ALPHA_EQUIVALENCE_CLASS}> <{CLASS}> ."
            )),
            "the α-class edge of {expression} must be an ordinary joinable triple"
        );
    }
    let typing = format!("<{CLASS}> <{RDF_TYPE}> <{MATH_ALPHA_EQUIVALENCE_CLASS_TYPE}> .");
    assert_eq!(
        ttl.matches(typing.as_str()).count(),
        1,
        "the shared class individual is typed exactly ONCE, not once per expression"
    );
    assert!(
        ttl.contains("rule/math-expression-identity"),
        "the α edges carry the expression-identity derivation's own rule provenance"
    );
}

/// No `math:` expression in the EDB means no identity was decided, so the section — banner
/// included — is absent. A bare header would read as a decision that never happened.
#[test]
fn closure_omits_the_alpha_section_entirely_when_no_expression_is_decided() {
    let result = projection_result(vec![axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        Some("el:subClassOf-transitive"),
    )]);
    let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
    assert!(!ttl.contains("alpha-equivalence"));
    assert!(!ttl.contains(MATH_ALPHA_EQUIVALENCE_CLASS));
}

#[test]
fn closure_skips_edb_axioms() {
    let mut edb = axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/B",
        None,
    );
    edb.is_edb = true;
    let result = projection_result(vec![edb]);
    let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
    assert!(!ttl.contains("reifies"));
}

#[test]
fn closure_missing_rule_name_fails_loud() {
    let result = projection_result(vec![axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        None,
    )]);
    let err = build_inferred_closure_ttl(&result, None, &[]).unwrap_err();
    assert!(err.message().contains("no rule_name"), "got: {err}");
}

#[test]
fn explanations_emit_derivation_with_premise() {
    let derived = axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        Some("el:subClassOf-transitive"),
    );
    let receipt = receipt_for_axiom(&derived);
    let canonical_rule = canonical_rule_iri("el:subClassOf-transitive");
    let result = projection_result(vec![derived]);
    let ttl = build_explanations_ttl(&result).unwrap();
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/Derivation>"));
    assert!(ttl.contains("gmeow/concludes> <<( "));
    assert!(ttl.contains("gmeow/hasPremise> <<( <http://example.org/A>"));
    assert!(ttl.contains(&format!("<{}> <{canonical_rule}>", gmeow("viaRule"))));
    assert!(
        !ttl.contains("<el:subClassOf-transitive>"),
        "the raw firing label is receipt data, never a public rule resource"
    );
    assert!(ttl.contains("receipt-rule-identity"));
    assert!(ttl.contains("el:subClassOf-transitive"));
    assert!(ttl.contains(&receipt.row.derivation_id));
    assert!(ttl.contains("\"derivation of an inferred axiom\"@en"));
}

#[test]
fn modal_artifacts_retain_the_exact_rule_sources_and_derivation_identity() {
    let evidence = crate::modal::ModalEvaluation {
        context: "https://example.org/modal/frame".to_owned(),
        formula: "https://example.org/modal/F".to_owned(),
        operator: crate::modal::ModalOp::Box,
        body: "https://example.org/modal/B".to_owned(),
        evaluation_world: "https://example.org/modal/w0".to_owned(),
        accessibility_relation: crate::modal::TYPED_ACCESSIBILITY[0].to_owned(),
        atom_subject: "https://example.org/modal/a".to_owned(),
        atom_predicate: "https://example.org/modal/knows".to_owned(),
        atom_object: "https://example.org/modal/b".to_owned(),
        frontier: crate::modal::ModalFrontier::CompletedFinitePredecessor {
            worlds: vec![crate::modal::ModalWorldEvidence {
                world: "https://example.org/modal/w1".to_owned(),
                atom_present: false,
            }],
        },
        conclusion_predicate: crate::modal::MODAL_NECESSITY_FAILS.to_owned(),
        conclusion_object: "https://example.org/modal/B".to_owned(),
    };
    let modal = InferredAxiom {
        subject: evidence.formula.clone(),
        predicate: evidence.conclusion_predicate.clone(),
        object: purrdf::TermValue::iri(&evidence.conclusion_object),
        world: evidence.context.clone(),
        is_edb: false,
        rule_name: Some(crate::modal::MODAL_RULE_IRI.to_owned()),
        premises: evidence
            .positive_premises()
            .into_iter()
            .map(|p| (p.subject, p.predicate, p.object))
            .collect(),
        modal_evaluation: Some(Box::new(evidence)),
    };
    let receipt = receipt_for_axiom(&modal);
    let result = projection_result(vec![modal]);

    let (closure, native) = paired_closure(&result, &[]);
    assert_eq!(
        native.quad_count(),
        0,
        "a modal claim must never become an unconditional default-graph fact"
    );
    let explanations = build_explanations_ttl(&result).unwrap();
    for artifact in [&closure, &explanations] {
        assert!(artifact.contains(&format!(
            "<{}> <{}>",
            gmeow("viaRule"),
            crate::modal::MODAL_RULE_IRI
        )));
        assert!(artifact.contains("receipt-rule-identity"));
        assert!(artifact.contains(&receipt.row.derivation_id));
        for source in &receipt.row.source_quad_ids {
            assert!(artifact.contains(source));
        }
    }
    assert!(
        explanations.contains(&format!("<{}>", receipt.row.derivation_id)),
        "the derivation is a named content-addressed resource"
    );
}

#[test]
fn native_closure_keeps_duplicate_proofs_separate_from_quoted_data_blanks() {
    let mut derived = axiom(
        "urn:projection:s",
        "urn:projection:p",
        "urn:projection:o",
        Some("projection:test-rule"),
    );
    derived.object = TermValue::Triple {
        s: (TermValue::Blank {
            label: "proof0".to_owned(),
            scope: BlankScope(1),
        })
        .into(),
        p: TermValue::iri("urn:projection:quoted").into(),
        o: TermValue::simple_literal("quoted native value").into(),
    };
    let result = projection_result(vec![derived.clone(), derived]);
    let (_, native) = paired_closure(&result, &[]);
    let proofs: Vec<_> = native.owned_reifiers().map(|r| r.reifier).collect();
    assert_eq!(
        proofs.len(),
        2,
        "identical derivations retain separate anonymous proof occurrences"
    );
    assert_ne!(proofs[0], proofs[1]);
    for proof in proofs {
        let RdfTerm::BlankNode(label) = proof else {
            panic!("proof reifiers remain anonymous");
        };
        assert_ne!(
            BlankScope::unqualify_label(&label).1,
            BlankScope(1),
            "proofs must not alias quoted data"
        );
    }
}

#[test]
fn native_closure_appends_without_aliasing_an_existing_carrier_blank() {
    let result = projection_result(vec![axiom(
        "urn:projection:s",
        "urn:projection:p",
        "urn:projection:o",
        Some("projection:test-rule"),
    )]);
    let mut builder = RdfDatasetBuilder::new();
    let existing = builder.intern_blank("proof0", BlankScope(1));
    let p = builder.intern_iri("urn:projection:existing");
    let o = builder.intern_iri("urn:projection:value");
    builder.push_quad(existing, p, o, None);
    build_inferred_closure_into(&result, &[], &mut builder).unwrap();
    let native = builder.freeze().unwrap();
    let proof = native.owned_reifiers().next().unwrap().reifier;
    let existing = native
        .owned_quads()
        .find(|q| q.predicate == "urn:projection:existing")
        .unwrap()
        .subject;
    assert_ne!(proof, existing);
}

#[test]
fn ledger_header_entries_gaps_and_counts() {
    let mut result = projection_result(vec![axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        Some("el:subClassOf-transitive"),
    )]);
    result.preservation = PreservationClaim::for_unsupported(["complementOf".to_owned()]);
    result.provenance.projection_class = result.preservation.clone();
    result.information = InformationState::Both;
    result.provenance.contradiction_witnesses = vec![crate::result::ContradictionWitness {
        individual: "http://example.org/x".to_owned(),
        world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
        premises: vec![],
    }];
    result.validate().unwrap();
    let ttl = build_dl_el_ledger_ttl(&result).unwrap();
    assert!(ttl.contains(&format!("gmeow/consistent> \"false\"^^<{XSD_BOOLEAN}>")));
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/CrosscheckLedger>"));
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/LedgerEntry>"));
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/gmeow/DlGap>"));
    assert!(ttl.contains("reason.dl-gap.complementOf"));
    assert!(ttl.contains(&format!("gmeow/entailmentCount> \"1\"^^<{XSD_INTEGER}>")));
    assert!(ttl.contains(&format!("gmeow/gapCount> \"1\"^^<{XSD_INTEGER}>")));
}

#[test]
fn reasoning_result_ttl_emits_status_fields_and_certificate() {
    // A consistent run: supported, completed, complete-for-fragment.
    let result = native_result(vec![]);
    let ttl = build_reasoning_result_ttl(&result);
    assert!(ttl.contains("#type> <https://blackcatinformatics.ca/logic/ReasoningResult>"));
    assert!(ttl.contains("logic/resultInput> <https://blackcatinformatics.ca/logic/InputValid>"));
    assert!(ttl.contains(
        "logic/resultEvaluation> <https://blackcatinformatics.ca/logic/EvaluationCompleted>"
    ));
    assert!(ttl.contains(
        "logic/resultCompleteness> <https://blackcatinformatics.ca/logic/CompleteForFragment>"
    ));
    assert!(
        ttl.contains(
            "logic/resultInformation> <https://blackcatinformatics.ca/logic/InfoSupported>"
        )
    );
    assert!(ttl.contains("logic/contractHash>"));
    assert!(ttl.contains("logic/engine>"));
}

#[test]
fn reasoning_result_ttl_inconsistent_is_both_with_witness() {
    // An inconsistent run: information=both, carrying a contradiction witness.
    let result = native_result(vec![RdfQuad::new(
        RdfTerm::iri("http://example.org/x"),
        "https://blackcatinformatics.ca/logic/instanceOf",
        RdfTerm::iri("https://blackcatinformatics.ca/logic/Nothing"),
    )]);
    let ttl = build_reasoning_result_ttl(&result);
    assert!(
        ttl.contains("logic/resultInformation> <https://blackcatinformatics.ca/logic/InfoBoth>")
    );
    assert!(
        ttl.contains("logic/contradictionWitness> <http://example.org/x>"),
        "the glut must carry its witness: {ttl}"
    );
}

#[test]
fn proof_and_counterproof_derivation_ids_are_sanitized_by_bare_iri() {
    // bare_iri strips a surrounding `<>` pair from a derivation_id.
    // A derivation_id stored as "<urn:x>" must emit as `<urn:x>`, NOT `<<urn:x>>`.
    use crate::result::{DerivationRef, InformationState};
    use std::collections::BTreeSet;

    let mut result = projection_result(vec![]);
    // Inject a proof and counterproof whose derivation_id is pre-wrapped in `<>`.
    // This simulates a derivation_id that accidentally carries angle-bracket delimiters.
    result.provenance.proof = Some(DerivationRef {
        derivation_id: "<urn:proof-x>".to_owned(),
        cited_iris: BTreeSet::new(),
    });
    result.provenance.counterproof = Some(DerivationRef {
        derivation_id: "<urn:counterproof-x>".to_owned(),
        cited_iris: BTreeSet::new(),
    });
    // The projection carries both explicit derivation references.
    result.information = InformationState::Both;
    result.validate().unwrap();

    let ttl = build_reasoning_result_ttl(&result);

    // The emitted lines must use exactly one pair of angle brackets, not doubled.
    assert!(
        ttl.contains("logic/resultProof> <urn:proof-x>"),
        "bare_iri must strip the surrounding <> from the proof derivation_id; got:\n{ttl}"
    );
    assert!(
        ttl.contains("logic/resultCounterproof> <urn:counterproof-x>"),
        "bare_iri must strip the surrounding <> from the counterproof derivation_id; got:\n{ttl}"
    );
    // Regression guard: <<urn:…>> must NOT appear (double angle brackets = invalid Turtle).
    assert!(
        !ttl.contains("<<urn:"),
        "double angle-bracket leaked into Turtle output; got:\n{ttl}"
    );
}
