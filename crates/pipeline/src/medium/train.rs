// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dictionary training: a thin adapter over [`purrdf::gts::dict`], and NOTHING else.
//!
//! PurRDF owns canonical, order-independent concatenation of the sample multiset,
//! deterministic FastCOVER training and finalization into a zstd dictionary.
//! Its native trainer reads the canonical corpus bytes and fixed training options;
//! it has no random generator or seed parameter.
//!
//! So this module is deliberately a pure function `&[&[u8]] -> Vec<u8>` plus a
//! strategy dispatch. It holds NO carrier state, NO registry, NO I/O. PurRDF owns
//! the order-independence and concurrency tests; GMEOW checks strategy dispatch,
//! finalized dictionary admission, and typed diagnostic translation.
//!
//! The upstream determinism contract makes the output a function of the sample
//! multiset and target length, including when pipeline stages run concurrently.

use purrdf::gts::dict::{dictionary_id, raw_content_dict, trained_dict};

use super::registry::DictionaryStrategy;

/// Build a finalized zstd dictionary from `corpus` under `strategy`.
///
/// `corpus` is the assembled sample multiset; its ORDER is irrelevant by
/// construction (upstream canonically sorts it before concatenating), which is what
/// lets the caller collect samples into a `BTreeSet` without pinning an emission
/// order anywhere.
///
/// [`DictionaryStrategy::TermTable`] shares the raw-content producer with
/// [`DictionaryStrategy::RawContent`]: the two differ in WHAT is fed in (the
/// bundle's own interned term table versus the declared corpus), which is a corpus
/// concern, not a training one — see [`super::corpus::term_table_sample`].
///
/// # Errors
/// An empty corpus, or a `target_len` too small to hold the finalized header and
/// offset history. Both are HARD FAILS: a dictionary that could not be built is
/// never silently replaced by "no dictionary", because a frame primed with the id
/// that dictionary was supposed to carry would then be undecodable.
pub fn build(
    strategy: DictionaryStrategy,
    corpus: &[&[u8]],
    target_len: usize,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
    let built = match strategy {
        DictionaryStrategy::Trained => trained_dict(corpus, target_len),
        DictionaryStrategy::RawContent | DictionaryStrategy::TermTable => {
            raw_content_dict(corpus, target_len)
        }
    };
    built.map_err(|err| {
        super::undeclared_dictionary(format!(
            "{strategy} training over {} sample(s) at target {target_len} bytes failed: {err}",
            corpus.len()
        ))
    })
}

/// The `Dictionary_ID` a finalized dictionary declares — the value every frame it
/// primes carries in its zstd frame header, and the join key a decoder resolves back
/// to the authored `gmeow:CompressionDictionary`.
///
/// # Errors
/// `dict` is not a parseable finalized zstd dictionary.
pub fn zstd_dictionary_id(dict: &[u8]) -> Result<u32, gmeow_errors::Diag> {
    dictionary_id(dict).map_err(|err| {
        super::digest_mismatch(format!(
            "the trained dictionary does not parse as a finalized zstd dictionary: {err}"
        ))
    })
}

#[path = "train.tests.rs"]
#[cfg(test)]
mod tests;
