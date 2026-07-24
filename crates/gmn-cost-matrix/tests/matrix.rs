// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Off-gate (`#[ignore]`) determinism test for the GMN token-cost matrix.
//!
//! This test exercises the three OFFLINE-available families (o200k_base + cl100k_base embedded in
//! tiktoken-rs, and the vendored blake3-pinned Qwen) fully offline — it never touches the network,
//! so it is CI-runnable with `--ignored`. The full five-family sweep (which additionally fetches
//! the restricted-license Llama + Gemma assets at maint-time) is the `maint-gmn-cost-matrix`
//! Makefile lane, not this test.
//!
//! It is `#[ignore]` so it stays OFF the default gate: the matrix INFORMS the glyph/tokenizer
//! co-design, while the on-gate teeth remain `gmeow_lang_bridge::compute_token_metrics`.

use gmeow_gmn_cost_matrix::{
    build_corpus, default_report_path, offline_vocabs, render_matrix, repo_root, write_report,
};

/// For each offline-available vocabulary the matrix is deterministic (two full renders are
/// byte-identical); every emitted GMN artifact yields a FINITE token count under every family (no
/// vocabulary miss is silently dropped); and byte-fragmenting glyphs are detected and flagged in
/// the report. The rendered matrix is written to the canonical `dist/bench/` path.
#[test]
#[ignore = "off-gate token-cost matrix (INFORMS; run explicitly with --ignored / maint lane)"]
fn token_cost_matrix_is_deterministic_per_vocab() {
    let root = repo_root();
    let vocabs = offline_vocabs().expect("the three offline vocabularies load (2 embedded + Qwen)");
    assert_eq!(
        vocabs.len(),
        3,
        "the offline family set is exactly o200k_base + cl100k_base + Qwen"
    );

    let (dict, artifacts) = build_corpus(&root).expect("grounding corpus assembles");
    assert!(
        !artifacts.is_empty(),
        "the grounding corpus produced at least one round-tripping GMN artifact"
    );

    // No-optionality / no silent miss: every GMN artifact yields a FINITE token count under every
    // vocabulary (a byte-fallback vocabulary always covers every byte, so a miss would be a bug).
    let mut total_gmn_tokens = 0u64;
    for artifact in &artifacts {
        for vocab in &vocabs {
            let n = vocab.count(&artifact.gmn).unwrap_or_else(|e| {
                panic!("finite token count required for {}: {e}", artifact.name)
            });
            assert!(
                n > 0,
                "GMN artifact {} tokenizes to a non-zero, finite count under {}",
                artifact.name,
                vocab.family
            );
            total_gmn_tokens += n as u64;
        }
    }
    assert!(
        total_gmn_tokens > 0,
        "the corpus carries measurable GMN token cost"
    );

    // Byte-fragmenting glyphs must actually be detectable: at least one pinned GMN glyph fragments
    // (> 1 token) under at least one offline vocabulary — the signal the report flags. (The GMN
    // glyph plane is deliberately built from rare Unicode that a typical BPE byte-falls-back on.)
    let any_fragment = dict
        .glyph_registry()
        .glyph_tokens()
        .into_iter()
        .filter(|g| !g.is_empty())
        .any(|g| {
            vocabs
                .iter()
                .any(|v| v.count(g).expect("glyph tokenizes to a finite count") > 1)
        });
    assert!(
        any_fragment,
        "at least one pinned GMN glyph byte-fragments under an offline vocabulary — the \
         co-design signal the matrix flags"
    );

    // Determinism: two full renders over the same corpus + vocabulary set are byte-identical.
    let first = render_matrix(&dict, &artifacts, &vocabs).expect("render 1");
    let second = render_matrix(&dict, &artifacts, &vocabs).expect("render 2");
    assert_eq!(
        first, second,
        "the token-cost matrix render is byte-identical across runs (per-vocab determinism)"
    );

    // The report is non-trivial and carries the byte-fragment flag legend + a family roster.
    assert!(first.contains("# GMN token-cost matrix"));
    assert!(first.contains("Byte-fragmenting glyphs per family"));
    assert!(
        first.contains(" o200k_base ")
            && first.contains(" cl100k_base ")
            && first.contains(" Qwen "),
        "the report names all three offline families"
    );
    assert!(
        first.contains('*'),
        "the report flags at least one byte-fragmenting glyph with `*`"
    );

    // Materialize the matrix at its canonical (git-ignored) path — proof the test produces it.
    let out = default_report_path(&root);
    write_report(&first, &out).expect("write the matrix report");
    assert!(
        out.exists(),
        "the matrix report was written to {}",
        out.display()
    );
}
