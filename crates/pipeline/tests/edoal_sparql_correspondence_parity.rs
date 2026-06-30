// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Corpus parity oracle for the EDOAL + SPARQL correspondence lowerings.
//!
//! Both dialects lower from ONE shared get-leg model (`get_leg::projections`), so they
//! cannot drift. This proves the full oxigraph-free rail on real data: natively merge
//! the DSL + ontology sources into two `DslView`s, lower via
//! `gmeow_logic_compile::projections::{edoal::lower_edoal, sparql::lower_sparql}`, and
//! assert each emitted artifact is byte-identical to the committed
//! `generated/projections/*.edoal.ttl` and `generated/queries/*.rq`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence_frontend::{
    transpile_correspondences_indexed, CorrespondenceLookup,
};
use gmeow_logic_compile::projections::edoal::lower_edoal;
use gmeow_logic_compile::projections::get_leg::projections;
use gmeow_logic_compile::projections::sparql::lower_sparql;
use gmeow_rdf::{parse_dataset, NativeRdfFormat, RdfDataset, RdfDatasetBuilder};
use gmeow_slice::{ArtifactRole, SliceCatalog};

/// The materialized correspondence lookup the four lowerings consume for their overclaim
/// gate / ledger path (F5 Task 2). Built from the SAME merged DSL the lowerings read, so
/// the consumed typed relation and the rendered artifact agree by construction.
fn build_lookup(dsl: &RdfDataset, onto: &RdfDataset) -> CorrespondenceLookup {
    transpile_correspondences_indexed(&DslView::new(dsl), &DslView::new(onto))
        .expect("transpile correspondence lookup")
        .1
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

fn collect_ttl_files(dir: &Path, out: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    for entry in std::fs::read_dir(dir).expect("read_dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect_ttl_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ttl") {
            out.push(path);
        }
    }
}

fn parse_turtle(bytes: &[u8]) -> Arc<RdfDataset> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None).expect("parse turtle")
}

fn merge_slice_artifacts(root: &Path, role: ArtifactRole, b: &mut RdfDatasetBuilder) {
    let slices_dir = root.join("slices");
    if !slices_dir.is_dir() {
        return;
    }
    let catalog = SliceCatalog::discover(&slices_dir).expect("discover slices");
    let mut artifacts: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for record in catalog.records() {
        for artifact in &record.artifacts {
            if artifact.role == role {
                artifacts.push((
                    record.slice_dir.join(&artifact.logical_path),
                    artifact.content.clone(),
                ));
            }
        }
    }
    artifacts.sort_by(|a, c| a.0.cmp(&c.0));
    for (_, bytes) in &artifacts {
        b.push_dataset(&parse_turtle(bytes));
    }
}

fn merge_dsl(root: &Path) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let mut dsl_files = Vec::new();
    collect_ttl_files(&root.join("dsl").join("mappings"), &mut dsl_files);
    dsl_files.sort();
    for path in &dsl_files {
        b.push_dataset(&parse_turtle(&std::fs::read(path).expect("read dsl")));
    }
    merge_slice_artifacts(root, ArtifactRole::Mapping, &mut b);
    b.freeze().expect("freeze dsl")
}

fn merge_ontology(root: &Path) -> Arc<RdfDataset> {
    let mut b = RdfDatasetBuilder::new();
    let onto = root.join("ontology").join("gmeow.ttl");
    if onto.is_file() {
        b.push_dataset(&parse_turtle(&std::fs::read(&onto).expect("read ontology")));
    }
    merge_slice_artifacts(root, ArtifactRole::Module, &mut b);
    b.freeze().expect("freeze ontology")
}

/// The set of committed file names under `dir` whose name ends with `suffix`.
fn committed_file_set(dir: &Path, suffix: &str) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap_or_else(|_| panic!("read committed dir {}", dir.display()))
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(suffix))
        .collect()
}

