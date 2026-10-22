// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored reports/ledgers only. No held-out source, shapes, queries or producer.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const EMPTY: &[u8] = br#"{"schema":1,"query_rows":{},"nodes":[],"claims":[]}"#;

#[test]
fn json_claim_observation_literal_envelope_keeps_sorted_query_maps_rows_nulls_and_empty_fields() {
    let mut report = crate::scoreboards::ClaimAuditReport::default();
    report.query_rows.insert("z".to_owned(), Vec::new());
    report.query_rows.insert(
        "a".to_owned(),
        vec![
            [
                ("z".to_owned(), None),
                ("a".to_owned(), Some("中\n\"\\".to_owned())),
            ]
            .into(),
        ],
    );
    let expected = r#"{"schema":1,"query_rows":{"a":[{"a":"中\n\"\\","z":null}],"z":[]},"nodes":[],"claims":[]}"#;
    let bytes = crate::scoreboards::corpus_observation::encode(&report).unwrap();
    assert_eq!(bytes, expected.as_bytes());
    let restored = crate::scoreboards::corpus_observation::restore(&bytes).unwrap();
    assert_eq!(restored.query_rows, report.query_rows);
    assert_eq!(
        crate::scoreboards::corpus_observation::encode(
            &crate::scoreboards::ClaimAuditReport::default()
        )
        .unwrap(),
        EMPTY
    );
}

#[test]
fn json_claim_observation_schema_requires_strict_u32_and_existing_supported_version() {
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", "null", "\"1\""] {
        let input =
            format!("{{\"schema\":{token},\"query_rows\":{{}},\"nodes\":[],\"claims\":[]}}");
        assert_eq!(
            from_slice::<Stored>(input.as_bytes())
                .unwrap_err()
                .pointer(),
            "/schema"
        );
    }
    let max = br#"{"schema":4294967295,"query_rows":{},"nodes":[],"claims":[]}"#;
    assert_eq!(from_slice::<Stored>(max).unwrap().schema, u32::MAX);
    for token in ["0", "2", "4294967295"] {
        let input =
            format!("{{\"schema\":{token},\"query_rows\":{{}},\"nodes\":[],\"claims\":[]}}");
        let error = crate::scoreboards::corpus_observation::restore(input.as_bytes()).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("unsupported produced claim audit schema {token}"))
        );
    }
}

#[test]
fn json_claim_observation_outer_record_denies_unknowns_missing_nulls_duplicates_and_positional_arrays()
 {
    for input in [
        "[]",
        "{}",
        "null",
        "{\"schema\":1,\"query_rows\":{},\"nodes\":[],\"claims\":[],\"extra\":null}",
        "{\"schema\":1,\"schema\":1,\"query_rows\":{},\"nodes\":[],\"claims\":[]}",
        "{\"schema\":1,\"query_rows\":{},\"nodes\":null,\"claims\":[]}",
        "{\"schema\":1,\"query_rows\":{},\"claims\":[]}",
    ] {
        assert!(from_slice::<Stored>(input.as_bytes()).is_err(), "{input}");
    }
    assert!(from_slice::<Stored>(b"{").is_err());
    assert!(
        from_slice::<Stored>(br#"{"schema":1,"query_rows":{},"nodes":[],"claims":[]} false"#)
            .is_err()
    );
}

#[test]
fn json_claim_observation_real_causal_nodes_replay_original_identities_and_verdict() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        Diag::from(std::io::Error::other("premise")).with_focus("urn:audit:premise"),
        StageId::new("premise"),
    );
    ledger.attach(
        Diag::from(std::io::Error::other("rejection"))
            .with_focus("urn:audit:rejection")
            .with_antecedents([cause]),
        StageId::new("rejection"),
    );
    let report = crate::scoreboards::ClaimAuditReport {
        ledger,
        ..Default::default()
    };
    let bytes = crate::scoreboards::corpus_observation::encode(&report).unwrap();
    let stored: Stored = from_slice(&bytes).unwrap();
    let borrowed = Recorded {
        schema: stored.schema,
        query_rows: &stored.query_rows,
        nodes: stored.nodes.iter().collect(),
        claims: &stored.claims,
    };
    assert_eq!(to_vec(&borrowed), bytes);
    let restored = crate::scoreboards::corpus_observation::restore(&bytes).unwrap();
    assert_eq!(
        restored.ledger().emit_sorted(),
        report.ledger().emit_sorted()
    );
    assert_eq!(restored.ledger().verdict(), report.ledger().verdict());
    assert_eq!(restored.ledger().len(), 2);
}
