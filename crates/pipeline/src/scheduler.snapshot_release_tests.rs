// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use gmeow_cli_core::Reporter;
use gmeow_errors::Report;
use gmeow_logic_compile::ir::LogicProgram;
use purrdf::provenance::DatasetProvenance;

use super::{PipelineSpec, StageSpec, registry_for, spec};
use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::memory_record::MemoryEvent;
use crate::node::{CachePolicy, OutputFamily, Stage, StageInput, StageOutput, StageProduct};
use crate::reads::Read;
use crate::scheduler::{CarrierRetention, ReadyOrder, RunContext, RunResult, run};

const ARTIFACT: &str = "generated/shared.txt";
const SNAPSHOT_ARTIFACT: &str = "generated/snapshot-only.txt";
const POST_RUN_ARTIFACT: &str = "pipeline/post-run.txt";
const POST_RUN_BYTES: &[u8] = b"retained for the post-run reader";
const USED_HANDLE: &str = "urn:handle:snapshot-used";
const UNREAD_HANDLE: &str = "urn:handle:snapshot-unread";

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
    retain_snapshot: bool,
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

    fn attaches_graphs(&self) -> &[String] {
        &self.spec.attaches_graphs
    }

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        let product = match self.id() {
            "a-exclusive" | "c-later-reader" | "d-retained" => {
                let data = b"<urn:subject> <urn:predicate> <urn:object> .";
                let dataset = purrdf::parse_dataset(data, "application/n-quads", None)
                    .expect("valid synthetic dataset");
                let artifacts = if self.id() == "a-exclusive" {
                    [
                        (POST_RUN_ARTIFACT.to_owned(), POST_RUN_BYTES.to_vec()),
                        (SNAPSHOT_ARTIFACT.to_owned(), b"snapshot-only".to_vec()),
                    ]
                    .into()
                } else {
                    BTreeMap::new()
                };
                StageProduct::from_artifacts_over(self.id(), dataset, artifacts)?
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
                assert_eq!(input.upstream["c-later-reader"].dataset().quad_count(), 1);
                self.order
                    .sibling_published
                    .send(())
                    .expect("signal snapshot");
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "stage-snapshot" => {
                StageProduct::from_bundle(self.id(), self.prepare_snapshot_before_digest(input)?)?
            }
            "sink" => {
                let snapshot = &input.upstream["stage-snapshot"];
                assert!(snapshot.handle(USED_HANDLE).is_some());
                assert_eq!(
                    snapshot.raw_bundle().handle(UNREAD_HANDLE).is_some(),
                    self.retain_snapshot,
                );
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
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
        assert_eq!(
            input.upstream["a-exclusive"].artifact(SNAPSHOT_ARTIFACT),
            Some(b"snapshot-only".as_slice())
        );
        assert_eq!(input.upstream["b-shared"].dataset().quad_count(), 0);
        assert_eq!(input.upstream["c-later-reader"].dataset().quad_count(), 1);
        assert_eq!(input.upstream["d-retained"].dataset().quad_count(), 1);
        let quads = [USED_HANDLE, UNREAD_HANDLE]
            .map(|graph| format!("<urn:s> <urn:p> <urn:o> <{graph}> .\n"))
            .concat();
        let dataset = purrdf::parse_dataset(quads.as_bytes(), "application/n-quads", None)
            .expect("synthetic typed backing graphs");
        let mut bundle =
            bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
        let mut control = purrdf::CanonicalizationControl::new(purrdf::CANON_CORPUS_POLICY, None);
        for graph in [USED_HANDLE, UNREAD_HANDLE] {
            let pin = bundle
                .graph_digest(graph, &mut control)
                .expect("snapshot graph digest");
            let program =
                LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), Some(graph.to_owned()));
            bundle
                .pin_handle(
                    graph,
                    PipelineHandle::Logic(Arc::new(program)),
                    pin,
                    &mut control,
                )
                .expect("pin snapshot typed handle");
        }
        Ok(Arc::new(bundle))
    }
}

