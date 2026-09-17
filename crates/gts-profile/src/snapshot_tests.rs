// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic controls for GMEOW's single snapshot publication boundary.

use std::cell::Cell;
use std::sync::Arc;

use purrdf::gts_compose::IngestCheckpoint;
use purrdf::{
    CompositeDatasetView, CompositeSource, DatasetView, FallibleDatasetView, GraphMatch, QuadIds,
    QuadRef, RdfDataset, RdfDatasetBuilder, RdfStoreCapabilities, TermId, TermRef, TermValue,
    ViewLimits, ViewOperationStatus,
};

use super::*;

const FINDING: &str = "https://blackcatinformatics.ca/gmeow/finding/test";
const DIAGNOSTICS: &str = "https://blackcatinformatics.ca/gmeow/graph/diagnostics";
const EMPTY_PROJECTION: &str = "https://blackcatinformatics.ca/gmeow/graph/test-empty-projection";

fn native_finding() -> Arc<RdfDataset> {
    let mut source = RdfDatasetBuilder::new();
    let subject = source.intern_iri(FINDING);
    let predicate = source.intern_iri("https://blackcatinformatics.ca/gmeow/findingGateVerdict");
    let object = source.intern_iri("https://blackcatinformatics.ca/gmeow/GateFatal");
    source.push_quad(subject, predicate, object, None);
    source.freeze().expect("tiny native finding")
}

fn publish(builder: SnapshotBuilder) -> gmeow_errors::Result<GmeowGtsEmission> {
    // gmeow-test-input: synthetic-only
    emit_gmeow_gts(
        builder,
        Vec::new(),
        Vec::new(),
        None,
        &baseline_medium_plan(),
    )
}

#[test]
fn owned_exit_refuses_a_poisoned_prefix_with_the_exact_native_cause() {
    let mut snapshot = SnapshotBuilder::new();
    let _accepted = snapshot
        .add_view(&native_finding())
        .expect("accepted finding");
    let accepted_payload = snapshot.snapshot_payload();

    let mut source = RdfDatasetBuilder::new();
    let subject = source.intern_iri(FINDING);
    let predicate = source.intern_iri("http://www.w3.org/ns/prov#wasDerivedFrom");
    let object = source.intern_iri("https://blackcatinformatics.ca/gmeow/source/test");
    let quoted = source.intern_triple(subject, predicate, object);
    source.push_quad(subject, predicate, quoted, None);
    let unsupported = source.freeze().expect("native quoted provenance value");
    let cause = snapshot
        .add_view(&unsupported)
        .expect_err("unsupported snapshot slot");
    assert!(matches!(&cause, GtsIngestError::UnrepresentableTerm { .. }));
    assert_eq!(snapshot.snapshot_payload(), accepted_payload);

    let error = publish(snapshot).expect_err("the accepted prefix is not the selected snapshot");
    let typed = error
        .downcast_ref::<error::SnapshotAdmission>()
        .expect("typed admission");
    assert_eq!(typed.cause, cause);
    assert_eq!(
        gmeow_errors::code::code_str(error.code()),
        error::SnapshotAdmission::CODE
    );
}

#[test]
fn exact_omitted_graph_inventory_survives_the_ownership_exit() {
    let mut source = RdfDatasetBuilder::new();
    let diagnostics = source.intern_iri(DIAGNOSTICS);
    let empty_projection = source.intern_iri(EMPTY_PROJECTION);
    source.declare_named_graph(diagnostics);
    source.declare_named_graph(empty_projection);
    let declarations = source.freeze().expect("declared empty GMEOW roles");
    let mut snapshot = SnapshotBuilder::new();
    let _finding = snapshot.add_view(&native_finding()).expect("finding input");
    let _declarations = snapshot.add_view(&declarations).expect("declaration input");
    let expected = snapshot.ingest_totals();
    assert_eq!(expected.rows_consumed, 1);
    assert_eq!(
        expected.declarations_omitted,
        vec![DIAGNOSTICS, EMPTY_PROJECTION]
    );

    let emission = publish(snapshot).expect("valid row-derived snapshot");
    assert_eq!(emission.ingestion, expected);
    let receipt = emission
        .ingestion_receipt()
        .expect("complete companion receipt");
    let decoded = read_ingestion_receipt(&emission.bytes, &receipt).expect("output-bound receipt");
    assert_eq!(decoded.ingestion, expected);
    assert!(decoded.source_receipts.is_empty());
    validate_mandated_frames(&emission.bytes).expect("mandated output profile");
    // gmeow-test-input: synthetic-only
    let baseline = view_to_gmeow_gts(&native_finding()).expect("same wire-visible finding");
    assert_eq!(
        emission.bytes, baseline.bytes,
        "omission evidence must not invent wire rows"
    );
}

