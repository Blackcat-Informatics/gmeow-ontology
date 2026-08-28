// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Rust twin of the retired `tests/test_feedback_bundle.py`.
//!
//! These integration tests cover the self-describing diagnostics feedback bundle:
//! the findings RDF as the snapshot graph plus SARIF and flat-JSON blobs, the
//! snapshot-content-id self-attestation, byte determinism, empty-report round-trip,
//! and verifier robustness on garbage / truncated input.

use gmeow_dev_cli::feedback_bundle::{
    META_SNAPSHOT_ID, REP_FINDINGS, REP_SARIF, build_feedback_bundle, read_report_blobs,
    verify_feedback_bundle,
};
use gmeow_errors::{Finding, Location, Report, Severity};

fn sample_report() -> Report {
    let mut report = Report::new("validate");
    let mut finding =
        Finding::new(Severity::Error, "shacl.MinCount", "missing property").with_tool("shacl");
    finding.add_location(Location::new(
        Some("core/ai/examples/grounded-claim.ttl".to_owned()),
        Some(12),
        Some(3),
        Some("gts:quad".to_owned()),
    ));
    report.add_finding(finding);
    report
}

#[test]
fn bundle_carries_sarif_and_findings_blobs() {
    let bundle = build_feedback_bundle(&sample_report()).expect("build feedback bundle");
    let mut graph = purrdf::gts::reader::read(&bundle, true, None);
    let blobs = read_report_blobs(&mut graph).expect("read report blobs");

    assert!(blobs.contains_key(REP_SARIF), "bundle carries SARIF blob");
    assert!(
        blobs.contains_key(REP_FINDINGS),
        "bundle carries findings blob"
    );

    let sarif: serde_json::Value =
        serde_json::from_slice(&blobs[REP_SARIF]).expect("SARIF parses as JSON");
    assert_eq!(sarif["version"], "2.1.0");

    let flat: serde_json::Value =
        serde_json::from_slice(&blobs[REP_FINDINGS]).expect("findings JSON parses");
    assert_eq!(
        flat["findings"].as_array().expect("findings array")[0]["code"],
        "shacl.MinCount"
    );
}

#[test]
fn bundle_self_attests() {
    let bundle = build_feedback_bundle(&sample_report()).expect("build feedback bundle");

    let mut graph = purrdf::gts::reader::read(&bundle, true, None);
    let blobs = read_report_blobs(&mut graph).expect("read report blobs");
    let flat: serde_json::Value =
        serde_json::from_slice(&blobs[REP_FINDINGS]).expect("findings JSON parses");

    assert!(
        flat["metadata"][META_SNAPSHOT_ID]
            .as_str()
            .expect("snapshot content id is a string")
            .starts_with("blake3:"),
        "metadata stamps a blake3 content id"
    );

    assert!(
        verify_feedback_bundle(&bundle),
        "bundle verifies against its own snapshot content id"
    );
}

#[test]
fn bundle_is_deterministic() {
    let a = build_feedback_bundle(&sample_report()).expect("build feedback bundle");
    let b = build_feedback_bundle(&sample_report()).expect("build feedback bundle again");
    assert_eq!(
        a, b,
        "two bundles built from the same report must be byte-identical"
    );
}

#[test]
fn empty_report_bundle_round_trips() {
    let bundle = build_feedback_bundle(&Report::new("validate")).expect("build empty bundle");
    assert!(
        verify_feedback_bundle(&bundle),
        "empty-report bundle self-attests"
    );
}

#[test]
fn verify_returns_false_on_garbage_bytes() {
    assert!(!verify_feedback_bundle(b""), "empty bytes do not verify");
    assert!(
        !verify_feedback_bundle(b"not a gts bundle at all"),
        "plain text does not verify"
    );
    assert!(
        !verify_feedback_bundle(&(0_u8..=255_u8).collect::<Vec<_>>()),
        "random bytes do not verify"
    );
}

#[test]
fn verify_returns_false_on_truncated_bundle() {
    let bundle = build_feedback_bundle(&sample_report()).expect("build feedback bundle");
    let truncated = &bundle[..bundle.len() / 2];
    assert!(
        !verify_feedback_bundle(truncated),
        "a truncated bundle does not verify"
    );
}