fn run_parallel_snapshot(
    panic_after_publication: bool,
    retain_snapshot: bool,
    ready_order: ReadyOrder,
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
    let snapshot_inputs = ["a-exclusive", "b-shared", "c-later-reader", "d-retained"];
    let mut snapshot = spec("stage-snapshot", &snapshot_inputs);
    snapshot.cache_disposition = CachePolicy::Recompute;
    snapshot.attaches_graphs = [UNREAD_HANDLE, USED_HANDLE].map(str::to_owned).to_vec();
    for input in snapshot_inputs {
        snapshot
            .reads
            .insert(input.to_owned(), [Read::Dataset].into());
    }
    snapshot
        .reads
        .get_mut("a-exclusive")
        .expect("snapshot declares the exclusive input")
        .insert(Read::Artifact(SNAPSHOT_ARTIFACT.to_owned()));
    let mut sibling = spec("stage-sibling", &["b-shared"]);
    sibling.reads.insert(
        "b-shared".to_owned(),
        [Read::Artifact(ARTIFACT.to_owned())].into(),
    );
    let mut after_sibling = spec("after-sibling", &["c-later-reader", "stage-sibling"]);
    after_sibling
        .reads
        .insert("c-later-reader".to_owned(), [Read::Dataset].into());
    let mut sink = spec("sink", &["after-sibling", "deferred", "stage-snapshot"]);
    sink.reads.insert(
        "stage-snapshot".to_owned(),
        [Read::Handle(USED_HANDLE.to_owned())].into(),
    );
    let pipeline = PipelineSpec {
        id: "parallel-snapshot-release".to_owned(),
        stages: vec![
            spec("a-exclusive", &[]),
            spec("b-shared", &[]),
            spec("c-later-reader", &[]),
            spec("d-retained", &[]),
            snapshot,
            sibling,
            after_sibling,
            spec("deferred", &["stage-sibling"]),
            sink,
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
                    retain_snapshot,
                }) as Arc<dyn Stage>
            })
            .collect(),
    );
    let bound = crate::loader::bind(&pipeline, &graph, &registry).expect("bound snapshot graph");
    let root = tempfile::tempdir().expect("synthetic pipeline root");
    let mut ctx = RunContext::open_ephemeral(root.path(), 3)?.with_progress(order);
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    ctx.fanout_paths.insert(SNAPSHOT_ARTIFACT.to_owned());
    ctx.retain_carriers(["d-retained"]);
    if retain_snapshot {
        ctx.retain_carriers(["stage-snapshot"]);
    }
    ctx.retain_post_run_inputs([(
        "a-exclusive".to_owned(),
        [Read::Artifact(POST_RUN_ARTIFACT.to_owned())].into(),
    )]);
    ctx.ready_order = ready_order;
    run(&graph, &bound, &mut ctx)
}

/// Snapshot releases exclusive inputs and unread output handles before hashing.
/// Pending readers and explicitly retained products keep their payloads, and
/// the receipt commits both handles. The work charge admits only the lighter
/// sibling successor before publication.
#[test]
fn snapshot_handoff_keeps_sibling_publication_live() {
    let readiness = || {
        ReadyOrder::from_estimate_json(
            br#"{
            "a-exclusive":{"transient_bytes":0,"published_bytes":0},
            "b-shared":{"transient_bytes":0,"published_bytes":0},
            "c-later-reader":{"transient_bytes":0,"published_bytes":0},
            "d-retained":{"transient_bytes":0,"published_bytes":0},
            "stage-snapshot":{"transient_bytes":8,"published_bytes":0},
            "stage-sibling":{"transient_bytes":1,"published_bytes":0},
            "after-sibling":{"transient_bytes":1,"published_bytes":0},
            "deferred":{"transient_bytes":3,"published_bytes":0},
            "sink":{"transient_bytes":0,"published_bytes":0}
        }"#,
            NonZeroU64::new(10),
        )
        .expect("fixed snapshot admission estimates")
    };
    let result =
        run_parallel_snapshot(false, false, readiness()).expect("parallel snapshot publishes");
    let retained =
        run_parallel_snapshot(false, true, readiness()).expect("retained snapshot publishes");
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
    let handle_release = result
        .memory
        .iter()
        .find(|record| {
            record.stage_id == "stage-snapshot"
                && record.event == MemoryEvent::Reclaim
                && record.phase.as_deref() == Some("release-output-handles")
        })
        .expect("snapshot reclaims its unread handles");
    assert!(handle_release.seq < digest_start);
    let snapshot = boundary("stage-snapshot", MemoryEvent::Complete);
    assert!(boundary("a-exclusive", MemoryEvent::Release) < digest_start);
    assert_eq!(result.released_fanout_paths, [SNAPSHOT_ARTIFACT]);
    assert!(
        result.products["a-exclusive"]
            .artifact(SNAPSHOT_ARTIFACT)
            .is_none()
    );
    assert!(boundary("after-sibling", MemoryEvent::Dispatch) < snapshot);
    assert!(snapshot < boundary("deferred", MemoryEvent::Dispatch));
    assert!(snapshot < boundary("b-shared", MemoryEvent::Release));
    assert!(digest_start < boundary("c-later-reader", MemoryEvent::Release));
    assert!(snapshot < boundary("sink", MemoryEvent::Dispatch));
    let [released_receipt, retained_receipt] = [&result, &retained].map(|run| {
        let mut receipts = run
            .stage_receipts
            .iter()
            .filter(|receipt| receipt.context.stage_id == "stage-snapshot");
        let receipt = receipts.next().expect("snapshot receipt");
        assert!(receipts.next().is_none(), "snapshot publishes once");
        receipt
    });
    assert_eq!(released_receipt.typed_handles.len(), 2);
    assert_eq!(released_receipt, retained_receipt);
    assert_eq!(
        result.products["b-shared"].artifact(ARTIFACT),
        Some(b"retained".as_slice())
    );
    assert_eq!(
        result.products["a-exclusive"].artifact(POST_RUN_ARTIFACT),
        Some(POST_RUN_BYTES)
    );
    assert_eq!(result.products["d-retained"].dataset().quad_count(), 1);
}

/// A panic in the snapshot continuation reports a terminal result to the scheduler.
#[test]
fn snapshot_progress_panic_reaches_terminal_failure() {
    let error = run_parallel_snapshot(true, false, ReadyOrder::StageId)
        .expect_err("snapshot panic is a failure");
    assert!(
        error
            .message()
            .contains("snapshot publication panicked: snapshot progress panic")
    );
}
