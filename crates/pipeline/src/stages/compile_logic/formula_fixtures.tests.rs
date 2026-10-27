// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::PreservationKind;

#[test]
fn selection_records_absence_without_substituting_another_formula() {
    let formula = Formula::atom(
        Term::Iri("urn:selected".to_owned()),
        vec![
            Term::Iri("urn:A".to_owned()),
            Term::Iri("urn:B".to_owned()),
            Term::Iri("urn:C".to_owned()),
        ],
    )
    .unwrap();
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_formulas(vec![formula]);
    let observation = observe(program, Vec::new(), Some("urn:missing"));
    assert_eq!(observation.formulas.len(), 1);
    assert_eq!(observation.selected_formulas, 0);
    assert!(observation.lowering.unwrap().existential_rules.is_empty());
}

#[test]
fn selected_ternary_formula_records_actual_shared_native_witness() {
    let formula = Formula::atom(
        Term::Iri("urn:selected".to_owned()),
        vec![
            Term::Iri("urn:A".to_owned()),
            Term::Iri("urn:B".to_owned()),
            Term::Iri("urn:C".to_owned()),
        ],
    )
    .unwrap();
    let program =
        LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), None).with_formulas(vec![formula]);
    let observation = observe(program, Vec::new(), Some("urn:selected"));
    let observations = BTreeMap::from([("authored".to_owned(), Ok(observation))]);
    let mut restored = json::from_bytes(
        &purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
    )
    .unwrap();
    let restored = restored.remove("authored").unwrap().unwrap();
    assert_eq!(restored.selected_formulas, 1);
    let lowered = restored.lowering.unwrap();
    assert!(lowered.rules.is_empty());
    assert_eq!(lowered.existential_rules.len(), 1);
    let head = &lowered.existential_rules[0].head;
    assert_eq!(head.len(), 4);
    assert!(head.iter().all(|atom| atom.subject == head[0].subject));
    assert!(
        lowered
            .preservation
            .polarities
            .contains(&PreservationKind::Exact)
    );
}

const FORMULA_WIRE: &str = r#"{"authored":{"Ok":{"formulas":[{"Not":{"Atom":{"relation":{"Iri":"urn:predicate"},"args":[{"Var":"x"}]}}}],"diagnostics":[{"severity":"Warning","code":"inspection","message":"retained 中","subject":"urn:formula"}],"selected_formulas":1,"lowering":{"Ok":{"rules":[{"numeric":[{"operator":"Add","left":{"Var":"?left"},"right":{"Literal":["01","http://www.w3.org/2001/XMLSchema#integer",null,null]},"result":{"Var":"?sum"}},{"operator":"Equal","left":{"Var":"?sum"},"right":{"Var":"?left"},"result":null}],"rule_iri":"urn:rule","head":[{"subject":{"Blank":"urn:explicit-blank"},"predicate":"urn:head","object":{"Literal":["directional","http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString","ar","rtl"]},"negated":false}],"body":[{"subject":{"Var":"urn:explicit-variable"},"predicate":"urn:body","object":{"Iri":"?explicit-iri"},"negated":true}],"distinct_pairs":[["?sum","?left"],["?left","?sum"]]}],"existential_rules":[{"numeric":[],"rule_iri":"urn:existential","head":[{"subject":{"Var":"?witness"},"predicate":"urn:existential-head","object":{"Iri":"urn:object"},"negated":false}],"body":[],"distinct_pairs":[]}],"preservation":{"polarities":["Exact"],"unsupported_constructs":[]}}}}}}"#;