#[test]
fn companion_receipt_refuses_wrong_output_trailing_and_malformed_fields() {
    // gmeow-test-input: synthetic-only
    let emission = view_to_gmeow_gts(&native_finding()).expect("finding output");
    let receipt = emission.ingestion_receipt().expect("receipt");
    let mut different = emission.bytes.clone();
    different.push(0);
    assert!(read_ingestion_receipt(&different, &receipt).is_err());
    let mut trailing = receipt.clone();
    trailing.push(0);
    assert!(read_ingestion_receipt(&emission.bytes, &trailing).is_err());
    let mut record: Value = ciborium::de::from_reader(receipt.as_slice()).expect("receipt CBOR");
    let Value::Array(fields) = &mut record else {
        panic!("fixed receipt record")
    };
    fields[5] = Value::Array(vec![Value::Integer(1.into())]);
    let mut malformed = Vec::new();
    ciborium::ser::into_writer(&record, &mut malformed).expect("synthetic mutation");
    assert!(GmeowGtsSourceReceipt::admit(&emission.bytes, &malformed).is_err());
    assert!(GmeowGtsSourceReceipt::admit(&emission.bytes, &[]).is_err());
}

#[test]
fn inherited_omissions_stay_in_the_original_report_outside_the_payload() {
    let mut source = RdfDatasetBuilder::new();
    let graph = source.intern_iri(EMPTY_PROJECTION);
    source.declare_named_graph(graph);
    // gmeow-test-input: synthetic-only
    let original = view_to_gmeow_gts(&source.freeze().expect("empty selected role"))
        .expect("original emission");
    let original_receipt = original.ingestion_receipt().expect("original receipt");
    // gmeow-test-input: synthetic-only
    let mut next = view_to_gmeow_gts(&native_finding()).expect("next selected output");
    let admitted = GmeowGtsSourceReceipt::admit(&original.bytes, &original_receipt)
        .expect("actual source admission");
    assert!(
        admitted.validate_input(&next.bytes).is_err(),
        "a held receipt cannot move to another source"
    );
    let before = next.bytes.clone();
    next.source_receipts.push(admitted);
    let encoded = next.ingestion_receipt().expect("complete chained receipt");
    let decoded = read_ingestion_receipt(&next.bytes, &encoded).expect("current output identity");
    assert!(
        decoded.ingestion.declarations_omitted.is_empty(),
        "current native counts are not historical losses"
    );
    assert_eq!(decoded.ingestion, next.ingestion);
    assert_eq!(decoded.source_receipts.len(), 1);
    let prior = read_ingestion_receipt(&original.bytes, decoded.source_receipts[0].encoded())
        .expect("original source identity");
    assert_eq!(prior.ingestion, original.ingestion);
    assert_eq!(prior.ingestion.declarations_omitted, vec![EMPTY_PROJECTION]);
    assert_eq!(
        next.bytes, before,
        "runtime evidence cannot change the GTS payload"
    );
}

#[test]
fn receipt_nesting_bound_refuses_unreadable_publication() {
    // gmeow-test-input: synthetic-only
    let mut emission = view_to_gmeow_gts(&native_finding()).expect("tiny selected output");
    for _ in 0..63 {
        let encoded = emission.ingestion_receipt().expect("bounded chain");
        let prior =
            GmeowGtsSourceReceipt::admit(&emission.bytes, &encoded).expect("bounded source chain");
        emission.source_receipts = vec![prior];
    }
    let encoded = emission
        .ingestion_receipt()
        .expect("maximum admitted chain");
    emission.source_receipts =
        vec![GmeowGtsSourceReceipt::admit(&emission.bytes, &encoded).expect("bounded prior")];
    assert!(
        emission.ingestion_receipt().is_err(),
        "writer must refuse before publishing an unreadable receipt"
    );
}

#[test]
fn runtime_ingestion_counts_do_not_change_snapshot_bytes() {
    let source = native_finding();
    // gmeow-test-input: synthetic-only
    let once = view_to_gmeow_gts(&source).expect("one native source");
    let mut snapshot = SnapshotBuilder::new();
    let _first = snapshot.add_view(&source).expect("first ingress");
    let _second = snapshot.add_view(&source).expect("same content again");
    let twice = publish(snapshot).expect("duplicate source admission");
    assert_eq!(twice.bytes, once.bytes);
    assert_eq!(
        twice.ingestion.rows_consumed,
        once.ingestion.rows_consumed * 2
    );
    assert_eq!(
        twice.ingestion.terms_interned,
        once.ingestion.terms_interned
    );
}

