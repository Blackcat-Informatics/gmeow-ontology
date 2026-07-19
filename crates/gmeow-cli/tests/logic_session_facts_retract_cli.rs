// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC2 (a retract RAISES a surviving fact's proof height) end-to-end on the shipped
//! `gmeow` binary.
//!
//! The `facts` reader surfaces per-fact min-proof-height for the resident open and the
//! INSERT delta path (`facts --apply`). This suite proves the RETRACT side is equally
//! observable on the production CLI via the new `facts --retract` flag — so AC2's
//! required "retract raises a surviving fact's proof height" recomputation is
//! demonstrable on the shipped binary, not only in an engine unit test.
//!
//! The diamond EDB gives `reach(a, c)` two independent proofs: a DIRECT edge a -> c
//! (height 1) and the TRANSITIVE path a -> b -> c (height 2), so its maintained
//! min-proof-height is 1. Retiring the direct a -> c edge drops the height-1 proof;
//! `reach(a, c)` survives via the transitive path, so its min-proof-height RISES 1 -> 2.

use std::path::{Path, PathBuf};

use assert_cmd::Command;

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

fn diamond_edb() -> PathBuf {
    fixture("diamond-edb.ttl")
}

fn program() -> PathBuf {
    fixture("transitive-closure.logic.ttl")
}

fn diamond_retract() -> PathBuf {
    fixture("diamond-retract.ttl")
}

/// Extract the `proof-height` of the `reach(a, c)` derivation from a `facts` stdout.
///
/// Panics if that derivation is absent — which is itself an assertion that
/// `reach(a, c)` is PRESENT in the maintained closure.
fn reach_a_c_proof_height(stdout: &str) -> u64 {
    let subject = "subject=<https://example.org/session/a>";
    let predicate = "predicate=https://example.org/session/reach";
    let object = "object=<https://example.org/session/c>";
    for line in stdout.lines() {
        let line = line.trim_start();
        if !line.starts_with("derivation ") {
            continue;
        }
        if !(line.contains(subject) && line.contains(predicate) && line.contains(object)) {
            continue;
        }
        let height = line
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix("proof-height="))
            .unwrap_or_else(|| panic!("reach(a,c) derivation carries a proof-height:\n{line}"));
        return height
            .parse::<u64>()
            .unwrap_or_else(|e| panic!("proof-height `{height}` parses as u64: {e}"));
    }
    panic!("stdout carries a reach(a,c) derivation (the fact survives):\n{stdout}");
}

/// True iff the maintained closure lists the `reach(a, c)` FACT row (independent of the
/// derivation-provenance block).
fn reach_a_c_present(stdout: &str) -> bool {
    stdout.lines().any(|line| {
        line.starts_with("fact https://example.org/session/reach ")
            && line.contains("<https://example.org/session/a>")
            && line.contains("<https://example.org/session/c>")
    })
}

#[test]
fn facts_retract_raises_surviving_fact_proof_height() {
    // (a) Baseline: over the diamond EDB, `reach(a, c)` has a DIRECT height-1 proof, so
    //     its maintained min-proof-height is 1.
    let before = gmeow()
        .args(["logic", "session", "facts"])
        .arg("--edb")
        .arg(diamond_edb())
        .arg("--program")
        .arg(program())
        .assert()
        .success();
    let before_stdout = String::from_utf8_lossy(&before.get_output().stdout).into_owned();
    assert!(
        reach_a_c_present(&before_stdout),
        "reach(a,c) is in the base maintained closure:\n{before_stdout}"
    );
    let before_height = reach_a_c_proof_height(&before_stdout);
    assert_eq!(
        before_height, 1,
        "the direct a->c edge gives reach(a,c) a height-1 proof:\n{before_stdout}"
    );

    // (b) Retract the DIRECT a -> c edge. The height-1 proof is gone, but `reach(a, c)`
    //     SURVIVES via the transitive path a -> b -> c, so its min-proof-height RISES to
    //     2. This is the CLI-observable AC2 witness.
    let after = gmeow()
        .args(["logic", "session", "facts"])
        .arg("--edb")
        .arg(diamond_edb())
        .arg("--program")
        .arg(program())
        .arg("--retract")
        .arg(diamond_retract())
        .assert()
        .success();
    let after_stdout = String::from_utf8_lossy(&after.get_output().stdout).into_owned();
    assert!(
        after_stdout.contains("outcome Applied"),
        "the retract is a genuine incremental Applied:\n{after_stdout}"
    );
    assert!(
        reach_a_c_present(&after_stdout),
        "reach(a,c) SURVIVES the retract (via the transitive path):\n{after_stdout}"
    );
    let after_height = reach_a_c_proof_height(&after_stdout);
    assert_eq!(
        after_height, 2,
        "retracting the direct edge RAISES reach(a,c) to the transitive height 2:\n{after_stdout}"
    );

    // (c) Explicit height rise: the retract strictly RAISED the surviving fact's proof
    //     height (1 -> 2), not merely left it unchanged.
    assert!(
        after_height > before_height,
        "the retract RAISES reach(a,c) proof height: {before_height} -> {after_height}"
    );
}
