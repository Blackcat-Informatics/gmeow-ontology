// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev-cli` — the repo-maintenance `gmeow-dev` command surface.
//!
//! Unlike the consumer `gmeow` binary (which reads an embedded bundle), every
//! command here NEEDS the repository: regeneration, fanout, reasoning,
//! verification, mappings, and the i18n toolchain all operate on the working
//! tree. Each command marshals its inputs and delegates to an already-native
//! backend, following the shared console convention from [`gmeow_cli_core`]:
//! product results → stdout, diagnostics → stderr, exit `0`/`1`/`2`.
//!
//! The clap `Cli`/`Commands` surface and its dispatch live here; the wired
//! command bodies live in the per-area `dev_*` modules.

// Successful producer inputs are shared without flattening failed initialization.
#![feature(once_cell_try)]

mod dev_build;
mod dev_common;
mod dev_conformance;
mod dev_docs_measure;
mod dev_docs_package;
mod dev_feedback;
mod dev_gates;
mod dev_i18n;
mod dev_logic;
mod dev_medium_sweep;
mod dev_pipeline_reads;
mod dev_producer;
mod dev_project;
mod dev_reason;
mod dev_shapes;
mod dev_slice_quality;
mod dev_sync;
mod dev_targets;
mod dev_term_release_authority;
mod dev_test_fixtures;
mod dev_transpile;
mod dev_validate;
mod error;
pub mod feedback_bundle;

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use gmeow_cli_core::ConsoleMode;
/// Whether synchronization updates the worktree or verifies it read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SyncMode {
    Update,
    Check,
}

impl SyncMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Update => "update",
            Self::Check => "check",
        }
    }
}

/// Which projections the unified synchronization phase materializes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SyncOutput {
    /// Complete pipeline: committed/generated, runtime dist, and external docs.
    All,
    /// Complete pipeline with only committed/generated outputs materialized.
    Generated,
    /// External site/book/print/snippet/model docs plus required fresh inputs.
    Docs,
}

/// Which kind of build `gmeow-dev sync` runs. A release is selected explicitly, never
/// inferred from the output scope or the environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SyncProfile {
    /// An ordinary build: stale dictionary sweep evidence is recorded on the shipped
    /// realization and reported, and the build continues.
    Ordinary,
    /// A release build: stale dictionary sweep evidence is refused.
    Release,
}

impl SyncProfile {
    pub(crate) const fn as_str(self) -> &'static str {
        self.build_profile().as_str()
    }

    pub(crate) const fn build_profile(self) -> gmeow_pipeline::BuildProfile {
        match self {
            Self::Ordinary => gmeow_pipeline::BuildProfile::Ordinary,
            Self::Release => gmeow_pipeline::BuildProfile::Release,
        }
    }
}

/// Which side of the strict pre-test fixture boundary to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TestFixtureMode {
    /// Run the explicit producer DAG before any test process starts.
    Produce,
    /// Authenticate existing receipts and products without a producer fallback.
    Verify,
}

/// Fixture subset used by the split CI producer/archive lanes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TestFixtureScope {
    All,
    ProducerIndependent,
    ProducerBound,
    Bundle,
    /// Exhaustive native divergence evidence, produced separately from required fixtures.
    ConformanceHeavy,
    /// Produce only the native browser codebook closure, or verify its embedding read-only.
    WasmCodebook,
}

impl SyncOutput {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Generated => "generated",
            Self::Docs => "docs",
        }
    }
}

use dev_common::{project_root, snapshot_bytes};

/// The `gmeow-dev` repo-maintenance CLI.
#[derive(Debug, Parser)]
#[command(
    name = "gmeow-dev",
    version,
    about = "The GMEOW ontology developer/maintenance CLI."
)]
pub struct Cli {
    /// The console output surface (flag > `GMEOW_CONSOLE` env > auto).
    #[arg(long, global = true, value_enum)]
    pub console: Option<ConsoleMode>,

    #[command(subcommand)]
    pub command: Commands,
}

// Field-bearing commands delegate their argument builders through Args.

/// Arguments for [`Commands::TestFixtures`].
#[derive(Debug, clap::Args)]
#[group(id = "TestFixtures")]
pub struct TestFixturesArgs {
    #[arg(value_enum)]
    mode: TestFixtureMode,
    #[arg(long = "scope", value_enum, default_value_t = TestFixtureScope::All)]
    scope: TestFixtureScope,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
    #[arg(long = "bundle-import-cache-root")]
    bundle_import_cache_root: Option<PathBuf>,
    #[arg(long = "expected-source-digest")]
    expected_source_digest: Option<String>,
}

/// Arguments for [`Commands::SliceSpecWorker`].
#[derive(Debug, clap::Args)]
#[group(id = "SliceSpecWorker")]
pub struct SliceSpecWorkerArgs {
    #[arg(long)]
    kind: String,
    #[arg(long = "spec", required = true)]
    specs: Vec<PathBuf>,
    #[arg(long, default_value_t = 1)]
    workers: usize,
}

/// Arguments for [`Commands::Sync`].
#[derive(Debug, clap::Args)]
#[group(id = "Sync")]
pub struct SyncArgs {
    /// Update locally or verify read-only. Defaults to check in CI, update elsewhere.
    #[arg(long = "mode", value_enum)]
    mode: Option<SyncMode>,
    /// Projection set. All outputs are made by default.
    #[arg(long = "outputs", value_enum, default_value_t = SyncOutput::All)]
    outputs: SyncOutput,
    /// Build profile. `release` refuses stale dictionary sweep evidence; the
    /// ordinary default records it on the bundle and warns.
    #[arg(long = "profile", value_enum, default_value_t = SyncProfile::Ordinary)]
    profile: SyncProfile,
    #[arg(short = 'j', long = "jobs")]
    jobs: Option<usize>,
    #[arg(long = "metadata")]
    metadata: bool,
    #[arg(long = "list-paths")]
    list_paths: bool,
    #[arg(long = "lang")]
    lang: Option<String>,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
    /// Stream live DAG stages and synchronization boundaries.
    #[arg(short = 'v', long = "verbose")]
    verbose: bool,
}

/// Arguments for [`Commands::Fanout`].
#[derive(Debug, clap::Args)]
#[group(id = "Fanout")]
pub struct FanoutArgs {
    #[arg(short = 'j', long = "jobs")]
    jobs: Option<usize>,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
}

/// Arguments for [`Commands::ConsoleAssemble`].
#[derive(Debug, clap::Args)]
#[group(id = "ConsoleAssemble")]
pub struct ConsoleAssembleArgs {
    #[arg(long = "out", default_value = "dist/console-smoke")]
    out: PathBuf,
}

/// Arguments for [`Commands::DocsPackage`].
#[derive(Debug, clap::Args)]
#[group(id = "DocsPackage")]
pub struct DocsPackageArgs {
    #[arg(long = "out", default_value = "dist/gmeow-docs.tar")]
    out: PathBuf,
}

