// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The successful-check receipt: an attestable statement that one exact commit passed
//! the whole gate under one exact task registry and toolchain contract.
//!
//! CI emits this on every green `main` push (`cargo xtask receipt create`) and attests
//! its provenance with `actions/attest-build-provenance`, so a downstream consumer can
//! verify "commit X passed the gate" without trusting the reporter. It is a supply-chain
//! artifact, not a scheduling input: the gate no longer reuses a base receipt to skip
//! tasks, so nothing here reads git history, downloads artifacts, or classifies changed
//! paths.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};

pub(crate) const REPOSITORY: &str = "Blackcat-Informatics/gmeow-ontology";
const RECEIPT_SCHEMA: &str = "gmeow-check-receipt-v1";

type Result<T> = gmeow_errors::Result<T>;

pub(crate) fn failure(message: impl Into<String>) -> Diag {
    Diag::new(
        gmeow_errors::code::register_code("xtask.check.evidence"),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
}

pub(crate) fn create_receipt(
    root: &Path,
    out: &Path,
    registry: &str,
    toolchain: &str,
    tasks: &[&str],
) -> Result<()> {
    let commit = git(root, ["rev-parse", "HEAD"])?;
    let tree = git(root, ["rev-parse", "HEAD^{tree}"])?;
    let mut body = format!(
        "schema={RECEIPT_SCHEMA}\nrepository={REPOSITORY}\ncommit={commit}\ntree={tree}\nregistry={registry}\ntoolchain={toolchain}\nstatus=success\n"
    );
    // Sorted + deduplicated so the receipt body is a pure function of the task set,
    // independent of CHECK_DAG declaration order.
    for task in tasks.iter().collect::<BTreeSet<_>>() {
        body.push_str("task=");
        body.push_str(task);
        body.push('\n');
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            failure(format!(
                "create receipt directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let temp = out.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temp, body)
        .map_err(|error| failure(format!("write receipt {}: {error}", temp.display())))?;
    std::fs::rename(&temp, out)
        .map_err(|error| failure(format!("install receipt {}: {error}", out.display())))?;
    Ok(())
}

pub(crate) fn digest_files(root: &Path, paths: &[&str]) -> Result<String> {
    let mut framed = Vec::new();
    for path in paths {
        framed.extend_from_slice(path.len().to_string().as_bytes());
        framed.push(b':');
        framed.extend_from_slice(path.as_bytes());
        framed.push(b':');
        let full = root.join(path);
        if full.is_file() {
            let digest = command_output(
                Command::new("git")
                    .arg("hash-object")
                    .arg("--")
                    .arg(path)
                    .current_dir(root),
                "git hash-object",
            )?;
            framed.extend_from_slice(digest.as_bytes());
        } else {
            framed.extend_from_slice(b"missing");
        }
        framed.push(b'\n');
    }
    hash_stdin(root, &framed)
}

pub(crate) fn hash_registry(root: &Path, registry: &str) -> Result<String> {
    hash_stdin(root, registry.as_bytes())
}

fn hash_stdin(root: &Path, bytes: &[u8]) -> Result<String> {
    let mut child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| failure(format!("spawn git hash-object: {error}")))?;
    child
        .stdin
        .take()
        .ok_or_else(|| failure("git hash-object stdin unavailable"))?
        .write_all(bytes)
        .map_err(|error| failure(format!("write git hash-object stdin: {error}")))?;
    let output = child
        .wait_with_output()
        .map_err(|error| failure(format!("wait for git hash-object: {error}")))?;
    output_text(output, "git hash-object --stdin")
}

fn git<I, S>(root: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    command_output(Command::new("git").args(args).current_dir(root), "git")
}

fn command_output(command: &mut Command, context: &str) -> Result<String> {
    let output = command
        .output()
        .map_err(|error| failure(format!("{context}: {error}")))?;
    output_text(output, context)
}

fn output_text(output: std::process::Output, context: &str) -> Result<String> {
    if !output.status.success() {
        return Err(failure(format!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_owned())
        .map_err(|_| failure(format!("{context}: command emitted non-UTF-8 output")))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
