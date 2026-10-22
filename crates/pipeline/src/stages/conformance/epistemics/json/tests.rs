// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic::reason::RlTriple;
use purrdf_lex::json::record::{from_slice, to_vec};
const ROW: &str = r#"{"subject":"urn:s","predicate":"urn:p","object":"\"中\"@xx","world":"urn:world","is_edb":false}"#;
#[test]
fn json_epistemics_literal_scope_keeps_total_independent_of_selected_duplicates() {
    let row = RlTriple {
        subject: "urn:s".into(),
        predicate: "urn:p".into(),
        object: "\"中\"@xx".into(),
        world: "urn:world".into(),
        is_edb: false,
    };
    let probe = Probe {
        total_triples: 19,
        triples: vec![row.clone(), row],
    };
    let expected_probe = format!("{{\"total_triples\":19,\"triples\":[{ROW},{ROW}]}}");
    assert_eq!(to_vec(&probe), expected_probe.as_bytes());
    let observed = Observation {
        universe: ["z".into(), "a".into()].into(),
        probes: [
            (
                "z".into(),
                Ok(Probe {
                    total_triples: 0,
                    triples: vec![],
                }),
            ),
            ("a".into(), Ok(probe)),
        ]
        .into(),
    };
    let expected = format!(
        "{{\"universe\":[\"a\",\"z\"],\"probes\":{{\"a\":{{\"Ok\":{expected_probe}}},\"z\":{{\"Ok\":{{\"total_triples\":0,\"triples\":[]}}}}}}}}"
    );
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: Observation = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.probes["a"].as_ref().unwrap().total_triples, 19);
    assert_eq!(restored.probes["a"].as_ref().unwrap().triples.len(), 2);
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let unknown = expected.replacen('{', "{\"extra\":null,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_epistemics_counts_and_probe_results_fail_at_native_nested_pointer() {
    for token in ["-0", "-1", "1e0", "1.0", "18446744073709551616", "null"] {
        let bad = format!(
            "{{\"universe\":[],\"probes\":{{\"p\":{{\"Ok\":{{\"total_triples\":{token},\"triples\":[]}}}}}}}}"
        );
        assert_eq!(
            from_slice::<Observation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/probes/p/Ok/total_triples"
        );
    }
    let max = format!("{{\"total_triples\":{},\"triples\":[]}}", usize::MAX);
    assert_eq!(
        from_slice::<Probe>(max.as_bytes()).unwrap().total_triples,
        usize::MAX
    );
    for bad in [
        "{}",
        "[]",
        r#"{"universe":null,"probes":{}}"#,
        r#"{"universe":[],"probes":{},"probes":{}}"#,
        r#"{"universe":[],"probes":{"p":{"Ok":null}}}"#,
        r#"{"universe":[],"probes":{"p":{"Ok":{"total_triples":0,"triples":null}}}}"#,
        r#"{"universe":[],"probes":{"p":{"Ok":{"total_triples":0,"triples":[]},"Err":null}}}"#,
    ] {
        assert!(from_slice::<Observation>(bad.as_bytes()).is_err(), "{bad}");
    }
    let duplicate = r#"{"universe":[],"probes":{"p":{"Ok":{"total_triples":-0,"triples":[]}},"p":{"Ok":{"total_triples":0,"triples":[]}}}}"#;
    assert_eq!(
        from_slice::<Observation>(duplicate.as_bytes())
            .unwrap_err()
            .pointer(),
        "/probes/p/Ok/total_triples"
    );
    let bad = format!(
        "{{\"total_triples\":1,\"triples\":[{}]}}",
        ROW.replace("\"is_edb\":false", "\"is_edb\":0")
    );
    assert_eq!(
        from_slice::<Probe>(bad.as_bytes()).unwrap_err().pointer(),
        "/triples/0/is_edb"
    );
}
#[test]
fn json_epistemics_recorded_refusal_preserves_causal_graph_and_replay() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("probe input")).with_focus("urn:probe:input"),
        StageId::new("input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("closure refusal"))
            .with_focus("urn:probe:closure")
            .with_context("exact original closure")
            .with_antecedents([premise]),
        StageId::new("closure"),
    );
    assert_ne!(refusal.root.fingerprint, refusal.causes[0].fingerprint);
    let observed = Observation {
        universe: Default::default(),
        probes: [("refused".into(), Err(refusal.clone()))].into(),
    };
    let restored: Observation = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.probes["refused"].as_ref().unwrap_err(), &refusal);
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