/// Arguments for [`Commands::ReleaseBundle`].
#[derive(Debug, clap::Args)]
#[group(id = "ReleaseBundle")]
pub struct ReleaseBundleArgs {
    #[arg(long = "out", default_value = "dist/gmeow.gts")]
    out: PathBuf,
    #[arg(long = "sign-key")]
    sign_key: PathBuf,
    #[arg(long = "public-key")]
    public_key: PathBuf,
    /// Source GTS; its adjacent .ingestion.cbor receipt is required and retained.
    #[arg(long = "source", default_value = "generated/dist/gmeow.gts")]
    source: PathBuf,
    #[arg(long = "issued-at")]
    issued_at: String,
    #[arg(
        long = "attester",
        default_value = "https://blackcatinformatics.ca/gmeow/agent/release-lane"
    )]
    attester: String,
    #[arg(
        long = "release-subject",
        default_value = "https://blackcatinformatics.ca/gmeow/release/gmeow.gts"
    )]
    release_subject: String,
    #[arg(long = "evidence")]
    evidence: Vec<String>,
}

/// Arguments for [`Commands::GtsFrameProfile`].
#[derive(Debug, clap::Args)]
#[group(id = "GtsFrameProfile")]
pub struct GtsFrameProfileArgs {
    #[arg(default_value = "generated/dist/gmeow.gts")]
    gts: PathBuf,
}

/// Arguments for [`Commands::MediumGate`].
#[derive(Debug, clap::Args)]
#[group(id = "MediumGate")]
pub struct MediumGateArgs {
    #[arg(default_value = "generated/dist/gmeow.gts")]
    gts: PathBuf,
    /// The bundle whose medium registry an artifact that carries none of its own
    /// (a runtime store) is audited against — the bundle its dictionaries came from.
    #[arg(long = "registry", default_value = "generated/dist/gmeow.gts")]
    registry: PathBuf,
}

/// Arguments for [`Commands::MediumSweep`].
#[derive(Debug, clap::Args)]
#[group(id = "MediumSweep")]
pub struct MediumSweepArgs {
    /// Write the complete measurement evidence to this path.
    #[arg(long, default_value = "bench/medium-baseline.json")]
    out: PathBuf,
    /// Scheduler width for the measured pipeline run. Defaults to every CPU.
    #[arg(short = 'j', long = "jobs")]
    jobs: Option<usize>,
}

/// Arguments for [`Commands::PipelineReads`].
#[derive(Debug, clap::Args)]
#[group(id = "PipelineReads")]
pub struct PipelineReadsArgs {
    /// Write the Turtle declarations to this path.
    #[arg(long)]
    out: PathBuf,
    /// Scheduler width for the recorded pipeline run. Defaults to every CPU.
    #[arg(short = 'j', long = "jobs")]
    jobs: Option<usize>,
}

/// Arguments for [`Commands::MediumSeed`].
#[derive(Debug, clap::Args)]
#[group(id = "MediumSeed")]
pub struct MediumSeedArgs {
    /// Write the temporary seed table to this path; it is not measurement evidence.
    #[arg(long, default_value = "bench/medium-baseline.json")]
    out: PathBuf,
}

/// Arguments for [`Commands::TermReleaseAuthority`].
#[derive(Debug, clap::Args)]
#[group(id = "TermReleaseAuthority")]
pub struct TermReleaseAuthorityArgs {
    /// Initialize the authority once; refuse to overwrite an existing authority.
    #[arg(long)]
    bootstrap: bool,
}

/// Arguments for [`Commands::Validate`].
#[derive(Debug, clap::Args)]
#[group(id = "Validate")]
pub struct ValidateArgs {
    #[arg(long = "timings")]
    timings: bool,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
    #[arg(long = "gts")]
    gts: Option<PathBuf>,
    #[arg(long = "trust-policy")]
    trust_policy: Option<PathBuf>,
    #[arg(long = "require-signed")]
    require_signed: bool,
    #[arg(long = "trusted-key")]
    trusted_key: Option<PathBuf>,
    #[arg(long = "deep")]
    deep: bool,
}

/// Arguments for [`Commands::Feedback`].
#[derive(Debug, clap::Args)]
#[group(id = "Feedback")]
pub struct FeedbackArgs {
    #[arg(long = "diagnostics-console")]
    diagnostics_console: Option<String>,
    #[arg(long = "diagnostics-artifacts")]
    diagnostics_artifacts: Option<String>,
    #[arg(long = "diagnostics-dir")]
    diagnostics_dir: Option<PathBuf>,
    #[arg(long = "diagnostics-stem")]
    diagnostics_stem: Option<String>,
    #[arg(long = "diagnostics-category")]
    diagnostics_category: Option<String>,
    #[arg(long = "timings")]
    timings: bool,
}

