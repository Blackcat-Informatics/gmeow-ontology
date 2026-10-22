// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const WALK: &str = r#"{"phases":["later","first","later"],"iterated":["中"]}"#;
#[test]
fn json_conformance_plan_literal_order_and_record_refusals() {
    let walk = PhaseWalk {
        phases: vec!["later".into(), "first".into(), "later".into()],
        iterated: vec!["中".into()],
    };
    assert_eq!(to_vec(&walk), WALK.as_bytes());
    let expected = format!(
        "{{\"source_path\":\"plan.ttl\",\"source_digest\":\"d\",\"plan_types\":[\"a\",\"z\"],\"body_roots\":[\"z\",\"a\"],\"walk\":{{\"Ok\":{WALK}}}}}"
    );
    let observation = Observation {
        source_path: "plan.ttl".into(),
        source_digest: "d".into(),
        plan_types: ["z".into(), "a".into()].into(),
        body_roots: vec!["z".into(), "a".into()],
        walk: Ok(walk),
    };
    assert_eq!(to_vec(&observation), expected.as_bytes());
    assert_eq!(
        to_vec(&from_slice::<Observation>(expected.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    for bad in [
        "{}".to_owned(),
        "[]".into(),
        expected.replace("\"body_roots\":[\"z\",\"a\"],", ""),
        expected.replace("\"plan_types\":[\"a\",\"z\"]", "\"plan_types\":null"),
        expected.replace("\"walk\":", "\"walk\":null,\"walk\":"),
        expected.replace(&format!("{{\"Ok\":{WALK}}}"), "{\"Ok\":null}"),
    ] {
        assert!(from_slice::<Observation>(bad.as_bytes()).is_err(), "{bad}");
    }
    assert!(from_slice::<PhaseWalk>(br#"{"phases":[],"iterated":[],"iterated":[]}"#).is_err());
    let unknown = expected.replacen('{', "{\"extra\":[],", 1);
    assert_eq!(
        to_vec(&from_slice::<Observation>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
}
#[test]
fn json_conformance_plan_and_signature_errors_preserve_causal_replay() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("parse premise")).with_focus("urn:plan:premise"),
        StageId::new("parse"),
    );
    let failure = ledger.record(
        Diag::from(std::io::Error::other("walk refused"))
            .with_focus("urn:plan:walk")
            .with_antecedents([premise]),
        StageId::new("walk"),
    );
    assert_ne!(failure.root.fingerprint, failure.causes[0].fingerprint);
    let observed = Observation {
        source_path: "p".into(),
        source_digest: "d".into(),
        plan_types: Default::default(),
        body_roots: vec![],
        walk: Err(failure.clone()),
    };
    let restored: Observation = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.walk.unwrap_err(), failure);
    let case = crate::stages::conformance::gmn_signatures::CaseObservation {
        query_path: "q".into(),
        query_digest: "d".into(),
        injection_digest: Some("i".into()),
        rows: Err(failure.clone()),
    };
    let restored: crate::stages::conformance::gmn_signatures::CaseObservation =
        from_slice(&to_vec(&case)).unwrap();
    assert_eq!(restored.rows.unwrap_err(), failure);
    let mut replay = DiagLedger::new();
    replay.replay(failure.causes);
    replay.replay([*failure.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
