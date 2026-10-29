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
    use gmeow_errors::diag::{
        Advice, ArtifactChange, Guidance, GuidanceModality, GuidanceSource, Region, Remediation,
    };
    use gmeow_errors::{
        DiagnosticAttribution, Finding, FindingCategory, Location, Report, Rule, Severity,
        Standpoint,
    };

    // Frozen originals pin the complete cache packet and actual producer commitments.
    let check_reference = |case: &str, handle: &PipelineHandle, encoded: &[u8], pin: &str| {
        let reference = include_str!("cache/fixtures/diagnostics-cbor-v1.txt")
            .lines()
            .map(|line| line.split('|').collect::<Vec<_>>())
            .find(|fields| fields[0] == case)
            .expect("every complete report packet has its original reference");
        assert_eq!(reference.len(), 7);
        assert_eq!(
            purrdf_hash::hex::encode(encoded),
            reference[1],
            "original complete cache bytes: {case}"
        );
        let PipelineHandle::Diagnostics(publication) = handle else {
            panic!("the selected complete diagnostic codec arm")
        };
        let mut native = Vec::new();
        publication.write_cbor(&mut native).unwrap();
        assert_eq!(
            native.as_slice(),
            encoded,
            "actual cache uses the complete native owner: {case}"
        );
        assert_eq!(
            purrdf_hash::hex::encode(purrdf_hash::blake3::hash(encoded).as_bytes()),
            reference[2],
            "original raw identity: {case}"
        );
        assert_eq!(
            handle_payload_digest(handle),
            reference[3],
            "actual payload identity: {case}"
        );
        assert_eq!(pin, reference[4], "original graph pin: {case}");
        let commitment = crate::handle_identity::handle_commitment(GRAPH_DIAGNOSTICS, pin, handle);
        assert_eq!(
            commitment.identity, reference[5],
            "actual commitment arm: {case}"
        );
        assert_eq!(
            commitment.digest, reference[6],
            "actual graph commitment: {case}"
        );
    };
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
        let pin = product
            .bundle()
            .handle(GRAPH_DIAGNOSTICS)
            .unwrap()
            .content_digest
            .to_hex();
        check_reference(&product.stage_id, handle, &encoded, &pin);
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

    // These complete boundary-only packets fill gaps in the existing rendered
    // report cases without asking a renderer or semantic engine to admit them.
    let mut rich = diagnostic_test_support::rich_report(DiagnosticReportOwner::Validate);
    let finding = &mut rich.findings[0];
    finding.source_graph = Some(purrdf::iri::BaseIri::parse("urn:diagnostic:source").unwrap());
    finding.source_focus = Some("urn:diagnostic:focus".into());
    finding.locations.push(Location {
        path: Some("synthetic/λ\0.ttl".into()),
        line: Some(u64::MAX),
        column: Some(u32::MAX),
        logical: Some("urn:diagnostic:logical".into()),
        gts_term_id: Some(u64::MAX),
        gts_quad_index: Some(u64::MAX),
        gts_reifier_id: Some(u64::MAX),
        gts_frame_index: Some(u64::MAX),
        gts_segment_index: Some(u64::MAX),
    });
    finding.related_locations.push(Location::default());
    finding.attributions.push(DiagnosticAttribution {
        slice_iri: "urn:diagnostic:attributed-slice".into(),
        role: "derivation-support".into(),
        evidence: None,
    });
    finding
        .remediation
        .push(Remediation::new("prose-only repair", Standpoint::Advisory));
    for (index, region) in [
        Region::default(),
        Region {
            start_line: Some(u32::MAX),
            start_column: None,
            end_line: None,
            end_column: Some(1),
        },
        Region {
            start_line: None,
            start_column: Some(u32::MAX),
            end_line: Some(u32::MAX),
            end_column: None,
        },
    ]
    .into_iter()
    .enumerate()
    {
        finding.remediation.push(
            Remediation::new(
                format!("sparse-region repair {index}"),
                Standpoint::Perspectival,
            )
            .with_artifact_change(ArtifactChange {
                artifact_uri: format!("synthetic-{index}.ttl"),
                region,
                replacement: "replacement λ\0\n".into(),
            }),
        );
    }
    let categories = [
        FindingCategory::DataShapeViolation,
        FindingCategory::ModelingDisciplineViolation,
        FindingCategory::ContradictionWitness,
        FindingCategory::PermittedEpistemicConflict,
        FindingCategory::UnsupportedSemanticFeature,
        FindingCategory::IncompleteCheck,
        FindingCategory::ProjectionLoss,
        FindingCategory::PolicyWarning,
        FindingCategory::Corroboration,
        FindingCategory::Transient,
    ];
    let severities = [
        Severity::Error,
        Severity::Warning,
        Severity::Note,
        Severity::Info,
    ];
    let standpoints = [
        Standpoint::Advisory,
        Standpoint::Perspectival,
        Standpoint::Binding,
    ];
    let modalities = [
        GuidanceModality::HowToUse,
        GuidanceModality::UseWhen,
        GuidanceModality::AvoidWhen,
    ];
    let sources = [
        GuidanceSource::RuleGoverningTerm,
        GuidanceSource::DocumentedTerm,
    ];
    for (index, category) in categories.into_iter().enumerate() {
        let standpoint = standpoints[index % standpoints.len()];
        let mut finding = Finding::new(
            severities[index % severities.len()],
            format!("synthetic.category-{index}"),
            format!("category wire {index} λ\0"),
        )
        .with_category(category)
        .with_standpoint(standpoint);
        finding.advice.push(Advice {
            standpoint,
            text: "advice with an explicit absent URI".into(),
            help_uri: None,
        });
        finding.guidance.push(Guidance {
            modality: modalities[index % modalities.len()],
            source: sources[index % sources.len()],
            term_iri: "urn:diagnostic:documented-term".into(),
            text: "source guidance".into(),
            standpoint,
            help_uri: None,
        });
        // Direct storage retains the original absent tool field rather than
        // letting Report::add_finding fill it from the report's owner.
        rich.findings.push(finding);
    }
    rich.rules
        .push(Rule::new("synthetic.sparse-rule", Severity::Info));
    rich.metadata.insert(
        "scalar-and-container-boundaries".into(),
        serde_json::json!({
            "signed-min": i64::MIN,
            "signed-max": i64::MAX,
            "unsigned-max": u64::MAX,
            "zero": 0,
            "minus-one": -1,
            "null": null,
            "false": false,
            "true": true,
            "empty-array": [],
            "empty-map": {},
            "text": "quoted \"text\" λ\0\n",
            "nested": [{"values": [null, false, "retained"]}],
        }),
    );
    rich.metadata.insert(
        "finite-float-widths".into(),
        serde_json::Value::Array(
            [
                0.5_f64,
                f64::from_bits(0x3fd5_5555_6000_0000),
                0.1_f64,
                -0.0_f64,
            ]
            .into_iter()
            .map(|value| {
                serde_json::Value::Number(
                    serde_json::Number::from_f64(value).expect("all captured floats are finite"),
                )
            })
            .collect(),
        ),
    );
    for (case, owner, report) in [
        (
            "empty-report",
            DiagnosticReportOwner::CompileLogic,
            Report::new("logic-compile"),
        ),
        ("rich-boundaries", DiagnosticReportOwner::Validate, rich),
    ] {
        let publication =
            Arc::new(DiagnosticsPublication::producer(owner, Arc::new(report)).unwrap());
        let handle = PipelineHandle::Diagnostics(Arc::clone(&publication));
        let encoded = encode_handle(&handle).unwrap();
        check_reference(case, &handle, &encoded, "synthetic-diagnostics-graph-pin");
        let restored = rebuild_handle("diagnostics", Some(&encoded)).unwrap();
        let PipelineHandle::Diagnostics(restored_publication) = &restored else {
            panic!("complete direct diagnostic boundary arm")
        };
        assert_eq!(restored_publication.reports, publication.reports);
        assert_eq!(encode_handle(&restored).unwrap(), encoded);
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
        let mut publication = DiagnosticsPublication::read_cbor(
            cached.handles[0].typed_payload.as_ref().unwrap().as_slice(),
        )
        .unwrap();
        let report = Arc::make_mut(publication.reports.get_mut(&owner).unwrap());
        match mutation {
            0 => {
                report.metadata.insert(
                    "nested-evidence".into(),
                    serde_json::json!({"source":"substituted"}),
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

/// Missing, trailing and duplicate-owner encodings are errors; a legitimate
/// singleton cannot claim another producer or stand in for both snapshot reports.
#[test]
fn diagnostics_codec_rejects_missing_payload_duplicate_owners_and_wrong_binding() {
    // gmeow-test-input: synthetic-only
    use gmeow_errors::cbor::{DiagnosticCborError, MAX_DIAGNOSTIC_CBOR_DEPTH};
    use purrdf_lex::cbor::{self, Value};
    use std::error::Error as _;
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

    let text = |value: &str| Value::Text(value.into());
    let publication = |fields: Vec<(Value, Value)>| {
        Value::Map(vec![(
            text("reports"),
            Value::Map(vec![(text("Validate"), Value::Map(fields))]),
        )])
    };
    let with_metadata = |value: Value| {
        publication(vec![
            (text("tool"), text("shacl")),
            (text("metadata"), value),
        ])
    };
    let refuse = |value: Value, expected: &str| {
        let bytes = cbor::encode(&value);
        let error = rebuild_handle("diagnostics", Some(&bytes)).expect_err(expected);
        assert!(error.message().contains(expected), "{error}");
        let kind = error
            .downcast_ref::<crate::error::Decode>()
            .expect("codec refusal retains the pipeline's typed diagnostic kind");
        assert!(
            kind.source().unwrap().is::<DiagnosticCborError>(),
            "the native admission cause is retained beneath Decode"
        );
        error
    };
    let duplicate = Value::Map(vec![(
        text("reports"),
        Value::Map(vec![
            (
                text("Validate"),
                Value::Map(vec![(text("tool"), text("shacl"))]),
            ),
            (
                text("Validate"),
                Value::Map(vec![(text("tool"), text("shacl"))]),
            ),
        ]),
    )]);
    refuse(duplicate, "duplicate");

    // Closed records and metadata cannot silently discard or replace evidence.
    let mut unknown_root = publication(vec![(text("tool"), text("shacl"))]);
    let Value::Map(fields) = &mut unknown_root else {
        unreachable!()
    };
    fields.push((text("foreign"), Value::Bool(true)));
    refuse(unknown_root, "unknown DiagnosticsPublication field");
    refuse(
        publication(vec![
            (text("tool"), text("shacl")),
            (text("foreign"), Value::Bool(true)),
        ]),
        "unknown Report field",
    );
    refuse(
        publication(vec![
            (text("tool"), text("shacl")),
            (text("tool"), text("shacl")),
        ]),
        "duplicate Report.tool",
    );
    for metadata in [
        Value::Map(vec![
            (text("same"), Value::Bool(false)),
            (text("same"), Value::Bool(true)),
        ]),
        Value::Map(vec![(
            text("nested"),
            Value::Map(vec![
                (text("same"), Value::Null),
                (text("same"), Value::Bool(true)),
            ]),
        )]),
    ] {
        refuse(with_metadata(metadata), "duplicate diagnostic metadata key");
    }
    for value in [Value::Bytes(vec![1]), Value::Tag(42, Box::new(Value::Null))] {
        refuse(
            with_metadata(Value::Map(vec![(text("unsupported"), value)])),
            "unsupported diagnostic metadata CBOR kind",
        );
    }
    refuse(
        with_metadata(Value::Map(vec![(
            text("nonfinite"),
            Value::Float(f64::NAN),
        )])),
        "nonfinite diagnostic metadata float",
    );

    let finding = |severity: Value, source_graph: Option<Value>| {
        let mut fields = vec![
            (text("severity"), severity),
            (text("code"), text("synthetic.refusal")),
            (text("message"), text("retained refusal")),
        ];
        if let Some(value) = source_graph {
            fields.push((text("source_graph"), value));
        }
        publication(vec![
            (text("tool"), text("shacl")),
            (text("findings"), Value::Array(vec![Value::Map(fields)])),
        ])
    };
    refuse(
        finding(Value::from(7_u64), None),
        "expected diagnostic text",
    );
    refuse(finding(text("fatal"), None), "unknown Severity variant");
    let invalid_iri = refuse(
        finding(text("error"), Some(text("invalid space"))),
        "invalid diagnostic source graph",
    );
    assert!(
        invalid_iri
            .downcast_ref::<crate::error::Decode>()
            .unwrap()
            .source()
            .unwrap()
            .source()
            .is_some(),
        "native IRI cause survives the typed admission error"
    );

    // Four enclosing maps consume the same original publication container budget.
    let nested = |depth: usize| (0..depth).fold(Value::Null, |value, _| Value::Array(vec![value]));
    let metadata_depth = MAX_DIAGNOSTIC_CBOR_DEPTH - 4;
    let at_limit = cbor::encode(&with_metadata(Value::Map(vec![(
        text("nested"),
        nested(metadata_depth),
    )])));
    let restored = rebuild_handle("diagnostics", Some(&at_limit))
        .expect("the exact container boundary is admitted");
    assert_eq!(
        encode_handle(&restored).unwrap(),
        at_limit,
        "deep admitted metadata retains every container"
    );
    let beyond_limit = cbor::encode(&with_metadata(Value::Map(vec![(
        text("nested"),
        nested(metadata_depth + 1),
    )])));
    let error = rebuild_handle("diagnostics", Some(&beyond_limit))
        .expect_err("the next container exceeds the selected bound");
    let native = error
        .downcast_ref::<crate::error::Decode>()
        .unwrap()
        .source()
        .unwrap();
    assert!(native.is::<DiagnosticCborError>());
    assert!(
        native.source().unwrap().is::<cbor::DecodeError>(),
        "native decoder enforces the enclosing bound"
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
