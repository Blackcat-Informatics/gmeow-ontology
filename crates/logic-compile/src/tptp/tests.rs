// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

use purrdf::{CompositeDatasetView, ViewLimits, parse_dataset};

use super::{FofProjectionStatus, project_tptp_fof};
use crate::frontend::{
    DocumentSelection, LogicParseError, PreparedLogicSource, SourceAdmission, SourceBase,
    SourceBaseOrigin, SourceDocument, SourceSelection,
};

fn compile(source: &str) -> crate::frontend::CompiledTheory {
    let dataset = parse_dataset(source.as_bytes(), "text/turtle", None).expect("valid RDF 1.2");
    PreparedLogicSource::new(&dataset)
        .expect("prepared source")
        .into_compiled(None)
        .expect("compiled source")
}

fn compile_documents(
    sources: &[(&str, &str, Option<SourceBase>)],
) -> crate::frontend::CompiledTheory {
    let documents: Vec<_> = sources
        .iter()
        .map(|(path, source, base)| {
            (
                SourceDocument {
                    path: (*path).to_owned(),
                    content_digest: format!("test-digest:{path}"),
                    role: "synthetic-test".to_owned(),
                    base: base.clone(),
                },
                parse_dataset(source.as_bytes(), "text/turtle", None).expect("valid RDF 1.2"),
            )
        })
        .collect();
    let composite = CompositeDatasetView::new(
        documents
            .iter()
            .map(|(_, dataset)| dataset.clone())
            .collect(),
        ViewLimits {
            max_sources: sources.len(),
            ..ViewLimits::default()
        },
    )
    .expect("admitted source composition");
    let materialized = composite.materialize().expect("materialized selection");
    let mut prepared = PreparedLogicSource::new(&materialized).expect("prepared selection");
    for (index, (receipt, source)) in documents.iter().enumerate() {
        prepared
            .record_document(receipt.clone(), source, |term| {
                let value = composite.term_value(composite.source_id(index, term));
                materialized
                    .as_ref()
                    .term_id_by_value(&value)
                    .ok_or_else(|| {
                        LogicParseError(format!(
                            "source term in {:?} has no materialized binding",
                            receipt.path
                        ))
                    })
            })
            .expect("document receipt");
    }
    prepared
        .into_compiled(None)
        .expect("compiled source selection")
}

fn project(source: &str) -> (SourceAdmission, super::FofProjection) {
    let theory = compile(source);
    let selection = SourceSelection::all_classical_roots(&theory);
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &admission);
    (admission, projection)
}

#[test]
fn ordinary_domain_assertions_and_subclass_law_reach_the_problem() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:A a logic:Class .
           ex:B a logic:Class .
           ex:alice a ex:A .
           ex:A logic:subClassOf ex:B ."#,
    );
    assert_eq!(projection.status, FofProjectionStatus::Complete);
    let subclass = projection
        .sentences
        .iter()
        .find(|sentence| {
            !sentence.source_units.is_empty()
                && sentence.body.contains("=>")
                && sentence.body.contains("I|https://example.org/A")
                && sentence.body.contains("I|https://example.org/B")
        })
        .expect("the selected subclass axiom retains its membership implication");
    assert!(
        !subclass.body.contains("<=>"),
        "subsumption must not become equivalence"
    );
    let problem = projection.problem.expect("complete problem");
    assert!(
        problem.contains("I|https://example.org/alice")
            && problem.contains("I|https://example.org/A"),
        "ordinary rdf:type assertion must not disappear: {problem}"
    );
}

#[test]
fn structural_owner_does_not_hide_an_unrelated_domain_assertion() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix principia: <https://example.org/principia/> .
           @prefix ex: <https://example.org/> .
           ex:formula a logic:Formula ;
               logic:relation ex:holds ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ] ;
               principia:transitionSourceContentSpace ex:space ."#,
    );
    let problem = projection
        .problem
        .expect("ordinary assertion on a structural owner must project");
    assert!(
        problem.contains("A|2'('I|https://example.org/principia/transitionSourceContentSpace"),
        "the structural declaration must not erase an unrelated domain predicate: {problem}"
    );
    assert!(problem.contains("I|https://example.org/space"), "{problem}");
}

#[test]
fn scalar_formula_grammar_never_becomes_domain_predicates() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:formula a logic:Formula ;
               logic:relation ex:holds ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ] ."#,
    );
    let problem = projection.problem.expect("supported formula projects");
    assert!(
        problem.contains("A|1'('I|https://example.org/holds"),
        "{problem}"
    );
    for syntax in ["termIndex", "termIri", "argument", "relation"] {
        assert!(
            !problem.contains(&format!(
                "'I|https://blackcatinformatics.ca/logic/{syntax}'"
            )),
            "reserved formula grammar leaked as a domain predicate: {problem}"
        );
    }
}

#[test]
fn procedural_term_comparison_blocks_the_whole_problem() {
    let (admission, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:left logic:termEqual ex:right ."#,
    );
    assert_ne!(
        admission.status,
        crate::frontend::SourceAdmissionStatus::Complete
    );
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| { blocker.code == "PROCEDURAL_RELATION_REQUIRES_BINDING_SEMANTICS" })
    );
}

