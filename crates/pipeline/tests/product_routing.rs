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
    CANONICAL_RDF12_PATH, CompileLogicStage, DIAG_RDF_PATH, DIAG_SARIF_PATH,
    LOGIC_PROJECTIONS_CHANNEL, PROJECTION_REPORT_PATH,
};
use gmeow_pipeline::stages::mappings::MappingsStage;

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

    // The IR, the logic-projections channel, and the diagnostics SARIF/RDF are
    // committed/in-memory products of the compile-logic DAG node — not files written
    // only by a side CLI, and not values that live only inside a conformance fixture.
    for path in [
        CANONICAL_RDF12_PATH,      // canonical IR
        LOGIC_PROJECTIONS_CHANNEL, // logic projection rows handed to mappings
        DIAG_SARIF_PATH,           // diagnostics → SARIF
        DIAG_RDF_PATH,             // diagnostics → gmeow:Finding RDF
    ] {
        assert!(
            product.artifact(path).is_some(),
            "{path} must be a stage-compile-logic product (not fixture-only)"
        );
    }

    // The committed loss ledger is now assembled by stage-mappings over the UNION of the
    // logic projection rows (from compile-logic's channel) and the correspondence
    // ledger; compile-logic no longer emits the committed file itself.
    assert!(
        product.artifact(PROJECTION_REPORT_PATH).is_none(),
        "compile-logic must no longer emit the committed projection report"
    );
    let mut mappings_upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    mappings_upstream.insert("stage-compile-logic".to_string(), product);
    let constraint_shapes = gmeow_pipeline::stages::constraint_shapes::ConstraintShapesStage
        .run(StageInput {
            root: &root,
            upstream: &BTreeMap::new(),
        })
        .expect("constraint-shapes stage")
        .product;
    mappings_upstream.insert(
        "stage-export-constraint-shapes".to_string(),
        constraint_shapes,
    );
    let mappings = MappingsStage::new()
        .run(StageInput {
            root: &root,
            upstream: &mappings_upstream,
        })
        .expect("mappings stage")
        .product;

    // The loss ledger is a non-trivial RDF report (the ProjectionReport individual),
    // not an empty placeholder — a generated artifact of the mappings stage.
    let report = std::str::from_utf8(mappings.artifact(PROJECTION_REPORT_PATH).expect("report"))
        .expect("utf8 report");
    assert!(
        report.contains("ProjectionReport"),
        "the loss ledger must carry a logic:ProjectionReport"
    );
    assert!(
        report.contains("/target/sssom:") && report.contains("/target/edoal:"),
        "the loss ledger must carry the correspondence-calculus rows"
    );

    // The PROJECTION_REPORT_PATH lives under generated/, NOT only under a conformance
    // fixture tree — the literal "no product terminates in a test fixture" guard.
    assert!(
        PROJECTION_REPORT_PATH.starts_with("generated/"),
        "the loss ledger must be a generated artifact, not a fixture path"
    );
}

#[test]
fn loss_ledger_and_diagnostics_reach_the_shipped_bundle() {
    let root = repo_root();

    // Inspect the SHIPPED bundle directly — the committed gmeow.gts a repo-free
    // consumer reads. `tests/full_parity.rs` guarantees it matches a fresh build, so
    // reading it (rather than rebuilding the whole snapshot here) is both faithful and
    // fast. This is the literal "the product reaches the bundle" assertion.
    let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).expect("read gmeow.gts");
    let bundle = purrdf::import_gts_events(&gts).expect("import_gts_events");
    let quads = purrdf::flat_rdf_quads_from_dataset(bundle.dataset.as_ref());

    let in_graph = |graph: &str| -> Vec<String> {
        quads
            .iter()
            .filter(|q| {
                q.graph_name
                    .as_ref()
                    .is_some_and(|g| g.to_string().contains(graph))
            })
            .map(|q| format!("{} <{}> {}", q.subject, q.predicate, q.object))
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
