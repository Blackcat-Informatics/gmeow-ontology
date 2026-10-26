// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual action-codec authentication of complete, explicitly synthetic reports.

use super::*;
use crate::bundle::{
    DiagnosticReportOwner, DiagnosticsPublication, diagnostic_test_support,
    diagnostics_from_product,
};
use crate::stages::carrier::GRAPH_DIAGNOSTICS;

/// Select every output of the tiny real diagnostic producer, including its
/// payload commitment and terminal artifacts, through the normal receipt API.
fn selection(product: &StageProduct) -> ReceiptOutputSelection {
    ReceiptOutputSelection {
        graphs: vec![GRAPH_DIAGNOSTICS.into()],
        handles: vec![GRAPH_DIAGNOSTICS.into()],
        logical_artifacts: product
            .artifact_refs()
            .unwrap()
            .keys()
            .map(|path| (*path).to_owned())
            .collect(),
        default_graph: default_graph_commitment(product).unwrap(),
        provenance: provenance_commitment(product).unwrap(),
        content_store: content_store_commitment(product).unwrap(),
        blob_representations: Vec::new(),
    }
}

/// The actual CBOR arm and persistent put/get preserve the entire rich report,
/// source owner, product receipt and accounted serialized payload bytes.
#[test]
fn diagnostics_codec_roundtrip_preserves_complete_reports_and_accounting() {
    // gmeow-test-input: synthetic-only
    let mut products = BTreeMap::new();
    for owner in [
        DiagnosticReportOwner::CompileLogic,
        DiagnosticReportOwner::Validate,
    ] {
        let product =
            diagnostic_test_support::product(owner, diagnostic_test_support::rich_report(owner));
        products.insert(product.stage_id.clone(), product);
    }
    let snapshot = diagnostic_test_support::snapshot_product(&products);
    products.insert(snapshot.stage_id.clone(), snapshot);
    for product in products.values() {
        let handle = &product.bundle().handle(GRAPH_DIAGNOSTICS).unwrap().payload;
        let encoded = encode_handle(handle).unwrap();
        let decoded = rebuild_handle("diagnostics", Some(&encoded)).unwrap();
        assert_eq!(
            handle_payload_digest(&decoded),
            handle_payload_digest(handle)
        );
        let expected = diagnostics_from_product(product, &product.stage_id).unwrap();
        let PipelineHandle::Diagnostics(decoded) = decoded else {
            panic!("complete Diagnostics codec arm")
        };
        assert_eq!(decoded.reports, expected.reports);

        let dir = tempfile::tempdir().unwrap();
        let cache = PipelineCache::open(dir.path()).unwrap();
        let context = StageKeyContext::new(
            &product.stage_id,
            "diagnostic-codec-control",
            vec![],
            vec![],
        );
        let selected = selection(product);
        let receipt = cache
            .put(&context, "stable", "persistent", &selected, product)
            .unwrap();
        let manifest = CachedBundle::from_product(product, &selected).unwrap();
        assert_eq!(
            receipt.product_blob_bytes,
            bincode::serialize(&manifest).unwrap().len() as u64
        );
        assert!(receipt.product_blob_bytes >= encoded.len() as u64);
        let restored = cache.get(&context).unwrap().unwrap();
        assert_eq!(restored.receipt, receipt);
        assert_eq!(restored.hydrated_bytes, receipt.product_blob_bytes);
        assert_eq!(restored.product.digest, product.digest);
        assert_eq!(restored.product.artifacts(), product.artifacts());
        let actual = diagnostics_from_product(&restored.product, &product.stage_id).unwrap();
        assert_eq!(actual.reports, expected.reports);
    }
}

