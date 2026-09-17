// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn a_hanging_child_is_killed_and_the_call_returns_within_the_deadline() {
    // The bug this closes: every subprocess this module runs went through a
    // plain `.output()`, which hangs forever if the child never exits. Prove
    // the fix with a genuinely hanging child and a SHORT deadline — the call
    // must return an error promptly, never block for anywhere near the child's
    // own runtime.
    //
    // `sleep 100 & wait` rather than a bare `sleep 100`: the bare form lets the
    // shell `exec` the sleep, so killing the child closes the pipes and the
    // hazard never appears. Backgrounding forces a real GRANDCHILD that keeps
    // the write end open after the shell dies — on every shell, not only the
    // ones that decline to exec. That is the case that made this green locally
    // and red on CI, so it is the case the test has to pin.
    let start = std::time::Instant::now();
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "sleep 100 & wait"]);
    let err = super::spawn_with_deadline(
        &mut command,
        "test hang",
        std::time::Duration::from_millis(200),
    )
    .expect_err("a child that never exits must time out, not hang forever");
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "spawn_with_deadline must return promptly after its own deadline elapses, took {elapsed:?}"
    );
    assert!(
        err.message().contains("timed out"),
        "the timeout must name itself, not surface as an ordinary I/O failure: {err}"
    );
}

#[test]
fn a_quick_child_returns_its_real_output_well_under_the_deadline() {
    // Negative control: a child that exits immediately must not be affected by
    // the polling/kill machinery at all — its real stdout comes back intact.
    let mut command = std::process::Command::new("sh");
    command.args(["-c", "echo hello"]);
    let output = super::spawn_with_deadline(
        &mut command,
        "test quick",
        std::time::Duration::from_secs(10),
    )
    .expect("a quick child must not time out");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "hello");
}

/// The receipt must name the exact commit, tree, registry digest, toolchain
/// digest, and task set it attests — that binding is the whole artifact.
#[test]
fn a_receipt_binds_commit_tree_registry_toolchain_and_tasks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let commit = git(root, ["rev-parse", "HEAD"]).expect("resolve HEAD");
    let tree = git(root, ["rev-parse", "HEAD^{tree}"]).expect("resolve HEAD tree");
    let tmp = tempfile::tempdir().expect("create temp dir");
    let out = tmp.path().join("receipt.txt");

    create_receipt(
        root,
        &out,
        "reg-digest-abc",
        "tc-digest-xyz",
        &["sync", "doc-lint", "compliance-report"],
    )
    .expect("create receipt");
    let body = std::fs::read_to_string(&out).expect("read receipt");

    for expected in [
        format!("schema={RECEIPT_SCHEMA}"),
        format!("repository={REPOSITORY}"),
        format!("commit={commit}"),
        format!("tree={tree}"),
        "registry=reg-digest-abc".to_owned(),
        "toolchain=tc-digest-xyz".to_owned(),
        "status=success".to_owned(),
        "task=sync".to_owned(),
        "task=doc-lint".to_owned(),
        "task=compliance-report".to_owned(),
    ] {
        assert!(
            body.lines().any(|line| line == expected),
            "receipt is missing {expected:?}:\n{body}"
        );
    }
}

/// The task list is sorted, so two runs over the same task SET produce identical
/// bytes regardless of the order the caller enumerated them in.
#[test]
fn the_receipt_body_is_order_independent() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tmp = tempfile::tempdir().expect("create temp dir");
    let first = tmp.path().join("receipt-a.txt");
    let second = tmp.path().join("receipt-b.txt");

    create_receipt(root, &first, "r", "t", &["sync", "audit", "validate"]).expect("first");
    create_receipt(root, &second, "r", "t", &["validate", "sync", "audit"]).expect("second");
    let a = std::fs::read_to_string(&first).expect("read first");
    let b = std::fs::read_to_string(&second).expect("read second");
    assert_eq!(
        a, b,
        "receipt bytes must depend on the task SET, not its order"
    );
}

/// A digest over the toolchain contract must change when any covered file's
/// content changes and stay stable when nothing does.
#[test]
fn the_toolchain_digest_is_content_addressed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let a = digest_files(root, &["Cargo.toml", "Makefile"]).expect("digest");
    let b = digest_files(root, &["Cargo.toml", "Makefile"]).expect("digest again");
    assert_eq!(a, b, "the same inputs must hash identically");
    let c = digest_files(root, &["Makefile", "Cargo.toml"]).expect("digest reordered");
    assert_ne!(
        a, c,
        "the digest must be sensitive to the covered file order"
    );
    let missing = digest_files(root, &["definitely-not-a-file-in-this-repo"]).expect("digest");
    assert_ne!(a, missing);
}

#[test]
fn the_registry_digest_distinguishes_task_registries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let a = hash_registry(root, "sync\0check-sync\0\n").expect("hash");
    let b = hash_registry(root, "sync\0check-sync\0\n").expect("hash again");
    let c = hash_registry(root, "sync\0check-sync\0rust-build\n").expect("hash other");
    assert_eq!(a, b);
    assert_ne!(a, c, "a changed dependency edge must change the digest");
}
