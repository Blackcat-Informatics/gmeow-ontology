// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Preserve complete metadata lines without invoking a generator or synchronizing.
#[test]
fn generator_metadata_preserves_complete_authored_lines() {
    let cases = [
        (
            "populated",
            gmeow_pipeline::GeneratorMetadata {
                name: "authored é中😀\n\"\\".into(),
                sources: vec!["z.ttl".into(), "a.ttl".into(), "z.ttl".into()],
                outputs: vec!["generated/z".into(), "generated/a".into()],
                dependencies: vec!["second".into(), "first".into(), "second".into()],
                source_hash: "authored-source-identity".into(),
            },
        ),
        (
            "empty",
            gmeow_pipeline::GeneratorMetadata {
                name: "empty".into(),
                sources: vec![],
                outputs: vec![],
                dependencies: vec![],
                source_hash: String::new(),
            },
        ),
    ];
    let reference = include_str!("dev_sync/fixtures/generator-metadata-json-v1.txt");
    assert_eq!(reference.lines().count(), cases.len());
    for ((name, record), row) in cases.into_iter().zip(reference.lines()) {
        let bytes = format!("{}\n", generator_metadata_json(&record).unwrap()).into_bytes();
        let mut fields = row.split_whitespace();
        assert_eq!(fields.next(), Some(name));
        let digest = fields.next().unwrap();
        let original = purrdf_hash::hex::decode(fields.next().unwrap()).unwrap();
        assert!(fields.next().is_none());
        assert_eq!(purrdf::ContentDigest::of(&bytes).to_hex(), digest, "{name}");
        assert_eq!(bytes, original, "{name}");
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

#[test]
/// The authored abstract participates in sync invalidation without widening to harness code.
fn generated_sync_inputs_exclude_unrelated_test_harness_implementation() {
    // This is a declaration audit only: it binds the DAG and enumerates paths. It
    // never starts a stage, generator, corpus build, or fixture producer.
    let root = repo_root();
    let files = declared_sync_input_files(&root, SyncOutput::Generated)
        .expect("enumerate generated sync input closure");
    let relative = files
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap_or(path))
        .collect::<BTreeSet<_>>();

    assert!(relative.contains(Path::new("ontology/gmeow.ttl")));
    assert!(
        relative.contains(Path::new("metadata/gmeow-abstract.txt")),
        "the authored abstract must invalidate the whole-run manifest before source loading"
    );
    assert!(
        relative.contains(Path::new("tests/fixtures/coverage/external/bii.ttl")),
        "a product-bearing fixture declared by the mappings stage remains an input"
    );
    for unrelated in [
        ".pre-commit-config.yaml",
        "crates/gmeow-dev-cli/tests/make_gate_contract.rs",
        "crates/slicetest/src/repository.rs",
    ] {
        assert!(
            !relative.contains(Path::new(unrelated)),
            "unrelated test/pre-commit implementation must not rebuild the corpus: {unrelated}"
        );
    }
}

#[test]
fn manifest_path_sanitizes_language() {
    let path = manifest_path(Path::new("/tmp/repo"), SyncOutput::All, "en,../../fr");
    assert!(path.ends_with("all-en_______fr.json"));
}

#[test]
fn false_ci_values_choose_update() {
    for value in ["", "0", "false", "off", "no"] {
        assert!(matches!(value, "" | "0" | "false" | "off" | "no"));
    }
}

#[test]
fn read_only_manifest_cannot_satisfy_update() {
    let manifest = SyncManifest {
        version: MANIFEST_VERSION,
        build_fingerprint: BUILD_FINGERPRINT.to_string(),
        build_identity: BuildIdentity::current(),
        input_digest: "same".to_string(),
        output: SyncOutput::Generated.as_str().to_string(),
        language: "default".to_string(),
        strict_checked: true,
        materialized: false,
        docs_rendered: false,
        managed_roots: Vec::new(),
        files: Vec::new(),
        managed_output_root: managed_output_root(&[]),
        stage_receipt_root: "0".repeat(64),
    };
    assert!(!manifest_is_current(
        Path::new("/does/not/matter"),
        &manifest,
        SyncMode::Update,
        SyncOutput::Generated,
        "default",
        "same",
    ));
    assert!(manifest_is_current(
        Path::new("/does/not/matter"),
        &manifest,
        SyncMode::Check,
        SyncOutput::Generated,
        "default",
        "same",
    ));
}

#[test]
fn managed_output_root_binds_content_but_not_observational_mtime() {
    let witness = FileWitness {
        path: "generated/example".to_string(),
        len: 3,
        modified_ns: 1,
        sha256: sha256(b"one"),
    };
    let root = managed_output_root(std::slice::from_ref(&witness));
    let mut changed_mtime = witness.clone();
    changed_mtime.modified_ns = 99;
    assert_eq!(
        managed_output_root(&[changed_mtime]),
        root,
        "mtime is run telemetry, never immutable output identity"
    );
    let mut changed_digest = witness;
    changed_digest.sha256 = sha256(b"two");
    assert_ne!(managed_output_root(&[changed_digest]), root);
}

#[test]
fn generated_and_docs_selection_exclude_unrequested_runtime_outputs() {
    let paths = [
        "generated/module-status.md".to_string(),
        "dist/gmeow-okf/index.md".to_string(),
    ];
    for output in [SyncOutput::Generated, SyncOutput::Docs] {
        let selected = paths
            .iter()
            .filter(|path| pipeline_output_selected(output, path))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(selected, vec!["generated/module-status.md"]);
    }
    assert_eq!(
        paths
            .iter()
            .filter(|path| pipeline_output_selected(SyncOutput::All, path))
            .count(),
        2
    );
}
