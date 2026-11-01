// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

const LEGACY: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
<urn:Legacy> a sh:NodeShape ; sh:targetClass <urn:C> ; sh:sparql [
    sh:message "required value" ;
    sh:select "SELECT $this WHERE { FILTER NOT EXISTS { $this <urn:p> <urn:required> } }"
] .
"#;

const DATA: &str = r#"
@prefix ex: <https://blackcatinformatics.ca/gmeow/testing/diagnostic-migration/> .
<urn:missing> a <urn:C> ; ex:checked true ; ex:expected true .
<urn:present> a <urn:C> ; <urn:p> <urn:required> ; ex:checked true .
<urn:unrelated> a <urn:Other> ; ex:checked true .
"#;

fn projected() -> String {
    let dataset = purrdf::parse_dataset(br#"
@prefix logic: <https://blackcatinformatics.ca/logic/> .
<https://example.test/NeedsValue> a logic:Constraint ; logic:formalizes <urn:C> ;
    logic:message "required value" ; logic:integrity <urn:all> .
<urn:all> a logic:Formula ;
    logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "this" ] ;
    logic:forall <urn:implication> .
<urn:implication> a logic:Formula ; logic:antecedent <urn:guard> ; logic:consequent <urn:requiredValue> .
<urn:guard> a logic:Formula ; logic:relation <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
        [ logic:termIndex 1 ; logic:termIri <urn:C> ] .
<urn:requiredValue> a logic:Formula ; logic:relation <urn:p> ;
    logic:argument [ logic:termIndex 0 ; logic:termVariable "this" ],
        [ logic:termIndex 1 ; logic:termIri <urn:required> ] .
"#, "text/turtle", None).unwrap();
    let source = gmeow_logic_compile::frontend::PreparedLogicSource::new(&dataset).unwrap();
    let product = super::super::compile_module(&source).unwrap();
    assert!(product.diagnostics.is_empty(), "{:?}", product.diagnostics);
    product.procedural_shacl
}

#[test]
fn migration_parity_compares_native_compiler_outputs_and_survives_record_roundtrip() {
    let record = compare(
        LEGACY,
        &projected(),
        &witnesses(DATA).unwrap(),
        "urn:Legacy",
        "https://example.test/NeedsValueProceduralConstraintShape",
    )
    .unwrap();
    let restored: Agreement =
        purrdf_lex::json::record::from_slice(&purrdf_lex::json::record::to_vec(&record)).unwrap();
    assert_eq!(restored.checked_focuses.len(), 3);
    assert_eq!(
        restored.expected_violations,
        BTreeSet::from(["<urn:missing>".to_owned()])
    );
    assert_eq!(restored.legacy, restored.projected);
    assert_eq!(restored.projected.len(), 1);
}

#[test]
fn migration_parity_rejects_missing_projection_and_changed_focus_severity_or_messages() {
    let projected = projected();
    let data = witnesses(DATA).unwrap();
    for wrong in [
        String::new(),
        projected.replace("<urn:C>", "<urn:Other>"),
        projected.replace("required value", "different message"),
        projected.replace("sh:Violation", "sh:Warning").replace(
            "<http://www.w3.org/ns/shacl#Violation>",
            "<http://www.w3.org/ns/shacl#Warning>",
        ),
    ] {
        assert_ne!(
            wrong, projected,
            "each mutation must change the projected surface"
        );
        assert!(
            compare(
                LEGACY,
                &wrong,
                &data,
                "urn:Legacy",
                "https://example.test/NeedsValueProceduralConstraintShape"
            )
            .is_err()
        );
    }
}

#[test]
fn migration_parity_rejects_wrong_source_and_wrong_or_vacuous_expectations() {
    let projected = projected();
    let mut data = witnesses(DATA).unwrap();
    assert!(
        compare(
            LEGACY,
            &projected,
            &data,
            "urn:Wrong",
            "https://example.test/NeedsValueProceduralConstraintShape"
        )
        .is_err()
    );
    data.expected = BTreeSet::from(["<urn:present>".to_owned()]);
    assert!(
        compare(
            LEGACY,
            &projected,
            &data,
            "urn:Legacy",
            "https://example.test/NeedsValueProceduralConstraintShape"
        )
        .is_err()
    );
    data.expected.clear();
    assert!(
        compare(
            LEGACY,
            &projected,
            &data,
            "urn:Legacy",
            "https://example.test/NeedsValueProceduralConstraintShape"
        )
        .is_err()
    );
}

#[test]
fn migration_parity_population_contains_all_guard_boundaries_and_original_blank_focuses() {
    let data = population().unwrap();
    assert_eq!(data.checked.len(), 329);
    assert_eq!(data.expected.len(), 6);
    assert_eq!(
        data.expected
            .iter()
            .filter(|focus| focus.starts_with("_:"))
            .count(),
        1
    );
    assert!(
        data.checked
            .iter()
            .any(|focus| focus.ends_with("disconnected>"))
    );
}
