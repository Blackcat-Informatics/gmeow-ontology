// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-cli-core` — the shared CLI foundation reused by both the consumer
//! `gmeow` binary and the repo-maintenance `gmeow-dev` binary.
//!
//! It owns the cross-cutting surface neither bin should reimplement:
//!
//! * [`ConsoleMode`] — the closed enum choosing the output surface, with the
//!   DX precedence rule (flag > env > default) and the non-TTY-agents-get-JSONL
//!   default resolution.
//! * [`DiagnosticsConfig`] — the resolved diagnostics output policy (console
//!   mode, artifact kinds, directory, stem, category) with the same flag > env
//!   > default precedence.
//! * [`Reporter`] — a small, object-safe trait over "how do I surface a
//!   [`gmeow_errors::Report`], progress, and a run summary", with a
//!   human-facing ([`HumanReporter`]) and a machine-facing ([`NdjsonReporter`])
//!   implementation.
//! * [`exit_code`] — the 0/1 process exit convention over a report.
//! * [`init_tracing`] — the idempotent stderr `tracing` subscriber install.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use gmeow_errors::render;
use gmeow_errors::{Diag, DiagLedger, Finding, Report, StageId};
use serde::Serialize;

pub mod error;

use error::{
    DocsProjectionFailed, EmptyArtifactSelection, UnknownArtifactKind, UnknownConsoleMode,
};

/// The output surface a CLI run presents on, resolved once at startup.
///
/// A CLOSED enum: these are the only surfaces the CLI supports, and adding one
/// is a deliberate, breaking vocabulary change (never a silent fallback). The
/// [`clap::ValueEnum`] derive makes it a first-class `--console <mode>` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ConsoleMode {
    /// Pick a concrete surface from the environment: [`ConsoleMode::Pretty`] on
    /// an interactive TTY, [`ConsoleMode::Jsonl`] otherwise (the DX rule — a
    /// non-TTY agent, pipe, or CI log wants machine-readable output by default).
    Auto,
    /// Rich, colored, human-facing terminal output.
    Pretty,
    /// Plain, uncolored human-facing text.
    Text,
    /// One JSON object per line (NDJSON), for agents and log pipelines.
    Jsonl,
    /// Suppress all diagnostic chrome (product results only).
    Silent,
}

impl ConsoleMode {
    /// Return the canonical kebab-case spelling of this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            ConsoleMode::Auto => "auto",
            ConsoleMode::Pretty => "pretty",
            ConsoleMode::Text => "text",
            ConsoleMode::Jsonl => "jsonl",
            ConsoleMode::Silent => "silent",
        }
    }

    /// Resolve the effective console mode from, in precedence order: an explicit
    /// `--console` flag, then an environment value (`auto|pretty|text|jsonl|silent`),
    /// then the default ([`ConsoleMode::Auto`]).
    ///
    /// [`ConsoleMode::Auto`] then collapses to a concrete surface:
    /// [`ConsoleMode::Pretty`] when `is_tty`, else [`ConsoleMode::Jsonl`]. An
    /// unrecognized env value is ignored (falls through to the default) rather
    /// than hard-failing — the flag remains the authoritative override.
    pub fn resolve(flag: Option<ConsoleMode>, env_val: Option<&str>, is_tty: bool) -> ConsoleMode {
        let chosen = flag
            .or_else(|| env_val.and_then(Self::parse_env))
            .unwrap_or(ConsoleMode::Auto);
        match chosen {
            ConsoleMode::Auto if is_tty => ConsoleMode::Pretty,
            ConsoleMode::Auto => ConsoleMode::Jsonl,
            other => other,
        }
    }

    /// Like [`ConsoleMode::resolve`], but [`ConsoleMode::Auto`] collapses to a
    /// HUMAN stderr surface off a TTY ([`ConsoleMode::Text`]) instead of
    /// [`ConsoleMode::Jsonl`].
    ///
    /// This is the resolution a CONSUMER CLI (`gmeow`) wants: stdout is the
    /// product stream (a converted document, a projected graph, the MCP
    /// JSON-RPC transport), so diagnostics must stay on stderr by default and
    /// never interleave NDJSON into piped product output. An agent still opts
    /// into the machine surface explicitly with `--console jsonl` (or
    /// `GMEOW_CONSOLE=jsonl`); the flag > env > default precedence is identical
    /// to [`ConsoleMode::resolve`].
    pub fn resolve_stderr_default(
        flag: Option<ConsoleMode>,
        env_val: Option<&str>,
        is_tty: bool,
    ) -> ConsoleMode {
        let chosen = flag
            .or_else(|| env_val.and_then(Self::parse_env))
            .unwrap_or(ConsoleMode::Auto);
        match chosen {
            ConsoleMode::Auto if is_tty => ConsoleMode::Pretty,
            ConsoleMode::Auto => ConsoleMode::Text,
            other => other,
        }
    }

    /// Parse an environment/config spelling into a mode, case-insensitively.
    /// Returns `None` for an unknown value so the caller can fall through to the
    /// next precedence tier.
    fn parse_env(value: &str) -> Option<ConsoleMode> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ConsoleMode::Auto),
            "pretty" => Some(ConsoleMode::Pretty),
            "text" => Some(ConsoleMode::Text),
            "jsonl" => Some(ConsoleMode::Jsonl),
            "silent" => Some(ConsoleMode::Silent),
            _ => None,
        }
    }
}

