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
mod error;
mod gmn;
mod passthrough;

pub use gmn::{DecodeFormat, DigestFormat};

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use gmeow_cli_core::ConsoleMode;

/// The embedded canonical GMEOW snapshot: the whole ontology + transforms, folded
/// into one GTS bundle, baked into the binary so `gmeow` needs no repository, no
/// generator inputs, and no network. Every command that defaults to "the bundle"
/// reads these bytes unless the user supplies a file / `--gts`.
///
/// The bundle is a git-ignored staged product materialized by `make sync` (or
/// `make install`), never a committed input. `build.rs` resolves it to an
/// absolute path, guards against it being absent or empty, and exposes that
/// path via the `GMEOW_BUNDLE_PATH` build-time env var this `include_bytes!`
/// reads — so the build fails closed with a bootstrap pointer (naming
/// `make sync`) rather than a bare "file not found" when the bundle hasn't
/// been materialized yet. `GMEOW_BUNDLE_PATH` may be set in the environment to
/// override the staged path for release/package flows; the same hard fail on
/// absence still applies.
pub const BUNDLE_GTS: &[u8] = include_bytes!(env!("GMEOW_BUNDLE_PATH"));

/// The GMEOW IRI namespace the discipline checks and term catalog key on.
pub(crate) const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// The `describe` output serialization — the clap surface for
/// [`gmeow_docs::card::CardFormat`].
#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum DescribeFormat {
    /// Human-facing Markdown prose (the default).
    #[default]
    Prose,
    /// Pretty JSON of the term card (the `card.json` shape).
    Json,
    /// TOON (Token-Oriented Object Notation) — compact, token-efficient output for
    /// LLM/agent consumers.
    Toon,
}

