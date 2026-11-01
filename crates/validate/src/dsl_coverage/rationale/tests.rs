// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent tiny canonical vocabulary, residual guard and CQ data. Tests never
//! load or compile repository canonical modules, fixtures or a corpus.

use super::*;

const VOCABULARY: &str = r#"
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
g:CompetencyQuestion a logic:Class ; logic:subClassOf
 [ a logic:Restriction ; logic:onProperty g:cqRationale ; logic:allValuesFrom [ logic:unionOf (xsd:string rdf:langString rdf:dirLangString) ] ],
 [ a logic:Restriction ; logic:onProperty g:cqReasoning ; logic:allValuesFrom [ logic:oneOf (g:reasoningNone g:reasoningRdfs) ] ] .
g:cqRationale a logic:DatatypeProperty ; logic:range rdfs:Literal .
g:ExampleConformance a logic:Class ; logic:subClassOf
 [ a logic:Restriction ; logic:onProperty g:conformanceRationale ; logic:allValuesFrom [ logic:unionOf (xsd:string rdf:langString rdf:dirLangString) ] ] .
g:conformanceRationale a logic:DatatypeProperty ; logic:range rdfs:Literal .
g:StructuralAssertion a logic:Class ; logic:subClassOf
 [ a logic:Restriction ; logic:onProperty g:saRationale ; logic:allValuesFrom [ logic:unionOf (xsd:string rdf:langString rdf:dirLangString) ] ] .
g:saRationale a logic:DatatypeProperty ; logic:range rdfs:Literal .
g:CompetencyQuestionRationaleRequiredConstraint a logic:DisjunctiveRequirednessConstraint ;
 logic:onClass g:CompetencyQuestion ; logic:anyOf g:cqRationale ;
 logic:formalizes g:CompetencyQuestion ; logic:message "Rationale required" .
g:OtherRequired a logic:DisjunctiveRequirednessConstraint ;
 logic:onClass g:CompetencyQuestion ; logic:anyOf g:cqQuery ;
 logic:formalizes g:CompetencyQuestion ; logic:message "Query required" .
"#;

const RESIDUAL: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix g: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:authored:legacy> a sh:NodeShape ; sh:targetClass g:CompetencyQuestion ;
 sh:property [ sh:path g:cqReasoning ; sh:in (g:reasoningNone g:reasoningRdfs) ] ;
 sh:property [ sh:path g:cqExpectAsk ; sh:maxCount 1 ; sh:datatype xsd:boolean ] ;
 sh:sparql [ sh:message "Query required" ; sh:select "SELECT $this WHERE { FILTER NOT EXISTS { $this <https://blackcatinformatics.ca/gmeow/cqQuery> ?query . } }" ] .
"#;

fn data(rationale: &str, extra: &str) -> String {
    format!(
        r#"@prefix g: <https://blackcatinformatics.ca/gmeow/> .
<urn:authored:cq> a g:CompetencyQuestion ; g:cqQuery "ASK {{ }}" ; g:cqExpectAsk true {rationale} . {extra}"#
    )
}

fn results(shapes: &str, data: &str) -> purrdf::shapes::report::ValidationReport {
    let (_, shapes) = crate::store::parse_validation_shapes(shapes).unwrap();
    let dataset = purrdf::parse_dataset(data.as_bytes(), "text/turtle", None).unwrap();
    crate::store::shacl_validate_dataset(&dataset, &shapes)
}

#[test]
fn cq_rationale_canonical_projection_admits_all_prose_kinds_and_coexisting_values() {
    let shapes = compose(VOCABULARY.as_bytes(), RESIDUAL).unwrap();
    for value in [
        r#""plain""#,
        r#""typed"^^<http://www.w3.org/2001/XMLSchema#string>"#,
        r#""private"@x-gmeow-english"#,
        r#""public"@fr"#,
        r#""left"@en--ltr"#,
        r#""يمين"@ar--rtl"#,
        r#""same", "same"@en, "same"@fr, "same"@en--ltr, "same"@en--rtl"#,
    ] {
        let report = results(&shapes, &data(&format!("; g:cqRationale {value}"), ""));
        assert!(report.conforms, "{value}: {:?}", report.results);
    }
}