/// Resolved diagnostics output policy (immutable).
///
/// This is the Rust twin of the retired `src/gmeow_tools/diagnostics_config.py`.
/// It owns *where* diagnostics go and *how* they are projected to the console —
/// console mode, artifact files, output directory, filename stem, and the stable
/// code-scanning category. The precedence rule is **flag > env > default** for
/// every knob.
///
/// Note: the diagnostics-specific auto mode collapses to [`ConsoleMode::Text`]
/// off a TTY (matching the original Python policy), whereas the general CLI
/// [`ConsoleMode::resolve`] collapses to [`ConsoleMode::Jsonl`] off a TTY. The
/// two surfaces have different defaults because the diagnostics rail is
/// consumed by humans reading CI logs by default, while the general CLI console
/// is consumed by agents/pipes that want machine-readable NDJSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticsConfig {
    /// How findings are projected to the console.
    pub console: ConsoleMode,
    /// Which artifact projections to write.
    pub artifacts: BTreeSet<String>,
    /// Directory receiving artifact files.
    pub directory: PathBuf,
    /// Filename stem for artifact files.
    pub stem: String,
    /// Stable code-scanning category (e.g. `gmeow`, `lint`, `rust`).
    pub category: String,
}

impl DiagnosticsConfig {
    /// The three artifact projections, in deterministic write order.
    pub const ARTIFACT_KINDS: &[&str] = &["json", "sarif", "html"];
    /// Default filename stem.
    pub const DEFAULT_STEM: &str = "gmeow-feedback";
    /// Default code-scanning category.
    pub const DEFAULT_CATEGORY: &str = "gmeow";

