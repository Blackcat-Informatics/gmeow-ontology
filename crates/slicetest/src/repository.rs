// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One cached producer verdict for every slice-resident declarative specification.
//!
//! The historical `datatest-stable` harness exposed one repository-discovering test
//! case per spec file. Under nextest that meant many process launches and repeated
//! reconstruction of the same merged ontology. More importantly, a test executable
//! retained a path to corpus construction. This module moves the complete sweep to the
//! explicit pre-test producer boundary. Each spec is a content-addressed DAG action
//! covering only the files its executor can read; an aggregate action binds every task
//! receipt and the repository-wide input census. Warm producers and all verifiers load
//! immutable verdicts with no execution callback.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity, STORE_FORMAT_VERSION,
    StoreLimits, VerifiedEntry, bytes_digest,
};
use gmeow_errors::{Diag, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::error::CellAggregate;
use crate::{exec, paths};

const ACTION: &str = "slice-spec-verdict";
const CODEC: &str = "gmeow-slice-spec-verdict-json-v3";
const TASK_CODEC: &str = "gmeow-slice-spec-task-verdict-json-v1";
const VERDICT_SCHEMA_VERSION: u32 = 3;
const TASK_VERDICT_SCHEMA_VERSION: u32 = 1;

/// Deterministic census carried by the authenticated all-specs verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceSpecVerdict {
    pub schema_version: u32,
    pub input_files: usize,
    pub competency_files: usize,
    pub structural_files: usize,
    pub conformance_files: usize,
    pub flagship_manifests: usize,
    /// Exact receipts for every independently cached declarative spec node.
    pub task_receipts: Vec<SliceSpecTaskIdentity>,
}

impl SliceSpecVerdict {
    /// Total fixed-name declarative spec files covered by this verdict.
    #[must_use]
    pub const fn spec_files(&self) -> usize {
        self.competency_files + self.structural_files + self.conformance_files
    }
}

/// Stable identity of one independently cached declarative-spec action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SliceSpecTaskIdentity {
    pub kind: String,
    pub path: String,
    pub action_key: String,
    pub receipt_digest: String,
}

/// Authenticated verdict identity plus whether this invocation executed the sweep.
#[derive(Debug, Clone)]
pub struct SliceSpecOutcome {
    pub action_key: String,
    pub receipt_digest: String,
    pub built: bool,
    pub verdict: SliceSpecVerdict,
}

/// Executor lane selected by the private, producer-authorized worker command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceSpecKind {
    Competency,
    Structural,
    Conformance,
}

impl SliceSpecKind {
    /// Stable CLI/cache spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Competency => "competency",
            Self::Structural => "structural",
            Self::Conformance => "conformance",
        }
    }

    /// Parse the private worker spelling.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "competency" => Ok(Self::Competency),
            "structural" => Ok(Self::Structural),
            "conformance" => Ok(Self::Conformance),
            _ => Err(fail(format!("unknown slice-spec worker kind {value:?}"))),
        }
    }
}

