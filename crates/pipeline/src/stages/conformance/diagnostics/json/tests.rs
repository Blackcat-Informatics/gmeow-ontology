// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf_lex::json::record::{from_slice, to_vec};

const META: &str = r#"{"traces":[["effect","root"]],"root_cause":[["effect","root"]],"cluster":[["effect","root"]],"cluster_root":["root"],"cluster_root_edges":[["root","root"]],"cluster_typed":["root"],"root_finding_typed":["root"],"glut":[["opposed","supported"],["supported","opposed"]]}"#;
const GATE: &str = r#"{"coordinates":{"finding":["error","blocking","binding"]},"category_blocking":{"category":"blocking"},"fatal":["finding"],"projections":{"fatal":{"nquads":"verbatim\n中","fatal":["finding"]}}}"#;

fn observation() -> Observations {
    Observations {
        meta_sources: ["z".into(), "a".into()].into(),
        meta: [("fixture".into(), Ok(from_slice(META.as_bytes()).unwrap()))].into(),
        gate: Ok(from_slice(GATE.as_bytes()).unwrap()),
        reports: [(
            "report".into(),
            Ok(MetaReportObservation {
                nquads: "verbatim\n中".into(),
                derivation: from_slice(META.as_bytes()).unwrap(),
            }),
        )]
        .into(),
    }
}

#[test]
fn native_diagnostic_observations_preserve_complete_wire_and_directed_evidence() {
    let derived = MetaDerivation {
        traces: [("effect".into(), "root".into())].into(),
        root_cause: [("effect".into(), "root".into())].into(),
        cluster: [("effect".into(), "root".into())].into(),
        cluster_root: ["root".into()].into(),
        cluster_root_edges: [("root".into(), "root".into())].into(),
        cluster_typed: ["root".into()].into(),
        root_finding_typed: ["root".into()].into(),
        glut: [
            ("supported".into(), "opposed".into()),
            ("opposed".into(), "supported".into()),
        ]
        .into(),
    };
    assert_eq!(to_vec(&derived), META.as_bytes());
    assert_eq!(
        from_slice::<MetaDerivation>(META.as_bytes()).unwrap(),
        derived
    );
    let expected = format!(
        r#"{{"meta_sources":["a","z"],"meta":{{"fixture":{{"Ok":{META}}}}},"gate":{{"Ok":{GATE}}},"reports":{{"report":{{"Ok":{{"nquads":"verbatim\n中","derivation":{META}}}}}}}}}"#
    );
    assert_eq!(to_vec(&observation()), expected.as_bytes());
    let restored: Observations = from_slice(expected.as_bytes()).unwrap();
    assert_eq!(to_vec(&restored), expected.as_bytes());
    let unknown = expected.replacen('{', "{\"extension\":false,", 1);
    assert_eq!(
        to_vec(&from_slice::<Observations>(unknown.as_bytes()).unwrap()),
        expected.as_bytes()
    );
    let repeated = META.replace(
        r#""traces":[["effect","root"]]"#,
        r#""traces":[["effect","root"],["effect","root"]]"#,
    );
    assert_eq!(
        to_vec(&from_slice::<MetaDerivation>(repeated.as_bytes()).unwrap()),
        META.as_bytes()
    );
}

#[test]
fn native_diagnostic_observations_refuse_incomplete_records_and_malformed_coordinates() {
    for row in ["[]", r#"["a"]"#, r#"["a","b","c"]"#, "null"] {
        let bad = META.replace(
            r#""traces":[["effect","root"]]"#,
            &format!(r#""traces":[{row}]"#),
        );
        assert_eq!(
            from_slice::<MetaDerivation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/traces/0"
        );
    }
    let bad = META.replace(r#"["effect","root"]"#, r#"["effect",false]"#);
    assert_eq!(
        from_slice::<MetaDerivation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/traces/0/1"
    );
    for row in ["[]", r#"["s","c"]"#, r#"["s","c","p","extra"]"#, "null"] {
        let bad = GATE.replace(r#"["error","blocking","binding"]"#, row);
        assert_eq!(
            from_slice::<GateObservation>(bad.as_bytes())
                .unwrap_err()
                .pointer(),
            "/coordinates/finding"
        );
    }
    let bad = GATE.replace(r#"["error","blocking","binding"]"#, r#"["s",false,"p"]"#);
    assert_eq!(
        from_slice::<GateObservation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/coordinates/finding/1"
    );
    // Every duplicate map value is admitted, even when a later member would replace it.
    let bad = GATE.replace(
        r#""coordinates":{"finding":["error","blocking","binding"]}"#,
        r#""coordinates":{"finding":[],"finding":["s","c","p"]}"#,
    );
    assert_eq!(
        from_slice::<GateObservation>(bad.as_bytes())
            .unwrap_err()
            .pointer(),
        "/coordinates/finding"
    );
    for value in ["{}", "[]", r#"{"traces":[],"traces":[]}"#] {
        assert!(from_slice::<MetaDerivation>(value.as_bytes()).is_err());
        assert!(from_slice::<Observations>(value.as_bytes()).is_err());
        assert!(from_slice::<GateObservation>(value.as_bytes()).is_err());
        assert!(from_slice::<MetaReportObservation>(value.as_bytes()).is_err());
        assert!(from_slice::<ProjectionObservation>(value.as_bytes()).is_err());
    }
    let duplicate = META.replacen('{', "{\"traces\":[],", 1);
    assert!(from_slice::<MetaDerivation>(duplicate.as_bytes()).is_err());
    let omitted = META.replace(r#""cluster_root_edges":[["root","root"]],"#, "");
    assert!(from_slice::<MetaDerivation>(omitted.as_bytes()).is_err());
}

#[test]
fn native_diagnostic_observation_cache_retains_causal_refusals_without_recomputation() {
    use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits};
    use gmeow_errors::{Diag, DiagLedger, StageId};
    let mut ledger = DiagLedger::new();
    let premise = ledger.attach(
        Diag::from(std::io::Error::other("authored premise")).with_focus("urn:diagnostic:premise"),
        StageId::new("input"),
    );
    let refusal = ledger.record(
        Diag::from(std::io::Error::other("authored refusal"))
            .with_focus("urn:diagnostic:refusal")
            .with_antecedents([premise]),
        StageId::new("derive"),
    );
    let directory = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        ActionStore::default_root(directory.path()),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let observed = super::super::super::execution::cached_native_inputs(
        &store,
        "authored-diagnostic-observations",
        Vec::new(),
        || {
            let mut observed = observation();
            observed.meta.insert("refusal".into(), Err(refusal.clone()));
            observed
                .reports
                .insert("refusal".into(), Err(refusal.clone()));
            Ok(observed)
        },
    )
    .unwrap();
    let restored: Observations = super::super::super::execution::cached_native_inputs(
        &store,
        "authored-diagnostic-observations",
        Vec::new(),
        || panic!("cache hit must not recompute observations"),
    )
    .unwrap();
    assert_eq!(to_vec(&observed), to_vec(&restored));
    assert_eq!(restored.meta["refusal"].as_ref().unwrap_err(), &refusal);
    assert_eq!(restored.reports["refusal"].as_ref().unwrap_err(), &refusal);
    let mut failed = observation();
    failed.gate = Err(refusal.clone());
    let restored: Observations = from_slice(&to_vec(&failed)).unwrap();
    assert_eq!(restored.gate.unwrap_err(), refusal);
    let mut replay = DiagLedger::new();
    replay.replay(refusal.causes);
    replay.replay([*refusal.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}
