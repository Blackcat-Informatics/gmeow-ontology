// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twins migrated from `slices/core/gts/tests/test_gts_slice.py`.
//!
//! The GTS transport slice's two invariants that a boolean SHACL `ASK` cannot
//! reach:
//! * `value_vocabulary_cardinality_floors` — the numeric cardinality of the open
//!   value vocabularies (`>= 7` `GTSProfile`, `>= 7` `TransformCodec`, `== 3`
//!   `CodecClass`), which an `ASK` cannot assert.
//! * `competency_queries_parse_and_run` — a parse+execute SMOKE over the slice's
//!   `queries/*.rq` with NO pinned expected result (authoring an expected result
//!   would fabricate an assertion the original smoke never made).
//!
//! The Python originals loaded `load_merged_graph(include_imports=True)`; the
//! native merged-ontology store is imports-false. The counted TBox vocabularies
//! (`GTSProfile`/`TransformCodec`/`CodecClass` individuals) live in the slice's own
//! `module.ttl`, so they are present without the vendored imports — verified green
//! by the floors below.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_value_vocabulary_cardinality_floors`: the open value vocabularies
/// (P9) — the numeric cardinality floors a boolean ASK cannot express.
#[gmeow_test_batch_macros::batch_test]
fn value_vocabulary_cardinality_floors() {
    let g = GraphStore::ontology();

    let profiles = g.subjects_of_type(&gm("GTSProfile"));
    assert!(
        profiles.len() >= 7,
        "expected >= 7 GTSProfile individuals, got {}: {profiles:?}",
        profiles.len()
    );

    let codecs = g.subjects_of_type(&gm("TransformCodec"));
    assert!(
        codecs.len() >= 7,
        "expected >= 7 TransformCodec individuals, got {}: {codecs:?}",
        codecs.len()
    );

    let classes = g.subjects_of_type(&gm("CodecClass"));
    assert_eq!(
        classes.len(),
        3,
        "expected exactly 3 CodecClass individuals, got {}: {classes:?}",
        classes.len()
    );
}

/// Twin of `test_competency_queries_parse_and_run`: a parse+execute SMOKE over every
/// `slices/core/gts/queries/*.rq`. The originals materialised `list(g.query(text))`
/// with no result assertion; the native twin runs each query via `select` over the
/// merged ontology (which panics on a parse/execution error), asserting only that
/// each executes. `evidence-packages-signers.rq` carries an `OPTIONAL` block
/// (registered `Feature::Optional` in `MIGRATION_FEATURE_REGISTRY` as
/// `gts-slice/evidence-packages-signers`).
#[gmeow_test_batch_macros::batch_test]
fn competency_queries_parse_and_run() {
    let g = GraphStore::ontology();
    let queries_dir = repo_root().join("slices/core/gts/queries");
    let mut rq_paths: Vec<std::path::PathBuf> = std::fs::read_dir(&queries_dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", queries_dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("rq"))
        .collect();
    rq_paths.sort();
    assert!(
        !rq_paths.is_empty(),
        "no .rq competency queries found under {}",
        queries_dir.display()
    );
    for path in &rq_paths {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        // `select` panics with the query text on any parse/execution failure — the
        // smoke's whole contract. We assert nothing about the (empty-over-TBox) rows.
        let _ = g.select(&[], &text);
    }
}
