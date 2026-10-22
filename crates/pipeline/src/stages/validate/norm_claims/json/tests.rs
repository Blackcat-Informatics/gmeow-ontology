// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored observations and causal ledgers only; no source catalog or closure run.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const CLAIMS: &str = r#"{"graph":"urn:graph","graph_digest":"中\n\"\\","asserted_quads":7,"advisory_assessments":["urn:a","urn:z"]}"#;
const CLOSURE: &str =
    r#"{"source_path":"norms.ttl","source_digest":"digest","source_quads":1,"closure_triples":9}"#;
const REJECTION: &str = r#"{"root":{"fingerprint":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"stage":"closure","grade":{"severity":"error","category":"data-shape-violation","standpoint":"binding"},"code":"authored.rejection","observations":[],"frames":[],"antecedents":[],"source_ctx":{"location":{},"focus":null,"term_role":null},"attributions":[],"advice":[],"labels":[],"tags":[],"knowledge":"supported","emitted_at":{"file":"authored.rs","line":1,"column":2}},"causes":[]}"#;

#[test]
fn json_norm_claims_literal_success_envelope_pins_bytes_and_set_order() {
    let observation = Observation {
        claims: ShippedObservation {
            graph: "urn:graph".to_owned(),
            graph_digest: "中\n\"\\".to_owned(),
            asserted_quads: 7,
            advisory_assessments: ["urn:z".to_owned(), "urn:a".to_owned()].into(),
        },
        closure: Ok(ClosureObservation {
            source_path: "norms.ttl".to_owned(),
            source_digest: "digest".to_owned(),
            source_quads: 1,
            closure_triples: 9,
        }),
    };
    let expected = format!("{{\"claims\":{CLAIMS},\"closure\":{{\"Ok\":{CLOSURE}}}}}");
    assert_eq!(to_vec(&observation), expected.as_bytes());
    let restored: Observation = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    assert_eq!(restored.claims, observation.claims);
    assert_eq!(restored.closure.unwrap().closure_triples, 9);
}

#[test]
fn json_norm_claims_recorded_failure_retains_causal_identity() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        Diag::from(std::io::Error::other("premise")).with_focus("urn:norm-claims:premise"),
        StageId::new("premise"),
    );
    let rejection = ledger.record(
        Diag::from(std::io::Error::other("closure refused"))
            .with_focus("urn:norm-claims:closure")
            .with_antecedents([cause]),
        StageId::new("closure"),
    );
    assert_eq!(rejection.causes.len(), 1);
    assert_ne!(rejection.root.fingerprint, rejection.causes[0].fingerprint);
    let expected_error = rejection.clone();
    let observation = Observation {
        claims: from_slice(CLAIMS.as_bytes()).unwrap(),
        closure: Err(rejection),
    };
    let bytes = to_vec(&observation);
    let restored: Observation = from_slice(&bytes).unwrap();
    assert_eq!(restored.closure.unwrap_err(), expected_error);
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("\"closure\":{\"Err\":{\"root\":")
    );
    let literal = format!("{{\"claims\":{CLAIMS},\"closure\":{{\"Err\":{REJECTION}}}}}");
    let restored: Observation = from_slice(literal.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), literal.as_bytes());
    assert_eq!(
        restored.closure.unwrap_err().root.code,
        "authored.rejection"
    );
}

#[test]
fn json_norm_claims_counts_require_unsigned_integer_tokens_with_host_bounds() {
    for token in [
        "-0",
        "-1",
        "1.0",
        "1e0",
        "18446744073709551616",
        "null",
        "\"1\"",
    ] {
        let bad = CLAIMS.replace(
            "\"asserted_quads\":7",
            &format!("\"asserted_quads\":{token}"),
        );
        let error = from_slice::<ShippedObservation>(bad.as_bytes()).unwrap_err();
        assert_eq!(error.pointer(), "/asserted_quads");
        for field in ["source_quads", "closure_triples"] {
            let old = if field == "source_quads" { 1 } else { 9 };
            let bad = CLOSURE.replace(
                &format!("\"{field}\":{old}"),
                &format!("\"{field}\":{token}"),
            );
            assert_eq!(
                from_slice::<ClosureObservation>(bad.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{field}")
            );
        }
    }
    let max = CLAIMS.replace(
        "\"asserted_quads\":7",
        &format!("\"asserted_quads\":{}", usize::MAX),
    );
    assert_eq!(
        from_slice::<ShippedObservation>(max.as_bytes())
            .unwrap()
            .asserted_quads,
        usize::MAX
    );
}

#[test]
fn json_norm_claims_records_and_result_refuse_missing_null_duplicate_and_mixed_variants() {
    for invalid in [
        "{}",
        "null",
        "[]",
        "{\"Ok\":null}",
        "{\"Err\":null}",
        "{\"ok\":{}}",
        "{\"Ok\":{},\"Err\":{}}",
        "{\"Ok\":{},\"Ok\":{}}",
    ] {
        let input = format!("{{\"claims\":{CLAIMS},\"closure\":{invalid}}}");
        assert!(
            from_slice::<Observation>(input.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    for bad in [
        CLAIMS.replace("\"graph\":\"urn:graph\",", ""),
        CLAIMS.replace("\"graph\":\"urn:graph\"", "\"graph\":null"),
        CLAIMS.replace("\"graph\":\"urn:graph\"", "\"graph\":\"a\",\"graph\":\"b\""),
    ] {
        assert!(from_slice::<ShippedObservation>(bad.as_bytes()).is_err());
    }
    let unknown = CLAIMS.replacen('{', "{\"unknown\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<ShippedObservation>(unknown.as_bytes()).unwrap()),
        CLAIMS.as_bytes()
    );
    assert!(from_slice::<Observation>(b"[]").is_err());
    assert!(from_slice::<ShippedObservation>(format!("{CLAIMS} false").as_bytes()).is_err());
}