#[test]
fn denotational_identity_uses_equality_without_a_unique_name_axiom() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:left logic:sameAs ex:right .
           ex:left logic:differentFrom ex:third ."#,
    );
    let problem = projection.problem.expect("complete identity problem");
    assert!(
        problem.contains(" = "),
        "sameAs must use denotational equality: {problem}"
    );
    assert!(
        problem.contains(" != "),
        "differentFrom must use inequality: {problem}"
    );
    assert!(
        !problem.contains("$distinct") && !problem.contains('"'),
        "constants must not introduce a unique-name assumption: {problem}"
    );
}

#[test]
fn quoted_payload_remains_unasserted() {
    let (admission, projection) = project(
        r#"@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
           @prefix ex: <https://example.org/> .
           ex:kept ex:predicate ex:value .
           ex:quote rdf:reifies <<( ex:hidden ex:predicate ex:payload )>> ."#,
    );
    assert!(admission.units.iter().any(|unit| matches!(
        unit.disposition,
        crate::frontend::SemanticUnitDisposition::QuotedUnasserted { .. }
    )));
    let problem = projection.problem.expect("quotation alone does not block");
    assert!(problem.contains("I|https://example.org/kept"));
    assert!(
        !problem.contains("I|https://example.org/hidden")
            && !problem.contains("I|https://example.org/payload"),
        "quoted payload must never be promoted into the assertional theory: {problem}"
    );
}

#[test]
fn selected_formula_with_term_equal_is_a_required_blocker() {
    let (admission, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:module logic:hasFormula ex:formula .
           ex:formula a logic:Formula ;
               logic:relation logic:termEqual ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:left ] ,
                              [ logic:termIndex 1 ; logic:termIri ex:right ] ."#,
    );
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(admission.blockers().any(|unit| matches!(
        &unit.disposition,
        crate::frontend::SemanticUnitDisposition::Blocked { code, .. }
            if code == "UNSUPPORTED_SELECTED_FORMULA"
    )));
}

#[test]
fn deleting_blocker_rows_cannot_authorize_a_partial_proof_problem() {
    let theory = compile(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:kept ex:predicate ex:value .
           ex:module logic:hasFormula ex:formula .
           ex:formula a logic:Formula ;
               logic:relation logic:termEqual ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:left ] ,
                              [ logic:termIndex 1 ; logic:termIri ex:right ] ."#,
    );
    let mut admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert!(admission.blockers().next().is_some());
    admission.units.retain(|unit| {
        !matches!(
            unit.disposition,
            crate::frontend::SemanticUnitDisposition::Blocked { .. }
                | crate::frontend::SemanticUnitDisposition::Malformed { .. }
        )
    });
    admission.status = crate::frontend::SourceAdmissionStatus::Complete;
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Blocked,
        "removing required-unit refusals must not authorize a partial theory"
    );
    assert!(projection.problem.is_none());
}

#[test]
fn serialized_admission_is_observation_data_not_compiler_authority() {
    let theory = compile("<urn:s> <urn:p> <urn:o> .");
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let original = project_tptp_fof(&theory, &admission);
    assert_eq!(original.status, FofProjectionStatus::Complete);
    let mut wire = serde_json::to_value(&admission).unwrap();
    assert!(wire.get("checked_evidence").is_none());
    // Even an injected private-field spelling must not restore live authority.
    wire["checked_evidence"] = serde_json::json!(vec![0_u8; 32]);
    let receipt: SourceAdmission = serde_json::from_value(wire).unwrap();
    assert!(!receipt.matches(&theory));
    let refused = project_tptp_fof(&theory, &receipt);
    assert_eq!(refused.status, FofProjectionStatus::Blocked);
    assert!(refused.problem.is_none() && refused.sentences.is_empty());
    let readmitted =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &readmitted).problem_digest,
        original.problem_digest
    );
    assert!(admission.clone().matches(&theory));
}

#[test]
fn edited_profile_selection_and_symbol_tables_cannot_change_proof_semantics() {
    let theory = compile(
        "@prefix logic: <https://blackcatinformatics.ca/logic/> .
         <urn:class> a logic:Class . <urn:s> a <urn:class> .",
    );
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &admission).status,
        FofProjectionStatus::Complete
    );
    let refuses = |edited: SourceAdmission| {
        let projection = project_tptp_fof(&theory, &edited);
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
        assert!(projection.problem.is_none() && projection.sentences.is_empty());
    };
    let mut edited = admission.clone();
    edited.profile.equality = "caller-selected identity semantics".to_owned();
    refuses(edited);
    let mut edited = admission.clone();
    edited.selection.rules = crate::frontend::RootSelection::None;
    edited.selection_digest = edited.selection.digest();
    refuses(edited);
    let mut edited = admission.clone();
    edited.declared_classes.clear();
    refuses(edited);
    let mut edited = admission.clone();
    edited
        .declared_builtins
        .insert("urn:caller-builtin".to_owned());
    refuses(edited);
}

#[test]
fn rewriting_an_admitted_assertion_cannot_reuse_its_old_source_unit_identity() {
    let theory = compile("<urn:s> <urn:p> <urn:o> .");
    let mut admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let assertion = admission
        .units
        .iter_mut()
        .find_map(|unit| match &mut unit.disposition {
            crate::frontend::SemanticUnitDisposition::Admitted(
                crate::frontend::AdmittedSemanticUnit::Assertion(assertion),
            ) => Some(assertion),
            _ => None,
        })
        .expect("the source assertion must reach admission");
    assertion.object = crate::ir::AtomicTerm::Iri("urn:forged-object".to_owned());
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none() && projection.sentences.is_empty());
}