#[test]
fn native_view_exit_does_not_materialize_the_shared_input() {
    let view = CompositeDatasetView::from_sources(
        vec![CompositeSource::new(native_finding())],
        ViewLimits::default(),
    )
    .expect("admitted contribution");
    let before = view.stats().work;
    // gmeow-test-input: synthetic-only
    let emission = view_to_gmeow_gts(&view).expect("native view emission");
    let after = view.stats().work;
    assert_eq!(emission.ingestion.rows_consumed, 1);
    assert_eq!(before.materializations, 0);
    assert_eq!(after.materializations, before.materializations);
    assert_eq!(after.freezes, before.freezes);
    assert_eq!(after.copied_rows, before.copied_rows);
    validate_mandated_frames(&emission.bytes).expect("view exit keeps GMEOW profile");
}

#[test]
fn signed_owned_exit_preserves_transport_key_and_frame_signatures() {
    let source = native_finding();
    let mut snapshot = SnapshotBuilder::new();
    let _ingestion = snapshot.add_view(&source).expect("native finding input");
    // gmeow-test-input: synthetic-only
    let emission = emit_gmeow_gts(
        snapshot,
        Vec::new(),
        Vec::new(),
        Some(GmeowGtsSigning {
            secret: [7; 32],
            key_id: "gmeow-profile-test".to_owned(),
            public_key_armor: "test-public-key-armor".to_owned(),
        }),
        &baseline_medium_plan(),
    )
    .expect("signed GMEOW output");
    validate_mandated_frames(&emission.bytes).expect("signed payloads keep level12");
    let graph = purrdf::gts::reader::read(&emission.bytes, false, None);
    assert!(graph.diagnostics.is_empty(), "{:?}", graph.diagnostics);
    let key = purrdf::gts::verify::extract_transport_key(&graph).expect("retained transport key");
    assert_eq!(key.kid, "gmeow-profile-test");
    assert_eq!(key.gpg, "test-public-key-armor");
    let [quad] = graph.quads.as_slice() else {
        panic!("the signed snapshot must retain exactly the selected GMEOW finding");
    };
    for (term, expected) in [
        (quad.0, FINDING),
        (
            quad.1,
            "https://blackcatinformatics.ca/gmeow/findingGateVerdict",
        ),
        (quad.2, "https://blackcatinformatics.ca/gmeow/GateFatal"),
    ] {
        assert_eq!(graph.terms[term].kind, purrdf::gts::model::TermKind::Iri);
        assert_eq!(graph.terms[term].value.as_deref(), Some(expected));
    }
    assert_eq!(quad.3, None);
    assert!(graph.reifiers.is_empty());
    assert!(graph.annotations.is_empty());
    let verifying_key = ed25519_dalek::SigningKey::from_bytes(&[7; 32]).verifying_key();
    let keyring =
        std::collections::HashMap::from([("gmeow-profile-test".to_owned(), verifying_key)]);
    let verification = purrdf::gts::verify::verify_file_with_keyring(&emission.bytes, &keyring);
    assert!(verification.ok, "{:?}", verification.errors);
    assert_eq!(
        verification.signed, 2,
        "both metadata and snapshot must be signed"
    );
    assert_eq!(verification.valid, verification.signed);
    assert_eq!(verification.invalid, 0);
    assert_eq!(verification.unverified, 0);
    assert_eq!(emission.ingestion.rows_consumed, 1);

    // Keep the profile audit strict on structured transport payloads too. This
    // alters only tiny synthetic wire data; it does not author a second bundle.
    let (mut frames, torn) = iter_items(&emission.bytes);
    assert_eq!(torn, None);
    let metadata = frames
        .iter_mut()
        .find_map(|(_, item)| {
            if let Value::Map(frame) = item
                && matches!(map_get(frame, "t"), Some(Value::Text(kind)) if kind == "meta")
            {
                Some(frame)
            } else {
                None
            }
        })
        .expect("signed transport-key frame");
    assert!(matches!(map_get(metadata, "d"), Some(Value::Bytes(_))));
    metadata.retain(|(key, _)| !matches!(key, Value::Text(key) if key == "x"));
    metadata
        .iter_mut()
        .find(|(key, _)| matches!(key, Value::Text(key) if key == "d"))
        .expect("metadata payload")
        .1 = Value::Map(vec![(
        "gts:transportKey".into(),
        Value::Map(vec![
            ("kid".into(), Value::Text(key.kid)),
            ("gpg".into(), Value::Text(key.gpg)),
        ]),
    )]);
    let mut uncompressed = Vec::new();
    for (_, frame) in frames {
        ciborium::ser::into_writer(&frame, &mut uncompressed).expect("tiny synthetic CBOR");
    }
    let error = validate_mandated_frames(&uncompressed)
        .expect_err("transport metadata cannot bypass the payload compression contract");
    assert!(
        error.message().contains("has no transform chain"),
        "{error}"
    );
}

