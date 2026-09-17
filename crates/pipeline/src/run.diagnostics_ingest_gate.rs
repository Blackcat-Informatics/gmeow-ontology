// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::PIPELINE_STAGE_ID;
use crate::node::{Stage, StageInput, StageProduct};

/// The real diagnostics producer the forward run ledger attributes nodes to.
const DIAG_PRODUCER_VALIDATE: &str = "stage-validate";
use crate::stages::source_load::BASE_GRAPH_PATH;
use crate::stages::validate::ValidateStage;
use gmeow_errors::DiagLedger;
use std::collections::BTreeMap;
use std::path::Path;

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).unwrap();
    std::fs::write(path, content).unwrap();
}

/// A fixture repo carrying one SHACL shape that requires `ex:required` on the
/// target node `ex:thing` — its source omits that property and violates minCount.
fn violating_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("shapes/gmeow-shapes.ttl"),
        r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 1 ;
        sh:message "required value is missing" ;
    ] .
"#,
    );
    write(
        &repo.path().join("generated/shapes/frame-shapes.ttl"),
        "# generated\n",
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    repo
}

/// The repo-relative source path the fixture span index attributes `ex:thing` to.
const FIXTURE_SPAN_PATH: &str = "slices/x/module.ttl";

/// A `stage-source-load` product carrying a native base graph AND a source-span
/// table mapping the SHACL focus subject `ex:thing` to a source position — the same
/// blob lane the real source-load stage attaches, so the validate stage's
/// `span_index()` read (and its finding enrichment) is exercised end-to-end.
fn source_load_product_with_spans() -> StageProduct {
    use std::sync::Arc;
    let mut source_artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let source = b"<https://example.test/thing> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Thing> .\n";
    source_artifacts.insert(BASE_GRAPH_PATH.to_owned(), source.to_vec());
    let mut spans = crate::ingest::SpanIndex::new();
    spans.insert(
        "https://example.test/thing",
        crate::ingest::SourceSpan::new(Arc::from(FIXTURE_SPAN_PATH), 12, 3, 200),
    );
    let span_blob = serde_json::to_vec(&spans).expect("encode span index");
    let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
        purrdf::parse_dataset(source, "application/n-quads", None).expect("native source fixture"),
        source_artifacts,
        purrdf::provenance::DatasetProvenance::new(),
        crate::stages::carrier::REP_SPAN_TABLE,
        "application/json",
        span_blob,
    );
    StageProduct::from_bundle("stage-source-load", Arc::new(bundle))
}

/// The four generated-shape producer products the fresh union hard-requires
/// (`shape_union_fresh::fresh_generated_shape_members`): each carries its
/// `generated/shapes/*.ttl` member as a comment-only Turtle byte product, the
/// same lane the real producers attach.
fn insert_generated_shape_producers(upstream: &mut BTreeMap<String, StageProduct>) {
    let product = |stage: &str, rels: &[&str]| {
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for rel in rels {
            artifacts.insert((*rel).to_string(), b"# generated\n".to_vec());
        }
        StageProduct::from_artifacts(stage, artifacts)
    };
    upstream.insert(
        "stage-compile-logic".to_owned(),
        product(
            "stage-compile-logic",
            &[
                crate::stages::compile_logic::VALIDATION_SHAPES_TTL_PATH,
                crate::stages::compile_logic::PROCEDURAL_CONSTRAINTS_PATH,
            ],
        ),
    );
    upstream.insert(
        "stage-export-constraint-shapes".to_owned(),
        product(
            "stage-export-constraint-shapes",
            &[crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH],
        ),
    );
    upstream.insert(
        "stage-export-frame-shapes".to_owned(),
        product(
            "stage-export-frame-shapes",
            &[crate::stages::frame_shapes::FRAME_SHAPES_PATH],
        ),
    );
    upstream.insert(
        "stage-export-result-shapes".to_owned(),
        product(
            "stage-export-result-shapes",
            &[crate::stages::result_shapes::RESULT_SHAPES_PATH],
        ),
    );
    // The validate stage's D5 abductive tier consumes stage-reason's reasoned closure;
    // an empty-EDB reason product yields an empty closure, so the reasoned union is the
    // authored source graph alone (this harness drives SHACL/enrichment, not entailment).
    upstream.insert(
        "stage-reason".to_owned(),
        crate::stages::reason::reason_product(b"").expect("stage-reason fixture product"),
    );
}

