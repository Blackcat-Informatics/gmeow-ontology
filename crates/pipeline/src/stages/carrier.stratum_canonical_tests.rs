// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn fixture_dataset(object: &str) -> std::sync::Arc<purrdf::RdfDataset> {
    parse_dataset(
        format!("<https://example.org/s> <https://example.org/p> <https://example.org/{object}> .")
            .as_bytes(),
        "text/turtle",
        None,
    )
    .expect("fixture dataset")
}

#[test]
fn streamed_flat_union_matches_the_prior_flat_canonical_path() {
    let first = parse_dataset(
        br#"@prefix ex: <https://example.org/> .
                @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
                _:same ex:p ex:o .
                ex:r rdf:reifies <<( _:same ex:q "value" )>> .
                ex:r ex:confidence "1" ."#,
        "text/turtle",
        None,
    )
    .expect("first RDF 1.2 source");
    let second = parse_dataset(
        br#"@prefix ex: <https://example.org/> .
                _:same ex:p ex:other ."#,
        "text/turtle",
        None,
    )
    .expect("second independently parsed source");

    let old_sources = [
        purrdf::flat_rdf_quads_from_dataset(first.as_ref()),
        purrdf::flat_rdf_quads_from_dataset(second.as_ref()),
    ];
    let old_refs: Vec<&[RdfQuad]> = old_sources.iter().map(Vec::as_slice).collect();
    let old_union = purrdf::flat_dataset_from_quad_sources(&old_refs).expect("old flat union");
    let expected =
        purrdf::canonical_flat_nquads(old_union.as_ref()).expect("old flat union canonicalizes");

    let actual = stratum_nquads(first.as_ref(), &[second]).expect("streamed flat union");
    assert_eq!(actual, expected);
}

#[test]
fn medium_stratum_identity_binds_the_world_of_attributed_claims() {
    let source = |world| {
        parse_dataset(
            format!(
                r#"@prefix ex: <https://example.org/> .
                       @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
                       ex:{world} {{
                           ex:claim rdf:reifies <<( _:item ex:name "اسم"@ar--rtl )>> ;
                               ex:accordingTo ex:observer .
                       }}"#
            )
            .as_bytes(),
            "application/trig",
            None,
        )
        .unwrap()
    };
    let first = source("reported");
    let second = source("hypothetical");
    // The medium envelope binds the complete attributed stratum. Moving
    // the same claim to another world must change its digest even though
    // neither source contains an ordinary assertion-table quad.
    assert_eq!(first.quad_count(), 0);
    let (first_digest, first_len) = snapshot_stratum_digest(&first, &[]).unwrap();
    let (second_digest, _) = snapshot_stratum_digest(&second, &[]).unwrap();
    assert_ne!(first_digest, second_digest);
    let expected = purrdf::canonical_flat_nquads(&first).unwrap();
    assert_eq!(
        first_digest,
        crate::medium::blake3_digest(expected.as_bytes())
    );
    assert_eq!(first_len, expected.len());

    // Distinct source documents may share blank labels, and each claim's
    // world remains part of the exact union checked by the reader.
    let source_quads = [
        purrdf::flat_rdf_quads_from_dataset(&first),
        purrdf::flat_rdf_quads_from_dataset(&second),
    ];
    let union =
        purrdf::flat_dataset_from_quad_sources(&[&source_quads[0], &source_quads[1]]).unwrap();
    let expected = purrdf::canonical_flat_nquads(&union).unwrap();
    let (digest, len) = snapshot_stratum_digest(&first, &[second]).unwrap();
    assert_eq!(digest, crate::medium::blake3_digest(expected.as_bytes()));
    assert_eq!(len, expected.len());
}

#[test]
fn pass_one_receipt_key_binds_the_exact_snapshot_and_each_extra_graph() {
    let carrier = fixture_dataset("carrier");
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-snapshot".to_string(),
        StageProduct::from_artifacts_over(
            "stage-snapshot",
            std::sync::Arc::clone(&carrier),
            BTreeMap::new(),
        ),
    );
    let first_graph = fixture_dataset("first");
    let second_graph = fixture_dataset("second");

    let key = pass_one_receipt_input_digest(
        &upstream,
        carrier.as_ref(),
        &[std::sync::Arc::clone(&first_graph)],
    )
    .expect("receipt key");
    assert_eq!(
        key,
        pass_one_receipt_input_digest(&upstream, carrier.as_ref(), &[first_graph])
            .expect("stable receipt key")
    );
    assert_ne!(
        key,
        pass_one_receipt_input_digest(&upstream, carrier.as_ref(), &[second_graph])
            .expect("changed receipt key"),
        "an auxiliary stratum-graph change must invalidate the receipt"
    );

    let unrelated = fixture_dataset("unrelated");
    assert!(
        pass_one_receipt_input_digest(&upstream, unrelated.as_ref(), &[]).is_err(),
        "a lookalike caller cannot key evidence for a dataset other than the exact snapshot"
    );
}

#[test]
fn present_pass_one_receipt_is_typed_and_fail_closed() {
    let expected_input = crate::medium::blake3_digest(b"input");
    let expected_content = crate::medium::blake3_digest(b"content");
    let expected_stratum = crate::medium::blake3_digest(b"stratum");
    let receipt = PassOneReceipt {
        schema_version: PASS_ONE_RECEIPT_SCHEMA_VERSION,
        algorithm: PASS_ONE_RECEIPT_ALGORITHM.to_string(),
        input_digest: expected_input.clone(),
        snapshot_content_digest: expected_content.clone(),
        stratum_digest: expected_stratum.clone(),
    };
    let mut artifacts = BTreeMap::new();
    artifacts.insert(
        PASS_ONE_RECEIPT_PATH.to_string(),
        serde_json::to_vec(&receipt).expect("receipt JSON"),
    );
    let mut upstream = BTreeMap::new();
    upstream.insert(
        "stage-gts-sink".to_string(),
        StageProduct::from_artifacts("stage-gts-sink", artifacts),
    );

    assert_eq!(
        reusable_pass_one_receipt(&upstream, &expected_input)
            .expect("matching receipt")
            .expect("receipt is present")
            .snapshot_content_digest,
        expected_content
    );
    assert!(
        reusable_pass_one_receipt(&upstream, &crate::medium::blake3_digest(b"changed")).is_err(),
        "a stale same-stage receipt must hard-fail"
    );

    upstream.insert(
        "stage-gts-sink".to_string(),
        StageProduct::from_artifacts("stage-gts-sink", BTreeMap::new()),
    );
    assert!(
        reusable_pass_one_receipt(&upstream, &expected_input).is_err(),
        "a completed terminal without its required receipt must hard-fail"
    );
}
