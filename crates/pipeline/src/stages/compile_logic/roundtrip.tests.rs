// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::RdfDataset;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn corpus_logic_module_common_logic_roundtrips() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
        .expect("producer-authenticated Common Logic observation");
    let observation: Observation = from_slice(&bytes).expect("decode observation");
    assert_eq!(observation.source, super::super::SOURCE_PATH);
    let dialects = observation
        .dialects
        .expect("canonical reference was produced");
    for (expected, dialect) in ["clif", "cgif", "xcl"].into_iter().zip(dialects) {
        assert_eq!(dialect.dialect, expected);
        dialect
            .result
            .unwrap_or_else(|error| panic!("{expected}: {error}"));
    }
}

fn synthetic() -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(
        b"<urn:example:A> <https://blackcatinformatics.ca/logic/subClassOf> <urn:example:B> .",
        "application/n-triples",
        None,
    )
    .unwrap()
}

#[test]
fn native_reference_records_every_dialect() {
    let mut artifacts = BTreeMap::new();
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    record(&program, &mut artifacts).unwrap();
    let observation: Observation = from_slice(&artifacts[CHANNEL]).unwrap();
    let dialects = observation.dialects.unwrap();
    for (name, observation) in ["clif", "cgif", "xcl"].into_iter().zip(dialects) {
        assert_eq!(observation.dialect, name);
        observation.result.unwrap();
    }
}

#[test]
fn malformed_projection_is_recorded_without_aborting_the_other_observations() {
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    let mut projection = xcl::project_xcl(&program).unwrap();
    projection.content = "<broken>".to_owned();
    let result = dialect("xcl", &program, Ok(projection), |text| {
        xcl::parse_xcl_str(text, None)
    });
    assert!(result.result.is_err());
    assert!(observe(&program).unwrap().iter().all(|r| r.result.is_ok()));
}

#[test]
fn complete_ir_comparison_records_source_drift() {
    let (program, _) = parse_logic_dataset(&synthetic(), None).unwrap();
    let mut changed = program.clone();
    changed.source_iri = Some("urn:other-source".to_owned());
    assert!(
        exact(&program, &changed)
            .unwrap_err()
            .message()
            .contains("source_iri differs")
    );
}

const ROUNDTRIP_WIRE: &str = r#"{"source":"slices/grounding/logic/module.ttl","dialects":{"Ok":[{"dialect":"clif","result":{"Ok":null}},{"dialect":"cgif","result":{"Ok":null}},{"dialect":"xcl","result":{"Ok":null}}]}}"#;

#[test]
fn native_common_logic_wire_preserves_order_open_names_and_causal_outcomes() {
    let observation: Observation = from_slice(ROUNDTRIP_WIRE.as_bytes()).unwrap();
    assert_eq!(observation.source, super::super::SOURCE_PATH);
    assert_eq!(to_vec(&observation), ROUNDTRIP_WIRE.as_bytes());
    let open = ROUNDTRIP_WIRE.replace("clif", "future dialect 中");
    let observation: Observation = from_slice(open.as_bytes()).unwrap();
    assert_eq!(
        observation.dialects.as_ref().unwrap()[0].dialect,
        "future dialect 中"
    );
    assert_eq!(to_vec(&observation), open.as_bytes());
    let unknown = ROUNDTRIP_WIRE
        .replacen('{', "{\"future\":null,", 1)
        .replace("\"dialect\":", "\"future\":null,\"dialect\":");
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        ROUNDTRIP_WIRE.as_bytes()
    );

    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:premise"),
        StageId::new("roundtrip-input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored refusal"))
            .with_focus("urn:projection")
            .with_antecedents([premise]),
        StageId::new("roundtrip-observation"),
    );
    let outer = Observation {
        source: super::super::SOURCE_PATH.into(),
        dialects: Err(refusal.clone()),
    };
    let restored: Observation = from_slice(&to_vec(&outer)).unwrap();
    assert_eq!(restored.dialects.unwrap_err(), refusal);
    let mut observation: Observation = from_slice(ROUNDTRIP_WIRE.as_bytes()).unwrap();
    observation.dialects.as_mut().unwrap()[1].result = Err(refusal.clone());
    let restored: Observation = from_slice(&to_vec(&observation)).unwrap();
    let dialects = restored.dialects.unwrap();
    assert!(dialects[0].result.is_ok() && dialects[2].result.is_ok());
    assert_eq!(dialects[1].dialect, "cgif");
    assert_eq!(dialects[1].result.as_ref().unwrap_err(), &refusal);
    let retained = dialects[1].result.as_ref().unwrap_err();
    assert_eq!(retained.causes.len(), 1);
    assert_eq!(
        retained.root.antecedents.as_ref(),
        &[retained.causes[0].fingerprint]
    );
}

#[test]
fn native_common_logic_admission_refuses_record_width_and_unit_shape_drift() {
    let named = purrdf_lex::json::read(ROUNDTRIP_WIRE).unwrap();
    let positional = purrdf_lex::json::Value::from(
        named
            .as_object()
            .unwrap()
            .iter()
            .map(|(_, value)| value.clone())
            .collect::<Vec<_>>(),
    );
    assert!(from_slice::<Observation>(&to_vec(&positional)).is_err());
    for invalid in [
        ROUNDTRIP_WIRE.replace("\"source\":", "\"source\":\"duplicate\",\"source\":"),
        ROUNDTRIP_WIRE.replace("\"source\":\"slices/grounding/logic/module.ttl\",", ""),
        ROUNDTRIP_WIRE.replace("\"dialects\":", "\"dialects\":null,\"dialects\":"),
        ROUNDTRIP_WIRE.replace("\"dialect\":\"clif\",", ""),
        ROUNDTRIP_WIRE.replace(
            "{\"dialect\":\"clif\",\"result\":{\"Ok\":null}}",
            "[\"clif\",{\"Ok\":null}]",
        ),
        ROUNDTRIP_WIRE.replace("\"result\":", "\"result\":null,\"result\":"),
        ROUNDTRIP_WIRE.replacen("\"Ok\":null", "\"Ok\":null,\"Err\":null", 1),
        ROUNDTRIP_WIRE.replacen("\"Ok\":null", "\"Unknown\":null", 1),
        ROUNDTRIP_WIRE.replace(
            "\"dialects\":{\"Ok\":[",
            "\"dialects\":{\"Err\":null,\"Ok\":[",
        ),
        "{".into(),
    ] {
        assert!(
            from_slice::<Observation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    for payload in ["{}", "[]", "true", "\"unit\"", "0"] {
        let invalid = ROUNDTRIP_WIRE.replacen("\"Ok\":null", &format!("\"Ok\":{payload}"), 1);
        let error = from_slice::<Observation>(invalid.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(error.contains("/dialects/Ok/0/result/Ok"), "{error}");
    }
    let cell = r#"{"dialect":"future","result":{"Ok":null}}"#;
    for width in [0, 1, 2, 4] {
        let cells = std::iter::repeat_n(cell, width)
            .collect::<Vec<_>>()
            .join(",");
        let invalid = format!(
            r#"{{"source":"slices/grounding/logic/module.ttl","dialects":{{"Ok":[{cells}]}}}}"#
        );
        let error = from_slice::<Observation>(invalid.as_bytes())
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("/dialects/Ok") && error.contains("exactly three"),
            "{error}"
        );
    }
}