#[test]
fn native_formula_wire_retains_complete_inspection_and_raw_numeric_metadata() {
    use gmeow_logic_compile::relational_core::{NumericOperator, RcNumeric, RcTerm};
    let mut observations = json::from_bytes(FORMULA_WIRE.as_bytes()).unwrap();
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
        FORMULA_WIRE.as_bytes()
    );
    let observation = observations.get_mut("authored").unwrap().as_mut().unwrap();
    let lowering = observation.lowering.as_mut().unwrap();
    assert_eq!(
        lowering.rules[0].head[0].subject,
        RcTerm::Blank("urn:explicit-blank".into())
    );
    assert_eq!(
        lowering.rules[0].body[0].subject,
        RcTerm::Var("urn:explicit-variable".into())
    );
    assert_eq!(
        lowering.rules[0].body[0].object,
        RcTerm::Iri("?explicit-iri".into())
    );
    let operators = [
        (NumericOperator::Add, "Add"),
        (NumericOperator::Subtract, "Subtract"),
        (NumericOperator::Multiply, "Multiply"),
        (NumericOperator::Divide, "Divide"),
        (NumericOperator::ExactDivide, "ExactDivide"),
        (NumericOperator::Equal, "Equal"),
        (NumericOperator::NotEqual, "NotEqual"),
        (NumericOperator::Less, "Less"),
        (NumericOperator::LessOrEqual, "LessOrEqual"),
        (NumericOperator::Greater, "Greater"),
        (NumericOperator::GreaterOrEqual, "GreaterOrEqual"),
    ];
    // Inspection transport retains data even when it is not an executable call.
    lowering.rules[0].numeric = operators
        .iter()
        .map(|(operator, _)| RcNumeric {
            operator: *operator,
            left: RcTerm::Iri("?raw-left".into()),
            right: RcTerm::Blank("urn:raw-right".into()),
            result: Some(RcTerm::Var("urn:raw-result".into())),
        })
        .collect();
    let bytes =
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes();
    let text = std::str::from_utf8(&bytes).unwrap();
    for (_, name) in operators {
        assert!(text.contains(&format!("\"operator\":\"{name}\"")));
    }
    let restored = json::from_bytes(&bytes).unwrap();
    let actual = restored["authored"]
        .as_ref()
        .unwrap()
        .lowering
        .as_ref()
        .unwrap();
    let expected = observations["authored"]
        .as_ref()
        .unwrap()
        .lowering
        .as_ref()
        .unwrap();
    assert_eq!(actual.rules[0].numeric, expected.rules[0].numeric);
    assert_eq!(actual.rules[0].head, expected.rules[0].head);
    assert_eq!(actual.rules[0].body, expected.rules[0].body);
    assert_eq!(
        actual.rules[0].distinct_pairs,
        expected.rules[0].distinct_pairs
    );
    assert_eq!(
        actual.existential_rules[0].head,
        expected.existential_rules[0].head
    );
    let omitted = FORMULA_WIRE.replace(",\"result\":null", "");
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(omitted.as_bytes()).unwrap()
        ))
        .into_bytes(),
        FORMULA_WIRE.as_bytes()
    );
    let object_enum =
        FORMULA_WIRE.replace("\"operator\":\"Equal\"", "\"operator\":{\"Equal\":null}");
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(object_enum.as_bytes()).unwrap()
        ))
        .into_bytes(),
        FORMULA_WIRE.as_bytes()
    );
}

#[test]
fn native_formula_observations_retain_both_causal_refusal_levels() {
    let mut observations = json::from_bytes(FORMULA_WIRE.as_bytes()).unwrap();
    let mut ledger = gmeow_errors::DiagLedger::new();
    let premise = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored formula premise"))
            .with_focus("urn:formula:premise"),
        gmeow_errors::StageId::new("formula-source"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("authored lowering refusal"))
            .with_focus("urn:formula:lowering")
            .with_antecedents([premise]),
        gmeow_errors::StageId::new("formula-observation"),
    );
    observations
        .get_mut("authored")
        .unwrap()
        .as_mut()
        .unwrap()
        .lowering = Err(refusal.clone());
    observations.insert("refused-source".into(), Err(refusal.clone()));
    let restored = json::from_bytes(
        &purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(&observations))
            .into_bytes(),
    )
    .unwrap();
    assert_eq!(restored["refused-source"].as_ref().unwrap_err(), &refusal);
    let observation = restored["authored"].as_ref().unwrap();
    assert_eq!(observation.lowering.as_ref().unwrap_err(), &refusal);
    assert_eq!(
        observation.formulas,
        observations["authored"].as_ref().unwrap().formulas
    );
    assert_eq!(
        observation.diagnostics,
        observations["authored"].as_ref().unwrap().diagnostics
    );
    assert_eq!(observation.selected_formulas, 1);
    assert_eq!(refusal.causes.len(), 1);
    assert_eq!(
        refusal.root.antecedents.as_ref(),
        &[refusal.causes[0].fingerprint]
    );
}