    /// Resolve the output policy from flags, environment, and defaults.
    ///
    /// Precedence is **flag > env > default** for every knob. `auto` resolves by
    /// `is_tty`: [`ConsoleMode::Pretty`] on a TTY, [`ConsoleMode::Text`]
    /// otherwise. Invalid `console`/`artifacts` tokens raise rather than fall
    /// back.
    ///
    /// `dist_dir` is the project `dist/` root used when no explicit directory is
    /// supplied. An explicit `--diagnostics-category` (or env category) scopes
    /// the default directory to `dist/diagnostics/<category>/`; otherwise the
    /// flat `dist/` convention is preserved.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve(
        console: Option<&str>,
        artifacts: Option<&str>,
        directory: Option<&Path>,
        stem: Option<&str>,
        category: Option<&str>,
        env: &HashMap<String, String>,
        is_tty: bool,
        dist_dir: &Path,
    ) -> gmeow_errors::Result<Self> {
        let console = Self::resolve_console(
            console
                .or_else(|| env.get("GMEOW_DIAGNOSTICS_CONSOLE").map(String::as_str))
                .unwrap_or("auto"),
            is_tty,
        )?;

        let artifacts = Self::parse_artifacts(
            artifacts
                .or_else(|| env.get("GMEOW_DIAGNOSTICS_ARTIFACTS").map(String::as_str))
                .unwrap_or("all"),
        )?;

        let category_flag = category;
        let resolved_category = category_flag
            .or_else(|| env.get("GMEOW_DIAGNOSTICS_CATEGORY").map(String::as_str))
            .unwrap_or(Self::DEFAULT_CATEGORY)
            .to_owned();

        let stem = stem
            .or_else(|| env.get("GMEOW_DIAGNOSTICS_STEM").map(String::as_str))
            .unwrap_or(Self::DEFAULT_STEM)
            .to_owned();

        // Directory precedence: an explicit flag or env dir is used verbatim.
        // Otherwise the default is keyed on whether a *category* was explicitly
        // requested: an aggregate/manual run (no category) keeps the flat
        // `dist/` convention, while a category run lands under
        // `dist/diagnostics/<category>/` so per-job artifacts never collide.
        let explicit_dir = directory
            .map(Path::to_path_buf)
            .or_else(|| env.get("GMEOW_DIAGNOSTICS_DIR").map(PathBuf::from));
        let category_explicit = category_flag.is_some()
            || env
                .get("GMEOW_DIAGNOSTICS_CATEGORY")
                .is_some_and(|s| !s.is_empty());
        let directory = if let Some(dir) = explicit_dir {
            dir
        } else if category_explicit {
            dist_dir.join("diagnostics").join(&resolved_category)
        } else {
            dist_dir.to_path_buf()
        };

        Ok(Self {
            console,
            artifacts,
            directory,
            stem,
            category: resolved_category,
        })
    }

    fn resolve_console(raw: &str, is_tty: bool) -> gmeow_errors::Result<ConsoleMode> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(if is_tty {
                ConsoleMode::Pretty
            } else {
                ConsoleMode::Text
            }),
            "pretty" => Ok(ConsoleMode::Pretty),
            "text" => Ok(ConsoleMode::Text),
            "jsonl" => Ok(ConsoleMode::Jsonl),
            "silent" => Ok(ConsoleMode::Silent),
            other => Err(Diag::of_kind(UnknownConsoleMode {
                value: other.to_owned(),
            })),
        }
    }

    fn parse_artifacts(raw: &str) -> gmeow_errors::Result<BTreeSet<String>> {
        let token = raw.trim().to_ascii_lowercase();
        if token == "none" {
            return Ok(BTreeSet::new());
        }
        if token == "all" {
            return Ok(Self::ARTIFACT_KINDS.iter().map(|&s| s.to_owned()).collect());
        }
        let kinds: BTreeSet<String> = token
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
        if kinds.is_empty() {
            return Err(Diag::of_kind(EmptyArtifactSelection {
                raw: raw.to_owned(),
            }));
        }
        let known: BTreeSet<String> = Self::ARTIFACT_KINDS.iter().map(|&s| s.to_owned()).collect();
        let unknown: Vec<String> = kinds.difference(&known).cloned().collect();
        if !unknown.is_empty() {
            return Err(Diag::of_kind(UnknownArtifactKind {
                unknown: unknown.join(", "),
                expected: known.into_iter().collect::<Vec<_>>().join(", "),
            }));
        }
        Ok(kinds)
    }
}

/// The documentation projection `export-docs` writes, shared by the consumer
/// `gmeow` and repo-maintenance `gmeow-dev` binaries — one closed vocabulary,
/// never two copies drifting apart.
#[derive(Debug, Clone, clap::ValueEnum)]
pub enum ExportFormat {
    /// The browsable HTML ontology-docs site (one language subtree).
    Site,
    /// The mdbook source tree (`book.toml`, `SUMMARY.md`, `src/…`; English-only).
    Mdbook,
    /// The Typst print projection (`gmeow.pdf`, `gmeow.typ`; English-only).
    Pdf,
    /// The flattened prompt-ready per-term card snippets (`terms/<slug>.md`).
    Snippets,
    /// The generated Pydantic v2 model package (`gmeow_models/…`) — the functional
    /// documentation surface (importing the models IS reading the ontology).
    Pydantic,
    /// Every projection, each under its own subdirectory of the output directory.
    All,
}

/// Filesystem reconciliation accounting for one rendered documentation tree.
///
/// `unchanged` means the existing bytes matched exactly, so the file was not
/// opened for writing and its inode/mtime stayed untouched. `removed` counts
/// stale files formerly owned by the projection; empty-directory pruning is not
/// counted separately.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct DocsProjectionReport {
    /// Files in the canonical rendered tree.
    pub produced: usize,
    /// Missing or byte-different files written to disk.
    pub written: usize,
    /// Byte-identical files deliberately left untouched.
    pub unchanged: usize,
    /// Stale projection-owned files removed from disk.
    pub removed: usize,
}

/// Write one docs projection tree into `dir`, reporting the confirmations error on
/// a fold/selection failure and any I/O error on write. Returns the process exit
/// code: `0` on success, `1` on a handled failure.
///
/// Every relative member path (`rel`) originates from a user-supplied `--gts`
/// snapshot, so it is untrusted input: BEFORE joining it under `dir`, this
/// rejects any member that is an absolute path or carries a `..`
/// ([`Component::ParentDir`]) component. Either shape can escape `dir` via
/// [`Path::join`] (an absolute `rel` replaces `dir` outright; a `..` component
/// walks back out of it), so both are a hard failure — never a silent skip —
/// naming the offending member.
pub fn write_docs_projection(
    dir: &Path,
    tree: Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag>,
) -> i32 {
    let tree = match tree {
        Ok(t) => t,
        Err(e) => return fail(format!("cannot create docs tree: {e}")),
    };
    write_docs_projection_tree(dir, &tree)
}