#[derive(Debug, Clone)]
struct SpecTask {
    kind: SliceSpecKind,
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SpecTaskVerdict {
    schema_version: u32,
    kind: String,
    path: String,
}

// Native SPARQL evaluation over the merged graph expands a compact Turtle source set
// into indexes, bindings, and result arenas. A 32-worker cold run over the current
// 12.9 MiB source set peaked near 129 GiB, so CPU count alone is not an admission
// signal. Scale the competency wave from live available memory and exact source bytes;
// this is deliberately an input-relative budget, never a fixed thread ceiling.
const MERGED_QUERY_EXPANSION_FACTOR: u64 = 256;
// Structural evaluator memory is dominated by query-plan/result arenas, not Turtle
// bytes. Empirical cold runs showed that same-process workers retained more than
// 80 GiB cumulatively, while an isolated large grounding spec peaked below 200 MiB
// and returned it on exit. A 4 GiB admission budget leaves over 20x that observed
// footprint per child plus the scheduler's separate 50% host reserve.
const MIN_STRUCTURAL_WORKER_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MIN_MERGED_QUERY_WORKER_BYTES: u64 = 512 * 1024 * 1024;

fn fail(detail: impl Into<String>) -> Diag {
    Diag::of_kind(CellAggregate {
        detail: detail.into(),
    })
}

/// Execute and publish the complete verdict on an exact-input miss.
///
/// A cache hit authenticates and returns the prior verdict without invoking a spec
/// executor. This function is producer-only; test code is statically forbidden from
/// calling it by the repository corpus-purity scanner.
pub fn produce_repository_verdict(
    repo_root: &Path,
    implementation_fingerprint: &str,
) -> Result<SliceSpecOutcome> {
    ensure_compiled_repo(repo_root)?;
    let context = action_context(repo_root, implementation_fingerprint)?;
    let store = ActionStore::open(
        ActionStore::default_root(repo_root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(format!("open slice-spec action store: {error}")))?;

    if let Some(entry) = store
        .get::<SliceSpecVerdict>(&context)
        .map_err(|error| fail(format!("load slice-spec verdict: {error}")))?
    {
        return outcome_from_entry(entry, false, context.inputs.len());
    }

    let verdict = execute_repository_specs(
        repo_root,
        implementation_fingerprint,
        &store,
        context.inputs.len(),
    )?;
    let bytes = serde_json::to_vec(&verdict)
        .map_err(|error| fail(format!("encode slice-spec verdict: {error}")))?;
    let receipt = store
        .publish(&context, bytes_digest(&bytes), verdict.clone(), &bytes)
        .map_err(|error| fail(format!("publish slice-spec verdict: {error}")))?;
    Ok(SliceSpecOutcome {
        action_key: receipt.action_key.as_str().to_owned(),
        receipt_digest: receipt.digest(),
        built: true,
        verdict,
    })
}

/// Authenticate the exact all-specs verdict without any producer fallback.
pub fn verify_cached(
    repo_root: &Path,
    implementation_fingerprint: &str,
) -> Result<SliceSpecOutcome> {
    ensure_compiled_repo(repo_root)?;
    let context = action_context(repo_root, implementation_fingerprint)?;
    let store = ActionStore::open_existing_read_only(
        ActionStore::default_root(repo_root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(format!("open slice-spec action store read-only: {error}")))?;
    let entry = store
        .get::<SliceSpecVerdict>(&context)
        .map_err(|error| fail(format!("authenticate slice-spec verdict: {error}")))?
        .ok_or_else(|| {
            fail(
                "authenticated slice-spec verdict is absent; a test-facing verifier may not execute the repository sweep",
            )
        })?;
    outcome_from_entry(entry, false, context.inputs.len())
}

fn outcome_from_entry(
    entry: VerifiedEntry<SliceSpecVerdict>,
    built: bool,
    expected_inputs: usize,
) -> Result<SliceSpecOutcome> {
    let decoded: SliceSpecVerdict = serde_json::from_slice(&entry.bytes)
        .map_err(|error| fail(format!("decode slice-spec verdict product: {error}")))?;
    if decoded != entry.receipt.payload {
        return Err(fail(
            "slice-spec receipt payload differs from its authenticated product",
        ));
    }
    if decoded.schema_version != VERDICT_SCHEMA_VERSION {
        return Err(fail(format!(
            "slice-spec verdict schema {} != {VERDICT_SCHEMA_VERSION}",
            decoded.schema_version
        )));
    }
    if decoded.input_files != expected_inputs {
        return Err(fail(format!(
            "slice-spec verdict input census {} != current exact-input census {expected_inputs}",
            decoded.input_files
        )));
    }
    if decoded.spec_files() == 0 {
        return Err(fail("slice-spec verdict covers no declarative spec files"));
    }
    if decoded.task_receipts.len() != decoded.spec_files() {
        return Err(fail(format!(
            "slice-spec verdict carries {} task receipts for {} declarative specs",
            decoded.task_receipts.len(),
            decoded.spec_files()
        )));
    }
    let identities = decoded
        .task_receipts
        .iter()
        .map(|identity| (&identity.kind, &identity.path, &identity.action_key))
        .collect::<BTreeSet<_>>();
    if identities.len() != decoded.task_receipts.len() {
        return Err(fail(
            "slice-spec verdict carries duplicate task receipt identities",
        ));
    }
    if entry.receipt.product_digest != bytes_digest(&entry.bytes) {
        return Err(fail(
            "slice-spec receipt semantic product digest does not match its bytes",
        ));
    }
    Ok(SliceSpecOutcome {
        action_key: entry.receipt.action_key.as_str().to_owned(),
        receipt_digest: entry.receipt.digest(),
        built,
        verdict: decoded,
    })
}

fn ensure_compiled_repo(repo_root: &Path) -> Result<()> {
    let requested = repo_root
        .canonicalize()
        .map_err(|error| fail(format!("canonicalize repository root: {error}")))?;
    let compiled = paths::repo_root();
    if requested != compiled {
        return Err(fail(format!(
            "slice-spec producer root {} differs from compiled repository root {}",
            requested.display(),
            compiled.display()
        )));
    }
    Ok(())
}

fn action_context(repo_root: &Path, implementation_fingerprint: &str) -> Result<ActionContext> {
    if implementation_fingerprint.is_empty() {
        return Err(fail(
            "slice-spec producer implementation fingerprint is empty",
        ));
    }
    let inputs = repository_inputs(repo_root)?;
    Ok(ActionContext::new(
        "test-fixtures",
        ACTION,
        ProducerIdentity::new(format!(
            "{implementation_fingerprint}:gmeow-slicetest:{CODEC}"
        )),
        CODEC,
        inputs,
    ))
}

fn task_action_context(
    repo_root: &Path,
    implementation_fingerprint: &str,
    task: &SpecTask,
) -> Result<ActionContext> {
    let logical_path = logical_path(repo_root, &task.path)?;
    Ok(ActionContext::new(
        "test-fixtures",
        format!("slice-spec-{}", task.kind.name()),
        ProducerIdentity::new(format!(
            "{implementation_fingerprint}:gmeow-slicetest:{TASK_CODEC}"
        )),
        TASK_CODEC,
        task_inputs(repo_root, task)?,
    )
    .with_dimension("kind", task.kind.name())
    .with_dimension("spec", logical_path))
}

fn task_inputs(repo_root: &Path, task: &SpecTask) -> Result<Vec<ActionInput>> {
    let mut files = vec![task.path.clone()];
    let spec = crate::dsl::load_spec(&task.path)?;
    let slice_dir = paths::slice_dir(&task.path);

    match task.kind {
        SliceSpecKind::Competency => {
            files.push(required_file(repo_root, "ontology/gmeow.ttl")?);
            collect_matching(&repo_root.join("slices"), &mut files, &|relative| {
                relative.file_name().and_then(OsStr::to_str) == Some("module.ttl")
            })?;
            for question in &spec.competency {
                if let Some(relative) = &question.query_file {
                    files.push(repo_root.join(relative));
                }
                if let Some(relative) = &question.project_query_file {
                    files.push(repo_root.join(relative));
                }
                if let Some(relative) = &question.data_file {
                    files.push(slice_dir.join(relative));
                }
            }
            if spec
                .competency
                .iter()
                .any(|question| question.reasoning == crate::dsl::ReasoningProfile::Native)
            {
                let math_examples = repo_root.join("slices/grounding/math/examples");
                files.extend(
                    [
                        "algebra-axioms.ttl",
                        "algebra-homomorphisms.ttl",
                        "e8-symmetry.ttl",
                        "homomorphic-encryption.ttl",
                        "chain-complex.ttl",
                    ]
                    .into_iter()
                    .map(|name| math_examples.join(name)),
                );
            }
        }
        SliceSpecKind::Structural => {
            files.push(paths::module_file(&slice_dir));
            if spec
                .structural
                .iter()
                .any(|assertion| assertion.scope == crate::dsl::Scope::ModuleAndExamples)
            {
                files.extend(direct_turtle_files(&paths::examples_dir(&slice_dir))?);
            }
            files.extend(
                spec.structural
                    .iter()
                    .filter_map(|assertion| assertion.fail_witness.as_ref())
                    .map(|relative| slice_dir.join(relative)),
            );
        }
        SliceSpecKind::Conformance => {
            files.extend(paths::conformance_module_files(&slice_dir));
            files.extend(paths::shapes_files(&slice_dir));
            files.extend(
                spec.conformance
                    .iter()
                    .map(|cell| slice_dir.join(&cell.file)),
            );
        }
    }

    files.sort();
    files.dedup();
    files
        .iter()
        .map(|path| raw_input(repo_root, path))
        .collect()
}

fn direct_turtle_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(fail(format!(
                "enumerate exact slice-spec inputs under {}: {error}",
                directory.display()
            )));
        }
    };
    let mut files = entries
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| {
            fail(format!(
                "enumerate exact slice-spec inputs under {}: {error}",
                directory.display()
            ))
        })?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|suffix| suffix == "ttl"))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn logical_path(repo_root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(repo_root)
        .map(|relative| relative.to_string_lossy().into_owned())
        .map_err(|_| {
            fail(format!(
                "slice-spec path escaped repository root: {}",
                path.display()
            ))
        })
}

