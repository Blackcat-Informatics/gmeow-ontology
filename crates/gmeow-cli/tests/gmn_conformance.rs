// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Production-surface proof that the SHIPPED `gmeow` binary exposes the GMN-1
//! conformance surface — the real `Cli`/`Commands::Gmn` clap dispatch in
//! `src/lib.rs`, driven through `assert_cmd` exactly like the other CLI tests.
//!
//! Before this surface landed, the codec's digest / codec / witness / pack layer was
//! reachable only from `crates/pipeline`'s production gates;
//! `gmeow gmn verify` returned `unrecognized subcommand`. This drives the built
//! binary over the committed frozen vector corpus and asserts:
//!
//! * `gmn verify` exits 0 over the real corpus (byte-frozen + per-claim + pack-root),
//! * `gmn digest`/`encode`/`decode` produce stable output on a small fixture, and
//! * `gmn verify` exits NON-ZERO when pointed at a deliberately corrupted vectors dir
//!   AND when pointed at a tampered pack root — the no-optionality hard-fail contract.

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// The repo root (this crate lives at `crates/gmeow-cli`). Absolute so the test is
/// insensitive to the process CWD `cargo`/`nextest` chooses.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

fn lang_module() -> PathBuf {
    repo_root().join("slices/grounding/lang/module.ttl")
}

fn vectors_dir() -> PathBuf {
    repo_root().join("slices/grounding/lang/tests/gmn1-vectors")
}

fn grammar() -> PathBuf {
    repo_root().join("slices/grounding/lang/grammars/gmn.ebnf")
}

/// The ring-tagged consume-path demonstrator (four claims at four security rings).
fn ring_consume_ttl() -> PathBuf {
    repo_root().join("slices/grounding/lang/examples/gmn-ring-consume.ttl")
}

/// A committed positive vector whose `.gmn` decodes standalone (all-IRI claim, no
/// out-of-band `r_<hash>` by-reference tokens).
fn claim_basic_ttl() -> PathBuf {
    vectors_dir().join("claim-basic.in.ttl")
}

fn claim_basic_gmn() -> PathBuf {
    vectors_dir().join("claim-basic.gmn")
}

/// Recursively copy `src` into `dst` (creating `dst`).
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

// ── verify: PASS over the real corpus ───────────────────────────────────────────