#[test]
fn cq_rationale_canonical_projection_refuses_numeric_iri_absence_and_mixed_bad_values() {
    let shapes = compose(VOCABULARY.as_bytes(), RESIDUAL).unwrap();
    for suffix in [
        "",
        "; g:cqRationale 7",
        "; g:cqRationale <urn:wrong>",
        "; g:cqRationale []",
        r#"; g:cqRationale "valid"@en, 7"#,
    ] {
        let report = results(&shapes, &data(suffix, ""));
        assert!(!report.conforms, "{suffix}");
        assert!(!report.results.is_empty(), "{suffix}");
    }
}

#[test]
fn cq_rationale_composition_preserves_untouched_legacy_failure_counts_without_duplicates() {
    let shapes = compose(VOCABULARY.as_bytes(), RESIDUAL).unwrap();
    for input in [
        data(
            r#"; g:cqRationale "valid" ; g:cqReasoning g:unsupported"#,
            "",
        ),
        data(r#"; g:cqRationale "valid" ; g:cqExpectAsk false"#, ""),
        data(r#"; g:cqRationale "valid""#, "").replace(r#"g:cqQuery "ASK { }" ; "#, ""),
    ] {
        let before = results(RESIDUAL, &input);
        let after = results(&shapes, &input);
        assert!(!before.conforms);
        assert_eq!(after.conforms, before.conforms);
        assert_eq!(after.results.len(), before.results.len(), "{input}");
    }
}

#[test]
fn cq_rationale_selected_canonical_inputs_are_required_and_fail_closed() {
    for vocabulary in [
        VOCABULARY.replace("logic:onProperty g:cqRationale", "logic:onProperty g:other"),
        VOCABULARY.replace("g:CompetencyQuestionRationaleRequiredConstraint a", "g:WrongRequired a"),
        VOCABULARY.replace("g:CompetencyQuestionRationaleRequiredConstraint a logic:DisjunctiveRequirednessConstraint", "g:CompetencyQuestionRationaleRequiredConstraint a logic:Constraint"),
        VOCABULARY.replace("logic:anyOf g:cqRationale", "logic:anyOf 7"),
        VOCABULARY.replace("logic:anyOf g:cqRationale", "logic:anyOf g:other"),
        VOCABULARY.replace("logic:anyOf g:cqRationale", "logic:anyOf g:cqRationale, g:other"),
        VOCABULARY.replace("logic:onClass g:CompetencyQuestion ; logic:anyOf g:cqRationale", "logic:onClass g:Other ; logic:anyOf g:cqRationale"),
        "not valid Turtle {{".to_owned(),
    ] {
        assert!(compose(vocabulary.as_bytes(), RESIDUAL).is_err());
    }
}

fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for directory in ["dsl/mappings", "dsl/statements", "dsl/tests", "shapes"] {
        std::fs::create_dir_all(root.path().join(directory)).unwrap();
    }
    for label in ["mapping", "statement"] {
        let directory = if label == "mapping" {
            "mappings"
        } else {
            "statements"
        };
        std::fs::write(
            root.path().join(format!("dsl/{directory}/vocabulary.ttl")),
            "<urn:s> <urn:p> <urn:o> .",
        )
        .unwrap();
        std::fs::write(
            root.path().join(format!("shapes/{label}-dsl-shapes.ttl")),
            "# independent residual\n",
        )
        .unwrap();
    }
    std::fs::write(root.path().join("dsl/tests/vocabulary.ttl"), VOCABULARY).unwrap();
    std::fs::write(root.path().join("shapes/test-dsl-shapes.ttl"), RESIDUAL).unwrap();
    root
}

#[test]
fn cq_rationale_live_dsl_resolver_and_validator_use_the_required_canonical_projection() {
    let root = root();
    let selected = super::super::authored_dsl_shacl_inputs(root.path()).unwrap();
    let file = root.path().join("authored-cq.ttl");
    for (suffix, expected) in [
        (r#"; g:cqRationale "justification"@fr--ltr"#, true),
        ("; g:cqRationale 7", false),
        ("", false),
    ] {
        std::fs::write(&file, data(suffix, "")).unwrap();
        let findings = crate::dsl_shacl::validate_dsl(
            std::slice::from_ref(&file),
            &selected.test_shapes,
            "test",
        )
        .unwrap();
        assert_eq!(findings.is_empty(), expected, "{suffix}");
    }
    std::fs::remove_file(root.path().join("dsl/tests/vocabulary.ttl")).unwrap();
    assert!(super::super::authored_dsl_shacl_inputs(root.path()).is_err());
}
