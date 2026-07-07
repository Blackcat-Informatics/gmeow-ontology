// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-cli` — the consumer/wheel-only `gmeow` command surface.
//!
//! This crate is the shippable CLI a wheel installs: every subcommand it exposes
//! runs WITHOUT the repository (the `gmeow` vs `gmeow-dev` razor). It is a single
//! self-contained, repo-free binary — the canonical `generated/dist/gmeow.gts`
//! snapshot is [embedded](BUNDLE_GTS) with `include_bytes!`, so every command that
//! defaults to "the bundle" reads those baked-in bytes unless the user passes a
//! file or `--gts`.
//!
//! The command bodies are thin: each one marshals its inputs and delegates to an
//! already-native backend (`gmeow_docs`, `gmeow_validate`, `gmeow_pipeline`,
//! `gmeow_music`, `purrdf`). The console convention is the shared one from
//! [`gmeow_cli_core`]: product results → stdout, errors/diagnostics → stderr,
//! exit `0` on success, `1` on a handled failure, `2` for clap usage errors, and
//! a passthrough child's own exit code for `gts` / `music`.

mod commands;
mod passthrough;

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use gmeow_cli_core::ConsoleMode;

/// The embedded canonical GMEOW snapshot: the whole ontology + transforms, folded
/// into one GTS bundle, baked into the binary so `gmeow` needs no repository, no
/// generator inputs, and no network. Every command that defaults to "the bundle"
/// reads these bytes unless the user supplies a file / `--gts`.
pub const BUNDLE_GTS: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../generated/dist/gmeow.gts"
));

/// The GMEOW IRI namespace the discipline checks and term catalog key on.
pub(crate) const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `gmeow` consumer CLI.
#[derive(Debug, Parser)]
#[command(name = "gmeow", version, about = "The GMEOW ontology consumer CLI.")]
pub struct Cli {
    /// The console output surface (flag > `GMEOW_CONSOLE` env > auto).
    #[arg(long, global = true, value_enum)]
    pub console: Option<ConsoleMode>,