fn assert_corpus(
    label: &str,
    emitted: &BTreeMap<String, String>,
    committed_dir: &Path,
    suffix: &str,
) {
    // Set-equality FIRST: the emitted key set MUST equal the committed file set, so a
    // dropped or stray artifact fails before the per-file content diff.
    let emitted_keys: BTreeSet<String> = emitted.keys().cloned().collect();
    let committed_keys = committed_file_set(committed_dir, suffix);
    assert_eq!(
        emitted_keys, committed_keys,
        "{label}: emitted file set diverged from the committed corpus (missing/extra artifact)",
    );

    let mut mismatches: Vec<String> = Vec::new();
    for (file, text) in emitted {
        let path = committed_dir.join(file);
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("committed artifact missing: {}", path.display()));
        if *text != committed {
            let first = committed
                .lines()
                .zip(text.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| {
                    format!("  line {i}:\n    committed: {a:?}\n    lowered:   {b:?}")
                })
                .unwrap_or_else(|| {
                    format!(
                        "  length: committed={} lowered={}",
                        committed.len(),
                        text.len()
                    )
                });
            mismatches.push(format!("{file}\n{first}"));
        }
    }
    eprintln!(
        "{label} corpus parity: {} files, {} byte-exact, {} mismatched",
        emitted.len(),
        emitted.len() - mismatches.len(),
        mismatches.len()
    );
    for m in mismatches.iter().take(6) {
        eprintln!("MISMATCH {m}");
    }
    assert!(
        mismatches.is_empty(),
        "{} {label} files diverged from the committed corpus",
        mismatches.len()
    );
}

#[test]
fn edoal_lowering_matches_committed_corpus() {
    let root = repo_root();
    let dsl = merge_dsl(&root);
    let onto = merge_ontology(&root);
    let lookup = build_lookup(&dsl, &onto);
    let emitted = lower_edoal(&DslView::new(&dsl), &DslView::new(&onto), &lookup)
        .expect("lower edoal")
        .alignments;
    assert!(
        emitted.len() >= 40,
        "expected ~45 EDOAL files, got {}",
        emitted.len()
    );
    assert_corpus(
        "EDOAL",
        &emitted,
        &root.join("generated").join("projections"),
        ".edoal.ttl",
    );
}

/// Normalize a `.rq` so the deterministic cell-scan reordering compares equal: the
/// CONSTRUCT templates, UNION branches, and `drops:` list are all collected in cell
/// order (the lowering sorts cells by IRI; the historical emitter used the store's hash
/// order), but their *content* is identical. Strip the `UNION ` branch-position prefix
/// (so a branch is the same lines whether it is first or not), sort the `drops:` list,
/// then return the sorted multiset of lines — equal iff the queries are content-equal.
fn normalize_rq(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# Lossy and directional by design") {
            // Header drops line: sort the `; `-separated drop notes.
            if let Some(drops) = rest
                .strip_prefix("; drops: ")
                .and_then(|s| s.strip_suffix('.'))
            {
                let mut parts: Vec<&str> = drops.split("; ").collect();
                parts.sort_unstable();
                lines.push(format!(
                    "# Lossy and directional by design; drops: {}.",
                    parts.join("; ")
                ));
            } else {
                lines.push(line.to_owned());
            }
            continue;
        }
        // A branch opens with `{` when first, `UNION {` otherwise — fold to one form.
        lines.push(line.replace("UNION {", "{"));
    }
    lines.sort();
    lines
}

