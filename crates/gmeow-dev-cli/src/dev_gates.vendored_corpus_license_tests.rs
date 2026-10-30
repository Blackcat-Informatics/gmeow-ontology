// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::vendored_corpus_license_findings;

/// A fresh temp directory owned by the returned [`tempfile::TempDir`].
///
/// Bind the guard to a live local (`let (_tmp, root) = tempdir("slug");`): when it
/// drops the directory and everything written under it is removed, on success, on
/// early return, and on panic alike.
fn tempdir(slug: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("gmeow-dev-cli-vendored-license-{slug}-"))
        .tempdir()
        .expect("create temp dir");
    let path = tmp.path().to_path_buf();
    (tmp, path)
}

fn write_descriptor(root: &std::path::Path, crate_name: &str, corpus_name: &str, json: &str) {
    let dir = root
        .join("crates")
        .join(crate_name)
        .join("tests")
        .join("vendored")
        .join(corpus_name);
    std::fs::create_dir_all(&dir).expect("mkdir vendored corpus dir");
    std::fs::write(dir.join("corpus.json"), json).expect("write corpus.json");
}

/// Positive: the real EWT descriptor (ring-fenced, attributed CC-BY-SA-4.0) shipped at
/// `crates/lang-bridge/tests/vendored/ud-english-ewt/corpus.json` is IMPORT_OK — the gate
/// yields NO findings against the live tree.
#[test]
fn real_ewt_descriptor_is_import_ok_no_findings() {
    let root = crate::dev_common::project_root();
    let descriptor = root
        .join("crates")
        .join("lang-bridge")
        .join("tests")
        .join("vendored")
        .join("ud-english-ewt")
        .join("corpus.json");
    assert!(
        descriptor.is_file(),
        "expected the real EWT descriptor to exist at {}",
        descriptor.display()
    );
    let findings = vendored_corpus_license_findings(&root);
    assert!(
        findings.is_empty(),
        "expected no vendored-corpus-license findings against the live tree, got: {findings:?}"
    );
}

/// Negative: a CC-BY-SA-4.0 descriptor that is NOT ring-fenced fails the classifier's
/// share-alike vendoring exception, so the gate must fold exactly one Error finding — the
/// proof that this actually hard-fails in production, not just in the classifier's own
/// unit tests.
#[test]
fn unfenced_cc_by_sa_descriptor_yields_one_error_finding() {
    let (_tmp, root) = tempdir("unfenced");
    write_descriptor(
        &root,
        "some-crate",
        "bad-corpus",
        r#"{
                "name": "bad-corpus",
                "treebank": "Bad_Treebank",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "version_or_commit": "main",
                "fetch_date": "2026-07-11",
                "attribution": "Some credited authors",
                "ring_fenced": false,
                "sent_ids": []
            }"#,
    );
    let findings = vendored_corpus_license_findings(&root);
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one finding for the unfenced CC-BY-SA descriptor: {findings:?}"
    );
    assert_eq!(findings[0].code, "vendored-corpus-license-violation");
}

/// Negative: a CC-BY-SA-4.0 descriptor with empty attribution likewise fails the exception
/// and is folded as an Error finding.
#[test]
fn unattributed_cc_by_sa_descriptor_yields_one_error_finding() {
    let (_tmp, root) = tempdir("unattributed");
    write_descriptor(
        &root,
        "some-crate",
        "bad-corpus",
        r#"{
                "name": "bad-corpus",
                "treebank": "Bad_Treebank",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "version_or_commit": "main",
                "fetch_date": "2026-07-11",
                "attribution": "   ",
                "ring_fenced": true,
                "sent_ids": []
            }"#,
    );
    let findings = vendored_corpus_license_findings(&root);
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one finding for the unattributed CC-BY-SA descriptor: {findings:?}"
    );
    assert_eq!(findings[0].code, "vendored-corpus-license-violation");
}