#[test]
fn admission_identity_includes_empty_named_graph_declarations() {
    let compile_with_context = |context: &str| {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        let s = builder.intern_iri("urn:s").unwrap();
        let p = builder.intern_iri("urn:p").unwrap();
        let o = builder.intern_iri("urn:o").unwrap();
        let graph = builder.intern_iri(context).unwrap();
        builder.push_quad(s, p, o, None).unwrap();
        builder.declare_named_graph(graph).unwrap();
        let data = builder.freeze().unwrap();
        PreparedLogicSource::new(&data)
            .unwrap()
            .into_compiled(None)
            .unwrap()
    };
    let left = compile_with_context("urn:empty-context:left");
    let right = compile_with_context("urn:empty-context:right");
    assert_eq!(left.source().dataset().named_graphs().count(), 1);
    assert_eq!(right.source().dataset().named_graphs().count(), 1);
    let admission =
        SourceAdmission::classical_fof(&left, SourceSelection::all_classical_roots(&left));
    let other =
        SourceAdmission::classical_fof(&right, SourceSelection::all_classical_roots(&right));
    assert_ne!(admission.source_digest, other.source_digest);
    assert!(!admission.matches(&right));
    assert_eq!(
        project_tptp_fof(&left, &admission).status,
        FofProjectionStatus::Complete
    );
    assert_eq!(
        project_tptp_fof(&right, &admission).status,
        FofProjectionStatus::Blocked
    );
}

#[test]
fn property_characteristic_record_emits_the_law_once() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:characteristic a logic:PropertyCharacteristicAssertion ;
               logic:characterizes ex:identifier ;
               logic:characteristicSort logic:functionalProperty .
           ex:one ex:identifier ex:value ."#,
    );
    let problem = projection
        .problem
        .expect("supported characteristic problem");
    assert!(
        problem.contains("V_x,V_y,V_z") && problem.contains("V_y = V_z"),
        "functional record must become its certified first-order law: {problem}"
    );
}

#[test]
fn full_literal_identity_survives_as_an_ordinary_constant() {
    let (_, projection) = project(
        r#"@prefix ex: <https://example.org/> .
           ex:s ex:value "bonjour"@fr--rtl ."#,
    );
    let problem = projection.problem.expect("literal problem");
    assert!(
        problem.contains("bonjour"),
        "lexical form survives: {problem}"
    );
    assert!(problem.contains("fr"), "language survives: {problem}");
    assert!(
        problem.contains("rtl"),
        "base direction survives: {problem}"
    );
    assert!(problem.is_ascii(), "TPTP problem must be printable ASCII");
}

#[test]
fn exact_document_selection_uses_composition_bindings_without_reparsing() {
    let theory = compile_documents(&[
        (
            "a.ttl",
            r#"@prefix ex: <https://example.org/> . ex:alice ex:status ex:admitted ."#,
            None,
        ),
        (
            "b.ttl",
            r#"@prefix ex: <https://example.org/> . ex:bob ex:status ex:excluded ."#,
            None,
        ),
    ]);
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["a.ttl".to_owned()]));
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &admission);
    let problem = projection.problem.expect("selected document projects");
    assert!(problem.contains("I|https://example.org/alice"), "{problem}");
    assert!(
        problem.contains("I|https://example.org/admitted"),
        "{problem}"
    );
    assert!(!problem.contains("I|https://example.org/bob"), "{problem}");
    assert!(
        !problem.contains("I|https://example.org/excluded"),
        "{problem}"
    );
}

#[test]
fn unselected_class_expression_does_not_block_selected_document() {
    let theory = compile_documents(&[
        (
            "selected.ttl",
            "<https://example.org/alice> <https://example.org/status> <https://example.org/ready> .",
            None,
        ),
        (
            "restriction.ttl",
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
               @prefix ex: <https://example.org/> .
               ex:Whole logic:subClassOf [ a logic:Restriction ;
                   logic:onProperty ex:part ; logic:someValuesFrom ex:Part ;
                   logic:minQualifiedCardinality 1 ; logic:onClass ex:Part ] ."#,
            None,
        ),
    ]);
    assert!(
        theory
            .axiom_sources()
            .iter()
            .flatten()
            .any(|source| { matches!(source, crate::frontend::AxiomSource::ClassExpression(_)) })
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["selected.ttl".into()]));
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let problem = projection
        .problem
        .expect("only the selected source is required");
    assert!(problem.contains("I|https://example.org/alice"), "{problem}");
    assert!(
        !problem.contains("I|https://example.org/Whole"),
        "{problem}"
    );

    let selected =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let projection = project_tptp_fof(&theory, &selected);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| { blocker.code == "UNSUPPORTED_CLASS_EXPRESSION" })
    );
}

#[test]
fn deduplicated_class_axiom_remains_required_with_one_selected_contributor() {
    let source = r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
                   @prefix ex: <https://example.org/> .
                   ex:Whole logic:subClassOf [ a logic:Restriction ;
                       logic:onProperty ex:part ; logic:someValuesFrom ex:Part ;
                       logic:minQualifiedCardinality 1 ; logic:onClass ex:Part ] ."#;
    let theory = compile_documents(&[("first.ttl", source, None), ("second.ttl", source, None)]);
    assert!(
        theory.axiom_sources().iter().any(|sources| {
            sources
                .iter()
                .filter(|source| matches!(source, crate::frontend::AxiomSource::ClassExpression(_)))
                .count()
                == 2
        }),
        "the control requires actual canonical axiom deduplication"
    );
    for selected in ["first.ttl", "second.ttl"] {
        let mut selection = SourceSelection::all_classical_roots(&theory);
        selection.documents = DocumentSelection::Exact(BTreeSet::from([selected.into()]));
        let admission = SourceAdmission::classical_fof(&theory, selection);
        let projection = project_tptp_fof(&theory, &admission);
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
        assert!(projection.problem.is_none());
        assert!(
            projection
                .blockers
                .iter()
                .any(|blocker| { blocker.code == "UNSUPPORTED_CLASS_EXPRESSION" }),
            "selected contributor {selected} cannot be excluded by deduplication"
        );
    }
}

