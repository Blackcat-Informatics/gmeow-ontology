// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The anti-rot gate for the glyph-optimality axis's token costs.
//!
//! The axis (`axes::gmn_glyph_optimality_axis`) decides whether an adopted glyph earns its
//! place by comparing its LLM-token cost against the ASCII fallback it replaces. It used to
//! ask `gmeow_lang_bridge::gmn_glyph_token_cost` — the real `cl100k_base` BPE — which drags
//! `tiktoken-rs`' ~1.7 MB embedded vocabulary into every image that links this crate,
//! including the browser reasoning segment. It now reads
//! [`GMN_SYMBOL_AUDIT_TOKEN_COSTS`](gmeow_lang_bridge::GMN_SYMBOL_AUDIT_TOKEN_COSTS), the
//! pinned table, and the vocabulary is a DEV-dependency of this crate alone.
//!
//! That trade is only honest if the pin cannot rot, which is what this lane proves, in both
//! directions:
//!
//! * **Soundness** — every pinned cost equals what the real BPE measures TODAY, for the
//!   tokenizer version this workspace resolves. A `tiktoken-rs` bump that re-prices a glyph
//!   reds here.
//! * **Completeness** — every token the LIVE audited inventory carries (every
//!   `gmeow:gmnCandidateGlyph` and `gmeow:gmnAsciiFallback` across every governance source
//!   module) is priced. Authoring a new symbol candidate without pinning its cost reds
//!   here, rather than degrading the axis at runtime.
//!
//! The axis is measuring the same thing it always measured. Only the source of the
//! (gate-enforced identical) integers changed.

use std::collections::BTreeSet;
use std::path::PathBuf;

use purrdf::{DatasetView, GraphMatch, TermRef, TermValue};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Every distinct token the glyph-optimality audit can be asked to weigh, read out of the
/// authored slice modules through the ONE canonical RDF parser — never a text scan, which
/// would miss a candidate authored with a different literal quoting or prefix form.
fn live_audit_tokens() -> BTreeSet<String> {
    const CANDIDATE_GLYPH: &str = "https://blackcatinformatics.ca/gmeow/gmnCandidateGlyph";
    const ASCII_FALLBACK: &str = "https://blackcatinformatics.ca/gmeow/gmnAsciiFallback";

    let root = repo_root();
    let mut tokens = BTreeSet::new();
    for module in gmeow_slice_quality::governance_source_modules(&root) {
        let bytes = std::fs::read(&module).unwrap_or_else(|e| {
            panic!("governance module {} does not read: {e}", module.display())
        });
        let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None).unwrap_or_else(|e| {
            panic!("governance module {} does not parse: {e}", module.display())
        });
        for predicate in [CANDIDATE_GLYPH, ASCII_FALLBACK] {
            let Some(pid) = dataset.term_id_by_value(&TermValue::iri(predicate)) else {
                continue;
            };
            for quad in dataset.quads_for_pattern(None, Some(pid), None, GraphMatch::Any) {
                if let TermRef::Literal { lexical, .. } = dataset.resolve(quad.o) {
                    tokens.insert(lexical.to_owned());
                }
            }
        }
    }
    assert!(
        !tokens.is_empty(),
        "the governance modules carry no gmn symbol candidate at all — the scan found \
         nothing to pin, which means this gate is vacuous"
    );
    tokens
}

/// Soundness + completeness in one call: `assert_pinned_audit_costs_match_the_real_bpe`
/// re-measures every pinned entry against the real `cl100k_base` BPE and then proves the
/// live inventory is fully covered.
#[test]
fn the_pinned_audit_costs_are_the_real_bpe_costs_and_cover_the_live_inventory() {
    let live = live_audit_tokens();
    let refs: Vec<&str> = live.iter().map(String::as_str).collect();
    gmeow_lang_bridge::assert_pinned_audit_costs_match_the_real_bpe(&refs);
}

/// The shipped codec legend's own pin, re-asserted here for the same reason: this crate is
/// the other consumer of `GLYPH_TOKEN_COSTS`' value space, and the two tables must not
/// drift apart in a build that `crates/gmn-wasm`'s lane does not cover.
#[test]
fn the_pinned_codec_legend_is_a_subset_of_the_pinned_audit_table() {
    for (glyph, cost) in gmeow_lang_bridge::GLYPH_TOKEN_COSTS {
        let audited =
            gmeow_lang_bridge::pinned_symbol_audit_token_cost(glyph).unwrap_or_else(|e| {
                panic!("the codec legend prices {glyph:?} but the audit table does not: {e}")
            });
        assert_eq!(
            *cost, audited,
            "the codec legend and the audit table disagree about {glyph:?}"
        );
    }
}
