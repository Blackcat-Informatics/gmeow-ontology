// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gmeow_cli_core::Reporter;
use gmeow_errors::Report;
use purrdf::provenance::DatasetProvenance;

use super::{PipelineSpec, StageSpec, registry_for, spec};
use crate::bundle::{PipelineHandle, bundle_from_artifacts};
use crate::memory_record::MemoryEvent;
use crate::node::{CachePolicy, OutputFamily, Stage, StageInput, StageOutput, StageProduct};
use crate::reads::Read;
use crate::scheduler::{
    CarrierRetention, ReadyOrder, RunContext, RunResult, SnapshotReleaseTrial,
    run_without_with_snapshot_trial,
};

const ARTIFACT: &str = "generated/shared.txt";
const EXCLUSIVE_ARTIFACT: &str = "generated/exclusive.txt";

struct PublicationOrder {
    digest_finished: mpsc::Sender<()>,
    digest: Mutex<mpsc::Receiver<()>>,
    sibling_published: mpsc::Sender<()>,
    publication: Mutex<mpsc::Receiver<()>>,
    panic_after_publication: bool,
}

impl Reporter for PublicationOrder {
    fn report(&self, _report: &Report) {}

    fn stage_start(&self, _stage: &str) {}

    fn stage_end(&self, stage: &str, _elapsed: Duration) {
        if stage != "stage-snapshot" {
            return;
        }
        self.digest_finished
            .send(())
            .expect("signal sibling reader");
        self.publication
            .lock()
            .expect("publication receiver")
            .recv_timeout(Duration::from_secs(60))
            .expect("the coordinator publishes the sibling while snapshot is held");
        assert!(!self.panic_after_publication, "snapshot progress panic");
    }

    fn summary(&self, _report: &Report) {}
}

struct SnapshotStage {
    spec: StageSpec,
    order: Arc<PublicationOrder>,
}

impl Stage for SnapshotStage {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn impl_version(&self) -> &str {
        "snapshot-release-fixture-v1"
    }

    fn output_family(&self) -> OutputFamily {
        self.spec.output_family
    }

    fn consumes(&self) -> &[String] {
        &self.spec.consumes
    }

    fn capabilities(&self) -> &[String] {
        &self.spec.capabilities
    }

