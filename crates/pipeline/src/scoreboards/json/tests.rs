// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Plain claims and authored query solutions; no corpus or scoreboard producer.

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const CLAIM: &str = r#"{"claim":"urn:c","text":"中\n\"\\","model":null,"method":"m","confidence":"1e400","evidence":[{"span":"s","chunk":null,"source":"source","start":-9223372036854775808,"end":9223372036854775807,"polarity":null}],"flags":{"ungrounded":true,"contradicted":false,"stale":true},"contradicts":["z","a"]}"#;

#[test]
fn json_claim_audit_literal_records_pin_order_nulls_text_confidence_and_signed_bounds() {
    let claim = FlatClaim {
        claim: "urn:c".to_owned(),
        text: Some("中\n\"\\".to_owned()),
        model: None,
        method: "m".to_owned(),
        confidence: Some("1e400".to_owned()),
        evidence: vec![ClaimEvidence {
            span: "s".to_owned(),
            chunk: None,
            source: Some("source".to_owned()),
            start: Some(i64::MIN),
            end: Some(i64::MAX),
            polarity: None,
        }],
        flags: ClaimFlags {
            ungrounded: true,
            contradicted: false,
            stale: true,
        },
        contradicts: vec!["z".to_owned(), "a".to_owned()],
    };
    assert_eq!(to_vec(&claim), CLAIM.as_bytes());
    assert_eq!(from_slice::<FlatClaim>(CLAIM.as_bytes()).unwrap(), claim);
}

#[test]
fn json_claim_audit_public_pretty_output_sorts_every_record_without_trailing_lf() {
    let mut report = crate::scoreboards::ClaimAuditReport::default();
    report.claims.push(FlatClaim {
        claim: "中\n\"\\".to_owned(),
        evidence: vec![ClaimEvidence {
            span: "s".to_owned(),
            chunk: None,
            source: Some("source".to_owned()),
            start: Some(-1),
            end: Some(2),
            polarity: None,
        }],
        ..FlatClaim::default()
    });
    let expected = r#"{
  "claims": [
    {
      "claim": "中\n\"\\",
      "confidence": null,
      "contradicts": [],
      "evidence": [
        {
          "chunk": null,
          "end": 2,
          "polarity": null,
          "source": "source",
          "span": "s",
          "start": -1
        }
      ],
      "flags": {
        "contradicted": false,
        "stale": false,
        "ungrounded": false
      },
      "method": "",
      "model": null,
      "text": null
    }
  ]
}"#;
    assert_eq!(
        crate::scoreboards::render_claim_audit_json(&report).unwrap(),
        expected
    );
    assert!(!expected.ends_with('\n'));
}

#[test]
fn json_claim_audit_offset_tokens_refuse_negative_zero_floats_exponents_and_overflow() {
    for name in ["start", "end"] {
        for token in [
            "-0",
            "1.0",
            "1e0",
            "9223372036854775808",
            "-9223372036854775809",
            "\"1\"",
            "false",
        ] {
            let input = format!("{{\"span\":\"s\",\"{name}\":{token}}}");
            assert_eq!(
                from_slice::<ClaimEvidence>(input.as_bytes())
                    .unwrap_err()
                    .pointer(),
                format!("/{name}")
            );
        }
        for token in ["0", "-1", "9223372036854775807", "-9223372036854775808"] {
            let input = format!("{{\"span\":\"s\",\"{name}\":{token}}}");
            assert!(from_slice::<ClaimEvidence>(input.as_bytes()).is_ok());
        }
    }
    let missing: ClaimEvidence = from_slice(br#"{"span":"s"}"#).unwrap();
    assert_eq!(missing.start, None);
    assert_eq!(missing.end, None);
    let nulls: ClaimEvidence = from_slice(br#"{"span":"s","start":null,"end":null}"#).unwrap();
    assert_eq!(missing, nulls);
}

#[test]
fn json_claim_audit_fields_keep_missing_null_duplicates_unknown_and_object_only_policies() {
    let absent = CLAIM.replace("\"model\":null,", "");
    assert_eq!(
        to_vec(&from_slice::<FlatClaim>(absent.as_bytes()).unwrap()),
        CLAIM.as_bytes()
    );
    let unknown = CLAIM.replacen('{', "{\"unknown\":1e400,", 1);
    assert_eq!(
        to_vec(&from_slice::<FlatClaim>(unknown.as_bytes()).unwrap()),
        CLAIM.as_bytes()
    );
    for bad in [
        "{}".to_owned(),
        "[]".to_owned(),
        "null".to_owned(),
        CLAIM.replace("\"method\":\"m\",", ""),
        CLAIM.replace("\"method\":\"m\"", "\"method\":null"),
        CLAIM.replace("\"model\":null", "\"model\":null,\"model\":null"),
        CLAIM.replace(
            "\"ungrounded\":true",
            "\"ungrounded\":true,\"ungrounded\":true",
        ),
        CLAIM.replace("\"span\":\"s\"", "\"span\":null"),
        CLAIM.replace(
            "\"start\":-9223372036854775808",
            "\"start\":null,\"start\":null",
        ),
    ] {
        assert!(from_slice::<FlatClaim>(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(from_slice::<FlatClaim>(format!("{CLAIM} false").as_bytes()).is_err());
    assert!(from_slice::<FlatClaim>(b"{").is_err());
}

#[test]
fn json_claim_audit_query_focus_literal_retains_native_bytes_nulls_and_fingerprint() {
    use crate::scoreboards::{ClaimAuditReport, native_query};
    use gmeow_errors::{
        Diag, DiagLedger, FindingCategory, Grade, Location, Severity, StageId, Standpoint,
        register_code,
    };
    use purrdf::TermValue;
    use std::path::Path;
    // IRI tag0, little-endian length5, then the literal UTF-8 bytes of urn:c.
    const FOCUS: &str = r#"{"claim":[0,5,0,0,0,0,0,0,0,117,114,110,58,99],"z":null}"#;
    let mut report = ClaimAuditReport::default();
    report
        .record_query(
            Path::new("authored.rq"),
            "stale-source-claims".to_owned(),
            native_query::Solutions {
                variables: vec!["z".to_owned(), "claim".to_owned()],
                rows: vec![vec![None, Some(TermValue::iri("urn:c"))]],
            },
        )
        .unwrap();
    let mut expected = DiagLedger::new();
    expected.attach(
        Diag::new(
            register_code("audit.stale-source"),
            Grade::new(
                Severity::Warning,
                FindingCategory::PolicyWarning,
                Standpoint::Perspectival,
            ),
            "independent expected identity",
        )
        .with_focus(FOCUS)
        .with_location(Location::new(
            Some("authored.rq".to_owned()),
            None,
            None,
            Some("urn:c".to_owned()),
        )),
        StageId::new("scoreboard.audit.query"),
    );
    let actual_nodes = report.ledger().emit_sorted();
    assert_eq!(actual_nodes.len(), 1);
    assert_eq!(actual_nodes[0].source_ctx.focus.as_ref().unwrap().0, FOCUS);
    assert_eq!(
        actual_nodes[0].fingerprint,
        expected.emit_sorted()[0].fingerprint
    );
    assert_eq!(report.query_rows["stale-source-claims"][0]["z"], None);
}
