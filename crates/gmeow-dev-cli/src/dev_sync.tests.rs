// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_action_cache::bytes_digest;

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
    let path = manifest_path(
        Path::new("/tmp/repo"),
        SyncOutput::All,
        SyncProfile::Ordinary,
        "en,../../fr",
    );
    assert!(path.ends_with("all-ordinary-en_______fr.json"));
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
        profile: SyncProfile::Ordinary.as_str().to_string(),
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
        SyncProfile::Ordinary,
        "default",
        "same",
    ));
    assert!(manifest_is_current(
        Path::new("/does/not/matter"),
        &manifest,
        SyncMode::Check,
        SyncOutput::Generated,
        SyncProfile::Ordinary,
        "default",
        "same",
    ));
    assert!(!manifest_is_current(
        Path::new("/does/not/matter"),
        &manifest,
        SyncMode::Check,
        SyncOutput::Generated,
        SyncProfile::Release,
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
        sha256: bytes_digest(b"one"),
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
    changed_digest.sha256 = bytes_digest(b"two");
    assert_ne!(managed_output_root(&[changed_digest]), root);
}

/// A stale metadata witness may match the file by digest but must reject changed bytes.
#[test]
fn sync_manifest_rechecks_managed_content_after_metadata_changes() {
    let directory = tempfile::tempdir().expect("create isolated sync output");
    let root = directory.path();
    let relative = "generated/example";
    let path = root.join(relative);
    std::fs::create_dir(root.join("generated")).expect("create managed output directory");
    let mut bytes = vec![b'a'; 128 * 1024];
    bytes.push(b'b');
    std::fs::write(&path, &bytes).expect("write managed output");
    let managed_roots = vec!["generated".to_owned()];
    let mut files = capture_outputs(root, &[], &managed_roots).expect("capture output witness");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, relative);
    assert_eq!(files[0].len, bytes.len() as u64);
    assert_eq!(files[0].sha256, bytes_digest(&bytes));
    files[0].modified_ns = u128::MAX;
    let manifest = SyncManifest {
        version: MANIFEST_VERSION,
        build_fingerprint: BUILD_FINGERPRINT.to_owned(),
        build_identity: BuildIdentity::current(),
        input_digest: "same".to_owned(),
        output: SyncOutput::Generated.as_str().to_owned(),
        profile: SyncProfile::Ordinary.as_str().to_owned(),
        language: "default".to_owned(),
        strict_checked: true,
        materialized: true,
        docs_rendered: false,
        managed_roots,
        managed_output_root: managed_output_root(&files),
        files,
        stage_receipt_root: "0".repeat(64),
    };
    let is_current = || {
        manifest_is_current(
            root,
            &manifest,
            SyncMode::Update,
            SyncOutput::Generated,
            SyncProfile::Ordinary,
            "default",
            "same",
        )
    };
    assert!(is_current());
    *bytes.last_mut().expect("output has a final byte") = b'c';
    std::fs::write(&path, bytes).expect("change final byte without changing size");
    assert!(!is_current());
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

/// The timings JSON is where a finished hosted run is read from: each memory record
/// carries the allocator's live and free totals, and a return of freed pages carries the
/// resident size before it beside the one after. A figure the host could not supply is
/// written as `null`, never as zero. A publication record carries its product's
/// footprint, the dataset's payload bytes among it.
#[test]
fn stage_memory_observation_carries_allocator_totals_and_the_reclaim_figures() {
    use gmeow_pipeline::memory_record::{
        AllocatorMemory, MemoryEvent, MemoryRecord, ProcessMemory, ProductFootprint,
        ReclaimedMemory, ReleasedFootprint,
    };
    use purrdf_lex::json::Value;
    let returned = MemoryRecord {
        pid: 7,
        seq: 3,
        stage_id: "stage-mappings".to_owned(),
        event: MemoryEvent::Reclaim,
        phase: None,
        process: ProcessMemory {
            rss_kib: Some(600),
            peak_rss_kib: Some(1_300),
        },
        allocator: Some(AllocatorMemory {
            live_bytes: 500,
            free_bytes: 90,
        }),
        product: None,
        released: None,
        reclaim: Some(ReclaimedMemory {
            rss_before_kib: Some(1_300),
            elapsed_us: 42,
        }),
    };
    let written = stage_memory_observation(&returned);
    assert_eq!(written["event"], Value::from("reclaim"));
    assert_eq!(written["rss_kib"], Value::from(600_u64));
    assert_eq!(written["allocator_live_bytes"], Value::from(500_u64));
    assert_eq!(written["allocator_free_bytes"], Value::from(90_u64));
    assert_eq!(written["reclaim"]["rss_before_kib"], Value::from(1_300_u64));
    assert_eq!(written["reclaim"]["elapsed_us"], Value::from(42_u64));

    let published = MemoryRecord {
        event: MemoryEvent::Complete,
        product: Some(ProductFootprint {
            quads: 11,
            terms: 7,
            dataset_payload_bytes: 4_096,
            committed_lane_bytes: 30,
            internal_lane_bytes: 20,
            carrier_released: false,
        }),
        reclaim: None,
        ..returned.clone()
    };
    let written = stage_memory_observation(&published);
    assert_eq!(written["product"]["quads"], Value::from(11_u64));
    assert_eq!(
        written["product"]["dataset_payload_bytes"],
        Value::from(4_096_u64)
    );

    let detached = MemoryRecord {
        event: MemoryEvent::ReleaseRepBlobs,
        released: Some(ReleasedFootprint {
            representation_records: 2,
            representation_payload_bytes: 100,
            ..ReleasedFootprint::default()
        }),
        reclaim: None,
        ..returned.clone()
    };
    let written = stage_memory_observation(&detached);
    assert_eq!(written["event"], Value::from("release-blob-reps"));
    assert_eq!(
        written["released"]["representation_records"],
        Value::from(2_u64)
    );
    assert_eq!(
        written["released"]["representation_payload_bytes"],
        Value::from(100_u64)
    );

    let unsupported = MemoryRecord {
        event: MemoryEvent::Dispatch,
        allocator: None,
        reclaim: None,
        ..returned
    };
    let written = stage_memory_observation(&unsupported);
    assert_eq!(written["allocator_live_bytes"], Value::from(None::<u64>));
    assert_eq!(written["allocator_free_bytes"], Value::from(None::<u64>));
    assert_eq!(written["reclaim"], Value::from(None::<u64>));
}
