// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authored JSON bytes and an isolated authenticated action store only.

use super::*;
use crate::cache::ValidationCache;
use gmeow_action_cache::{ProducerIdentity, bytes_digest};
use purrdf_lex::json::record::{from_slice, to_vec};

const PAYLOAD: &str = r#"{"findings":[{"severity":"error","code":"authored.code","message":"evidence\n中😀","source_graph":"urn:graph","source_focus":"urn:focus","tool":"tiny","locations":[{"path":"a.ttl","line":18446744073709551615,"column":4294967295}],"related_locations":[{}],"related_labels":[{"location":{},"message":"origin"}],"suggestions":["repair"],"advice":[{"standpoint":"advisory","text":"review","help_uri":null}],"remediation":[{"text":"fix","standpoint":"binding"}],"finding_iri":"urn:finding","anchor_iri":"urn:anchor","anchor_non_trivial":true,"antecedents":["urn:prior"],"root_cause":"urn:root","cluster":"urn:cluster","cross_node_glut_with":["urn:other"],"tags":["tag"],"detail":"detail","failure_class":"urn:failure","category":"data-shape-violation","standpoint":"binding","attributions":[{"slice_iri":"urn:slice","role":"assertion-origin"}],"documented_terms":["urn:term"],"guidance":[{"modality":"HowToUse","source":"DocumentedTerm","term_iri":"urn:term","text":"usage","standpoint":"advisory"}],"derived_from_quads":["urn:quad"],"cited_iris":["urn:citation"]}]}"#;

fn cache(root: &std::path::Path, implementation: &str) -> ValidationCache {
    ValidationCache::new(
        root,
        ProducerIdentity::new(bytes_digest(implementation.as_bytes())),
    )
    .unwrap()
}

#[test]
fn json_cached_findings_actual_store_preserves_complete_bytes_replay_and_context() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache(directory.path(), "authored-producer");
    let expected: CachedResult = from_slice(PAYLOAD.as_bytes()).unwrap();
    cache
        .write_cached_result("selected", "input", &expected)
        .unwrap();
    let blobs: Vec<_> = std::fs::read_dir(cache.cache_dir().join("blobs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(blobs.len(), 1);
    assert_eq!(std::fs::read(&blobs[0]).unwrap(), PAYLOAD.as_bytes());
    let replay = cache
        .read_cached_result("selected", "input")
        .unwrap()
        .unwrap();
    assert_eq!(replay, expected);
    assert_eq!(to_vec(&replay), PAYLOAD.as_bytes());
    assert!(
        cache
            .read_cached_result("other", "input")
            .unwrap()
            .is_none()
    );
    assert!(
        cache
            .read_cached_result("selected", "other")
            .unwrap()
            .is_none()
    );
    let changed = self::cache(directory.path(), "other-producer");
    assert!(
        changed
            .read_cached_result("selected", "input")
            .unwrap()
            .is_none()
    );
    std::fs::write(&blobs[0], b"{}").unwrap();
    assert!(cache.read_cached_result("selected", "input").is_err());
}

#[test]
fn json_cached_findings_required_named_record_and_vector_are_strict() {
    for input in [
        "null",
        "[]",
        "{}",
        r#"{"findings":null}"#,
        r#"{"findings":{}}"#,
        r#"{"findings":[false]}"#,
        r#"{"findings":[],"findings":[]}"#,
        r#"{"findings":[{"severity":"error","code":"c"}]}"#,
    ] {
        assert!(
            from_slice::<CachedResult>(input.as_bytes()).is_err(),
            "{input}"
        );
    }
    assert_eq!(to_vec(&CachedResult::default()), br#"{"findings":[]}"#);
    let input = br#"{"findings":[],"unknown":{"number":1e400}}"#;
    assert_eq!(
        from_slice::<CachedResult>(input).unwrap(),
        CachedResult::default()
    );
}

#[test]
fn json_cached_findings_authenticated_but_malformed_payload_is_not_an_empty_verdict() {
    let directory = tempfile::tempdir().unwrap();
    let cache = cache(directory.path(), "authored-producer");
    for bytes in [
        b"[]".as_slice(),
        br#"{"findings":null}"#,
        br#"{"findings":[],"findings":[]}"#,
    ] {
        // Separate selected inputs avoid conflicting immutable publications.
        // Admission must run through the actual read method's exact context.
        let selected = cache.context("selected", &bytes_digest(bytes));
        cache
            .store
            .publish(&selected, bytes_digest(bytes), (), bytes)
            .unwrap();
        assert!(
            cache
                .read_cached_result("selected", &bytes_digest(bytes))
                .is_err()
        );
    }
}