#[test]
fn unselected_contextual_reification_does_not_block_selected_document() {
    let theory = compile_documents(&[
        (
            "selected.ttl",
            "<https://example.org/alice> <https://example.org/status> <https://example.org/ready> .",
            None,
        ),
        (
            "claim.ttl",
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
               @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
               @prefix ex: <https://example.org/> .
               ex:claim rdf:reifies <<(ex:Whole logic:subClassOf ex:Part)>> ;
                   logic:standpoint ex:observer ."#,
            None,
        ),
    ]);
    assert!(
        theory
            .axiom_sources()
            .iter()
            .flatten()
            .any(|source| { matches!(source, crate::frontend::AxiomSource::Reification(_)) })
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["selected.ttl".into()]));
    let admission = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let problem = projection.problem.expect("selected source projects");
    assert!(
        !problem.contains("I|https://example.org/Whole"),
        "{problem}"
    );
    assert!(
        !problem.contains("I|https://example.org/observer"),
        "{problem}"
    );

    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| { blocker.code == "CONTEXTUAL_REIFICATION_REQUIRES_TYPED_TRANSLATION" })
    );
}

#[test]
fn boolean_source_class_laws_define_membership_without_asserting_list_syntax() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:Both logic:intersectionOf (ex:Left ex:Right) .
           ex:Either logic:unionOf (ex:Left ex:Right) .
           ex:Other logic:complementOf ex:Both .
           ex:Named logic:oneOf (ex:a ex:b) ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    for (class, operator) in [
        ("Both", " & "),
        ("Either", " | "),
        ("Other", "~"),
        ("Named", " = "),
    ] {
        let law = projection.sentences.iter().find(|sentence| {
            sentence.body.contains("<=>")
                && sentence
                    .body
                    .contains(&format!("I|https://example.org/{class}"))
                && sentence.body.contains(operator)
        });
        assert!(
            law.is_some(),
            "missing {class} membership law: {projection:?}"
        );
    }
    let problem = projection.problem.unwrap();
    for syntax in [
        "unionOf",
        "intersectionOf",
        "complementOf",
        "oneOf",
        "first",
        "rest",
    ] {
        assert!(
            !problem.contains(&format!("/{syntax}'")) && !problem.contains(&format!("#{syntax}'")),
            "{problem}"
        );
    }
}

#[test]
fn singleton_boolean_classes_keep_formula_arity() {
    for (constructor, members, expected) in [
        ("unionOf", "(ex:Only)", "I|https://example.org/Only"),
        ("intersectionOf", "(ex:Only)", "I|https://example.org/Only"),
        ("oneOf", "(ex:Only)", " = "),
    ] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
             @prefix ex: <https://example.org/> . ex:C logic:{constructor} {members} ."
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{constructor} {members}: {projection:?}"
        );
        let law = projection
            .sentences
            .iter()
            .find(|sentence| sentence.body.contains("<=>"))
            .unwrap();
        assert!(law.body.contains(expected), "{law:?}");
        assert!(
            !law.body.contains(" & ") && !law.body.contains(" | "),
            "{law:?}"
        );
    }
}

#[test]
fn boolean_classes_refuse_malformed_uninterpreted_and_excluded_support() {
    for definition in [
        "ex:C logic:unionOf _:list . _:list <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> ex:A ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:list .",
        "ex:C logic:unionOf (ex:A) ; logic:complementOf ex:B .",
        "ex:C logic:oneOf (\"literal\") .",
        "ex:C logic:unionOf (xsd:integer ex:A) .",
        "ex:C a logic:Datarange ; logic:unionOf (ex:A ex:B) .",
        "ex:C logic:unionOf (ex:A ex:B) ; logic:standpoint ex:observer .",
        "ex:C logic:unionOf (ex:Unsupported) . ex:Unsupported logic:onProperty ex:p ; logic:minQualifiedCardinality 2 ; logic:onClass ex:A .",
    ] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . \
             @prefix ex: <https://example.org/> . {definition}"
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{definition}: {projection:?}"
        );
        assert!(projection.problem.is_none());
    }
    let theory = compile_documents(&[
        (
            "owner.ttl",
            "<https://example.org/C> <https://blackcatinformatics.ca/logic/unionOf> <https://example.org/list> .",
            None,
        ),
        (
            "list.ttl",
            "<https://example.org/list> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://example.org/A> ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
            None,
        ),
    ]);
    let all =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &all).status,
        FofProjectionStatus::Complete
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["owner.ttl".into()]));
    let selected = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &selected);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| blocker.reason.contains("class-list support"))
    );
}