impl From<DescribeFormat> for gmeow_docs::card::CardFormat {
    fn from(format: DescribeFormat) -> Self {
        match format {
            DescribeFormat::Prose => gmeow_docs::card::CardFormat::Prose,
            DescribeFormat::Json => gmeow_docs::card::CardFormat::Json,
            DescribeFormat::Toon => gmeow_docs::card::CardFormat::Toon,
        }
    }
}

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
    /// Verify GTS signatures, the reasoned deep-semantic pass, and the source-free
    /// ontology-completeness checks, rendered as one proof-carrying report.
    Verify {
        /// GTS snapshot to verify (default: bundled gmeow.gts).
        file: Option<PathBuf>,
        /// Out-of-band armored OpenPGP public key (overrides the embedded key).
        #[arg(long = "trusted-key")]
        trusted_key: Option<PathBuf>,
        /// Permit unsigned local snapshots.
        #[arg(long = "allow-unsigned")]
        allow_unsigned: bool,
        /// Output for the unified report: `human`, `sarif`, or `json`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
        /// Opt-in Tier-2 native semantic pass (reasoning) over the bundle,
        /// mirroring `validate --deep`. Plain `verify` never reasons.
        #[arg(long = "deep")]
        deep: bool,
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
        /// A GMEOW term across any grounding namespace: a registered CURIE
        /// (`gmeow:Entity`, `logic:Formula`, `math:Function`, `lang:Denotation`),
        /// the full IRI, a bare local name, or a unique case-insensitive prefix. A
        /// bare local name that names terms in more than one namespace is reported
        /// as ambiguous (never silently resolved) — qualify it with its CURIE.
        term: String,
        /// Describe from this `.gts` package instead of the bundle.
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
        /// Output serialization: `prose` (Markdown, default), `json`, or `toon`.
        #[arg(long = "format", short = 'f', value_enum, default_value_t = DescribeFormat::Prose)]
        format: DescribeFormat,
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
    /// Generate CrossRef DOI deposit XML from bundled self-description data.
    Crossref {
        /// Output XML path.
        #[arg(long = "out", short = 'o', default_value = "dist/crossref-deposit.xml")]
        out: PathBuf,
        /// GTS snapshot with self-description metadata.
        #[arg(long = "gts")]
        gts: Option<PathBuf>,
    },
    /// Explain a diagnostic witness by its stable fingerprint IRI (a finding) or
    /// anchor IRI (a cluster): print its provenance DAG plus the substrate algebra
    /// (gate verdict, minimal fatal cut, anchor cluster, and any Belnap gluts).
    Explain {
        /// A finding fingerprint IRI or an anchor IRI from `graph/diagnostics`.
        target_iri: String,
        /// GTS snapshot to read diagnostics from (default: bundled gmeow.gts).
        #[arg(long)]
        file: Option<PathBuf>,
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
    /// Conjecture-and-refutation tools over the `logic:` engine.
    Conjecture {
        #[command(subcommand)]
        command: ConjectureCommands,
    },
    /// Authoring-candidate propose/verify seam: submit a verdict-gated candidate to the
    /// append-only candidate library, withdraw one, or list what has been admitted.
    Candidate {
        #[command(subcommand)]
        command: CandidateCommands,
    },
    /// Decide whether a premise RDF graph ENTAILS a conclusion (`A ⊨ C`), natively,
    /// by refutation over the DL consistency calculus. Prints `entailed`,
    /// `not-entailed`, or an honest `gap:<shape>` when the conclusion is outside the
    /// soundly-refutable fragment. Syntax is inferred from each file's extension
    /// (`.ttl`, `.nt`, `.nq`, `.rdf`/`.owl`/`.xml`, `.trig`).
    Entails {
        /// The premise RDF graph `A`.
        premise: PathBuf,
        /// The conclusion RDF graph `C`.
        conclusion: PathBuf,
    },
    /// Native `logic:` reasoning-engine tools, driven directly over authored
    /// `logic:` cells (no repository or pipeline run required).
    Logic {
        #[command(subcommand)]
        command: LogicCommands,
    },
    /// GMEOW slice-quality tools: score an external slice directory against the embedded bundle.
    Slice {
        #[command(subcommand)]
        command: SliceCommands,
    },
    /// External documentation distribution tools (issue 1491 Task 5): the
    /// checkout-free consumer twin of the release-time docs distribution — dogfoods
    /// the Task-2 catalog and verifies the Task-3/4 content-addressed distribution.
    Docs {
        #[command(subcommand)]
        cmd: DocsCmd,
    },
    /// GMN-1 conformance surface: the shipped, checkout-free twin of the GMN-1
    /// codec gates. `digest`/`encode`/`decode` expose the digest + codec legs, and
    /// `verify` is the conformance driver an independent GMN-1 implementation runs
    /// against the frozen vector corpus. Every subcommand HARD-FAILS (non-zero exit)
    /// on any codec / digest / pack-root mismatch.
    Gmn {
        #[command(subcommand)]
        command: GmnCommands,
    },
    /// Register an external relation provider and run a hybrid-retrieval query.
    ///
    /// Loads ordinary asserted RDF facts (Turtle or N-Triples) into an isolated
    /// query world, parses a Datalog query program that references the
    /// registered provider relation, seals a query-scoped, deterministically
    /// budgeted [`gmeow_logic::external_relation::TableRelationProvider`] over a
    /// caller-supplied candidate table, and dispatches the annotated query
    /// end-to-end. Prints every resolved answer binding (with its composed
    /// annotation) plus the query receipt: every contributing provider's
    /// identity and artifact generation, and per-invocation
    /// request/response/status evidence — the observable proof that the
    /// query-scoped external-relation engine runs on the shipped `gmeow`
    /// binary, not only inside `crates/logic`'s own test binary.
    #[command(name = "hybrid-query")]
    HybridQuery {
        /// RDF facts (Turtle `.ttl`/`.turtle` or N-Triples `.nt`/`.ntriples`)
        /// re-homed into an isolated query world and joined against the
        /// provider relation.
        #[arg(long = "facts")]
        facts: PathBuf,
        /// A Datalog query program (`:- prefix(...)` directives, rules, and a
        /// single `?-` goal) that references the registered provider relation
        /// IRI (`--relation`) plus any ordinary RDF predicates from `--facts`.
        #[arg(long = "program")]
        program: PathBuf,
        /// The external provider's candidate tuples: DERIVED QUERY INPUTS,
        /// never asserted ontology facts, so this is deliberately a plain
        /// line-oriented table, not RDF. One tuple per line:
        /// `<arg1-iri> <arg2-iri> annotation order-key`, whitespace-separated
        /// (tabs or spaces, repeated whitespace collapses); blank lines and
        /// lines starting with `#` are ignored. `arg1-iri`/`arg2-iri` MUST be
        /// bracketed absolute IRIs (`<https://example.org/x>`); `annotation`
        /// is a signed 64-bit ZWeight integer; `order-key` is the provider's
        /// own lexical rank key for the pushed-down ascending total order
        /// (ties break on canonical tuple order).
        #[arg(long = "candidates")]
        candidates: PathBuf,
        /// The provider relation IRI referenced by `--program` (fixed arity 2,
        /// both arguments IRIs, `logic:SimilarityAnnotation` dimension).
        #[arg(long = "relation")]
        relation: String,
        /// Provider identity IRI (provenance only).
        #[arg(
            long = "provider-iri",
            default_value = "https://blackcatinformatics.ca/gmeow/hybrid-query/provider"
        )]
        provider_iri: String,
        /// Model/algorithm identity IRI (provenance only).
        #[arg(
            long = "model-iri",
            default_value = "https://blackcatinformatics.ca/gmeow/hybrid-query/model"
        )]
        model_iri: String,
        /// Immutable artifact-generation IRI the `--candidates` table is
        /// pinned to (provenance only).
        #[arg(
            long = "artifact-generation",
            default_value = "https://blackcatinformatics.ca/gmeow/hybrid-query/generation/1"
        )]
        artifact_generation: String,
        /// Ordered-prefix row limit pushed into the provider on each call.
        #[arg(long = "per-call-limit", default_value_t = 64)]
        per_call_limit: usize,
        /// Deterministic operation-wide provider call budget.
        #[arg(long = "max-calls", default_value_t = 64)]
        max_calls: u64,
        /// Deterministic operation-wide provider row budget.
        #[arg(long = "max-rows", default_value_t = 4096)]
        max_rows: u64,
    },
}