/// Reconcile an already-rendered documentation tree to `dir` without taking
/// ownership. This lets one canonical site render feed multiple destinations
/// without cloning its byte payloads or rendering twice.
pub fn write_docs_projection_tree(dir: &Path, tree: &BTreeMap<String, Vec<u8>>) -> i32 {
    match reconcile_docs_projection_tree(dir, tree) {
        Ok(_) => {
            println!("docs -> {}", dir.display());
            0
        }
        Err(diag) => fail(diag.to_string()),
    }
}

/// Reconcile an already-rendered documentation tree and return exact filesystem
/// accounting. Unlike [`write_docs_projection_tree`], this result-bearing form
/// lets a larger synchronization command include docs in its aggregate
/// idempotency report.
pub fn reconcile_docs_projection_tree(
    dir: &Path,
    tree: &BTreeMap<String, Vec<u8>>,
) -> Result<DocsProjectionReport, Diag> {
    let mut expected = BTreeSet::new();
    let mut report = DocsProjectionReport {
        produced: tree.len(),
        ..DocsProjectionReport::default()
    };
    for (rel, data) in tree {
        let rel_path = Path::new(rel);
        if rel_path.is_absolute()
            || rel_path
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(docs_projection_diag(format!(
                "refusing to write docs member outside the export directory: {rel:?}"
            )));
        }
        expected.insert(rel_path.to_path_buf());
        let target = dir.join(rel_path);
        // Never follow a pre-existing symlink while reconciling an untrusted
        // projection path. Replace the link itself with the projected regular file.
        if std::fs::symlink_metadata(&target).is_ok_and(|meta| meta.file_type().is_symlink())
            && let Err(e) = std::fs::remove_file(&target)
        {
            return Err(docs_projection_diag(format!(
                "cannot replace symlink {}: {e}",
                target.display()
            )));
        }
        // Idempotency policy: an equal projection is already synchronized. Do not
        // rewrite it (or touch its mtime/inode), which keeps warm docs runs cheap
        // and prevents downstream rebuilds triggered only by filesystem churn.
        match std::fs::read(&target) {
            Ok(existing) if existing == *data => {
                report.unchanged += 1;
                continue;
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(docs_projection_diag(format!(
                    "cannot read {}: {e}",
                    target.display()
                )));
            }
        }
        if let Some(parent) = target.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return Err(docs_projection_diag(format!(
                "cannot create {}: {e}",
                parent.display()
            )));
        }
        if let Err(e) = std::fs::write(&target, data) {
            return Err(docs_projection_diag(format!(
                "cannot write {}: {e}",
                target.display()
            )));
        }
        report.written += 1;
    }

    // A projection tree owns its destination. Remove stale members so update mode
    // reaches a fixed point even when a canonical page disappears. This runs after
    // all writes, never follows directory symlinks, and leaves byte-identical files
    // untouched.
    let mut existing_files = Vec::new();
    let mut existing_dirs = Vec::new();
    if let Err(e) = collect_projection_members(dir, dir, &mut existing_files, &mut existing_dirs) {
        return Err(docs_projection_diag(format!(
            "cannot inspect {}: {e}",
            dir.display()
        )));
    }
    for (rel, path) in existing_files {
        if !expected.contains(&rel) {
            if let Err(e) = std::fs::remove_file(&path) {
                return Err(docs_projection_diag(format!(
                    "cannot remove stale docs member {}: {e}",
                    path.display()
                )));
            }
            report.removed += 1;
        }
    }
    existing_dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in existing_dirs {
        match std::fs::remove_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(docs_projection_diag(format!(
                    "cannot prune {}: {e}",
                    path.display()
                )));
            }
        }
    }
    Ok(report)
}

fn collect_projection_members(
    root: &Path,
    dir: &Path,
    files: &mut Vec<(PathBuf, PathBuf)>,
    dirs: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_projection_members(root, &path, files, dirs)?;
            dirs.push(path);
        } else {
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            files.push((rel, path));
        }
    }
    Ok(())
}