/// `gmeow gmn verify` exits 0 over the committed frozen corpus and prints the
/// pass summary (positives byte-frozen + round-tripped, negatives classified).
#[test]
fn gmn_verify_passes_over_the_committed_corpus() {
    gmeow()
        .args([
            "gmn",
            "verify",
            "--vectors",
            vectors_dir().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
            "--grammar",
            grammar().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("positives 19/19"))
        .stdout(predicate::str::contains("gmn conformance PASS"));
}

// ── digest / encode / decode: stable output on a small fixture ───────────────────

/// `gmeow gmn digest` prints the frozen codebook Merkle root and the fixture's
/// content digest, both `blake3:…` and stable run-to-run.
#[test]
fn gmn_digest_is_stable() {
    // The codebook digest is the value the frozen `manifest.ttl` is pinned against.
    gmeow()
        .args([
            "gmn",
            "digest",
            claim_basic_ttl().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "codebook_digest blake3:26a68453ecfe47867551038b8b247e9f4b07b3815bd87979c401afb0f7edf5ce",
        ))
        .stdout(predicate::str::contains("content_digest blake3:"));
}

/// `gmeow gmn encode` reproduces the frozen `claim-basic.gmn` byte-for-byte.
#[test]
fn gmn_encode_matches_the_frozen_vector() {
    let frozen = fs::read_to_string(claim_basic_gmn()).expect("read frozen .gmn");
    gmeow()
        .args([
            "gmn",
            "encode",
            claim_basic_ttl().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::eq(frozen));
}

/// `gmeow gmn decode` reconstructs the source triple as canonical N-Quads.
#[test]
fn gmn_decode_reconstructs_the_source() {
    gmeow()
        .args([
            "gmn",
            "decode",
            claim_basic_gmn().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "<https://blackcatinformatics.ca/gmeow/gate1> \
             <https://blackcatinformatics.ca/gmeow/hasState> \
             <https://blackcatinformatics.ca/gmeow/doorGate1> .",
        ));
}

// ── project: the consume-path security-ring filter on the real CLI surface ───────

/// `gmeow gmn project --ring gmnRingTrusted` over the demonstrator admits core + trusted +
/// nato content and EXCLUDES the out-of-ring restricted claim — the filter runs end-to-end on
/// the shipped binary, not just the library. stdout carries the ring-filtered GMN-1 payload.
#[test]
fn gmn_project_excludes_out_of_ring_content_on_the_cli() {
    gmeow()
        .args([
            "gmn",
            "project",
            ring_consume_ttl().to_str().unwrap(),
            "--ring",
            "gmnRingTrusted",
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        // admitted content is present in the projected GMN-1 …
        .stdout(predicate::str::contains("ringDemoCoreDatum"))
        .stdout(predicate::str::contains("ringDemoTrustedDatum"))
        .stdout(predicate::str::contains("ringDemoNatoDatum"))
        // … and the out-of-ring restricted claim is EXCLUDED (absent from stdout).
        .stdout(predicate::str::contains("ringDemoRestrictedDatum").not())
        .stderr(predicate::str::contains("admitted 3/4 claims, excluded 1"));
}

/// `gmeow gmn project --ring gmnRingNato` exercises the compartment axis: only nato-compartmented
/// content is admitted; plain same-level trusted content is EXCLUDED.
#[test]
fn gmn_project_compartment_axis_excludes_plain_content_on_the_cli() {
    gmeow()
        .args([
            "gmn",
            "project",
            ring_consume_ttl().to_str().unwrap(),
            "--ring",
            "gmnRingNato",
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("ringDemoNatoDatum"))
        .stdout(predicate::str::contains("ringDemoTrustedDatum").not())
        .stdout(predicate::str::contains("ringDemoCoreDatum").not())
        .stderr(predicate::str::contains("admitted 1/4 claims, excluded 3"));
}

/// A tiny `--budget` forces whole-claim elision, disclosed on stderr — never a silent cut.
#[test]
fn gmn_project_budget_discloses_elision_on_the_cli() {
    gmeow()
        .args([
            "gmn",
            "project",
            ring_consume_ttl().to_str().unwrap(),
            "--ring",
            "gmnRingTrusted",
            "--budget",
            "20",
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("admitted claims elided"))
        .stderr(predicate::str::contains("never silently dropped"));
}

/// An unresolvable `--ring` hard-fails (`lang:GmnRingLatticeMalformed`) — no degraded default.
#[test]
fn gmn_project_unknown_ring_hard_fails_on_the_cli() {
    gmeow()
        .args([
            "gmn",
            "project",
            ring_consume_ttl().to_str().unwrap(),
            "--ring",
            "gmnRingNotAThing",
            "--lang-module",
            lang_module().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "not a resolvable gmeow:GmnSecurityRing",
        ));
}

// ── verify: HARD-FAIL on a corrupted corpus and a tampered pack ──────────────────

/// A deliberately corrupted vectors dir (one frozen `.gmn` byte tampered) makes
/// `gmn verify` exit NON-ZERO with the byte-mismatch diagnostic — the byte-exact
/// tooth, proven falsifiable.
#[test]
fn gmn_verify_fails_on_a_corrupted_vectors_dir() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let corrupt = tmp.path().join("gmn1-vectors");
    copy_tree(&vectors_dir(), &corrupt);
    // Append junk to a frozen positive output so its recomputed encoding no longer
    // matches byte-for-byte.
    let target = corrupt.join("claim-basic.gmn");
    let mut bytes = fs::read(&target).expect("read frozen .gmn");
    bytes.extend_from_slice(b"CORRUPT");
    fs::write(&target, bytes).expect("write tampered .gmn");

    gmeow()
        .args([
            "gmn",
            "verify",
            "--vectors",
            corrupt.to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
            "--grammar",
            grammar().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("positives 18/19"))
        .stderr(predicate::str::contains("byte mismatch"));
}

/// A tampered `gmeow:gmnPackRoot` in a supplied pack file makes `gmn verify` exit
/// NON-ZERO — the pack-root tooth.
#[test]
fn gmn_verify_fails_on_a_tampered_pack_root() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let pack = tmp.path().join("pack.ttl");
    fs::write(
        &pack,
        "<https://blackcatinformatics.ca/gmeow/gmnPackCurrent> \
         <https://blackcatinformatics.ca/gmeow/gmnPackRoot> \"blake3:deadbeef\" .\n",
    )
    .expect("write tampered pack");

    gmeow()
        .args([
            "gmn",
            "verify",
            "--vectors",
            vectors_dir().to_str().unwrap(),
            "--lang-module",
            lang_module().to_str().unwrap(),
            "--grammar",
            grammar().to_str().unwrap(),
            "--pack",
            pack.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("gmnPackRoot"));
}
