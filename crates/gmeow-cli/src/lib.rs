// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-cli` — the consumer/wheel-only `gmeow` command surface.
//!
//! This crate is the shippable CLI a wheel installs: every subcommand it exposes
//! runs WITHOUT the repository (the `gmeow` vs `gmeow-dev` razor). The command
//! variants here are stubs for now — the top-level clap wiring and the console
//! convention are what this task establishes; real command bodies land later.

use clap::{Parser, Subcommand};
use gmeow_cli_core::ConsoleMode;

/// The `gmeow` consumer CLI.
#[derive(Debug, Parser)]
#[command(name = "gmeow", version, about = "The GMEOW ontology consumer CLI.")]
pub struct Cli {
    /// The console output surface (flag > `GMEOW_CONSOLE` env > auto).
    #[arg(long, global = true, value_enum)]
    pub console: Option<ConsoleMode>,

    /// The natural-language tree to surface (e.g. `en`).
    #[arg(long, global = true)]
    pub lang: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

/// The consumer subcommands. Each is a stub for now.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print version information.
    Version,
    /// Print environment and bundle info.
    Info,
    /// Verify a bundle or artifact.
    Verify,
    /// Verify a signed release bundle.
    VerifyReleaseBundle,
    /// Describe an ontology term or resource.
    Describe,
    /// Validate data against the ontology shapes.
    Validate,
    /// Build a derived artifact.
    Build,
    /// Project the ontology to a target vocabulary.
    Project,
    /// Transpile logic to another dialect.
    Transpile,
    /// Export the bundle to a serialization.
    Export,
    /// Convert between serializations.
    Convert,
    /// Extract documentation.
    ExtractDocs,
    /// Cross-reference terms across sources.
    Crossref,
    /// Run the MCP server surface.
    Mcp,
    /// GTS bundle operations.
    Gts,
    /// Music-package operations.
    Music,
}

impl Commands {
    /// The stable kebab/lower name used in the stub diagnostic line.
    fn name(&self) -> &'static str {
        match self {
            Commands::Version => "version",
            Commands::Info => "info",
            Commands::Verify => "verify",
            Commands::VerifyReleaseBundle => "verify-release-bundle",
            Commands::Describe => "describe",
            Commands::Validate => "validate",
            Commands::Build => "build",
            Commands::Project => "project",
            Commands::Transpile => "transpile",
            Commands::Export => "export",
            Commands::Convert => "convert",
            Commands::ExtractDocs => "extract-docs",
            Commands::Crossref => "crossref",
            Commands::Mcp => "mcp",
            Commands::Gts => "gts",
            Commands::Music => "music",
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
