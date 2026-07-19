// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC6 (paged composition) end-to-end on the shipped `gmeow` binary.
//!
//! These drive the real `gmeow logic session … --paged` surface over the committed
//! `logic-session` fixtures, proving that `ReasoningSession::open_paged` is exercised
//! through the production CLI (not only in unit tests): the paged open prints the
//! seven-axis identity plus non-trivial page-fault composition metrics, and the paged
//! `facts` readback maintains the IDENTICAL derived closure the resident path does.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Absolute path of a committed `logic-session` fixture, relative to this crate.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/logic-session")
        .join(name)
}

fn edb() -> PathBuf {
    fixture("edb.ttl")
}

fn program() -> PathBuf {
    fixture("transitive-closure.logic.ttl")
}

/// The closure-only projection of a `facts` run: every line EXCEPT the identity-bound
/// `head` line and the trailing `paged-*` composition-metric lines. The `head` axis
/// legitimately differs between the paged and resident opens (the paged source names a
/// different `dataSourceContract`), but the maintained derived closure — the facts,
/// provenance count, per-fact derivations, premises, and proof heights — must be
/// byte-identical.
fn closure_projection(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| !line.starts_with("head ") && !line.starts_with("paged-"))
        .collect()
}

#[test]
fn open_paged_prints_identity_and_nontrivial_page_faults() {
    // `--page-size 1` splits the two-quad EDB into two single-quad pages, so the
    // page-fault accounting is genuinely non-trivial (two pages requested + consumed).
    let assert = gmeow()
        .args(["logic", "session", "open"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--paged")
        .args(["--page-size", "1"])
        .assert()
        .success();

    let output = assert.get_output();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // (a) The seven-axis SessionIdentity prints, including the paged source contract
    //     threaded into the data-generation axis, plus the genesis head + disposition.
    assert!(
        stdout.contains("ReasoningSessionIdentity"),
        "seven-axis identity prints: {stdout}"
    );
    assert!(
        stdout.contains("session/paged-in-memory-provider-v1"),
        "the paged source contract is threaded into the identity: {stdout}"
    );
    assert!(
        stdout.contains("dataGeneration") && stdout.contains("urn:blake3:"),
        "the content-addressed data-generation prints: {stdout}"
    );
    assert!(
        stdout.contains("genesis-head "),
        "the genesis journal head prints: {stdout}"
    );
    assert!(
        stdout.contains("fragment-disposition incremental"),
        "the fixed program is the incremental fragment: {stdout}"
    );

    // (b) Non-trivial paged composition metrics: two pages requested + consumed for the
    //     two-quad EDB paged at one quad per page, and two delivered primary quads.
    assert!(
        stdout.contains("paged-backend-requested-pages 2"),
        "two pages requested: {stdout}"
    );
    assert!(
        stdout.contains("paged-backend-consumed-pages 2"),
        "two pages consumed: {stdout}"
    );
    assert!(
        stdout.contains("paged-source-delivered-quads 2"),
        "two primary quads paged in: {stdout}"
    );
}

#[test]
fn open_paged_default_page_size_pages_whole_world() {
    // Without `--page-size`, the whole world is a single page — still a genuine paged
    // open (one page requested + consumed), demonstrating the un-chunked composition.
    gmeow()
        .args(["logic", "session", "open"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--paged")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("paged-backend-requested-pages 1")
                .and(predicate::str::contains("paged-backend-consumed-pages 1"))
                .and(predicate::str::contains("paged-source-delivered-quads 2")),
        );
}

#[test]
fn facts_paged_reads_back_the_resident_closure() {
    let resident = gmeow()
        .args(["logic", "session", "facts"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .assert()
        .success();
    let resident_stdout = String::from_utf8_lossy(&resident.get_output().stdout).into_owned();

    let paged = gmeow()
        .args(["logic", "session", "facts"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--paged")
        .args(["--page-size", "1"])
        .assert()
        .success();
    let paged_stdout = String::from_utf8_lossy(&paged.get_output().stdout).into_owned();

    // The maintained derived closure (facts + provenance + derivations + proof heights)
    // is byte-identical across the resident and paged opens.
    assert_eq!(
        closure_projection(&paged_stdout),
        closure_projection(&resident_stdout),
        "paged closure equals resident closure\n--- paged ---\n{paged_stdout}\n--- resident ---\n{resident_stdout}"
    );

    // The closure is non-trivial (the transitive `reach` edges are materialized).
    assert!(
        paged_stdout.contains("facts 5"),
        "five maintained facts: {paged_stdout}"
    );
    assert!(
        paged_stdout.contains(
            "fact https://example.org/session/reach <https://example.org/session/a> <https://example.org/session/c>"
        ),
        "the derived transitive edge a->c is present: {paged_stdout}"
    );

    // The paged readback additionally surfaces its page-fault composition metrics.
    assert!(
        paged_stdout.contains("paged-source-delivered-quads 2"),
        "paged facts surfaces composition metrics: {paged_stdout}"
    );
    // The resident path has NO paged metrics (it is a resident open).
    assert!(
        !resident_stdout.contains("paged-"),
        "the resident path emits no paged metrics: {resident_stdout}"
    );
}