#[test]
fn boolean_class_lists_fail_closed_instead_of_admitting_a_prefix() {
    for cells in [
        "ex:list rdf:first ex:A, ex:B ; rdf:rest rdf:nil .",
        "ex:list rdf:first ex:A ; rdf:rest rdf:nil, ex:tail .",
        "ex:list rdf:first ex:A .",
        "ex:list rdf:first ex:A ; rdf:rest ex:list .",
        "ex:list rdf:first ex:A ; rdf:rest rdf:nil . rdf:nil rdf:first ex:B .",
    ] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> . \
             @prefix ex: <https://example.org/> . ex:C logic:unionOf ex:list . {cells}"
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{cells}: {projection:?}"
        );
        assert!(projection.problem.is_none());
        assert!(
            projection
                .blockers
                .iter()
                .any(|blocker| blocker.code == "MALFORMED_CLASS_LIST"),
            "{projection:?}"
        );
    }
    for constructor in ["unionOf", "intersectionOf", "oneOf"] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
             <https://example.org/C> logic:{constructor} () ."
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{projection:?}"
        );
        assert!(projection.problem.is_none());
        assert!(
            projection
                .blockers
                .iter()
                .any(|blocker| blocker.code == "MALFORMED_CLASS_LIST")
        );
    }
}

