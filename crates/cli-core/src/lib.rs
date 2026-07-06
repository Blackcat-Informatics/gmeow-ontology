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

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gmeow_errors::render;
use gmeow_errors::{Finding, Report};
use serde::Serialize;

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

/// Errors raised while resolving a [`DiagnosticsConfig`].
///
/// Invalid tokens are hard failures: the diagnostics policy has no silent
/// fallback, so a typo cannot silently degrade output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiagnosticsConfigError {
    /// An unrecognized `--diagnostics-console` / `GMEOW_DIAGNOSTICS_CONSOLE` token.
    #[error("unknown diagnostics console mode: {0:?}")]
    UnknownConsoleMode(String),
    /// One or more entries in an artifact selector are not known kinds.
    #[error(
        "unknown diagnostics artifact kind(s): {unknown:?} \
         (expected a subset of {expected:?}, or 'none'/'all')"
    )]
    UnknownArtifactKind {
        /// The unrecognized token(s) from the selector.
        unknown: Vec<String>,
        /// The canonical artifact kinds the selector may name.
        expected: Vec<String>,
    },
    /// The artifact selector parsed to an empty set.
    #[error("empty diagnostics artifact selection: {0:?}")]
    EmptyArtifactSelection(String),
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
    ) -> Result<Self, DiagnosticsConfigError> {
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

    fn resolve_console(raw: &str, is_tty: bool) -> Result<ConsoleMode, DiagnosticsConfigError> {
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
            other => Err(DiagnosticsConfigError::UnknownConsoleMode(other.to_owned())),
        }
    }

    fn parse_artifacts(raw: &str) -> Result<BTreeSet<String>, DiagnosticsConfigError> {
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
            return Err(DiagnosticsConfigError::EmptyArtifactSelection(
                raw.to_owned(),
            ));
        }
        let known: BTreeSet<String> = Self::ARTIFACT_KINDS.iter().map(|&s| s.to_owned()).collect();
        let unknown: Vec<String> = kinds.difference(&known).cloned().collect();
        if !unknown.is_empty() {
            return Err(DiagnosticsConfigError::UnknownArtifactKind {
                unknown,
                expected: known.into_iter().collect(),
            });
        }
        Ok(kinds)
    }
}

/// How a CLI run surfaces its diagnostics, progress, and closing summary.
///
/// Deliberately small and object-safe (`&dyn Reporter`) so the two bins can hold
/// a boxed reporter chosen at startup from the resolved [`ConsoleMode`] without
/// threading a generic through every command. Product results (the actual
/// answer a command computes) go to stdout by the command itself; a `Reporter`
/// owns the *diagnostic* channel (stderr for humans, stdout NDJSON for agents).
pub trait Reporter {
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
}