/// A descriptor missing a required field (`ring_fenced`) is itself a HARD FAIL — no
/// optionality, no silent skip.
#[test]
fn descriptor_missing_required_field_is_hard_fail() {
    let (_tmp, root) = tempdir("missing-field");
    write_descriptor(
        &root,
        "some-crate",
        "bad-corpus",
        r#"{
                "name": "bad-corpus",
                "spdx_license": "CC-BY-SA-4.0",
                "source_url": "https://example.org/treebank.conllu",
                "attribution": "Some credited authors"
            }"#,
    );
    let findings = vendored_corpus_license_findings(&root);
    assert_eq!(
        findings.len(),
        1,
        "expected exactly one finding for the descriptor missing ring_fenced: {findings:?}"
    );
    assert_eq!(findings[0].code, "vendored-corpus-license-invalid");
    let path = root.join("crates/some-crate/tests/vendored/bad-corpus/corpus.json");
    let accepted = r#"{
        "spdx_license":"first", "spdx_license":"MIT",
        "source_url":"first", "source_url":"https://example.test/é中😀",
        "attribution":"first", "attribution":"credited é中😀\n\"\\",
        "ring_fenced":false, "ring_fenced":true,
        "unowned_metadata":{"retained":true}
    }"#;
    std::fs::write(&path, accepted).unwrap();
    let descriptor = super::load_vendored_corpus_descriptor(&path).unwrap();
    assert_eq!(descriptor.spdx_license, "MIT");
    assert_eq!(descriptor.source_url, "https://example.test/é中😀");
    assert_eq!(descriptor.attribution, "credited é中😀\n\"\\");
    assert!(descriptor.ring_fenced);
    assert!(vendored_corpus_license_findings(&root).is_empty());

    for (bytes, field) in [
        (
            r#"{"spdx_license":null,"source_url":"source","attribution":"credit","ring_fenced":true}"#,
            "spdx_license",
        ),
        (
            r#"{"spdx_license":"MIT","source_url":false,"attribution":"credit","ring_fenced":true}"#,
            "source_url",
        ),
        (
            r#"{"spdx_license":"MIT","source_url":"source","attribution":1,"ring_fenced":true}"#,
            "attribution",
        ),
        (
            r#"{"spdx_license":"MIT","source_url":"source","attribution":"credit","ring_fenced":"true"}"#,
            "ring_fenced",
        ),
        (
            r#"{"spdx_license":"MIT","source_url":"source","attribution":"credit","ring_fenced":null}"#,
            "ring_fenced",
        ),
    ] {
        std::fs::write(&path, bytes).unwrap();
        let error = super::load_vendored_corpus_descriptor(&path)
            .err()
            .unwrap()
            .to_string();
        assert!(
            error.contains("corpus.json")
                && error.contains(&format!("missing required field {field:?}")),
            "{error}"
        );
        let findings = vendored_corpus_license_findings(&root);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "vendored-corpus-license-invalid");
    }
    std::fs::write(&path, "not JSON").unwrap();
    let error = super::load_vendored_corpus_descriptor(&path)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("corpus.json") && error.contains("invalid JSON"),
        "{error}"
    );
    let absent = path.with_file_name("absent.json");
    let error = super::load_vendored_corpus_descriptor(&absent)
        .err()
        .unwrap()
        .to_string();
    assert!(
        error.contains("absent.json") && error.contains("cannot read corpus.json"),
        "{error}"
    );
}

/// Preserve the complete selected audit packet without an ontology or producer.
#[test]
fn box_roles_json_preserves_authored_audit_packet() {
    use gmeow_validate::box_roles::{BoxRoleAudit, RoleFinding};
    let finding = |term: &str| RoleFinding {
        term: term.into(),
        kind: "class".into(),
        source: "authored.ttl".into(),
        message: "authored".into(),
    };
    let audit = BoxRoleAudit {
        term_count: usize::MAX,
        role_counts: std::collections::BTreeMap::from([
            ("z-role".into(), 0),
            ("a-role é中😀".into(), usize::MAX),
        ]),
        missing: vec![finding("urn:missing:z"), finding("urn:missing:a")],
        invalid: vec![finding("urn:invalid:é中😀\n\"\\")],
    };
    let bytes = super::box_roles_json(&audit).unwrap().into_bytes();
    let reference = include_str!("dev_gates/fixtures/box-roles-json-v1.txt");
    assert_eq!(reference.lines().count(), 1);
    let mut fields = reference.split_whitespace();
    assert_eq!(fields.next(), Some("box-roles"));
    let digest = fields.next().unwrap();
    let original = purrdf_hash::hex::decode(fields.next().unwrap()).unwrap();
    assert!(fields.next().is_none());
    assert_eq!(purrdf::ContentDigest::of(&bytes).to_hex(), digest);
    assert_eq!(bytes, original);
    assert!(bytes.ends_with(b"\n"));
    assert!(!bytes.ends_with(b"\n\n"));
}