/// Arguments for [`Commands::ExternalTool`].
#[derive(Debug, clap::Args)]
#[group(id = "ExternalTool")]
pub struct ExternalToolArgs {
    #[arg(long = "name")]
    name: String,
    #[arg(long = "diagnostics-console")]
    diagnostics_console: Option<String>,
    #[arg(long = "diagnostics-artifacts")]
    diagnostics_artifacts: Option<String>,
    #[arg(long = "diagnostics-dir")]
    diagnostics_dir: Option<PathBuf>,
    #[arg(long = "diagnostics-stem")]
    diagnostics_stem: Option<String>,
    #[arg(long = "diagnostics-category")]
    diagnostics_category: Option<String>,
    /// The external command to run, e.g. `-- mypy src`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

/// Arguments for [`Commands::Audit`].
#[derive(Debug, clap::Args)]
#[group(id = "Audit")]
pub struct AuditArgs {
    files: Vec<PathBuf>,
    #[arg(long = "json")]
    json: bool,
    #[arg(long = "strict")]
    strict: bool,
}

/// Arguments for [`Commands::ComplianceReport`].
#[derive(Debug, clap::Args)]
#[group(id = "ComplianceReport")]
pub struct ComplianceReportArgs {
    #[arg(long = "from-passing-check")]
    from_passing_check: bool,
}

/// Arguments for [`Commands::Reason`].
#[derive(Debug, clap::Args)]
#[group(id = "Reason")]
pub struct ReasonArgs {
    #[arg(long = "mode", default_value = "native")]
    mode: String,
    /// Recompute the closure instead of reusing the shipped graph/reasoning verdict.
    #[arg(long = "fresh")]
    fresh: bool,
    #[arg(long = "merge")]
    merge: bool,
    #[arg(long = "profile", default_value = "DL")]
    profile: String,
    #[arg(long = "full")]
    full: bool,
    #[arg(long = "exclude-tautologies")]
    exclude_tautologies: Option<String>,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
}

/// Arguments for [`Commands::Verify`].
#[derive(Debug, clap::Args)]
#[group(id = "Verify")]
pub struct VerifyArgs {
    #[arg(long = "mode", default_value = "native")]
    mode: String,
    /// Recompute the closure instead of reusing the shipped graph/reasoning verdict.
    #[arg(long = "fresh")]
    fresh: bool,
    #[arg(long = "reasoned-input")]
    reasoned_input: Option<PathBuf>,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
}

/// Arguments for [`Commands::ReasonVerify`].
#[derive(Debug, clap::Args)]
#[group(id = "ReasonVerify")]
pub struct ReasonVerifyArgs {
    /// Recompute the closure instead of reusing the shipped graph/reasoning verdict.
    #[arg(long = "fresh")]
    fresh: bool,
    #[arg(long = "merge")]
    merge: bool,
    #[arg(long = "timings-json")]
    timings_json: Option<PathBuf>,
}

/// Arguments for [`Commands::Temporal`].
#[derive(Debug, clap::Args)]
#[group(id = "Temporal")]
pub struct TemporalArgs {
    query: String,
    #[arg(long = "data")]
    data: Option<PathBuf>,
    #[arg(long = "focus")]
    focus: Option<String>,
    /// Zoned xsd:dateTime window start in the default UTC-Gregorian frame.
    #[arg(long = "window-start")]
    window_start: Option<String>,
    /// Zoned xsd:dateTime window end in the default UTC-Gregorian frame.
    #[arg(long = "window-end")]
    window_end: Option<String>,
    #[arg(long = "valid-at")]
    valid_at: Option<String>,
    #[arg(long = "as-of")]
    as_of: Option<String>,
}

/// Arguments for [`Commands::Extract`].
#[derive(Debug, clap::Args)]
#[group(id = "Extract")]
pub struct ExtractArgs {
    #[arg(long = "target")]
    target: String,
}

/// Arguments for [`Commands::LintAlignment`].
#[derive(Debug, clap::Args)]
#[group(id = "LintAlignment")]
pub struct LintAlignmentArgs {
    #[arg(long = "network")]
    network: bool,
    #[arg(long = "strict")]
    strict: bool,
}

/// Arguments for [`Commands::RefreshTargetAxioms`].
#[derive(Debug, clap::Args)]
#[group(id = "RefreshTargetAxioms")]
pub struct RefreshTargetAxiomsArgs {
    #[arg(long = "target", default_value = "all")]
    target: String,
}

/// Arguments for [`Commands::Wikidata`].
#[derive(Debug, clap::Args)]
#[group(id = "Wikidata")]
pub struct WikidataArgs {
    #[arg(long = "existence")]
    existence: bool,
    #[arg(long = "fixtures")]
    fixtures: bool,
}

/// Arguments for [`Commands::WikidataCoverage`].
#[derive(Debug, clap::Args)]
#[group(id = "WikidataCoverage")]
pub struct WikidataCoverageArgs {
    #[arg(long = "json")]
    json: bool,
    #[arg(long = "threshold", default_value_t = 0.5)]
    threshold: f64,
}

/// Arguments for [`Commands::DcCoverage`].
#[derive(Debug, clap::Args)]
#[group(id = "DcCoverage")]
pub struct DcCoverageArgs {
    #[arg(long = "json")]
    json: bool,
    #[arg(long = "threshold", default_value_t = 0.5)]
    threshold: f64,
}

/// Arguments for [`Commands::UpProjectionAudit`].
#[derive(Debug, clap::Args)]
#[group(id = "UpProjectionAudit")]
pub struct UpProjectionAuditArgs {
    #[arg(long = "report")]
    report: Option<PathBuf>,
    #[arg(long = "gaps")]
    gaps: bool,
}

/// Arguments for [`Commands::Coverage`].
#[derive(Debug, clap::Args)]
#[group(id = "Coverage")]
pub struct CoverageArgs {
    #[arg(long = "gaps")]
    gaps: bool,
    #[arg(long = "min-class")]
    min_class: Option<f64>,
    #[arg(long = "min-predicate")]
    min_predicate: Option<f64>,
}

/// Arguments for [`Commands::Project`].
#[derive(Debug, clap::Args)]
#[group(id = "Project")]
pub struct ProjectArgs {
    source: Option<PathBuf>,
    #[arg(long = "profile", default_value = "all")]
    profile: String,
    #[arg(long = "data", default_value = "")]
    data: String,
    #[arg(long = "lang", short = 'l')]
    lang: Option<String>,
}

/// Arguments for [`Commands::Transform`].
#[derive(Debug, clap::Args)]
#[group(id = "Transform")]
pub struct TransformArgs {
    abox: PathBuf,
    #[arg(long = "out", short = 'o')]
    out: Option<PathBuf>,
    #[arg(long = "profiles", default_value = "all")]
    profiles: String,
    #[arg(long = "diff-target")]
    diff_target: Option<PathBuf>,
    #[arg(long = "report")]
    report: Option<PathBuf>,
    #[arg(long = "lang", short = 'l')]
    lang: Option<String>,
}

/// Arguments for [`Commands::UpProject`].
#[derive(Debug, clap::Args)]
#[group(id = "UpProject")]
pub struct UpProjectArgs {
    source: PathBuf,
    #[arg(long = "out", short = 'o')]
    out: Option<PathBuf>,
}

/// Arguments for [`Commands::Acceptance`].
#[derive(Debug, clap::Args)]
#[group(id = "Acceptance")]
pub struct AcceptanceArgs {
    source: Option<PathBuf>,
    #[arg(long = "out", short = 'o')]
    out: Option<PathBuf>,
    #[arg(long = "min-recall")]
    min_recall: Option<f64>,
}

/// Arguments for [`Commands::Quality`].
#[derive(Debug, clap::Args)]
#[group(id = "Quality")]
pub struct QualityArgs {
    #[arg(long = "foops-url", default_value = "")]
    foops_url: String,
    #[arg(long = "strict")]
    strict: bool,
}

/// Arguments for [`Commands::CompileGts`].
#[derive(Debug, clap::Args)]
#[group(id = "CompileGts")]
pub struct CompileGtsArgs {
    #[arg(long = "out", short = 'o')]
    out: Option<PathBuf>,
    #[arg(long = "sign-key")]
    sign_key: Option<PathBuf>,
    #[arg(long = "public-key")]
    public_key: Option<PathBuf>,
    /// The build profile the snapshot is regenerated under. Signing requires `release`.
    #[arg(long = "profile", value_enum, default_value = "ordinary")]
    profile: SyncProfile,
}

/// Arguments for [`Commands::ImportFoundation`].
#[derive(Debug, clap::Args)]
#[group(id = "ImportFoundation")]
pub struct ImportFoundationArgs {
    jsonl: PathBuf,
    #[arg(long = "out", default_value = "build/foundation")]
    out: PathBuf,
    #[arg(long = "nq")]
    nq: Option<PathBuf>,
}

/// Arguments for [`Commands::Describe`].
#[derive(Debug, clap::Args)]
#[group(id = "Describe")]
pub struct DescribeArgs {
    term: String,
    #[arg(long = "gts")]
    gts: Option<PathBuf>,
    #[arg(long = "lang", short = 'l')]
    lang: Option<String>,
}

/// Arguments for [`Commands::ShapeEquivalence`].
#[derive(Debug, clap::Args)]
#[group(id = "ShapeEquivalence")]
pub struct ShapeEquivalenceArgs {
    /// Restrict the scan (and the exit code) to legacy `shapes.ttl` under this directory
    /// (repo-relative or absolute); default scans every slice.
    #[arg(long = "path")]
    path: Option<PathBuf>,
}

/// Arguments for [`Commands::ConformanceBless`].
#[derive(Debug, clap::Args)]
#[group(id = "ConformanceBless")]
pub struct ConformanceBlessArgs {
    /// The cases to refresh; at least one is required.
    #[arg(required = true)]
    cases: Vec<PathBuf>,
    /// Seed a new case: write the full produced golden set, not only the goldens it commits.
    #[arg(long)]
    init: bool,
}

/// Arguments for [`Commands::ShapeLift`].
#[derive(Debug, clap::Args)]
#[group(id = "ShapeLift")]
pub struct ShapeLiftArgs {
    /// Restrict the scan (and the exit code) to legacy `shapes.ttl` under this directory
    /// (repo-relative or absolute); default scans every slice.
    #[arg(long = "path")]
    path: Option<PathBuf>,
}

/// Arguments for [`Commands::ShapeMigrate`].
#[derive(Debug, clap::Args)]
#[group(id = "ShapeMigrate")]
pub struct ShapeMigrateArgs {
    /// Restrict the scan to legacy `shapes.ttl` under this directory (repo-relative or absolute);
    /// default scans every slice.
    #[arg(long = "path")]
    path: Option<PathBuf>,
    /// Write the changes (inject axioms, or delete blocks under `--prune`); default is dry-run.
    #[arg(long = "apply")]
    apply: bool,
    /// Prune phase: delete `shapes.ttl` blocks the projector already reproduces (run after a
    /// regenerate) instead of injecting grounding.
    #[arg(long = "prune")]
    prune: bool,
}

/// Arguments for [`Commands::Certify`].
#[derive(Debug, clap::Args)]
#[group(id = "Certify")]
pub struct CertifyArgs {
    input_path: PathBuf,
    #[arg(long = "profile")]
    profile: Option<String>,
}

/// Arguments for [`Commands::SliceQualitySeedFloors`].
#[derive(Debug, clap::Args)]
#[group(id = "SliceQualitySeedFloors")]
pub struct SliceQualitySeedFloorsArgs {
    /// Seed exactly one named rubric axis (its IRI local name, e.g.
    /// axisShapeMigration). Exactly one of --axis / --all-axes is required.
    #[arg(long = "axis")]
    axis: Option<String>,
    /// Seed every rubric axis that lacks a committed floor for a given slice.
    /// Exactly one of --axis / --all-axes is required.
    #[arg(long = "all-axes")]
    all_axes: bool,
}

/// Arguments for [`Commands::SliceQualityRelocationPreview`].
#[derive(Debug, clap::Args)]
#[group(id = "SliceQualityRelocationPreview")]
pub struct SliceQualityRelocationPreviewArgs {
    /// A term IRI that would move. Repeatable; at least one is required.
    #[arg(long = "term", required = true)]
    term: Vec<String>,
    /// The slice IRI (or slice local name) the terms would depart.
    #[arg(long = "from")]
    from: String,
    /// The slice IRI (or slice local name) the terms would arrive at.
    #[arg(long = "to")]
    to: String,
}

/// Arguments for [`Commands::SliceFixDeps`].
#[derive(Debug, clap::Args)]
#[group(id = "SliceFixDeps")]
pub struct SliceFixDepsArgs {
    #[arg(long = "apply")]
    apply: bool,
    #[arg(long = "slices-dir")]
    slices_dir: Option<PathBuf>,
}

/// Arguments for [`Commands::BoxRoles`].
#[derive(Debug, clap::Args)]
#[group(id = "BoxRoles")]
pub struct BoxRolesArgs {
    #[command(subcommand)]
    command: BoxRolesCommands,
}

/// Arguments for [`Commands::Logic`].
#[derive(Debug, clap::Args)]
#[group(id = "Logic")]
pub struct LogicArgs {
    #[command(subcommand)]
    command: LogicCommands,
}

/// Arguments for [`Commands::I18n`].
#[derive(Debug, clap::Args)]
#[group(id = "I18n")]
pub struct I18nArgs {
    #[command(subcommand)]
    command: I18nCommands,
}

/// Score a slice against the slice-quality rubric and emit ranked uplift advice.
// Keep this field-rich command's builder outside Commands::augment_subcommands.
#[derive(Debug, clap::Args)]
#[group(id = "SliceQuality")]
pub struct SliceQualityArgs {
    /// The slice directory to score (omit with --all).
    #[arg(conflicts_with = "all")]
    path: Option<PathBuf>,
    /// Sweep every slice under slices/ instead of one path.
    #[arg(long = "all")]
    all: bool,
    /// Output rendering: text (default), json, sarif, or rdf.
    #[arg(long = "format")]
    format: Option<String>,
    /// Gate this run at an explicit tier: exit non-zero if the measured
    /// roll-up is below TIER (a tier local name or label, e.g. Grounded,
    /// Linked, Exemplified, Maximal, Registered). Unset = advisory (always
    /// exit 0). With --all, fails if ANY swept slice is below TIER, naming
    /// each. Render/emit still happen first; the gate only sets the exit code.
    #[arg(long = "min-tier")]
    min_tier: Option<String>,
    /// Diagnostics console surface (flag > env > auto).
    #[arg(long = "diagnostics-console")]
    diagnostics_console: Option<String>,
    /// Comma-separated diagnostics artifacts to write: json,sarif,html.
    #[arg(long = "diagnostics-artifacts")]
    diagnostics_artifacts: Option<String>,
    /// Directory the diagnostics artifacts are written under.
    #[arg(long = "diagnostics-dir")]
    diagnostics_dir: Option<PathBuf>,
    /// Filename stem for the written diagnostics artifacts.
    #[arg(long = "diagnostics-stem")]
    diagnostics_stem: Option<String>,
    /// Diagnostics category stamped into the report metadata.
    #[arg(long = "diagnostics-category")]
    diagnostics_category: Option<String>,
}

/// The developer subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print the build recipe embedded by the dedicated producer builder.
    #[command(hide = true)]
    BuildIdentity,
    /// Print the gmeow package version.
    Version,
    /// Show a summary of the bundled GMEOW ontology snapshot.
    Info,
    /// Produce or authenticate the exact pre-test fixture DAG.
    #[command(name = "test-fixtures", hide = true)]
    TestFixtures(TestFixturesArgs),
    /// Private child-process executor for one producer-owned declarative-spec wave.
    #[command(name = "slice-spec-worker", hide = true)]
    SliceSpecWorker(SliceSpecWorkerArgs),
    /// Run the canonical one-pass pipeline, strict gates, and output projections.
    Sync(SyncArgs),
    /// Project the flat consumer tree back out of gmeow.gts.
    Fanout(FanoutArgs),
    /// Measure real, deterministic per-format documentation byte sizes and the
    /// three external-distribution design totals.
    #[command(name = "docs-measure")]
    DocsMeasure,
    /// Verify two independent documentation productions and their size accounting.
    DocsMeasureVerify,
    /// Assemble the standalone `<gmeow-console>` tree (shell + shared engine assets)
    /// into a directory, off the same bundle-backed exec the site render uses.
    #[command(name = "console-assemble")]
    ConsoleAssemble(ConsoleAssembleArgs),
    /// Package the materialized `dist/gmeow-docs/` external documentation
    /// distribution into one deterministic content-addressed release asset,
    /// alongside a `.blake3` sidecar for the DCAT release manifest.
    #[command(name = "docs-package")]
    DocsPackage(DocsPackageArgs),
    /// Fold evidence into a signed GTS output with its mandatory ingestion companion.
    #[command(name = "release-bundle")]
    ReleaseBundle(ReleaseBundleArgs),
    /// Audit the mandatory zstd-rsyncable frame profile of a GTS bundle.
    #[command(name = "gts-frame-profile")]
    GtsFrameProfile(GtsFrameProfileArgs),
    /// Audit the whole MEDIUM axis of any GMEOW-authored GTS artifact — the dist
    /// bundle, a runtime `~/.gmeow/*.gts` store, or a whole-artifact product.
    #[command(name = "medium-gate")]
    MediumGate(MediumGateArgs),
    /// Measure the complete dictionary and codec grids on the optimized producer.
    MediumSweep(MediumSweepArgs),
    /// Run the pipeline once recording what every stage body reads through every
    /// dataflow edge, and write the declarations those reads need to the file named by
    /// `--out`.
    #[command(name = "pipeline-reads")]
    PipelineReads(PipelineReadsArgs),
    /// Write a declaration-only bootstrap table with zero measured evidence.
    MediumSeed(MediumSeedArgs),
    /// Advance the computed term-changelog authority at an accepted release boundary.
    TermReleaseAuthority(TermReleaseAuthorityArgs),
    /// Validate Turtle syntax, term annotations, and SHACL conformance.
    Validate(ValidateArgs),
    /// Write first-class diagnostics artifacts for the whole dev gate.
    Feedback(FeedbackArgs),
    /// Run an external gate tool and represent a failure as a canonical finding.
    #[command(name = "external-tool")]
    ExternalTool(ExternalToolArgs),
    /// Verify every constitutional principle has live enforcement.
    #[command(name = "constitution-check")]
    ConstitutionCheck,
    /// Audit claims (ungrounded / contradicted / stale) over data files.
    Audit(AuditArgs),
    /// Emit the RDF compliance report.
    #[command(name = "compliance-report")]
    ComplianceReport(ComplianceReportArgs),
    /// Reason over the ontology (native EL/DL, Docker-free).
    Reason(ReasonArgs),
    /// Explain unsatisfiable classes / inconsistency.
    Explain,
    /// Run reasoned-graph negative tests (native).
    Verify(VerifyArgs),
    /// Run native reasoning followed by reasoned-graph verify.
    #[command(name = "reason-verify")]
    ReasonVerify(ReasonVerifyArgs),
    /// Run TQL: typed placement display or scoped zoned UTC-Gregorian timestamp comparisons.
    Temporal(TemporalArgs),
    /// Report the import/extract policy for an alignment target.
    Extract(ExtractArgs),
    /// Lint SSSOM property mappings for direction/domain-range mismatches.
    #[command(name = "lint-alignment")]
    LintAlignment(LintAlignmentArgs),
    /// Lint the rust-rendered ontology-docs site.
    #[command(name = "doc-lint")]
    DocLint,
    /// Verify Rust crate layering and repository-static policy.
    #[command(name = "crate-check")]
    CrateCheck,
    /// Verify that production and fuzzing resolve the same PurRDF package identities.
    #[command(name = "fuzz-substrate-check")]
    FuzzSubstrateCheck,
    /// Re-vendor minimal target-axiom snapshots (network).
    #[command(name = "refresh-target-axioms")]
    RefreshTargetAxioms(RefreshTargetAxiomsArgs),
    /// Build alignment axioms + VoID linksets from SSSOM.
    Mappings,
    /// Validate the Wikidata QIDs/PIDs used in the mappings.
    Wikidata(WikidataArgs),
    /// Report Wikidata mapping coverage by domain/module.
    #[command(name = "wikidata-coverage")]
    WikidataCoverage(WikidataCoverageArgs),
    /// Report Dublin Core mapping coverage by namespace.
    #[command(name = "dc-coverage")]
    DcCoverage(DcCoverageArgs),
    /// Audit consumer→GMEOW up-projection invertibility.
    #[command(name = "up-projection-audit")]
    UpProjectionAudit(UpProjectionAuditArgs),
    /// Report how much of the vendored entity slice GMEOW covers.
    Coverage(CoverageArgs),
    /// Generate the CrossRef DOI deposit XML.
    Crossref,
    /// Canonicalize the authored ontology sources.
    Normalize,
    /// Build serializations and OWL-native syntaxes into dist/.
    Build,
    /// Project GMEOW to a pure vocabulary profile.
    Project(ProjectArgs),
    /// Transpile an A-Box to MAXIMAL(G).
    Transform(TransformArgs),
    /// Lift a consumer-vocabulary RDF file UP into pure GMEOW.
    #[command(name = "up-project")]
    UpProject(UpProjectArgs),
    /// Score the full transpile against real data.
    Acceptance(AcceptanceArgs),
    /// Run OOPS! (pitfalls) and optionally FOOPS! (FAIR).
    Quality(QualityArgs),
    /// Compile the statement-complete GTS snapshot and its ingestion companion.
    #[command(name = "compile-gts")]
    CompileGts(CompileGtsArgs),
    /// Start the GMEOW MCP server (wired in the MCP task).
    Mcp,
    /// Import the foundation corpus.
    #[command(name = "import-foundation")]
    ImportFoundation(ImportFoundationArgs),
    /// Describe a GMEOW term as useful prose.
    Describe(DescribeArgs),
    /// Prove legacy `shapes.ttl` blocks are reproduced by the projected validation shapes.
    #[command(name = "shape-equivalence")]
    ShapeEquivalence(ShapeEquivalenceArgs),
    /// Refresh the expected outputs of the named conformance cases from the native runner.
    /// Every case is named explicitly, by path or by its id under `conformance/logic/cases`.
    #[command(name = "conformance-bless")]
    ConformanceBless(ConformanceBlessArgs),
    /// Propose (and certify) the OWL antecedent that would ground each legacy `shapes.ttl` block.
    #[command(name = "shape-lift")]
    ShapeLift(ShapeLiftArgs),
    /// Automated shape migration: inject the lifted OWL grounding into each class's module.ttl
    /// (`--apply`), then delete the now-equivalent `shapes.ttl` blocks (`--prune`, after a
    /// regenerate). Default is a dry-run report.
    #[command(name = "shape-migrate")]
    ShapeMigrate(ShapeMigrateArgs),
    /// Statically certify a logic program against its declared profile.
    Certify(CertifyArgs),
    /// Score a slice against the slice-quality rubric and emit ranked uplift advice.
    #[command(name = "slice-quality")]
    SliceQuality(SliceQualityArgs),
    /// Enforce the opt-in slice-quality tier ratchet (a `make check` gate).
    #[command(name = "slice-quality-gate")]
    SliceQualityGate,
    /// Emit `gmeow:AxisFloorCommitment` TTL for the live measured scores, so a human
    /// can seed a NEW axis's floors at the actual measurement and paste them into
    /// `slices/core/slice-quality-rubric/module.ttl`. ONE-SHOT per axis: this seeds a
    /// new axis's floors ONCE — re-running to "refresh" an already-floored axis is
    /// forbidden (a dropped score would red monotonicity; a risen score would silently
    /// ratchet the floor up = banned auto-calibration). Raise a floor later only by a
    /// deliberate hand-edit of the individual, never a seeder re-run. Emit-only: the
    /// TTL goes to stdout for the human to commit.
    #[command(name = "slice-quality-seed-floors")]
    SliceQualitySeedFloors(SliceQualitySeedFloorsArgs),
    /// Emit `gmeow:ProjectionCeilingCommitment` TTL at the CURRENT measured
    /// ungrounded residue for every (slice, guarded-vocabulary) pair with nonzero
    /// residue, so a human can grandfather the existing residue into
    /// `slices/core/slice-quality-rubric/module.ttl`. Uses the SAME shared counter
    /// (`gmeow_slice_quality::measure_repo_residues`) the ratchet gate reads — seed
    /// and gate can never diverge. EMIT-ONLY, GRANDFATHER-ONCE: this seeds the
    /// ceiling ABox at whatever residue is live the moment it is run; re-running it
    /// to "refresh" a ceiling whose measured residue has since risen is a banned
    /// auto-calibration (the correct response to a risen residue is the gate reading,
    /// never a re-seed that raises the ceiling to match). Lowering a ceiling later,
    /// after a genuine measured migration grounds constructs out of the residue, is
    /// always a deliberate hand-edit of the individual, never a seeder re-run. The
    /// TTL goes to stdout for the human to commit.
    #[command(name = "slice-quality-seed-ceilings")]
    SliceQualitySeedCeilings {},
    /// Report-only migration dashboard for the projection-vocabulary ratchet: for
    /// every (slice, guarded-vocabulary) cell with either a live measured residue
    /// or a committed ceiling, print measured/ceiling/headroom. `measured` is a
    /// LIVE scan through the same shared counter the ratchet gate reads — it is
    /// NEVER persisted as a `SoundUnder` projection (a scan result is entailed by
    /// no resident individual). Always exits 0; never gates `make check`. A
    /// ceiling is never tuned to this report's numbers — lowering one is always a
    /// deliberate hand-edit after a genuine measured migration.
    #[command(name = "slice-quality-projection-debt")]
    SliceQualityProjectionDebt {},
    /// Report-only PREVIEW of relocating named terms between two slices: per guarded
    /// vocabulary, the transport plan (how much residue would move, what credit the
    /// source lowering would raise, what the destination would have to ask for), the
    /// residual UNPAID demand, the residue-conservation reason codes that explain why
    /// residue is not conserved across the move, and the AXIS-FLOOR COLLATERAL — the
    /// committed axis floors on both slices with their live headroom. Floors are
    /// deliberately NOT netted by relocation (an axis floor measures the documentation
    /// quality of the inventory a slice OWNS, which really is location-dependent), so a
    /// maintainer must see that cost before moving, not after. When none of the
    /// requested terms anchors residue in the source slice, it lists the terms that DO,
    /// per vocabulary, with the construct count each would carry — the discovery
    /// surface, since the relocation-invariant anchor is derived and cannot be read off
    /// the Turtle. Never gates on its findings; malformed input, rubric-load and
    /// measurement errors still fail non-zero. Its numbers are never fed back into a
    /// ceiling.
    #[command(name = "slice-quality-relocation-preview")]
    SliceQualityRelocationPreview(SliceQualityRelocationPreviewArgs),
    /// Propose manifest dependency edits as a reviewable unified diff.
    #[command(name = "slice-fix-deps")]
    SliceFixDeps(SliceFixDepsArgs),
    /// Audit graph-box role coverage in authored sources.
    #[command(name = "box-roles")]
    BoxRoles(BoxRolesArgs),
    /// Logic-stack operations.
    Logic(LogicArgs),
    /// Internationalization toolchain.
    I18n(I18nArgs),
}

