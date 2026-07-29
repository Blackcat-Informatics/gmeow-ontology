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

mod dev_build;
mod dev_common;
mod dev_docs_measure;
mod dev_docs_package;
mod dev_feedback;
mod dev_gates;
mod dev_i18n;
mod dev_logic;
mod dev_project;
mod dev_reason;
mod dev_shapes;
mod dev_slice_quality;
mod dev_sync;
mod dev_targets;
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

/// The developer subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print the gmeow package version.
    Version,
    /// Show a summary of the bundled GMEOW ontology snapshot.
    Info,
    /// Run the canonical one-pass pipeline, strict gates, and output projections.
    Sync {
        /// Update locally or verify read-only. Defaults to check in CI, update elsewhere.
        #[arg(long = "mode", value_enum)]
        mode: Option<SyncMode>,
        /// Projection set. All outputs are made by default.
        #[arg(long = "outputs", value_enum, default_value_t = SyncOutput::All)]
        outputs: SyncOutput,
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
    },
    /// Project the flat consumer tree back out of gmeow.gts.
    Fanout {
        #[arg(short = 'j', long = "jobs")]
        jobs: Option<usize>,
        #[arg(long = "timings-json")]
        timings_json: Option<PathBuf>,
    },
    /// Measure real, deterministic per-format documentation byte sizes and the
    /// three external-distribution design totals.
    #[command(name = "docs-measure")]
    DocsMeasure,
    /// Package the materialized `dist/gmeow-docs/` external documentation
    /// distribution into one deterministic content-addressed release asset,
    /// alongside a `.blake3` sidecar for the DCAT release manifest.
    #[command(name = "docs-package")]
    DocsPackage {
        #[arg(long = "out", default_value = "dist/gmeow-docs.tar")]
        out: PathBuf,
    },
    /// Fold check/conformance/SARIF evidence into a SIGNED gmeow.gts.
    #[command(name = "release-bundle")]
    ReleaseBundle {
        #[arg(long = "out", default_value = "dist/gmeow.gts")]
        out: PathBuf,
        #[arg(long = "sign-key")]
        sign_key: PathBuf,
        #[arg(long = "public-key")]
        public_key: PathBuf,
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
    },
    /// Audit the mandatory zstd-rsyncable frame profile of a GTS bundle.
    #[command(name = "gts-frame-profile")]
    GtsFrameProfile {
        #[arg(default_value = "generated/dist/gmeow.gts")]
        gts: PathBuf,
    },
    /// Audit the whole MEDIUM axis of any GMEOW-authored GTS artifact — the dist
    /// bundle, a runtime `~/.gmeow/*.gts` store, or a whole-artifact product.
    #[command(name = "medium-gate")]
    MediumGate {
        #[arg(default_value = "generated/dist/gmeow.gts")]
        gts: PathBuf,
        /// The bundle whose medium registry an artifact that carries none of its own
        /// (a runtime store) is audited against — the bundle its dictionaries came from.
        #[arg(long = "registry", default_value = "generated/dist/gmeow.gts")]
        registry: PathBuf,
    },
    /// Validate Turtle syntax, term annotations, and SHACL conformance.
    Validate {
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
    },
    /// Write first-class diagnostics artifacts for the whole dev gate.
    Feedback {
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
    },
    /// Run an external gate tool and represent a failure as a canonical finding.
    #[command(name = "external-tool")]
    ExternalTool {
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
    },
    /// Verify every constitutional principle has live enforcement.
    #[command(name = "constitution-check")]
    ConstitutionCheck,
    /// Audit claims (ungrounded / contradicted / stale) over data files.
    Audit {
        files: Vec<PathBuf>,
        #[arg(long = "json")]
        json: bool,
        #[arg(long = "strict")]
        strict: bool,
    },
    /// Emit the RDF compliance report.
    #[command(name = "compliance-report")]
    ComplianceReport {
        #[arg(long = "from-passing-check")]
        from_passing_check: bool,
    },
    /// Reason over the ontology (native EL/DL, Docker-free).
    Reason {
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
    },
    /// Explain unsatisfiable classes / inconsistency.
    Explain,
    /// Run reasoned-graph negative tests (native).
    Verify {
        #[arg(long = "mode", default_value = "native")]
        mode: String,
        /// Recompute the closure instead of reusing the shipped graph/reasoning verdict.
        #[arg(long = "fresh")]
        fresh: bool,
        #[arg(long = "reasoned-input")]
        reasoned_input: Option<PathBuf>,
        #[arg(long = "timings-json")]
        timings_json: Option<PathBuf>,
    },
    /// Run native reasoning followed by reasoned-graph verify.
    #[command(name = "reason-verify")]
    ReasonVerify {
        /// Recompute the closure instead of reusing the shipped graph/reasoning verdict.
        #[arg(long = "fresh")]
        fresh: bool,
        #[arg(long = "merge")]
        merge: bool,
        #[arg(long = "timings-json")]
        timings_json: Option<PathBuf>,
    },
    /// Run a TQL (Temporal Query Language) query over the events model.
    Temporal {
        query: String,
        #[arg(long = "data")]
        data: Option<PathBuf>,
        #[arg(long = "focus")]
        focus: Option<String>,
        #[arg(long = "window-start")]
        window_start: Option<String>,
        #[arg(long = "window-end")]
        window_end: Option<String>,
        #[arg(long = "valid-at")]
        valid_at: Option<String>,
        #[arg(long = "as-of")]
        as_of: Option<String>,
    },
    /// Report the import/extract policy for an alignment target.
    Extract {
        #[arg(long = "target")]
        target: String,
    },
    /// Lint SSSOM property mappings for direction/domain-range mismatches.
    #[command(name = "lint-alignment")]
    LintAlignment {
        #[arg(long = "network")]
        network: bool,
        #[arg(long = "strict")]
        strict: bool,
    },
    /// Lint the rust-rendered ontology-docs site.
    #[command(name = "doc-lint")]
    DocLint,
    /// Verify Rust crate layering and repository-static policy.
    #[command(name = "crate-check")]
    CrateCheck,
    /// Re-vendor minimal target-axiom snapshots (network).
    #[command(name = "refresh-target-axioms")]
    RefreshTargetAxioms {
        #[arg(long = "target", default_value = "all")]
        target: String,
    },
    /// Build alignment axioms + VoID linksets from SSSOM.
    Mappings,
    /// Validate the Wikidata QIDs/PIDs used in the mappings.
    Wikidata {
        #[arg(long = "existence")]
        existence: bool,
        #[arg(long = "fixtures")]
        fixtures: bool,
    },
    /// Report Wikidata mapping coverage by domain/module.
    #[command(name = "wikidata-coverage")]
    WikidataCoverage {
        #[arg(long = "json")]
        json: bool,
        #[arg(long = "threshold", default_value_t = 0.5)]
        threshold: f64,
    },
    /// Report Dublin Core mapping coverage by namespace.
    #[command(name = "dc-coverage")]
    DcCoverage {
        #[arg(long = "json")]
        json: bool,
        #[arg(long = "threshold", default_value_t = 0.5)]
        threshold: f64,
    },
    /// Audit consumer→GMEOW up-projection invertibility.
    #[command(name = "up-projection-audit")]
    UpProjectionAudit {
        #[arg(long = "report")]
        report: Option<PathBuf>,
        #[arg(long = "gaps")]
        gaps: bool,
    },
    /// Report how much of the vendored entity slice GMEOW covers.
    Coverage {
        #[arg(long = "gaps")]
        gaps: bool,
        #[arg(long = "min-class")]
        min_class: Option<f64>,
        #[arg(long = "min-predicate")]
        min_predicate: Option<f64>,
    },
    /// Generate the CrossRef DOI deposit XML.
    Crossref,
    /// Canonicalize the authored ontology sources.
    Normalize,
    /// Build serializations and OWL-native syntaxes into dist/.
    Build,
    /// Project GMEOW to a pure vocabulary profile.
    Project {
        source: Option<PathBuf>,
        #[arg(long = "profile", default_value = "all")]
        profile: String,
        #[arg(long = "data", default_value = "")]
        data: String,
        #[arg(long = "lang", short = 'l')]
        lang: Option<String>,
    },
    /// Transpile an A-Box to MAXIMAL(G).
    Transform {
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
    },
    /// Lift a consumer-vocabulary RDF file UP into pure GMEOW.
    #[command(name = "up-project")]
    UpProject {
        source: PathBuf,
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
    },
    /// Score the full transpile against real data.
    Acceptance {
        source: Option<PathBuf>,
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
        #[arg(long = "min-recall")]
        min_recall: Option<f64>,
    },
    /// Run OOPS! (pitfalls) and optionally FOOPS! (FAIR).
    Quality {
        #[arg(long = "foops-url", default_value = "")]
        foops_url: String,
        #[arg(long = "strict")]
        strict: bool,
    },
    /// Compile the statement-complete GTS dist snapshot.
    #[command(name = "compile-gts")]
    CompileGts {
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
        #[arg(long = "sign-key")]
        sign_key: Option<PathBuf>,
        #[arg(long = "public-key")]
        public_key: Option<PathBuf>,
    },
    /// Start the GMEOW MCP server (wired in the MCP task).
    Mcp,
    /// Import the foundation corpus.
    #[command(name = "import-foundation")]
    ImportFoundation {
        jsonl: PathBuf,
        #[arg(long = "out", default_value = "build/foundation")]
        out: PathBuf,
        #[arg(long = "nq")]
        nq: Option<PathBuf>,
    },
    /// Describe a GMEOW term as useful prose.
    Describe {
        term: String,
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
        #[arg(long = "lang", short = 'l')]
        lang: Option<String>,
    },
    /// Prove legacy `shapes.ttl` blocks are reproduced by the projected validation shapes.
    #[command(name = "shape-equivalence")]
    ShapeEquivalence {
        /// Restrict the scan (and the exit code) to legacy `shapes.ttl` under this directory
        /// (repo-relative or absolute); default scans every slice.
        #[arg(long = "path")]
        path: Option<PathBuf>,
    },
    /// Propose (and certify) the OWL antecedent that would ground each legacy `shapes.ttl` block.
    #[command(name = "shape-lift")]
    ShapeLift {
        /// Restrict the scan (and the exit code) to legacy `shapes.ttl` under this directory
        /// (repo-relative or absolute); default scans every slice.
        #[arg(long = "path")]
        path: Option<PathBuf>,
    },
    /// Automated shape migration: inject the lifted OWL grounding into each class's module.ttl
    /// (`--apply`), then delete the now-equivalent `shapes.ttl` blocks (`--prune`, after a
    /// regenerate). Default is a dry-run report.
    #[command(name = "shape-migrate")]
    ShapeMigrate {
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
    },
    /// Statically certify a logic program against its declared profile.
    Certify {
        input_path: PathBuf,
        #[arg(long = "profile")]
        profile: Option<String>,
    },
    /// Score a slice against the slice-quality rubric and emit ranked uplift advice.
    #[command(name = "slice-quality")]
    SliceQuality {
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
    },
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
    SliceQualitySeedFloors {
        /// Seed exactly one named rubric axis (its IRI local name, e.g.
        /// axisShapeMigration). Exactly one of --axis / --all-axes is required.
        #[arg(long = "axis")]
        axis: Option<String>,
        /// Seed every rubric axis that lacks a committed floor for a given slice.
        /// Exactly one of --axis / --all-axes is required.
        #[arg(long = "all-axes")]
        all_axes: bool,
    },
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
    /// Propose manifest dependency edits as a reviewable unified diff.
    #[command(name = "slice-fix-deps")]
    SliceFixDeps {
        #[arg(long = "apply")]
        apply: bool,
        #[arg(long = "slices-dir")]
        slices_dir: Option<PathBuf>,
    },
    /// Audit graph-box role coverage in authored sources.
    #[command(name = "box-roles")]
    BoxRoles {
        #[command(subcommand)]
        command: BoxRolesCommands,
    },
    /// Logic-stack operations.
    Logic {
        #[command(subcommand)]
        command: LogicCommands,
    },
    /// Internationalization toolchain.
    I18n {
        #[command(subcommand)]
        command: I18nCommands,
    },
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
        #[arg(long = "mode")]
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
/// tree (like every other dev command) and passes the repository root so the
/// [`McpMode::Dev`](gmeow_pipeline::mcp::McpMode::Dev) repo-reading maintenance
/// tools (validate/reason/sync/constitution) are exposed alongside the
/// consumer surface. Blocks on the JSON-RPC loop until EOF.
fn mcp() -> i32 {
    use gmeow_pipeline::mcp::{McpMode, McpServer};
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let server = match McpServer::from_snapshot(&bytes, Some(root), McpMode::Dev) {
        Ok(server) => server,
        Err(e) => return dev_common::fail(format!("mcp: {e}")),
    };
    match server.run_stdio() {
        Ok(()) => 0,
        Err(e) => dev_common::fail(format!("mcp: {e}")),
    }
}

