// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The deterministic GMN-1 **token-metric 7-vector** and its SOUND compression gate.
//!
//! The GMN codebook declares an EXPECTED rate (`gmeow:gmnDeclaredRate` →
//! `gmeow:gmnRateTokensPerStatement`, "the design expectation the token metrics measure
//! against"). This module computes the MEASURED realization over the grounding corpus: a
//! seven-metric vector, each metric a dimensionless `math:Quantity` magnitude, wrapped as
//! a `gmeow:Measurement` observation individual (the projection stage emits the RDF; this
//! module owns the arithmetic).
//!
//! Every quantity is a pure function of the source bytes + the pinned dictionary, so the
//! vector is byte-deterministic across runs (the emission's determinism test asserts it).
//!
//! ## The SOUND compression gate (the whole point)
//!
//! The flagship claim is "GMN costs fewer LLM tokens than Turtle". Gating that on the naive
//! `chars/4` estimate for BOTH surfaces would ship a FALSE claim: a BPE tokenizer
//! byte-falls-back on **rare Unicode glyphs** (each GMN glyph is a 2–4-byte UTF-8 sequence a
//! typical vocabulary does not merge), so `chars/4` UNDER-counts GMN exactly on its glyphs.
//! But the converse over-count is just as wrong: charging EVERY GMN byte as a fallback token
//! (`gmn_worst = total_bytes`) is internally inconsistent — it penalizes GMN's ASCII at 1
//! byte/token while granting Turtle a 4:1 ASCII merge. Real BPE byte-fallback happens ONLY on
//! the non-ASCII glyph bytes; ASCII merges identically for both surfaces.
//!
//! The gate therefore compares a CONSISTENT adversarial bound:
//!
//! * [`gmn_worst_case_tokens`](TokenMetrics::gmn_worst_case_tokens)
//!   `= ceil(gmn_ascii_bytes / 4) + gmn_nonascii_bytes`. Every non-ASCII glyph byte falls
//!   back to its own token (the worst a tokenizer does to a glyph — zero merge credit), while
//!   ASCII gets the SAME optimistic 4:1 merge Turtle gets. This is the sound formalization of
//!   "byte-fallback on rare Unicode glyphs".
//! * [`turtle_best_case_tokens`](TokenMetrics::turtle_best_case_tokens)
//!   `= ceil(turtle_chars / 4)` — an OPTIMISTIC lower bound on Turtle's token cost. Four
//!   characters per token is the best-case efficiency a BPE tokenizer reaches on plain ASCII;
//!   structured Turtle (angle-bracketed IRIs, prefixes, punctuation that breaks merges)
//!   realistically does WORSE (more tokens), so `chars/4` is a principled floor that only
//!   makes the claim HARDER to satisfy.
//!
//! The gate requires `gmn_worst_case_tokens < turtle_best_case_tokens` over the corpus. It is
//! FALSIFIABLE: a future glyph-dense dialect whose non-ASCII byte-fallback cost exceeded the
//! Turtle savings would push `gmn_worst` over `turtle_best` and RED the gate. The realistic
//! reading (both surfaces at `chars/4`,
//! [`gmn_realistic_tokens`](TokenMetrics::gmn_realistic_tokens)) is carried beside it so the
//! product is fully transparent about the adversarial vs realistic spread. Live BPE
//! tokenization is out of scope on-gate (the off-gate token-cost matrix owns that); this
//! deterministic byte-fallback bound is the on-gate teeth.

use crate::gmn1_codec::{
    Gmn0Model, GmnDictionary, gmn0_canonically_equal, gmn1_read, gmn1_write, measure_coverage,
};