#[test]
fn boolean_class_laws_compose_with_restrictions_and_retain_source_blank_identity() {
    let (admission, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:Whole logic:subClassOf _:outer .
           _:outer logic:onProperty ex:status ; logic:allValuesFrom _:choices .
           _:choices logic:oneOf (ex:ready ex:done) ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let definitions: Vec<_> = admission
        .admitted()
        .filter_map(|unit| {
            if let crate::frontend::SemanticUnitDisposition::Admitted(
                crate::frontend::AdmittedSemanticUnit::ClassDefinition(definition),
            ) = &unit.disposition
            {
                Some(definition)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(definitions.len(), 2);
    for definition in definitions {
        for value in definition.constants.values() {
            if let crate::ir::AtomicTerm::Blank(blank) = value {
                assert!(
                    projection
                        .problem
                        .as_ref()
                        .unwrap()
                        .contains(&format!("B|{blank}"))
                );
            }
        }
    }
    assert!(
        !projection
            .problem
            .unwrap()
            .contains("I|https://blackcatinformatics.ca/logic/enumeration/")
    );
}

#[test]
fn shared_boolean_list_cells_keep_each_selected_class_owner() {
    let theory = compile_documents(&[
        (
            "first.ttl",
            "<https://example.org/A> <https://blackcatinformatics.ca/logic/unionOf> <https://example.org/list> .",
            None,
        ),
        (
            "second.ttl",
            "<https://example.org/B> <https://blackcatinformatics.ca/logic/unionOf> <https://example.org/list> .",
            None,
        ),
        (
            "cells.ttl",
            "<https://example.org/list> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <https://example.org/Member> ; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
            None,
        ),
    ]);
    for (owner, excluded) in [("first.ttl", "B"), ("second.ttl", "A")] {
        let mut selection = SourceSelection::all_classical_roots(&theory);
        selection.documents =
            DocumentSelection::Exact(BTreeSet::from([owner.into(), "cells.ttl".into()]));
        let admission = SourceAdmission::classical_fof(&theory, selection);
        let projection = project_tptp_fof(&theory, &admission);
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "{owner}: {projection:?}"
        );
        let problem = projection.problem.unwrap();
        assert!(
            !problem.contains(&format!("I|https://example.org/{excluded}'")),
            "{problem}"
        );
        assert!(problem.contains("I|https://example.org/Member"));
    }
}

#[test]
fn source_restriction_laws_preserve_blank_identity_and_do_not_assert_syntax() {
    let (admission, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:Whole logic:subClassOf _:restriction .
           ex:link logic:range _:restriction .
           _:restriction a logic:Restriction ; logic:onProperty ex:part ;
               logic:someValuesFrom ex:Part ; logic:allValuesFrom ex:Part ;
               logic:hasValue ex:fixed ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let definitions: Vec<_> = admission
        .admitted()
        .filter_map(|unit| {
            if let crate::frontend::SemanticUnitDisposition::Admitted(
                crate::frontend::AdmittedSemanticUnit::ClassDefinition(definition),
            ) = &unit.disposition
            {
                Some(definition)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(definitions.len(), 1);
    let blank = definitions[0]
        .constants
        .values()
        .find_map(|value| {
            if let crate::ir::AtomicTerm::Blank(blank) = value {
                Some(blank)
            } else {
                None
            }
        })
        .expect("the formula retains the actual source blank");
    let law = projection
        .sentences
        .iter()
        .find(|sentence| sentence.body.contains("<=>"))
        .expect("membership is defined by equivalence, not merely implication");
    assert!(law.body.contains(&format!("B|{blank}")), "{law:?}");
    assert!(law.body.contains("(? ["), "{law:?}");
    assert_eq!(law.body.matches("(! [").count(), 2, "{law:?}");
    assert!(
        law.body.contains("A|2'('I|https://example.org/part"),
        "{law:?}"
    );
    assert!(law.body.contains("I|https://example.org/fixed"), "{law:?}");
    let problem = projection.problem.unwrap();
    assert!(
        !problem.contains("I|https://blackcatinformatics.ca/logic/onProperty"),
        "{problem}"
    );
    assert!(
        !problem.contains("I|https://blackcatinformatics.ca/logic/someValuesFrom"),
        "{problem}"
    );
    assert!(
        !problem.contains("I|https://blackcatinformatics.ca/logic/restriction/"),
        "{problem}"
    );
}

#[test]
fn restriction_dependencies_are_class_laws_not_recursive_syntax_expansion() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:recursive a logic:Restriction ; logic:onProperty ex:part ;
               logic:someValuesFrom ex:recursive ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    assert!(
        projection
            .problem
            .unwrap()
            .contains("I|https://example.org/recursive")
    );

    let (_, refused) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:outer a logic:Restriction ; logic:onProperty ex:part ;
               logic:someValuesFrom ex:middle .
           ex:middle a logic:Restriction ; logic:onProperty ex:part ;
               logic:someValuesFrom ex:inner .
           ex:inner a logic:Restriction ; logic:onProperty ex:part ;
               logic:minQualifiedCardinality 2 ; logic:onClass ex:Part ."#,
    );
    assert_eq!(refused.status, FofProjectionStatus::Blocked);
    assert!(refused.problem.is_none());
    assert_eq!(
        refused
            .blockers
            .iter()
            .filter(|blocker| { blocker.code == "UNADMITTED_CLASS_DEPENDENCY" })
            .count(),
        2
    );
}

#[test]
fn class_restrictions_do_not_erase_context_datatypes_or_procedural_semantics() {
    for extra in [
        "logic:someValuesFrom ex:Part ; logic:standpoint ex:observer",
        "logic:someValuesFrom xsd:integer",
        "logic:hasValue \"1\"^^xsd:integer",
        "logic:someValuesFrom ex:Part ; <http://www.w3.org/2002/07/owl#allValuesFrom> ex:Other",
    ] {
        let source = format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> . \
            @prefix ex: <https://example.org/> . \
            ex:r a logic:Restriction ; logic:onProperty ex:p ; {extra} ."
        );
        let (_, projection) = project(&source);
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{extra}: {projection:?}"
        );
        assert!(projection.problem.is_none());
    }
    for property in ["termEqual", "argument"] {
        let (_, projection) = project(&format!(
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:r a logic:Restriction ; logic:onProperty logic:{property} ;
               logic:hasValue ex:value ."#,
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Blocked,
            "{property}: {projection:?}"
        );
        assert!(projection.problem.is_none());
    }
}

#[test]
fn top_and_bottom_class_membership_have_their_defined_meaning() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:universal a logic:Thing . ex:impossible a logic:Nothing .
           ex:All logic:sameAs logic:Thing . ex:Empty logic:sameAs logic:Nothing .
           ex:throughAlias a ex:All, ex:Empty ."#,
    );
    assert_eq!(projection.status, FofProjectionStatus::Complete);
    let profile: Vec<_> = projection
        .sentences
        .iter()
        .filter(|sentence| sentence.source_units.is_empty() && !sentence.body.contains("<=>"))
        .collect();
    assert_eq!(
        profile.len(),
        4,
        "top/bottom meanings belong to the semantic profile"
    );
    for namespace in [
        "https://blackcatinformatics.ca/logic/",
        "http://www.w3.org/2002/07/owl#",
    ] {
        for (class, empty) in [("Thing", false), ("Nothing", true)] {
            let law = profile
                .iter()
                .find(|law| law.body.contains(&format!("I|{namespace}{class}")))
                .expect("each primitive class has a quantified profile law");
            assert!(law.body.contains("(! ["), "{law:?}");
            assert_eq!(law.body.contains("(~ "), empty, "{law:?}");
        }
    }
    let problem = projection.problem.unwrap();
    assert!(
        problem.contains(
            "'I|https://example.org/All' = 'I|https://blackcatinformatics.ca/logic/Thing'"
        ),
        "{problem}"
    );
    assert!(
        problem.contains(
            "'I|https://example.org/Empty' = 'I|https://blackcatinformatics.ca/logic/Nothing'"
        ),
        "{problem}"
    );
    assert!(
        problem.contains("I|https://example.org/throughAlias"),
        "{problem}"
    );
}

#[test]
fn class_dependencies_cannot_read_definitions_from_excluded_documents() {
    let theory = compile_documents(&[
        (
            "outer.ttl",
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix ex: <https://example.org/> .
            ex:outer a logic:Restriction ; logic:onProperty ex:p ; logic:someValuesFrom ex:inner ."#,
            None,
        ),
        (
            "inner.ttl",
            r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
            @prefix ex: <https://example.org/> .
            ex:inner a logic:Restriction ; logic:onProperty ex:q ; logic:hasValue ex:item ."#,
            None,
        ),
    ]);
    let whole =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    assert_eq!(
        project_tptp_fof(&theory, &whole).status,
        FofProjectionStatus::Complete
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.documents = DocumentSelection::Exact(BTreeSet::from(["outer.ttl".into()]));
    let selected = SourceAdmission::classical_fof(&theory, selection);
    let projection = project_tptp_fof(&theory, &selected);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(
        projection
            .blockers
            .iter()
            .any(|blocker| blocker.code == "UNADMITTED_CLASS_DEPENDENCY")
    );
}

#[test]
fn blank_schema_class_support_does_not_admit_literal_class_objects() {
    for predicate in ["domain", "range"] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
            <https://example.org/p> logic:{predicate} \"not a class resource\" ."
        ));
        assert_eq!(projection.status, FofProjectionStatus::Blocked);
        assert!(projection.problem.is_none());
        assert!(
            projection
                .blockers
                .iter()
                .any(|blocker| blocker.code == "NON_RESOURCE_SCHEMA_CLASS")
        );
    }
}

