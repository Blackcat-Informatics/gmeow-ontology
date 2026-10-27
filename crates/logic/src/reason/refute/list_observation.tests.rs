// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::{BlankScope, RdfTextDirection, TermValue};

const WIRE: &str = r#"{"Ok":{"schema_version":1,"source_catalog_id":"authored catalog","source_modules":{"a/module.ttl":"digest a","z/module.ttl":"digest z"},"destination_graph":"urn:observed-graph","admission_contract":"authored admission","selected_world_count":2,"list_diagnostics":[{"severity":"Warning","code":"LIST_INPUT","message":"retained 中","subject":null}],"blank_list_fields":["blank row"],"invalid_constructor_objects":["invalid row"],"list_sizes":{"urn:empty":0,"urn:head":2},"list_errors":["retained traversal refusal"],"admission_defects":["MalformedNil",{"ConflictingListField":{"subject":"urn:cell","predicate":"urn:first"}},{"ExpressionMultiplicity":{"subject":"urn:class","predicate":"urn:union"}},{"UnsupportedExpressionOwner":{"owner":{"Triple":{"s":{"Blank":{"label":"same","scope":17}},"p":{"Iri":"urn:p"},"o":{"Triple":{"s":{"Blank":{"label":"same","scope":19}},"p":{"Iri":"urn:q"},"o":{"Literal":{"lexical_form":"raw 01","datatype":"urn:raw-datatype","language":"MiXeD","direction":"Rtl"}}}}}},"predicate":"urn:definition"}},{"UnsupportedExpressionOperand":{"owner":"urn:owner","predicate":"urn:operand","operand":{"Literal":{"lexical_form":"01","datatype":"urn:datatype","language":"MiXeD","direction":"Rtl"}}}},{"UnsupportedListMember":{"owner":"urn:owner","head":"urn:head"}},{"CyclicList":{"owner":"urn:cycle","head":"urn:cycle-head"}},{"IncompleteList":{"owner":"urn:incomplete","head":"urn:missing-head"}}]}}"#;

#[test]
fn native_list_observation_wire_retains_all_source_issues_and_quoted_identity() {
    let outcome = decode_list_corpus_observation(WIRE.as_bytes()).unwrap();
    assert_eq!(encode_list_corpus_observation(&outcome), WIRE.as_bytes());
    let observation = outcome.unwrap();
    assert_eq!(observation.admission_defects.len(), 8);
    assert_eq!(observation.list_sizes["urn:empty"], 0);
    assert_eq!(observation.list_diagnostics[0].message, "retained 中");
    let RefutationSourceIssue::UnsupportedExpressionOwner { owner, predicate } =
        &observation.admission_defects[3]
    else {
        panic!("retained owner issue")
    };
    assert_eq!(predicate, "urn:definition");
    let TermValue::Triple { s, p, o } = owner else {
        panic!("retained outer statement")
    };
    assert_eq!(
        s.as_ref(),
        &TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(17)
        }
    );
    assert_eq!(p.as_ref(), &TermValue::iri("urn:p"));
    let TermValue::Triple { s, p, o } = o.as_ref() else {
        panic!("retained nested statement")
    };
    assert_eq!(
        s.as_ref(),
        &TermValue::Blank {
            label: "same".into(),
            scope: BlankScope(19)
        }
    );
    assert_eq!(p.as_ref(), &TermValue::iri("urn:q"));
    assert_eq!(
        o.as_ref(),
        &TermValue::Literal {
            lexical_form: "raw 01".into(),
            datatype: "urn:raw-datatype".into(),
            language: Some("MiXeD".into()),
            direction: Some(RdfTextDirection::Rtl),
        }
    );
    let RefutationSourceIssue::UnsupportedExpressionOperand { operand, .. } =
        &observation.admission_defects[4]
    else {
        panic!("retained operand issue")
    };
    assert_eq!(
        operand,
        &TermValue::Literal {
            lexical_form: "01".into(),
            datatype: "urn:datatype".into(),
            language: Some("MiXeD".into()),
            direction: Some(RdfTextDirection::Rtl),
        }
    );
    let unit_object = WIRE.replace("[\"MalformedNil\",", "[{\"MalformedNil\":null},");
    assert_eq!(
        encode_list_corpus_observation(
            &decode_list_corpus_observation(unit_object.as_bytes()).unwrap()
        ),
        WIRE.as_bytes()
    );
    let max_schema = WIRE.replace("\"schema_version\":1", "\"schema_version\":4294967295");
    let outcome = decode_list_corpus_observation(max_schema.as_bytes()).unwrap();
    assert_eq!(outcome.as_ref().unwrap().schema_version, u32::MAX);
    assert_eq!(
        encode_list_corpus_observation(&outcome),
        max_schema.as_bytes()
    );
}