/// `gmeow-dev box-roles` subcommands.
#[derive(Debug, Subcommand)]
pub enum BoxRolesCommands {
    /// Audit explicit ABox/TBox/RBox/CBox/ConfigBox role coverage.
    Audit {
        #[arg(long = "json")]
        json: bool,
    },
}

/// `gmeow-dev logic` subcommands.
#[derive(Debug, Subcommand)]
pub enum LogicCommands {
    /// Resolve a backward goal over a materialized world.
    Query {
        world: PathBuf,
        query_file: PathBuf,
        #[arg(long = "profile", default_value = "PositiveHornProfile")]
        profile: String,
        #[arg(long = "world-iri")]
        world_iri: Option<String>,
        #[arg(long = "max-answers")]
        max_answers: Option<usize>,
        #[arg(long = "max-steps")]
        max_steps: Option<u64>,
        #[arg(long = "json")]
        json: bool,
    },
    /// Compile logic: vocabulary → IR → canonical artifact + projections.
    Compile {
        #[arg(long = "check")]
        check: bool,
        #[arg(long = "mode", value_parser = dev_logic::mode_parser())]
        mode: Option<String>,
    },
}

/// `gmeow-dev i18n` subcommands.
#[derive(Debug, Subcommand)]
pub enum I18nCommands {
    /// Extract translatable ontology strings into gettext catalogs.
    Extract {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "output-dir", short = 'o')]
        output_dir: Option<PathBuf>,
        #[arg(long = "lang", short = 'l')]
        lang: Option<String>,
        #[arg(long = "terms-only")]
        terms_only: bool,
    },
    /// Reject malformed, stale-risk, or mechanically corrupted translations.
    Lint {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "max-fuzzy-ratio", default_value_t = 100.0)]
        max_fuzzy_ratio: f64,
    },
    /// Sync English translations from PO catalogs back to canonical sources.
    #[command(name = "sync-english")]
    SyncEnglish {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// Merge committed PO translations into a multilingual RDF graph.
    Merge {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "output", short = 'o')]
        output: Option<PathBuf>,
        #[arg(long = "lang")]
        lang: Option<String>,
    },
    /// Export translated PO catalogs to a flat CSV file.
    #[command(name = "export-csv")]
    ExportCsv {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "output", short = 'o')]
        output: Option<PathBuf>,
    },
    /// Export translated PO catalogs to an XLIFF 1.2 file.
    #[command(name = "export-xliff")]
    ExportXliff {
        #[arg(long = "root")]
        root: Option<PathBuf>,
        #[arg(long = "output", short = 'o')]
        output: Option<PathBuf>,
    },
}

