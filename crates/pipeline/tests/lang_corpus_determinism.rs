// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Gate 6 — the three `lang:` corpus producers are byte-identical across repeated
//! regeneration, discharged ON-GATE by execution (not by fixture existence).
//!
//! The three pure corpus producers are:
//!
//! * [`lang_lowering::build_corpus`] → the `graph/lang-lowering-corpus` N-Triples,
//! * [`lang_form::build_corpus`] → the `graph/lang-form-corpus` N-Triples,
//! * [`lang_projection::build_corpus`] → the `graph/lang-projection-corpus` N-Triples
//!   PLUS the committed external `generated/projections/lang/**` artifacts (per-reading
//!   `.conllu`, OntoLex, EBNF, …).
//!
//! # Two legs, and why the two-in-process-run leg is not a false green
//!
//! Determinism is proven on two complementary legs:
//!
//! 1. **This test — the in-process two-run byte replay.** Each producer runs TWICE in one
//!    process and every serialized byte payload (corpus graphs AND the committed `.conllu`
//!    projection artifacts) must be byte-identical across the two runs.
//!
//! 2. **The drift lane — `run_full(RunMode::Check)` (`make sync SYNC_MODE=check SYNC_OUTPUTS=generated`).** Fresh
//!    process, re-derives every committed artifact and reconciles it: the per-reading
//!    `.conllu` files by EXACT BYTES (`crates/pipeline/src/run.rs:737-755`, the non-RDF
//!    committed-artifact compare), and the corpus graphs through the `gmeow.gts`
//!    superset/fold gate. The three corpora ride that lane because the mappings stage folds
//!    them (`crates/pipeline/src/stages/mappings.rs:278-294`): `lang_projection`'s `.conllu`
//!    artifacts are inserted into the committed-artifact set, and the corpus graphs are
//!    carried as named graphs into the bundle.
//!
//! A same-process two-run byte compare is, on its own, defeatable by the `HashMap`/`HashSet`
//! per-process `RandomState` seed: two hash collections built in one process share the
//! process seed, so an unsorted iteration into the serialization could agree in-process yet
//! diverge across FRESH processes. This test rules that false green out at the ROOT rather
//! than merely relying on the fresh-process drift lane: [`assert_sorted_canonical`] proves
//! every corpus payload is in the SORTED, DEDUPED canonical order the shared
//! `ntriples_sorted`/`to_ntriples` emitters produce. Sorted output is a pure function of the
//! LINE SET — independent of any `RandomState` — so if any producer had leaked a
//! `HashMap`/`HashSet` iteration into its bytes, the payload would NOT be in sorted order and
//! this assertion would red. (Confirmed against the code: the only `HashMap`/`HashSet` uses
//! in `crates/lang-bridge/src` are the digest-collision guard and test-only sets, none in an
//! emitted-bytes path; every producer routes its lines through the sort-and-dedup
//! canonicalizer.) The sorted-order property + the two-run byte identity + the fresh-process
//! drift lane together discharge Gate 6 with no false green.

use std::path::{Path, PathBuf};

use gmeow_pipeline::gmeow_slice_vocab;
use gmeow_pipeline::stages::{lang_form, lang_lowering, lang_projection};
use purrdf::slice::SliceCatalog;

/// The repo root (two levels up from this crate's manifest), the same anchor the sibling
/// integration tests resolve the committed sources and artifacts under.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// The shared in-memory source catalog — discovered exactly as the mappings stage discovers
/// it, so every producer runs over the SAME composed-source universe the production
/// `compile_mappings` path holds (never a fresh, independent second disk walk).
fn repo_catalog() -> SliceCatalog {
    SliceCatalog::discover(&repo_root().join("slices"), gmeow_slice_vocab())
        .expect("discover slice catalog")
}

/// The executable defeat of the `HashMap`/`HashSet` per-process-seed false green: assert a
/// serialized N-Triples payload is in the SORTED, DEDUPED canonical order every `lang:`
/// producer emits through the shared `ntriples_sorted`/`to_ntriples` canonicalizer. Sorted
/// order is a pure function of the line set — independent of any `RandomState` — so this
/// property holding is what makes the same-process two-run byte compare a sound proxy for
/// cross-process byte identity (which the `run_full(RunMode::Check)` drift lane additionally
/// reconciles fresh-process). If any producer leaked an unsorted `HashMap`/`HashSet`
/// iteration into its bytes, the payload would not be in sorted order and this would red.
fn assert_sorted_canonical(label: &str, bytes: &[u8]) {
    let text =
        std::str::from_utf8(bytes).unwrap_or_else(|e| panic!("{label}: corpus not UTF-8: {e}"));
    let lines: Vec<&str> = text.lines().collect();

    let mut sorted = lines.clone();
    sorted.sort_unstable();
    assert_eq!(
        lines, sorted,
        "{label}: the corpus N-Triples are not in sorted canonical order — a \
         HashMap/HashSet iteration must have leaked into the serialization (the \
         per-process-seed false green Gate 6 forbids)"
    );

    let mut deduped = sorted.clone();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        lines.len(),
        "{label}: the corpus N-Triples carry duplicate lines — the canonical emitter dedups, \
         so a duplicate means the payload did not pass through it"
    );
}

/// Gate 6 (lowering corpus): [`lang_lowering::build_corpus`] is byte-reproducible across two
/// runs AND emits sorted canonical output (no HashMap-seed false green).
#[test]
fn lang_corpus_determinism_lowering_graph_is_reproducible_and_sorted() {
    let a = lang_lowering::build_corpus()
        .expect("build lowering corpus a")
        .ntriples;
    let b = lang_lowering::build_corpus()
        .expect("build lowering corpus b")
        .ntriples;
    assert_eq!(
        a, b,
        "the lang-lowering corpus must be byte-identical across two runs"
    );
    assert!(!a.is_empty(), "the lang-lowering corpus must not be empty");
    assert_sorted_canonical("lang-lowering-corpus", &a);
}