#[test]
fn class_constraint_ambiguity_is_malformed_but_identical_repetition_is_not() {
    for (properties, ambiguous) in [("ex:p, ex:q", true), ("ex:p, ex:p", false)] {
        let (admission, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
            @prefix ex: <https://example.org/> . \
            ex:r a logic:Restriction ; logic:onProperty {properties} ; \
                logic:someValuesFrom ex:Part ."
        ));
        if ambiguous {
            assert_eq!(
                admission.status,
                crate::frontend::SourceAdmissionStatus::Malformed
            );
            assert_eq!(projection.status, FofProjectionStatus::Blocked);
            assert!(projection.problem.is_none());
            assert!(
                projection
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code == "AMBIGUOUS_CLASS_CONSTRAINT")
            );
            assert!(projection.blockers.iter().any(|blocker| {
                blocker.code == "COMPILER_ERROR_IN_SELECTED_THEORY"
                    && blocker.reason.contains("AMBIGUOUS_RESTRICTION_PROPERTY")
            }));
        } else {
            assert_eq!(
                projection.status,
                FofProjectionStatus::Complete,
                "{projection:?}"
            );
        }
    }
}

#[test]
fn profile_class_laws_cannot_authorize_an_empty_source_selection() {
    let theory = compile(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <https://example.org/> .
        ex:f a logic:Formula ; logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:left ],
                           [ logic:termIndex 1 ; logic:termIri ex:right ] ."#,
    );
    let mut selection = SourceSelection::all_classical_roots(&theory);
    selection.formulas = crate::frontend::RootSelection::None;
    let admission = SourceAdmission::classical_fof(&theory, selection);
    assert_eq!(
        admission.status,
        crate::frontend::SourceAdmissionStatus::Empty
    );
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(projection.sentences.is_empty());
}

#[test]
fn property_object_equality_reaches_positive_and_negative_applications() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:p logic:sameAs ex:q . ex:subject ex:p ex:object .
           ex:negative a logic:Formula ; logic:not ex:atom .
           ex:atom a logic:Formula ; logic:relation ex:q ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:subject ],
                              [ logic:termIndex 1 ; logic:termIri ex:object ] ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    let problem = projection.problem.unwrap();
    assert!(
        problem.contains("'I|https://example.org/p' = 'I|https://example.org/q'"),
        "{problem}"
    );
    assert!(problem.contains("'A|2'('I|https://example.org/p','I|https://example.org/subject','I|https://example.org/object')"), "positive application must carry the property object: {problem}");
    assert!(problem.contains("(~ 'A|2'('I|https://example.org/q','I|https://example.org/subject','I|https://example.org/object'))"), "negative application must use the same first-order relation: {problem}");
}

#[test]
fn reified_applications_preserve_property_identity_at_admitted_source_arities() {
    for arity in 1..=3 {
        let arguments = (0..arity)
            .map(|index| format!("[ logic:termIndex {index} ; logic:termIri ex:value{index} ]"))
            .collect::<Vec<_>>()
            .join(",");
        let tail = if arity == 0 {
            String::new()
        } else {
            format!("; logic:argument {arguments}")
        };
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
            @prefix ex: <https://example.org/> . \
            ex:p logic:sameAs ex:q . \
            ex:positive a logic:Formula ; logic:relation ex:p {tail} . \
            ex:negative a logic:Formula ; logic:not ex:atom . \
            ex:atom a logic:Formula ; logic:relation ex:q {tail} ."
        ));
        assert_eq!(
            projection.status,
            FofProjectionStatus::Complete,
            "arity {arity}: {projection:?}"
        );
        let problem = projection.problem.unwrap();
        for property in ["p", "q"] {
            let suffix = if arity == 0 { ")" } else { "," };
            let application = format!("'A|{arity}'('I|https://example.org/{property}'{suffix}");
            assert!(problem.contains(&application), "{application}: {problem}");
        }
        assert!(
            !problem.contains("'P|"),
            "per-name predicates must not split object identity: {problem}"
        );
    }
}

#[test]
fn unary_application_and_membership_share_one_interpretation_without_a_declaration_switch() {
    for declaration in ["", "ex:p a logic:Class ."] {
        let (_, projection) = project(&format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> . \
            @prefix ex: <https://example.org/> . {declaration} \
            ex:f a logic:Formula ; logic:relation ex:p ; \
                logic:argument [ logic:termIndex 0 ; logic:termIri ex:value ] ."
        ));
        let problem = projection.problem.expect("unary source projects");
        assert!(
            problem.contains("'A|1'('I|https://example.org/p','I|https://example.org/value')"),
            "{problem}"
        );
        assert!(projection.sentences.iter().any(|sentence| {
            sentence.source_units.is_empty()
                && sentence.body.contains("'A|1'(V_profile_left,V_profile_right)")
                && sentence.body.contains("'A|2'('I|https://blackcatinformatics.ca/logic/instanceOf',V_profile_right,V_profile_left)")
                && sentence.body.contains("<=>")
        }), "unary predication must agree with reflected membership: {problem}");
    }
}