#[test]
fn verify_returns_false_when_snapshot_id_is_tampered() {
    let bundle = build_feedback_bundle(&sample_report()).expect("build feedback bundle");
    let mut graph = purrdf::gts::reader::read(&bundle, true, None);
    let blobs = read_report_blobs(&mut graph).expect("read report blobs");
    let mut flat: serde_json::Value =
        serde_json::from_slice(&blobs[REP_FINDINGS]).expect("findings JSON parses");

    // Corrupt the attested content id; the verifier must not accept it.
    if let Some(metadata) = flat["metadata"].as_object_mut() {
        metadata.insert(
            META_SNAPSHOT_ID.to_owned(),
            serde_json::Value::String(
                "blake3:0000000000000000000000000000000000000000000000000000000000000000"
                    .to_owned(),
            ),
        );
    }

    let snapshot_dataset =
        purrdf::gts::dataset_from_gts_graph(&graph).expect("fold graph back to dataset");
    let mut builder = purrdf::gts_compose::SnapshotBuilder::new();
    builder
        .add_dataset(&snapshot_dataset)
        .expect("re-add findings dataset");
    // gmeow-test-input: synthetic-only
    let snapshot_bytes = purrdf::gts_compose::emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        {
            let mut report_blobs = Vec::new();
            for (rep, bytes) in &blobs {
                let data = if rep == REP_FINDINGS {
                    serde_json::to_vec_pretty(&flat).expect("serialize mutated findings")
                } else {
                    bytes.clone()
                };
                let media_type = if rep == REP_SARIF {
                    "application/sarif+json"
                } else {
                    "application/json"
                };
                report_blobs.push(purrdf::gts_compose::BlobRow {
                    data,
                    media_type: media_type.to_owned(),
                    rep: rep.clone(),
                });
            }
            report_blobs
        },
        None,
        None,
        None,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
        &purrdf::gts_compose::MediumPlan::dist_default(None),
    )
    .expect("re-emit bundle");

    assert!(
        !verify_feedback_bundle(&snapshot_bytes),
        "a bundle with a forged snapshot content id does not verify"
    );
}

/// The feedback bundle is authored GMEOW GTS output, so EVERY payload frame it
/// carries — the snapshot frame AND the two small report blobs — must use the one
/// mandated transform (`zstd-rsyncable` @ level 12). The SARIF/findings blobs sit
/// well under the rsyncable threshold, which is exactly where a raw `emit_gts`
/// call silently falls back to plain `zstd`.
#[test]
fn bundle_uses_the_mandated_frame_profile() {
    let bundle = build_feedback_bundle(&sample_report()).expect("build feedback bundle");
    gmeow_gts_profile::validate_mandated_frames(&bundle)
        .expect("feedback bundle uses the mandated zstd-rsyncable-L12 frame profile");
    // …and the SECOND half: the branch the ontology routes this producer to. The
    // feedback bundle carries no medium registry of its own — which is exactly why the
    // universal rule above must stay registry-independent — so its declaration is read
    // from the slice that owns the producer→medium map.
    audit_declared_media(&bundle);
}

/// Hold a feedback bundle to the medium `gmeow:gtsProducerFeedbackBundle` declares,
/// and assert that declaration really routes to the whole-artifact branch.
fn audit_declared_media(bundle: &[u8]) {
    use gmeow_pipeline::medium::registry::{MediumRegistry, MediumSourceKind};

    const PRODUCER: &str = "https://blackcatinformatics.ca/gmeow/gtsProducerFeedbackBundle";
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let text = std::fs::read(root.join("slices/core/gts/module.ttl")).expect("the gts slice reads");
    let ds = purrdf::parse_dataset(
        &text,
        "text/turtle",
        Some("https://blackcatinformatics.ca/gmeow/"),
    )
    .expect("the gts slice parses");
    let registry = MediumRegistry::from_dataset(&ds).expect("the live medium axis reads");
    let medium_iri = gmeow_pipeline::declared_medium_of(&ds, PRODUCER)
        .expect("the feedback producer is declared");
    assert_eq!(
        registry
            .media()
            .get(&medium_iri)
            .expect("a declared gmeow:Medium")
            .source_kind,
        MediumSourceKind::WholeArtifact,
        "the feedback bundle must route to the whole-artifact branch"
    );
    gmeow_pipeline::validate_declared_media(
        bundle,
        &gmeow_pipeline::MediumDeclaration {
            medium: &medium_iri,
            registry: &registry,
        },
    )
    .expect("the feedback bundle satisfies its declared whole-artifact medium");
}

/// The same audit over the EMPTY report: an empty findings graph still emits a
/// snapshot frame plus both (tiny) blob frames, so the no-size-threshold rule
/// binds there too.
#[test]
fn empty_bundle_uses_the_mandated_frame_profile() {
    let bundle = build_feedback_bundle(&Report::new("validate")).expect("build empty bundle");
    gmeow_gts_profile::validate_mandated_frames(&bundle)
        .expect("empty feedback bundle uses the mandated zstd-rsyncable-L12 frame profile");
    audit_declared_media(&bundle);
}
