// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored transport controls; neither test enters the CL producer or cache.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const OBSERVATION: &str = r#"{"diagnostics":[{"severity":"Warning","code":"authored","message":"retained 中","subject":null}],"plain":{"Ok":[{"graph":"urn:world","subject":{"Iri":"urn:subject"},"predicate":"urn:edge","object":{"Blank":{"label":"object","scope":3}},"graph_component":"urn:world","derivation_id":"urn:derivation","rule_iri":"urn:rule","source_quad_ids":["urn:premise"],"profile":"urn:profile","budget_status":"Partial"}]},"annotated":{"Ok":{"certification":{"query_class":"StratifiedAggregate","preservation":{"polarities":["Exact"],"unsupported_constructs":[]},"declared_deviations":["AddAssociative"],"lineage_contract":"CompleteGroupSupport"},"quads":[{"quad":{"graph":"urn:world","subject":{"Iri":"urn:subject"},"predicate":"urn:edge","object":{"Blank":{"label":"object","scope":3}},"graph_component":"urn:world","derivation_id":"urn:derivation","rule_iri":"urn:rule","source_quad_ids":["urn:premise"],"profile":"urn:profile","budget_status":"Partial"},"annotation":-7,"derivations":[{"rule_iri":"urn:rule","sources":[{"graph":"urn:world","subject":"<urn:subject>","predicate":"urn:edge","object":"_:object"}],"tuple_sources":[],"provider_sources":[],"annotation":-7}]}]}}}"#;

#[test]
fn native_cl_observation_wire_preserves_diagnostics_and_refuses_ambiguous_outcomes() {
    let wire = format!("{{\"Ok\":{OBSERVATION}}}");
    let outcome: ObservationOutcome = from_slice(wire.as_bytes()).unwrap();
    assert_eq!(to_vec(&outcome), wire.as_bytes());
    let observation = outcome.0.unwrap();
    assert_eq!(observation.diagnostics[0].message, "retained 中");
    assert_eq!(observation.plain.unwrap()[0].graph, "urn:world");
    let annotated = observation.annotated.unwrap();
    assert_eq!(annotated.quads[0].annotation, -7);
    assert_eq!(
        annotated.quads[0].derivations[0].sources[0].object,
        "_:object"
    );
    assert_eq!(
        annotated.certification.lineage_contract,
        gmeow_logic::annotation::AnnotationLineageContract::CompleteGroupSupport
    );
    let unknown = OBSERVATION
        .replacen('{', "{\"future\":null,", 1)
        .replace(
            "\"annotated\":{\"Ok\":{",
            "\"annotated\":{\"Ok\":{\"future\":null,",
        )
        .replace("\"certification\":{", "\"certification\":{\"future\":null,");
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        OBSERVATION.as_bytes()
    );
    // Keep every otherwise valid field value in the old positional struct form.
    let named = purrdf_lex::json::read(OBSERVATION).unwrap();
    let positional = Value::from(
        named
            .as_object()
            .unwrap()
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
    );
    let positional: Value = Object::new().with("Ok", positional).into();
    let positional = String::from_utf8(to_vec(&positional)).unwrap();
    for invalid in [
        format!("{{\"Ok\":{OBSERVATION},\"Err\":null}}"),
        format!("{{\"Ok\":{OBSERVATION},\"Ok\":{OBSERVATION}}}"),
        format!(
            "{{\"Ok\":{}}}",
            OBSERVATION.replace("\"plain\":{\"Ok\":[", "\"plain\":{\"Err\":null,\"Ok\":[")
        ),
        format!(
            "{{\"Ok\":{}}}",
            OBSERVATION.replace("\"quads\":[", "\"quads\":[],\"quads\":[")
        ),
        format!(
            "{{\"Ok\":{}}}",
            OBSERVATION.replace("\"declared_deviations\":[\"AddAssociative\"],", "")
        ),
        positional,
        r#"{"Err":null}"#.into(),
    ] {
        assert!(
            from_slice::<ObservationOutcome>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn native_cl_outer_and_execution_refusals_retain_complete_causal_evidence() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:premise"),
        StageId::new("authored-input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored refusal"))
            .with_focus("urn:rejection")
            .with_antecedents([premise]),
        StageId::new("authored-observation"),
    );
    let outer = ObservationOutcome(Err(refusal.clone()));
    let decoded: ObservationOutcome = from_slice(&to_vec(&outer)).unwrap();
    assert_eq!(decoded.0.unwrap_err(), refusal);
    let observed = ObservationOutcome(Ok(Observation {
        diagnostics: Vec::new(),
        plain: Err(refusal.clone()),
        annotated: Err(refusal.clone()),
    }));
    let decoded: ObservationOutcome = from_slice(&to_vec(&observed)).unwrap();
    let decoded = decoded.0.unwrap();
    assert_eq!(decoded.plain.unwrap_err(), refusal);
    let retained = decoded.annotated.unwrap_err();
    assert_eq!(retained, refusal);
    assert_eq!(retained.causes.len(), 1);
    assert_eq!(
        retained.root.antecedents.as_ref(),
        &[retained.causes[0].fingerprint]
    );
}
