// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface acceptance of the enriched `gmeow verify` path:
//! renders the SAME proof-carrying enrichment as `gmeow validate` — remediation
//! ("how to fix") + per-term usage guidance — on ALL THREE channels
//! (text / SARIF / JSON), and its ontology-completeness findings are non-blocking
//! Warnings (the severity pin: a bundle that would pass verify today keeps exit 0).
//!
//! The fixture is a minimal, unsigned GTS bundle carrying one `gmeow:`-namespaced
//! `owl:Class` that is LABELED but carries NO `skos:definition` (so verify emits a
//! real `ontology.missing-definition` finding) and authors `gmeow:howToUse` guidance
//! on the same term (so the documented-term guidance join lights up on
//! verify). Driving the BUILT binary proves the whole command end-to-end, not just a
//! library seam.

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;
use purrdf::gts::model::{Term, TermKind};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";
const GMEOW_HOW_TO_USE: &str = "https://blackcatinformatics.ca/gmeow/howToUse";
const TEST_CLASS: &str = "https://blackcatinformatics.ca/gmeow/VerifyEnrichmentTestClass";

/// The authored guidance prose — must survive to every channel through the
/// documented-term guidance join.
const GUIDANCE_PROSE: &str = "Use this class to exercise verify enrichment.";
/// The rule-catalogue remediation prose for `ontology.missing-definition` (a stable
/// substring of `crates/validate/src/rule_catalog.rs`'s authored fix).
const REMEDIATION_PROSE: &str = "Add a skos:definition to the term";

fn iri(value: &str) -> Term {
    Term {
        kind: TermKind::Iri,
        value: Some(value.to_string()),
        datatype: None,
        lang: None,
        direction: None,
        reifier: None,
    }
}

fn en_literal(text: &str) -> Term {
    Term {
        kind: TermKind::Literal,
        value: Some(text.to_string()),
        datatype: None,
        lang: Some("en".to_string()),
        direction: None,
        reifier: None,
    }
}

/// A minimal unsigned GTS bundle carrying one labeled `gmeow:` class. When
/// `with_definition` is false the class carries NO `skos:definition` (verify emits
/// `ontology.missing-definition`); it always authors `gmeow:howToUse` guidance.
fn test_bundle(with_definition: bool) -> Vec<u8> {
    let mut writer = purrdf::gts::writer::Writer::new("gmeow-cli-verify-enrichment-test");
    writer.add_terms(&[
        iri(TEST_CLASS),                            // 0
        iri(RDF_TYPE),                              // 1
        iri(OWL_CLASS),                             // 2
        iri(RDFS_LABEL),                            // 3
        en_literal("Verify Enrichment Test Class"), // 4
        iri(GMEOW_HOW_TO_USE),                      // 5
        en_literal(GUIDANCE_PROSE),                 // 6
        iri(SKOS_DEFINITION),                       // 7
        en_literal("A synthetic definition."),      // 8
    ]);
    let mut quads: Vec<(usize, usize, usize, Option<usize>)> =
        vec![(0, 1, 2, None), (0, 3, 4, None), (0, 5, 6, None)];
    if with_definition {
        quads.push((0, 7, 8, None));
    }
    writer.add_quads(&quads);
    writer.to_bytes()
}

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Write the bundle to a temp file and run `gmeow verify --allow-unsigned
/// --format <format>` over it, returning the process output (the caller keeps the
/// `TempDir` alive).
fn run_verify(bytes: &[u8], format: &str) -> (tempfile::TempDir, PathBuf, Output) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let bundle_path = dir.path().join("fixture.gts");
    std::fs::write(&bundle_path, bytes).expect("write fixture bundle");
    let output = gmeow()
        .args(["verify", "--allow-unsigned", "--format", format])
        .arg(&bundle_path)
        .output()
        .expect("run gmeow verify");
    (dir, bundle_path, output)
}

/// Text channel: the ontology finding renders with its "how to fix" remediation AND
/// its per-term usage guidance line — the same enrichment `gmeow validate` shows.
#[test]
fn verify_text_shows_remediation_and_guidance() {
    let (_dir, _path, output) = run_verify(&test_bundle(false), "human");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a missing-definition Warning is non-blocking (severity pin): stdout={stdout}\nstderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("ontology.missing-definition"),
        "the ontology finding must be emitted: {stdout}"
    );
    assert!(
        stdout.contains("how to fix") && stdout.contains(REMEDIATION_PROSE),
        "the remediation 'how to fix' line must render: {stdout}"
    );
    assert!(
        stdout.contains(GUIDANCE_PROSE),
        "the per-term usage guidance must render on verify: {stdout}"
    );
}

/// SARIF channel: the ontology finding carries `fixes` (remediation) and the
/// pinned `gmeow.howToUse` guidance property, and the whole document is valid JSON.
#[test]
fn verify_sarif_carries_fixes_and_guidance() {
    let (_dir, _path, output) = run_verify(&test_bundle(false), "sarif");
    assert!(
        output.status.success(),
        "verify --format sarif must succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: serde_json::Value =
        serde_json::from_str(&stdout).expect("SARIF output must be valid JSON");
    assert!(doc.is_object(), "SARIF is a JSON object: {stdout}");
    assert!(
        stdout.contains("ontology.missing-definition"),
        "SARIF names the ontology finding: {stdout}"
    );
    assert!(
        stdout.contains("\"fixes\"") && stdout.contains(REMEDIATION_PROSE),
        "SARIF must carry the remediation as a `fixes` entry: {stdout}"
    );
    assert!(
        stdout.contains("gmeow.howToUse") && stdout.contains(GUIDANCE_PROSE),
        "SARIF must carry the per-term guidance under gmeow.howToUse: {stdout}"
    );
}

/// JSON channel: the serialized report carries the same remediation + guidance
/// prose, and the whole document is valid JSON.
#[test]
fn verify_json_carries_remediation_and_guidance() {
    let (_dir, _path, output) = run_verify(&test_bundle(false), "json");
    assert!(output.status.success(), "verify --format json must succeed");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let _doc: serde_json::Value =
        serde_json::from_str(&stdout).expect("JSON output must be valid JSON");
    assert!(
        stdout.contains("ontology.missing-definition"),
        "JSON names the ontology finding: {stdout}"
    );
    assert!(
        stdout.contains(REMEDIATION_PROSE),
        "JSON must carry the remediation prose: {stdout}"
    );
    assert!(
        stdout.contains(GUIDANCE_PROSE),
        "JSON must carry the per-term guidance prose: {stdout}"
    );
}

/// Severity pin: a fully-clean bundle (labeled AND defined) emits no ontology
/// finding and exits 0 — verify's exit contract is unchanged for a clean bundle.
#[test]
fn verify_clean_bundle_exit_code_unchanged() {
    let (_dir, _path, output) = run_verify(&test_bundle(true), "human");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a clean bundle must verify with exit 0: stdout={stdout}\nstderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !stdout.contains("ontology.missing-definition")
            && !stdout.contains("ontology.missing-label"),
        "a clean (labeled + defined) bundle emits no ontology-completeness finding: {stdout}"
    );
    assert!(
        stdout.contains("verification passed"),
        "the human channel reports overall success: {stdout}"
    );
}