/// Parse the arguments, dispatch to the wired backend, and return the exit code.
pub fn run() -> i32 {
    let cli = Cli::parse();
    let console = cli.console;
    match cli.command {
        Commands::Version => version(),
        Commands::Info => info(),
        Commands::Sync {
            mode,
            outputs,
            jobs,
            metadata,
            list_paths,
            lang,
            timings_json,
            verbose,
        } => dev_sync::sync(
            mode,
            outputs,
            jobs,
            lang.as_deref(),
            timings_json.as_deref(),
            metadata,
            list_paths,
            verbose,
            console,
        ),
        Commands::Fanout { jobs, timings_json } => {
            dev_build::fanout(jobs, timings_json.as_deref(), console)
        }
        Commands::DocsMeasure => dev_docs_measure::docs_measure(),
        Commands::DocsPackage { out } => dev_docs_package::docs_package(&out),
        Commands::ReleaseBundle {
            out,
            sign_key,
            public_key,
            source,
            issued_at,
            attester,
            release_subject,
            evidence,
        } => dev_build::release_bundle(
            &out,
            &sign_key,
            &public_key,
            &source,
            &issued_at,
            &attester,
            &release_subject,
            &evidence,
        ),
        Commands::GtsFrameProfile { gts } => dev_validate::gts_frame_profile(&gts),
        Commands::MediumGate { gts, registry } => dev_validate::medium_gate(&gts, &registry),
        Commands::Validate {
            timings,
            timings_json,
            gts,
            trust_policy,
            require_signed,
            trusted_key,
            deep,
        } => dev_validate::validate(
            timings,
            timings_json.as_deref(),
            gts.as_deref(),
            trust_policy.as_deref(),
            require_signed,
            trusted_key.as_deref(),
            deep,
        ),
        Commands::Feedback {
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
            timings: _,
        } => dev_feedback::feedback(
            diagnostics_console.and_then(parse_console),
            diagnostics_artifacts.as_deref(),
            diagnostics_dir.as_deref(),
            diagnostics_stem.as_deref(),
            diagnostics_category.as_deref(),
        ),
        Commands::ExternalTool {
            name,
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
            command,
        } => dev_feedback::external_tool(
            &command,
            &name,
            diagnostics_console.and_then(parse_console),
            diagnostics_artifacts.as_deref(),
            diagnostics_dir.as_deref(),
            diagnostics_stem.as_deref(),
            diagnostics_category.as_deref(),
        ),
        Commands::ConstitutionCheck => dev_gates::constitution_check(),
        Commands::Audit {
            files,
            json,
            strict,
        } => dev_gates::audit(&files, json, strict),
        Commands::ComplianceReport { from_passing_check } => {
            dev_project::compliance_report(from_passing_check)
        }
        Commands::Reason {
            mode,
            fresh,
            timings_json,
            ..
        } => dev_reason::reason(&mode, fresh, timings_json.as_deref()),
        Commands::Explain => dev_reason::explain(),
        Commands::Verify {
            mode,
            fresh,
            timings_json,
            ..
        } => dev_reason::verify(&mode, fresh, timings_json.as_deref()),
        Commands::ReasonVerify {
            fresh,
            merge: _,
            timings_json,
        } => dev_reason::reason_verify(fresh, timings_json.as_deref()),
        Commands::Temporal {
            query,
            data,
            focus,
            window_start,
            window_end,
            valid_at,
            as_of,
        } => dev_project::temporal(
            &query,
            data.as_deref(),
            focus.as_deref(),
            window_start.as_deref(),
            window_end.as_deref(),
            valid_at.as_deref(),
            as_of.as_deref(),
        ),
        Commands::Extract { target } => dev_gates::extract(&target),
        Commands::LintAlignment { network, strict } => dev_gates::lint_alignment(network, strict),
        Commands::DocLint => dev_gates::doc_lint(),
        Commands::CrateCheck => dev_gates::crate_check(),
        Commands::RefreshTargetAxioms { target } => dev_project::refresh_target_axioms(&target),
        Commands::Mappings => dev_build::mappings(),
        Commands::Wikidata {
            existence,
            fixtures,
        } => dev_gates::wikidata(existence, fixtures),
        Commands::WikidataCoverage { json, threshold } => {
            dev_gates::wikidata_coverage(json, threshold)
        }
        Commands::DcCoverage { json, threshold } => dev_gates::dc_coverage(json, threshold),
        Commands::UpProjectionAudit { report, gaps } => {
            dev_project::up_projection_audit(report.as_deref(), gaps)
        }
        Commands::Coverage {
            gaps,
            min_class,
            min_predicate,
        } => dev_gates::coverage(gaps, min_class, min_predicate),
        Commands::Crossref => dev_project::crossref(),
        Commands::Normalize => dev_build::normalize(),
        Commands::Build => dev_transpile::build(),
        Commands::Project {
            source,
            profile,
            data,
            lang,
        } => dev_transpile::project(source.as_deref(), &profile, &data, lang.as_deref()),
        Commands::Transform {
            abox,
            out,
            profiles,
            diff_target,
            report,
            lang,
        } => dev_transpile::transform(
            &abox,
            out.as_deref(),
            &profiles,
            diff_target.as_deref(),
            report.as_deref(),
            lang.as_deref(),
        ),
        Commands::UpProject { source, out } => dev_transpile::up_project(&source, out.as_deref()),
        Commands::Acceptance {
            source,
            out,
            min_recall,
        } => dev_gates::acceptance(source.as_deref(), out.as_deref(), min_recall),
        Commands::Quality { foops_url, strict } => dev_gates::quality(&foops_url, strict),
        Commands::CompileGts {
            out,
            sign_key,
            public_key,
        } => dev_build::compile_gts(out.as_deref(), sign_key.as_deref(), public_key.as_deref()),
        Commands::Mcp => mcp(),
        Commands::ImportFoundation { jsonl, out, nq } => {
            dev_project::import_foundation(&jsonl, &out, nq.as_deref())
        }
        Commands::Describe { term, gts, lang } => {
            dev_project::describe(&term, gts.as_deref(), lang.as_deref())
        }
        Commands::ShapeEquivalence { path } => dev_shapes::shape_equivalence(path.as_deref()),
        Commands::ShapeLift { path } => dev_shapes::shape_lift(path.as_deref()),
        Commands::ShapeMigrate { path, apply, prune } => {
            if prune {
                dev_shapes::shape_prune(path.as_deref(), apply)
            } else {
                dev_shapes::shape_migrate(path.as_deref(), apply)
            }
        }
        Commands::Certify {
            input_path,
            profile,
        } => dev_reason::certify(&input_path, profile.as_deref()),
        Commands::SliceQuality {
            path,
            all,
            format,
            min_tier,
            diagnostics_console,
            diagnostics_artifacts,
            diagnostics_dir,
            diagnostics_stem,
            diagnostics_category,
        } => dev_slice_quality::slice_quality(
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
        Commands::SliceQualitySeedFloors { axis, all_axes } => {
            dev_slice_quality::slice_quality_seed_floors(axis.as_deref(), all_axes)
        }
        Commands::SliceQualitySeedCeilings {} => dev_slice_quality::slice_quality_seed_ceilings(),
        Commands::SliceQualityProjectionDebt {} => {
            dev_slice_quality::slice_quality_projection_debt()
        }
        Commands::SliceFixDeps { apply, slices_dir } => {
            dev_feedback::slice_fix_deps(apply, slices_dir.as_deref())
        }
        Commands::BoxRoles { command } => match command {
            BoxRolesCommands::Audit { json } => dev_gates::box_roles_audit(json),
        },
        Commands::Logic { command } => match command {
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
        Commands::I18n { command } => match command {
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