#[test]
fn native_formula_admission_refuses_ambiguous_records_and_shadowed_invalid_sources() {
    let unknown = FORMULA_WIRE
        .replace("\"formulas\":", "\"future\":null,\"formulas\":")
        .replace("\"rules\":", "\"future\":null,\"rules\":")
        .replace("\"numeric\":", "\"future\":null,\"numeric\":")
        .replace("\"operator\":", "\"future\":null,\"operator\":")
        .replace("\"subject\":", "\"future\":null,\"subject\":");
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(unknown.as_bytes()).unwrap()
        ))
        .into_bytes(),
        FORMULA_WIRE.as_bytes()
    );
    let valid_shadow = FORMULA_WIRE.replacen('{', "{\"authored\":{\"Ok\":{\"formulas\":[],\"diagnostics\":[],\"selected_formulas\":0,\"lowering\":{\"Ok\":{\"rules\":[],\"existential_rules\":[],\"preservation\":{\"polarities\":[\"Exact\"],\"unsupported_constructs\":[]}}}}},", 1);
    assert_eq!(
        purrdf_lex::json::write_compact(&gmeow_errors::json::recorded_map_to_json(
            &json::from_bytes(valid_shadow.as_bytes()).unwrap()
        ))
        .into_bytes(),
        FORMULA_WIRE.as_bytes()
    );
    for invalid in [
        FORMULA_WIRE.replacen('{', "{\"authored\":{\"Ok\":null},", 1),
        FORMULA_WIRE.replace("\"formulas\":", "\"formulas\":[],\"formulas\":"),
        FORMULA_WIRE.replace("\"selected_formulas\":1,", ""),
        FORMULA_WIRE.replace("\"selected_formulas\":1", "\"selected_formulas\":1e0"),
        FORMULA_WIRE.replace("\"selected_formulas\":1", "\"selected_formulas\":-1"),
        FORMULA_WIRE.replace("\"selected_formulas\":1", "\"selected_formulas\":18446744073709551616"),
        FORMULA_WIRE.replace("\"Ok\":{\"rules\":", "\"Ok\":null,\"Ok\":{\"rules\":"),
        FORMULA_WIRE.replace("\"rules\":", "\"rules\":[],\"rules\":"),
        FORMULA_WIRE.replace("\"numeric\":", "\"numeric\":[],\"numeric\":"),
        FORMULA_WIRE.replace("\"rule_iri\":\"urn:rule\",", ""),
        FORMULA_WIRE.replace("\"operator\":\"Add\",", ""),
        FORMULA_WIRE.replace("\"result\":null", "\"result\":null,\"result\":null"),
        FORMULA_WIRE.replace("\"operator\":\"Add\"", "\"operator\":\"Unknown\""),
        FORMULA_WIRE.replace("\"Equal\"", "{\"Equal\":false}"),
        FORMULA_WIRE.replace("\"subject\":", "\"subject\":null,\"subject\":"),
        FORMULA_WIRE.replace("\"negated\":true", "\"negated\":null"),
        FORMULA_WIRE.replace("\"Blank\":\"urn:explicit-blank\"", "\"Blank\":null"),
        FORMULA_WIRE.replace("\"Iri\":\"?explicit-iri\"", "\"Iri\":\"?explicit-iri\",\"Blank\":\"other\""),
        FORMULA_WIRE.replace("[\"?sum\",\"?left\"]", "[\"?sum\",\"?left\",\"extra\"]"),
        FORMULA_WIRE.replace("[\"?sum\",\"?left\"]", "[\"?sum\",null]"),
        FORMULA_WIRE.replace("\"ar\",\"rtl\"", "\"ar\",\"rtl\",null"),
        FORMULA_WIRE.replace("\"ar\",\"rtl\"", "null,\"rtl\""),
        r#"{"authored":{"Ok":[[],[],0,{"Ok":{}}]}}"#.to_owned(),
        r#"{"authored":{"Ok":{"formulas":[],"diagnostics":[],"selected_formulas":0,"lowering":{"Ok":[[],[],{"polarities":["Exact"],"unsupported_constructs":[]}]}}}}"#.to_owned(),
    ] {
        assert!(json::from_bytes(invalid.as_bytes()).is_err(), "{invalid}");
    }
}
