// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits};

#[test]
fn verification_observation_cache_preserves_nested_reports_without_recomputation() {
    // gmeow-test-input: synthetic-only
    let deep = format!(
        "{}1e400{}",
        "[".repeat(gmeow_errors::METADATA_JSON_DEPTH),
        "]".repeat(gmeow_errors::METADATA_JSON_DEPTH)
    );
    let report = gmeow_errors::Report::from_json_slice(
        format!(
            r#"{{"schema":"{}","tool":"verify","metadata":{{"deep":{deep}}}}}"#,
            gmeow_errors::REPORT_JSON_SCHEMA
        )
        .as_bytes(),
    )
    .unwrap();
    let observed = SourceObservation {
        source_path: "authored/scene.ttl".into(),
        source_digest: "authored-source".into(),
        original: report.clone(),
        repairs: std::collections::BTreeMap::from([(
            "repair".into(),
            RepairObservation {
                mutation_digest: "authored-mutation".into(),
                removed_rows: 1,
                inserted_rows: 2,
                report,
            },
        )]),
    };
    let directory = tempfile::tempdir().unwrap();
    let store = ActionStore::open(
        directory.path(),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .unwrap();
    let encoded = crate::stages::conformance::execution::cached_bytes(
        &store,
        "authored-verify-report-v2",
        Vec::new(),
        || Ok(purrdf_lex::json::write_compact(&observed.json_value().unwrap()).into_bytes()),
    )
    .unwrap();
    let hit = crate::stages::conformance::execution::cached_bytes(
        &store,
        "authored-verify-report-v2",
        Vec::new(),
        || panic!("an authenticated observation hit must not recompute the scene"),
    )
    .unwrap();
    assert_eq!(hit, encoded);
    let restored: SourceObservation = gmeow_errors::json::read_diagnostic_document(&hit).unwrap();
    assert_eq!(restored.source_path, observed.source_path);
    assert_eq!(restored.source_digest, observed.source_digest);
    assert_eq!(restored.original, observed.original);
    assert_eq!(
        restored.repairs["repair"].report,
        observed.repairs["repair"].report
    );
    assert_eq!(
        restored.repairs["repair"].mutation_digest,
        "authored-mutation"
    );
    assert_eq!(
        (
            restored.repairs["repair"].removed_rows,
            restored.repairs["repair"].inserted_rows
        ),
        (1, 2)
    );
    let aggregate = Observations {
        producer: crate::cache::BuildIdentity::current(),
        query_digests: std::collections::BTreeMap::from([("query".into(), "digest".into())]),
        query_set_digest: "query-set".into(),
        law_source_digests: std::collections::BTreeMap::from([("law".into(), "law-digest".into())]),
        sources: std::collections::BTreeMap::from([("scene".into(), restored)]),
    };
    let value = aggregate.json_value().unwrap();
    let decoded: Observations = gmeow_errors::json::read_diagnostic_document(
        purrdf_lex::json::write_compact(&value).as_bytes(),
    )
    .unwrap();
    assert!(decoded.json_value().unwrap().same_text(&value));
}
