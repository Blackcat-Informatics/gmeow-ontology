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
    let expected = purrdf::canonical_flat_nquads(
        old_union.as_ref(),
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .expect("old flat union canonicalizes");

    let actual = stratum_nquads(first.as_ref(), &[second]).expect("streamed flat union");
    assert_eq!(actual, expected.nquads());
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
    let expected = purrdf::canonical_flat_nquads(
        &first,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    assert_eq!(
        first_digest,
        crate::medium::blake3_digest(expected.nquads().as_bytes())
    );
    assert_eq!(first_len, expected.nquads().len());

    // Distinct source documents may share blank labels, and each claim's
    // world remains part of the exact union checked by the reader.
    let source_quads = [
        purrdf::flat_rdf_quads_from_dataset(&first),
        purrdf::flat_rdf_quads_from_dataset(&second),
    ];
    let union =
        purrdf::flat_dataset_from_quad_sources(&[&source_quads[0], &source_quads[1]]).unwrap();
    let expected = purrdf::canonical_flat_nquads(
        &union,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )
    .unwrap();
    let (digest, len) = snapshot_stratum_digest(&first, &[second]).unwrap();
    assert_eq!(
        digest,
        crate::medium::blake3_digest(expected.nquads().as_bytes())
    );
    assert_eq!(len, expected.nquads().len());
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
        )
        .expect("the stage carrier digests"),
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
    let wire = format!(
        r#"{{"schema_version":1,"algorithm":"gts-pass-one-canonical-identities-v1","input_digest":"{expected_input}","snapshot_content_digest":"{expected_content}","stratum_digest":"{expected_stratum}"}}"#
    );
    let encoded = purrdf_lex::json::record::to_vec(&receipt);
    assert_eq!(encoded, wire.as_bytes());
    let with_receipt = |bytes| {
        BTreeMap::from([(
            "stage-gts-sink".to_string(),
            StageProduct::from_artifacts(
                "stage-gts-sink",
                BTreeMap::from([(PASS_ONE_RECEIPT_PATH.to_string(), bytes)]),
            )
            .expect("the stage carrier digests"),
        )])
    };
    let mut upstream = with_receipt(encoded);

    assert_eq!(
        reusable_pass_one_receipt(&upstream, &expected_input)
            .expect("matching receipt")
            .expect("receipt is present"),
        receipt
    );
    let unknown = wire.replacen('{', "{\"future\":null,", 1);
    assert_eq!(
        reusable_pass_one_receipt(&with_receipt(unknown.into_bytes()), &expected_input)
            .unwrap()
            .unwrap(),
        receipt
    );
    assert!(
        reusable_pass_one_receipt(&BTreeMap::new(), &expected_input)
            .unwrap()
            .is_none()
    );

    for invalid in [
        wire.replace(
            "\"algorithm\":\"gts-pass-one-canonical-identities-v1\",",
            "",
        ),
        wire.replace(
            "\"schema_version\":1",
            "\"schema_version\":1,\"schema_version\":1",
        ),
        wire.replace(
            &format!("\"stratum_digest\":\"{expected_stratum}\""),
            "\"stratum_digest\":null",
        ),
        format!(
            r#"[1,"gts-pass-one-canonical-identities-v1","{expected_input}","{expected_content}","{expected_stratum}"]"#
        ),
        "{".into(),
    ] {
        assert!(
            reusable_pass_one_receipt(&with_receipt(invalid.into_bytes()), &expected_input)
                .is_err()
        );
    }
    for token in ["-0", "-1", "1.0", "1e0", "4294967296", "null", "\"1\""] {
        let invalid = wire.replace(
            "\"schema_version\":1",
            &format!("\"schema_version\":{token}"),
        );
        let error = reusable_pass_one_receipt(&with_receipt(invalid.into_bytes()), &expected_input)
            .unwrap_err()
            .to_string();
        assert!(error.contains("/schema_version"), "{error}");
    }
    let widest = wire.replace("\"schema_version\":1", "\"schema_version\":4294967295");
    assert_eq!(
        purrdf_lex::json::record::from_slice::<PassOneReceipt>(widest.as_bytes())
            .unwrap()
            .schema_version,
        u32::MAX
    );
    for (invalid, context) in [
        (widest, "schema version"),
        (
            wire.replace(PASS_ONE_RECEIPT_ALGORITHM, "wrong-algorithm"),
            "algorithm",
        ),
        (
            wire.replace(&expected_content, "blake3:UPPER"),
            "snapshot content digest",
        ),
        (
            wire.replace(&expected_stratum, "blake3:UPPER"),
            "stratum digest",
        ),
    ] {
        let error = reusable_pass_one_receipt(&with_receipt(invalid.into_bytes()), &expected_input)
            .unwrap_err()
            .to_string();
        assert!(error.contains(context), "{error}");
    }
    assert!(
        reusable_pass_one_receipt(&upstream, &crate::medium::blake3_digest(b"changed")).is_err(),
        "a stale same-stage receipt must hard-fail"
    );

    upstream.insert(
        "stage-gts-sink".to_string(),
        StageProduct::from_artifacts("stage-gts-sink", BTreeMap::new())
            .expect("the stage carrier digests"),
    );
    assert!(
        reusable_pass_one_receipt(&upstream, &expected_input).is_err(),
        "a completed terminal without its required receipt must hard-fail"
    );
}