/// The `gmeow slice` nested subcommands.
#[derive(Debug, Subcommand)]
pub enum SliceCommands {
    /// Score an external slice directory against the embedded gmeow.gts bundle
    /// (no repo checkout required) and render its quality report.
    Quality {
        /// Path to the external slice directory to score.
        dir: PathBuf,
        /// Output serialization: `human` (default), `json`, or `sarif`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
    },
    /// Lint an external slice directory against the embedded gmeow.gts bundle (no
    /// repo checkout): PASS if the roll-up tier meets the slice's own declared
    /// gmeow:sliceQualityTier and any --min-tier bar; advisories are emitted as
    /// graded findings but never gate. Exit 0 = met, 1 = below bar, 2 = hard fail.
    Lint {
        /// Path to the external slice directory to lint.
        dir: PathBuf,
        /// Also fail if the roll-up tier is below this rung (tier label or IRI local name).
        #[arg(long = "min-tier")]
        min_tier: Option<String>,
        /// Output serialization: `human` (default), `json`, or `sarif`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
    },
    /// Assemble and render a `gmeow:AuthoringPacket` authoring brief for a slice
    /// directory, computed over the slice's OWN sources (module.ttl, mappings/,
    /// i18n/) with the SINGLE canonical, SHACL-conformance-gated exemplar tiering. The
    /// committed `generated/briefs/authoring-packets.nt` is the canonical repo projection
    /// of this brief for in-repo slices; this command is its live, checkout-free twin.
    Brief {
        /// Path to the slice directory to brief by LIVE re-assembly over the slice's own
        /// sources (needs a checkout with `generated/shapes/`). Mutually exclusive with
        /// `--from-bundle`; exactly one of the two must be given.
        dir: Option<PathBuf>,
        /// Serve the PRE-ASSEMBLED packet(s) for this slice (short-name `ai` or full slice
        /// IRI) straight from the embedded gmeow.gts bundle — checkout-free. Mutually
        /// exclusive with `dir`.
        #[arg(long = "from-bundle")]
        from_bundle: Option<String>,
        /// Restrict to the subdomain axis (defined-term local-name prefix; bundle default
        /// `whole`).
        #[arg(long)]
        axis: Option<String>,
        /// The zero-based batch index of the 25-term chunk to cover (out of range
        /// is a hard error). Omitted with no axis = the whole slice as one packet.
        #[arg(long)]
        batch: Option<u32>,
        /// Output serialization: `human` (default), `json`, or `turtle`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
    },
    /// Show the committed projection-vocabulary ratchet — the guarded registry and the
    /// per-(slice, vocabulary) ceilings — straight from the embedded gmeow.gts bundle
    /// (the commitments view; live measured residue needs a repo checkout).
    ProjectionCeilings {
        /// Output serialization: `human` (default) or `tsv`.
        #[arg(long = "format", short = 'f', default_value = "human")]
        format: String,
    },
}

/// The `gmeow docs` nested subcommands (issue 1491 Task 5).
#[derive(Debug, Subcommand)]
pub enum DocsCmd {
    /// Resolve the per-format consumer-need matrix by querying the meta-level
    /// distribution-catalog named graph shipped inside the embedded `gmeow.gts`
    /// bundle (AC2) — dogfooding the Task-2 ontology content, never a re-authored
    /// static table.
    Matrix,
    /// Verify a materialized documentation distribution's blake3 content digests
    /// against its DCAT manifest (`<dir>/manifest/docs-manifest.ttl`).
    Verify {
        /// The materialized `dist/gmeow-docs/`-shaped documentation distribution root.
        #[arg(long, default_value = "dist/gmeow-docs")]
        dir: PathBuf,
        /// Restrict verification to a single distribution slug (default: every
        /// distribution the manifest declares).
        #[arg(long)]
        format: Option<String>,
    },
}

