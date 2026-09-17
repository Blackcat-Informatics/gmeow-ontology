// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

// -------------------------------------------------------------------------
// mf: vocabulary tests (updated from original)
// -------------------------------------------------------------------------

const MF_MANIFEST: &str = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:pos a mf:PositiveEntailment ;\n\
    mf:name \"clash-entails\" ;\n\
    mf:action ex:premise.nq ;\n\
    mf:result ex:conclusion.nq .\n\
ex:neg a mf:NegativeEntailment ;\n\
    mf:name \"no-entailment\" ;\n\
    mf:action ex:open.nq .\n";

#[test]
fn mf_extracts_positive_and_negative_entries() {
    let entries = parse_test_manifest(MF_MANIFEST, None).unwrap();
    assert_eq!(entries.len(), 2);

    let pos = entries.iter().find(|e| e.name == "clash-entails").unwrap();
    assert_eq!(pos.kind, ManifestTestKind::PositiveEntailment);
    assert_eq!(pos.outcome(), ExternalOutcome::Inconsistent);
    assert!(
        matches!(&pos.action, Some(OntologyDoc::Reference(iri)) if iri.ends_with("premise.nq"))
    );
    assert!(matches!(
        &pos.result,
        Some(OntologyDoc::Reference(iri)) if iri.ends_with("conclusion.nq")
    ));

    let neg = entries.iter().find(|e| e.name == "no-entailment").unwrap();
    assert_eq!(neg.kind, ManifestTestKind::NegativeEntailment);
    assert_eq!(neg.outcome(), ExternalOutcome::Consistent);
    assert!(neg.result.is_none());
}

#[test]
fn mf_ignores_non_test_entries() {
    let src = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:syntax a mf:PositiveSyntax ; mf:action ex:a.ttl .\n";
    assert!(parse_test_manifest(src, None).unwrap().is_empty());
}

#[test]
fn mf_entailment_entry_missing_action_hard_fails() {
    let src = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:pos a mf:PositiveEntailment ; mf:name \"x\" .\n";
    let err = parse_test_manifest(src, None).unwrap_err();
    assert!(err.message().contains("no premise document"), "{err}");
}

#[test]
fn malformed_turtle_hard_fails() {
    assert!(parse_test_manifest("@prefix bad <", None).is_err());
}

// -------------------------------------------------------------------------
// otest: vocabulary — kind → outcome mapping
// -------------------------------------------------------------------------

fn make_otest_entry(type_suffix: &str, extra_prop: &str) -> String {
    format!(
        "@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
             @prefix ex: <https://gmeow.example/ent/> .\n\
             ex:t a otest:{type_suffix} ;\n\
                 otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" .\n\
             {extra_prop}\n"
    )
}

#[test]
fn otest_consistency_maps_to_consistent() {
    let src = make_otest_entry("ConsistencyTest", "");
    let entries = parse_test_manifest(&src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, ManifestTestKind::Consistency);
    assert_eq!(entries[0].outcome(), ExternalOutcome::Consistent);
}

#[test]
fn single_typed_consistency_is_not_also_positive_entailment() {
    let entries = parse_test_manifest(&make_otest_entry("ConsistencyTest", ""), None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        !entries[0].also_positive_entailment,
        "a plain ConsistencyTest must not be flagged as a positive-entailment test"
    );
}

/// The W3C `rdfbased-sem-*` metamodeling cases declare BOTH `ConsistencyTest`
/// and `PositiveEntailmentTest`. The `also_positive_entailment` flag must be set
/// regardless of which type wins `kind`, so the grade lane can route their empty
/// consistency premise to the entailment lane instead of a vacuous DlGap.
#[test]
fn dual_typed_consistency_plus_positive_entailment_sets_flag() {
    let src = "@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
             @prefix ex: <https://gmeow.example/ent/> .\n\
             ex:t a otest:PositiveEntailmentTest, otest:ConsistencyTest ;\n\
                 otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" ;\n\
                 otest:rdfXmlConclusionOntology \"<rdf:RDF/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].also_positive_entailment,
        "a dual-typed ConsistencyTest + PositiveEntailmentTest must set the flag"
    );
}

#[test]
fn otest_inconsistency_maps_to_inconsistent() {
    let src = make_otest_entry("InconsistencyTest", "");
    let entries = parse_test_manifest(&src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, ManifestTestKind::Inconsistency);
    assert_eq!(entries[0].outcome(), ExternalOutcome::Inconsistent);
}

#[test]
fn otest_positive_entailment_test_maps_to_inconsistent() {
    let src = make_otest_entry("PositiveEntailmentTest", "");
    let entries = parse_test_manifest(&src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, ManifestTestKind::PositiveEntailment);
    assert_eq!(entries[0].outcome(), ExternalOutcome::Inconsistent);
}

#[test]
fn otest_negative_entailment_test_maps_to_consistent() {
    let src = make_otest_entry("NegativeEntailmentTest", "");
    let entries = parse_test_manifest(&src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, ManifestTestKind::NegativeEntailment);
    assert_eq!(entries[0].outcome(), ExternalOutcome::Consistent);
}