/// Emit an Error-grade diagnostic through the console sink and yield the failure
/// exit code `1`.
///
/// The shared implementation the two bins' own `fail` helpers mirror;
/// [`write_docs_projection`] uses it directly so it never needs to thread a
/// caller-supplied error sink through. It resolves a reporter from the
/// environment (honouring `GMEOW_CONSOLE` and the stderr TTY) so the docs-export
/// error surfaces as the same graded witness every other diagnostic does — human
/// text on a TTY, an NDJSON `finding` line for agents — never a bare stderr write.
fn docs_projection_diag(message: impl Into<String>) -> Diag {
    Diag::of_kind(DocsProjectionFailed {
        detail: message.into(),
    })
}

fn fail(message: impl AsRef<str>) -> i32 {
    use std::io::IsTerminal;
    let diag = docs_projection_diag(message.as_ref());
    let mode = ConsoleMode::resolve(
        None,
        std::env::var("GMEOW_CONSOLE").ok().as_deref(),
        std::io::stderr().is_terminal(),
    );
    emit_and_exit(reporter_for(mode).as_ref(), diag, "gmeow")
}

/// How a CLI run surfaces its diagnostics, progress, and closing summary.
///
/// Deliberately small and object-safe (`&dyn Reporter`) so the two bins can hold
/// a boxed reporter chosen at startup from the resolved [`ConsoleMode`] without
/// threading a generic through every command. Product results (the actual
/// answer a command computes) go to stdout by the command itself; a `Reporter`
/// owns the *diagnostic* channel (stderr for humans, stdout NDJSON for agents).
pub trait Reporter: Send + Sync {
    /// Surface a completed diagnostics report.
    fn report(&self, report: &Report);

    /// Mark the start of a named pipeline stage.
    fn stage_start(&self, stage: &str);

    /// Mark the end of a named pipeline stage, with its wall-clock duration.
    fn stage_end(&self, stage: &str, elapsed: Duration);

    /// Emit a one-line run summary (counts, verdict).
    fn summary(&self, report: &Report);
}

/// A human-facing reporter: diagnostics render to stderr as colored text (via
/// [`gmeow_errors::render::to_text`]), leaving stdout clear for product
/// results.
#[derive(Debug, Default, Clone, Copy)]
pub struct HumanReporter;

impl HumanReporter {
    /// Construct a human reporter.
    pub fn new() -> Self {
        Self
    }

    /// The `anstyle` style for a severity — red errors, yellow warnings, dimmed
    /// notes/info — applied to the stderr diagnostic block.
    fn verdict_style(ok: bool) -> anstyle::Style {
        let color = if ok {
            anstyle::AnsiColor::Green
        } else {
            anstyle::AnsiColor::Red
        };
        anstyle::Style::new()
            .bold()
            .fg_color(Some(anstyle::Color::Ansi(color)))
    }
}

impl Reporter for HumanReporter {
    fn report(&self, report: &Report) {
        let text = render::to_text(report);
        if !text.is_empty() {
            let mut err = anstream::stderr();
            let _ = writeln!(err, "{text}");
        }
    }

    fn stage_start(&self, stage: &str) {
        let mut err = anstream::stderr();
        let _ = writeln!(err, "→ {stage}");
    }

    fn stage_end(&self, stage: &str, elapsed: Duration) {
        let mut err = anstream::stderr();
        let _ = writeln!(err, "✓ {stage} ({} ms)", elapsed.as_millis());
    }

    fn summary(&self, report: &Report) {
        let style = Self::verdict_style(report.ok());
        let verdict = if report.ok() { "ok" } else { "failed" };
        let mut err = anstream::stderr();
        let _ = writeln!(
            err,
            "{style}{}: {verdict}{style:#} ({} error(s), {} warning(s))",
            report.tool,
            report.error_count(),
            report.warning_count(),
        );
    }
}

/// The line-framed NDJSON envelope every [`NdjsonReporter`] event serializes to.
///
/// The `event` tag is the stable discriminator (`stage_start`, `stage_end`,
/// `finding`, `summary`). A `finding` event embeds the canonical
/// [`gmeow_errors::Finding`] serde form verbatim under `finding` — there is
/// no second finding schema.
#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum NdjsonEvent<'a> {
    StageStart {
        stage: &'a str,
    },
    StageEnd {
        stage: &'a str,
        elapsed_ms: u128,
    },
    Finding {
        tool: &'a str,
        finding: &'a Finding,
    },
    Summary {
        tool: &'a str,
        ok: bool,
        errors: usize,
        warnings: usize,
    },
}

/// A machine-facing reporter: one JSON object per line to stdout, each carrying
/// a stable `event` field. Findings are line-framed from the canonical
/// [`gmeow_errors::Finding`] serde form — the same schema the JSON/SARIF
/// renderers project — so an agent consuming the stream never has to reconcile a
/// second shape.
#[derive(Debug, Default, Clone, Copy)]
pub struct NdjsonReporter;

