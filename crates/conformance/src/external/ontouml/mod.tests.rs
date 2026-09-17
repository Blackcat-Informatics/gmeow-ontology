// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn set(items: &[&str]) -> std::collections::BTreeSet<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn agree_when_documented_fired() {
    let fired = set(&["FreeRole", "MixIden"]);
    assert_eq!(compare(Some("FreeRole"), &fired), DisciplineVerdict::Agree);
    assert_eq!(native_verdict_string(Some("FreeRole"), &fired), "FreeRole");
}

#[test]
fn corpus_only_when_documented_missed() {
    let fired = set(&["MixIden"]);
    assert_eq!(
        compare(Some("FreeRole"), &fired),
        DisciplineVerdict::CorpusOnly
    );
    assert_eq!(native_verdict_string(Some("FreeRole"), &fired), "MixIden");
}

#[test]
fn agree_when_clean_fires_nothing() {
    let fired = set(&[]);
    assert_eq!(compare(None, &fired), DisciplineVerdict::Agree);
    assert_eq!(native_verdict_string(None, &fired), "clean");
}

#[test]
fn engine_only_when_clean_fires_something() {
    let fired = set(&["RelComp"]);
    assert_eq!(compare(None, &fired), DisciplineVerdict::EngineOnly);
    assert_eq!(native_verdict_string(None, &fired), "RelComp");
}

#[test]
fn free_role_model_fires_free_role_end_to_end() {
    // A lone role class (no rigid ancestor) is the classic FreeRole anti-pattern.
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Wanderer a ontouml:Class ; ontouml:stereotype ontouml:role .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let (quads, _dataset) = evaluate_model(
        &model,
        "https://example.org/onto/schema",
        AntiRigidityPolicy::SchemaOnly,
    )
    .unwrap();
    let fired = fired_disciplines(&quads);
    assert!(fired.contains("FreeRole"), "fired={fired:?}");
    assert_eq!(compare(Some("FreeRole"), &fired), DisciplineVerdict::Agree);
}

#[test]
fn functional_relator_fires_relcomp_end_to_end() {
    // A concrete relator mediating a single functional relatum is the RelComp
    // anti-pattern (a relator must mediate at least two entities).
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Marriage a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Spouse a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Marriage ; ontouml:mediatedEnd ex:Spouse ;\n\
    ontouml:functionalMediation true .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let (quads, _dataset) = evaluate_model(
        &model,
        "https://example.org/onto/schema",
        AntiRigidityPolicy::SchemaOnly,
    )
    .unwrap();
    let fired = fired_disciplines(&quads);
    assert!(fired.contains("RelComp"), "fired={fired:?}");
}

#[test]
fn two_ended_relator_does_not_fire_relcomp() {
    // A relator mediating two distinct entities satisfies the discipline.
    let src = "\
@prefix ontouml: <https://w3id.org/ontouml#> .\n\
@prefix ex: <https://example.org/onto/> .\n\
ex:Employment a ontouml:Class ; ontouml:stereotype ontouml:relator .\n\
ex:Employee a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:Employer a ontouml:Class ; ontouml:stereotype ontouml:role .\n\
ex:med a ontouml:Relation ; ontouml:stereotype ontouml:mediation ;\n\
    ontouml:relatorEnd ex:Employment ; ontouml:mediatedEnd ex:Employee , ex:Employer .\n";
    let model = parse_ontouml_model(src, None).unwrap();
    let (quads, _dataset) = evaluate_model(
        &model,
        "https://example.org/onto/schema",
        AntiRigidityPolicy::SchemaOnly,
    )
    .unwrap();
    let fired = fired_disciplines(&quads);
    assert!(!fired.contains("RelComp"), "fired={fired:?}");
}
