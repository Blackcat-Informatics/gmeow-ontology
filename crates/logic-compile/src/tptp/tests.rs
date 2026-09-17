// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeSet;

use purrdf::{CompositeDatasetView, DatasetView, ViewLimits, parse_dataset};

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
                materialized.term_id_by_value(&value).ok_or_else(|| {
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
    let problem = projection.problem.expect("complete problem");
    assert!(
        problem.contains("I|https://example.org/alice")
            && problem.contains("I|https://example.org/A"),
        "ordinary rdf:type assertion must not disappear: {problem}"
    );
    assert!(
        problem.contains("<=>") == false && problem.contains("=>"),
        "subclass must become a membership implication: {problem}"
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
        problem.contains("P|2|https://example.org/principia/transitionSourceContentSpace"),
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
        problem.contains("P|1|https://example.org/holds"),
        "{problem}"
    );
    for syntax in ["termIndex", "termIri", "argument", "relation"] {
        assert!(
            !problem.contains(&format!(
                "P|2|https://blackcatinformatics.ca/logic/{syntax}"
            )) && !problem.contains(&format!(
                "P|1|https://blackcatinformatics.ca/logic/{syntax}"
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