/// `gmeow-dev version` — print the package version to stdout.
fn version() -> i32 {
    println!("{}", env!("CARGO_PKG_VERSION"));
    0
}

/// `gmeow-dev info` — summarize the committed GTS snapshot.
fn info() -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let graph = purrdf::gts::reader::read(&bytes, true, None);
    println!("gmeow.gts");
    println!("  terms        {}", graph.terms.len());
    println!("  quads        {}", graph.quads.len());
    println!("  reifiers     {}", graph.reifiers.len());
    println!("  annotations  {}", graph.annotations.len());
    println!("  docs blobs   {}", graph.blobs.len());
    println!("  opaque       {}", graph.opaque.len());
    for diag in &graph.diagnostics {
        dev_common::note(
            "gmeow-dev.info.diagnostic",
            format!("{}: {}", diag.code, diag.detail),
        );
    }
    0
}

/// `gmeow-dev mcp` — serve the native, repo-anchored MCP developer surface over
/// stdio. Reads the on-disk `generated/dist/gmeow.gts` snapshot from the working
/// tree (like every other dev command) and passes the repository root to
/// [`gmeow_mcp_dev::dev_server`], which registers the four repo-reading maintenance
/// tools (validate/reason/sync/constitution) plus the Constitution resource into the
/// consumer surface through the MCP extension seam. Blocks on the JSON-RPC loop
/// until EOF.
fn mcp() -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let server = match gmeow_mcp_dev::dev_server(&bytes, root) {
        Ok(server) => server,
        Err(e) => return dev_common::fail(format!("mcp: {e}")),
    };
    match server.run_stdio() {
        Ok(()) => 0,
        Err(e) => dev_common::fail(format!("mcp: {e}")),
    }
}

