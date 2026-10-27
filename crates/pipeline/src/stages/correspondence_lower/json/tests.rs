// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::projections::emotionml::WorkedEnvelope;
use purrdf_lex::json::{
    record::{ToJson, from_slice},
    write_compact,
};

const WIRE: &str = r#"{"observation_present":false,"envelope":{"intensity":"raw\nintensity","dimensions":[["z","-0.000"],["a","1e400"],["z","text"]]}}"#;

#[test]
fn native_worked_envelope_preserves_complete_wire_and_raw_ordered_rows() {
    let expected = WorkedEnvelopeObservation {
        observation_present: false,
        envelope: WorkedEnvelope {
            intensity: "raw\nintensity".to_owned(),
            dimensions: vec![
                ("z".to_owned(), "-0.000".to_owned()),
                ("a".to_owned(), "1e400".to_owned()),
                ("z".to_owned(), "text".to_owned()),
            ],
        },
    };
    let bytes = write_compact(&expected.to_json()).into_bytes();
    assert_eq!(
        bytes,
        WIRE.as_bytes(),
        "compact declaration-order bytes have no terminal newline"
    );
    let decoded: WorkedEnvelopeObservation =
        from_slice(&bytes).expect("complete native observation");
    assert!(
        !decoded.observation_present,
        "false is retained observation data"
    );
    assert_eq!(decoded.envelope.intensity, expected.envelope.intensity);
    assert_eq!(
        decoded.envelope.dimensions, expected.envelope.dimensions,
        "duplicate dimensions and row order remain untouched"
    );
    assert_eq!(write_compact(&decoded.to_json()), WIRE);

    let with_unknowns = WIRE
        .replacen(
            "{\"observation_present\":",
            "{\"future\":[],\"observation_present\":",
            1,
        )
        .replace("\"envelope\":{", "\"envelope\":{\"future\":false,");
    let decoded: WorkedEnvelopeObservation = from_slice(with_unknowns.as_bytes())
        .expect("unknown fields are tolerated at both named record levels");
    assert_eq!(write_compact(&decoded.to_json()), WIRE);
}

#[test]
fn native_worked_envelope_refuses_ambiguous_records_and_malformed_dimension_rows() {
    let malformed = [
        r#"[false,{"intensity":"raw","dimensions":[]}]"#.to_owned(),
        r#"{"observation_present":false,"envelope":["raw",[["z","text"]]]}"#.to_owned(),
        WIRE.replace(
            "\"observation_present\":false",
            "\"observation_present\":true,\"observation_present\":false",
        ),
        WIRE.replace("\"envelope\":", "\"envelope\":null,\"envelope\":"),
        WIRE.replace("\"intensity\":", "\"intensity\":\"old\",\"intensity\":"),
        WIRE.replace("\"observation_present\":false,", ""),
        r#"{"observation_present":false}"#.to_owned(),
        WIRE.replace("\"intensity\":\"raw\\nintensity\",", ""),
        r#"{"observation_present":false,"envelope":{"intensity":"raw"}}"#.to_owned(),
        WIRE.replace(
            "\"observation_present\":false",
            "\"observation_present\":\"false\"",
        ),
        WIRE.replace("\"intensity\":\"raw\\nintensity\"", "\"intensity\":0.5"),
        r#"{"observation_present":false,"envelope":{"intensity":"raw","dimensions":null}}"#
            .to_owned(),
        WIRE.replace("[\"z\",\"-0.000\"]", "null"),
        WIRE.replace("[\"z\",\"-0.000\"]", "[\"z\"]"),
        WIRE.replace("[\"z\",\"-0.000\"]", "[\"z\",\"-0.000\",\"extra\"]"),
        WIRE.replace("[\"z\",\"-0.000\"]", "[true,\"-0.000\"]"),
        WIRE.replace("[\"z\",\"-0.000\"]", "[\"z\",0]"),
    ];
    for input in malformed {
        assert_ne!(input, WIRE, "refusal input must change the complete wire");
        assert!(
            from_slice::<WorkedEnvelopeObservation>(input.as_bytes()).is_err(),
            "must refuse {input}"
        );
    }
    let short_row = WIRE.replace("[\"z\",\"-0.000\"]", "[\"z\"]");
    let error = from_slice::<WorkedEnvelopeObservation>(short_row.as_bytes())
        .err()
        .expect("short row refusal");
    assert_eq!(error.pointer(), "/envelope/dimensions/0");
    let numeric_cell = WIRE.replace("[\"z\",\"-0.000\"]", "[\"z\",0]");
    let error = from_slice::<WorkedEnvelopeObservation>(numeric_cell.as_bytes())
        .err()
        .expect("numeric cell refusal");
    assert_eq!(error.pointer(), "/envelope/dimensions/0/1");
    let missing_intensity = WIRE.replace("\"intensity\":\"raw\\nintensity\",", "");
    let error = from_slice::<WorkedEnvelopeObservation>(missing_intensity.as_bytes())
        .err()
        .expect("missing intensity refusal");
    assert_eq!(error.pointer(), "/envelope");
    assert_eq!(error.message(), "missing field `intensity`");
}
