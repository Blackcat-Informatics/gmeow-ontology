// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Dictionary training: a thin adapter over [`purrdf::gts::dict`], and NOTHING else.
//!
//! Upstream already owns deterministic dictionary construction — canonical
//! order-independent concatenation of the sample multiset, an EXPLICIT
//! [`DictSeed`], a save/restore of the thread-local `fastrand` state around
//! FastCOVER, and finalization into a real zstd dictionary binary. Reimplementing
//! any of that here would shadow upstream rather than subsume it (`.goals`:
//! SUBSUME/EXTEND upstream, never shadow it), and a second implementation of
//! "canonical corpus order" is exactly the kind of divergence that produces two
//! dictionaries with one id.
//!
//! So this module is deliberately a pure function `&[&[u8]] -> Vec<u8>` plus a
//! strategy dispatch. It holds NO carrier state, NO registry, NO I/O. That purity
//! is what makes the order-independence and concurrency tests below meaningful:
//! there is no hidden input for a difference to hide in.
//!
//! # Determinism by construction, not by discipline
//!
//! The pipeline runs its DAG on every available CPU, so a "train on one thread
//! only" workaround would be unenforceable. It is also unnecessary: the seed is an
//! explicit [`DictSeed::FromCorpus`] (BLAKE3 over the canonical corpus bytes), and
//! `fastrand`'s generator is thread-LOCAL and round-tripped around the call, so the
//! output is a pure function of `(corpus, target_len)` no matter what any other
//! thread — or the same thread, before or after — is doing with `fastrand`.

use purrdf::gts::dict::{DictSeed, dictionary_id, raw_content_dict, trained_dict};

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
        DictionaryStrategy::Trained => trained_dict(corpus, target_len, DictSeed::FromCorpus),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A corpus with enough repeated structure for FastCOVER to actually train on.
    /// Shaped like the RDF the real dictionaries see, so the test exercises the
    /// production path rather than a degenerate one.
    fn sample_corpus() -> Vec<Vec<u8>> {
        (0..400u32)
            .map(|i| {
                format!(
                    "<https://blackcatinformatics.ca/gmeow/term{}> \
                     <https://blackcatinformatics.ca/gmeow/definition> \
                     \"a definition of term {} in the gmeow ontology\" .\n",
                    i % 37,
                    i
                )
                .into_bytes()
            })
            .collect()
    }

    fn slices(owned: &[Vec<u8>]) -> Vec<&[u8]> {
        owned.iter().map(Vec::as_slice).collect()
    }

    /// (a) Training the SAME corpus twice yields byte-identical dictionaries, under
    /// both strategy families.
    #[test]
    fn training_the_same_corpus_twice_is_byte_identical() {
        let owned = sample_corpus();
        let corpus = slices(&owned);
        for strategy in [
            DictionaryStrategy::Trained,
            DictionaryStrategy::RawContent,
            DictionaryStrategy::TermTable,
        ] {
            let first = build(strategy, &corpus, 4096).expect("build");
            let second = build(strategy, &corpus, 4096).expect("build");
            assert_eq!(first, second, "{strategy} must be byte-reproducible");
            assert!(!first.is_empty(), "{strategy} produced empty bytes");
            // A finalized dictionary declares a non-zero Dictionary_ID; a bare
            // raw-content blob would not parse at all.
            assert_ne!(
                zstd_dictionary_id(&first).expect("finalized dictionary"),
                0,
                "{strategy} must produce a FINALIZED dictionary a decoder can prime with"
            );
        }
    }

    /// (b) REVERSING the corpus iteration order changes nothing: the dictionary is a
    /// function of the sample multiset, which is what lets the caller assemble
    /// samples into a `BTreeSet` without that set's order becoming load-bearing.
    #[test]
    fn reversing_corpus_iteration_order_is_byte_identical() {
        let owned = sample_corpus();
        let forward = slices(&owned);
        let mut reversed = forward.clone();
        reversed.reverse();
        for strategy in [
            DictionaryStrategy::Trained,
            DictionaryStrategy::RawContent,
            DictionaryStrategy::TermTable,
        ] {
            assert_eq!(
                build(strategy, &forward, 4096).expect("build"),
                build(strategy, &reversed, 4096).expect("build"),
                "{strategy} must be a pure function of the sample MULTISET"
            );
        }
    }

    /// (c) CONCURRENT training from many threads is byte-identical to the serial
    /// result — runnable for the first time now that the seed is explicit.
    ///
    /// This is the test the pipeline actually needs: the DAG runs on every CPU, so
    /// "train on one thread" is not an available discipline. Each thread ALSO
    /// perturbs the ambient `fastrand` stream before and after training, so a
    /// regression to an ambient-seeded trainer would show up as a difference rather
    /// than passing by luck.
    #[test]
    fn concurrent_training_from_many_threads_is_byte_identical() {
        let owned = sample_corpus();
        let expected: Vec<Vec<u8>> = [DictionaryStrategy::Trained, DictionaryStrategy::RawContent]
            .iter()
            .map(|s| build(*s, &slices(&owned), 4096).expect("serial build"))
            .collect();

        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8u64)
                .map(|worker| {
                    let owned = &owned;
                    scope.spawn(move || {
                        // Perturb this thread's ambient generator in a
                        // worker-specific way: if training observed or leaked
                        // ambient state, these would diverge.
                        fastrand::seed(worker * 7919 + 1);
                        let _ = fastrand::u64(..);
                        let out: Vec<Vec<u8>> =
                            [DictionaryStrategy::Trained, DictionaryStrategy::RawContent]
                                .iter()
                                .map(|s| build(*s, &slices(owned), 4096).expect("concurrent build"))
                                .collect();
                        let _ = fastrand::u64(..);
                        out
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("worker thread"))
                .collect::<Vec<_>>()
        });

        for (worker, got) in results.iter().enumerate() {
            assert_eq!(
                got, &expected,
                "worker {worker} produced different dictionary bytes than the serial build — \
                 training is not a pure function of (corpus, target_len)"
            );
        }
    }

    /// An empty corpus is a HARD FAIL, never "no dictionary": a frame primed with
    /// the id this dictionary was supposed to carry would be undecodable.
    #[test]
    fn an_empty_corpus_hard_fails() {
        let error = build(DictionaryStrategy::Trained, &[], 4096)
            .expect_err("an empty corpus must be rejected");
        assert_eq!(
            error.code(),
            crate::error::MediumUndeclaredDictionary::register()
        );
    }
}
