// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored source observations and diagnostics only; no selected artifact is loaded.
use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};
const OBSERVED: &str = r#"{"sources":{"a":"digest-a","z":"digest-z"},"catalogs":{"x":[{"iri":"urn:cell","subject":"s","predicate":"p","obj":"o","source_endpoint":null,"target_endpoint":null,"sssom_file":"中\n\"\\","morphism_class":null,"morphism_kind":null,"preservation":null,"confidence":null,"grounding":false}]},"transpilation":{"a":{"Ok":0},"z":{"Ok":2}},"declared_subjects":{"a":["s","z"]},"imported_classes":{},"bfo_labels":{"b":["label"]}}"#;
#[test]
fn json_grounding_observation_complete_literal_preserves_record_collection_and_null_order() {
    let observed: Observations = from_slice(OBSERVED.as_bytes()).unwrap();
    assert_eq!(to_vec(&observed), OBSERVED.as_bytes());
    assert_eq!(observed.catalogs["x"][0].sssom_file, "中\n\"\\");
    assert_eq!(observed.transpilation["a"].as_ref().unwrap(), &0);
    let unknown = OBSERVED.replace(r#""sources":"#, r#""extra":null,"sources":"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        OBSERVED.as_bytes()
    );
    let absent = OBSERVED.replace(r#""source_endpoint":null,"#, "");
    assert_eq!(
        to_vec(&from_slice::<Observations>(absent.as_bytes()).unwrap()),
        OBSERVED.as_bytes()
    );
    let duplicate_set = OBSERVED.replace(r#"["s","z"]"#, r#"["z","s","s"]"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(duplicate_set.as_bytes()).unwrap()),
        OBSERVED.as_bytes()
    );
}
#[test]
fn json_grounding_observation_strict_count_widths_and_nested_map_admission() {
    for token in ["-0", "-1", "1.0", "1e0", "18446744073709551616", "null"] {
        let bad = OBSERVED.replace(r#""a":{"Ok":0}"#, &format!("\"a\":{{\"Ok\":{token}}}"));
        assert_eq!(
            from_slice::<Observations>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/transpilation/a/Ok",
            "{token}"
        );
    }
    let max = OBSERVED.replace(
        r#""a":{"Ok":0}"#,
        &format!("\"a\":{{\"Ok\":{}}}", usize::MAX),
    );
    assert_eq!(
        *from_slice::<Observations>(max.as_bytes())
            .unwrap()
            .transpilation["a"]
            .as_ref()
            .unwrap(),
        usize::MAX
    );
    let duplicate = OBSERVED.replace(r#""a":{"Ok":0}"#, r#""a":{"Ok":7},"a":{"Ok":0}"#);
    assert_eq!(
        to_vec(&from_slice::<Observations>(duplicate.as_bytes()).unwrap()),
        OBSERVED.as_bytes()
    );
    let invalid_earlier = OBSERVED.replace(r#""a":{"Ok":0}"#, r#""a":{"Ok":-0},"a":{"Ok":0}"#);
    assert_eq!(
        from_slice::<Observations>(invalid_earlier.as_bytes())
            .unwrap_err()
            .pointer(),
        "/transpilation/a/Ok"
    );
}
#[test]
fn json_grounding_observation_refuses_record_duplicates_missing_fields_and_bad_outcomes() {
    for bad in [
        OBSERVED.replace(r#""imported_classes":{},"#, ""),
        OBSERVED.replace(r#""imported_classes":{}"#, r#""imported_classes":null"#),
        OBSERVED.replace(
            r#""grounding":false"#,
            r#""grounding":false,"grounding":true"#,
        ),
        OBSERVED.replace(r#""a":{"Ok":0}"#, r#""a":{"Ok":0,"Err":null}"#),
        OBSERVED.replace(r#""a":{"Ok":0}"#, r#""a":{"Unknown":null}"#),
        format!("{OBSERVED} []"),
    ] {
        assert!(from_slice::<Observations>(bad.as_bytes()).is_err(), "{bad}");
    }
    let bad = OBSERVED.replace(r#""grounding":false"#, r#""grounding":"false""#);
    assert_eq!(
        from_slice::<Observations>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/catalogs/x/0/grounding"
    );
    assert!(
        from_slice::<Cell>(br#"["i","s","p","o",null,null,"f",null,null,null,null,false]"#)
            .is_err(),
        "records are object-only"
    );
}
#[test]
fn json_grounding_observation_recorded_failures_preserve_cause_identity_and_replay() {
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:grounding:premise"),
        StageId::new("read"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored refusal"))
            .with_focus("urn:grounding:result")
            .with_antecedents([premise]),
        StageId::new("transpile"),
    );
    assert_ne!(refusal.root.fingerprint, refusal.causes[0].fingerprint);
    let outcome = TranspilationOutcome(Err(refusal.clone()));
    let restored: TranspilationOutcome = from_slice(&to_vec(&outcome)).unwrap();
    assert_eq!(restored.0.unwrap_err(), refusal);
    let mut observed: Observations = from_slice(OBSERVED.as_bytes()).unwrap();
    observed
        .transpilation
        .insert("a".into(), Err(refusal.clone()));
    let restored: Observations = from_slice(&to_vec(&observed)).unwrap();
    assert_eq!(restored.transpilation["a"].as_ref().unwrap_err(), &refusal);
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