    fn cache_policy(&self) -> CachePolicy {
        self.spec.cache_disposition
    }

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        let product = match self.id() {
            "a-exclusive" => {
                let data = b"<urn:subject> <urn:predicate> <urn:object> .";
                let dataset = purrdf::parse_dataset(data, "application/n-quads", None)
                    .expect("valid synthetic dataset");
                StageProduct::from_artifacts_over(
                    self.id(),
                    dataset,
                    [(EXCLUSIVE_ARTIFACT.to_owned(), b"complete".to_vec())].into(),
                )?
            }
            "b-shared" => StageProduct::from_artifacts(
                self.id(),
                [(ARTIFACT.to_owned(), b"retained".to_vec())].into(),
            )?,
            "stage-sibling" => {
                let source = &input.upstream["b-shared"];
                self.order
                    .digest
                    .lock()
                    .expect("digest receiver")
                    .recv_timeout(Duration::from_secs(60))
                    .expect("snapshot reaches its publication callback");
                assert_eq!(source.artifact(ARTIFACT), Some(b"retained".as_slice()));
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "after-sibling" => {
                self.order
                    .sibling_published
                    .send(())
                    .expect("signal snapshot");
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "late-reader" => {
                assert_eq!(
                    input.upstream["b-shared"].artifact(ARTIFACT),
                    Some(b"retained".as_slice())
                );
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "stage-snapshot" => {
                StageProduct::from_bundle(self.id(), self.prepare_snapshot_before_digest(input)?)?
            }
            _ => StageProduct::from_artifacts(self.id(), BTreeMap::new())?,
        };
        Ok(StageOutput::new(product))
    }

    fn prepare_snapshot_before_digest(
        &self,
        input: StageInput<'_>,
    ) -> gmeow_errors::Result<Arc<purrdf::PipelineBundle<PipelineHandle>>> {
        assert_eq!(input.upstream["a-exclusive"].dataset().quad_count(), 1);
        assert_eq!(input.upstream["b-shared"].dataset().quad_count(), 0);
        Ok(Arc::new(bundle_from_artifacts(
            BTreeMap::new(),
            DatasetProvenance::new(),
        )))
    }
}

fn run_parallel_snapshot(
    panic_after_publication: bool,
    ready_order: ReadyOrder,
    retention: CarrierRetention,
) -> gmeow_errors::Result<RunResult> {
    let (digest_finished, digest) = mpsc::channel();
    let (sibling_published, publication) = mpsc::channel();
    let order = Arc::new(PublicationOrder {
        digest_finished,
        digest: Mutex::new(digest),
        sibling_published,
        publication: Mutex::new(publication),
        panic_after_publication,
    });
    let mut snapshot = spec("stage-snapshot", &["a-exclusive", "b-shared"]);
    snapshot.cache_disposition = CachePolicy::Recompute;
    for input in ["a-exclusive", "b-shared"] {
        snapshot
            .reads
            .insert(input.to_owned(), [Read::Dataset].into());
    }
    let mut sibling = spec("stage-sibling", &["b-shared"]);
    sibling.reads.insert(
        "b-shared".to_owned(),
        [Read::Artifact(ARTIFACT.to_owned())].into(),
    );
    let mut late = spec("late-reader", &["b-shared", "stage-snapshot"]);
    late.reads.insert(
        "b-shared".to_owned(),
        [Read::Artifact(ARTIFACT.to_owned())].into(),
    );
    let pipeline = PipelineSpec {
        id: "parallel-snapshot-release".to_owned(),
        stages: vec![
            spec("a-exclusive", &[]),
            spec("b-shared", &[]),
            snapshot,
            sibling,
            spec("after-sibling", &["stage-sibling"]),
            spec("deferred", &["stage-sibling"]),
            late,
            spec("sink", &["after-sibling", "deferred", "late-reader"]),
        ],
    };
    let graph = pipeline.validate().expect("valid parallel snapshot graph");
    let registry = registry_for(
        &pipeline,
        pipeline
            .stages
            .iter()
            .map(|spec| {
                Arc::new(SnapshotStage {
                    spec: spec.clone(),
                    order: Arc::clone(&order),
                }) as Arc<dyn Stage>
            })
            .collect(),
    );
    let bound = crate::loader::bind(&pipeline, &graph, &registry).expect("bound snapshot graph");
    let root = tempfile::tempdir().expect("synthetic pipeline root");
    let mut ctx = RunContext::open_ephemeral(root.path(), 3)?.with_progress(order);
    ctx.carrier_retention = retention;
    ctx.retained_artifacts
        .insert(("b-shared".to_owned(), ARTIFACT.to_owned()));
    ctx.ready_order = ready_order;
    run_without_with_snapshot_trial(
        &graph,
        &bound,
        &mut ctx,
        &BTreeSet::new(),
        Some(SnapshotReleaseTrial::BeforeDigest),
    )
}

/// Snapshot releases an exclusive carrier while keeping its charge through
/// hashing. The sibling's light successor fits while the heavier one waits.
#[test]
fn snapshot_handoff_keeps_sibling_publication_live() {
    let estimates = serde_json::from_str(
        r#"{
            "a-exclusive":{"transient_bytes":0,"published_bytes":0},
            "b-shared":{"transient_bytes":0,"published_bytes":0},
            "stage-snapshot":{"transient_bytes":8,"published_bytes":0},
            "stage-sibling":{"transient_bytes":1,"published_bytes":0},
            "after-sibling":{"transient_bytes":1,"published_bytes":0},
            "deferred":{"transient_bytes":3,"published_bytes":0},
            "late-reader":{"transient_bytes":0,"published_bytes":0},
            "sink":{"transient_bytes":0,"published_bytes":0}
        }"#,
    )
    .expect("fixed snapshot admission estimates");
    let result = run_parallel_snapshot(
        false,
        ReadyOrder::Memory(Some(estimates), NonZeroU64::new(10)),
        CarrierRetention::DropAfterLastConsumer,
    )
    .expect("parallel snapshot publishes");
    let boundary = |stage, event| {
        let mut records = result
            .memory
            .iter()
            .filter(|record| record.stage_id == stage && record.event == event);
        let seq = records.next().expect("stage boundary").seq;
        assert!(records.next().is_none(), "stage boundary occurs once");
        seq
    };
    let digest_start = result
        .memory
        .iter()
        .find(|record| {
            record.stage_id == "stage-snapshot"
                && record.event == MemoryEvent::PhaseStart
                && record.phase.as_deref() == Some("digest-stage-product")
        })
        .expect("snapshot digest starts")
        .seq;
    let snapshot = boundary("stage-snapshot", MemoryEvent::Complete);
    assert!(boundary("a-exclusive", MemoryEvent::Release) < digest_start);
    assert!(boundary("after-sibling", MemoryEvent::Dispatch) < snapshot);
    assert!(snapshot < boundary("deferred", MemoryEvent::Dispatch));
    assert!(snapshot < boundary("b-shared", MemoryEvent::Release));
    assert!(snapshot < boundary("sink", MemoryEvent::Dispatch));
    assert_eq!(
        result
            .stage_receipts
            .iter()
            .filter(|receipt| receipt.context.stage_id == "stage-snapshot")
            .count(),
        1,
    );
    assert_eq!(
        result.products["b-shared"].artifact(ARTIFACT),
        Some(b"retained".as_slice())
    );
}

/// Fixture publishing discards finished artifact lanes before snapshot hashing,
/// keeps an artifact for a pending consumer, and returns an explicitly selected
/// byte artifact alongside unchanged receipts and product identities.
#[test]
fn receipt_only_snapshot_releases_artifacts_after_their_last_reader() {
    let control = run_parallel_snapshot(
        false,
        ReadyOrder::StageId,
        CarrierRetention::DropAfterLastConsumer,
    )
    .expect("control publishes");
    let result = run_parallel_snapshot(false, ReadyOrder::StageId, CarrierRetention::ReceiptOnly)
        .expect("receipt-only fixture publishes");
    let digest_start = result
        .memory
        .iter()
        .find(|record| {
            record.stage_id == "stage-snapshot"
                && record.event == MemoryEvent::PhaseStart
                && record.phase.as_deref() == Some("digest-stage-product")
        })
        .expect("snapshot digest starts")
        .seq;
    let exclusive = result
        .memory
        .iter()
        .find(|record| record.stage_id == "a-exclusive" && record.event == MemoryEvent::Release)
        .expect("snapshot releases the exclusive input");
    assert!(exclusive.seq < digest_start);
    assert_eq!(
        exclusive
            .released
            .expect("release sizes")
            .committed_lane_bytes,
        8
    );
    let late_reader = result
        .memory
        .iter()
        .find(|record| record.stage_id == "late-reader" && record.event == MemoryEvent::Complete)
        .expect("the pending artifact reader publishes");
    let shared = result
        .memory
        .iter()
        .find(|record| {
            record.stage_id == "b-shared" && record.event == MemoryEvent::ReleaseArtifacts
        })
        .expect("the last reader releases the shared artifact lane");
    assert!(late_reader.seq < shared.seq);
    assert_eq!(
        shared.released.expect("release sizes").committed_lane_bytes,
        8
    );
    assert!(result.products["b-shared"].artifact(ARTIFACT).is_none());
    assert_eq!(
        result.retained_artifacts[&("b-shared".to_owned(), ARTIFACT.to_owned())],
        b"retained"
    );
    assert_eq!(result.combined_digest, control.combined_digest);
    assert_eq!(result.receipt_root, control.receipt_root);
}

/// A panic in the snapshot continuation reports a terminal result to the scheduler.
#[test]
fn snapshot_progress_panic_reaches_terminal_failure() {
    let error = run_parallel_snapshot(
        true,
        ReadyOrder::StageId,
        CarrierRetention::DropAfterLastConsumer,
    )
    .err()
    .expect("snapshot panic is a failure");
    assert!(
        error
            .message()
            .contains("snapshot publication panicked: snapshot progress panic")
    );
}