fn repository_inputs(repo_root: &Path) -> Result<Vec<ActionInput>> {
    let mut paths = vec![
        required_file(repo_root, "ontology/gmeow.ttl")?,
        required_file(repo_root, "generated/shapes/validation-shapes.ttl")?,
        required_file(repo_root, "generated/shapes/constraint-shapes.ttl")?,
        required_file(repo_root, "generated/shapes/procedural-constraints.ttl")?,
    ];
    collect_matching(&repo_root.join("slices"), &mut paths, &|relative| {
        let file_name = relative.file_name().and_then(OsStr::to_str);
        let extension = relative.extension().and_then(OsStr::to_str);
        let under = |name: &str| {
            relative
                .parent()
                .is_some_and(|parent| parent.components().any(|part| part.as_os_str() == name))
        };
        matches!(file_name, Some("module.ttl" | "shapes.ttl"))
            || extension == Some("ttl") && (under("tests") || under("examples"))
            || extension == Some("rq") && under("queries")
    })?;
    collect_matching(&repo_root.join("queries"), &mut paths, &|relative| {
        relative
            .extension()
            .is_some_and(|extension| extension == "rq")
    })?;
    paths.sort();
    paths.dedup();
    paths
        .iter()
        .map(|path| raw_input(repo_root, path))
        .collect()
}