impl NdjsonReporter {
    /// Construct an NDJSON reporter.
    pub fn new() -> Self {
        Self
    }

    /// Serialize one event as a single line to stdout.
    fn emit(&self, event: &NdjsonEvent<'_>) {
        if let Ok(line) = serde_json::to_string(event) {
            let mut out = std::io::stdout();
            let _ = writeln!(out, "{line}");
        }
    }
}

impl Reporter for NdjsonReporter {
    fn report(&self, report: &Report) {
        for finding in &report.normalized().findings {
            self.emit(&NdjsonEvent::Finding {
                tool: &report.tool,
                finding,
            });
        }
    }

    fn stage_start(&self, stage: &str) {
        self.emit(&NdjsonEvent::StageStart { stage });
    }

    fn stage_end(&self, stage: &str, elapsed: Duration) {
        self.emit(&NdjsonEvent::StageEnd {
            stage,
            elapsed_ms: elapsed.as_millis(),
        });
    }

    fn summary(&self, report: &Report) {
        self.emit(&NdjsonEvent::Summary {
            tool: &report.tool,
            ok: report.ok(),
            errors: report.error_count(),
            warnings: report.warning_count(),
        });
    }
}

/// A reporter that suppresses all diagnostic chrome (the `silent` surface):
/// every event is dropped so only product results (written by the command
/// itself) reach stdout.
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentReporter;

impl Reporter for SilentReporter {
    fn report(&self, _report: &Report) {}
    fn stage_start(&self, _stage: &str) {}
    fn stage_end(&self, _stage: &str, _elapsed: Duration) {}
    fn summary(&self, _report: &Report) {}
}

/// A boxed [`Reporter`] for a resolved [`ConsoleMode`]: line-framed NDJSON for
/// `jsonl` (agents/pipelines), a silent sink for `silent`, and human-facing
/// stderr text for every interactive/`pretty`/`text` surface. This is the single
/// reporter factory both the consumer `gmeow` binary and the repo-maintenance
/// `gmeow-dev` binary construct their startup reporter from.
pub fn reporter_for(mode: ConsoleMode) -> Box<dyn Reporter> {
    match mode {
        ConsoleMode::Jsonl => Box::new(NdjsonReporter::new()),
        ConsoleMode::Silent => Box::new(SilentReporter),
        _ => Box::new(HumanReporter::new()),
    }
}

/// Lower a single [`Diag`] to a one-finding [`Report`] WITHOUT a pipeline carrier.
///
/// A CLI or config error can arise *before* any pipeline carrier (and its
/// [`DiagLedger`]) exists — yet it must still be emitted as the same graded
/// witness a mid-pipeline finding is, not a bare string. The canonical
/// Diag→Finding lowering lives on the ledger projection
/// ([`DiagLedger::project_report`]), so this constructor stands up a fresh local
/// ledger, attaches the one diagnostic stamped with the emitting `tool` as its
/// stage, and returns the projected, normalized report. The single diagnostic
/// keeps its grade, so [`Report::ok`] / [`exit_code`] read the same gate a
/// carrier-borne finding would.
pub fn report_diag(diag: Diag, tool: &str) -> Report {
    let mut ledger = DiagLedger::new();
    ledger.attach(diag, StageId::new(tool));
    ledger.project_report(tool).normalized()
}

/// Emit one pre-carrier [`Diag`] through `reporter` and return the process exit
/// code it maps to — the one-call CLI/config-error path built on
/// [`report_diag`] and [`exit_code`].
pub fn emit_and_exit(reporter: &dyn Reporter, diag: Diag, tool: &str) -> i32 {
    let report = report_diag(diag, tool);
    reporter.report(&report);
    exit_code(&report)
}

/// Route a NON-GATING **note** — a progress/status/chatter line — through
/// `reporter` as a [`FindingCategory::Transient`](gmeow_errors::FindingCategory)
/// logging witness. This is the substrate replacement idiom for a bare
/// stderr chatter line: the message becomes a graded (Note-severity,
/// Advisory, Transient) [`Diag::note`], is lowered to a one-finding [`Report`]
/// (via [`report_diag`], stamped with `tool` as its stage), and is surfaced on
/// the reporter's channel — human text on stderr, an NDJSON `finding` line on
/// stdout, or dropped by a silent sink. It NEVER gates ([`Report::ok`] stays
/// true), so it is safe for pure narration.
///
/// `code` is the stable finding-code string the witness carries (interned once,
/// idempotently); reuse a per-area `<crate>.<area>.note` code for a family of
/// related chatter and let the message carry the specifics.
pub fn note(reporter: &dyn Reporter, tool: &str, code: &str, message: impl Into<String>) {
    let diag = Diag::note(gmeow_errors::code::register_code(code), message);
    reporter.report(&report_diag(diag, tool));
}

