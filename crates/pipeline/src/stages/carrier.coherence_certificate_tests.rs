// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use gmeow_logic::result_rdf::{GRAPH_REASONING, project_reasoning_dataset};
use purrdf::RdfTerm;
use std::sync::Arc;

/// Wrap a reasoned result as a `stage-reason` product carrying the typed Reasoning
/// handle pinned to `graph/reasoning` — the SAME shape `stage-reason` emits, which
/// `fold_coherence_certificate` reuses instead of reasoning a second time.
fn reason_product(result: &gmeow_logic::result::ReasoningResult) -> StageProduct {
    let reasoning = project_reasoning_dataset(result).expect("project the native result");
    let dataset = rooted_in_graph(&reasoning, GRAPH_REASONING)
        .expect("route the complete reasoning projection");
    let mut bundle = bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
    let pinned = bundle.graph_digest(GRAPH_REASONING);
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(result.clone())),
            pinned,
        )
        .unwrap();
    StageProduct::from_bundle("stage-reason", Arc::new(bundle))
}

/// `fold_coherence_certificate` folds a `graph/attestations` coherence artifact over the
/// composed carrier, REUSING `stage-reason`'s single reasoning pass (never re-reasoning),
/// so every terminal gmeow.gts carries the certificate the consumer read tool surfaces.
#[test]
fn fold_attaches_a_coherence_artifact_to_graph_attestations() {
    // A tiny consistent EDB → a real reasoned result (no forbidden violation).
    let edb = concat!(
        "<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> ",
        "<http://example.org/B> <http://gmeow.example/w> .\n"
    );
    let reasoned = crate::stages::reason::reason_artifacts(edb.as_bytes()).expect("reason");
    let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
    upstream.insert("stage-reason".to_string(), reason_product(&reasoned.result));

    let composed = parse_dataset(edb.as_bytes(), "application/n-quads", None).unwrap();
    let folded = fold_coherence_certificate(composed, &upstream).expect("fold certificate");

    let attestations = folded.project_named_graph(crate::stages::release::GRAPH_ATTESTATIONS);
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let coherence_typed = attestations.owned_quads().any(|q| {
        q.predicate == rdf_type && matches!(&q.object, RdfTerm::Iri(o) if o.contains("Coherence"))
    });
    assert!(
        coherence_typed,
        "the fold must attach a typed logic:Coherence* artifact to graph/attestations"
    );
    // The certificate pins a real bundle identity + per-graph axiom digest (the tamper
    // surface), so the read tool can surface non-trivial hashes.
    let has_bundle_hash = attestations.owned_quads().any(|q| {
        q.predicate == "https://blackcatinformatics.ca/logic/bundleHash"
            && matches!(&q.object, RdfTerm::Literal(l) if !l.lexical_form.is_empty())
    });
    assert!(has_bundle_hash, "the folded certificate pins a bundle hash");

    // Deterministic: re-folding the same carrier + result is byte-identical.
    let composed2 = parse_dataset(edb.as_bytes(), "application/n-quads", None).unwrap();
    let folded2 = fold_coherence_certificate(composed2, &upstream).expect("fold again");
    let nq1 = purrdf::canonical_flat_nquads(folded.as_ref()).unwrap();
    let nq2 = purrdf::canonical_flat_nquads(folded2.as_ref()).unwrap();
    assert_eq!(nq1, nq2, "the folded certificate is deterministic");
}