#[test]
fn selected_snapshot_medium_cannot_weaken_the_level_contract() {
    for level in [None, Some(2), Some(11), Some(13)] {
        let mut snapshot = SnapshotBuilder::new();
        let _ingestion = snapshot
            .add_view(&native_finding())
            .expect("native finding input");
        // gmeow-test-input: synthetic-only
        let error = emit_gmeow_gts(
            snapshot,
            Vec::new(),
            Vec::new(),
            None,
            &MediumPlan::undicted(level),
        )
        .expect_err("selected medium must retain the GMEOW level");
        assert!(error.downcast_ref::<error::Profile>().is_some());
    }
}

/// A selected input whose final completion check discovers a missing piece.
/// The control grades GMEOW's error propagation; it does not exercise a parser,
/// query engine or upstream format-conformance suite.
struct IncompleteFinding {
    dataset: Arc<RdfDataset>,
    checkpoints: Cell<usize>,
}

#[derive(Debug, Clone)]
struct MissingFindingPartition;

impl std::fmt::Display for MissingFindingPartition {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("selected finding partition is missing")
    }
}

impl std::error::Error for MissingFindingPartition {}

impl DatasetView for IncompleteFinding {
    type Id = TermId;
    type ProbePlan = ();

    fn quads(&self) -> impl Iterator<Item = QuadIds<TermId>> + '_ {
        self.dataset
            .quads()
            .filter(move |_| self.checkpoints.get() < 2)
    }

    fn quad_refs(&self) -> impl Iterator<Item = QuadRef<'_, TermId>> + '_ {
        DatasetView::quad_refs(self.dataset.as_ref()).filter(move |_| self.checkpoints.get() < 2)
    }

    fn resolve(&self, id: TermId) -> TermRef<'_> {
        self.dataset.resolve(id)
    }

    fn term_id_by_value(&self, value: &TermValue) -> Option<TermId> {
        self.dataset.term_id_by_value(value)
    }

    fn capabilities(&self) -> RdfStoreCapabilities {
        self.dataset.capabilities()
    }

    fn probe_plan(&self, _: bool, _: bool, _: bool, _: GraphMatch<TermId>) {}

    fn quads_for_pattern_with_plan(
        &self,
        _: &(),
        subject: Option<TermId>,
        predicate: Option<TermId>,
        object: Option<TermId>,
        graph: GraphMatch<TermId>,
    ) -> impl Iterator<Item = QuadIds<TermId>> + '_ {
        self.quads_for_pattern(subject, predicate, object, graph)
    }

    fn term_count(&self) -> usize {
        self.dataset.term_count()
    }
}

impl FallibleDatasetView for IncompleteFinding {
    type Error = MissingFindingPartition;
    type Evidence = usize;

    fn operation_status(&self) -> ViewOperationStatus<Self::Error, Self::Evidence> {
        let sampled = self.checkpoints.get();
        self.checkpoints.set(sampled + 1);
        if sampled == 0 {
            ViewOperationStatus::Ready { evidence: sampled }
        } else {
            ViewOperationStatus::Failed {
                error: MissingFindingPartition,
                evidence: sampled,
            }
        }
    }
}

#[test]
fn native_view_exit_keeps_the_failed_completion_checkpoint() {
    let source = IncompleteFinding {
        dataset: native_finding(),
        checkpoints: Cell::new(0),
    };
    // gmeow-test-input: synthetic-only
    let error = view_to_gmeow_gts(&source).expect_err("incomplete selected finding input");
    let typed = error
        .downcast_ref::<error::SnapshotAdmission>()
        .expect("typed admission");
    assert!(matches!(
        &typed.cause,
        GtsIngestError::ViewNotReady { checkpoint: IngestCheckpoint::AfterRows, cause }
            if cause == "selected finding partition is missing"
    ));
    assert!(source.checkpoints.get() >= 2);
}