/// Route a NON-GATING **info** witness — the lowest-severity chatter — through
/// `reporter`. The [`info`] twin of [`note`]: identical routing, an
/// [`Severity::Info`](gmeow_errors::Severity) [`Diag::info`] grade instead of
/// Note. Never gates.
pub fn info(reporter: &dyn Reporter, tool: &str, code: &str, message: impl Into<String>) {
    let diag = Diag::info(gmeow_errors::code::register_code(code), message);
    reporter.report(&report_diag(diag, tool));
}

/// The process exit code a report maps to: `0` when the report is clean
/// ([`Report::ok`]), else `1`.
///
/// This preserves the 0/1/2 convention: `2` is reserved for clap usage errors,
/// which clap emits itself (this function never returns it).
pub fn exit_code(report: &Report) -> i32 {
    if report.ok() { 0 } else { 1 }
}

/// Install a stderr `tracing` subscriber with an `EnvFilter` sourced from
/// `GMEOW_LOG` (preferred) then `RUST_LOG`, defaulting to `warn`.
///
/// Idempotent: uses `try_init` and swallows the already-initialized error, so
/// both `main`s and any test may call it without a double-install panic.
pub fn init_tracing() {
    let filter = std::env::var("GMEOW_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| "warn".to_owned());
    let env_filter = tracing_subscriber::EnvFilter::try_new(filter)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    // `try_init` returns Err if a global subscriber is already set — ignore it so
    // this stays idempotent across bins and tests.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_resolves_to_pretty_on_a_tty() {
        assert_eq!(ConsoleMode::resolve(None, None, true), ConsoleMode::Pretty);
    }

    #[test]
    fn auto_resolves_to_jsonl_off_a_tty() {
        // The DX rule: a non-TTY agent/pipe gets machine-readable output.
        assert_eq!(ConsoleMode::resolve(None, None, false), ConsoleMode::Jsonl);
    }

    #[test]
    fn env_overrides_the_default() {
        assert_eq!(
            ConsoleMode::resolve(None, Some("text"), true),
            ConsoleMode::Text
        );
        // Case-insensitive, whitespace-trimmed.
        assert_eq!(
            ConsoleMode::resolve(None, Some(" Silent "), false),
            ConsoleMode::Silent
        );
    }

    #[test]
    fn flag_wins_over_env() {
        assert_eq!(
            ConsoleMode::resolve(Some(ConsoleMode::Pretty), Some("jsonl"), false),
            ConsoleMode::Pretty
        );
    }

    #[test]
    fn unknown_env_falls_through_to_default() {
        // An unrecognized env value is ignored, not a hard-fail: Auto default
        // then resolves by TTY.
        assert_eq!(
            ConsoleMode::resolve(None, Some("garbage"), true),
            ConsoleMode::Pretty
        );
        assert_eq!(
            ConsoleMode::resolve(None, Some("garbage"), false),
            ConsoleMode::Jsonl
        );
    }

    #[test]
    fn exit_code_maps_ok_and_failure() {
        let clean = Report::new("t");
        assert_eq!(exit_code(&clean), 0);
        let mut failed = Report::new("t");
        failed.add_finding(Finding::new(gmeow_errors::Severity::Error, "x", "boom"));
        assert_eq!(exit_code(&failed), 1);
    }

    #[test]
    fn write_docs_projection_writes_a_clean_tree() {
        let (_tmp, tmp) = tempdir();
        let mut tree = BTreeMap::new();
        tree.insert("a/b.md".to_owned(), b"hello".to_vec());
        let code = write_docs_projection(&tmp, Ok(tree));
        assert_eq!(code, 0);
        assert_eq!(
            std::fs::read(tmp.join("a/b.md")).unwrap(),
            b"hello".to_vec()
        );
    }

    #[test]
    fn write_docs_projection_removes_stale_members() {
        let (_tmp, tmp) = tempdir();
        std::fs::create_dir_all(tmp.join("stale/nested")).unwrap();
        std::fs::write(tmp.join("stale/nested/old.md"), b"old").unwrap();
        let tree = BTreeMap::from([("live.md".to_owned(), b"live".to_vec())]);
        assert_eq!(write_docs_projection(&tmp, Ok(tree)), 0);
        assert_eq!(std::fs::read(tmp.join("live.md")).unwrap(), b"live");
        assert!(!tmp.join("stale/nested/old.md").exists());
        assert!(!tmp.join("stale").exists());
    }

    #[test]
    fn docs_projection_report_accounts_for_write_skip_and_removal() {
        let (_tmp, tmp) = tempdir();
        std::fs::write(tmp.join("same.md"), b"same").unwrap();
        std::fs::write(tmp.join("changed.md"), b"old").unwrap();
        std::fs::write(tmp.join("stale.md"), b"stale").unwrap();
        let tree = BTreeMap::from([
            ("changed.md".to_owned(), b"new".to_vec()),
            ("same.md".to_owned(), b"same".to_vec()),
        ]);

        let report = reconcile_docs_projection_tree(&tmp, &tree).unwrap();

        assert_eq!(
            report,
            DocsProjectionReport {
                produced: 2,
                written: 1,
                unchanged: 1,
                removed: 1,
            }
        );
        assert_eq!(std::fs::read(tmp.join("changed.md")).unwrap(), b"new");
        assert!(!tmp.join("stale.md").exists());
    }

    #[test]
    fn write_docs_projection_does_not_touch_equal_files() {
        let (_tmp, tmp) = tempdir();
        let tree = BTreeMap::from([("same.md".to_owned(), b"same".to_vec())]);
        assert_eq!(write_docs_projection(&tmp, Ok(tree.clone())), 0);
        let before = std::fs::metadata(tmp.join("same.md"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(write_docs_projection(&tmp, Ok(tree)), 0);
        let after = std::fs::metadata(tmp.join("same.md"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after, "equal docs output had its mtime touched");
    }

    #[test]
    fn write_docs_projection_rejects_absolute_member_paths() {
        let (_tmp, tmp) = tempdir();
        let mut tree = BTreeMap::new();
        // An absolute member would replace `dir` outright under `Path::join`,
        // escaping the export directory entirely.
        tree.insert("/etc/passwd".to_owned(), b"pwned".to_vec());
        let code = write_docs_projection(&tmp, Ok(tree));
        assert_eq!(code, 1);
        assert!(!Path::new("/etc/passwd_gmeow_test_marker").exists());
    }

    #[test]
    fn write_docs_projection_rejects_parent_dir_traversal() {
        let (_tmp, tmp) = tempdir();
        let mut tree = BTreeMap::new();
        tree.insert("../escape.md".to_owned(), b"pwned".to_vec());
        let code = write_docs_projection(&tmp, Ok(tree));
        assert_eq!(code, 1);
        assert!(!tmp.parent().unwrap().join("escape.md").exists());
    }

    /// A fresh temp directory for a single test, owned by a [`tempfile::TempDir`]
    /// so it is removed when the guard drops — on success, on panic, and on early
    /// return. The caller must bind the guard (`let (_tmp, dir) = tempdir();`);
    /// binding it to a bare `_` would drop it immediately and delete the directory
    /// out from under the test. The working root is a child of the guard's
    /// directory, so even a path-traversal escape one level up stays inside the
    /// cleaned-up tree.
    fn tempdir() -> (tempfile::TempDir, PathBuf) {
        let guard = tempfile::tempdir().expect("create temp dir");
        let dir = guard.path().join("gmeow-cli-core-test");
        std::fs::create_dir_all(&dir).unwrap();
        (guard, dir)
    }

    #[test]
    fn report_diag_of_an_error_grade_gates() {
        // An Error-grade pre-carrier Diag lowers to a report that is NOT ok and
        // exits 1 — the same gate a carrier-borne Error finding hits.
        use gmeow_errors::grade::{FindingCategory, Grade, Severity, Standpoint};
        let code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.error");
        let diag = Diag::new(
            code,
            Grade::new(
                Severity::Error,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
            "config could not be resolved",
        );
        let report = report_diag(diag, "gmeow");
        assert!(!report.ok());
        assert_eq!(report.error_count(), 1);
        assert_eq!(exit_code(&report), 1);
    }

    #[test]
    fn report_diag_of_transient_chatter_is_clean() {
        // A Note/Info Transient chatter Diag lowers to an ok report and exits 0 —
        // chatter never gates.
        let code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.note");
        let report = report_diag(Diag::note(code, "just narrating progress"), "gmeow");
        assert!(report.ok());
        assert_eq!(exit_code(&report), 0);

        let info_code = gmeow_errors::code::register_code("test.cli-core.pre-carrier.info");
        let info_report = report_diag(Diag::info(info_code, "low-severity witness"), "gmeow");
        assert!(info_report.ok());
        assert_eq!(exit_code(&info_report), 0);
    }
}