fn required_file(repo_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = repo_root.join(relative);
    if !path.is_file() {
        return Err(fail(format!(
            "required slice-spec input is absent: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn collect_matching(
    root: &Path,
    out: &mut Vec<PathBuf>,
    include: &impl Fn(&Path) -> bool,
) -> Result<()> {
    fn visit(
        base: &Path,
        directory: &Path,
        out: &mut Vec<PathBuf>,
        include: &impl Fn(&Path) -> bool,
    ) -> Result<()> {
        let mut entries = std::fs::read_dir(directory)
            .map_err(|error| fail(format!("enumerate {}: {error}", directory.display())))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| fail(format!("enumerate {}: {error}", directory.display())))?;
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| fail(format!("inspect {}: {error}", path.display())))?;
            if metadata.file_type().is_dir() {
                visit(base, &path, out, include)?;
            } else {
                let relative = path.strip_prefix(base).unwrap_or(&path);
                if include(relative) {
                    out.push(path);
                }
            }
        }
        Ok(())
    }

    if !root.is_dir() {
        return Err(fail(format!(
            "required slice-spec input tree is absent: {}",
            root.display()
        )));
    }
    visit(root, root, out, include)
}

fn raw_input(repo_root: &Path, path: &Path) -> Result<ActionInput> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| fail(format!("inspect exact input {}: {error}", path.display())))?;
    let logical_path = path
        .strip_prefix(repo_root)
        .map_err(|_| fail(format!("input escaped repository root: {}", path.display())))?
        .to_string_lossy()
        .into_owned();
    let (file_kind, executable, identity_bytes) = if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(path)
            .map_err(|error| fail(format!("read symlink {}: {error}", path.display())))?;
        let content = std::fs::read(path)
            .map_err(|error| fail(format!("read symlink target {}: {error}", path.display())))?;
        let mut identity = target.as_os_str().as_encoded_bytes().to_vec();
        identity.push(0);
        identity.extend(content);
        (FileKind::Symlink, false, identity)
    } else if metadata.file_type().is_file() {
        let bytes = std::fs::read(path)
            .map_err(|error| fail(format!("read exact input {}: {error}", path.display())))?;
        (FileKind::File, is_executable(&metadata), bytes)
    } else {
        return Err(fail(format!(
            "slice-spec input is neither a regular file nor a symlink: {}",
            path.display()
        )));
    };
    Ok(ActionInput::Raw {
        logical_path,
        file_kind,
        executable,
        digest: bytes_digest(&identity_bytes),
    })
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

fn execute_repository_specs(
    repo_root: &Path,
    implementation_fingerprint: &str,
    store: &ActionStore,
    input_files: usize,
) -> Result<SliceSpecVerdict> {
    let tasks = discover_specs(repo_root)?;
    let competency_files = tasks
        .iter()
        .filter(|task| task.kind == SliceSpecKind::Competency)
        .count();
    let structural_files = tasks
        .iter()
        .filter(|task| task.kind == SliceSpecKind::Structural)
        .count();
    let conformance_files = tasks
        .iter()
        .filter(|task| task.kind == SliceSpecKind::Conformance)
        .count();

    let mut misses = Vec::new();
    let mut hits = 0usize;
    for task in &tasks {
        let context = task_action_context(repo_root, implementation_fingerprint, task)?;
        if store
            .get::<SpecTaskVerdict>(&context)
            .map_err(|error| {
                fail(format!(
                    "load {} slice-spec receipt {}: {error}",
                    task.kind.name(),
                    task.path.display()
                ))
            })?
            .is_some()
        {
            hits += 1;
        } else {
            misses.push(task.clone());
        }
    }
    println!(
        "slice-spec DAG: tasks={} receipt_hits={hits} misses={}",
        tasks.len(),
        misses.len()
    );
    // `cargo clippy` and doctest compilation are independent siblings of this
    // producer after the shared Rust build. Cargo may atomically relink
    // `target/debug/gmeow-dev` while those siblings run; resolving `current_exe()`
    // separately for every child therefore leaves a transient deleted-path race.
    // Pin the already-running executable once, before any child starts, and launch
    // every worker through that immutable content-addressed hard link. A complete
    // receipt hit launches no child and therefore creates no executable snapshot.
    let worker_executable = if misses.is_empty() {
        None
    } else {
        Some(pin_worker_executable(repo_root)?)
    };

    for kind in [
        SliceSpecKind::Competency,
        SliceSpecKind::Structural,
        SliceSpecKind::Conformance,
    ] {
        let wave = misses
            .iter()
            .filter(|task| task.kind == kind)
            .cloned()
            .collect::<Vec<_>>();
        if wave.is_empty() {
            println!(
                "slice-spec producer: phase={} state=receipt-hit files={}",
                kind.name(),
                tasks.iter().filter(|task| task.kind == kind).count()
            );
            continue;
        }

        match kind {
            SliceSpecKind::Competency => {
                let source_bytes = merged_graph_source_bytes(repo_root)?;
                let per_worker_bytes = source_bytes
                    .saturating_mul(MERGED_QUERY_EXPANSION_FACTOR)
                    .max(MIN_MERGED_QUERY_WORKER_BYTES);
                let (workers, available) =
                    memory_admitted_workers(wave.len(), natural_workers(), per_worker_bytes);
                println!(
                    "slice-spec producer: phase=competency state=started misses={} workers={workers} available_memory_bytes={} merged_source_bytes={source_bytes} per_worker_budget_bytes={per_worker_bytes}",
                    wave.len(),
                    available.map_or_else(|| "unknown".to_owned(), |bytes| bytes.to_string())
                );
                run_worker_process(
                    worker_executable
                        .as_deref()
                        .expect("a miss pins one worker executable"),
                    repo_root,
                    implementation_fingerprint,
                    kind,
                    &wave,
                    Some(workers),
                )?;
            }
            SliceSpecKind::Structural => {
                let (workers, available) = memory_admitted_workers(
                    wave.len(),
                    natural_workers(),
                    MIN_STRUCTURAL_WORKER_BYTES,
                );
                println!(
                    "slice-spec producer: phase=structural state=started misses={} workers={workers} available_memory_bytes={} per_worker_budget_bytes={MIN_STRUCTURAL_WORKER_BYTES}",
                    wave.len(),
                    available.map_or_else(|| "unknown".to_owned(), |bytes| bytes.to_string())
                );
                run_isolated_worker_processes(
                    worker_executable
                        .as_deref()
                        .expect("a miss pins one worker executable"),
                    repo_root,
                    implementation_fingerprint,
                    kind,
                    &wave,
                    workers,
                )?;
            }
            SliceSpecKind::Conformance => {
                // One conformance file already fans its cells across every available CPU.
                // Keep the outer DAG serial and isolate each file so completed arenas are
                // returned to the OS before the next full-width child starts.
                println!(
                    "slice-spec producer: phase=conformance state=started misses={} inner_workers=num-cpus",
                    wave.len()
                );
                run_isolated_worker_processes(
                    worker_executable
                        .as_deref()
                        .expect("a miss pins one worker executable"),
                    repo_root,
                    implementation_fingerprint,
                    kind,
                    &wave,
                    1,
                )?;
            }
        }
        println!("slice-spec producer: phase={} state=complete", kind.name());
    }

    let task_receipts = tasks
        .iter()
        .map(|task| load_task_identity(repo_root, implementation_fingerprint, store, task))
        .collect::<Result<Vec<_>>>()?;

    validate_flagship_manifests(repo_root)?;

    Ok(SliceSpecVerdict {
        schema_version: VERDICT_SCHEMA_VERSION,
        input_files,
        competency_files,
        structural_files,
        conformance_files,
        flagship_manifests: 3,
        task_receipts,
    })
}

