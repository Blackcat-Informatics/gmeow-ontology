// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::super::model::parse_ontouml_model;
use super::*;

const WORLD: &str = "https://example.org/onto/schema";

fn assert_fact(dataset: &RdfDataset, subject: &str, predicate: &str, object: &str) {
    let expected = RdfQuad::new(RdfTerm::iri(subject), predicate, RdfTerm::iri(object))
        .in_graph(RdfTerm::iri(WORLD));
    assert!(
        dataset.owned_quads().any(|quad| quad == expected),
        "missing GMEOW lowering fact: {expected:?}"
    );
}

#[test]
fn lowers_free_role_facts() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Person a ontouml:Class ; ontouml:stereotype ontouml:kind .\n\
ex:Customer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:g1 a ontouml:Generalization ; ontouml:general ex:Person ; ontouml:specific ex:Customer .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let dataset = lower_model_dataset(&model, WORLD).unwrap();
    assert_fact(
        &dataset,
        "https://example.org/onto/Person",
        RDF_TYPE,
        "https://blackcatinformatics.ca/logic/Kind",
    );
    assert_fact(
        &dataset,
        "https://example.org/onto/Customer",
        RDF_TYPE,
        "https://blackcatinformatics.ca/logic/Role",
    );
    assert_fact(
        &dataset,
        "https://example.org/onto/Customer",
        "https://blackcatinformatics.ca/logic/subClassOf",
        "https://example.org/onto/Person",
    );
}

#[test]
fn lowers_functional_mediation_role_and_type_pun() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Marriage ; ontouml:mediatedEnd ex:Spouse ;\n\
    ontouml:functionalMediation true .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let dataset = lower_model_dataset(&model, WORLD).unwrap();
    assert_fact(
        &dataset,
        "https://example.org/onto/Marriage",
        RDF_TYPE,
        "https://blackcatinformatics.ca/logic/Relator",
    );
    assert_fact(
        &dataset,
        "https://example.org/onto/Marriage",
        "https://blackcatinformatics.ca/logic/mediates",
        "https://example.org/onto/med#end0",
    );
    assert_fact(
        &dataset,
        "https://example.org/onto/med#end0",
        RDF_TYPE,
        OWL_FUNCTIONAL_PROPERTY,
    );
}

#[test]
fn unsupported_stereotype_is_a_lower_time_gap() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Water a ontouml:Class ; ontouml:stereotype ontouml:quantity .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let err = lower_model_dataset(&model, WORLD).unwrap_err();
    assert!(matches!(err, OntoumlError::Unsupported(_)), "{err}");
}
