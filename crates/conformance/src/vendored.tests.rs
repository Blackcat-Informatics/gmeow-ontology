// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use serde_json::json;

fn meta_value(license: &str, lane: &str) -> Value {
    json!({
        "name": "tiny",
        "spdx_license": license,
        "source_url": "https://example.org/tiny",
        "version_or_commit": "v1",
        "refresh_command": "cargo run -p gmeow-conformance --bin ingest-external -- ...",
        "lane": lane,
    })
}

#[test]
fn parses_a_well_formed_corpus_json() {
    let m = parse_corpus_meta(&meta_value("CC-BY-4.0", "a")).unwrap();
    assert_eq!(m.name, "tiny");
    assert_eq!(m.spdx_license, "CC-BY-4.0");
    assert_eq!(m.lane, Lane::A);
}

#[test]
fn import_ok_corpus_passes_the_audit() {
    let m = parse_corpus_meta(&meta_value("CC-BY-4.0", "a")).unwrap();
    assert!(audit_vendorable(&m).is_ok());
}

#[test]
fn reference_only_corpus_fails_the_audit() {
    let m = parse_corpus_meta(&meta_value("CC-BY-NC-SA-4.0", "b")).unwrap();
    let err = audit_vendorable(&m).unwrap_err();
    assert!(err.message().contains("REFERENCE_ONLY"), "{err}");
}

#[test]
fn unknown_license_fails_the_audit() {
    let m = parse_corpus_meta(&meta_value("WTFPL", "a")).unwrap();
    assert!(audit_vendorable(&m).is_err());
}

#[test]
fn unknown_lane_hard_fails() {
    let err = parse_corpus_meta(&meta_value("CC-BY-4.0", "c")).unwrap_err();
    assert!(err.message().contains("lane must be"), "{err}");
}

#[test]
fn divergence_lane_parses() {
    let m = parse_corpus_meta(&meta_value("W3C", "divergence")).unwrap();
    assert_eq!(m.lane, Lane::Divergence);
}

#[test]
fn native_profiled_lane_round_trips() {
    let m = parse_corpus_meta(&meta_value("W3C", "native-profiled")).unwrap();
    assert_eq!(m.lane, Lane::NativeProfiled);
    // The wire token is the inverse of `parse`.
    assert_eq!(m.lane.as_str(), "native-profiled");
}

#[test]
fn missing_field_hard_fails() {
    let err = parse_corpus_meta(&json!({ "name": "tiny" })).unwrap_err();
    assert!(
        err.message().contains("missing the required string field"),
        "{err}"
    );
}

#[test]
fn unknown_key_hard_fails() {
    let mut v = meta_value("CC-BY-4.0", "a");
    v.as_object_mut().unwrap().insert("nope".into(), json!(1));
    let err = parse_corpus_meta(&v).unwrap_err();
    assert!(err.message().contains("unknown key"), "{err}");
}

/// `lane_for_case` is the consumer that makes the `lane` field load-bearing: the
/// Lane-A native runners skip a case iff this returns `Some(Lane::B)`. Exercise all
/// three branches over a synthetic corpus tree (no new dev-dependency: plain
/// `std::fs` under a pid-unique temp dir).
#[test]
fn lane_for_case_routes_external_corpora_and_ignores_endogenous() {
    use std::fs;

    fn corpus_json(name: &str, lane: &str) -> String {
        format!(
            "{{ \"name\": \"{name}\", \"spdx_license\": \"CC-BY-4.0\", \
                 \"source_url\": \"https://example.org/{name}\", \
                 \"version_or_commit\": \"v1\", \"refresh_command\": \"noop\", \
                 \"lane\": \"{lane}\" }}\n"
        )
    }

    let tmp = tempfile::tempdir().expect("create temp dir");
    let base = tmp.path();

    // External Lane-B corpus: a case here must be skipped by the native gate.
    let case_b = base.join("external/heavy-corpus/some-case");
    fs::create_dir_all(&case_b).unwrap();
    fs::write(
        base.join("external/heavy-corpus/corpus.json"),
        corpus_json("heavy-corpus", "b"),
    )
    .unwrap();

    // External Lane-A corpus: a case here runs in the native gate.
    let case_a = base.join("external/light-corpus/case-a");
    fs::create_dir_all(&case_a).unwrap();
    fs::write(
        base.join("external/light-corpus/corpus.json"),
        corpus_json("light-corpus", "a"),
    )
    .unwrap();

    // Endogenous case: no parent corpus.json → always native, never skipped.
    let endo = base.join("profiles/plain-case");
    fs::create_dir_all(&endo).unwrap();

    assert_eq!(lane_for_case(&case_b).unwrap(), Some(Lane::B));
    assert_eq!(lane_for_case(&case_a).unwrap(), Some(Lane::A));
    assert_eq!(lane_for_case(&endo).unwrap(), None);
}
