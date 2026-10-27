// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::{
    record::{ToJson, from_slice},
    write_compact,
};
use std::collections::BTreeMap;

const WIRE: &str = r#"{"evaluations":[{"answers":[{"atom":"z(\"raw\")","bindings":{"A":"second","Z":"first"},"derivation_iri":"raw proof","proof_checks":false},{"atom":"a","bindings":{},"derivation_iri":"other proof","proof_checks":true}],"clauses":["z.","a."],"description":"line\ntext","goal":"?raw","iri":"not normalized","name":"display","status":"future status","verdict_probe_atoms":["z","a"],"verdicts":[{"atom":"z","verdict":"future verdict"},{"atom":"a","verdict":"undefined"}]}],"program_projections":{"a":"first","z":"second"},"projection":"projection\n","repeated_program_projections":{"other":"independent"},"repeated_projection":"different\n"}"#;

#[test]
fn native_goal_observation_preserves_complete_lexical_wire_and_raw_evidence() {
    let expected = GoalDirectedObservation {
        evaluations: vec![GoalDirectedEvaluation {
            iri: "not normalized".to_owned(),
            name: "display".to_owned(),
            description: "line\ntext".to_owned(),
            goal: "?raw".to_owned(),
            status: "future status".to_owned(),
            answers: vec![
                GoalDirectedAnswer {
                    atom: "z(\"raw\")".to_owned(),
                    bindings: BTreeMap::from([
                        ("Z".to_owned(), "first".to_owned()),
                        ("A".to_owned(), "second".to_owned()),
                    ]),
                    derivation_iri: "raw proof".to_owned(),
                    proof_checks: false,
                },
                GoalDirectedAnswer {
                    atom: "a".to_owned(),
                    bindings: BTreeMap::new(),
                    derivation_iri: "other proof".to_owned(),
                    proof_checks: true,
                },
            ],
            verdicts: vec![
                GoalDirectedVerdict {
                    atom: "z".to_owned(),
                    verdict: "future verdict".to_owned(),
                },
                GoalDirectedVerdict {
                    atom: "a".to_owned(),
                    verdict: "undefined".to_owned(),
                },
            ],
            clauses: vec!["z.".to_owned(), "a.".to_owned()],
            verdict_probe_atoms: vec!["z".to_owned(), "a".to_owned()],
        }],
        program_projections: BTreeMap::from([
            ("z".to_owned(), "second".to_owned()),
            ("a".to_owned(), "first".to_owned()),
        ]),
        projection: "projection\n".to_owned(),
        repeated_program_projections: BTreeMap::from([(
            "other".to_owned(),
            "independent".to_owned(),
        )]),
        repeated_projection: "different\n".to_owned(),
    };
    let bytes = write_compact(&expected.to_json()).into_bytes();
    assert_eq!(
        bytes,
        WIRE.as_bytes(),
        "all nested fields retain lexical order without a terminal newline"
    );
    let decoded: GoalDirectedObservation = from_slice(&bytes).expect("complete native observation");
    assert_eq!(
        decoded, expected,
        "transport must not normalize or validate observed evidence"
    );

    let with_unknowns = WIRE
        .replacen("{\"evaluations\":", "{\"future\":null,\"evaluations\":", 1)
        .replacen("{\"answers\":", "{\"future\":[],\"answers\":", 1)
        .replacen("{\"atom\":", "{\"future\":{},\"atom\":", 1)
        .replace(
            "{\"atom\":\"z\",\"verdict\":",
            "{\"future\":false,\"atom\":\"z\",\"verdict\":",
        );
    assert_eq!(
        from_slice::<GoalDirectedObservation>(with_unknowns.as_bytes())
            .expect("unknown fields at every named level"),
        expected
    );
    let overwritten = WIRE
        .replace("\"A\":\"second\"", "\"A\":\"old\",\"A\":\"second\"")
        .replace("\"a\":\"first\"", "\"a\":\"old\",\"a\":\"first\"")
        .replace(
            "\"other\":\"independent\"",
            "\"other\":\"old\",\"other\":\"independent\"",
        );
    assert_eq!(
        from_slice::<GoalDirectedObservation>(overwritten.as_bytes())
            .expect("last valid typed map values win"),
        expected
    );
}

#[test]
fn native_goal_observation_refuses_ambiguous_records_and_shadowed_invalid_maps() {
    let malformed = [
        "[]".to_owned(),
        WIRE.replacen(
            "{\"evaluations\":",
            "{\"evaluations\":[],\"evaluations\":",
            1,
        ),
        WIRE.replacen("{\"answers\":", "{\"answers\":[],\"answers\":", 1),
        WIRE.replacen("{\"atom\":", "{\"atom\":\"duplicate\",\"atom\":", 1),
        WIRE.replace(
            "\"verdict\":\"future verdict\"",
            "\"verdict\":\"old\",\"verdict\":\"future verdict\"",
        ),
        WIRE.replace("\"proof_checks\":false", "\"proof_checks\":\"false\""),
        WIRE.replace(",\"proof_checks\":false", ""),
        WIRE.replace(
            "\"bindings\":{\"A\":\"second\",\"Z\":\"first\"}",
            "\"bindings\":null",
        ),
        WIRE.replace("\"clauses\":[\"z.\",\"a.\"]", "\"clauses\":[\"z.\",false]"),
        WIRE.replace("\"verdicts\":[", "\"verdicts\":null,\"ignored\":["),
        WIRE.replace("\"status\":\"future status\",", ""),
        WIRE.replace("\"projection\":\"projection\\n\"", "\"projection\":null"),
        WIRE.replace(
            "\"repeated_projection\":\"different\\n\"",
            "\"repeated_projection\":[]",
        ),
        WIRE.replace("\"A\":\"second\"", "\"A\":false,\"A\":\"second\""),
        WIRE.replace("\"A\":\"second\"", "\"A\":\"second\",\"A\":false"),
        WIRE.replace("\"a\":\"first\"", "\"a\":null,\"a\":\"first\""),
        WIRE.replace(
            "\"other\":\"independent\"",
            "\"other\":{},\"other\":\"independent\"",
        ),
    ];
    for input in malformed {
        assert_ne!(input, WIRE, "refusal input must change the complete wire");
        assert!(
            from_slice::<GoalDirectedObservation>(input.as_bytes()).is_err(),
            "must refuse {input}"
        );
    }
    // Populated positional structs are refused at every nested record boundary.
    assert!(from_slice::<GoalDirectedAnswer>(br#"["atom",{},"proof",false]"#).is_err());
    assert!(from_slice::<GoalDirectedVerdict>(br#"["atom","future"]"#).is_err());
    assert!(
        from_slice::<GoalDirectedEvaluation>(
            br#"["iri","name","description","goal","status",[],[],[],[]]"#
        )
        .is_err()
    );
    assert!(
        from_slice::<GoalDirectedObservation>(br#"[[],"projection","repeated",{},{}]"#).is_err()
    );
}
