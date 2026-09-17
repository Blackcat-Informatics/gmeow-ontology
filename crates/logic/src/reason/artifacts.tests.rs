// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::reason::dl::{DlCoverage, DlVerdict, InconsistencyWitness};
use crate::reason::el::InferredAxiom;
use crate::result::ResultProvenance;

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

#[test]
fn corpus_derived_objects_and_premises_keep_rdf12_types_in_both_artifacts() {
    let quoted = RdfTerm::triple(RdfTriple::new(
        RdfTerm::iri("urn:inner:subject"),
        "urn:inner:predicate",
        RdfTerm::literal(RdfLiteral::typed(
            "quoted \"value\"\nline",
            "http://www.w3.org/2001/XMLSchema#string",
        )),
    ));
    for object in [
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
    ] {
        let surface = emit_term(&object);
        let mut builder = RdfDatasetBuilder::new();
        let object_id = builder.intern_owned_term(&object);
        builder.push_owned_quad(&RdfQuad::new(
            RdfTerm::iri("urn:source"),
            "urn:property",
            object.clone(),
        ));
        let native = builder.freeze().unwrap();
        let mut derived = axiom(
            "urn:result",
            "urn:property",
            "urn:value",
            Some("native:typed"),
        );
        derived.object = native.term_value(object_id);
        derived.world = crate::reason::rl::DEFAULT_WORLD.into();
        derived
            .premises
            .push(("urn:premise".into(), "urn:property".into(), surface));
        let result = result_with(vec![derived], true);
        let closure = build_inferred_closure_ttl(&result, None, &[]).unwrap();
        let closure = purrdf::parse_dataset(closure.as_bytes(), "text/turtle", None)
            .expect("the closure retains typed derived objects");
        assert!(closure.owned_quads().any(|quad| {
            quad.subject == RdfTerm::iri("urn:result")
                && quad.predicate == "urn:property"
                && quad.object == object
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
            crate::reason::term_value_to_rdf_term(&closure.term_value(typed.o)).unwrap(),
            object,
            "runtime delta publication retains the same RDF 1.2 term",
        );
        let empty = RdfDataset::union(&[]);
        let crate::verify::ReasonedGraphOutcome::Ready(verified) =
            crate::verify::materialize_reasoned_graph(&empty, &result)
                .expect("the downstream verifier accepts typed derived objects")
        else {
            panic!("the synthetic result has no incomplete closure");
        };
        assert!(verified.dataset.owned_quads().any(|quad| {
            quad.subject == RdfTerm::iri("urn:result")
                && quad.predicate == "urn:property"
                && quad.object == object
        }));
        assert!(verified.derived_predicates.contains("urn:property"));
        let explanations = build_explanations_ttl(&result).unwrap();
        let explanations = purrdf::parse_dataset(explanations.as_bytes(), "text/turtle", None)
            .expect("explanations retain recursive conclusion and premise terms");
        for (property, subject) in [("concludes", "urn:result"), ("hasPremise", "urn:premise")] {
            let expected = RdfTerm::triple(RdfTriple::new(
                RdfTerm::iri(subject),
                "urn:property",
                object.clone(),
            ));
            assert!(
                explanations
                    .owned_quads()
                    .any(|quad| { quad.predicate == gmeow(property) && quad.object == expected }),
                "{property}"
            );
        }
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
        let result = result_with(
            vec![axiom(
                "urn:result",
                "urn:property",
                object,
                Some("native:typed"),
            )],
            true,
        );
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
        emit_term(&premise_object("\"plain\"").unwrap()),
        "\"plain\"^^<http://www.w3.org/2001/XMLSchema#string>"
    );
}

fn result_with(inferred: Vec<InferredAxiom>, consistent: bool) -> ReasoningResult {
    let verdict = DlVerdict {
        consistent,
        unsatisfiable_classes: vec![],
        // An inconsistent verdict folds to information=both, which requires a
        // justifying witness; supply one so the (debug-asserted) invariant holds.
        inconsistencies: if consistent {
            vec![]
        } else {
            vec![InconsistencyWitness {
                individual: "http://example.org/x".to_owned(),
                world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
                premises: vec![],
            }]
        },
        coverage: DlCoverage {
            present: vec![],
            decided: vec![],
            unsupported: vec![],
        },
        gaps: vec![],
        boundary_findings: vec![],
    };
    ReasoningResult::from_dl_verdict(inferred, &verdict, prov())
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
    let derived = axiom(
        "http://example.org/A",
        RDFS_SUBCLASS_OF,
        "http://example.org/C",
        Some("el:subClassOf-transitive"),
    );
    let receipt = receipt_for_axiom(&derived);
    let canonical_rule = canonical_rule_iri("el:subClassOf-transitive");
    assert_eq!(receipt.row.rule_iri, canonical_rule);
    assert_eq!(receipt.raw_rule_identity, "el:subClassOf-transitive");
    assert_eq!(
        receipt.row.derivation_id,
        crate::provenance::mint_derivation_id(
            "el:subClassOf-transitive",
            &[receipt.row.source_quad_ids[0].as_str()]
        ),
        "the receipt hash preserves the native firing identity bytes"
    );
    let result = result_with(vec![derived], true);
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

/// The α-equivalence section is the SHIPPED half of the expression-identity derivation:
/// two α-equivalent expressions must land on ONE class individual, that individual must be
/// typed exactly once however many expressions reach it, and every edge must carry the
/// derivation's own rule provenance rather than borrowing an EL/DL rule's.
#[test]
fn closure_emits_one_joinable_class_for_alpha_equivalent_expressions() {
    const CLASS: &str = "https://blackcatinformatics.ca/math/alphaClass/deadbeef";
    let result = result_with(
        vec![axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            Some("el:subClassOf-transitive"),
        )],
        true,
    );
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
    let result = result_with(
        vec![axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            Some("el:subClassOf-transitive"),
        )],
        true,
    );
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
    let result = result_with(vec![edb], true);
    let ttl = build_inferred_closure_ttl(&result, None, &[]).unwrap();
    assert!(!ttl.contains("reifies"));
}

#[test]
fn closure_missing_rule_name_fails_loud() {
    let result = result_with(
        vec![axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            None,
        )],
        true,
    );
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
    let result = result_with(vec![derived], true);
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
    let result = result_with(vec![modal], true);

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
        s: Box::new(TermValue::Blank {
            label: "proof0".to_owned(),
            scope: BlankScope(1),
        }),
        p: Box::new(TermValue::iri("urn:projection:quoted")),
        o: Box::new(TermValue::simple_literal("quoted native value")),
    };
    let result = result_with(vec![derived.clone(), derived], true);
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
    let result = result_with(
        vec![axiom(
            "urn:projection:s",
            "urn:projection:p",
            "urn:projection:o",
            Some("projection:test-rule"),
        )],
        true,
    );
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
    let verdict = DlVerdict {
        consistent: false,
        unsatisfiable_classes: vec![],
        // information=both needs a justifying witness (invariant).
        inconsistencies: vec![InconsistencyWitness {
            individual: "http://example.org/x".to_owned(),
            world: "https://blackcatinformatics.ca/gmeow/graph/imports".to_owned(),
            premises: vec![],
        }],
        coverage: DlCoverage {
            present: vec!["complementOf".to_owned()],
            decided: vec![],
            unsupported: vec!["complementOf".to_owned()],
        },
        // gaps are reconstructed from coverage.unsupported by the builder, so
        // the input gaps here are immaterial to the ledger output.
        gaps: vec![],
        boundary_findings: vec![],
    };
    let result = ReasoningResult::from_dl_verdict(
        vec![axiom(
            "http://example.org/A",
            RDFS_SUBCLASS_OF,
            "http://example.org/C",
            Some("el:subClassOf-transitive"),
        )],
        &verdict,
        prov(),
    );
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
    let result = result_with(vec![], true);
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
    let result = result_with(vec![], false);
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

    let mut result = result_with(vec![], true);
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
    // Force information=both so validate() does not fire the glut-needs-witness
    // invariant. We override the information state directly; the unit test is
    // checking IRI sanitization, not state-machine rules.
    result.information = InformationState::Both;

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
