// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored maps/outcomes and causal ledgers only; no catalogs, selector or producer.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const OBSERVATION: &str = r#"{"parsed_catalogs":{"a.po":3,"z.po":0},"reviewed_coverage":{"a.po":["z|predicate","中\n\"\\","z|predicate"],"z.po":[]}}"#;
const REJECTION: &str = r#"{"root":{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"reviewed","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"reviewed.catalog.invalid","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}},"causes":[]}"#;

#[test]
fn json_reviewed_coverage_literal_observation_and_success_outcome_preserve_order_and_multiplicity()
{
    let observation = ReviewedCoverageObservation {
        parsed_catalogs: [("z.po".to_owned(), 0), ("a.po".to_owned(), 3)].into(),
        reviewed_coverage: [
            ("z.po".to_owned(), vec![]),
            (
                "a.po".to_owned(),
                vec![
                    "z|predicate".to_owned(),
                    "中\n\"\\".to_owned(),
                    "z|predicate".to_owned(),
                ],
            ),
        ]
        .into(),
    };
    assert_eq!(to_vec(&observation), OBSERVATION.as_bytes());
    let expected = format!("{{\"Ok\":{OBSERVATION}}}");
    assert_eq!(
        to_vec(&ReviewedCoverageOutcome(Ok(observation))),
        expected.as_bytes()
    );
    let restored: ReviewedCoverageOutcome = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let restored = restored.0.unwrap();
    assert_eq!(restored.reviewed_coverage["a.po"].len(), 3);
    assert_eq!(restored.parsed_catalogs["z.po"], 0);
    let empty = br#"{"Ok":{"parsed_catalogs":{},"reviewed_coverage":{}}}"#;
    assert_eq!(
        to_vec(&from_slice::<ReviewedCoverageOutcome>(empty).unwrap()),
        empty
    );
}

#[test]
fn json_reviewed_coverage_literal_error_outcome_retains_original_diagnostic_payload() {
    let expected = format!("{{\"Err\":{REJECTION}}}");
    let restored: ReviewedCoverageOutcome = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let error = restored.0.unwrap_err();
    assert_eq!(error.root.code, "reviewed.catalog.invalid");
    assert!(error.causes.is_empty());
}

#[test]
fn json_reviewed_coverage_recorded_causal_identity_survives_outcome_roundtrip_and_replay() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("catalog premise"))
            .with_focus("urn:reviewed:premise")
            .with_derived_from_quads(["urn:reviewed:quad".to_owned()]),
        StageId::new("premise"),
    );
    let rejection = ledger.record(
        Diag::from(std::io::Error::other("catalog refused"))
            .with_focus("urn:reviewed:catalog")
            .with_antecedents([premise]),
        StageId::new("reviewed"),
    );
    assert_eq!(rejection.causes.len(), 1);
    assert_ne!(rejection.root.fingerprint, rejection.causes[0].fingerprint);
    let expected = rejection.clone();
    let bytes = to_vec(&ReviewedCoverageOutcome(Err(rejection)));
    let restored: ReviewedCoverageOutcome = from_slice(&bytes).unwrap();
    assert_eq!(to_vec(&restored), bytes);
    let restored = restored.0.unwrap_err();
    assert_eq!(restored, expected);
    assert_eq!(
        restored.root.antecedents.as_ref(),
        &[restored.causes[0].fingerprint]
    );
    let mut replay = DiagLedger::new();
    replay.replay(restored.causes);
    replay.replay([*restored.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}

#[test]
fn json_reviewed_coverage_counts_require_unsigned_tokens_and_host_width_inside_native_maps() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "\"1\"",
        "false",
    ] {
        let bad = format!(
            "{{\"Ok\":{{\"parsed_catalogs\":{{\"a.po\":{token}}},\"reviewed_coverage\":{{}}}}}}"
        );
        assert_eq!(
            from_slice::<ReviewedCoverageOutcome>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/Ok/parsed_catalogs/a.po"
        );
    }
    let max = format!(
        "{{\"parsed_catalogs\":{{\"a.po\":{}}},\"reviewed_coverage\":{{}}}}",
        usize::MAX
    );
    assert_eq!(
        from_slice::<ReviewedCoverageObservation>(max.as_bytes())
            .unwrap()
            .parsed_catalogs["a.po"],
        usize::MAX
    );
    if usize::BITS < 64 {
        assert!(
            from_slice::<ReviewedCoverageObservation>(
                br#"{"parsed_catalogs":{"a.po":4294967296},"reviewed_coverage":{}}"#
            )
            .is_err()
        );
    }
}

