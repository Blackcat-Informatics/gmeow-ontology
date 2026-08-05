// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **GMN token-cost matrix**: an off-gate, INFORMS-only sweep of the five mandated
//! tokenizer families over the SAME emitted GMN / Turtle / JSON-LD serializations of the
//! grounding corpus the on-gate Task-7 byte-fallback estimator GATES.
//!
//! ## Why this exists (and why it is off-gate)
//!
//! The on-gate estimator ([`gmeow_lang_bridge::compute_token_metrics`]) is a deterministic,
//! model-agnostic byte-fallback BOUND — it is the teeth behind the flagship "GMN costs fewer
//! tokens than Turtle" claim and it must ship in every build. This matrix is the complementary
//! EMPIRICAL sweep: it runs REAL, production BPE / SentencePiece vocabularies (the ones the GMN
//! surface will actually be read by) over the emitted artifacts and flags, per vocabulary, the
//! GMN glyphs that BYTE-FRAGMENT (tokenize to more than one token). That signal INFORMS the S2–S4
//! glyph/tokenizer co-design revisions; it never gates, because a live tokenizer's merges are an
//! external artifact the build cannot pin as a normative invariant.
//!
//! ## The five families and how each vocabulary is obtained
//!
//! * **o200k_base**, **cl100k_base** — the two OpenAI vocabularies COMPILED INTO the binary by
//!   `tiktoken-rs` (`include_str!`, no I/O, deterministic). Reused, not rebuilt.
//! * **Qwen** (`Qwen/Qwen2.5-0.5B`, Apache-2.0, ungated) — VENDORED under
//!   `assets/vocab/qwen/tokenizer.json`, blake3-pinned to [`QWEN_TOKENIZER_BLAKE3`].
//! * **Llama** (Meta Llama 3 Community License) and **Gemma** (Gemma Terms of Use) —
//!   FETCHED at maint-time into a git-ignored `.tmp/` and blake3-verified against
//!   [`LLAMA_TOKENIZER_BLAKE3`] / [`GEMMA_TOKENIZER_BLAKE3`]; NEVER committed. Their licenses are
//!   restricted, non-free, and AGPL-incompatible, so committing the assets into this
//!   AGPL-3.0-only tree would be a real license conflict — mirroring the repo's own Lane-B
//!   "never vendored" corpora (`maint-tptp-corpus`, `maint-ontouml-corpus`). See
//!   `assets/vocab/PROVENANCE.md`.
//!
//! No-optionality: once a family is selected it is mandatory. A missing/undecodable vocabulary or
//! a digest mismatch is a HARD FAIL ([`MatrixError`]) — never a silent drop to a smaller family
//! set and never a byte approximation.

use std::fmt;
use std::path::{Path, PathBuf};

use gmeow_lang_bridge::{Gmn0Model, GmnDictionary, gmn0_canonically_equal, gmn1_read, gmn1_write};
use tiktoken_rs::CoreBPE;
use tokenizers::Tokenizer as HfTokenizer;

/// The blake3 content pin of the vendored `Qwen/Qwen2.5-0.5B` `tokenizer.json`
/// (7,031,645 bytes, Apache-2.0). The vendored asset is verified against this on load, so a
/// corrupted or swapped file is a HARD FAIL rather than a silently different tokenization.
pub const QWEN_TOKENIZER_BLAKE3: &str =
    "b91332ca0c7a5f8e173effc53337026f64f17d4f25ca09205d4c1d5ecae4d621";

/// The blake3 content pin of the Meta Llama 3 `tokenizer.json` (9,085,698 bytes). Fetched at
/// maint-time (default source: the ungated `NousResearch/Meta-Llama-3-8B` re-host of Meta's
/// exact Llama-3 tokenizer); the byte content is license-restricted and NEVER committed.
pub const LLAMA_TOKENIZER_BLAKE3: &str =
    "174e70b51765e4514178cbae91eb5e54975cfdf3946427a75b8cba4954de898e";