/// The measured GMN-1 token-metric 7-vector over a grounding corpus, plus the compression
/// gate's two adversarial witness counts. Byte counts and glyph/compression aggregates are
/// taken over the sources whose GMN emission round-trips exactly (the ones that ship a
/// `.gmn` artifact); the two RATE metrics span every parseable source (their denominator).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TokenMetrics {
    // ── the seven-metric vector ──────────────────────────────────────────────────
    /// 1. `bytes_on_disk`: total UTF-8 byte length of the GMN serialization over the
    ///    round-tripping corpus (the flagship on-disk product size).
    pub bytes_on_disk: u64,
    /// 2. `tokens_in_context`: the deterministic `estimate_tokens` (chars/4, rounded up)
    ///    over the aggregate GMN text — the model-agnostic context-window estimate.
    pub tokens_in_context: u64,
    /// 3. `ast_validity_rate`: fraction of parseable sources whose GMN document re-parses
    ///    (`gmn1_read` succeeds) — `[0,1]`.
    pub ast_validity_rate: f64,
    /// 4. `roundtrip_loss`: fraction of parseable sources that FAIL the canonical
    ///    round-trip (`gmn0_canonically_equal` false, or write/read failed) — `[0,1]`.
    pub roundtrip_loss: f64,
    /// 5. `compression_ratio`: aggregate GMN bytes ÷ aggregate Turtle bytes over the
    ///    round-tripping corpus (`< 1` means GMN is smaller on disk).
    pub compression_ratio: f64,
    /// 6. `glyph_density`: fraction of GMN characters covered by a registry glyph token
    ///    (longest-match) — `[0,1]`.
    pub glyph_density: f64,
    /// 7. `dictionary_hit_rate`: fraction of corpus quads the codec covers losslessly
    ///    against the pinned dictionary (`measure_coverage`) — `[0,1]`.
    pub dictionary_hit_rate: f64,

    // ── the compression-gate witnesses (shipped beside the vector as data) ────────
    /// The GMN CONSISTENT adversarial worst case (the gate's left side):
    /// `ceil(gmn_ascii_bytes / 4) + gmn_nonascii_bytes` — ASCII merged 4:1 (as for Turtle),
    /// every non-ASCII glyph byte falling back to its own token. See the module doc for why
    /// this, not `total_bytes`, is the sound formalization.
    pub gmn_worst_case_tokens: u64,
    /// The GMN realistic reading (both surfaces at `chars/4`): `ceil(gmn_chars / 4)`. Equal to
    /// [`tokens_in_context`](Self::tokens_in_context) up to per-source rounding; carried so the
    /// adversarial vs realistic spread is transparent. NOT the gate — that uses the worst case.
    pub gmn_realistic_tokens: u64,
    /// The Turtle optimistic best case (the gate's right side): `ceil(turtle_chars / 4)` over
    /// the round-tripping corpus.
    pub turtle_best_case_tokens: u64,
    /// The GMN ASCII byte count over the round-tripping corpus — the merge-eligible bytes (the
    /// worst-case ASCII term's basis). Provenance for the byte-fallback split.
    pub gmn_ascii_bytes: u64,
    /// The GMN non-ASCII (glyph) byte count over the round-tripping corpus — the byte-fallback
    /// bytes (the worst-case glyph term). Provenance for the byte-fallback split.
    pub gmn_nonascii_bytes: u64,
    /// Aggregate Turtle byte length (the compression-ratio denominator's byte basis).
    pub turtle_bytes_on_disk: u64,
    /// Aggregate JSON-LD byte length — the third serialization in the `bytes_on_disk`
    /// comparison (GMN vs Turtle vs JSON-LD over the SAME dataset).
    pub jsonld_bytes_on_disk: u64,

    // ── corpus provenance ─────────────────────────────────────────────────────────
    /// The number of parseable sources measured (the rate-metric denominator).
    pub total_sources: u64,
    /// The number of sources whose GMN emission round-trips exactly (the byte-aggregate
    /// denominator; the sources that ship a `.gmn` artifact).
    pub measured_sources: u64,
}

impl TokenMetrics {
    /// Whether the SOUND compression gate holds: GMN's byte-fallback worst case is strictly
    /// cheaper than Turtle's optimistic best case over the corpus. A vacuous corpus (no
    /// round-tripping source, so no GMN bytes to defend) does NOT hold — there is no claim
    /// to make, and the caller emits no metrics product rather than a vacuous one.
    #[must_use]
    pub fn compression_gate_holds(&self) -> bool {
        self.measured_sources > 0 && self.gmn_worst_case_tokens < self.turtle_best_case_tokens
    }
}

/// The deterministic, model-agnostic token estimate for `text`: one token per ~4 characters,
/// rounded up (the standard rough byte-pair ratio). Mirrors `gmeow_docs::llms::estimate_tokens`
/// exactly; redeclared here rather than depended on because `gmeow-docs` sits DOWNSTREAM of
/// `gmeow-lang-bridge` (a dependency would invert the crate DAG). Empty in → `0`.
#[must_use]
fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// A bounded `[0,1]` ratio with the vacuous-denominator convention (`0/0 == 1.0`, matching
/// [`crate::gmn1_codec::CoverageReport::fraction`]): nothing to fail is trivially perfect.
#[allow(clippy::cast_precision_loss)]
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        1.0
    } else {
        numerator as f64 / denominator as f64
    }
}

