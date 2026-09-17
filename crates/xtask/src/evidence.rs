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

/// How long a subprocess [`command_output`] spawns may run before it is killed. A
/// child that never exits must never hang the whole gate: `git` here is local and
/// finishes in well under this bound, but a plumbing call that blocks on an index
/// lock, a credential prompt, or a wedged filesystem does not, and there is no
/// caller in a position to interrupt it.
const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// How often the deadline poll loop checks [`std::process::Child::try_wait`] while
/// waiting for the child to exit.
const COMMAND_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

fn command_output(command: &mut Command, context: &str) -> Result<String> {
    let output = spawn_with_deadline(command, context, COMMAND_TIMEOUT)?;
    output_text(output, context)
}

/// Spawn `command` with piped stdout/stderr and poll for exit until it completes or
/// `timeout` elapses, killing the child on expiry — a bounded-deadline replacement
/// for the blocking [`Command::output`], which has no timeout at all and hangs
/// forever if the child never exits.
///
/// stdout/stderr are drained on dedicated threads WHILE polling, not read only
/// after exit: a child that writes more than one pipe buffer (~64KiB on Linux)
/// would otherwise deadlock against this thread only checking `try_wait`, exactly
/// the failure mode a timeout exists to rule out.
///
/// On expiry the child is killed (best-effort — a kill failure is not itself
/// surfaced) and this returns the SAME error shape [`Command::output`]'s I/O-error
/// arm already returns, so every caller's existing error handling keeps working
/// unchanged; a timeout is just one more way the command "failed to run".
fn spawn_with_deadline(
    command: &mut Command,
    context: &str,
    timeout: std::time::Duration,
) -> Result<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| failure(format!("{context}: {error}")))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| failure(format!("{context}: no stdout pipe")))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| failure(format!("{context}: no stderr pipe")))?;
    let stdout_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    // DETACH the readers; do NOT join them. `kill` reaches the child we
                    // spawned, not its descendants, and a surviving grandchild keeps the
                    // write end of both pipes open — `sh -c "cmd"` forks rather than
                    // execs whenever the shell declines that optimization, which is
                    // exactly the shape this deadline exists for. `read_to_end` does not
                    // return until every writer closes, so joining here blocks for the
                    // GRANDCHILD's full runtime and silently reinstates the hang this
                    // function was written to remove. The buffers are discarded on this
                    // path anyway, so the threads have nothing left to deliver: they end
                    // when the pipe finally closes and drop what they read. Returning
                    // promptly is the contract, and it must not depend on a process we
                    // cannot reach.
                    drop(stdout_reader);
                    drop(stderr_reader);
                    return Err(failure(format!(
                        "{context}: timed out after {timeout:?} and was killed"
                    )));
                }
                std::thread::sleep(COMMAND_POLL_INTERVAL);
            }
            Err(error) => return Err(failure(format!("{context}: {error}"))),
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| failure(format!("{context}: stdout reader thread panicked")))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| failure(format!("{context}: stderr reader thread panicked")))?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
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

#[path = "evidence.tests.rs"]
#[cfg(test)]
mod tests;