/// Run the real `stage-validate` stage over the violating fixture, returning its
/// full output (product + forward diags).
fn run_validate(repo: &Path) -> crate::node::StageOutput {
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert(
        "stage-source-load".to_owned(),
        source_load_product_with_spans(),
    );
    insert_generated_shape_producers(&mut upstream);
    ValidateStage::new()
        .run(StageInput {
            root: repo,
            upstream: &upstream,
        })
        .expect("validate stage")
}

/// The source span attached by `source_load_product_with_spans` is LIFTED onto the
/// real SHACL finding's focus-node location (path + line) and rides into the forward
/// DiagNode — the proof the span table is genuinely consumed (non-dark).
#[test]
fn source_span_is_lifted_onto_the_real_shacl_finding_and_diag_node() {
    let repo = violating_repo();
    let out = run_validate(repo.path());

    // The shipped SHACL JSON projection carries the enriched focus location.
    let json = out
        .product
        .artifact(crate::stages::validate::SHACL_JSON_PATH)
        .expect("shacl.json artifact");
    let report: serde_json::Value = serde_json::from_slice(json).expect("shacl json");
    let location = report["findings"]
        .as_array()
        .and_then(|fs| {
            fs.iter()
                .find(|f| f["code"].as_str().unwrap_or("").starts_with("shacl."))
        })
        .and_then(|f| f["locations"].as_array())
        .and_then(|ls| ls.first())
        .expect("a SHACL finding with a location");
    assert_eq!(
        location["path"].as_str(),
        Some(FIXTURE_SPAN_PATH),
        "the finding's focus location carries the lifted source path"
    );
    assert_eq!(
        location["line"].as_u64(),
        Some(12),
        "the finding's focus location carries the lifted 1-based line"
    );
    assert_eq!(
        location["logical"].as_str(),
        Some("https://example.test/thing"),
        "the bare-IRI focus join key is preserved"
    );

    // The forward DiagNode carries the same source path (the RDF projection is path +
    // GTS coords only, so line is intentionally lossy on the node — the path is what
    // travels into the run ledger).
    let node = out
        .diags
        .iter()
        .find(|n| n.code.starts_with("shacl."))
        .expect("a forward SHACL DiagNode");
    assert_eq!(
        node.source_ctx.location.path.as_deref(),
        Some(FIXTURE_SPAN_PATH),
        "the lifted source path travels into the forward DiagNode"
    );
}

/// Drop-after-last-consumer HARD FAIL: once the span blob is stripped from the
/// source-load product (as the scheduler does after the last consumer level), any
/// later `span_index()` read is a typed `SpanTableConsumedAfterDrop` error — and the
/// stripped product no longer carries the span blob (so it cannot ship).
#[test]
fn span_index_hard_fails_after_the_drop_and_is_not_shipped() {
    let product = source_load_product_with_spans();
    // Before the drop the accessor resolves the table.
    assert!(
        product.span_index().is_ok(),
        "span table present before drop"
    );

    // Strip exactly as the scheduler does at the drop point.
    let stripped =
        crate::bundle::strip_rep_blob(product.bundle(), crate::stages::carrier::REP_SPAN_TABLE)
            .expect("strip span blob");
    let dropped = product.with_released_bundle(stripped);
    assert_eq!(
        dropped.digest, product.digest,
        "published identity survives payload release"
    );

    // Not shipped: the stripped product carries no span blob for the sink to fold.
    assert!(
        crate::bundle::bundle_rep_blob(dropped.bundle(), crate::stages::carrier::REP_SPAN_TABLE)
            .is_none(),
        "the stripped product must not carry the span blob"
    );

    // Reachable HARD FAIL: a later read is the typed SpanTableConsumedAfterDrop.
    let err = dropped
        .span_index()
        .expect_err("span_index must hard-fail after drop");
    assert!(
        err.downcast_ref::<crate::error::SpanTableConsumedAfterDrop>()
            .is_some(),
        "the drop hard-fail must be a typed SpanTableConsumedAfterDrop, got: {err}"
    );
}

