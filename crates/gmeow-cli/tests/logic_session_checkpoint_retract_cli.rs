// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! AC1 (checkpoint/restore over a SUPPRESSION) end-to-end on the shipped `gmeow`
//! binary.
//!
//! This proves the `checkpoint --retract` surface is reachable on the production
//! binary — not just in library unit tests — and that a committed delta carrying a
//! NON-EMPTY suppression is persisted into the checkpoint and faithfully replayed on
//! restore. The prior fix wired `CommittedDelta.retirement_nquads`, but no production
//! surface ever populated it (the CLI `checkpoint` command built additions-only
//! deltas): this suite drives the real retirement-replay branch.

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

fn edb() -> PathBuf {
    fixture("edb.ttl")
}

fn program() -> PathBuf {
    fixture("transitive-closure.logic.ttl")
}

fn additions() -> PathBuf {
    fixture("additions.ttl")
}

fn retire() -> PathBuf {
    fixture("retire.ttl")
}

/// The value of the (last) `head <hash>` line in a command's stdout.
fn head_line(stdout: &str) -> String {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("head "))
        .next_back()
        .unwrap_or_else(|| panic!("stdout carries a `head` line:\n{stdout}"))
        .trim()
        .to_owned()
}

/// The value of the `journal-head <hash>` line in a checkpoint's stdout.
fn journal_head_line(stdout: &str) -> String {
    stdout
        .lines()
        .filter_map(|line| line.strip_prefix("journal-head "))
        .next_back()
        .unwrap_or_else(|| panic!("stdout carries a `journal-head` line:\n{stdout}"))
        .trim()
        .to_owned()
}

/// The `restore` command's printed head (`  head <hash>`, indented under the outcome).
fn restore_head_line(stdout: &str) -> String {
    stdout
        .lines()
        .filter_map(|line| line.trim().strip_prefix("head "))
        .next_back()
        .unwrap_or_else(|| panic!("restore stdout carries a `head` line:\n{stdout}"))
        .trim()
        .to_owned()
}

#[test]
fn checkpoint_retract_persists_suppression_and_restore_replays_it() {
    // (a) Reference: apply the SAME combined delta (add c->d, retract a->b) through the
    //     `apply` command to fix the authoritative post-retract head.
    let apply = gmeow()
        .args(["logic", "session", "apply"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--additions")
        .arg(additions())
        .arg("--retract")
        .arg(retire())
        .assert()
        .success();
    let apply_stdout = String::from_utf8_lossy(&apply.get_output().stdout).into_owned();
    assert!(
        apply_stdout.contains("outcome Applied"),
        "the combined add+retract delta is a genuine incremental Applied: {apply_stdout}"
    );
    let post_retract_head = head_line(&apply_stdout);

    // (b) Mint a checkpoint AFTER the combined add+retract delta. Its committed delta
    //     must carry the suppression, and the checkpoint's journal head must equal the
    //     `apply`-path post-retract head (same committed transition).
    let out = tempfile::Builder::new()
        .prefix("gmeow-cp-retract")
        .suffix(".json")
        .tempfile()
        .expect("temp checkpoint path");
    let cp = gmeow()
        .args(["logic", "session", "checkpoint"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--apply")
        .arg(additions())
        .arg("--retract")
        .arg(retire())
        .arg("-o")
        .arg(out.path())
        .assert()
        .success();
    let cp_stdout = String::from_utf8_lossy(&cp.get_output().stdout).into_owned();
    assert!(
        cp_stdout.contains("outcome Applied"),
        "the checkpoint's pre-mint delta applies the suppression: {cp_stdout}"
    );
    assert_eq!(
        journal_head_line(&cp_stdout),
        post_retract_head,
        "the checkpoint's journal head is the post-retract head (same committed transition)"
    );

    // (c) The persisted checkpoint carries a NON-EMPTY retirement payload — the exact
    //     retired edge (a->b), re-homed into the session world — proving the previously
    //     DARK `retirement_nquads` branch is populated on the shipped surface.
    let json = std::fs::read_to_string(out.path()).expect("checkpoint JSON readable");
    let a_to_b = "<https://example.org/session/a> <https://example.org/session/edge> \
         <https://example.org/session/b> \
         <https://blackcatinformatics.ca/gmeow/logic/session/world> .";
    assert!(
        json.contains("retirement_nquads"),
        "the checkpoint JSON carries the retirement field: {json}"
    );
    assert!(
        !json.contains("\"retirement_nquads\": []"),
        "the retirement payload is NON-EMPTY (the delta carried a suppression): {json}"
    );
    assert!(
        json.contains(a_to_b),
        "the retired a->b edge is persisted in the checkpoint: {json}"
    );

    // (d) Restore replays the persisted delta over the BASE EDB. Restore verifies the
    //     replayed journal head against the checkpoint's durable head, so a Restored
    //     outcome at the post-retract head proves the suppression was replayed faithfully
    //     (a dropped suppression would diverge the replayed head and fail restore).
    let restored = gmeow()
        .args(["logic", "session", "restore"])
        .arg("--in")
        .arg(out.path())
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .assert()
        .success();
    let restored_stdout = String::from_utf8_lossy(&restored.get_output().stdout).into_owned();
    assert!(
        restored_stdout.contains("outcome Restored"),
        "the retract checkpoint restores by replay: {restored_stdout}"
    );
    assert_eq!(
        restore_head_line(&restored_stdout),
        post_retract_head,
        "restore reproduces the exact post-retract head (the suppression survived replay)"
    );

    // (e) Contrast: the SAME additions WITHOUT the retract commit a DIFFERENT head, so
    //     the suppression genuinely changed the committed transition (not a silent no-op).
    let insert_only = gmeow()
        .args(["logic", "session", "checkpoint"])
        .arg("--edb")
        .arg(edb())
        .arg("--program")
        .arg(program())
        .arg("--apply")
        .arg(additions())
        .arg("-o")
        .arg(out.path())
        .assert()
        .success();
    let insert_only_stdout = String::from_utf8_lossy(&insert_only.get_output().stdout).into_owned();
    assert_ne!(
        journal_head_line(&insert_only_stdout),
        post_retract_head,
        "the insert-only checkpoint commits a different head than the add+retract delta"
    );
}
