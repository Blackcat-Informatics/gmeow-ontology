// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Issue 1672 (F2): substrate drift surfaces as a `gmeow:Finding` on the PRODUCTION
//! validate path.
//!
//! The substrate reconciliation A-Box rides `graph/provenance` in the source-load
//! product. `ValidateStage::run` folds it into the validated corpus so the derived
//! `PinAgreementConstraintProceduralConstraintShape` (targetClass `gmeow:PinClaim`) has
//! target data — a disagreeing A-Box (two `gmeow:PinClaim`s about the same component and
//! dimension carrying different values) then fires the constraint, and the violation
//! flows through the existing diagnostics fold into a `gmeow:Finding` carrying a finding
//! IRI + code-blind anchor. Before F2, `run` validated only the authored default graph,
//! so no substrate A-Box entered the SHACL target set and the constraint could never
//! fire — this test is RED without the fold and GREEN with it.
//!
//! Driven through the crate's PUBLIC surface: the REAL `ValidateStage::run`, the
//! producer-authenticated `procedural-constraints.ttl` shape union
//! member (which carries the derived pin shapes), and a source-load product whose
//! `graph/provenance` carries the disagreeing A-Box.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_pipeline::bundle::bundle_from_artifacts_over_with_rep_blob;
use gmeow_pipeline::bundle_blobs::REP_SPAN_TABLE;
use gmeow_pipeline::ingest::SpanIndex;
use gmeow_pipeline::node::{Stage, StageInput, StageProduct};
use gmeow_pipeline::stages::source_load::BASE_GRAPH_PATH;
use gmeow_pipeline::stages::validate::{SHACL_RDF_PATH, ValidateStage};
use purrdf::provenance::DatasetProvenance;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const GRAPH_PROVENANCE: &str = "https://blackcatinformatics.ca/gmeow/graph/provenance";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A benign authored shape half (mirrors the crate-internal `mock_repo`): the derived
/// pin shapes ride in through the REAL committed `procedural-constraints.ttl` fresh
/// member below, not through this file.
const BENIGN_SHAPES: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.test/> .
ex:NoopShape a sh:NodeShape ; sh:targetNode ex:nothing .
"#;

fn mock_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(&repo.path().join("shapes/gmeow-shapes.ttl"), BENIGN_SHAPES);
    write(
        &repo.path().join("generated/shapes/frame-shapes.ttl"),
        "# generated\n",
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    // The substrate A-Box rides in through graph/provenance (not re-derived from disk),
    // but `ValidateStage::run` folds the substrate build INPUTS into the recorded
    // `shaclInputDigest`. This repo simulates one carrying a substrate, so those input
    // files must exist for the digest fold (their bytes are folded, never parsed here).
    for rel in gmeow_pipeline::stages::substrate_graph::substrate_input_paths(repo.path()) {
        write(&rel, "# substrate input placeholder\n");
    }
    repo
}

/// A DISAGREEING substrate A-Box, in `graph/provenance` N-Quads: two `gmeow:PinClaim`s
/// about the SAME component (purrdf) and dimension (crate version) carrying DIFFERENT
/// claimed values. Every node IRI lies under `…/gmeow/substrate/…`, the aboutness key
/// `ValidateStage::run` filters `graph/provenance` on.
fn disagreeing_abox_nquads() -> Vec<u8> {
    let claim = |slug: &str, value: &str| {
        format!(
            "<{GMEOW}substrate/claim/purrdf-{slug}> \
                 <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <{GMEOW}PinClaim> <{GRAPH_PROVENANCE}> .\n\
             <{GMEOW}substrate/claim/purrdf-{slug}> \
                 <{GMEOW}claimedComponent> <{GMEOW}substrate/component/purrdf> <{GRAPH_PROVENANCE}> .\n\
             <{GMEOW}substrate/claim/purrdf-{slug}> \
                 <{GMEOW}claimDimension> <{GMEOW}dimensionCrateVersion> <{GRAPH_PROVENANCE}> .\n\
             <{GMEOW}substrate/claim/purrdf-{slug}> \
                 <{GMEOW}claimedValue> \"{value}\" <{GRAPH_PROVENANCE}> .\n"
        )
    };
    format!(
        "{}{}",
        claim("lockfile", "0.12.0"),
        claim("prose", "0.13.0")
    )
    .into_bytes()
}