#[test]
fn shacl_diagnostic_reaches_the_run_ledger_attributed_to_stage_validate() {
    let repo = violating_repo();
    // Run the REAL validate stage; its FORWARD diags are the single source the
    // scheduler replays into the run ledger.
    let out = run_validate(repo.path());
    assert!(
        !out.diags.is_empty(),
        "the violating fixture must produce forward diagnostic nodes"
    );

    // Fold the forward nodes exactly as the scheduler's commit phase does.
    let mut ledger = DiagLedger::new();
    ledger.replay(out.diags.clone());

    // The ledger node for the SHACL violation is attributed to the REAL producing
    // stage, never the synthetic reconcile stage.
    let nodes = ledger.emit_sorted();
    let shacl = nodes
        .iter()
        .find(|n| n.code.starts_with("shacl."))
        .expect("SHACL diagnostic folded into the run ledger");
    assert_eq!(
        shacl.stage.as_str(),
        DIAG_PRODUCER_VALIDATE,
        "the SHACL diagnostic must be attributed to the real producing stage"
    );
    assert_ne!(
        shacl.stage.as_str(),
        PIPELINE_STAGE_ID,
        "the SHACL diagnostic must NOT be attributed to the synthetic reconcile stage"
    );

    // It projects into RunReport.findings (the ledger is the single source).
    let findings = ledger.findings("gmeow-pipeline");
    assert!(
        findings.iter().any(|f| f.code.starts_with("shacl.")),
        "the forward SHACL diagnostic must project into the wire findings"
    );
}

/// The product's `diagnostics:nodes` blob carries EXACTLY the forward `diags` the
/// stage emitted — the cache lane (which round-trips this blob) recovers the same
/// run-ledger contribution byte-for-byte on a cache hit.
#[test]
fn product_diag_nodes_blob_equals_the_emitted_diags() {
    let repo = violating_repo();
    let out = run_validate(repo.path());
    let from_blob = out.product.diag_nodes();
    assert!(
        !from_blob.is_empty(),
        "the diagnostics:nodes blob must be non-empty"
    );
    assert_eq!(
        from_blob, out.diags,
        "the product blob must byte-equal the emitted forward diags"
    );
}

