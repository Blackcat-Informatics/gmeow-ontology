// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

#[test]
fn json_glossary_literal_triples_and_sorted_outcomes_preserve_duplicate_rows() {
    let row = Triple(
        "s".into(),
        "p".into(),
        purrdf::TermValue::Literal {
            lexical_form: "中\n\"\\".into(),
            datatype: "d".into(),
            language: None,
            direction: None,
        },
    );
    let triple = r#"["s","p",{"Literal":{"lexical_form":"中\n\"\\","datatype":"d","language":null,"direction":null}}]"#;
    assert_eq!(to_vec(&row), triple.as_bytes());
    let observed = Observations(
        [
            ("z".into(), Ok(vec![])),
            ("a".into(), Ok(vec![row.clone(), row])),
        ]
        .into(),
    );
    let expected = format!("{{\"a\":{{\"Ok\":[{triple},{triple}]}},\"z\":{{\"Ok\":[]}}}}");
    assert_eq!(to_vec(&observed), expected.as_bytes());
    let restored: Observations = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(restored.0["a"].as_ref().unwrap().len(), 2);
    assert_eq!(to_vec(&restored), expected.as_bytes());
}
#[test]
fn json_glossary_exact_tuple_results_and_typed_map_values_refuse_malformed_input() {
    for bad in [
        "[]",
        r#"["s"]"#,
        r#"["s","p"]"#,
        r#"["s","p",{"Iri":"o"},null]"#,
        r#"[null,"p",{"Iri":"o"}]"#,
        r#"["s",null,{"Iri":"o"}]"#,
        r#"["s","p",null]"#,
    ] {
        assert!(from_slice::<Triple>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = r#"{"source":{"Ok":[["s","p",{"Blank":{"label":"b","scope":-0}}]]}}"#;
    assert_eq!(
        from_slice::<Observations>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/source/Ok/0/2/Blank/scope"
    );
    for bad in [
        "[]",
        "null",
        r#"{"source":null}"#,
        r#"{"source":{"Ok":null}}"#,
        r#"{"source":{"Ok":[],"Err":null}}"#,
        r#"{"source":{"Ok":[],"Ok":[]}}"#,
        r#"{"source":{"Other":[]}}"#,
        r#"{"source":{"Ok":null},"source":{"Ok":[]}}"#,
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let last: Observations =
        from_slice(br#"{"source":{"Ok":[["s","p",{"Iri":"o"}]]},"source":{"Ok":[]}}"#).unwrap();
    assert!(last.0["source"].as_ref().unwrap().is_empty());
    assert!(from_slice::<Observations>(br#"{} null"#).is_err());
}
#[test]
fn json_glossary_and_auxiliary_refusals_preserve_causal_identity_and_replay() {
    use crate::stages::conformance::native_scene::AuxiliaryOutcome;
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("source unavailable")).with_focus("urn:glossary:source"),
        StageId::new("read"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("observation refused"))
            .with_focus("urn:glossary:observation")
            .with_antecedents([premise]),
        StageId::new("observe"),
    );
    assert_ne!(refusal.root.fingerprint, refusal.causes[0].fingerprint);
    let observed = Observations([("bad".into(), Err(refusal.clone()))].into());
    let restored: Observations = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.0["bad"].as_ref().unwrap_err(), &refusal);
    let auxiliary = AuxiliaryOutcome(Err(refusal.clone()));
    let restored: AuxiliaryOutcome = from_slice(&to_vec(&auxiliary)).unwrap();
    assert_eq!(restored.0.unwrap_err(), refusal);
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