// -------------------------------------------------------------------------
// otest: inline RDF/XML premise
// -------------------------------------------------------------------------

#[test]
fn otest_inline_premise_yields_inline_rdf_xml_doc() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF xmlns:rdf=\\\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\\\"/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(&entries[0].action, Some(OntologyDoc::InlineRdfXml(content)) if content.contains("<rdf:RDF")),
        "expected Some(InlineRdfXml) with RDF/XML content, got {:?}",
        entries[0].action
    );
}

#[test]
fn otest_inline_conclusion_yields_inline_rdf_xml_doc() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:PositiveEntailmentTest ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" ;\n\
    otest:rdfXmlConclusionOntology \"<rdf:RDF xmlns:rdf=\\\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\\\"/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        matches!(&entries[0].result, Some(OntologyDoc::InlineRdfXml(c)) if c.contains("<rdf:RDF")),
        "expected Some(InlineRdfXml) for conclusion, got {:?}",
        entries[0].result
    );
}

// -------------------------------------------------------------------------
// Name extraction precedence
// -------------------------------------------------------------------------

#[test]
fn name_from_rdfs_label_when_no_mf_name() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    rdfs:label \"my-rdfs-label\" ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "my-rdfs-label");
}

#[test]
fn name_from_otest_identifier_when_no_mf_name_or_rdfs_label() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    otest:identifier \"my-otest-id\" ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "my-otest-id");
}

#[test]
fn mf_name_takes_precedence_over_rdfs_label_and_otest_identifier() {
    let src = "\
@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    mf:name \"mf-wins\" ;\n\
    rdfs:label \"rdfs-label\" ;\n\
    otest:identifier \"otest-id\" ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "mf-wins");
}

#[test]
fn name_falls_back_to_subject_iri_when_all_absent() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    otest:rdfXmlPremiseOntology \"<rdf:RDF/>\" .\n";
    let entries = parse_test_manifest(src, None).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(
        entries[0].name.contains("gmeow.example/ent/t"),
        "expected subject IRI as fallback name, got {:?}",
        entries[0].name
    );
}

// -------------------------------------------------------------------------
// Hard-fail cases
// -------------------------------------------------------------------------

#[test]
fn empty_inline_rdf_xml_premise_hard_fails() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    otest:rdfXmlPremiseOntology \"   \" .\n";
    let err = parse_test_manifest(src, None).unwrap_err();
    assert!(
        err.message().contains("empty otest:rdfXmlPremiseOntology"),
        "{err}"
    );
}

#[test]
fn otest_entry_with_no_premise_hard_fails() {
    let src = "\
@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n\
@prefix ex: <https://gmeow.example/ent/> .\n\
ex:t a otest:ConsistencyTest ;\n\
    otest:identifier \"no-premise\" .\n";
    let err = parse_test_manifest(src, None).unwrap_err();
    assert!(err.message().contains("no premise document"), "{err}");
}

// -------------------------------------------------------------------------
// parse_test_manifest_rdfxml — RDF/XML input path
// -------------------------------------------------------------------------

#[test]
fn rdfxml_manifest_extracts_consistency_test_with_inline_premise() {
    // A minimal RDF/XML manifest carrying one otest:ConsistencyTest with an inline
    // otest:rdfXmlPremiseOntology literal. Verifies that parse_test_manifest_rdfxml
    // extracts the entry and returns it with the correct kind and action.
    let src = r#"<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF
    xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
    xmlns:otest="http://www.w3.org/2007/OWL/testOntology#"
    xmlns:ex="https://gmeow.example/rdfxml-test/">
  <otest:ConsistencyTest rdf:about="https://gmeow.example/rdfxml-test/con1">
    <otest:identifier>con1</otest:identifier>
    <otest:rdfXmlPremiseOntology rdf:datatype="http://www.w3.org/2001/XMLSchema#string">&lt;rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/&gt;</otest:rdfXmlPremiseOntology>
  </otest:ConsistencyTest>
</rdf:RDF>"#;
    let entries =
        parse_test_manifest_rdfxml(src, None).expect("RDF/XML manifest must parse without error");
    assert_eq!(entries.len(), 1, "expected exactly one entry");
    let e = &entries[0];
    assert_eq!(e.name, "con1", "otest:identifier should be used as name");
    assert_eq!(
        e.kind,
        ManifestTestKind::Consistency,
        "otest:ConsistencyTest must map to Consistency kind"
    );
    assert!(
        matches!(&e.action, Some(OntologyDoc::InlineRdfXml(c)) if c.contains("rdf:RDF")),
        "action must be Some(InlineRdfXml) containing the premise, got {:?}",
        e.action
    );
    assert_eq!(
        e.outcome(),
        ExternalOutcome::Consistent,
        "ConsistencyTest outcome must be Consistent"
    );
}