/// The `gmeow gmn` nested subcommands (the GMN-1 conformance surface over
/// `gmeow_lang_bridge`'s digest / codec / witness / pack layer).
#[derive(Debug, Subcommand)]
pub enum GmnCommands {
    /// Print the codebook Merkle root (`codebook_digest`) AND the input's content
    /// digest (`content_digest`, over its RDFC-1.0 canonical N-Quads).
    Digest {
        /// The RDF (Turtle) input to digest.
        input: PathBuf,
        /// Override the embedded bundle's `gmeow:gmnCodebookCurrent` + `gmeow:gmnDictV3` with a
        /// lang `module.ttl` file (default: the embedded `gmeow.gts` snapshot).
        #[arg(long = "lang-module")]
        lang_module: Option<PathBuf>,
        /// Output serialization: `text` (two labeled lines, default) or `json`.
        #[arg(long = "format", short = 'f', value_enum, default_value_t = DigestFormat::Text)]
        format: DigestFormat,
    },
    /// Encode an RDF (Turtle) input to GMN-1 text on stdout (hard-fails with the
    /// typed `Gmn1Error` on any uncovered / out-of-domain construct).
    Encode {
        /// The RDF (Turtle) input to encode.
        input: PathBuf,
        /// Override the embedded bundle's codebook/dictionary with a lang `module.ttl` file
        /// (default: the embedded `gmeow.gts` snapshot).
        #[arg(long = "lang-module")]
        lang_module: Option<PathBuf>,
    },
    /// Decode GMN-1 text back to the reconstructed GMN-0 model on stdout.
    Decode {
        /// The GMN-1 (`.gmn`) document to decode.
        input: PathBuf,
        /// Override the embedded bundle's codebook/dictionary with a lang `module.ttl` file
        /// (default: the embedded `gmeow.gts` snapshot).
        #[arg(long = "lang-module")]
        lang_module: Option<PathBuf>,
        /// Output serialization: `nquads` (canonical, default) or `turtle`.
        #[arg(long = "format", short = 'f', value_enum, default_value_t = DecodeFormat::Nquads)]
        format: DecodeFormat,
    },
    /// The conformance driver: over a frozen vector corpus, prove every positive
    /// vector is byte-frozen + round-trips, every codec-tier negative raises its
    /// recorded class, and the recomputed codebook digest + pack root match the bundle's
    /// declarations. Exits non-zero on any failure.
    Verify {
        /// The frozen vector corpus dir. Defaults to the in-repo corpus
        /// (`slices/grounding/lang/tests/gmn1-vectors`) when present; REQUIRED outside a checkout
        /// (the corpus is a test artifact, not shipped in the bundle).
        #[arg(long = "vectors")]
        vectors: Option<PathBuf>,
        /// Override the embedded bundle's codebook/dictionary with a lang `module.ttl` file
        /// (default: the embedded `gmeow.gts` snapshot).
        #[arg(long = "lang-module")]
        lang_module: Option<PathBuf>,
        /// Override the embedded bundle's grammar leaf with an authored GMN grammar file to hash
        /// (default: the `gmeow:gmnGrammarDigest` in the embedded `gmeow.gts` snapshot).
        #[arg(long = "grammar")]
        grammar: Option<PathBuf>,
        /// Override the embedded bundle's pack-root declaration with a conformance-pack projection
        /// file (default: the `gmeow:gmnPackRoot` in the embedded `gmeow.gts` snapshot).
        #[arg(long = "pack")]
        pack: Option<PathBuf>,
    },
}

/// The `gmeow conjecture` nested subcommands (native `gmeow_pipeline` engine).
#[derive(Debug, Subcommand)]
pub enum ConjectureCommands {
    /// Test a candidate `logic:` formula against a KB in an isolated, standpoint-scoped
    /// scenario world, print the engine verdict, and — unless `--dry-run` — APPEND it to the
    /// append-only conjecture library (`GMEOW_CONJECTURE_PATH`, else `~/.gmeow/conjectures.gts`).
    Test {
        /// A Turtle `logic:` document naming exactly one candidate formula.
        #[arg(long = "formula")]
        formula: PathBuf,
        /// A Turtle KB the candidate is tested against.
        #[arg(long = "kb")]
        kb: PathBuf,
        /// The REQUIRED reified standpoint scope IRI (Principle 9).
        #[arg(long = "standpoint")]
        standpoint: String,
        /// Optionally, the `math:Conjecture` twin IRI. When given, the statement is bridged to
        /// the runtime `logic:Conjecture` node via `math:conjectureUnderTest` (on every
        /// verdict), and a refutation's counterexample is additionally re-exposed via
        /// `math:hasCounterexample`.
        #[arg(long = "math-conjecture")]
        math_conjecture: Option<String>,
        /// Compute and print the verdict but WRITE NOTHING to the library.
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Optional derived-closure-size ceiling on the isolated scenario evaluation: when the
        /// derived (non-EDB) closure exceeds this many steps the run is stamped BudgetExhausted
        /// → lifecycle open → discharge Unknown. Omitted = unbounded.
        #[arg(long = "max-steps")]
        max_steps: Option<u64>,
        /// Optional derived-closure-size ceiling in answer bindings (the binding-count twin of
        /// `--max-steps`). Omitted = unbounded.
        #[arg(long = "max-answers")]
        max_answers: Option<usize>,
    },
}

