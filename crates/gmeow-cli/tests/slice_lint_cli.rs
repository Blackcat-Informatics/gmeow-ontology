// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One production-surface proof for the shipped `slice quality`/`slice lint` twins.
//!
//! Detailed tier domination, malformed input, format selection, and rendering teeth are
//! pure unit contracts in `gmeow-slice-quality`. Repeating each through a fresh CLI
//! process decoded and scored the same embedded corpus twelve times. This test retains
//! the integration seam: both commands score one checkout-free foreign slice and expose
//! the same non-vacuous advisory assessment.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/external-slice")
}

fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dest dir");
    for entry in fs::read_dir(src).expect("read source dir") {
        let entry = entry.expect("dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}

fn staged_fixture() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let slice_dir = tmp.path().join("external-slice");
    copy_tree(&fixture_root(), &slice_dir);
    assert!(
        !slice_dir.components().any(|c| c.as_os_str() == "slices"),
        "the staged slice path must have no slices/ ancestor: {}",
        slice_dir.display()
    );
    (tmp, slice_dir)
}

/// Extract advisory identities shared by the quality JSON and lint SARIF surfaces.
fn shared_advisory_code(finding: &serde_json::Value) -> Option<&str> {
    finding
        .get("code")
        .or_else(|| finding.get("ruleId"))
        .and_then(|code| code.as_str())
        .filter(|code| {
            !matches!(
                *code,
                "slice-quality.grade"
                    | "slice-quality.rollup"
                    | "slice-quality.lint.below-min-tier"
            )
        })
}

#[test]
/// The installed CLI preserves one assessment and real source identities across formats.
fn slice_lint_and_slice_quality_share_one_checkout_free_assessment() {
    let (_tmp, slice_dir) = staged_fixture();
    let expected_manifest = slice_dir
        .join("manifest.ttl")
        .to_string_lossy()
        .replace('\\', "/");
    let expected_module = slice_dir
        .join("module.ttl")
        .to_string_lossy()
        .replace('\\', "/");

    let quality_out = gmeow()
        .args(["slice", "quality", "--format", "json"])
        .arg(&slice_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let quality_json: serde_json::Value =
        serde_json::from_slice(&quality_out).expect("quality stdout is parseable JSON");
    let quality_findings = quality_json
        .get("findings")
        .and_then(|findings| findings.as_array())
        .expect("quality report carries a findings array");

    let lint_out = gmeow()
        .args([
            "slice",
            "lint",
            "--format",
            "sarif",
            "--min-tier",
            "Registered",
        ])
        .arg(&slice_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let lint_json: serde_json::Value =
        serde_json::from_slice(&lint_out).expect("lint stdout is parseable SARIF JSON");
    let lint_findings = lint_json["runs"][0]["results"]
        .as_array()
        .expect("lint SARIF carries a results array");

    let quality_codes = quality_findings
        .iter()
        .filter_map(shared_advisory_code)
        .collect::<std::collections::BTreeSet<_>>();
    let lint_codes = lint_findings
        .iter()
        .filter_map(shared_advisory_code)
        .collect::<std::collections::BTreeSet<_>>();

    assert!(
        quality_codes.contains("slice-quality.doc-maturity.missing-dimension")
            && quality_codes.contains("slice-quality.advice-coverage.unharvested"),
        "the shared assessment must be non-vacuous: {quality_codes:?}"
    );
    assert_eq!(
        quality_codes, lint_codes,
        "lint and quality must expose the same underlying advisory assessment"
    );

    let mut source_uris = std::collections::BTreeSet::new();
    for result in lint_findings {
        let primary = &result["locations"][0];
        let uri = primary["physicalLocation"]["artifactLocation"]["uri"]
            .as_str()
            .expect("every lint result has a real physical source");
        assert!(
            uri == expected_manifest.as_str() || uri == expected_module.as_str(),
            "lint SARIF must name the exact real slice-owned file, got {uri}"
        );
        assert!(
            primary["physicalLocation"].get("region").is_none(),
            "file-level identity must not fabricate a line/column region: {primary:#?}"
        );
        assert!(
            primary
                .get("properties")
                .and_then(|properties| properties.get("gmeow.syntheticPhysicalLocation"))
                .is_none(),
            "a real slice source must not be marked synthetic: {primary:#?}"
        );
        assert!(!uri.contains("ontology/gmeow.ttl"));
        source_uris.insert(uri);
    }
    assert!(source_uris.contains(expected_manifest.as_str()));
    assert!(
        source_uris.contains(expected_module.as_str()),
        "term-owned findings should resolve to module.ttl when it exists: {source_uris:?}"
    );
}