/// Gate 6 (prose-lift corpus): [`lang_form::build_corpus`] is byte-reproducible across two
/// runs AND emits sorted canonical output.
#[test]
fn lang_corpus_determinism_form_graph_is_reproducible_and_sorted() {
    let catalog = repo_catalog();
    let a = lang_form::build_corpus(Some(&catalog))
        .expect("build form corpus a")
        .ntriples;
    let b = lang_form::build_corpus(Some(&catalog))
        .expect("build form corpus b")
        .ntriples;
    assert_eq!(
        a, b,
        "the lang-form corpus must be byte-identical across two runs"
    );
    assert!(!a.is_empty(), "the lang-form corpus must not be empty");
    assert_sorted_canonical("lang-form-corpus", &a);
}

/// Gate 6 (projection corpus): [`lang_projection::build_corpus`] is byte-reproducible across
/// two runs for BOTH the corpus graph AND every committed external artifact (the per-reading
/// `.conllu` files et al.), and the corpus graph is in sorted canonical order.
#[test]
fn lang_corpus_determinism_projection_graph_and_artifacts_are_reproducible() {
    let catalog = repo_catalog();
    let a = lang_projection::build_corpus(Some(&catalog)).expect("build projection corpus a");
    let b = lang_projection::build_corpus(Some(&catalog)).expect("build projection corpus b");

    // The corpus graph: byte-identical across runs, and in sorted canonical order.
    assert_eq!(
        a.ntriples, b.ntriples,
        "the lang-projection corpus graph must be byte-identical across two runs"
    );
    assert!(
        !a.ntriples.is_empty(),
        "the lang-projection corpus graph must not be empty"
    );
    assert_sorted_canonical("lang-projection-corpus", &a.ntriples);

    // The committed external artifacts: every (path, bytes) pair — including the per-reading
    // `.conllu` files — is byte-identical across the two runs, both in path order and in
    // content. `build_corpus` sorts `artifacts` by path, so a plain vec-equality is the
    // full determinism check (ordering + content).
    assert_eq!(
        a.artifacts, b.artifacts,
        "every generated lang projection artifact must be byte-identical across two runs \
         (paths and bytes), including the per-reading .conllu files"
    );
    assert!(
        !a.artifacts.is_empty(),
        "the lang-projection corpus must produce external artifacts"
    );

    // The per-reading `.conllu` projection artifacts specifically must be present and stable
    // — they are the byte-fragile UD trees Gate 2 hardens and Gate 6 replays.
    let conllu_count = a
        .artifacts
        .iter()
        .filter(|(p, _)| p.starts_with("generated/projections/lang/conllu/"))
        .count();
    assert!(
        conllu_count > 0,
        "the projection corpus must emit per-reading .conllu artifacts"
    );
}

/// Gate 6 drift-lane coverage (executable): the committed
/// `generated/projections/lang/conllu/*.conllu` files reproduce byte-for-byte from a fresh
/// producer run, and every committed file is a produced artifact (no orphan, no drift). This
/// is exactly what `run_full(RunMode::Check)` reconciles fresh-process at
/// `crates/pipeline/src/run.rs:737-755` (the non-RDF committed-artifact byte compare), asserted
/// here directly rather than as prose so the drift-lane leg is executable on-gate.
#[test]
fn lang_corpus_determinism_committed_conllu_reconciles_with_the_drift_lane() {
    let root = repo_root();
    let catalog = repo_catalog();
    let corpus = lang_projection::build_corpus(Some(&catalog)).expect("build projection corpus");

    // Index the produced per-reading `.conllu` artifacts by their committed logical path.
    let produced: Vec<(&String, &Vec<u8>)> = corpus
        .artifacts
        .iter()
        .filter(|(p, _)| p.starts_with("generated/projections/lang/conllu/"))
        .map(|(p, b)| (p, b))
        .collect();
    assert!(
        !produced.is_empty(),
        "the projection corpus must produce per-reading .conllu artifacts to reconcile"
    );

    // Leg 1 — producer → committed: every produced `.conllu` byte-reproduces its committed
    // file (the exact `run_full(Check)` non-RDF compare).
    for (path, bytes) in &produced {
        let committed = std::fs::read(root.join(path))
            .unwrap_or_else(|e| panic!("committed {path} must exist for reconciliation: {e}"));
        assert_eq!(
            &committed, *bytes,
            "committed {path} must reproduce byte-for-byte from the producer (the \
             run_full(Check) drift-lane reconciliation)"
        );
    }

    // Leg 2 — committed → producer: every committed `.conllu` on disk is a produced artifact
    // (no committed file the producer no longer emits — the orphan direction of the drift lane).
    let committed_dir = root.join("generated/projections/lang/conllu");
    let mut committed_files: Vec<PathBuf> = std::fs::read_dir(&committed_dir)
        .expect("the committed conllu projection dir must exist")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("conllu"))
        .collect();
    committed_files.sort();
    assert!(
        !committed_files.is_empty(),
        "the committed conllu projection dir must carry .conllu files"
    );
    for file in &committed_files {
        let logical = logical_path(&root, file);
        assert!(
            produced.iter().any(|(p, _)| **p == logical),
            "committed {logical} is not produced by the projection corpus (drift-lane orphan)"
        );
    }
}

/// The committed logical path (repo-root-relative, forward slashes) for an absolute path
/// under the repo, matching the keys `build_corpus` mints its artifacts under.
fn logical_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root)
        .expect("committed file is under the repo root")
        .to_string_lossy()
        .replace('\\', "/")
}