#[test]
fn native_list_observation_refusal_retains_the_complete_causal_closure() {
    let mut ledger = gmeow_errors::DiagLedger::new();
    let premise = ledger.attach(
        gmeow_errors::Diag::from(std::io::Error::other("authored catalog refusal"))
            .with_focus("urn:list:catalog"),
        gmeow_errors::StageId::new("list-input"),
    );
    let refusal = ledger.record(
        gmeow_errors::Diag::from(std::io::Error::other("authored list refusal"))
            .with_focus("urn:list:projection")
            .with_antecedents([premise]),
        gmeow_errors::StageId::new("list-observation"),
    );
    let restored =
        decode_list_corpus_observation(&encode_list_corpus_observation(&Err(refusal.clone())))
            .unwrap();
    assert_eq!(restored.unwrap_err(), refusal);
    assert_eq!(refusal.causes.len(), 1);
    assert_eq!(
        refusal.root.antecedents.as_ref(),
        &[refusal.causes[0].fingerprint]
    );
}

#[test]
fn native_list_observation_admission_refuses_ambiguous_and_shadowed_invalid_evidence() {
    let unknown = WIRE
        .replace("\"schema_version\":", "\"future\":null,\"schema_version\":")
        .replace("\"subject\":", "\"future\":null,\"subject\":")
        .replace("\"owner\":", "\"future\":null,\"owner\":")
        .replace("\"predicate\":", "\"future\":null,\"predicate\":");
    assert_eq!(
        encode_list_corpus_observation(
            &decode_list_corpus_observation(unknown.as_bytes()).unwrap()
        ),
        WIRE.as_bytes()
    );
    let valid_shadow = WIRE
        .replace(
            "\"a/module.ttl\":",
            "\"a/module.ttl\":\"earlier\",\"a/module.ttl\":",
        )
        .replace("\"urn:head\":2", "\"urn:head\":3,\"urn:head\":2");
    assert_eq!(
        encode_list_corpus_observation(
            &decode_list_corpus_observation(valid_shadow.as_bytes()).unwrap()
        ),
        WIRE.as_bytes()
    );
    for invalid in [
        WIRE.replacen("{\"Ok\":", "{\"Err\":null,\"Ok\":", 1),
        WIRE.replacen("{\"Ok\":", "{\"Ok\":null,\"Ok\":", 1),
        WIRE.replace("\"schema_version\":1", "\"schema_version\":4294967296"),
        WIRE.replace("\"selected_world_count\":2", "\"selected_world_count\":-1"),
        WIRE.replace("\"selected_world_count\":2", "\"selected_world_count\":2e0"),
        WIRE.replace(
            "\"selected_world_count\":2",
            "\"selected_world_count\":18446744073709551616",
        ),
        WIRE.replace("\"source_catalog_id\":\"authored catalog\",", ""),
        WIRE.replace(
            "\"schema_version\":",
            "\"schema_version\":1,\"schema_version\":",
        ),
        WIRE.replace(
            "\"a/module.ttl\":",
            "\"a/module.ttl\":false,\"a/module.ttl\":",
        ),
        WIRE.replace("\"urn:head\":2", "\"urn:head\":1e0,\"urn:head\":2"),
        WIRE.replace(
            "\"urn:head\":2",
            "\"urn:head\":18446744073709551616,\"urn:head\":2",
        ),
        WIRE.replace(
            "\"subject\":\"urn:cell\"",
            "\"subject\":\"earlier\",\"subject\":\"urn:cell\"",
        ),
        WIRE.replace(
            "\"owner\":\"urn:owner\",\"predicate\":\"urn:operand\",",
            "\"owner\":\"urn:owner\",",
        ),
        WIRE.replace("\"scope\":17", "\"scope\":4294967296"),
        WIRE.replace("\"scope\":19", "\"scope\":19.0"),
        WIRE.replace("\"direction\":\"Rtl\"", "\"direction\":\"unknown\""),
        WIRE.replace("[\"MalformedNil\",", "[{\"MalformedNil\":false},"),
        WIRE.replace(
            "[\"MalformedNil\",",
            "[{\"MalformedNil\":null,\"CyclicList\":{}},",
        ),
        WIRE.replace("[\"MalformedNil\",", "[\"Unknown\","),
        WIRE.replace(
            "{\"ConflictingListField\":{\"subject\":\"urn:cell\",\"predicate\":\"urn:first\"}}",
            "{\"ConflictingListField\":[\"urn:cell\",\"urn:first\"]}",
        ),
        r#"{"Ok":[1,"catalog",{},"graph","contract",0,[],[],[],{},[],[]]}"#.to_owned(),
        r#"{"Err":{}}"#.to_owned(),
    ] {
        assert!(
            decode_list_corpus_observation(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