/// Report a selected allocator observation failure through the dev CLI console.
pub fn report_allocator_observation_failure(error: &std::io::Error) {
    dev_common::emit_error("gmeow-dev.allocator-observation.failed", error.to_string());
}

/// Report that the selected stage memory log could not be opened.
pub fn report_stage_memory_log_failure(error: &gmeow_cli_core::StageMemoryLogError) {
    dev_common::emit_error("gmeow-dev.stage-memory-log.failed", error.to_string());
}

/// Parse the arguments, dispatch to the wired backend, and return the exit code.
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let status = error.exit_code();
            if let Err(print_error) = error.print() {
                return dev_common::fail(format!("print CLI help or usage error: {print_error}"));
            }
            return status;
        }
    };
    if dev_producer::requires_admission(&cli.command)
        && let Err(error) = dev_producer::admit()
    {
        return dev_common::fail(format!("producer admission: {error}"));
    }
    let console = cli.console;
    match cli.command {
        Commands::Version => version(),
        Commands::BuildIdentity => {
            println!("{}", gmeow_pipeline::cache::PRODUCER_BUILD_CONTRACT);
            0
        }
        Commands::Info => info(),
        Commands::TestFixtures(TestFixturesArgs {
            mode,
            scope,
            timings_json,
            bundle_import_cache_root,
            expected_source_digest,
        }) => dev_test_fixtures::run(
            mode,
            scope,
            timings_json.as_deref(),
            bundle_import_cache_root.as_deref(),
            expected_source_digest.as_deref(),
        ),
        Commands::SliceSpecWorker(SliceSpecWorkerArgs {
            kind,
            specs,
            workers,
        }) => dev_test_fixtures::run_slice_spec_worker(&kind, &specs, workers),
        Commands::Sync(SyncArgs {
            mode,
            outputs,
            profile,
            jobs,
            metadata,
            list_paths,
            lang,
            timings_json,
            verbose,
        }) => dev_sync::sync(
            mode,
            outputs,
            profile,
            jobs,
            lang.as_deref(),
            timings_json.as_deref(),
            metadata,
            list_paths,
            verbose,
            console,
        ),
        Commands::Fanout(FanoutArgs { jobs, timings_json }) => {
            dev_build::fanout(jobs, timings_json.as_deref(), console)
        }
        Commands::DocsMeasure => dev_docs_measure::docs_measure(),
        Commands::DocsMeasureVerify => dev_docs_measure::docs_measure_verify(),
        Commands::ConsoleAssemble(ConsoleAssembleArgs { out }) => {
            dev_project::console_assemble(&out)
        }
        Commands::DocsPackage(DocsPackageArgs { out }) => dev_docs_package::docs_package(&out),
        Commands::ReleaseBundle(ReleaseBundleArgs {
            out,
            sign_key,
            public_key,
            source,
            issued_at,
            attester,
            release_subject,
            evidence,
        }) => dev_build::release_bundle(
            &out,
            &sign_key,
            &public_key,
            &source,
            &issued_at,
            &attester,
            &release_subject,
            &evidence,
        ),
        Commands::GtsFrameProfile(GtsFrameProfileArgs { gts }) => {
            dev_validate::gts_frame_profile(&gts)
        }
        Commands::MediumGate(MediumGateArgs { gts, registry }) => {
            dev_validate::medium_gate(&gts, &registry)
        }
        Commands::MediumSweep(MediumSweepArgs { out, jobs }) => dev_medium_sweep::run(&out, jobs),
        Commands::MediumSeed(MediumSeedArgs { out }) => dev_medium_sweep::seed(&out),
        Commands::PipelineReads(PipelineReadsArgs { out, jobs }) => {
            dev_pipeline_reads::run(&out, jobs)
        }
        Commands::TermReleaseAuthority(TermReleaseAuthorityArgs { bootstrap }) => {
            dev_term_release_authority::run(bootstrap)
        }
        Commands::Validate(ValidateArgs {
            timings,
            timings_json,
            gts,
            trust_policy,
            require_signed,
            trusted_key,
            deep,
        }) => dev_validate::validate(
            timings,
            timings_json.as_deref(),
            gts.as_deref(),
            trust_policy.as_deref(),
            require_signed,
            trusted_key.as_deref(),
            deep,
        ),
        Commands::Feedback(FeedbackArgs {
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
            timings: _,
        }) => dev_feedback::feedback(
            diagnostics_console.and_then(parse_console),
            diagnostics_artifacts.as_deref(),
            diagnostics_dir.as_deref(),
            diagnostics_stem.as_deref(),
            diagnostics_category.as_deref(),
        ),
        Commands::ExternalTool(ExternalToolArgs {
            name,
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
            command,
        }) => dev_feedback::external_tool(
            &command,
            &name,
            diagnostics_console.and_then(parse_console),
            diagnostics_artifacts.as_deref(),
            diagnostics_dir.as_deref(),
            diagnostics_stem.as_deref(),
            diagnostics_category.as_deref(),
        ),
        Commands::ConstitutionCheck => dev_gates::constitution_check(),
        Commands::Audit(AuditArgs {
            files,
            json,
            strict,
        }) => dev_gates::audit(&files, json, strict),
        Commands::ComplianceReport(ComplianceReportArgs { from_passing_check }) => {
            dev_project::compliance_report(from_passing_check)
        }
        Commands::Reason(ReasonArgs {
            mode,
            fresh,
            timings_json,
            ..
        }) => dev_reason::reason(&mode, fresh, timings_json.as_deref()),
        Commands::Explain => dev_reason::explain(),
        Commands::Verify(VerifyArgs {
            mode,
            fresh,
            timings_json,
            ..
        }) => dev_reason::verify(&mode, fresh, timings_json.as_deref()),
        Commands::ReasonVerify(ReasonVerifyArgs {
            fresh,
            merge: _,
            timings_json,
        }) => dev_reason::reason_verify(fresh, timings_json.as_deref()),
        Commands::Temporal(TemporalArgs {
            query,
            data,
            focus,
            window_start,
            window_end,
            valid_at,
            as_of,
        }) => dev_project::temporal(
            &query,
            data.as_deref(),
            focus.as_deref(),
            window_start.as_deref(),
            window_end.as_deref(),
            valid_at.as_deref(),
            as_of.as_deref(),
        ),
        Commands::Extract(ExtractArgs { target }) => dev_gates::extract(&target),
        Commands::LintAlignment(LintAlignmentArgs { network, strict }) => {
            dev_gates::lint_alignment(network, strict)
        }
        Commands::DocLint => dev_gates::doc_lint(),
        Commands::CrateCheck => dev_gates::crate_check(),
        Commands::FuzzSubstrateCheck => dev_gates::fuzz_substrate_check(),
        Commands::RefreshTargetAxioms(RefreshTargetAxiomsArgs { target }) => {
            dev_project::refresh_target_axioms(&target)
        }
        Commands::Mappings => dev_build::mappings(),
        Commands::Wikidata(WikidataArgs {
            existence,
            fixtures,
        }) => dev_gates::wikidata(existence, fixtures),
        Commands::WikidataCoverage(WikidataCoverageArgs { json, threshold }) => {
            dev_gates::wikidata_coverage(json, threshold)
        }
        Commands::DcCoverage(DcCoverageArgs { json, threshold }) => {
            dev_gates::dc_coverage(json, threshold)
        }
        Commands::UpProjectionAudit(UpProjectionAuditArgs { report, gaps }) => {
            dev_project::up_projection_audit(report.as_deref(), gaps)
        }
        Commands::Coverage(CoverageArgs {
            gaps,
            min_class,
            min_predicate,
        }) => dev_gates::coverage(gaps, min_class, min_predicate),
        Commands::Crossref => dev_project::crossref(),
        Commands::Normalize => dev_build::normalize(),
        Commands::Build => dev_transpile::build(),
        Commands::Project(ProjectArgs {
            source,
            profile,
            data,
            lang,
        }) => dev_transpile::project(source.as_deref(), &profile, &data, lang.as_deref()),
        Commands::Transform(TransformArgs {
            abox,
            out,
            profiles,
            diff_target,
            report,
            lang,
        }) => dev_transpile::transform(
            &abox,
            out.as_deref(),
            &profiles,
            diff_target.as_deref(),
            report.as_deref(),
            lang.as_deref(),
        ),
        Commands::UpProject(UpProjectArgs { source, out }) => {
            dev_transpile::up_project(&source, out.as_deref())
        }
        Commands::Acceptance(AcceptanceArgs {
            source,
            out,
            min_recall,
        }) => dev_gates::acceptance(source.as_deref(), out.as_deref(), min_recall),
        Commands::Quality(QualityArgs { foops_url, strict }) => {
            dev_gates::quality(&foops_url, strict)
        }
        Commands::CompileGts(CompileGtsArgs {
            out,
            sign_key,
            public_key,
            profile,
        }) => dev_build::compile_gts(
            out.as_deref(),
            sign_key.as_deref(),
            public_key.as_deref(),
            profile.build_profile(),
        ),
        Commands::Mcp => mcp(),
        Commands::ImportFoundation(ImportFoundationArgs { jsonl, out, nq }) => {
            dev_project::import_foundation(&jsonl, &out, nq.as_deref())
        }
        Commands::Describe(DescribeArgs { term, gts, lang }) => {
            dev_project::describe(&term, gts.as_deref(), lang.as_deref())
        }
        Commands::ShapeEquivalence(ShapeEquivalenceArgs { path }) => {
            dev_shapes::shape_equivalence(path.as_deref())
        }
        Commands::ConformanceBless(ConformanceBlessArgs { cases, init }) => {
            dev_conformance::conformance_bless(&cases, init)
        }
        Commands::ShapeLift(ShapeLiftArgs { path }) => dev_shapes::shape_lift(path.as_deref()),
        Commands::ShapeMigrate(ShapeMigrateArgs { path, apply, prune }) => {
            if prune {
                dev_shapes::shape_prune(path.as_deref(), apply)
            } else {
                dev_shapes::shape_migrate(path.as_deref(), apply)
            }
        }
        Commands::Certify(CertifyArgs {
            input_path,
            profile,
        }) => dev_reason::certify(&input_path, profile.as_deref()),
        Commands::SliceQuality(SliceQualityArgs {
            path,
            all,
            format,
            min_tier,
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
        }) => dev_slice_quality::slice_quality(
            path.as_deref(),
            all,
            format.as_deref(),
            min_tier.as_deref(),
            diagnostics_console.and_then(parse_console),
            diagnostics_artifacts.as_deref(),
            diagnostics_dir.as_deref(),
            diagnostics_stem.as_deref(),
            diagnostics_category.as_deref(),
        ),
        Commands::SliceQualityGate => dev_slice_quality::slice_quality_gate(),
        Commands::SliceQualitySeedFloors(SliceQualitySeedFloorsArgs { axis, all_axes }) => {
            dev_slice_quality::slice_quality_seed_floors(axis.as_deref(), all_axes)
        }
        Commands::SliceQualitySeedCeilings {} => dev_slice_quality::slice_quality_seed_ceilings(),
        Commands::SliceQualityProjectionDebt {} => {
            dev_slice_quality::slice_quality_projection_debt()
        }
        Commands::SliceQualityRelocationPreview(SliceQualityRelocationPreviewArgs {
            term,
            from,
            to,
        }) => dev_slice_quality::slice_quality_relocation_preview(&term, &from, &to),
        Commands::SliceFixDeps(SliceFixDepsArgs { apply, slices_dir }) => {
            dev_feedback::slice_fix_deps(apply, slices_dir.as_deref())
        }
        Commands::BoxRoles(BoxRolesArgs { command }) => match command {
            BoxRolesCommands::Audit { json } => dev_gates::box_roles_audit(json),
        },
        Commands::Logic(LogicArgs { command }) => match command {
            LogicCommands::Query {
                world,
                query_file,
                profile,
                world_iri,
                max_answers,
                max_steps,
                json,
            } => dev_logic::query(
                &world,
                &query_file,
                &profile,
                world_iri.as_deref(),
                max_answers,
                max_steps,
                json,
            ),
            LogicCommands::Compile { check, mode } => dev_logic::compile(check, mode.as_deref()),
        },
        Commands::I18n(I18nArgs { command }) => match command {
            I18nCommands::Extract {
                root,
                output_dir,
                lang,
                terms_only,
            } => dev_i18n::extract(
                root.as_deref(),
                output_dir.as_deref(),
                lang.as_deref(),
                terms_only,
            ),
            I18nCommands::Lint {
                root,
                max_fuzzy_ratio,
            } => dev_i18n::lint(root.as_deref(), max_fuzzy_ratio),
            I18nCommands::SyncEnglish { root, dry_run } => {
                dev_i18n::sync_english(root.as_deref(), dry_run)
            }
            I18nCommands::Merge { root, output, lang } => {
                dev_i18n::merge(root.as_deref(), output.as_deref(), lang.as_deref())
            }
            I18nCommands::ExportCsv { root, output } => {
                dev_i18n::export_csv(root.as_deref(), output.as_deref())
            }
            I18nCommands::ExportXliff { root, output } => {
                dev_i18n::export_xliff(root.as_deref(), output.as_deref())
            }
        },
    }
}

/// Parse a `--diagnostics-console` spelling into a [`ConsoleMode`].
fn parse_console(value: String) -> Option<ConsoleMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Some(ConsoleMode::Auto),
        "pretty" => Some(ConsoleMode::Pretty),
        "text" => Some(ConsoleMode::Text),
        "jsonl" => Some(ConsoleMode::Jsonl),
        "silent" => Some(ConsoleMode::Silent),
        _ => None,
    }
}