#[test]
fn json_reviewed_coverage_typed_map_duplicates_admit_every_value_before_last_wins() {
    let counts = br#"{"parsed_catalogs":{"a.po":1,"a.po":2},"reviewed_coverage":{"a.po":["first"],"a.po":["last","last"]}}"#;
    let observation: ReviewedCoverageObservation = from_slice(counts).unwrap();
    assert_eq!(observation.parsed_catalogs["a.po"], 2);
    assert_eq!(observation.reviewed_coverage["a.po"], ["last", "last"]);
    let bad_count = br#"{"parsed_catalogs":{"a.po":-0,"a.po":2},"reviewed_coverage":{}}"#;
    assert_eq!(
        from_slice::<ReviewedCoverageObservation>(bad_count)
            .unwrap_err()
            .pointer(),
        "/parsed_catalogs/a.po"
    );
    let bad_vector = br#"{"parsed_catalogs":{},"reviewed_coverage":{"a.po":null,"a.po":[]}}"#;
    assert_eq!(
        from_slice::<ReviewedCoverageObservation>(bad_vector)
            .unwrap_err()
            .pointer(),
        "/reviewed_coverage/a.po"
    );
    let wrong_item = br#"{"Ok":{"parsed_catalogs":{},"reviewed_coverage":{"a.po":["term",null]}}}"#;
    assert_eq!(
        from_slice::<ReviewedCoverageOutcome>(wrong_item)
            .unwrap_err()
            .pointer(),
        "/Ok/reviewed_coverage/a.po/1"
    );
}

#[test]
fn json_reviewed_coverage_records_keep_required_fields_duplicate_refusal_and_unknown_policy() {
    for invalid in [
        "{}",
        "[]",
        "null",
        "{\"parsed_catalogs\":{}}",
        "{\"reviewed_coverage\":{}}",
        "{\"parsed_catalogs\":null,\"reviewed_coverage\":{}}",
        "{\"parsed_catalogs\":{},\"reviewed_coverage\":null}",
        "{\"parsed_catalogs\":{},\"parsed_catalogs\":{},\"reviewed_coverage\":{}}",
        "{\"parsed_catalogs\":{},\"reviewed_coverage\":{},\"reviewed_coverage\":{}}",
    ] {
        assert!(
            from_slice::<ReviewedCoverageObservation>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let unknown = OBSERVATION.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<ReviewedCoverageObservation>(unknown.as_bytes()).unwrap()),
        OBSERVATION.as_bytes()
    );
}

#[test]
fn json_reviewed_coverage_outcomes_refuse_mixed_unknown_wrong_payload_and_trailing_inputs() {
    for invalid in [
        "null",
        "[]",
        "{}",
        "\"Ok\"",
        "{\"Ok\":null}",
        "{\"Err\":null}",
        "{\"Ok\":{},\"Err\":{}}",
        "{\"Ok\":{},\"Ok\":{}}",
        "{\"Err\":{},\"Err\":{}}",
        "{\"Unknown\":{}}",
    ] {
        assert!(
            from_slice::<ReviewedCoverageOutcome>(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let invalid_error = format!(
        "{{\"Err\":{}}}",
        REJECTION.replace("\"causes\":[]", "\"causes\":null")
    );
    assert_eq!(
        from_slice::<ReviewedCoverageOutcome>(invalid_error.as_bytes())
            .unwrap_err()
            .pointer(),
        "/Err/causes"
    );
    assert!(from_slice::<ReviewedCoverageOutcome>(b"{").is_err());
    let trailing = format!("{{\"Ok\":{OBSERVATION}}} false");
    assert!(from_slice::<ReviewedCoverageOutcome>(trailing.as_bytes()).is_err());
}