/// Count the characters of `text` covered by a registry glyph token under a deterministic
/// longest-match scan. `glyphs` MUST be ordered longest-first (as
/// [`GmnGlyphRegistry::glyph_tokens`](crate::gmn1_codec::GmnGlyphRegistry::glyph_tokens)
/// returns), so the greedy walk is unambiguous. A position matching no glyph advances one
/// character. Returns the number of glyph-covered characters (the density numerator).
fn count_glyph_chars(text: &str, glyphs: &[&str]) -> u64 {
    let mut covered = 0u64;
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(glyph) = glyphs
            .iter()
            .find(|g| !g.is_empty() && rest.starts_with(**g))
        {
            covered += glyph.chars().count() as u64;
            rest = &rest[glyph.len()..];
        } else {
            // Advance exactly one character (byte-safe: step to the next char boundary).
            let step = rest.chars().next().map_or(1, char::len_utf8);
            rest = &rest[step..];
        }
    }
    covered
}

/// Serialize `model`'s GMN-0 normal-form dataset to Turtle and JSON-LD, returning both byte
/// lengths and the Turtle character count. Both serializations are over the SAME frozen
/// dataset the GMN writer lowered, so the three surfaces compare like for like. Serialization
/// of a deterministically-ordered dataset is itself deterministic (the emission's determinism
/// test proves it end-to-end).
fn serialize_sizes(model: &Gmn0Model) -> Option<(u64, u64, u64)> {
    let dataset = model.to_dataset();
    let turtle =
        purrdf::serialize_dataset_to_format(&*dataset, purrdf::NativeRdfFormat::Turtle, None)
            .ok()?;
    let jsonld = purrdf::serialize_dataset_to_jsonld(&*dataset).ok()?;
    let turtle_bytes = turtle.bytes.len() as u64;
    let turtle_chars = String::from_utf8(turtle.bytes).ok()?.chars().count() as u64;
    let jsonld_bytes = jsonld.len() as u64;
    Some((turtle_bytes, turtle_chars, jsonld_bytes))
}

/// Compute the [`TokenMetrics`] 7-vector over `sources` against the pinned `dict`.
///
/// A source that does not parse as Turtle is out of the metric domain and is skipped (it
/// never entered the GMN pipeline). Every parseable source counts toward the two rate
/// metrics' denominator; only a source whose GMN emission round-trips exactly contributes
/// its bytes/glyph/compression aggregates — those are the artifacts GMN actually ships, so
/// the compression claim is scoped honestly to them.
#[must_use]
pub fn compute_token_metrics(
    sources: &[crate::registry::NamedSource],
    dict: &GmnDictionary,
) -> TokenMetrics {
    measure_token_metrics(sources, dict).aggregate()
}

/// Native per-source measurements retained for reductions over explicit source scopes.
/// Source positions refer to the original input, including sources outside the parse domain;
/// selecting a scope never parses or encodes its corpus again.
#[derive(Clone, Debug)]
pub struct MeasuredTokenCorpus {
    sources: Vec<(usize, MetricTotals)>,
}

impl MeasuredTokenCorpus {
    /// The measurement over every parseable source in the selected input.
    #[must_use]
    pub fn aggregate(&self) -> TokenMetrics {
        self.selected(|_| true)
    }

    /// Reduce the already measured sources whose original positions satisfy `includes`.
    #[must_use]
    pub fn selected(&self, includes: impl Fn(usize) -> bool) -> TokenMetrics {
        let mut totals = MetricTotals::default();
        for (index, source) in &self.sources {
            if includes(*index) {
                totals.add(source);
            }
        }
        totals.metrics()
    }
}

#[derive(Clone, Debug, Default)]
struct MetricTotals {
    total_sources: u64,
    valid_sources: u64,
    roundtrip_sources: u64,
    gmn_bytes: u64,
    gmn_ascii_bytes: u64,
    gmn_nonascii_bytes: u64,
    gmn_chars: u64,
    tokens_in_context: u64,
    glyph_chars: u64,
    turtle_bytes: u64,
    turtle_chars: u64,
    jsonld_bytes: u64,
    covered_quads: u64,
    total_quads: u64,
}

