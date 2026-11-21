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
    let (reasoning, native_receipt) =
        project_reasoning_dataset(result).expect("project the native result");
    let dataset = rooted_in_graph(&reasoning, GRAPH_REASONING)
        .expect("route the complete reasoning projection");
    let artifacts = native_receipt
        .map(|receipt| {
            (
                crate::stages::reason::NATIVE_RECEIPT_PATH.to_owned(),
                receipt
                    .into_owned_bytes()
                    .expect("initial publication owns its bytes"),
            )
        })
        .into_iter()
        .collect();
    let mut bundle = bundle_from_artifacts_over(dataset, artifacts, DatasetProvenance::new());
    let pinned = bundle
        .graph_digest(
            GRAPH_REASONING,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .unwrap();
    bundle
        .pin_handle(
            GRAPH_REASONING,
            PipelineHandle::Reasoning(Arc::new(result.clone())),
            pinned,
            &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
        )
        .unwrap();
    StageProduct::from_bundle("stage-reason", Arc::new(bundle)).expect("the stage carrier digests")
}

/// The shipped coherence artifact commits to the flat canonical carrier before
/// the certificate joins it, including independently scoped blank nodes and
/// native statement metadata.
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

    let sources = [
        parse_dataset(edb.as_bytes(), "application/n-quads", None).expect("parse the EDB"),
        parse_into_graph(
            br#"_:member <urn:test:label> "member"@en .
                <urn:test:claim> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies>
                    <<( _:member <urn:test:label> "member"@en )>> ;
                    <urn:test:source> <urn:test:evidence> ."#,
            "text/turtle",
            "urn:test:attributed",
        )
        .expect("parse attributed RDF in its own scope"),
        parse_into_graph(
            b"_:member <urn:test:label> <urn:test:other> .",
            "text/turtle",
            "urn:test:neighbour",
        )
        .expect("parse the independently scoped blank with the same spelling"),
    ];
    let parts = || {
        let mut parts = CarrierParts::new(&upstream);
        for dataset in &sources {
            parts.push_dataset(Arc::clone(dataset));
        }
        parts
    };
    let canonical = purrdf::try_canonicalize_flat_view(
        &parts().view().expect("compose the pre-certificate carrier"),
        purrdf::CanonHash::Sha256,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .expect("render the certificate's flat canonical preimage");
    let expected = purrdf::gts::writer::digest_string(canonical.nquads().as_bytes());
    let folded = fold_coherence_certificate(parts(), &upstream).expect("fold certificate");

    let attestations = folded
        .project_graph(Some(&purrdf::RdfTerm::iri(
            crate::stages::release::GRAPH_ATTESTATIONS,
        )))
        .unwrap();
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let coherence_typed = attestations.owned_quads().any(|q| {
        q.predicate == rdf_type && matches!(&q.object, RdfTerm::Iri(o) if o.contains("Coherence"))
    });
    assert!(
        coherence_typed,
        "the fold must attach a typed logic:Coherence* artifact to graph/attestations"
    );
    let has_bundle_hash = attestations.owned_quads().any(|q| {
        q.predicate == "https://blackcatinformatics.ca/logic/bundleHash"
            && matches!(&q.object, RdfTerm::Literal(l) if l.lexical_form == expected)
    });
    assert!(
        has_bundle_hash,
        "the folded certificate pins the BLAKE3 digest of the flat canonical carrier"
    );

    // Deterministic: re-folding the same carrier + result is byte-identical.
    let folded2 = fold_coherence_certificate(parts(), &upstream).expect("fold again");
    let nq1 = purrdf::canonical_flat_nquads(
        folded.as_ref(),
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    let nq2 = purrdf::canonical_flat_nquads(
        folded2.as_ref(),
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    assert_eq!(
        nq1.nquads(),
        nq2.nquads(),
        "the folded certificate is deterministic"
    );

    let mut refused = parts();
    let reserved = format!("{}reserved", purrdf::RESERVED_NAMESPACE);
    let invalid = format!("<{reserved}> <urn:test:label> <urn:test:other> .");
    refused.push_dataset(
        parse_dataset(invalid.as_bytes(), "application/n-quads", None)
            .expect("the native parser admits the reserved IRI for the canonicalization control"),
    );
    let error = fold_coherence_certificate(refused, &upstream)
        .expect_err("the certificate must refuse reserved canonical vocabulary");
    assert!(error.message().contains(&reserved));
}
