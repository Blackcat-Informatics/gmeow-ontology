// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixture-termination gate: every `logic:` compiler product must reach a real
//! downstream consumer, not dead-end on disk or in a conformance fixture.
//!
//! The compiler emits four information products — the canonical IR, the
//! projection-report loss ledger, the derivation-graph explanations, and the
//! compile diagnostics. This gate proves each is routed:
//!
//! * IR + loss ledger + diagnostics are first-class `stage-compile-logic` DAG
//!   products (committed artifacts the regenerate/drift gate owns), and
//! * the loss ledger rides the assembled bundle as the `projection-ledger` named
//!   graph while the compile diagnostics union into the `diagnostics` graph.
//!
//! The IR (`REP_AXIOMS`) and the derivation-graph reports (`REP_REASONING`) are
//! pinned by the snapshot unit tests `build_archive_blobs_folds_the_axiom_surface`
//! and `build_reasoning_blob_folds_the_report_artifacts`; this gate covers the two
//! products that previously terminated in fixtures — the loss ledger and diagnostics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_pipeline::node::{Stage, StageInput, StageProduct};
use gmeow_pipeline::stages::compile_logic::{
    CompileLogicStage, CANONICAL_RDF12_PATH, DIAG_RDF_PATH, DIAG_SARIF_PATH, PROJECTION_REPORT_PATH,
};

/// The bundle named-graph IRIs the loss ledger and diagnostics ride.
const GRAPH_PROJECTION_LEDGER: &str =
    "https://blackcatinformatics.ca/gmeow/graph/projection-ledger";
const GRAPH_DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Run the compile-logic stage over the repo and return its product.
fn compile_product(root: &Path) -> StageProduct {
    let upstream = BTreeMap::new();
    CompileLogicStage::new()
        .run(StageInput {
            root,
            upstream: &upstream,
        })
        .expect("compile-logic stage")
        .product
}

#[test]
fn compiler_products_are_first_class_dag_artifacts() {
    let root = repo_root();
    let product = compile_product(&root);

    // The IR, the loss ledger, and the diagnostics SARIF/RDF are committed-path
    // artifacts of the DAG — not files written only by a side CLI, and not values
    // that live only inside a conformance fixture comparison.
    for path in [
        CANONICAL_RDF12_PATH,   // canonical IR
        PROJECTION_REPORT_PATH, // loss ledger
        DIAG_SARIF_PATH,        // diagnostics → SARIF
        DIAG_RDF_PATH,          // diagnostics → gmeow:Finding RDF
    ] {
        assert!(
            product.artifact(path).is_some(),
            "{path} must be a stage-compile-logic product (not fixture-only)"
        );
    }

    // The loss ledger is a non-trivial RDF report (the ProjectionReport individual),
    // not an empty placeholder.
    let report = std::str::from_utf8(product.artifact(PROJECTION_REPORT_PATH).unwrap()).unwrap();
    assert!(
        report.contains("ProjectionReport"),
        "the loss ledger must carry a logic:ProjectionReport"
    );

    // The PROJECTION_REPORT_PATH lives under generated/, NOT only under a conformance
    // fixture tree — the literal "no product terminates in a test fixture" guard.
    assert!(
        PROJECTION_REPORT_PATH.starts_with("generated/"),
        "the loss ledger must be a generated artifact, not a fixture path"
    );
}

#[test]
fn loss_ledger_and_diagnostics_reach_the_assembled_bundle() {
    let root = repo_root();

    // Assemble a snapshot the way SnapshotStage would, with the real stage products
    // build_snapshot consumes (statements RDF 1.2, docs graph, SHACL diagnostics, and
    // the compile-logic product).
    let (_, rdf12) = gmeow_pipeline::stages::statements::compile_statements(&root).unwrap();
    let docs = gmeow_pipeline::stages::docs_render::render_docs_graph(&root).unwrap();

    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    let mut st: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    st.insert(
        gmeow_pipeline::stages::statements::RDF12_PATH.to_string(),
        rdf12.into_bytes(),
    );
    upstream.insert(
        "stage-statements".to_string(),
        StageProduct::from_artifacts("stage-statements", st),
    );
    let mut dc: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    dc.insert(
        gmeow_pipeline::stages::docs_render::DOCS_GRAPH_PATH.to_string(),
        docs.into_bytes(),
    );
    upstream.insert(
        "stage-docs-render".to_string(),
        StageProduct::from_artifacts("stage-docs-render", dc),
    );
    let mut vd: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    vd.insert(
        gmeow_pipeline::stages::validate::SHACL_RDF_PATH.to_string(),
        Vec::new(),
    );
    upstream.insert(
        "stage-validate".to_string(),
        StageProduct::from_artifacts("stage-validate", vd),
    );
    upstream.insert("stage-compile-logic".to_string(), compile_product(&root));

    let gts =
        gmeow_pipeline::stages::snapshot::build_snapshot(&root, &upstream, Vec::new(), Vec::new())
            .expect("build_snapshot");

    // Import the assembled bundle and inspect its named graphs.
    let bundle = gmeow_rdf::import_gts_events(&gts).expect("import_gts_events");
    let quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(bundle.dataset.as_ref())
        .expect("flat quads");

    let in_graph = |graph: &str| -> Vec<String> {
        quads
            .iter()
            .filter(|q| q.graph_name.to_string().contains(graph))
            .map(|q| format!("{} {} {}", q.subject, q.predicate, q.object))
            .collect()
    };

    // The loss ledger rides the projection-ledger named graph.
    let ledger = in_graph("graph/projection-ledger");
    assert!(
        !ledger.is_empty(),
        "the {GRAPH_PROJECTION_LEDGER} named graph must be present in the bundle"
    );
    assert!(
        ledger.iter().any(|q| q.contains("ProjectionReport")),
        "the projection-ledger graph must carry a logic:ProjectionReport"
    );
    assert!(
        ledger.iter().any(|q| q.contains("lossyDrop")),
        "the projection-ledger graph must carry at least one gmeow:lossyDrop"
    );

    // The compile diagnostics union into the diagnostics graph.
    let diagnostics = in_graph("graph/diagnostics");
    assert!(
        diagnostics.iter().any(|q| q.contains("logic-compile")),
        "the {GRAPH_DIAGNOSTICS} graph must carry at least one logic-compile finding"
    );
}
