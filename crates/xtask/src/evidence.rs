// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Verified base-check receipts and conservative changed-path impact selection.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use gmeow_errors::{Diag, FindingCategory, Grade, Severity, Standpoint};

pub(crate) const REPOSITORY: &str = "Blackcat-Informatics/gmeow-ontology";
const RECEIPT_SCHEMA: &str = "gmeow-check-receipt-v1";
const SIGNER_WORKFLOW: &str = "Blackcat-Informatics/gmeow-ontology/.github/workflows/ci.yml";

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

#[derive(Debug)]
struct Receipt {
    repository: String,
    commit: String,
    tree: String,
    registry: String,
    toolchain: String,
    status: String,
    tasks: BTreeSet<String>,
}

#[derive(Debug)]
pub(crate) struct ImpactDecision {
    pub(crate) base: String,
    pub(crate) selected: BTreeSet<String>,
    pub(crate) reasons: BTreeMap<String, BTreeSet<String>>,
    pub(crate) changed_paths: Vec<String>,
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
    for task in tasks {
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

pub(crate) fn verified_impact_decision(
    root: &Path,
    explicit_base: Option<&str>,
    registry: &str,
    toolchain: &str,
    task_names: &[&str],
) -> Result<ImpactDecision> {
    let base = match explicit_base {
        Some(base) => git(root, ["rev-parse", base])?,
        None => git(root, ["merge-base", "HEAD", "origin/main"])
            .or_else(|_| git(root, ["rev-parse", "HEAD"]))?,
    };
    let receipt_path = obtain_receipt(root, &base)?;
    verify_attestation(root, &receipt_path, &base)?;
    let receipt = parse_receipt(&receipt_path)?;
    validate_receipt(root, &receipt, &base, registry, toolchain, task_names)?;

    let changed_paths = changed_paths(root, &base)?;
    let (selected, reasons) = select_tasks(&changed_paths, task_names);
    Ok(ImpactDecision {
        base,
        selected,
        reasons,
        changed_paths,
    })
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

fn obtain_receipt(root: &Path, base: &str) -> Result<PathBuf> {
    let cache = root
        .join(".cache/gmeow-task/receipts")
        .join(format!("{base}.txt"));
    if cache.is_file() {
        return Ok(cache);
    }

    let run_id = command_output(
        Command::new("gh")
            .args([
                "run",
                "list",
                "--repo",
                REPOSITORY,
                "--workflow",
                "ci.yml",
                "--commit",
                base,
                "--event",
                "push",
                "--status",
                "success",
                "--limit",
                "1",
                "--json",
                "databaseId",
                "--jq",
                ".[0].databaseId",
            ])
            .current_dir(root),
        "find successful base CI run",
    )?;
    if run_id.is_empty() || run_id == "null" {
        return Err(failure(format!(
            "no successful main-push CI receipt exists for {base}"
        )));
    }

    let download = root
        .join(".cache/gmeow-task")
        .join(format!("receipt-download-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&download);
    std::fs::create_dir_all(&download)
        .map_err(|error| failure(format!("create receipt download directory: {error}")))?;
    let artifact = format!("gmeow-check-receipt-{base}");
    let status = Command::new("gh")
        .args([
            "run", "download", &run_id, "--repo", REPOSITORY, "--name", &artifact, "--dir",
        ])
        .arg(&download)
        .current_dir(root)
        .status()
        .map_err(|error| failure(format!("download base receipt: {error}")))?;
    if !status.success() {
        let _ = std::fs::remove_dir_all(&download);
        return Err(failure(format!(
            "download base receipt from run {run_id}: {status}"
        )));
    }
    let downloaded = find_named_file(&download, "check-receipt.txt")?
        .ok_or_else(|| failure("downloaded artifact contains no check-receipt.txt"))?;
    if let Some(parent) = cache.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| failure(format!("create receipt cache: {error}")))?;
    }
    std::fs::copy(&downloaded, &cache)
        .map_err(|error| failure(format!("cache verified-receipt candidate: {error}")))?;
    let _ = std::fs::remove_dir_all(&download);
    Ok(cache)
}

fn verify_attestation(root: &Path, receipt: &Path, base: &str) -> Result<()> {
    let status = Command::new("gh")
        .args([
            "attestation",
            "verify",
            "--repo",
            REPOSITORY,
            "--signer-workflow",
            SIGNER_WORKFLOW,
            "--source-digest",
            base,
            "--source-ref",
            "refs/heads/main",
            "--deny-self-hosted-runners",
        ])
        .arg(receipt)
        .current_dir(root)
        .stdout(Stdio::null())
        .status()
        .map_err(|error| failure(format!("verify base receipt attestation: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(failure(format!(
            "base receipt attestation rejected: {status}"
        )))
    }
}

fn parse_receipt(path: &Path) -> Result<Receipt> {
    let body = std::fs::read_to_string(path)
        .map_err(|error| failure(format!("read receipt {}: {error}", path.display())))?;
    let mut fields: BTreeMap<&str, &str> = BTreeMap::new();
    let mut tasks = BTreeSet::new();
    for line in body.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| failure(format!("malformed receipt line {line:?}")))?;
        if key == "task" {
            tasks.insert(value.to_owned());
        } else if fields.insert(key, value).is_some() {
            return Err(failure(format!("duplicate receipt field {key:?}")));
        }
    }
    let field = |name: &str| {
        fields
            .get(name)
            .copied()
            .map(str::to_owned)
            .ok_or_else(|| failure(format!("receipt missing {name}")))
    };
    if field("schema")? != RECEIPT_SCHEMA {
        return Err(failure("unsupported check receipt schema"));
    }
    Ok(Receipt {
        repository: field("repository")?,
        commit: field("commit")?,
        tree: field("tree")?,
        registry: field("registry")?,
        toolchain: field("toolchain")?,
        status: field("status")?,
        tasks,
    })
}