/// Pin this producer's already-running executable before concurrent Cargo siblings can
/// relink their shared output path.
///
/// A temporary hard link captures one inode when the runner and repository cache share a
/// filesystem. Managed build directories may live on another filesystem; only `EXDEV`
/// falls back to an exclusive byte copy into the repository-local temporary path. Its
/// SHA-256 then names the durable link. Reusing an existing final path is permitted only
/// after re-hashing it to the same exact identity.
fn pin_worker_executable(repo_root: &Path) -> Result<PathBuf> {
    let executable = std::env::current_exe()
        .map_err(|error| fail(format!("resolve slice-spec worker executable: {error}")))?;
    let directory = repo_root
        .join(".cache")
        .join("gmeow-test-fixtures")
        .join("workers");
    std::fs::create_dir_all(&directory).map_err(|error| {
        fail(format!(
            "create immutable slice-spec worker directory {}: {error}",
            directory.display()
        ))
    })?;
    let temporary = directory.join(format!(
        ".gmeow-dev-{}-{}.tmp",
        std::process::id(),
        std::thread::current().name().unwrap_or("producer")
    ));
    snapshot_worker_executable(&executable, &temporary).map_err(|error| {
        fail(format!(
            "pin running slice-spec worker {} as {}: {error}",
            executable.display(),
            temporary.display()
        ))
    })?;

    let digest = executable_sha256(&temporary).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })?;
    let pinned = directory.join(format!(
        "gmeow-dev-{digest}{}",
        std::env::consts::EXE_SUFFIX
    ));
    match std::fs::hard_link(&temporary, &pinned) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = executable_sha256(&pinned)?;
            if existing != digest {
                let _ = std::fs::remove_file(&temporary);
                return Err(fail(format!(
                    "immutable slice-spec worker identity collision at {}: expected {digest}, got {existing}",
                    pinned.display()
                )));
            }
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            return Err(fail(format!(
                "publish immutable slice-spec worker {}: {error}",
                pinned.display()
            )));
        }
    }
    std::fs::remove_file(&temporary).map_err(|error| {
        fail(format!(
            "remove temporary slice-spec worker link {}: {error}",
            temporary.display()
        ))
    })?;
    Ok(pinned)
}

fn snapshot_worker_executable(executable: &Path, temporary: &Path) -> std::io::Result<()> {
    snapshot_worker_executable_with(executable, temporary, |source, destination| {
        std::fs::hard_link(source, destination)
    })
}

fn snapshot_worker_executable_with(
    executable: &Path,
    temporary: &Path,
    hard_link: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match hard_link(executable, temporary) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_worker_executable(executable, temporary)
        }
        Err(error) => Err(error),
    }
}

