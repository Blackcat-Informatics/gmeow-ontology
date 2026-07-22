// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Corpus parity oracle for the SSSOM correspondence lowering.
//!
//! Proves the full oxigraph-free rail end-to-end on real data: parse the DSL + slice
//! mapping sources natively into one merged `RdfDataset`, lower every
//! native alignment cells via `gmeow_logic_compile::projections::sssom::lower_sssom`,
//! and assert each emitted TSV is byte-identical to the committed
//! `generated/mappings/*.sssom.tsv`. The committed artifacts are the source of truth
//! (the historical oxigraph emitter already matches them), so new-lowering == committed
//! transitively proves new == old.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::correspondence_frontend::transpile_correspondences_indexed;
use gmeow_logic_compile::projections::sssom::lower_sssom;
use purrdf::dataset_view::{DatasetView, GraphMatch};
use purrdf::slice::{ArtifactRole, SliceCatalog};
use purrdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, TermRef, TermValue, parse_dataset};

const GM_VERSION_FINGERPRINT: &str = "https://blackcatinformatics.ca/gmeow/versionFingerprint";
const GM_DATE_PUBLISHED: &str = "https://blackcatinformatics.ca/gmeow/datePublished";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root resolves")
}

/// Recursively collect every `*.ttl` under `dir` (no-op if absent).
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

/// Parse a Turtle byte source into a fresh dataset.
fn parse_turtle(bytes: &[u8]) -> Arc<RdfDataset> {
    parse_dataset(bytes, NativeRdfFormat::Turtle.media_type(), None).expect("parse turtle source")
}

/// Merge the SSSOM source set into one dataset, in the SAME order the historical
/// emitter loads them: the sorted `dsl/mappings/**/*.ttl` tree first, then the sorted
/// slice `*/*/mappings/*.ttl` artifacts. `push_dataset` standardizes-apart each
/// source's blank scope, so the merge is collision-free.
fn merge_sssom_sources(root: &Path) -> Arc<RdfDataset> {
    let mut builder = RdfDatasetBuilder::new();

    let dsl_dir = root.join("dsl").join("mappings");
    let mut dsl_files: Vec<PathBuf> = Vec::new();
    collect_ttl_files(&dsl_dir, &mut dsl_files);
    dsl_files.sort();
    for path in &dsl_files {
        let bytes = std::fs::read(path).expect("read dsl source");
        builder.push_dataset(&parse_turtle(&bytes));
    }

    let slices_dir = root.join("slices");
    if slices_dir.is_dir() {
        let catalog = SliceCatalog::discover(
            &slices_dir,
            purrdf::SliceVocab::for_namespace("https://blackcatinformatics.ca/gmeow/"),
        )
        .expect("discover slices");
        let mut slice_mappings: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        for record in catalog.records() {
            for artifact in &record.artifacts {
                if artifact.role == ArtifactRole::Mapping {
                    let path = record.slice_dir.join(&artifact.logical_path);
                    slice_mappings.push((path, artifact.content.clone()));
                }
            }
        }
        slice_mappings.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, bytes) in &slice_mappings {
            builder.push_dataset(&parse_turtle(bytes));
        }
    }

    builder.freeze().expect("freeze merged dataset")
}

/// Read `(version, release_date)` from `metadata/gmeow-self.ttl`: the Manifestation is
/// the subject of `gmeow:versionFingerprint`; its `gmeow:datePublished` is the date.
fn read_self_metadata(root: &Path) -> (String, String) {
    let bytes = std::fs::read(root.join("metadata").join("gmeow-self.ttl")).expect("self ttl");
    let ds = parse_turtle(&bytes);
    let vfp = ds
        .term_id_by_value(&TermValue::Iri(GM_VERSION_FINGERPRINT.to_owned()))
        .expect("versionFingerprint predicate present");
    // The single subject carrying gmeow:versionFingerprint.
    let manifestation = ds
        .quads_for_pattern(None, Some(vfp), None, GraphMatch::Default)
        .next()
        .expect("a manifestation with versionFingerprint")
        .s;
    let subject_iri = match ds.resolve(manifestation) {
        TermRef::Iri(iri) => iri.to_owned(),
        _ => panic!("versionFingerprint subject is not an IRI"),
    };
    let view = DslView::new(&ds);
    let version = view
        .object_literal(&subject_iri, GM_VERSION_FINGERPRINT)
        .expect("versionFingerprint literal");
    let release_date = view
        .object_literal(&subject_iri, GM_DATE_PUBLISHED)
        .expect("datePublished literal");
    (version, release_date)
}

#[test]
fn sssom_lowering_matches_committed_corpus() {
    let root = repo_root();
    let merged = merge_sssom_sources(&root);
    let view = DslView::new(&merged);
    let (version, release_date) = read_self_metadata(&root);

    // The materialized correspondence lookup the SSSOM ledger gate consumes (F5 Task 2),
    // built from the SAME merged DSL so the consumed typed relation and the rendered TSV
    // agree by construction. An empty ontology view suffices (equivalence keys read only
    // the DSL).
    let empty = parse_turtle(b"");
    let (_program, lookup) = transpile_correspondences_indexed(&view, &DslView::new(&empty))
        .expect("transpile correspondence lookup");

    let emitted: BTreeMap<String, String> = lower_sssom(&view, &version, &release_date, &lookup)
        .expect("lower sssom")
        .sets;
    assert!(
        emitted.len() >= 60,
        "expected ~66 SSSOM sets, lowered {}",
        emitted.len()
    );

    let committed_dir = root.join("generated").join("mappings");

    // Set-equality FIRST: the emitted key set MUST equal the committed `*.sssom.tsv`
    // file set, so a dropped or stray artifact fails before the per-file content diff
    // (a `>=N` count guard would let a swap slip through).
    let emitted_keys: BTreeSet<String> = emitted.keys().cloned().collect();
    let committed_keys: BTreeSet<String> = std::fs::read_dir(&committed_dir)
        .expect("read committed mappings dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sssom.tsv"))
        .collect();
    assert_eq!(
        emitted_keys, committed_keys,
        "emitted SSSOM file set diverged from the committed corpus (missing/extra artifact)",
    );

    let mut mismatches: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for (file, tsv) in &emitted {
        let path = committed_dir.join(file);
        let committed = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("committed artifact missing: {}", path.display()));
        checked += 1;
        if *tsv != committed {
            let first_diff = committed
                .lines()
                .zip(tsv.lines())
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| {
                    format!("  line {i}:\n    committed: {a:?}\n    lowered:   {b:?}")
                })
                .unwrap_or_else(|| {
                    format!(
                        "  length: committed={} lowered={}",
                        committed.len(),
                        tsv.len()
                    )
                });
            mismatches.push(format!("{file}\n{first_diff}"));
        }
    }

    assert!(
        mismatches.is_empty(),
        "{} SSSOM sets diverged from the committed corpus (of {checked} checked):\n{}",
        mismatches.len(),
        mismatches
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
