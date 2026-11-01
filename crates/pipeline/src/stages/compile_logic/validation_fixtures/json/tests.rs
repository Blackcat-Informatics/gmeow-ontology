// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WIRE: &str = r#"{"modules":{"m":{"Ok":{"source_rdf":"retained source\n","diagnostics":[{"severity":"Warning","code":"authored","message":"retained 中","subject":null}],"shapes":[{"ir":{"iri":"raw shape","target":{"Class":"raw class"},"properties":[],"node_kind":"ValidationShape","standpoint":null,"node_components":[],"label":null,"failure_class":null},"shacl":"retained shape"}],"procedural_shacl":"retained procedure"}}},"inputs":{"empty":[],"raw":[0,127,128,255]},"cases":[{"module":"m","input":"raw","findings":{"Ok":[["focus z","shape z"],["focus a","shape a"]]}}],"diagnostics_migration":{"Ok":{"legacy_shape":"legacy","projected_shape":"projected","checked_focuses":["a","z"],"expected_violations":["a"],"legacy":[{"focus":"a","path":null,"value":"retained value","component":"raw component","severity":"raw severity","messages":["message z","message a"]}],"projected":[]}}}"#;

#[test]
fn validation_fixture_native_wire_preserves_complete_products_bytes_and_causal_outcomes() {
    let mut fixtures: ValidationFixtures = from_slice(WIRE.as_bytes()).unwrap();
    assert_eq!(to_vec(&fixtures), WIRE.as_bytes());
    assert_eq!(fixtures.inputs["raw"], [0, 127, 128, 255]);
    assert!(fixtures.inputs["empty"].is_empty());
    assert_eq!(
        fixtures.cases[0].findings.as_ref().unwrap(),
        &[
            ("focus z".to_owned(), "shape z".to_owned()),
            ("focus a".to_owned(), "shape a".to_owned())
        ]
    );
    let mut ledger = gmeow_errors::DiagLedger::new();
    let premise = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored validation input refusal"))
            .with_focus("urn:validation:input"),
        gmeow_errors::StageId::new("validation-input"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("authored validation product refusal"))
            .with_focus("urn:validation:product")
            .with_antecedents([premise]),
        gmeow_errors::StageId::new("validation-product"),
    );
    fixtures
        .modules
        .insert("refused".into(), Err(refusal.clone()));
    fixtures.cases[0].findings = Err(refusal.clone());
    fixtures.diagnostics_migration = Err(refusal.clone());
    let restored: ValidationFixtures = from_slice(&to_vec(&fixtures)).unwrap();
    assert_eq!(restored.modules["refused"].as_ref().unwrap_err(), &refusal);
    assert_eq!(restored.cases[0].findings.as_ref().unwrap_err(), &refusal);
    assert_eq!(
        restored.diagnostics_migration.as_ref().unwrap_err(),
        &refusal
    );
    assert_eq!(restored.inputs, fixtures.inputs);
    assert_eq!(
        restored.modules["m"].as_ref().unwrap().source_rdf,
        "retained source\n"
    );
    assert_eq!(refusal.causes.len(), 1);
    assert_eq!(
        refusal.root.antecedents.as_ref(),
        &[refusal.causes[0].fingerprint]
    );
}

#[test]
fn validation_fixture_native_admission_refuses_ambiguous_and_shadowed_invalid_records() {
    let unknown = WIRE
        .replace("\"modules\":", "\"future\":null,\"modules\":")
        .replace("\"source_rdf\":", "\"future\":null,\"source_rdf\":")
        .replace("\"ir\":", "\"future\":null,\"ir\":")
        .replace("\"module\":", "\"future\":null,\"module\":")
        .replace("\"legacy_shape\":", "\"future\":null,\"legacy_shape\":")
        .replace("\"focus\":", "\"future\":null,\"focus\":");
    assert_eq!(
        to_vec(&from_slice::<ValidationFixtures>(unknown.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let valid_shadow = WIRE
        .replace("\"inputs\":{", "\"inputs\":{\"raw\":[1],")
        .replace("[\"a\",\"z\"]", "[\"z\",\"a\",\"z\"]");
    assert_eq!(
        to_vec(&from_slice::<ValidationFixtures>(valid_shadow.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    let absent_path = WIRE.replace("\"path\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<ValidationFixtures>(absent_path.as_bytes()).unwrap()),
        WIRE.as_bytes()
    );
    for invalid in [
        WIRE.replace("\"modules\":{", "\"modules\":{\"m\":{\"Ok\":null},"),
        WIRE.replace("\"inputs\":{", "\"inputs\":{\"raw\":[256],"),
        WIRE.replace("[0,127,128,255]", "[-0,127,128,255]"),
        WIRE.replace("[0,127,128,255]", "[0e0,127,128,255]"),
        WIRE.replace(
            "\"source_rdf\":",
            "\"source_rdf\":\"earlier\",\"source_rdf\":",
        ),
        WIRE.replace(
            "\"procedural_shacl\":\"retained procedure\"",
            "\"procedural_shacl\":null",
        ),
        WIRE.replace("\"shacl\":\"retained shape\"", "\"shacl\":null"),
        WIRE.replace("\"module\":\"m\",", ""),
        WIRE.replace("\"findings\":", "\"findings\":null,\"findings\":"),
        WIRE.replace(
            "[\"focus z\",\"shape z\"]",
            "[\"focus z\",\"shape z\",\"extra\"]",
        ),
        WIRE.replace("[\"focus z\",\"shape z\"]", "[\"focus z\",null]"),
        WIRE.replace(
            "\"legacy_shape\":",
            "\"legacy_shape\":\"earlier\",\"legacy_shape\":",
        ),
        WIRE.replace("\"messages\":", "\"messages\":[],\"messages\":"),
        WIRE.replace("\"value\":\"retained value\"", "\"value\":false"),
        WIRE.replace(
            "\"diagnostics_migration\":{\"Ok\":",
            "\"diagnostics_migration\":{\"Err\":null,\"Ok\":",
        ),
        r#"[{"m":null},{"raw":[0]},[{"module":"m","input":"raw","findings":{"Ok":[]}}],{"Ok":{}}]"#
            .to_owned(),
    ] {
        assert!(
            from_slice::<ValidationFixtures>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