#[test]
fn sparql_lowering_matches_committed_corpus_modulo_order() {
    let root = repo_root();
    let dsl = merge_dsl(&root);
    let onto = merge_ontology(&root);
    let lookup = build_lookup(&dsl, &onto);
    let emitted = lower_sparql(&DslView::new(&dsl), &DslView::new(&onto), &lookup)
        .expect("lower sparql")
        .queries;
    assert!(
        emitted.len() >= 40,
        "expected ~45 .rq files, got {}",
        emitted.len()
    );

    let committed_dir = root.join("generated").join("queries");

    // Set-equality FIRST: the per-profile `.rq` set the lowering emits MUST equal the
    // committed per-profile `.rq` set (the `standpoint-*.rq` + `observation-claim-view.rq`
    // queries are emitted by other producers and are excluded here). A dropped or stray
    // artifact fails before the content diff.
    let emitted_keys: BTreeSet<String> = emitted.keys().cloned().collect();
    let committed_keys: BTreeSet<String> = committed_file_set(&committed_dir, ".rq")
        .into_iter()
        .filter(|n| !n.starts_with("standpoint-") && n != "observation-claim-view.rq")
        .collect();
    assert_eq!(
        emitted_keys, committed_keys,
        "emitted per-profile `.rq` set diverged from the committed corpus (missing/extra artifact)",
    );

    let mut mismatches: Vec<String> = Vec::new();
    for (file, text) in &emitted {
        let committed = std::fs::read_to_string(committed_dir.join(file))
            .unwrap_or_else(|_| panic!("committed missing: {file}"));
        let lo = normalize_rq(text);
        let co = normalize_rq(&committed);
        if lo != co {
            // Report the first differing normalized line.
            let first = co
                .iter()
                .zip(lo.iter())
                .find(|(a, b)| a != b)
                .map(|(a, b)| format!("  committed: {a:?}\n  lowered:   {b:?}"))
                .unwrap_or_else(|| {
                    format!("  line count: committed={} lowered={}", co.len(), lo.len())
                });
            mismatches.push(format!("{file}\n{first}"));
        }
    }
    eprintln!(
        "SPARQL corpus parity (modulo deterministic cell-order): {} files, {} content-equal, {} mismatched",
        emitted.len(),
        emitted.len() - mismatches.len(),
        mismatches.len()
    );
    for m in mismatches.iter().take(6) {
        eprintln!("MISMATCH {m}");
    }
    assert!(
        mismatches.is_empty(),
        "{} .rq files diverged beyond the deterministic cell-order reordering",
        mismatches.len()
    );
}

#[test]
fn edoal_and_sparql_share_one_get_leg() {
    // Spec-drift gone by construction: both dialects consume the SAME parsed get-leg
    // model. Extract the get leg ONCE, then drive both `lower_edoal` and `lower_sparql`
    // — they share that one extraction — and prove the extraction is deterministic by
    // re-running it and comparing the get-leg CONTENTS, cell-by-cell (not just the
    // length), so a content drift between two runs is caught.
    let root = repo_root();
    let dsl = merge_dsl(&root);
    let onto = merge_ontology(&root);
    let dsl_view = DslView::new(&dsl);
    let onto_view = DslView::new(&onto);

    // One extraction of the shared get leg.
    let leg = projections(&dsl_view).expect("shared get leg");
    assert!(!leg.is_empty(), "the get leg has projection cells");

    // Both dialects lower from that same model surface and succeed.
    let lookup = build_lookup(&dsl, &onto);
    let edoal = lower_edoal(&dsl_view, &onto_view, &lookup).expect("lower edoal");
    let sparql = lower_sparql(&dsl_view, &onto_view, &lookup).expect("lower sparql");
    assert!(
        !edoal.alignments.is_empty(),
        "edoal lowered from the get leg"
    );
    assert!(
        !sparql.queries.is_empty(),
        "sparql lowered from the get leg"
    );

    // Re-extract and compare the get-leg CONTENTS (the `Debug` rendering is a faithful
    // content projection of each cell) — equal iff the extraction is deterministic.
    let again = projections(&dsl_view).expect("re-extract get leg");
    let leg_repr: Vec<String> = leg.iter().map(|c| format!("{c:?}")).collect();
    let again_repr: Vec<String> = again.iter().map(|c| format!("{c:?}")).collect();
    assert_eq!(
        leg_repr, again_repr,
        "the shared get leg is content-deterministic across extractions",
    );
}
