// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const FREE_ROLE: &str = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Person a ontouml:Class ; ontouml:stereotype ontouml:kind .\n\
ex:Customer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Wanderer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:g1 a ontouml:Generalization ; ontouml:general ex:Person ; ontouml:specific ex:Customer .\n";

#[test]
fn parses_free_role_model() {
    let m = parse_ontouml_model(FREE_ROLE, None).unwrap();
    assert_eq!(m.classes.len(), 3);
    // Deterministic sort by IRI: Customer, Person, Wanderer.
    assert_eq!(m.classes[0].iri, "https://example.org/onto/Customer");
    assert_eq!(m.classes[0].stereotypes, vec!["role".to_string()]);
    assert_eq!(m.classes[1].stereotypes, vec!["kind".to_string()]);
    assert_eq!(m.generalizations.len(), 1);
    assert_eq!(
        m.generalizations[0].general,
        "https://example.org/onto/Person"
    );
    assert_eq!(
        m.generalizations[0].specific,
        "https://example.org/onto/Customer"
    );
    assert!(m.mediations.is_empty());
}

#[test]
fn parses_mediation_shape_b() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Marriage ; ontouml:mediatedEnd ex:Spouse ;\n\
    ontouml:functionalMediation true .\n";
    let m = parse_ontouml_model(src, None).unwrap();
    assert_eq!(m.mediations.len(), 1);
    let med = &m.mediations[0];
    assert_eq!(med.relator, "https://example.org/onto/Marriage");
    assert_eq!(
        med.mediated,
        vec!["https://example.org/onto/Spouse".to_string()]
    );
    assert!(med.functional);
}

#[test]
fn parses_mediation_shape_a() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Employment a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Employee a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Employer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:p1 , ex:p2 , ex:p3 .\n\
ex:p1 ontouml:propertyType ex:Employment .\n\
ex:p2 ontouml:propertyType ex:Employee .\n\
ex:p3 ontouml:propertyType ex:Employer .\n";
    let m = parse_ontouml_model(src, None).unwrap();
    assert_eq!(m.mediations.len(), 1);
    let med = &m.mediations[0];
    assert_eq!(med.relator, "https://example.org/onto/Employment");
    assert_eq!(
        med.mediated,
        vec![
            "https://example.org/onto/Employee".to_string(),
            "https://example.org/onto/Employer".to_string()
        ]
    );
    assert!(!med.functional);
}

#[test]
fn parses_mediation_shape_a_functional_via_cardinality() {
    // The real FAIR-catalog serialization: a mediation Relation with two
    // `ontouml:relationEnd` Property nodes, each carrying an `ontouml:cardinality`
    // → `ontouml:Cardinality` → `ontouml:upperBound`. The mediated (Spouse) end has
    // upper bound "1", so the mediation is functional (the RelComp shape) — WITHOUT
    // the self-authored `ontouml:functionalMediation` convenience flag.
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:pR , ex:pM .\n\
ex:pR a ontouml:Property ; ontouml:propertyType ex:Marriage ; ontouml:cardinality ex:cR .\n\
ex:cR a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"*\" .\n\
ex:pM a ontouml:Property ; ontouml:propertyType ex:Spouse ; ontouml:cardinality ex:cM .\n\
ex:cM a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"1\" .\n";
    let m = parse_ontouml_model(src, None).unwrap();
    assert_eq!(m.mediations.len(), 1);
    let med = &m.mediations[0];
    assert_eq!(med.relator, "https://example.org/onto/Marriage");
    assert_eq!(
        med.mediated,
        vec!["https://example.org/onto/Spouse".to_string()]
    );
    assert!(
        med.functional,
        "a single mediated end with ontouml:upperBound \"1\" is functional"
    );
}

#[test]
fn shape_a_unbounded_mediated_end_is_not_functional() {
    // A mediated end with upper bound "*" (unbounded) is NOT functional — the
    // relator can reach many relata, so RelComp must not fire.
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relationEnd ex:pR , ex:pM .\n\
ex:pR a ontouml:Property ; ontouml:propertyType ex:Marriage .\n\
ex:pM a ontouml:Property ; ontouml:propertyType ex:Spouse ; ontouml:cardinality ex:cM .\n\
ex:cM a ontouml:Cardinality ; ontouml:lowerBound \"1\" ; ontouml:upperBound \"*\" .\n";
    let m = parse_ontouml_model(src, None).unwrap();
    let med = &m.mediations[0];
    assert!(
        !med.functional,
        "an unbounded mediated end is not functional"
    );
}

#[test]
fn unsupported_stereotype_surfaces_at_lower_time_not_parse_time() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Water a ontouml:Class ; ontouml:stereotype ontouml:quantity .\n";
    // Parse records the raw stereotype without complaint.
    let m = parse_ontouml_model(src, None).unwrap();
    assert_eq!(m.classes[0].stereotypes, vec!["quantity".to_string()]);
    // The gap surfaces only when the stereotype is mapped.
    let err = logic_local_for_stereotype("quantity").unwrap_err();
    assert!(matches!(err, OntoumlError::Unsupported(_)), "{err}");
}

#[test]
fn mediation_without_relator_is_unsupported() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Left a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:mediatedEnd ex:Left .\n";
    let err = parse_ontouml_model(src, None).unwrap_err();
    assert!(matches!(err, OntoumlError::Unsupported(_)), "{err}");
}

#[test]
fn generalization_missing_end_is_syntax() {
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:g1 a ontouml:Generalization ; ontouml:general ex:Person .\n";
    let err = parse_ontouml_model(src, None).unwrap_err();
    assert!(matches!(err, OntoumlError::Syntax(_)), "{err}");
}