/// The `gmeow candidate` nested subcommands — the neurosymbolic propose/verify seam over the
/// native `gmeow_pipeline` engine (the same shared cores the MCP `submit_candidate` /
/// `withdraw_candidate` / `list_candidates` tools run: one implementation, not two).
#[derive(Debug, Subcommand)]
pub enum CandidateCommands {
    /// Test a candidate `logic:` formula against a KB and — ONLY if the isolated-world verdict
    /// CORROBORATES it (admissible) — APPEND it to the append-only candidate library
    /// (`GMEOW_CANDIDATE_PATH`, else `~/.gmeow/candidates.gts`). A refuted or open candidate is
    /// never admitted and writes nothing.
    Submit {
        /// A Turtle `logic:` document naming exactly one candidate formula.
        #[arg(long = "formula")]
        formula: PathBuf,
        /// A Turtle KB the candidate is tested against.
        #[arg(long = "kb")]
        kb: PathBuf,
        /// The REQUIRED reified standpoint scope IRI (Principle 9).
        #[arg(long = "standpoint")]
        standpoint: String,
        /// Optionally, the `math:Conjecture` twin IRI (as `gmeow conjecture test`).
        #[arg(long = "math-conjecture")]
        math_conjecture: Option<String>,
        /// Optional provenance: the slice IRI this candidate is proposed FOR.
        #[arg(long = "for-slice")]
        for_slice: Option<String>,
        /// Optional provenance: the `gmeow:AuthoringPacket` IRI this candidate answers.
        #[arg(long = "for-packet")]
        for_packet: Option<String>,
        /// Compute and print the verdict but WRITE NOTHING to the library.
        #[arg(long = "dry-run")]
        dry_run: bool,
        /// Optional derived-closure-size ceiling (steps). Omitted = unbounded.
        #[arg(long = "max-steps")]
        max_steps: Option<u64>,
        /// Optional derived-closure-size ceiling (answer bindings). Omitted = unbounded.
        #[arg(long = "max-answers")]
        max_answers: Option<usize>,
    },
    /// Withdraw a persisted candidate (P10 supersession, never deletion): append a compensating
    /// "withdrawn" segment. An unknown or already-withdrawn id is a hard error.
    Withdraw {
        /// The candidate node IRI to withdraw.
        #[arg(long = "candidate-id")]
        candidate_id: String,
        /// An optional author reason recorded with the withdrawal.
        #[arg(long = "reason")]
        reason: Option<String>,
        /// Witness the withdrawal but WRITE NOTHING.
        #[arg(long = "dry-run")]
        dry_run: bool,
    },
    /// List every admitted candidate with its effective disposition (in-library | withdrawn) and
    /// target provenance. A missing library is an empty list.
    List {
        /// Filter by target-slice provenance (a slice IRI).
        #[arg(long = "slice")]
        slice: Option<String>,
        /// Filter by effective disposition: `in-library` or `withdrawn`.
        #[arg(long = "disposition")]
        disposition: Option<String>,
    },
}

/// The `gmeow logic` nested subcommands (native `gmeow_logic` engine).
#[derive(Debug, Subcommand)]
pub enum LogicCommands {
    /// Evaluate one or more authored `logic:ReasoningProgram` cells through the
    /// native proof-carrying SLG-WFS backward (goal-directed) engine
    /// ([`gmeow_logic::goal_directed::evaluate_reasoning_programs`]) — the SAME
    /// production path `stage-goal-directed` folds into `gmeow.gts`'s
    /// `graph/goal-directed`, run interactively over a caller-supplied cell
    /// instead of through a full pipeline regeneration. Prints every
    /// proof-checked answer (atom, bindings, derivation IRI) and every
    /// three-valued well-founded verdict, in the engine's own deterministic
    /// order. Hard-fails (never a silent empty success) on a missing
    /// `--program-file`, a cell carrying zero `logic:ReasoningProgram`
    /// individuals, or a `--program-iri` naming no program in the cell.
    Backward {
        /// A Turtle document naming one or more `logic:ReasoningProgram`
        /// individuals, e.g. `slices/grounding/logic/examples/reasoning-programs.ttl`.
        #[arg(long = "program-file")]
        program_file: PathBuf,
        /// Evaluate only the `logic:ReasoningProgram` with this exact IRI
        /// (default: evaluate every program the cell carries). A bare CURIE
        /// (`ex:peanoAdd`) is NOT accepted — pass the program's full IRI.
        #[arg(long = "program-iri")]
        program_iri: Option<String>,
        /// An additional Turtle document whose told `rdfs:subClassOf` edges seed
        /// the order-sorted unification lattice — e.g.
        /// `slices/grounding/math/module.ttl`, whose `math:Integer ⊑
        /// math:RationalNumber ⊑ math:RealNumber ⊑ …` chain the engine composes
        /// into `math:Integer ⊑ math:RealNumber` internally (the engine computes
        /// its own reflexive-transitive closure over whatever covering edges it
        /// is given, so passing the told chain is sufficient — never a
        /// pre-reasoned closure). Default: use ONLY the `rdfs:subClassOf` edges
        /// told in `--program-file` itself. A program whose sort obligation
        /// needs an edge absent from every source resolves to ZERO order-sorted
        /// answers for that obligation — a correct, honest gap, never a silent
        /// hardcoded fallback to some vocabulary's subsort tower.
        #[arg(long = "subsort-source")]
        subsort_source: Option<PathBuf>,
    },
    /// Drive the stable operational `gmeow_logic::runtime::ReasoningSession` façade
    /// over authored `logic:` programs and RDF EDBs: open a content-addressed
    /// session, apply content-addressed deltas, READ BACK the incrementally
    /// maintained derived closure with its proof provenance, and round-trip a
    /// content-addressed / identity-gated checkpoint to disk. This is the real
    /// production consumer of the façade — it drives AND surfaces the reasoning
    /// output, so the maintained closure is never a test-only surface.
    Session {
        #[command(subcommand)]
        command: LogicSessionCommands,
    },
}

