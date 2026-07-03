// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev-cli` — the repo-maintenance `gmeow-dev` command surface.
//!
//! Unlike the consumer `gmeow` binary, every command here NEEDS the repository:
//! regeneration, fanout, reasoning, verification, mappings, and the i18n
//! toolchain all operate on the working tree. The command variants are stubs for
//! now — this task establishes the top-level clap wiring and the console
//! convention; real command bodies land later.

use clap::{Parser, Subcommand};
use gmeow_cli_core::ConsoleMode;

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

    /// The degree of parallelism for pipeline stages.
    #[arg(long, global = true)]
    pub jobs: Option<usize>,

    #[command(subcommand)]
    pub command: Commands,
}

/// The developer subcommands. Each is a stub for now.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print version information.
    Version,
    /// Print environment and workspace info.
    Info,
    /// Regenerate all generated artifacts from the slices.
    Regenerate,
    /// Run the post-pipeline fanout.
    Fanout,
    /// Assemble a release bundle.
    ReleaseBundle,
    /// Check the generated artifacts for drift.
    CheckGenerated,
    /// Validate the ontology and shapes.
    Validate,
    /// Emit developer feedback diagnostics.
    Feedback,
    /// Run an external tool integration.
    ExternalTool,
    /// Check constitution-gate evidence.
    ConstitutionCheck,
    /// Audit the ontology or provenance.
    Audit,
    /// Emit a compliance report.
    ComplianceReport,
    /// Cross-check competency queries.
    CrosscheckQueries,
    /// Run native reasoning.
    Reason,
    /// Explain a derivation.
    Explain,
    /// Run native verification.
    Verify,
    /// Run reasoning followed by verification.
    ReasonVerify,
    /// Temporal-reasoning operations.
    Temporal,
    /// Extract data from the ontology.
    Extract,
    /// Lint cross-source alignments.
    LintAlignment,
    /// Lint documentation.
    DocLint,
    /// Check the Rust crates.
    CrateCheck,
    /// Refresh target axioms.
    RefreshTargetAxioms,
    /// Emit alignment mappings.
    Mappings,
    /// Wikidata operations.
    Wikidata,
    /// Report Wikidata coverage.
    WikidataCoverage,
    /// Report Dublin Core coverage.
    DcCoverage,
    /// Audit the up-projection.
    UpProjectionAudit,
    /// Report ontology coverage.
    Coverage,
    /// Cross-reference terms across sources.
    Crossref,
    /// Normalize serializations.
    Normalize,
    /// Build a derived artifact.
    Build,
    /// Project the ontology to a target vocabulary.
    Project,
    /// Apply a transform.
    Transform,
    /// Run the up-projection.
    UpProject,
    /// Run the acceptance suite.
    Acceptance,
    /// Run the quality gate.
    Quality,
    /// Compile the ontology to a GTS bundle.
    CompileGts,
    /// Run the MCP server surface.
    Mcp,
    /// Import the foundation corpus.
    ImportFoundation,
    /// Describe an ontology term or resource.
    Describe,
    /// Extract documentation.
    ExtractDocs,
    /// Emit a certification artifact.
    Certify,
    /// Fix slice crate dependencies.
    SliceFixDeps,
    /// Box-role operations.
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
    /// Audit box-role assignments.
    Audit,
}

/// `gmeow-dev logic` subcommands.
#[derive(Debug, Subcommand)]
pub enum LogicCommands {
    /// Run a logic query.
    Query,
    /// Compile the logic core.
    Compile,
}

/// `gmeow-dev i18n` subcommands.
#[derive(Debug, Subcommand)]
pub enum I18nCommands {
    /// Extract translatable strings.
    Extract,
    /// Sync the English source tree.
    SyncEnglish,
    /// Merge translations.
    Merge,
    /// Export translations as CSV.
    ExportCsv,
    /// Export translations as XLIFF.
    ExportXliff,
}

impl Commands {
    /// The stable kebab/lower name used in the stub diagnostic line, including
    /// the nested subcommand where present.
    fn name(&self) -> String {
        let base = match self {
            Commands::Version => "version",
            Commands::Info => "info",
            Commands::Regenerate => "regenerate",
            Commands::Fanout => "fanout",
            Commands::ReleaseBundle => "release-bundle",
            Commands::CheckGenerated => "check-generated",
            Commands::Validate => "validate",
            Commands::Feedback => "feedback",
            Commands::ExternalTool => "external-tool",
            Commands::ConstitutionCheck => "constitution-check",
            Commands::Audit => "audit",
            Commands::ComplianceReport => "compliance-report",
            Commands::CrosscheckQueries => "crosscheck-queries",
            Commands::Reason => "reason",
            Commands::Explain => "explain",
            Commands::Verify => "verify",
            Commands::ReasonVerify => "reason-verify",
            Commands::Temporal => "temporal",
            Commands::Extract => "extract",
            Commands::LintAlignment => "lint-alignment",
            Commands::DocLint => "doc-lint",
            Commands::CrateCheck => "crate-check",
            Commands::RefreshTargetAxioms => "refresh-target-axioms",
            Commands::Mappings => "mappings",
            Commands::Wikidata => "wikidata",
            Commands::WikidataCoverage => "wikidata-coverage",
            Commands::DcCoverage => "dc-coverage",
            Commands::UpProjectionAudit => "up-projection-audit",
            Commands::Coverage => "coverage",
            Commands::Crossref => "crossref",
            Commands::Normalize => "normalize",
            Commands::Build => "build",
            Commands::Project => "project",
            Commands::Transform => "transform",
            Commands::UpProject => "up-project",
            Commands::Acceptance => "acceptance",
            Commands::Quality => "quality",
            Commands::CompileGts => "compile-gts",
            Commands::Mcp => "mcp",
            Commands::ImportFoundation => "import-foundation",
            Commands::Describe => "describe",
            Commands::ExtractDocs => "extract-docs",
            Commands::Certify => "certify",
            Commands::SliceFixDeps => "slice-fix-deps",
            Commands::BoxRoles { command } => {
                return format!("box-roles {}", command.name());
            }
            Commands::Logic { command } => {
                return format!("logic {}", command.name());
            }
            Commands::I18n { command } => {
                return format!("i18n {}", command.name());
            }
        };
        base.to_owned()
    }
}

impl BoxRolesCommands {
    fn name(&self) -> &'static str {
        match self {
            BoxRolesCommands::Audit => "audit",
        }
    }
}

impl LogicCommands {
    fn name(&self) -> &'static str {
        match self {
            LogicCommands::Query => "query",
            LogicCommands::Compile => "compile",
        }
    }
}

impl I18nCommands {
    fn name(&self) -> &'static str {
        match self {
            I18nCommands::Extract => "extract",
            I18nCommands::SyncEnglish => "sync-english",
            I18nCommands::Merge => "merge",
            I18nCommands::ExportCsv => "export-csv",
            I18nCommands::ExportXliff => "export-xliff",
        }
    }
}

/// Parse the arguments, dispatch, and return the process exit code.
///
/// Every command is a stub: it prints `unimplemented: <name>` to stderr and
/// returns `0`. clap emits its own usage errors (exit `2`) before this returns.
pub fn run() -> i32 {
    let cli = Cli::parse();
    eprintln!("unimplemented: {}", cli.command.name());
    0
}
