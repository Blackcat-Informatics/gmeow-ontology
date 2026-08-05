// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native coverage for `Dataset::from_gts` — the headline capability the docs bundle
//! explorer depends on (`crates/docs/assets/gmeow-docs.js` calls `fromGts` to load the
//! shipped `gmeow.gts`), yet before this test nothing invoked it: the other query-wasm
//! test suites (`tests/witness_query.rs`, `js/tests/witness.test.mjs`,
//! `js/tests/shipped.test.mjs`) all exercise `Dataset::parse` over `corpus.trig`. A
//! rebuild that silently dropped or broke `from_gts` would have shipped a dead
//! explorer with every existing gate green.
//!
//! The fixture bundle is built with the MANDATED GMEOW GTS authorship profile
//! (`gmeow_gts_profile::emit_gmeow_gts`, backed by `purrdf::gts_compose::SnapshotBuilder`
//! and `MediumPlan`) — never a direct `purrdf::gts_compose::emit_gts` call — so this
//! test reads back exactly the shape of bundle every real GMEOW `gmeow.gts` is.

use gmeow_gts_profile::emit_gmeow_gts;
use gmeow_query_wasm::Dataset;
use purrdf::gts_compose::SnapshotBuilder;

const DEFAULT_GRAPH_TTL: &str = concat!(
    "@prefix ex: <https://example.org/> .\n",
    "ex:alice a ex:Person ;\n",
    "    ex:knows ex:bob .\n",
    "ex:bob a ex:Person .\n",
);

const NAMED_GRAPH_IRI: &str = "https://example.org/graph/named";

/// A TriG document carrying a default-graph triple plus one named graph — the
/// structure `Dataset::from_gts` must preserve rather than fold into a single union.
fn bundle_source_trig() -> String {
    format!(
        "{DEFAULT_GRAPH_TTL}\n<{NAMED_GRAPH_IRI}> {{\n    <https://example.org/carol> a <https://example.org/Person> .\n}}\n"
    )
}

/// Build a real `gmeow.gts` bundle (header + one snapshot frame, zstd-rsyncable at
/// level 12, exactly as production code emits) carrying [`bundle_source_trig`].
fn build_test_gts() -> Vec<u8> {
    let dataset = purrdf::parse_dataset(bundle_source_trig().as_bytes(), "trig", None)
        .expect("parse fixture trig");
    let mut builder = SnapshotBuilder::new();
    builder.add_dataset(&dataset).expect("add_dataset");
    let bytes = emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
        .expect("emit fixture gmeow.gts");
    gmeow_gts_profile::validate_mandated_frames(&bytes)
        .expect("fixture bundle uses the mandated GTS frame profile");
    bytes
}

#[test]
fn from_gts_reads_every_quad_in_a_real_bundle() {
    let gts = build_test_gts();
    let dataset = Dataset::from_gts(&gts).expect("Dataset::from_gts reads the fixture bundle");
    // 3 default-graph quads (`ex:alice a ex:Person`, `ex:alice ex:knows ex:bob`,
    // `ex:bob a ex:Person`) + 1 named-graph quad (`ex:carol a ex:Person`) = 4 quads
    // total, across every graph.
    assert_eq!(
        dataset.size(),
        4,
        "Dataset::from_gts must read every quad in every graph of the bundle"
    );
}

#[test]
fn from_gts_preserves_the_named_graph() {
    let gts = build_test_gts();
    let dataset = Dataset::from_gts(&gts).expect("Dataset::from_gts reads the fixture bundle");
    let nquads = dataset
        .serialize("nquads")
        .expect("serialize the round-tripped dataset as N-Quads");
    assert!(
        nquads.contains(NAMED_GRAPH_IRI),
        "the named graph {NAMED_GRAPH_IRI} did not survive Dataset::from_gts: {nquads}"
    );
    // And the default-graph triples are NOT tagged with that graph — from_gts must
    // not fold everything into one graph in either direction.
    let default_graph_line = nquads
        .lines()
        .find(|line| line.contains("https://example.org/alice"))
        .expect("the default-graph ex:alice triple survives");
    assert!(
        !default_graph_line.contains(NAMED_GRAPH_IRI),
        "a default-graph quad was mis-homed into the named graph: {default_graph_line}"
    );
}

#[test]
fn from_gts_reads_the_bundle_queryably() {
    // The whole point of `from_gts` over a flattened extract: a query can select the
    // named graph specifically, and the RDF 1.2 statement layer stays addressable.
    let gts = build_test_gts();
    let dataset = Dataset::from_gts(&gts).expect("Dataset::from_gts reads the fixture bundle");
    let out = dataset
        .query(
            &format!(
                "SELECT ?s WHERE {{ GRAPH <{NAMED_GRAPH_IRI}> {{ ?s a <https://example.org/Person> }} }}"
            ),
            None,
        )
        .expect("query the named graph");
    assert!(
        out.contains("https://example.org/carol"),
        "querying the named graph through from_gts did not find its content: {out}"
    );
}

// The refusal contract (garbage bytes must throw, not silently produce an empty
// dataset) is asserted in `js/tests/shipped.test.mjs`, not here: `Dataset::from_gts`'s
// error path constructs a `JsError`, a wasm-bindgen imported type that PANICS if
// constructed on a native target ("cannot call wasm-bindgen imported functions on
// non-wasm targets") — the same reason `src/lib.rs`'s unit tests defer the `query()`
// throw contract to the Node lane. A native test driving that path would abort the
// test process, not exercise the boundary it claims to guard.