/// The `gmeow logic session` nested subcommands — the production consumer of the
/// operational [`gmeow_logic::runtime::ReasoningSession`] façade.
///
/// Every subcommand loads the authored `logic:` program (`--program`) and the
/// authorized RDF EDB (`--edb`, Turtle/N-Triples/N-Quads/TriG, re-homed into one
/// named-graph world) through the SAME production loaders the rest of the CLI uses,
/// then drives the real façade — never a re-implementation.
#[derive(Debug, Subcommand)]
pub enum LogicSessionCommands {
    /// Open a session over an authorized EDB + program and print the full
    /// seven-axis `SessionIdentity` (as N-Quads), the genesis journal head, and the
    /// fixed program's fragment disposition.
    Open {
        /// The authorized RDF EDB (Turtle/N-Triples/N-Quads/TriG).
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
        /// The single named-graph world IRI the EDB is re-homed into (the façade
        /// maintains exactly one world). Default: the session world IRI.
        #[arg(long = "world")]
        world: Option<String>,
        /// Compose over a demand-paged world-source: page the authorized EDB back in
        /// through a `PagedDataset` and drive `ReasoningSession::open_paged`, then print
        /// the page-fault composition metrics. Implied by `--page-size`.
        #[arg(long)]
        paged: bool,
        /// Chunk the paged world into pages of this many quads (implies `--paged`); a
        /// value `>=` the quad count (or omitted) pages the whole world as one page.
        #[arg(long = "page-size")]
        page_size: Option<usize>,
    },
    /// Open a session, build a content-addressed `SessionDelta` (additions +
    /// optional retirements, anchored on the session's own data-generation and
    /// current head), apply it, and print the typed `OperationOutcome` variant with
    /// its evidence plus the advanced journal head.
    Apply {
        /// The authorized RDF EDB.
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
        /// The facts to insert (RDF, re-homed into the session world).
        #[arg(long = "additions")]
        additions: PathBuf,
        /// Active state to retire/suppress (RDF, re-homed into the session world).
        #[arg(long = "retract")]
        retract: Option<PathBuf>,
        /// Optional committed-derivation step budget for the insertion.
        #[arg(long = "max-steps")]
        max_steps: Option<u64>,
    },
    /// Read the incrementally-maintained derived closure back out — the production
    /// READER that makes the maintained answer set observable. Optionally applies a
    /// delta first (`--apply`), then prints the deterministic, diffable closure with
    /// per-fact proof provenance (firing rule, premises, signed Z-weight).
    Facts {
        /// The authorized RDF EDB.
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
        /// Optionally apply this additions delta before reading the closure back.
        #[arg(long = "apply")]
        apply: Option<PathBuf>,
        /// Active state to retire/suppress before reading the closure back (RDF,
        /// re-homed into the session world exactly as `--apply` additions are, via the
        /// identical suppression path `checkpoint --retract` / `apply --retract` use).
        /// Folds a NON-EMPTY suppression into the applied delta, so the read-back
        /// closure and its per-fact proof heights reflect the retraction — e.g. a
        /// surviving fact's min-proof-height RISES when its shortest proof is retired.
        #[arg(long = "retract")]
        retract: Option<PathBuf>,
        /// Read the closure back over a demand-paged world-source
        /// (`ReasoningSession::open_paged`) instead of the resident open, and print the
        /// page-fault composition metrics. Implied by `--page-size`. The maintained
        /// closure read back is identical to the resident path.
        #[arg(long)]
        paged: bool,
        /// Chunk the paged world into pages of this many quads (implies `--paged`); a
        /// value `>=` the quad count (or omitted) pages the whole world as one page.
        #[arg(long = "page-size")]
        page_size: Option<usize>,
    },
    /// Open a session (optionally applying a delta first), mint a content-addressed
    /// checkpoint, and write it to disk as JSON (identity + EDB generation + journal
    /// head + content address).
    Checkpoint {
        /// The authorized RDF EDB.
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
        /// Optionally apply this additions delta before checkpointing.
        #[arg(long = "apply")]
        apply: Option<PathBuf>,
        /// Active state to retire/suppress in the applied delta (RDF, re-homed into
        /// the session world exactly as the additions are). Folds a NON-EMPTY
        /// suppression into the committed delta the checkpoint persists and replays.
        #[arg(long = "retract")]
        retract: Option<PathBuf>,
        /// The path to write the checkpoint JSON to.
        #[arg(long = "out", short = 'o')]
        out: PathBuf,
    },
    /// Load a checkpoint from disk and restore a session by deterministic
    /// re-materialization, printing the typed outcome — including the identity-gated
    /// `Invalid{IdentityMismatch}` rejection when the checkpoint does not match the
    /// current identity, and `Invalid{CorruptCheckpoint}` when the bytes were
    /// tampered with.
    Restore {
        /// The checkpoint JSON to load.
        #[arg(long = "in")]
        input: PathBuf,
        /// The authorized RDF EDB to re-materialize from.
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
    },
    /// Restart from a checkpoint and resume at its durable journal head. If
    /// `--reapply` re-submits an already-committed delta (anchored on the stale
    /// pre-checkpoint head), print the `Invalid{PreconditionMismatch}` refusal — the
    /// structural no-double-apply guard surviving a persist→restore boundary.
    Restart {
        /// The checkpoint JSON to load.
        #[arg(long = "in")]
        input: PathBuf,
        /// The authorized RDF EDB to re-materialize from.
        #[arg(long = "edb")]
        edb: PathBuf,
        /// The authored `logic:`-vocabulary program Turtle.
        #[arg(long = "program")]
        program: PathBuf,
        /// Re-submit this already-committed additions delta (anchored on the
        /// genesis head) to demonstrate the double-apply refusal after a restart.
        #[arg(long = "reapply")]
        reapply: Option<PathBuf>,
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
    /// Ingest a captured classifier output (JSON) into attributed GMEOW evidence
    /// Turtle: a gmeow:ModelInferenceRun + one gmeow:AffectClassifierOutput per
    /// label (+ supported gmeow:AffectiveClaim / gmeow:AffectEvaluationConcluded).
    /// Evidence, never inner-affect fact. Serves every registered adapter
    /// (GoEmotions / SST-2 / CardiffNLP / j-hartmann / zero-shot), dispatched by
    /// the capture's declared label set — no per-model subcommand.
    Ingest {
        /// Captured run JSON — a `ClassifierRunCapture` envelope.
        source: PathBuf,
        /// Output Turtle file (default: stdout).
        #[arg(long = "out", short = 'o')]
        out: Option<PathBuf>,
    },
    /// Recover the captured run from attributed GMEOW evidence Turtle (the inverse
    /// of `ingest`) — the blind get leg of the losslessness round-trip. The label
    /// set is auto-detected from the evidence graph's emitted labels. Prints the
    /// reconstructed capture as JSON.
    Recover {
        /// GMEOW evidence Turtle, as produced by `ingest`.
        source: PathBuf,
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
    // The consumer razor: stdout is the product stream, so `auto` resolves to a
    // human stderr surface (Pretty on a TTY, Text off it) and diagnostics never
    // interleave NDJSON into piped output. `--console jsonl` opts into the agent
    // surface deliberately.
    ConsoleMode::resolve_stderr_default(flag, env_val.as_deref(), std::io::stderr().is_terminal())
}

/// Parse the arguments, dispatch to the wired backend, and return the process
/// exit code. clap emits its own usage errors (exit `2`) before this returns.
pub fn run() -> i32 {
    let cli = Cli::parse();
    let console = resolve_console(cli.console);
    // One boxed reporter for the whole run, chosen from the resolved console mode:
    // human stderr text, NDJSON for agents, or a silent sink. Every command emits
    // its diagnostics through this shared reporter channel, never a bare stderr line.
    let reporter = gmeow_cli_core::reporter_for(console);
    let reporter = reporter.as_ref();
    let lang = cli.lang;
    match cli.command {
        Commands::Version => commands::version(),
        Commands::Info { file } => commands::info(reporter, file.as_deref()),
        Commands::Verify {
            file,
            trusted_key,
            allow_unsigned,
            format,
            deep,
        } => commands::verify(
            reporter,
            file.as_deref(),
            trusted_key.as_deref(),
            allow_unsigned,
            &format,
            deep,
        ),
        Commands::VerifyReleaseBundle { bundle, public_key } => {
            commands::verify_release_bundle(reporter, &bundle, public_key.as_deref())
        }
        Commands::Describe { term, gts, format } => commands::describe(
            reporter,
            &term,
            gts.as_deref(),
            lang.as_deref(),
            format.into(),
        ),
        Commands::Validate {
            instance,
            schema,
            format,
            deep,
        } => commands::validate(reporter, &instance, schema.as_deref(), &format, deep),
        Commands::Build { out, gts } => commands::build(reporter, &out, gts.as_deref()),
        Commands::Project {
            source,
            profile,
            out,
            format,
        } => commands::project(
            reporter,
            source.as_deref(),
            &profile,
            &out,
            &format,
            lang.as_deref(),
        ),
        Commands::Transpile {
            source,
            out,
            profiles,
        } => commands::transpile(
            reporter,
            &source,
            out.as_deref(),
            &profiles,
            lang.as_deref(),
        ),
        Commands::Export { out, gts } => {
            commands::export(reporter, &out, gts.as_deref(), lang.as_deref())
        }
        Commands::Convert {
            source,
            from,
            to,
            out,
            loss_report,
            base,
        } => commands::convert(
            reporter,
            &source,
            &from,
            &to,
            out.as_deref(),
            loss_report.as_deref(),
            base.as_deref(),
        ),
        Commands::Crossref { out, gts } => commands::crossref(reporter, &out, gts.as_deref()),
        Commands::Explain { target_iri, file } => commands::explain(reporter, target_iri, file),
        Commands::Mcp => commands::mcp(reporter),
        Commands::Gts { args } => passthrough::gts(reporter, &args),
        Commands::Music { command } => passthrough::music(reporter, &command),
        Commands::Affect { command } => passthrough::affect(reporter, &command),
        Commands::Conjecture { command } => match command {
            ConjectureCommands::Test {
                formula,
                kb,
                standpoint,
                math_conjecture,
                dry_run,
                max_steps,
                max_answers,
            } => commands::conjecture_test(
                reporter,
                &formula,
                &kb,
                &standpoint,
                math_conjecture.as_deref(),
                dry_run,
                max_steps,
                max_answers,
            ),
        },
        Commands::Candidate { command } => match command {
            CandidateCommands::Submit {
                formula,
                kb,
                standpoint,
                math_conjecture,
                for_slice,
                for_packet,
                dry_run,
                max_steps,
                max_answers,
            } => commands::candidate_submit(
                reporter,
                &formula,
                &kb,
                &standpoint,
                math_conjecture.as_deref(),
                for_slice.as_deref(),
                for_packet.as_deref(),
                dry_run,
                max_steps,
                max_answers,
            ),
            CandidateCommands::Withdraw {
                candidate_id,
                reason,
                dry_run,
            } => commands::candidate_withdraw(reporter, &candidate_id, reason.as_deref(), dry_run),
            CandidateCommands::List { slice, disposition } => {
                commands::candidate_list(reporter, slice.as_deref(), disposition.as_deref())
            }
        },
        Commands::Entails {
            premise,
            conclusion,
        } => commands::entails(reporter, &premise, &conclusion),
        Commands::Logic { command } => match command {
            LogicCommands::Backward {
                program_file,
                program_iri,
                subsort_source,
            } => commands::logic_backward(
                reporter,
                &program_file,
                program_iri.as_deref(),
                subsort_source.as_deref(),
            ),
            LogicCommands::Session { command } => match command {
                LogicSessionCommands::Open {
                    edb,
                    program,
                    world,
                    paged,
                    page_size,
                } => commands::logic_session_open(
                    reporter,
                    &edb,
                    &program,
                    world.as_deref(),
                    paged,
                    page_size,
                ),
                LogicSessionCommands::Apply {
                    edb,
                    program,
                    additions,
                    retract,
                    max_steps,
                } => commands::logic_session_apply(
                    reporter,
                    &edb,
                    &program,
                    &additions,
                    retract.as_deref(),
                    max_steps,
                ),
                LogicSessionCommands::Facts {
                    edb,
                    program,
                    apply,
                    retract,
                    paged,
                    page_size,
                } => commands::logic_session_facts(
                    reporter,
                    &edb,
                    &program,
                    apply.as_deref(),
                    retract.as_deref(),
                    paged,
                    page_size,
                ),
                LogicSessionCommands::Checkpoint {
                    edb,
                    program,
                    apply,
                    retract,
                    out,
                } => commands::logic_session_checkpoint(
                    reporter,
                    &edb,
                    &program,
                    apply.as_deref(),
                    retract.as_deref(),
                    &out,
                ),
                LogicSessionCommands::Restore {
                    input,
                    edb,
                    program,
                } => commands::logic_session_restore(reporter, &input, &edb, &program),
                LogicSessionCommands::Restart {
                    input,
                    edb,
                    program,
                    reapply,
                } => commands::logic_session_restart(
                    reporter,
                    &input,
                    &edb,
                    &program,
                    reapply.as_deref(),
                ),
            },
        },
        Commands::Slice { command } => match command {
            SliceCommands::Quality { dir, format } => {
                commands::slice_quality(reporter, &dir, &format)
            }
            SliceCommands::Lint {
                dir,
                min_tier,
                format,
            } => commands::slice_lint(reporter, &dir, min_tier.as_deref(), &format),
            SliceCommands::Brief {
                dir,
                from_bundle,
                axis,
                batch,
                format,
            } => commands::slice_brief(
                reporter,
                dir.as_deref(),
                from_bundle.as_deref(),
                axis.as_deref(),
                batch,
                &format,
            ),
            SliceCommands::ProjectionCeilings { format } => {
                commands::slice_projection_ceilings(reporter, &format)
            }
        },
        Commands::Docs { cmd } => match cmd {
            DocsCmd::Matrix => commands::docs_matrix(reporter),
            DocsCmd::Verify { dir, format } => {
                commands::docs_verify(reporter, &dir, format.as_deref())
            }
        },
        Commands::Gmn { command } => match command {
            GmnCommands::Digest {
                input,
                lang_module,
                format,
            } => gmn::digest(reporter, &input, lang_module.as_deref(), format),
            GmnCommands::Encode { input, lang_module } => {
                gmn::encode(reporter, &input, lang_module.as_deref())
            }
            GmnCommands::Decode {
                input,
                lang_module,
                format,
            } => gmn::decode(reporter, &input, lang_module.as_deref(), format),
            GmnCommands::Verify {
                vectors,
                lang_module,
                grammar,
                pack,
            } => gmn::verify(
                reporter,
                vectors.as_deref(),
                lang_module.as_deref(),
                grammar.as_deref(),
                pack.as_deref(),
            ),
        },
        Commands::HybridQuery {
            facts,
            program,
            candidates,
            relation,
            provider_iri,
            model_iri,
            artifact_generation,
            per_call_limit,
            max_calls,
            max_rows,
        } => commands::hybrid_query(
            reporter,
            &facts,
            &program,
            &candidates,
            &relation,
            &provider_iri,
            &model_iri,
            &artifact_generation,
            per_call_limit,
            max_calls,
            max_rows,
        ),
    }
}
