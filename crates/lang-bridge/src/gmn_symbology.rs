// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN symbology plane's two machine primitives: the **real token-cost** of a glyph and
//! the **canonical `@λ` tabular column order**.
//!
//! GMN is a source code over the LLM token channel, so a symbol earns a glyph slot only if it
//! is actually cheap *in tokens* — not merely short in bytes. [`gmn_glyph_token_cost`] answers
//! that with the real cost: it encodes the string with a pinned, embedded `cl100k_base` BPE
//! vocabulary and returns the token count. The vocabulary is compiled into the binary (no
//! network, no filesystem read), so the measurement is deterministic and reproducible — the
//! same string always yields the same count. This is simultaneously the `⟦·⟧` fragmentation
//! benchmark (a glyph that fragments into several tokens costs more than a one-token named key,
//! so it is dispositioned to the key) and the per-glyph cost feed the machine-compression
//! sibling folds into the token-cost matrix.
//!
//! [`GMN_LANG_AST_COLUMNS`] pins the `@λ` (lang-AST) tabular batch to the **existing** CoNLL-U
//! column contract rather than inventing a rival: it is the ten Universal-Dependencies columns
//! in their canonical order, and `lang_ast_columns_match_conllu_serializer` asserts
//! that order against the [`crate::conllu`] serializer's own field order, so the two cannot
//! drift.

use std::sync::OnceLock;

use tiktoken_rs::{CoreBPE, cl100k_base};

/// The pinned BPE tokenizer whose vocabulary defines "a token" for GMN glyph-cost. Built once
/// and reused; the `cl100k_base` tables are embedded in the binary, so construction touches no
/// network and no filesystem and the result is process-stable.
fn tokenizer() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        // The embedded `cl100k_base` tables are well-formed; a failure here would be a
        // build-time packaging defect, not a runtime input error, so it is a hard fault.
        cl100k_base().expect("embedded cl100k_base BPE vocabulary must load")
    })
}

/// The real token cost of a GMN glyph (or any string) over the LLM token channel: the number
/// of `cl100k_base` BPE tokens it encodes to. Deterministic for a given input — the vocabulary
/// is pinned and embedded. A multi-token glyph (e.g. a non-ASCII bracket that the byte-level
/// BPE fragments) costs more than a one-token ASCII named key, which is exactly the signal the
/// glyph-vs-named-key disposition reads off.
///
/// "Ordinary" encoding is used: special-token markers are treated as literal text, because a
/// glyph string never carries the tokenizer's control tokens.
pub fn gmn_glyph_token_cost(glyph: &str) -> usize {
    tokenizer().encode_ordinary(glyph).len()
}

/// The canonical `@λ` (lang-AST) tabular column order: the ten Universal-Dependencies /
/// CoNLL-U columns, verbatim. A schema-once `@claims`-style batch of `@λ` rows declares these
/// columns in this order; the dialect reuses the CoNLL-U contract instead of minting a rival
/// column scheme. Pinned against the [`crate::conllu`] serializer by the test below.
pub const GMN_LANG_AST_COLUMNS: [&str; 10] = [
    "ID", "FORM", "LEMMA", "UPOS", "XPOS", "FEATS", "HEAD", "DEPREL", "DEPS", "MISC",
];

#[path = "gmn_symbology.tests.rs"]
#[cfg(test)]
mod tests;