    /// Language(s) for emitted labels and definitions: a BCP-47 tag (`en`, `zh`,
    /// `fr`) or an internal tag (`x-gmeow-english`). Comma-separated for multiple.
    /// Overrides `GMEOW_LANG`; an empty value (`--lang ''`) selects the default
    /// English carrier.
    #[arg(long, global = true)]
    pub lang: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

/// The consumer subcommands.
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Print the gmeow package version.
    Version,
    /// Show a summary of a GMEOW ontology snapshot (default: the bundle).
    Info {
        /// GTS snapshot to inspect (default: bundled gmeow.gts).
        file: Option<PathBuf>,
    },
    /// Verify GTS signatures and the source-free ontology checks.
    Verify {
        /// GTS snapshot to verify (default: bundled gmeow.gts).
        file: Option<PathBuf>,
        /// Out-of-band armored OpenPGP public key (overrides the embedded key).
        #[arg(long = "trusted-key")]
        trusted_key: Option<PathBuf>,
        /// Permit unsigned local snapshots.
        #[arg(long = "allow-unsigned")]
        allow_unsigned: bool,
    },
    /// Consumer verification of a signed release bundle.
    #[command(name = "verify-release-bundle")]
    VerifyReleaseBundle {
        /// Signed release bundle to verify.
        #[arg(long = "bundle")]
        bundle: PathBuf,
        /// Optional out-of-band trusted Ed25519 OpenPGP PUBLIC certificate.
        #[arg(long = "public-key")]
        public_key: Option<PathBuf>,
    },
    /// Describe a GMEOW term as useful prose from a GTS snapshot.
    Describe {
        /// A GMEOW term: `gmeow:X`, a local name, or a prefix.
        term: String,
        /// Describe from this `.gts` package instead of the bundle.
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
    },
    /// Validate RDF data against the bundle, or a JSON/YAML instance against a schema.
    Validate {
        /// RDF data or a JSON/YAML instance.
        instance: PathBuf,
        /// JSON Schema to validate against (forces JSON-Schema instance mode).
        #[arg(long = "schema", short = 's')]
        schema: Option<PathBuf>,
        /// Output for RDF conformance: `human`, `sarif`, or `json`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
        /// Opt-in Tier-2 native semantic pass over your data merged with the bundle.
        #[arg(long = "deep")]
        deep: bool,
    },
    /// Build derived serializations from a GTS snapshot.
    Build {
        /// Output directory for derived serializations.
        #[arg(long = "out", short = 'o', default_value = "dist/bundle")]
        out: PathBuf,
        /// GTS snapshot to serialize (default: bundled snapshot).
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
    },
    /// Project GMEOW to a pure schema.org / FOAF / vCard / … profile.
    Project {
        /// A GMEOW data file (.ttl), a transpiled `.gts` to filter, or nothing
        /// for the bundled snapshot.
        source: Option<PathBuf>,
        /// View/profile: `gmeow`, `all`, `maximal`, or a vocabulary profile.
        #[arg(long = "profile", default_value = "gmeow")]
        profile: String,
        /// Output directory.
        #[arg(long = "out", short = 'o', default_value = "dist/project")]
        out: PathBuf,
        /// Output serialization: `turtle` or `yaml-ld`.
        #[arg(long = "format", short = 'f', default_value = "turtle")]
        format: String,
    },
    /// Transpile consumer RDF → pure GMEOW → MAXIMAL multi-vocab.
    Transpile {
        /// A non-GMEOW source RDF file, an OKF bundle directory, or `-` for stdin.
        source: PathBuf,
        /// Output directory (default `dist/transpile/<stem>/`).
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
        /// Projection profiles for the maximal pass: `all` or `name,…`.
        #[arg(long = "profiles", default_value = "all")]
        profiles: String,
    },
    /// Export flat consumer views from a GTS snapshot.
    Export {
        /// Output directory.
        #[arg(long = "out", short = 'o', default_value = "dist/export")]
        out: PathBuf,
        /// GTS snapshot to export (default: bundled snapshot).
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
    },
    /// Transcode any RDF-1.2 syntax/projection to any other, recording loss.
    Convert {
        /// Input RDF document, or `-` to read from stdin.
        source: String,
        /// Source codec.
        #[arg(long = "from")]
        from: String,
        /// Target codec.
        #[arg(long = "to")]
        to: String,
        /// Output path (default: stdout).
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
        /// Write the realized loss ledger (JSON) here (default: stderr summary).
        #[arg(long = "loss-report")]
        loss_report: Option<PathBuf>,
        /// Base IRI for relative-IRI resolution.
        #[arg(long = "base")]
        base: Option<String>,
    },
    /// Extract the browsable docs tree from a GTS snapshot.
    #[command(name = "extract-docs")]
    ExtractDocs {
        /// Output directory for the docs tree.
        #[arg(long = "directory", short = 'd')]
        directory: PathBuf,
        /// GTS snapshot to document (default: bundled gmeow.gts).
        file: Option<PathBuf>,
        /// Write into a non-empty output directory.
        #[arg(long = "force")]
        force: bool,
    },
    /// Generate CrossRef DOI deposit XML from bundled self-description data.
    Crossref {
        /// Output XML path.
        #[arg(long = "out", short = 'o', default_value = "dist/crossref-deposit.xml")]
        out: PathBuf,
        /// GTS snapshot with self-description metadata.
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
    },
    /// Start the consumer-safe GMEOW MCP server (stdio transport).
    Mcp,
    /// Dispatch to the external Graph Transport Substrate (GTS) CLI.
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Gts {
        /// Arguments forwarded verbatim to the `gts` binary.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// GMEOW music-package projection tools.
    Music {
        #[command(subcommand)]
        command: MusicCommands,
    },
    /// GMEOW affect-intensity geometry tools.
    Affect {
        #[command(subcommand)]
        command: AffectCommands,
    },
}

/// The `gmeow affect` nested subcommands (native `gmeow_affect` engine).
#[derive(Debug, Subcommand)]
pub enum AffectCommands {
    /// Compute the affect-intensity geometry of derived-intensity observations
    /// in a GTS snapshot (metric-tensor norm √(xᵀGx), never a raw L²).
    Intensity {
        /// Source `.gts` snapshot carrying the affect-intensity observation(s).
        source: PathBuf,
        /// A single `gmeow:DerivedAffectIntensityObservation` IRI to compute
        /// (default: every derived-intensity observation in the snapshot).
        #[arg(long)]
        observation: Option<String>,
        /// A second observation IRI: report the metric distance and cosine
        /// between it and `--observation` (requires `--observation`).
        #[arg(long, requires = "observation")]
        to: Option<String>,
    },
}

/// The `gmeow music` nested subcommands (native `gmeow_music` engine).
#[derive(Debug, Subcommand)]
pub enum MusicCommands {
    /// Project a GTS music-package to a notation format.
    Render {
        /// Source `.gts` music-package file.
        source: PathBuf,
        /// Output format.
        #[arg(long = "to")]
        to: String,
        /// Output file.
        #[arg(long = "out", short = 'o')]
        out: PathBuf,
    },
    /// Project a MusicXML file into a GTS music-package.
    Import {
        /// Source MusicXML file.
        source: PathBuf,
        /// Output `.gts` file.
        #[arg(long = "out", short = 'o')]
        out: PathBuf,
    },
}

/// Resolve the effective [`ConsoleMode`] from the flag, the `GMEOW_CONSOLE`
/// environment value, and whether stderr is a TTY.
pub(crate) fn resolve_console(flag: Option<ConsoleMode>) -> ConsoleMode {
    use std::io::IsTerminal;
    let env_val = std::env::var("GMEOW_CONSOLE").ok();
    ConsoleMode::resolve(flag, env_val.as_deref(), std::io::stderr().is_terminal())
}

/// Parse the arguments, dispatch to the wired backend, and return the process
/// exit code. clap emits its own usage errors (exit `2`) before this returns.
pub fn run() -> i32 {
    let cli = Cli::parse();
    let console = resolve_console(cli.console);
    let lang = cli.lang;
    match cli.command {
        Commands::Version => commands::version(),
        Commands::Info { file } => commands::info(file.as_deref()),
        Commands::Verify {
            file,
            trusted_key,
            allow_unsigned,
        } => commands::verify(file.as_deref(), trusted_key.as_deref(), allow_unsigned),
        Commands::VerifyReleaseBundle { bundle, public_key } => {
            commands::verify_release_bundle(&bundle, public_key.as_deref())
        }
        Commands::Describe { term, gts } => {
            commands::describe(&term, gts.as_deref(), lang.as_deref())
        }
        Commands::Validate {
            instance,
            schema,
            format,
            deep,
        } => commands::validate(&instance, schema.as_deref(), &format, deep, console),
        Commands::Build { out, gts } => commands::build(&out, gts.as_deref()),
        Commands::Project {
            source,
            profile,
            out,
            format,
        } => commands::project(source.as_deref(), &profile, &out, &format, lang.as_deref()),
        Commands::Transpile {
            source,
            out,
            profiles,
        } => commands::transpile(&source, out.as_deref(), &profiles, lang.as_deref()),
        Commands::Export { out, gts } => commands::export(&out, gts.as_deref(), lang.as_deref()),
        Commands::Convert {
            source,
            from,
            to,
            out,
            loss_report,
            base,
        } => commands::convert(
            &source,
            &from,
            &to,
            out.as_deref(),
            loss_report.as_deref(),
            base.as_deref(),
        ),
        Commands::ExtractDocs {
            directory,
            file,
            force,
        } => commands::extract_docs(&directory, file.as_deref(), force, lang.as_deref()),
        Commands::Crossref { out, gts } => commands::crossref(&out, gts.as_deref()),
        Commands::Mcp => commands::mcp(),
        Commands::Gts { args } => passthrough::gts(&args),
        Commands::Music { command } => passthrough::music(&command),
        Commands::Affect { command } => passthrough::affect(&command),
    }
}