fn copy_worker_executable(executable: &Path, temporary: &Path) -> std::io::Result<()> {
    let result = (|| {
        let mut source = std::fs::File::open(executable)?;
        let permissions = source.metadata()?.permissions();
        let mut target = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(temporary)?;
        std::io::copy(&mut source, &mut target)?;
        target.set_permissions(permissions)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

fn executable_sha256(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|error| {
        fail(format!(
            "open worker executable {}: {error}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| {
            fail(format!(
                "hash worker executable {}: {error}",
                path.display()
            ))
        })?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn load_task_identity(
    repo_root: &Path,
    implementation_fingerprint: &str,
    store: &ActionStore,
    task: &SpecTask,
) -> Result<SliceSpecTaskIdentity> {
    let context = task_action_context(repo_root, implementation_fingerprint, task)?;
    let entry = store
        .get::<SpecTaskVerdict>(&context)
        .map_err(|error| {
            fail(format!(
                "authenticate {} slice-spec receipt {}: {error}",
                task.kind.name(),
                task.path.display()
            ))
        })?
        .ok_or_else(|| {
            fail(format!(
                "slice-spec worker returned without publishing {}",
                task.path.display()
            ))
        })?;
    task_identity_from_entry(repo_root, task, entry)
}

fn task_identity_from_entry(
    repo_root: &Path,
    task: &SpecTask,
    entry: VerifiedEntry<SpecTaskVerdict>,
) -> Result<SliceSpecTaskIdentity> {
    let decoded: SpecTaskVerdict = serde_json::from_slice(&entry.bytes)
        .map_err(|error| fail(format!("decode slice-spec task verdict: {error}")))?;
    let expected = SpecTaskVerdict {
        schema_version: TASK_VERDICT_SCHEMA_VERSION,
        kind: task.kind.name().to_owned(),
        path: logical_path(repo_root, &task.path)?,
    };
    if decoded != expected || decoded != entry.receipt.payload {
        return Err(fail(format!(
            "slice-spec task verdict identity mismatch for {}",
            task.path.display()
        )));
    }
    if entry.receipt.product_digest != bytes_digest(&entry.bytes) {
        return Err(fail(format!(
            "slice-spec task product digest mismatch for {}",
            task.path.display()
        )));
    }
    Ok(SliceSpecTaskIdentity {
        kind: decoded.kind,
        path: decoded.path,
        action_key: entry.receipt.action_key.as_str().to_owned(),
        receipt_digest: entry.receipt.digest(),
    })
}

fn run_isolated_worker_processes(
    executable: &Path,
    repo_root: &Path,
    implementation_fingerprint: &str,
    kind: SliceSpecKind,
    tasks: &[SpecTask],
    admitted_workers: usize,
) -> Result<()> {
    let workers = admitted_workers.max(1).min(tasks.len().max(1));
    let failures = std::thread::scope(|scope| {
        (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    tasks
                        .iter()
                        .enumerate()
                        .filter(|(index, _)| index % workers == worker)
                        .filter_map(|(_, task)| {
                            run_worker_process(
                                executable,
                                repo_root,
                                implementation_fingerprint,
                                kind,
                                std::slice::from_ref(task),
                                None,
                            )
                            .err()
                            .map(|error| format!("{}: {error}", task.path.display()))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .enumerate()
            .flat_map(|(worker, handle)| match handle.join() {
                Ok(failures) => failures,
                Err(_) => vec![format!("isolated slice-spec launcher {worker} panicked")],
            })
            .collect::<Vec<_>>()
    });
    if failures.is_empty() {
        return Ok(());
    }
    Err(fail(format!(
        "{} isolated {} worker(s) failed:\n{}",
        failures.len(),
        kind.name(),
        failures.join("\n")
    )))
}

fn run_worker_process(
    executable: &Path,
    repo_root: &Path,
    implementation_fingerprint: &str,
    kind: SliceSpecKind,
    tasks: &[SpecTask],
    inner_workers: Option<usize>,
) -> Result<()> {
    let mut command = Command::new(executable);
    command
        .current_dir(repo_root)
        .env(
            "GMEOW_SLICE_SPEC_WORKER_AUTHORITY",
            implementation_fingerprint,
        )
        .arg("slice-spec-worker")
        .arg("--kind")
        .arg(kind.name());
    if let Some(workers) = inner_workers {
        command.arg("--workers").arg(workers.to_string());
    }
    for task in tasks {
        command.arg("--spec").arg(&task.path);
    }
    let status = command.status().map_err(|error| {
        fail(format!(
            "launch isolated {} worker through {}: {error}",
            kind.name(),
            executable.display()
        ))
    })?;
    if !status.success() {
        return Err(fail(format!(
            "isolated {} worker exited with {status}",
            kind.name()
        )));
    }
    Ok(())
}

fn validate_flagship_manifests(repo_root: &Path) -> Result<()> {
    const MANIFESTS: [(&str, [&str; 5]); 3] = [
        (
            "lang",
            [
                "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/sentenceToFormula",
                "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/proseSelfReading",
                "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/docsAsTranslation",
                "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/serializationsAsGrammars",
                "https://blackcatinformatics.ca/gmeow/examples/lang/acceptance/ambiguityHeldHonestly",
            ],
        ),
        (
            "logic",
            [
                "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/elRlDlClosure",
                "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/correspondenceSection",
                "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/counterfactualStratumC",
                "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/symmetricConjecture",
                "https://blackcatinformatics.ca/gmeow/examples/logic/acceptance/chaseTerminationCertificate",
            ],
        ),
        (
            "math",
            [
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/e8Symmetry",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/homomorphicEncryption",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/proofAsProcess",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/rBridge",
                "https://blackcatinformatics.ca/gmeow/examples/math/acceptance/aiSelfStructure",
            ],
        ),
    ];

    for (slice, canonical) in MANIFESTS {
        let slice_dir = repo_root.join("slices/grounding").join(slice);
        crate::flagship::validate_flagship_manifest(&slice_dir, &canonical)
            .map_err(|detail| fail(format!("{slice} flagship manifest: {detail}")))?;
    }
    Ok(())
}

fn discover_specs(repo_root: &Path) -> Result<Vec<SpecTask>> {
    let slices = repo_root.join("slices");
    let mut paths = Vec::new();
    collect_matching(&slices, &mut paths, &|relative| {
        relative
            .parent()
            .is_some_and(|parent| parent.file_name().is_some_and(|name| name == "tests"))
            && matches!(
                relative.file_name().and_then(OsStr::to_str),
                Some("competency.ttl" | "structural.ttl" | "example-conformance.ttl")
            )
    })?;
    paths.sort();
    let tasks = paths
        .into_iter()
        .filter_map(|path| {
            let kind = match path.file_name().and_then(OsStr::to_str)? {
                "competency.ttl" => SliceSpecKind::Competency,
                "structural.ttl" => SliceSpecKind::Structural,
                "example-conformance.ttl" => SliceSpecKind::Conformance,
                _ => return None,
            };
            Some(SpecTask { kind, path })
        })
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return Err(fail("no declarative slice spec files were discovered"));
    }
    Ok(tasks)
}

/// Execute and durably publish exact task receipts inside a private producer worker.
///
/// The public producer launches this through the hidden `gmeow-dev slice-spec-worker`
/// command. Tests are statically forbidden from invoking that command or this entry
/// point. A successful task is published immediately, so a later task failure or host
/// interruption never discards completed work.
pub fn execute_worker(
    repo_root: &Path,
    implementation_fingerprint: &str,
    kind: SliceSpecKind,
    requested_paths: &[PathBuf],
    admitted_workers: usize,
) -> Result<()> {
    ensure_compiled_repo(repo_root)?;
    if requested_paths.is_empty() {
        return Err(fail("slice-spec worker received no exact spec paths"));
    }
    let discovered = discover_specs(repo_root)?;
    let mut selected = Vec::with_capacity(requested_paths.len());
    let mut seen = BTreeSet::new();
    for requested in requested_paths {
        let candidate = if requested.is_absolute() {
            requested.clone()
        } else {
            repo_root.join(requested)
        };
        let canonical = candidate.canonicalize().map_err(|error| {
            fail(format!(
                "canonicalize requested slice-spec worker path {}: {error}",
                candidate.display()
            ))
        })?;
        if !seen.insert(canonical.clone()) {
            return Err(fail(format!(
                "slice-spec worker received duplicate path {}",
                canonical.display()
            )));
        }
        let task = discovered
            .iter()
            .find(|task| task.path == canonical)
            .ok_or_else(|| {
                fail(format!(
                    "slice-spec worker path is not a discovered fixed-name spec: {}",
                    canonical.display()
                ))
            })?;
        if task.kind != kind {
            return Err(fail(format!(
                "slice-spec worker kind {} does not match {}",
                kind.name(),
                canonical.display()
            )));
        }
        selected.push(task.clone());
    }

    let store = ActionStore::open_existing_writable(
        ActionStore::default_root(repo_root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(format!("join slice-spec worker action store: {error}")))?;
    let mut misses = Vec::new();
    for task in &selected {
        let context = task_action_context(repo_root, implementation_fingerprint, task)?;
        match store
            .get::<SpecTaskVerdict>(&context)
            .map_err(|error| fail(format!("load worker task receipt: {error}")))?
        {
            Some(entry) => {
                let identity = task_identity_from_entry(repo_root, task, entry)?;
                println!(
                    "slice-spec task: mode=receipt-hit kind={} path={} action={} receipt={}",
                    identity.kind, identity.path, identity.action_key, identity.receipt_digest
                );
            }
            None => misses.push(task.clone()),
        }
    }
    if misses.is_empty() {
        return Ok(());
    }

    let workers = admitted_workers.max(1).min(misses.len());
    if workers == 1 {
        let mut failures = Vec::new();
        for task in &misses {
            match run_task(task) {
                Ok(()) => {
                    publish_task_receipt(repo_root, implementation_fingerprint, &store, task)?;
                }
                Err(error) => failures.push(format!("{}: {error}", task.path.display())),
            }
        }
        if failures.is_empty() {
            return Ok(());
        }
        return Err(fail(format!(
            "{} {} slice-spec task(s) failed:\n{}",
            failures.len(),
            kind.name(),
            failures.join("\n")
        )));
    }

    let (sender, receiver) = std::sync::mpsc::channel::<(usize, Result<()>)>();
    let failures = std::thread::scope(|scope| -> Result<Vec<String>> {
        let handles = (0..workers)
            .map(|worker| {
                let sender = sender.clone();
                let misses = &misses;
                scope.spawn(move || {
                    for (index, task) in misses.iter().enumerate() {
                        if index % workers == worker {
                            let result = run_task(task);
                            if sender.send((index, result)).is_err() {
                                return;
                            }
                        }
                    }
                })
            })
            .collect::<Vec<_>>();
        drop(sender);

        let mut failures = Vec::new();
        for (index, result) in receiver {
            let task = &misses[index];
            match result {
                Ok(()) => {
                    publish_task_receipt(repo_root, implementation_fingerprint, &store, task)?
                }
                Err(error) => failures.push(format!("{}: {error}", task.path.display())),
            }
        }
        for (worker, handle) in handles.into_iter().enumerate() {
            if handle.join().is_err() {
                failures.push(format!("slice-spec executor thread {worker} panicked"));
            }
        }
        Ok(failures)
    })?;
    if failures.is_empty() {
        return Ok(());
    }
    Err(fail(format!(
        "{} {} slice-spec task(s) failed:\n{}",
        failures.len(),
        kind.name(),
        failures.join("\n")
    )))
}

fn publish_task_receipt(
    repo_root: &Path,
    implementation_fingerprint: &str,
    store: &ActionStore,
    task: &SpecTask,
) -> Result<()> {
    let context = task_action_context(repo_root, implementation_fingerprint, task)?;
    let payload = SpecTaskVerdict {
        schema_version: TASK_VERDICT_SCHEMA_VERSION,
        kind: task.kind.name().to_owned(),
        path: logical_path(repo_root, &task.path)?,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| fail(format!("encode slice-spec task verdict: {error}")))?;
    let receipt = store
        .publish(&context, bytes_digest(&bytes), payload.clone(), &bytes)
        .map_err(|error| {
            fail(format!(
                "publish {} slice-spec task {}: {error}",
                task.kind.name(),
                task.path.display()
            ))
        })?;
    println!(
        "slice-spec task: mode=built kind={} path={} action={} receipt={}",
        payload.kind,
        payload.path,
        receipt.action_key,
        receipt.digest()
    );
    Ok(())
}

fn natural_workers() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

fn memory_admitted_workers(
    task_count: usize,
    logical_workers: usize,
    per_worker_bytes: u64,
) -> (usize, Option<u64>) {
    let available = available_memory_bytes();
    let memory_workers = available.map_or(logical_workers, |bytes| {
        usize::try_from((bytes / 2) / per_worker_bytes)
            .unwrap_or(usize::MAX)
            .max(1)
    });
    (
        logical_workers.min(memory_workers).min(task_count.max(1)),
        available,
    )
}

fn merged_graph_source_bytes(repo_root: &Path) -> Result<u64> {
    let mut sources = vec![required_file(repo_root, "ontology/gmeow.ttl")?];
    collect_matching(&repo_root.join("slices"), &mut sources, &|relative| {
        relative.file_name().and_then(OsStr::to_str) == Some("module.ttl")
    })?;
    sources.sort();
    sources.dedup();
    sources.into_iter().try_fold(0u64, |total, path| {
        let bytes = std::fs::metadata(&path)
            .map_err(|error| fail(format!("measure merged source {}: {error}", path.display())))?
            .len();
        Ok(total.saturating_add(bytes))
    })
}

fn available_memory_bytes() -> Option<u64> {
    let host = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let mut fields = line.split_whitespace();
                if fields.next()? != "MemAvailable:" {
                    return None;
                }
                fields
                    .next()?
                    .parse::<u64>()
                    .ok()
                    .map(|kibibytes| kibibytes.saturating_mul(1024))
            })
        });
    let cgroup = current_cgroup_available_bytes();
    match (host, cgroup) {
        (Some(host), Some(cgroup)) => Some(host.min(cgroup)),
        (Some(host), None) => Some(host),
        (None, Some(cgroup)) => Some(cgroup),
        (None, None) => None,
    }
}

fn current_cgroup_available_bytes() -> Option<u64> {
    let membership = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let relative = membership
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?
        .trim_start_matches('/');
    let directory = Path::new("/sys/fs/cgroup").join(relative);
    let maximum = std::fs::read_to_string(directory.join("memory.max"))
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    let current = std::fs::read_to_string(directory.join("memory.current"))
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(maximum.saturating_sub(current))
}

fn run_task(task: &SpecTask) -> Result<()> {
    match task.kind {
        SliceSpecKind::Competency => exec::run_competency_file(&task.path),
        SliceSpecKind::Structural => exec::run_structural_file(&task.path),
        SliceSpecKind::Conformance => exec::run_conformance_file(&task.path),
    }
}

#[cfg(test)]
mod tests {
    use std::io::ErrorKind;

    use tempfile::tempdir;

    use super::snapshot_worker_executable_with;

    #[test]
    fn worker_snapshot_copies_exact_bytes_when_hard_links_cross_devices() {
        let source_dir = tempdir().expect("source tempdir");
        let cache_dir = tempdir().expect("cache tempdir");
        let source = source_dir.path().join("gmeow-dev");
        let snapshot = cache_dir.path().join(".gmeow-dev.tmp");
        let bytes = b"exact worker executable bytes\0\xff";
        std::fs::write(&source, bytes).expect("write worker fixture");

        snapshot_worker_executable_with(&source, &snapshot, |_, _| {
            Err(std::io::Error::from(ErrorKind::CrossesDevices))
        })
        .expect("cross-device worker snapshot falls back to an exact copy");

        assert_eq!(
            std::fs::read(&snapshot).expect("read worker snapshot"),
            bytes
        );
        assert_eq!(
            std::fs::metadata(&snapshot)
                .expect("snapshot metadata")
                .permissions(),
            std::fs::metadata(&source)
                .expect("source metadata")
                .permissions()
        );
    }

    #[test]
    fn worker_snapshot_fails_closed_for_other_link_errors() {
        let directory = tempdir().expect("tempdir");
        let source = directory.path().join("gmeow-dev");
        let snapshot = directory.path().join(".gmeow-dev.tmp");
        std::fs::write(&source, b"worker").expect("write worker fixture");

        let error = snapshot_worker_executable_with(&source, &snapshot, |_, _| {
            Err(std::io::Error::from(ErrorKind::PermissionDenied))
        })
        .expect_err("non-EXDEV link failure must not copy");

        assert_eq!(error.kind(), ErrorKind::PermissionDenied);
        assert!(!snapshot.exists(), "failed snapshot must leave no bytes");
    }
}