/// The blake3 content pin of the Gemma 2 `tokenizer.json` (17,525,357 bytes). Fetched at
/// maint-time (default source: the ungated `unsloth/gemma-2-2b` re-host of Google's exact
/// Gemma-2 tokenizer); the byte content is license-restricted and NEVER committed.
pub const GEMMA_TOKENIZER_BLAKE3: &str =
    "7e8d9bfc505e187f92921e574a36991eef018ded6744adc658d2d343d6de1010";

/// A hard failure in the cost-matrix pipeline. Every variant is a HARD FAIL: no path degrades to
/// a smaller family set, a stale digest, or a byte approximation.
#[derive(Debug)]
pub enum MatrixError {
    /// A filesystem read/write failed (a grounding source, a vendored asset, the report path).
    Io(String),
    /// A vendored/fetched tokenizer asset's blake3 did not match its committed pin.
    DigestMismatch {
        /// The tokenizer family whose asset drifted.
        family: String,
        /// The committed pin the asset must match.
        expected: String,
        /// The digest actually computed over the bytes on disk.
        actual: String,
    },
    /// A tokenizer vocabulary failed to load or a text failed to encode.
    Tokenizer(String),
    /// The grounding corpus could not be assembled (dictionary load, parse, serialization).
    Corpus(String),
    /// A command-line argument to the maint binary was missing or unrecognized.
    Cli(String),
}

impl fmt::Display for MatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MatrixError::Io(m) => write!(f, "I/O failure: {m}"),
            MatrixError::DigestMismatch {
                family,
                expected,
                actual,
            } => write!(
                f,
                "{family} tokenizer asset blake3 mismatch: expected {expected}, got {actual} — \
                 the pinned vocabulary drifted; refusing to tokenize against an unpinned asset"
            ),
            MatrixError::Tokenizer(m) => write!(f, "tokenizer failure: {m}"),
            MatrixError::Corpus(m) => write!(f, "grounding corpus failure: {m}"),
            MatrixError::Cli(m) => write!(f, "argument error: {m}"),
        }
    }
}

impl std::error::Error for MatrixError {}

/// A convenience alias for the crate's fallible operations.
pub type Result<T> = std::result::Result<T, MatrixError>;

/// How a tokenizer vocabulary reached this run — the provenance class rendered in the report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceClass {
    /// Compiled into the binary (the two tiktoken-rs OpenAI vocabularies).
    Embedded,
    /// Committed in-repo and blake3-verified on load (Qwen, Apache-2.0).
    Vendored,
    /// Fetched at maint-time into a git-ignored `.tmp/`, blake3-verified, never committed
    /// (Llama, Gemma — restricted licenses).
    Fetched,
}

impl SourceClass {
    /// The human label used in the matrix report.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SourceClass::Embedded => "embedded (tiktoken-rs)",
            SourceClass::Vendored => "vendored (in-repo, blake3-pinned)",
            SourceClass::Fetched => "fetched at maint-time (never vendored)",
        }
    }
}

/// A tokenizer backend: either an embedded tiktoken BPE or a loaded HuggingFace tokenizer.
enum Backend {
    Tiktoken(Box<CoreBPE>),
    Hf(Box<HfTokenizer>),
}

/// A loaded tokenizer family plus its provenance class. [`Vocab::count`] returns the number of
/// tokens a string encodes to under this vocabulary.
pub struct Vocab {
    /// The family name (`o200k_base`, `cl100k_base`, `Qwen`, `Llama`, `Gemma`).
    pub family: String,
    /// How this vocabulary was obtained (for the report's provenance column).
    pub source_class: SourceClass,
    backend: Backend,
}

impl Vocab {
    /// The number of tokens `text` encodes to under this vocabulary. Ordinary encoding is used
    /// (special-token markers are treated as literal text) and, for the HF families, no special
    /// tokens are added — the count is the pure content cost. A byte-level / byte-fallback BPE
    /// always covers every byte, so the count is always FINITE; an encode failure is a HARD FAIL.
    pub fn count(&self, text: &str) -> Result<usize> {
        match &self.backend {
            Backend::Tiktoken(bpe) => Ok(bpe.encode_ordinary(text).len()),
            Backend::Hf(tok) => tok
                .encode(text, false)
                .map(|enc| enc.len())
                .map_err(|e| MatrixError::Tokenizer(format!("{} encode failed: {e}", self.family))),
        }
    }
}