#[test]
fn native_rule_carriers_use_reflected_properties_in_both_head_and_body() {
    use crate::ir::{AtomicTerm, ContextualScope, LogicAxiom, LogicProgram, LogicRule};
    let atom = |property| {
        LogicAxiom::new(
            "?x",
            property,
            AtomicTerm::resource("?y"),
            false,
            ContextualScope::default(),
        )
        .unwrap()
    };
    let rule = LogicRule::new(
        atom("https://example.org/q"),
        vec![atom("https://example.org/p")],
        vec![],
        ContextualScope::default(),
    );
    let program = LogicProgram::new(vec![], vec![rule], vec![], None);
    let carrier = crate::projections::rdf::project_canonical_rdf12_dataset(&program).unwrap();
    let theory = PreparedLogicSource::new(&carrier.dataset)
        .unwrap()
        .into_compiled(None)
        .unwrap();
    assert_eq!(theory.program().rules.len(), 1);
    let admission =
        SourceAdmission::classical_fof(&theory, SourceSelection::all_classical_roots(&theory));
    let projection = project_tptp_fof(&theory, &admission);
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    assert!(
        projection.sentences.iter().any(|sentence| {
            !sentence.source_units.is_empty()
                && sentence.body.contains("=>")
                && sentence.body.contains("'A|2'('I|https://example.org/p',")
                && sentence.body.contains("'A|2'('I|https://example.org/q',")
        }),
        "the actual RDF 1.2 rule reader/writer path must preserve both role objects"
    );
}

#[test]
fn intrinsic_operator_aliases_have_profile_meanings_for_every_direct_schema_lowering() {
    let (_, projection) =
        project("<https://example.org/s> <https://example.org/p> <https://example.org/o> .");
    for (namespace, local) in [
        ("https://blackcatinformatics.ca/logic/", "sameAs"),
        ("https://blackcatinformatics.ca/logic/", "differentFrom"),
        ("https://blackcatinformatics.ca/logic/", "subClassOf"),
        ("https://blackcatinformatics.ca/logic/", "equivalentClass"),
        ("https://blackcatinformatics.ca/logic/", "disjointWith"),
        ("https://blackcatinformatics.ca/logic/", "subPropertyOf"),
        (
            "https://blackcatinformatics.ca/logic/",
            "equivalentProperty",
        ),
        ("https://blackcatinformatics.ca/logic/", "inverseOf"),
        ("https://blackcatinformatics.ca/logic/", "domain"),
        ("https://blackcatinformatics.ca/logic/", "range"),
        ("http://www.w3.org/1999/02/22-rdf-syntax-ns#", "type"),
        ("http://www.w3.org/2000/01/rdf-schema#", "subClassOf"),
        ("http://www.w3.org/2002/07/owl#", "sameAs"),
    ] {
        let application = format!("'A|2'('I|{namespace}{local}',V_profile_left,V_profile_right)");
        assert!(
            projection.sentences.iter().any(|sentence| {
                sentence.source_units.is_empty()
                    && sentence.body.contains(&application)
                    && sentence.body.contains("<=>")
            }),
            "missing reified interpretation for {namespace}{local}"
        );
    }
}

#[test]
fn restriction_roles_can_alias_intrinsic_schema_operators() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:alias logic:sameAs logic:subClassOf .
           ex:subtypes a logic:Restriction ; logic:onProperty ex:alias ; logic:hasValue ex:Target ."#,
    );
    assert_eq!(
        projection.status,
        FofProjectionStatus::Complete,
        "{projection:?}"
    );
    assert!(projection.sentences.iter().any(|sentence| {
        !sentence.source_units.is_empty()
            && sentence.body.contains("<=>")
            && sentence
                .body
                .contains("'A|2'('I|https://example.org/alias',")
            && sentence.body.contains("I|https://example.org/Target")
    }));
    let (_, direct) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:subtypes a logic:Restriction ; logic:onProperty logic:subClassOf ; logic:hasValue ex:Target ."#,
    );
    assert_eq!(direct.status, FofProjectionStatus::Complete, "{direct:?}");
    assert!(direct.sentences.iter().any(|sentence| {
        !sentence.source_units.is_empty()
            && sentence.body.contains("<=>")
            && sentence.body.contains("=>")
            && sentence.body.contains("I|https://example.org/Target")
    }));
}

#[test]
fn missing_required_atomic_arguments_remain_a_malformed_source_refusal() {
    let (_, projection) = project(
        r#"@prefix logic: <https://blackcatinformatics.ca/logic/> .
           @prefix ex: <https://example.org/> .
           ex:f a logic:Formula ; logic:relation ex:p ."#,
    );
    assert_eq!(projection.status, FofProjectionStatus::Blocked);
    assert!(projection.problem.is_none());
    assert!(projection.blockers.iter().any(|blocker| {
        blocker.code == "COMPILER_ERROR_IN_SELECTED_THEORY"
            && blocker
                .reason
                .contains("requires at least one logic:argument")
    }));
}

#[test]
fn source_identity_binds_the_complete_base_receipt() {
    let source =
        r#"@prefix ex: <https://example.org/vocab/> . ex:subject ex:predicate ex:object ."#;
    let compile_at = |base: &str| {
        compile_documents(&[(
            "same.ttl",
            source,
            Some(SourceBase {
                iri: base.to_owned(),
                origin: SourceBaseOrigin::Caller,
            }),
        )])
    };
    let left = compile_at("https://example.org/left/");
    let right = compile_at("https://example.org/right/");
    let left = SourceAdmission::classical_fof(&left, SourceSelection::all_classical_roots(&left));
    let right =
        SourceAdmission::classical_fof(&right, SourceSelection::all_classical_roots(&right));
    assert_ne!(
        left.source_digest, right.source_digest,
        "different admitted bases must never share a source identity"
    );
}