fn validate_receipt(
    root: &Path,
    receipt: &Receipt,
    base: &str,
    registry: &str,
    toolchain: &str,
    task_names: &[&str],
) -> Result<()> {
    let expected_tasks = task_names
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    let tree = git(root, ["rev-parse", &format!("{base}^{{tree}}")])?;
    if receipt.repository != REPOSITORY
        || receipt.commit != base
        || receipt.tree != tree
        || receipt.registry != registry
        || receipt.toolchain != toolchain
        || receipt.status != "success"
        || receipt.tasks != expected_tasks
    {
        return Err(failure(
            "base receipt does not match the current repository, task registry, or toolchain contract",
        ));
    }
    Ok(())
}

fn changed_paths(root: &Path, base: &str) -> Result<Vec<String>> {
    let mut paths = nul_paths(
        Command::new("git")
            .args(["diff", "--name-only", "--no-renames", "-z", base, "--"])
            .current_dir(root),
        "git diff changed paths",
    )?;
    paths.extend(nul_paths(
        Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard", "-z"])
            .current_dir(root),
        "git untracked paths",
    )?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn select_tasks(
    paths: &[String],
    task_names: &[&str],
) -> (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>) {
    let all = task_names.iter().copied().collect::<BTreeSet<_>>();
    let mut selected = BTreeSet::new();
    let mut reasons: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut add = |task: &str, path: &str| {
        if all.contains(task) {
            selected.insert(task.to_owned());
            reasons
                .entry(task.to_owned())
                .or_default()
                .insert(path.to_owned());
        }
    };
    for path in paths {
        let extension = Path::new(path)
            .extension()
            .and_then(OsStr::to_str)
            .unwrap_or_default();
        if path == "Makefile"
            || path == "Cargo.toml"
            || path == "Cargo.lock"
            || path == "rust-toolchain.toml"
            || path.starts_with(".cargo/")
            || path.starts_with("crates/")
            || path.starts_with("scripts/")
        {
            for task in &all {
                add(task, path);
            }
            continue;
        }

        add("check-lint", path);
        if path.starts_with(".github/") {
            continue;
        }

        // Semantic membership MUST be checked before the docs shortcut: a
        // `.md` file under a semantic prefix (e.g. a design doc pulled into a
        // Rust test via `include_str!`) still needs the full semantic gate,
        // not the docs-only set. Checking extension alone would fail-OPEN and
        // silently skip Rust/reason coverage that CI still enforces.
        let semantic = ["ttl", "nt", "nq", "trig", "rq", "sssom", "csv", "tsv"]
            .contains(&extension)
            || [
                "slices/",
                "dsl/",
                "imports/",
                "metadata/",
                "shapes/",
                "queries/",
                "ontology/",
                "governance/",
                "conformance/",
                "coverage/",
                "tests/",
                "validations/",
                "generated/",
            ]
            .iter()
            .any(|prefix| path.starts_with(prefix));
        if semantic {
            for task in [
                "sync",
                "rust-build",
                "validate",
                "constitution-check",
                "audit",
                "wikidata",
                "coverage",
                "acceptance",
                "reason-gate",
                "lint-alignment",
                "i18n-lint",
                "doc-lint",
                "coherence-gate-teeth",
                "slice-quality-gate",
                "bench-soak",
                "compliance-report",
            ] {
                add(task, path);
            }
            continue;
        }
        if path.starts_with("docs/") || extension == "md" {
            for task in ["sync", "doc-lint", "compliance-report"] {
                add(task, path);
            }
            if path == "CONSTITUTION.md" {
                add("constitution-check", path);
            }
            continue;
        }
        if path.starts_with("i18n/") || extension == "po" {
            for task in ["sync", "i18n-lint", "doc-lint", "compliance-report"] {
                add(task, path);
            }
            continue;
        }
        if path.starts_with("bench/") {
            for task in ["bench-soak", "compliance-report"] {
                add(task, path);
            }
            continue;
        }
        for task in &all {
            add(task, path);
        }
    }
    (selected, reasons)
}

fn find_named_file(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    for entry in std::fs::read_dir(root).map_err(|error| {
        failure(format!(
            "read artifact directory {}: {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| failure(format!("read artifact entry: {error}")))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_named_file(&path, name)? {
                return Ok(Some(found));
            }
        } else if path.file_name() == Some(OsStr::new(name)) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn nul_paths(command: &mut Command, context: &str) -> Result<Vec<String>> {
    let output = command
        .output()
        .map_err(|error| failure(format!("{context}: {error}")))?;
    if !output.status.success() {
        return Err(failure(format!(
            "{context}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec()).map_err(|_| {
                failure(format!(
                    "{context}: non-UTF-8 path cannot be impact-classified"
                ))
            })
        })
        .collect()
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

    const TASKS: &[&str] = &[
        "sync",
        "check-lint",
        "rust-build",
        "rust-gate",
        "validate",
        "reason-gate",
        "doc-lint",
        "compliance-report",
    ];

    #[test]
    fn rust_changes_fail_closed_to_the_full_registry() {
        let (selected, _) = select_tasks(&["crates/logic/src/lib.rs".to_owned()], TASKS);
        assert_eq!(selected.len(), TASKS.len());
    }

    #[test]
    fn docs_changes_reuse_unaffected_rust_and_reasoning_proofs() {
        let (selected, _) = select_tasks(&["docs/guide.md".to_owned()], TASKS);
        assert_eq!(
            selected,
            ["check-lint", "compliance-report", "doc-lint", "sync"]
                .into_iter()
                .map(str::to_owned)
                .collect()
        );
    }

    #[test]
    fn slices_markdown_selects_the_full_semantic_gate_not_docs_only() {
        let (selected, _) =
            select_tasks(&["slices/grounding/math/design/FOO.md".to_owned()], TASKS);
        assert!(
            selected.contains("rust-build"),
            "semantic-prefix markdown must select rust-build: {selected:?}"
        );
        assert!(
            selected.contains("reason-gate"),
            "semantic-prefix markdown must select reason-gate: {selected:?}"
        );
        let docs_only: BTreeSet<String> = ["check-lint", "compliance-report", "doc-lint", "sync"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_ne!(
            selected, docs_only,
            "semantic-prefix markdown must not be misrouted to the docs-only set"
        );

        let (md_selected, _) = select_tasks(&["slices/x/foo.md".to_owned()], TASKS);
        let (ttl_selected, _) = select_tasks(&["slices/x/foo.ttl".to_owned()], TASKS);
        assert_eq!(
            md_selected, ttl_selected,
            "a .md file under a semantic prefix must select the same gate as a .ttl file there"
        );
    }

    #[test]
    fn no_changes_selects_no_work() {
        let (selected, reasons) = select_tasks(&[], TASKS);
        assert!(selected.is_empty());
        assert!(reasons.is_empty());
    }
}
