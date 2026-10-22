// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const POSITIVE: &str = r#"{"text":"中\n\"\\","source":"s","reconstructed":{"Ok":"s"},"content_digest":"unchanged","per_claim":{"Ok":null},"idempotence":{"Ok":null}}"#;
#[test]
fn json_conformance_positive_literal_bytes_and_null_witnesses() {
    let observed = PositiveObservation {
        text: "中\n\"\\".into(),
        source: "s".into(),
        reconstructed: Ok("s".into()),
        content_digest: "unchanged".into(),
        per_claim: Ok(()),
        idempotence: Ok(()),
    };
    assert_eq!(to_vec(&observed), POSITIVE.as_bytes());
    let restored: PositiveObservation = from_slice(POSITIVE.as_bytes()).unwrap();
    assert!(restored.failures(observed.text.as_bytes()).is_empty());
    assert_eq!(to_vec(&restored), POSITIVE.as_bytes());
    let unknown = POSITIVE.replacen('{', "{\"extra\":true,", 1);
    assert_eq!(
        to_vec(&from_slice::<PositiveObservation>(unknown.as_bytes()).unwrap()),
        POSITIVE.as_bytes()
    );
}
#[test]
fn json_conformance_positive_requires_real_unit_and_result_payloads() {
    for field in ["per_claim", "idempotence"] {
        for payload in ["false", "0", "[]", "{}", "\"\""] {
            let bad = POSITIVE.replace(
                &format!("\"{field}\":{{\"Ok\":null}}"),
                &format!("\"{field}\":{{\"Ok\":{payload}}}"),
            );
            assert_eq!(
                from_slice::<PositiveObservation>(bad.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{field}/Ok")
            );
        }
    }
    for bad in [
        "{}".into(),
        "[]".into(),
        POSITIVE.replace("\"source\":\"s\",", ""),
        POSITIVE.replace("\"source\":\"s\"", "\"source\":null"),
        POSITIVE.replace("\"text\":", "\"text\":\"first\",\"text\":"),
        POSITIVE.replace(
            "\"per_claim\":{\"Ok\":null}",
            "\"per_claim\":{\"Ok\":null,\"Err\":null}",
        ),
        POSITIVE.replace(
            "\"idempotence\":{\"Ok\":null}",
            "\"idempotence\":{\"Other\":null}",
        ),
        format!("{POSITIVE} []"),
    ] {
        assert!(
            from_slice::<PositiveObservation>(bad.as_bytes()).is_err(),
            "{bad}"
        );
    }
}
#[test]
fn json_conformance_positive_all_error_branches_retain_causal_identity() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:vector:premise"),
        StageId::new("input"),
    );
    let failure = ledger.record(
        Diag::from(std::io::Error::other("codec refused"))
            .with_focus("urn:vector:codec")
            .with_antecedents([cause]),
        StageId::new("codec"),
    );
    let observed = PositiveObservation {
        text: "surface".into(),
        source: "graph".into(),
        reconstructed: Err(failure.clone()),
        content_digest: "digest".into(),
        per_claim: Err(failure.clone()),
        idempotence: Err(failure.clone()),
    };
    let restored: PositiveObservation = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.reconstructed.unwrap_err(), failure);
    assert_eq!(restored.per_claim.unwrap_err(), failure);
    assert_eq!(restored.idempotence.unwrap_err(), failure);
    assert_eq!(
        failure.root.antecedents.as_ref(),
        &[failure.causes[0].fingerprint]
    );
    assert_ne!(failure.root.fingerprint, failure.causes[0].fingerprint);
    let mut replay = DiagLedger::new();
    replay.replay(failure.causes);
    replay.replay([*failure.root]);
    assert_eq!(replay.verdict(), ledger.verdict());
    assert_eq!(replay.len(), 2);
}