/// Load the `o200k_base` (GPT-4o) vocabulary embedded in `tiktoken-rs`.
pub fn load_o200k() -> Result<Vocab> {
    let bpe = tiktoken_rs::o200k_base()
        .map_err(|e| MatrixError::Tokenizer(format!("embedded o200k_base failed to load: {e}")))?;
    Ok(Vocab {
        family: "o200k_base".to_owned(),
        source_class: SourceClass::Embedded,
        backend: Backend::Tiktoken(Box::new(bpe)),
    })
}

/// Load the `cl100k_base` (GPT-4 / GPT-3.5) vocabulary embedded in `tiktoken-rs`.
pub fn load_cl100k() -> Result<Vocab> {
    let bpe = tiktoken_rs::cl100k_base()
        .map_err(|e| MatrixError::Tokenizer(format!("embedded cl100k_base failed to load: {e}")))?;
    Ok(Vocab {
        family: "cl100k_base".to_owned(),
        source_class: SourceClass::Embedded,
        backend: Backend::Tiktoken(Box::new(bpe)),
    })
}

/// The path to the vendored Qwen `tokenizer.json` under this crate's `assets/`.
#[must_use]
pub fn qwen_asset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/vocab/qwen/tokenizer.json")
}

/// Verify `bytes` hash to `expected` (lowercase hex blake3); return the actual digest on mismatch.
fn verify_blake3(family: &str, bytes: &[u8], expected: &str) -> Result<()> {
    let actual = blake3::hash(bytes).to_hex().to_string();
    if actual == expected {
        Ok(())
    } else {
        Err(MatrixError::DigestMismatch {
            family: family.to_owned(),
            expected: expected.to_owned(),
            actual,
        })
    }
}

/// Load a HuggingFace `tokenizer.json` from `path`, blake3-verified against `expected_pin`. Shared
/// by the vendored (Qwen) and fetched (Llama, Gemma) families so every non-tiktoken vocabulary is
/// content-pinned on the SAME path — a drifted asset is a HARD FAIL, never silently tokenized.
pub fn load_hf(
    family: &str,
    source_class: SourceClass,
    path: &Path,
    expected_pin: &str,
) -> Result<Vocab> {
    let bytes = std::fs::read(path).map_err(|e| {
        MatrixError::Io(format!(
            "read {family} tokenizer.json at {}: {e}",
            path.display()
        ))
    })?;
    verify_blake3(family, &bytes, expected_pin)?;
    let tok = HfTokenizer::from_bytes(&bytes).map_err(|e| {
        MatrixError::Tokenizer(format!("{family} tokenizer.json failed to load: {e}"))
    })?;
    Ok(Vocab {
        family: family.to_owned(),
        source_class,
        backend: Backend::Hf(Box::new(tok)),
    })
}

/// Load the vendored, blake3-pinned Qwen vocabulary from this crate's `assets/`.
pub fn load_qwen() -> Result<Vocab> {
    load_hf(
        "Qwen",
        SourceClass::Vendored,
        &qwen_asset_path(),
        QWEN_TOKENIZER_BLAKE3,
    )
}

/// Load the Llama vocabulary from a maint-time-fetched `tokenizer.json` at `path`, verified
/// against [`LLAMA_TOKENIZER_BLAKE3`]. The bytes are never committed.
pub fn load_llama(path: &Path) -> Result<Vocab> {
    load_hf("Llama", SourceClass::Fetched, path, LLAMA_TOKENIZER_BLAKE3)
}

/// Load the Gemma vocabulary from a maint-time-fetched `tokenizer.json` at `path`, verified
/// against [`GEMMA_TOKENIZER_BLAKE3`]. The bytes are never committed.
pub fn load_gemma(path: &Path) -> Result<Vocab> {
    load_hf("Gemma", SourceClass::Fetched, path, GEMMA_TOKENIZER_BLAKE3)
}

/// The three OFFLINE-available families in canonical order: the two embedded tiktoken vocabularies
/// plus the vendored, blake3-pinned Qwen. This is the family set the `#[ignore]` determinism test
/// runs fully offline (no network); the full five-family sweep adds the fetched Llama + Gemma.
pub fn offline_vocabs() -> Result<Vec<Vocab>> {
    Ok(vec![load_o200k()?, load_cl100k()?, load_qwen()?])
}