impl MetricTotals {
    fn add(&mut self, source: &Self) {
        self.total_sources += source.total_sources;
        self.valid_sources += source.valid_sources;
        self.roundtrip_sources += source.roundtrip_sources;
        self.gmn_bytes += source.gmn_bytes;
        self.gmn_ascii_bytes += source.gmn_ascii_bytes;
        self.gmn_nonascii_bytes += source.gmn_nonascii_bytes;
        self.gmn_chars += source.gmn_chars;
        self.tokens_in_context += source.tokens_in_context;
        self.glyph_chars += source.glyph_chars;
        self.turtle_bytes += source.turtle_bytes;
        self.turtle_chars += source.turtle_chars;
        self.jsonld_bytes += source.jsonld_bytes;
        self.covered_quads += source.covered_quads;
        self.total_quads += source.total_quads;
    }

    fn metrics(&self) -> TokenMetrics {
        TokenMetrics {
            bytes_on_disk: self.gmn_bytes,
            tokens_in_context: self.tokens_in_context,
            ast_validity_rate: ratio(self.valid_sources, self.total_sources),
            roundtrip_loss: 1.0 - ratio(self.roundtrip_sources, self.total_sources),
            compression_ratio: ratio(self.gmn_bytes, self.turtle_bytes),
            glyph_density: ratio(self.glyph_chars, self.gmn_chars),
            dictionary_hit_rate: ratio(self.covered_quads, self.total_quads),
            gmn_worst_case_tokens: self.gmn_ascii_bytes.div_ceil(4) + self.gmn_nonascii_bytes,
            gmn_realistic_tokens: self.gmn_chars.div_ceil(4),
            turtle_best_case_tokens: self.turtle_chars.div_ceil(4),
            gmn_ascii_bytes: self.gmn_ascii_bytes,
            gmn_nonascii_bytes: self.gmn_nonascii_bytes,
            turtle_bytes_on_disk: self.turtle_bytes,
            jsonld_bytes_on_disk: self.jsonld_bytes,
            total_sources: self.total_sources,
            measured_sources: self.roundtrip_sources,
        }
    }
}

/// Measure each source once, retaining integer witnesses before any aggregate rounding.
/// Parse failures remain outside the domain. Read success contributes to AST validity even
/// when canonical equality or serialization fails; byte totals count only complete round trips.
#[must_use]
pub fn measure_token_metrics(
    sources: &[crate::registry::NamedSource],
    dict: &GmnDictionary,
) -> MeasuredTokenCorpus {
    let glyphs = dict.glyph_registry().glyph_tokens();
    MeasuredTokenCorpus {
        sources: sources
            .iter()
            .enumerate()
            .filter_map(|(index, source)| {
                measure_source(source, dict, &glyphs).map(|measured| (index, measured))
            })
            .collect(),
    }
}

fn measure_source(
    source: &crate::registry::NamedSource,
    dict: &GmnDictionary,
    glyphs: &[&str],
) -> Option<MetricTotals> {
    let dataset = purrdf::parse_dataset(&source.bytes, "text/turtle", None).ok()?;
    let model = Gmn0Model::from_dataset(&dataset);
    let coverage = measure_coverage(&model, dict);
    let mut measured = MetricTotals {
        total_sources: 1,
        covered_quads: coverage.covered as u64,
        total_quads: coverage.total as u64,
        ..MetricTotals::default()
    };
    let Ok(doc) = gmn1_write(&model, dict) else {
        return Some(measured);
    };
    let Ok(back) = gmn1_read(&doc, dict) else {
        return Some(measured);
    };
    measured.valid_sources = 1;
    if !gmn0_canonically_equal(&model, &back) {
        return Some(measured);
    }
    let Some((turtle_bytes, turtle_chars, jsonld_bytes)) = serialize_sizes(&model) else {
        return Some(measured);
    };
    measured.roundtrip_sources = 1;
    measured.gmn_bytes = doc.text.len() as u64;
    measured.gmn_ascii_bytes = doc.text.bytes().filter(u8::is_ascii).count() as u64;
    measured.gmn_nonascii_bytes = doc.text.bytes().filter(|byte| !byte.is_ascii()).count() as u64;
    measured.gmn_chars = doc.text.chars().count() as u64;
    measured.tokens_in_context = estimate_tokens(&doc.text);
    measured.glyph_chars = count_glyph_chars(&doc.text, glyphs);
    measured.turtle_bytes = turtle_bytes;
    measured.turtle_chars = turtle_chars;
    measured.jsonld_bytes = jsonld_bytes;
    Some(measured)
}

#[path = "gmn_metrics.tests.rs"]
#[cfg(test)]
mod tests;
