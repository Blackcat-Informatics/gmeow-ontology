// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow verify --deep` acceptance: plain `verify` never reasons (gated fix
//! for the ~280s unconditional deep pass over the shipped bundle — see
//! `bundle_smoke.rs::verify_allow_unsigned_needs_no_external_gts_on_path`),
//! but `--deep` runs the SAME Tier-2 native semantic pass `gmeow validate
//! --deep` runs and folds a real `validate.deep.*` reasoned-quad verdict into
//! the unified report, carrying its explain-skeleton derivation citation.
//!
//! The fixture is a TINY entailed-inconsistency TBox (mirrors
//! `crates/validate/src/validate_all.rs`'s Task-5 `INCONSISTENT_TTL` /
//! `gts_bytes_from_turtle` test helpers, adapted here for a CLI-binary test):
//! `ex:A rdfs:subClassOf ex:B, ex:C` with `ex:B owl:disjointWith ex:C` and
//! `ex:x rdf:type ex:A` forces `ex:x` into `owl:Nothing` — a real DL clash the
//! native reasoner must find, keeping this test's bundle deliberately small so
//! it runs in well under a second, not the ~280s the full shipped bundle takes.

use std::path::PathBuf;
use std::process::Output;

use assert_cmd::Command;

/// The inconsistent TBox: `x : A`, `A ⊑ B`, `A ⊑ C`, `B ⊐⊏ C` forces `x` into
/// `owl:Nothing` — mirrors `crates/validate/src/validate_all.rs`'s
/// `INCONSISTENT_TTL` fixture.
const INCONSISTENT_TTL: &str = "\
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix ex: <http://gmeow.example/> .
ex:A rdfs:subClassOf ex:B .
ex:A rdfs:subClassOf ex:C .
ex:B owl:disjointWith ex:C .
ex:x rdf:type ex:A .
";

/// Build canonical GTS bytes from a Turtle string — mirrors
/// `validate_all.rs::gts_bytes_from_turtle` (private to that crate, so
/// replicated here against only public `purrdf` APIs).
fn gts_bytes_from_turtle(ttl: &str) -> Vec<u8> {
    let dataset =
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse test turtle");
    // gmeow-test-input: synthetic-only
    purrdf::gts_write::to_gts(
        &dataset,
        &purrdf::RdfLookaside::default(),
        "gmeow-cli-verify-deep-test",
    )
    .expect("encode GTS bytes")
}

fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Write the bundle to a temp file and run `gmeow verify --allow-unsigned
/// [--deep] --format <format>` over it, returning the process output (the
/// caller keeps the `TempDir` alive).
fn run_verify(bytes: &[u8], format: &str, deep: bool) -> (tempfile::TempDir, PathBuf, Output) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let bundle_path = dir.path().join("inconsistent.gts");
    std::fs::write(&bundle_path, bytes).expect("write fixture bundle");
    let mut cmd = gmeow();
    cmd.args(["verify", "--allow-unsigned", "--format", format]);
    if deep {
        cmd.arg("--deep");
    }
    let output = cmd.arg(&bundle_path).output().expect("run gmeow verify");
    (dir, bundle_path, output)
}

/// Plain `verify` (no `--deep`) never reasons: the inconsistent TBox above
/// would fail a deep pass, but without `--deep` verify must NOT surface the
/// reasoned `validate.deep.*` finding and must pass (no signature/integrity/
/// ontology-completeness finding is at stake in this fixture either).
#[test]
fn verify_without_deep_does_not_reason_over_an_inconsistent_bundle() {
    let (_dir, _path, output) =
        run_verify(&gts_bytes_from_turtle(INCONSISTENT_TTL), "human", false);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("validate.deep"),
        "plain verify must not run the reasoning pass at all: stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        output.status.success(),
        "plain verify must pass since it never reasons over this fixture: \
         stdout={stdout}\nstderr={stderr}"
    );
}

/// `verify --deep --format human` over the tiny inconsistent bundle: the
/// reasoned `validate.deep.inconsistent` finding renders with its
/// explain-skeleton derivation citation ("derived from: ") and the run fails
/// (a Belnap-both / inconsistency is a hard-fail Error).
#[test]
fn verify_deep_human_renders_inconsistency_with_derivation() {
    let (_dir, _path, output) = run_verify(&gts_bytes_from_turtle(INCONSISTENT_TTL), "human", true);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "an entailed inconsistency under --deep must fail verify: \
         stdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stdout.contains("validate.deep.inconsistent"),
        "the reasoned inconsistency finding must render: stdout={stdout}"
    );
    assert!(
        stdout.contains("derived from: "),
        "the explain-skeleton derivation citation must render on --deep: stdout={stdout}"
    );
}

/// `verify --deep --format sarif` over the same fixture: the SARIF JSON is
/// valid and carries the same reasoned finding + derivation citation.
#[test]
fn verify_deep_sarif_carries_derivation() {
    let (_dir, _path, output) = run_verify(&gts_bytes_from_turtle(INCONSISTENT_TTL), "sarif", true);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "an entailed inconsistency under --deep must fail verify (sarif channel): stdout={stdout}"
    );
    let doc: serde_json::Value =
        serde_json::from_str(&stdout).expect("SARIF output must be valid JSON");
    assert!(doc.is_object(), "SARIF is a JSON object: {stdout}");
    assert!(
        stdout.contains("validate.deep.inconsistent"),
        "SARIF names the reasoned inconsistency finding: {stdout}"
    );
    assert!(
        stdout.contains("gmeow.derivedFromQuad"),
        "SARIF must carry the explain-skeleton derivation citation under the \
         pinned gmeow.derivedFromQuad property key: {stdout}"
    );
}