/// The repository root, resolved from this crate's manifest directory (`crates/gmn-cost-matrix`).
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The grounding slice directories the matrix runs over — the SAME `slices/grounding/{logic,lang,
/// math}` corpus (`module.ttl` + `examples/*.ttl`) the on-gate GMN-1 round-trip gate is total over.
const GROUNDING_SLICES: [&str; 3] = ["logic", "lang", "math"];

/// One grounding source's three emitted serializations, over the SAME frozen GMN-0 dataset.
pub struct Artifact {
    /// The repo-relative source path (e.g. `slices/grounding/lang/module.ttl`).
    pub name: String,
    /// The emitted GMN-1 text (the `.gmn` artifact the projection ships for this source).
    pub gmn: String,
    /// The Turtle serialization of the same normalized dataset.
    pub turtle: String,
    /// The JSON-LD serialization of the same normalized dataset.
    pub jsonld: String,
}

/// Collect every grounding source path (`slices/grounding/<slice>/module.ttl` plus each
/// `examples/*.ttl`), repo-relative and sorted — the SAME domain as the on-gate round-trip gate.
fn collect_grounding_sources(root: &Path) -> Result<Vec<String>> {
    let mut sources: Vec<String> = Vec::new();
    for slice in GROUNDING_SLICES {
        let slice_dir = root.join("slices/grounding").join(slice);
        let module_path = slice_dir.join("module.ttl");
        if module_path.is_file() {
            sources.push(rel(root, &module_path));
        }
        let examples_dir = slice_dir.join("examples");
        if examples_dir.is_dir() {
            let entries = std::fs::read_dir(&examples_dir).map_err(|e| {
                MatrixError::Corpus(format!("read dir {}: {e}", examples_dir.display()))
            })?;
            for entry in entries {
                let entry = entry.map_err(|e| {
                    MatrixError::Corpus(format!("dir entry in {slice}/examples: {e}"))
                })?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
                    sources.push(rel(root, &path));
                }
            }
        }
    }
    sources.sort();
    Ok(sources)
}

/// Render `path` relative to `root` (falling back to the full path if it is not under `root`).
fn rel(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Load the pinned GMN dictionary from the lang slice's authored `module.ttl`.
fn load_dictionary(root: &Path) -> Result<GmnDictionary> {
    let path = root.join("slices/grounding/lang/module.ttl");
    let bytes = std::fs::read(&path)
        .map_err(|e| MatrixError::Corpus(format!("read {}: {e}", path.display())))?;
    let ds = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| MatrixError::Corpus(format!("parse {}: {e}", path.display())))?;
    GmnDictionary::from_dataset(&ds)
        .map_err(|e| MatrixError::Corpus(format!("gmeow:gmnDictV3 failed to load: {}", e.0)))
}

/// Serialize `model`'s GMN-0 normal-form dataset to Turtle and JSON-LD, returning both strings.
/// Both are over the SAME frozen dataset the GMN writer lowered, so the three surfaces compare
/// like for like — the identical construction [`gmeow_lang_bridge::compute_token_metrics`] uses.
fn serialize_turtle_jsonld(model: &Gmn0Model) -> Result<(String, String)> {
    let dataset = model.to_dataset();
    let turtle =
        purrdf::serialize_dataset_to_format(&*dataset, purrdf::NativeRdfFormat::Turtle, None)
            .map_err(|e| MatrixError::Corpus(format!("Turtle serialization failed: {e}")))?;
    let turtle = String::from_utf8(turtle.bytes)
        .map_err(|e| MatrixError::Corpus(format!("Turtle bytes are not UTF-8: {e}")))?;
    let jsonld = purrdf::serialize_dataset_to_jsonld(&*dataset)
        .map_err(|e| MatrixError::Corpus(format!("JSON-LD serialization failed: {e}")))?;
    Ok((turtle, jsonld))
}