/// Build the source-load product carrying the disagreeing A-Box in `graph/provenance`,
/// wire the four shape producers (the compile-logic pair supplies the REAL committed
/// `procedural-constraints.ttl` so the derived pin shapes join the union), and drive the
/// REAL `ValidateStage::run`. Returns the emitted `shacl.nq` diagnostics text.
fn run_and_capture_shacl_nq() -> String {
    use gmeow_pipeline::stages::{
        compile_logic::{PROCEDURAL_CONSTRAINTS_PATH, VALIDATION_SHAPES_TTL_PATH},
        constraint_shapes::CONSTRAINT_SHAPES_PATH,
        frame_shapes::FRAME_SHAPES_PATH,
        result_shapes::RESULT_SHAPES_PATH,
    };

    let repo = mock_repo();

    // The source-load product: an empty authored base graph, the disagreeing substrate
    // A-Box in graph/provenance, and the mandatory REP_SPAN_TABLE blob.
    let provenance = purrdf::parse_dataset(&disagreeing_abox_nquads(), "application/n-quads", None)
        .expect("disagreeing A-Box parses");
    let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    artifacts.insert(BASE_GRAPH_PATH.to_string(), Vec::new());
    let span_blob = serde_json::to_vec(&SpanIndex::new()).expect("encode span index");
    let bundle = bundle_from_artifacts_over_with_rep_blob(
        provenance,
        artifacts,
        DatasetProvenance::new(),
        REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-source-load".to_string(),
        StageProduct::from_bundle("stage-source-load", Arc::new(bundle)),
    );

    // The producer-selected procedural-constraints.ttl carries the derived pin shapes; the
    // other members ride header-only (they contribute no shape that targets the A-Box).
    let procedural = gmeow_pipeline::fixture::authenticated_artifact(
        &repo_root(),
        "stage-compile-logic",
        PROCEDURAL_CONSTRAINTS_PATH,
    )
    .expect("producer-selected procedural-constraints.ttl");
    type ProducerFixture = (&'static str, Vec<(&'static str, Vec<u8>)>);
    let producers: [ProducerFixture; 4] = [
        (
            "stage-compile-logic",
            vec![
                (VALIDATION_SHAPES_TTL_PATH, b"# generated\n".to_vec()),
                (PROCEDURAL_CONSTRAINTS_PATH, procedural),
            ],
        ),
        (
            "stage-export-constraint-shapes",
            vec![(CONSTRAINT_SHAPES_PATH, b"# generated\n".to_vec())],
        ),
        (
            "stage-export-frame-shapes",
            vec![(FRAME_SHAPES_PATH, b"# generated\n".to_vec())],
        ),
        (
            "stage-export-result-shapes",
            vec![(RESULT_SHAPES_PATH, b"# generated\n".to_vec())],
        ),
    ];
    for (producer, members) in producers {
        let members: BTreeMap<String, Vec<u8>> = members
            .into_iter()
            .map(|(rel, bytes)| (rel.to_string(), bytes))
            .collect();
        upstream.insert(
            producer.to_string(),
            StageProduct::from_artifacts(producer, members),
        );
    }

    // The D5 abductive tier consumes stage-reason's reasoned closure; an empty-EDB
    // fixture yields an empty closure (the reasoned union is the authored graph alone).
    upstream.insert(
        "stage-reason".to_string(),
        gmeow_pipeline::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
    );

    let output = ValidateStage::new()
        .run(StageInput {
            root: repo.path(),
            upstream: &upstream,
        })
        .expect("validate stage run over the disagreeing substrate A-Box");

    String::from_utf8(
        output
            .product
            .artifact(SHACL_RDF_PATH)
            .expect("shacl.nq artifact on the stage product")
            .to_vec(),
    )
    .expect("shacl.nq is UTF-8")
}

#[test]
fn disagreeing_substrate_abox_mints_a_pin_agreement_finding_on_the_production_path() {
    let shacl_nq = run_and_capture_shacl_nq();

    // The PinAgreement constraint fired: its authored message rides the finding.
    assert!(
        shacl_nq.contains("Substrate claim sites disagree"),
        "the substrate PinAgreement constraint must fire on a disagreeing A-Box through the \
         production validate path — no finding was minted:\n{shacl_nq}"
    );
    // The violation is a first-class gmeow:Finding with a blake3 finding IRI …
    assert!(
        shacl_nq.contains(&format!("{GMEOW}diagnostics/finding/")),
        "the drift must mint a gmeow:Finding carrying a finding IRI:\n{shacl_nq}"
    );
    // … and a code-blind anchor (the cross-node join key the root-cause meta-fold resolves
    // antecedents on — a raw SHACL violation carries no `findingAntecedent` of its own; the
    // anchor is the identity that lets one be attached downstream).
    assert!(
        shacl_nq.contains(&format!("{GMEOW}findingAnchor")),
        "the finding must carry a gmeow:findingAnchor:\n{shacl_nq}"
    );
    assert!(
        shacl_nq.contains(&format!("{GMEOW}diagnostics/anchor/")),
        "the anchor must be a code-blind diagnostics anchor IRI:\n{shacl_nq}"
    );
    // The finding is typed as a first-class gmeow:Finding on the shipped diagnostics graph.
    assert!(
        shacl_nq.contains(&format!("<{GMEOW}Finding>")),
        "the drift must be a first-class gmeow:Finding:\n{shacl_nq}"
    );
}