/// Metadata absent from RDF still belongs to both immutable native and complete
/// product commitments. Re-encoding a changed report cannot authenticate it.
#[test]
fn diagnostics_codec_rejects_report_only_evidence_tampering() {
    // gmeow-test-input: synthetic-only
    for mutation in 0..5 {
        let owner = DiagnosticReportOwner::Validate;
        let product =
            diagnostic_test_support::product(owner, diagnostic_test_support::rich_report(owner));
        let mut cached = CachedBundle::from_product(&product, &selection(&product)).unwrap();
        let mut publication: DiagnosticsPublication =
            ciborium::de::from_reader(cached.handles[0].typed_payload.as_ref().unwrap().as_slice())
                .unwrap();
        let report = Arc::make_mut(publication.reports.get_mut(&owner).unwrap());
        match mutation {
            0 => {
                report.metadata.insert(
                    "nested-evidence".into(),
                    purrdf_lex::json::record::FromJson::from_json(
                        &purrdf_lex::json::record::read_document(br#"{"source":"substituted"}"#)
                            .unwrap(),
                    )
                    .unwrap(),
                );
            }
            1 => {
                report.findings[1].locations[0].logical =
                    Some("https://example.org/substituted".into())
            }
            2 => report.findings[1].standpoint = Some(gmeow_errors::Standpoint::Advisory),
            3 => {
                report.findings[1].attributions[0].evidence =
                    Some("substituted source owner evidence".into())
            }
            _ => report.findings[1].related_labels[0].message = "substituted source label".into(),
        }
        let changed = PipelineHandle::Diagnostics(Arc::new(publication));
        cached.handles[0].typed_payload = Some(encode_handle(&changed).unwrap());
        let stale: CachedBundle =
            bincode::deserialize(&bincode::serialize(&cached).unwrap()).unwrap();
        assert!(
            stale
                .into_product()
                .unwrap_err()
                .is::<crate::error::CacheMismatch>(),
            "mutation {mutation}"
        );
        cached.handles[0].payload_digest = handle_payload_digest(&changed);
        assert!(
            cached
                .into_product()
                .unwrap_err()
                .is::<crate::error::CacheMismatch>(),
            "the old complete product rejects mutation {mutation} even after payload rehash"
        );
    }
}

/// Binary metadata restores exact lexical evidence and refuses malformed or
/// noncanonical strings, even when the surrounding CBOR publication is valid.
#[test]
fn diagnostics_codec_requires_canonical_json_evidence_on_hydration() {
    // gmeow-test-input: synthetic-only
    use purrdf_lex::cbor::{Value, encode};
    let publication = |lexical: &str| {
        encode(&Value::Map(vec![(
            Value::Text("reports".into()),
            Value::Map(vec![(
                Value::Text("Validate".into()),
                Value::Map(vec![
                    (Value::Text("tool".into()), Value::Text("shacl".into())),
                    (
                        Value::Text("metadata".into()),
                        Value::Map(vec![(
                            Value::Text("evidence".into()),
                            Value::Text(lexical.into()),
                        )]),
                    ),
                ]),
            )]),
        )]))
    };
    let canonical = r#"{"a":[1e400,-0,1.00],"z":"中"}"#;
    let PipelineHandle::Diagnostics(restored) =
        rebuild_handle("diagnostics", Some(&publication(canonical))).unwrap()
    else {
        panic!("complete native diagnostic publication");
    };
    let report = restored.report(DiagnosticReportOwner::Validate).unwrap();
    assert_eq!(report.metadata["evidence"].canonical_json(), canonical);
    let roundtrip = gmeow_errors::Report::from_json_slice(
        gmeow_errors::render::to_json(report).unwrap().as_bytes(),
    )
    .unwrap();
    assert_eq!(&roundtrip, report.as_ref());
    for invalid in [
        " null",
        r#"{"b":1,"a":2}"#,
        r#"{"a":1,"a":2}"#,
        "[",
        r#""\ud800""#,
    ] {
        assert!(
            rebuild_handle("diagnostics", Some(&publication(invalid))).is_err(),
            "accepted {invalid}"
        );
    }
    let too_deep = format!(
        "{}0{}",
        "[".repeat(gmeow_errors::METADATA_JSON_DEPTH + 1),
        "]".repeat(gmeow_errors::METADATA_JSON_DEPTH + 1)
    );
    assert!(rebuild_handle("diagnostics", Some(&publication(&too_deep))).is_err());
}

/// Missing, trailing and duplicate-owner encodings are errors; a legitimate
/// singleton cannot claim another producer or stand in for both snapshot reports.
#[test]
fn diagnostics_codec_rejects_missing_payload_duplicate_owners_and_wrong_binding() {
    // gmeow-test-input: synthetic-only
    use serde::ser::{SerializeMap, SerializeStruct};
    assert!(rebuild_handle("diagnostics", None).is_err());
    assert!(rebuild_handle("old-diagnostics", Some(&[])).is_err());
    assert!(
        rebuild_handle("diagnostics", Some(&[0xa0])).is_err(),
        "the report map is mandatory"
    );
    let product = diagnostic_test_support::product(
        DiagnosticReportOwner::Validate,
        gmeow_errors::Report::new("shacl"),
    );
    let handle = &product.bundle().handle(GRAPH_DIAGNOSTICS).unwrap().payload;
    let mut bytes = encode_handle(handle).unwrap();
    bytes.push(0);
    assert!(rebuild_handle("diagnostics", Some(&bytes)).is_err());

    struct DuplicateReports<'a>(&'a gmeow_errors::Report);
    impl serde::Serialize for DuplicateReports<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut map = serializer.serialize_map(Some(2))?;
            map.serialize_entry(&DiagnosticReportOwner::Validate, self.0)?;
            map.serialize_entry(&DiagnosticReportOwner::Validate, self.0)?;
            map.end()
        }
    }
    struct DuplicatePublication<'a>(&'a gmeow_errors::Report);
    impl serde::Serialize for DuplicatePublication<'_> {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            let mut object = serializer.serialize_struct("DiagnosticsPublication", 1)?;
            object.serialize_field("reports", &DuplicateReports(self.0))?;
            object.end()
        }
    }
    let mut duplicate = Vec::new();
    ciborium::ser::into_writer(
        &DuplicatePublication(&gmeow_errors::Report::new("shacl")),
        &mut duplicate,
    )
    .unwrap();
    assert!(
        rebuild_handle("diagnostics", Some(&duplicate))
            .unwrap_err()
            .message()
            .contains("duplicate")
    );

    for wrong_stage in ["stage-compile-logic", "stage-snapshot", "another-stage"] {
        let mut cached = CachedBundle::from_product(&product, &selection(&product)).unwrap();
        cached.stage_id = wrong_stage.into();
        assert!(cached.into_product().is_err(), "wrong owner {wrong_stage}");
    }
    let mut cached = CachedBundle::from_product(&product, &selection(&product)).unwrap();
    cached.handles[0].graph = "https://example.org/other-graph".into();
    assert!(cached.into_product().is_err());
}