/// Assemble the corpus artifacts: for every grounding source whose GMN emission round-trips
/// exactly (the sources that actually ship a `.gmn`), capture its emitted GMN text plus the
/// Turtle and JSON-LD serializations of the same normalized dataset. Returns the pinned
/// dictionary alongside so the caller can enumerate the glyph table for the per-symbol matrix.
///
/// A source that fails to parse as Turtle, or whose GMN emission does not round-trip, is out of
/// the measured domain and skipped — mirroring [`gmeow_lang_bridge::compute_token_metrics`]'s
/// honest scoping to the artifacts GMN actually ships. An EMPTY corpus is a HARD FAIL: there is
/// nothing to measure and a vacuous matrix would be misleading.
pub fn build_corpus(root: &Path) -> Result<(GmnDictionary, Vec<Artifact>)> {
    let dict = load_dictionary(root)?;
    let sources = collect_grounding_sources(root)?;
    let mut artifacts = Vec::new();
    for source in &sources {
        let bytes = std::fs::read(root.join(source))
            .map_err(|e| MatrixError::Corpus(format!("read {source}: {e}")))?;
        let Ok(dataset) = purrdf::parse_dataset(&bytes, "text/turtle", None) else {
            continue;
        };
        let model = Gmn0Model::from_dataset(&dataset);
        let Ok(doc) = gmn1_write(&model, &dict) else {
            continue;
        };
        let Ok(back) = gmn1_read(&doc, &dict) else {
            continue;
        };
        if !gmn0_canonically_equal(&model, &back) {
            continue;
        }
        let (turtle, jsonld) = serialize_turtle_jsonld(&model)?;
        artifacts.push(Artifact {
            name: source.clone(),
            gmn: doc.text,
            turtle,
            jsonld,
        });
    }
    if artifacts.is_empty() {
        return Err(MatrixError::Corpus(
            "no grounding source produced a round-tripping GMN artifact — nothing to measure"
                .to_owned(),
        ));
    }
    Ok((dict, artifacts))
}