/// Cache-replay byte-identity over a REAL non-empty product: persisting the validate
/// product to the per-stage cache and re-reading it recovers a `diagnostics:nodes`
/// blob whose folded run-ledger `emit_sorted()` is BYTE-IDENTICAL to the fresh run
/// (guarding against a vacuous `[]` blob).
#[test]
fn cache_replay_yields_byte_identical_run_ledger() {
    use crate::cache::{PipelineCache, ReceiptOutputSelection, StageKeyContext};

    let repo = violating_repo();
    let out = run_validate(repo.path());
    assert!(
        !out.diags.is_empty(),
        "the fixture must yield a non-empty node set"
    );

    // Fresh-run ledger bytes.
    let fresh = {
        let mut ledger = DiagLedger::new();
        ledger.replay(out.diags.clone());
        serde_json::to_vec(
            &ledger
                .emit_sorted()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };

    // Persist + re-read the product through the real per-stage cache.
    let dir = tempfile::tempdir().unwrap();
    let cache = PipelineCache::open(dir.path()).unwrap();
    let context = StageKeyContext::new("stage-validate", "test-v1", Vec::new(), Vec::new());
    let selection = ReceiptOutputSelection {
        graphs: out
            .product
            .dataset()
            .owned_named_graphs()
            .filter_map(|term| match term {
                purrdf::RdfTerm::Iri(iri) => Some(iri),
                _ => None,
            })
            .collect(),
        blob_representations: out
            .product
            .bundle()
            .lookaside()
            .blobs
            .iter()
            .filter_map(|blob| blob.representation.clone())
            .collect(),
        logical_artifacts: out
            .product
            .bundle()
            .lookaside()
            .resources
            .iter()
            .filter_map(|resource| resource.name.clone())
            .collect(),
        handles: out.product.bundle().handles().keys().cloned().collect(),
        default_graph: crate::cache::default_graph_commitment(&out.product).unwrap(),
        provenance: crate::cache::provenance_commitment(&out.product).unwrap(),
        content_store: crate::cache::content_store_commitment(&out.product).unwrap(),
    };
    cache
        .put(&context, "stable", "persistent", &selection, &out.product)
        .unwrap();
    let restored = cache.get(&context).unwrap().expect("cache hit");
    let restored_nodes = restored.product.diag_nodes();
    assert!(
        !restored_nodes.is_empty(),
        "the cache-restored product must carry a NON-empty diagnostics:nodes blob"
    );

    // Warm-cache ledger bytes.
    let warm = {
        let mut ledger = DiagLedger::new();
        ledger.replay(restored_nodes);
        serde_json::to_vec(
            &ledger
                .emit_sorted()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .unwrap()
    };
    assert_eq!(
        fresh, warm,
        "a warm-cache run's ledger must be byte-identical to the fresh run"
    );
}

/// A fixture repo whose one shape CONFORMS over the empty source graph (minCount 0),
/// so `stage-validate` produces only the informational `shacl.clean` record.
fn conforming_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().unwrap();
    write(
        &repo.path().join("shapes/gmeow-shapes.ttl"),
        r#"
@prefix ex: <https://example.test/> .
@prefix sh: <http://www.w3.org/ns/shacl#> .

ex:RequiredShape a sh:NodeShape ;
    sh:targetNode ex:thing ;
    sh:property [
        sh:path ex:required ;
        sh:minCount 0 ;
    ] .
"#,
    );
    write(
        &repo.path().join("generated/shapes/frame-shapes.ttl"),
        "# generated\n",
    );
    std::fs::create_dir_all(repo.path().join("slices")).unwrap();
    repo
}

/// A2 golden-delta over the REAL producer: adding/removing a triggering SHACL
/// violation changes BOTH the shipped `generated/diagnostics/shacl.nq` artifact AND
/// the run-ledger node set — proving the two are bound to the same producer findings.
#[test]
fn violation_delta_changes_both_shacl_rdf_and_run_ledger_nodes() {
    use crate::stages::validate::SHACL_RDF_PATH;

    let violating = run_validate(violating_repo().path());
    let conforming = run_validate(conforming_repo().path());

    // The committed `shacl.nq` artifact differs (a real surface delta).
    assert_ne!(
        violating.product.artifact(SHACL_RDF_PATH),
        conforming.product.artifact(SHACL_RDF_PATH),
        "a triggered SHACL violation must change the shacl.nq artifact"
    );
    // And the run-ledger node set differs: the violation contributes real finding
    // nodes; the conforming run contributes exactly the informational `shacl.clean`
    // record (keeps stage-validate's graph/diagnostics attach delta stable on a
    // clean corpus — a zero-findings validation is a report, not an absence). With
    // the fixed advisory demonstrator removed (greenfield), this candidate-free
    // conforming corpus harvests NO advisory (no accepted logic:CategoryRecommendation
    // candidate is authored in it), so the clean run is exactly the one shacl.clean
    // node. (Harvested advisories surfacing on a candidate-bearing source is proven
    // by the stage test `stage_validate_emits_both_advice_projections`.)
    assert!(
        !violating.diags.is_empty(),
        "the violating run must contribute run-ledger nodes"
    );
    assert_eq!(
        conforming.diags.len(),
        1,
        "the conforming candidate-free run contributes exactly the shacl.clean record \
             (no harvested advisory): {:?}",
        conforming.diags
    );
    assert_eq!(
        conforming.diags[0].grade.severity,
        gmeow_errors::Severity::Info,
        "the conforming run's only node is the informational shacl.clean record"
    );
    assert_ne!(
        violating.diags, conforming.diags,
        "the run-ledger node set must differ with the violation"
    );
}