/// The `U+XXXX`-style codepoint spelling of a glyph string (space-separated for multi-codepoint
/// glyphs) — the stable, render-independent identity used in the per-symbol table.
fn codepoints(glyph: &str) -> String {
    glyph
        .chars()
        .map(|c| format!("U+{:04X}", c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ordered, deduped glyph table for the per-symbol matrix: the pinned registry glyphs, sorted
/// by codepoint sequence so the report is byte-stable regardless of registry insertion order.
fn ordered_glyphs(dict: &GmnDictionary) -> Vec<String> {
    let mut glyphs: Vec<String> = dict
        .glyph_registry()
        .glyph_tokens()
        .into_iter()
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
        .collect();
    glyphs.sort_by(|a, b| a.chars().cmp(b.chars()));
    glyphs.dedup();
    glyphs
}

/// Escape a glyph for a markdown table cell (only the pipe is table-significant here).
fn md_cell(glyph: &str) -> String {
    glyph.replace('|', "\\|")
}

/// A selector picking one serialization surface (GMN / Turtle / JSON-LD) off an [`Artifact`].
type SurfaceSelector = fn(&Artifact) -> &str;

/// The aggregate token cost of `select`ing one serialization field across all artifacts, under
/// `vocab` — the per-format column value.
fn format_total(
    artifacts: &[Artifact],
    vocab: &Vocab,
    select: impl Fn(&Artifact) -> &str,
) -> Result<u64> {
    let mut total = 0u64;
    for a in artifacts {
        total += vocab.count(select(a))? as u64;
    }
    Ok(total)
}

/// Render the deterministic token-cost matrix report over `vocabs` (whatever family set the
/// caller loaded — three offline for the determinism test, five for the maint lane). The output
/// is a pure function of the corpus + dictionary + vocabulary set, so two runs are byte-identical.
///
/// The report carries three matrices: (1) per-format aggregate token totals (GMN vs Turtle vs
/// JSON-LD) with the GMN÷Turtle ratio; (2) the per-symbol glyph token cost with byte-fragmenting
/// glyphs flagged `*`; (3) the byte-fragmenting glyph roster per family (the S2–S4 co-design feed).
pub fn render_matrix(
    dict: &GmnDictionary,
    artifacts: &[Artifact],
    vocabs: &[Vocab],
) -> Result<String> {
    let mut out = String::new();
    out.push_str("# GMN token-cost matrix\n\n");
    out.push_str(
        "Off-gate INFORMS-only sweep of the mandated tokenizer families over the emitted GMN / \
         Turtle / JSON-LD serializations of the `slices/grounding/{logic,lang,math}` corpus \
         (`module.ttl` + `examples/*.ttl`), scoped to the sources whose GMN emission round-trips \
         exactly. The on-gate teeth are `gmeow_lang_bridge::compute_token_metrics` \
         (deterministic byte-fallback bound); this matrix flags, per real vocabulary, the GMN \
         glyphs that byte-fragment, feeding the S2–S4 glyph/tokenizer co-design.\n\n",
    );

    // ── Family roster (provenance) ─────────────────────────────────────────────────
    out.push_str("## Tokenizer families\n\n");
    out.push_str("| Family | Provenance |\n|---|---|\n");
    for v in vocabs {
        out.push_str(&format!("| {} | {} |\n", v.family, v.source_class.label()));
    }
    out.push('\n');

    // ── Corpus scope ───────────────────────────────────────────────────────────────
    out.push_str(&format!(
        "Measured over {} round-tripping grounding source(s).\n\n",
        artifacts.len()
    ));

    // ── Matrix 1: per-format aggregate token cost ──────────────────────────────────
    out.push_str("## Per-format aggregate token cost\n\n");
    out.push_str("Total tokens over the round-tripping corpus, per serialization surface.\n\n");
    out.push_str("| Format |");
    for v in vocabs {
        out.push_str(&format!(" {} |", v.family));
    }
    out.push_str("\n|---|");
    for _ in vocabs {
        out.push_str("---:|");
    }
    out.push('\n');
    let formats: [(&str, SurfaceSelector); 3] = [
        ("GMN", |a| a.gmn.as_str()),
        ("Turtle", |a| a.turtle.as_str()),
        ("JSON-LD", |a| a.jsonld.as_str()),
    ];
    // Cache per-(format,vocab) totals — reused for the ratio table below.
    let mut totals: Vec<Vec<u64>> = Vec::with_capacity(formats.len());
    for (label, select) in &formats {
        out.push_str(&format!("| {label} |"));
        let mut row = Vec::with_capacity(vocabs.len());
        for v in vocabs {
            let t = format_total(artifacts, v, select)?;
            out.push_str(&format!(" {t} |"));
            row.push(t);
        }
        out.push('\n');
        totals.push(row);
    }
    out.push('\n');

    // The flagship reading: GMN ÷ Turtle per family (< 1.0 ⇒ GMN is cheaper in tokens).
    out.push_str("### GMN ÷ Turtle token ratio (lower ⇒ GMN cheaper)\n\n");
    out.push_str("| Family | GMN ÷ Turtle | GMN ÷ JSON-LD |\n|---|---:|---:|\n");
    for (i, v) in vocabs.iter().enumerate() {
        let gmn = totals[0][i] as f64;
        let turtle = totals[1][i] as f64;
        let jsonld = totals[2][i] as f64;
        let r_ttl = if turtle > 0.0 { gmn / turtle } else { 0.0 };
        let r_json = if jsonld > 0.0 { gmn / jsonld } else { 0.0 };
        out.push_str(&format!("| {} | {r_ttl:.3} | {r_json:.3} |\n", v.family));
    }
    out.push('\n');

    // ── Matrix 2: per-symbol glyph token cost ──────────────────────────────────────
    let glyphs = ordered_glyphs(dict);
    out.push_str("## Per-symbol glyph token cost\n\n");
    out.push_str(
        "Token cost of each pinned GMN glyph under each vocabulary. `*` flags a BYTE-FRAGMENTING \
         glyph (> 1 token) — the weaker in-scope glyph/tokenizer co-design signal.\n\n",
    );
    out.push_str("| Glyph | Codepoints |");
    for v in vocabs {
        out.push_str(&format!(" {} |", v.family));
    }
    out.push_str("\n|---|---|");
    for _ in vocabs {
        out.push_str("---:|");
    }
    out.push('\n');
    // Per-vocab fragmenting rosters, collected while filling the table.
    let mut fragmenting: Vec<Vec<(String, String, usize)>> = vec![Vec::new(); vocabs.len()];
    for glyph in &glyphs {
        out.push_str(&format!("| `{}` | {} |", md_cell(glyph), codepoints(glyph)));
        for (vi, v) in vocabs.iter().enumerate() {
            let n = v.count(glyph)?;
            let flag = if n > 1 {
                fragmenting[vi].push((glyph.clone(), codepoints(glyph), n));
                "*"
            } else {
                ""
            };
            out.push_str(&format!(" {n}{flag} |"));
        }
        out.push('\n');
    }
    out.push('\n');

    // ── Matrix 3: byte-fragmenting glyph roster per family ─────────────────────────
    out.push_str("## Byte-fragmenting glyphs per family\n\n");
    out.push_str(
        "Glyphs that tokenize to more than one token under the family (byte-fallback / \
         fragmentation). These INFORM the S2–S4 glyph revisions.\n\n",
    );
    for (vi, v) in vocabs.iter().enumerate() {
        let roster = &fragmenting[vi];
        out.push_str(&format!(
            "* **{}**: {} byte-fragmenting glyph(s)",
            v.family,
            roster.len()
        ));
        if roster.is_empty() {
            out.push_str(" — none.\n");
        } else {
            out.push_str(".\n");
            for (glyph, cps, n) in roster {
                out.push_str(&format!("  * `{}` ({cps}): {n} tokens\n", md_cell(glyph)));
            }
        }
    }
    out.push('\n');

    Ok(out)
}

/// The canonical report path under the git-ignored `dist/` build-output tree. This OFF-GATE
/// maint report is never produced by the on-gate pipeline, so it must live OUTSIDE the pipeline's
/// `generated/` tree (which the superset gate scans for runtime `generated/` reads); `dist/` is
/// git-ignored, off-gate, and not superset-scanned.
#[must_use]
pub fn default_report_path(root: &Path) -> PathBuf {
    root.join("dist/bench/gmn-token-cost-matrix.md")
}

/// Write `report` to `path`, creating parent directories.
pub fn write_report(report: &str, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| MatrixError::Io(format!("create {}: {e}", parent.display())))?;
    }
    std::fs::write(path, report)
        .map_err(|e| MatrixError::Io(format!("write {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every committed digest pin is a 64-hex-char (lowercase) blake3 digest.
    #[test]
    fn pins_are_lowercase_hex_blake3() {
        for (family, pin) in [
            ("Qwen", QWEN_TOKENIZER_BLAKE3),
            ("Llama", LLAMA_TOKENIZER_BLAKE3),
            ("Gemma", GEMMA_TOKENIZER_BLAKE3),
        ] {
            assert_eq!(
                pin.len(),
                64,
                "{family} pin is a 32-byte blake3 (64 hex chars)"
            );
            assert!(
                pin.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{family} pin is lowercase hex"
            );
        }
    }

    /// The `U+XXXX` codepoint spelling is stable and space-joins multi-codepoint glyphs.
    #[test]
    fn codepoints_spelling_is_stable() {
        assert_eq!(codepoints("¬"), "U+00AC");
        assert_eq!(codepoints("⟦⟧"), "U+27E6 U+27E7");
    }

    /// The vendored Qwen asset on disk matches its committed pin — a corrupted or swapped
    /// in-repo asset is caught on the DEFAULT gate, not only in the maint lane.
    #[test]
    fn vendored_qwen_asset_matches_its_pin() {
        let bytes =
            std::fs::read(qwen_asset_path()).expect("vendored Qwen tokenizer.json is present");
        verify_blake3("Qwen", &bytes, QWEN_TOKENIZER_BLAKE3)
            .expect("the vendored Qwen asset matches its committed blake3 pin");
    }

    /// The three offline vocabularies load (2 embedded + vendored Qwen) and every one returns a
    /// finite, non-zero token count — the crate's core offline promise, verified without network.
    #[test]
    fn offline_vocabs_load_and_count() {
        let vocabs = offline_vocabs().expect("offline vocabularies load");
        assert_eq!(vocabs.len(), 3);
        for v in &vocabs {
            let n = v.count("subClassOf").expect("finite token count");
            assert!(n >= 1, "{} returns a finite non-zero count", v.family);
        }
    }
}
