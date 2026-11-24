// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use purrdf::provenance::DatasetProvenance;

use super::{PipelineSpec, StageSpec, registry_for, spec};
use crate::bundle::{attach_rep_blob, bundle_from_artifacts};
use crate::memory_record::MemoryEvent;
use crate::node::{CachePolicy, OutputFamily, Stage, StageInput, StageOutput, StageProduct};
use crate::reads::{EdgeAccess, Read};
use crate::scheduler::{CarrierRetention, RunContext, run};

const EARLY: &str = "fixture:early";
const LATE: &str = "fixture:late";
const UNREAD: &str = "fixture:unread";

fn producer() -> gmeow_errors::Result<StageProduct> {
    let mut bundle = bundle_from_artifacts(BTreeMap::new(), DatasetProvenance::new());
    for (representation, bytes) in [
        (EARLY, b"early".as_slice()),
        (LATE, b"late".as_slice()),
        (UNREAD, b"unread".as_slice()),
    ] {
        bundle = attach_rep_blob(bundle, representation, "text/plain", bytes.to_vec())?;
    }
    StageProduct::from_bundle("producer", Arc::new(bundle))
}

/// A named in-flight reader keeps its own bytes while the producer can release
/// another representation from the same carrier.
#[test]
fn a_named_representation_view_keeps_only_its_payload_allocation() {
    let mut owner = producer().expect("publish a synthetic carrier");
    let mut reader = owner.consumer_view(EdgeAccess::new(
        "late",
        "producer",
        [Read::BlobRep(LATE.to_owned())].into(),
    ));
    reader
        .restrict_rep_blob_reads()
        .expect("select the late lane");
    assert!(reader.rep_blob(EARLY).is_none());
    assert_eq!(reader.raw_bundle().blobs().len(), 1);
    assert!(std::ptr::eq(
        reader.rep_blob(LATE).expect("late bytes in flight"),
        owner.rep_blob(LATE).expect("published late bytes"),
    ));

    let identity = owner.digest.clone();
    let release = owner
        .release_unread_rep_blobs(|rep| rep == LATE)
        .expect("detach early bytes")
        .expect("the detached representation has a memory record");
    assert_eq!(release.event, MemoryEvent::ReleaseRepBlobs);
    assert_eq!(owner.digest, identity);
    assert!(owner.rep_blob(EARLY).is_none());
    assert_eq!(reader.rep_blob(LATE), Some(b"late".as_slice()));
}

struct BlobStage {
    spec: StageSpec,
    whole_reader: bool,
    overlap_gate: Option<Arc<PublicationLatch>>,
}

struct PublicationLatch {
    publish: mpsc::Sender<()>,
    published: Mutex<mpsc::Receiver<()>>,
}

impl Stage for BlobStage {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn impl_version(&self) -> &str {
        "blob-retention-fixture-v1"
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

    fn attaches_blob_reps(&self) -> &[String] {
        &self.spec.attaches_blob_reps
    }

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        if self.id() == "producer" {
            return producer().map(StageOutput::new);
        }
        match self.id() {
            "first" => {
                let source = &input.upstream["producer"];
                assert_eq!(source.rep_blob(EARLY), Some(b"early".as_slice()));
            }
            "late" => {
                let bytes = input.upstream["producer"].rep_blob(LATE);
                // Bound a broken fixture's wait. The test checks scheduler events,
                // independent of how long a valid dispatch takes.
                self.overlap_gate
                    .as_ref()
                    .expect("overlap gate")
                    .published
                    .lock()
                    .expect("publication latch")
                    .recv_timeout(Duration::from_secs(60))
                    .expect("the first reader published and its successor ran");
                assert_eq!(bytes, Some(b"late".as_slice()));
            }
            "after-first" => {
                self.overlap_gate
                    .as_ref()
                    .expect("overlap gate")
                    .publish
                    .send(())
                    .expect("signal the in-flight reader");
            }
            "sink" => {
                let source = &input.upstream["producer"];
                if self.whole_reader {
                    assert_eq!(source.bundle().lookaside().blobs.len(), 3);
                } else {
                    assert_eq!(source.rep_blob(LATE), Some(b"late".as_slice()));
                }
            }
            "terminal" => {}
            other => panic!("unknown blob fixture stage: {other}"),
        }
        let output = BTreeMap::from([(
            format!("generated/{}.txt", self.id()),
            format!("{} output\n", self.id()).into_bytes(),
        )]);
        StageProduct::from_artifacts(self.id(), output).map(StageOutput::new)
    }
}

/// Fresh and hydrated representations obey their declared lifetimes and retain
/// the same terminal bytes, published identity and receipts.
#[test]
fn keyed_representation_release_preserves_cold_and_warm_results() {
    for (retained, whole_reader) in [(false, false), (true, false), (false, true)] {
        let mut producer = spec("producer", &[]);
        producer.attaches_blob_reps = [EARLY, LATE, UNREAD].map(str::to_owned).to_vec();
        let mut first = spec("first", &["producer"]);
        first.cache_disposition = CachePolicy::Recompute;
        first.reads.insert(
            "producer".to_owned(),
            [Read::BlobRep(EARLY.to_owned())].into(),
        );
        let mut sink = spec("sink", &["first", "producer"]);
        sink.cache_disposition = CachePolicy::Recompute;
        let sink_read = if whole_reader {
            Read::Carrier
        } else {
            Read::BlobRep(LATE.to_owned())
        };
        sink.reads.insert("producer".to_owned(), [sink_read].into());
        let pipeline = PipelineSpec {
            id: "blob-retention".to_owned(),
            stages: vec![producer, first, sink],
        };
        let graph = pipeline.validate().expect("representation retention graph");
        let registry = registry_for(
            &pipeline,
            pipeline
                .stages
                .iter()
                .map(|spec| {
                    Arc::new(BlobStage {
                        spec: spec.clone(),
                        whole_reader,
                        overlap_gate: None,
                    }) as Arc<dyn Stage>
                })
                .collect(),
        );
        let bound = crate::loader::bind(&pipeline, &graph, &registry).expect("bound stages");
        let directory = tempfile::tempdir().expect("synthetic cache directory");
        let execute = |retention| {
            let mut ctx = RunContext::open(directory.path(), 1).expect("open synthetic cache");
            ctx.carrier_retention = retention;
            if retained {
                ctx.retained_carriers.insert("producer".to_owned());
            }
            run(&graph, &bound, &mut ctx).expect("enforced representation retention run")
        };
        let cold = execute(CarrierRetention::DropAfterLastConsumer);
        let warm = execute(CarrierRetention::DropAfterLastConsumer);
        let control = execute(CarrierRetention::RetainAll);
        assert!(
            warm.stage_timings
                .iter()
                .any(|stage| stage.stage_id == "producer" && stage.cached)
        );
        assert_eq!(
            warm.products["producer"].rep_blob(EARLY).is_some(),
            retained
        );
        assert_eq!(
            warm.products["producer"].rep_blob(UNREAD).is_some(),
            retained
        );
        for result in [&cold, &warm] {
            let detached: Vec<_> = result
                .memory
                .iter()
                .filter(|record| {
                    record.stage_id == "producer" && record.event == MemoryEvent::ReleaseRepBlobs
                })
                .collect();
            if retained || whole_reader {
                assert!(
                    detached.is_empty(),
                    "the whole carrier retains both readers' bytes"
                );
            } else {
                let [unread, after_first] = detached.as_slice() else {
                    panic!("the producer must release at publication and after the first reader");
                };
                let boundary = |id, event| {
                    result
                        .memory
                        .iter()
                        .find(|record| record.stage_id == id && record.event == event)
                        .expect("scheduler records the stage boundary")
                        .seq
                };
                assert!(unread.seq < boundary("first", MemoryEvent::Dispatch));
                assert!(boundary("first", MemoryEvent::Complete) < after_first.seq);
                assert!(after_first.seq < boundary("sink", MemoryEvent::Dispatch));
                assert_eq!(
                    after_first
                        .released
                        .expect("first detach sizes")
                        .representation_payload_bytes,
                    b"early".len(),
                );
            }
        }
        for result in [&warm, &control] {
            assert_eq!(result.combined_digest, cold.combined_digest);
            assert_eq!(result.receipt_root, cold.receipt_root);
            assert_eq!(
                result.products["sink"].artifacts(),
                cold.products["sink"].artifacts()
            );
            assert_eq!(
                crate::reads::observed_turtle(&result.recorded_reads),
                crate::reads::observed_turtle(&cold.recorded_reads),
            );
        }
    }
}

/// An executing named reader holds its borrowed payload through the scheduler's
/// release of a different representation from that same producer.
#[test]
fn releasing_one_representation_preserves_an_in_flight_reader() {
    let mut producer = spec("producer", &[]);
    producer.attaches_blob_reps = [EARLY, LATE, UNREAD].map(str::to_owned).to_vec();
    let mut first = spec("first", &["producer"]);
    first.reads.insert(
        "producer".to_owned(),
        [Read::BlobRep(EARLY.to_owned())].into(),
    );
    let mut late = spec("late", &["producer"]);
    late.reads.insert(
        "producer".to_owned(),
        [Read::BlobRep(LATE.to_owned())].into(),
    );
    let mut terminal = spec("terminal", &["after-first", "late"]);
    terminal
        .capabilities
        .push(crate::node::SINK_CAPABILITY.to_owned());
    let pipeline = PipelineSpec {
        id: "blob-retention-overlap".to_owned(),
        stages: vec![
            producer,
            first,
            late,
            spec("after-first", &["first"]),
            terminal,
        ],
    };
    let graph = pipeline.validate().expect("representation overlap graph");
    let (publish, published) = mpsc::channel();
    let gate = Arc::new(PublicationLatch {
        publish,
        published: Mutex::new(published),
    });
    let registry = registry_for(
        &pipeline,
        pipeline
            .stages
            .iter()
            .map(|spec| {
                Arc::new(BlobStage {
                    spec: spec.clone(),
                    whole_reader: false,
                    overlap_gate: Some(Arc::clone(&gate)),
                }) as Arc<dyn Stage>
            })
            .collect(),
    );
    let bound = crate::loader::bind(&pipeline, &graph, &registry).expect("bound stages");
    let directory = tempfile::tempdir().expect("synthetic overlap root");
    let mut ctx = RunContext::open_uncached(directory.path(), 2);
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    ctx.ready_order = crate::scheduler::ReadyOrder::StageId;
    let result = run(&graph, &bound, &mut ctx).expect("overlapping readers finish");
    let boundary = |id, event| {
        result
            .memory
            .iter()
            .find(|record| record.stage_id == id && record.event == event)
            .expect("scheduler records the stage boundary")
            .seq
    };
    let early_release = result
        .memory
        .iter()
        .find(|record| {
            record.stage_id == "producer"
                && record.event == MemoryEvent::ReleaseRepBlobs
                && record
                    .released
                    .is_some_and(|released| released.representation_payload_bytes == b"early".len())
        })
        .expect("the scheduler releases the early payload")
        .seq;
    assert!(boundary("late", MemoryEvent::Dispatch) < early_release);
    assert!(boundary("first", MemoryEvent::Complete) < early_release);
    assert!(early_release < boundary("after-first", MemoryEvent::Dispatch));
    assert!(
        boundary("after-first", MemoryEvent::Dispatch) < boundary("late", MemoryEvent::Complete)
    );
}

/// Hiding representation bytes from a dataset-only view preserves attachment
/// classification and the persistent store-closure check.
#[test]
fn projected_views_preserve_inherited_blob_and_store_identities() {
    let mut owner = producer().expect("publish a synthetic carrier");
    owner
        .preserve_input_content_keys()
        .expect("validate the published store");
    let whole = owner.consumer_view(EdgeAccess::new(
        "aggregate",
        "producer",
        [Read::Dataset].into(),
    ));
    let mut projected = whole.clone();
    projected
        .restrict_rep_blob_reads()
        .expect("serve only the dataset lane");
    assert!(projected.raw_bundle().blobs().is_empty());

    let mut aggregate = BlobStage {
        spec: spec("aggregate", &["producer"]),
        whole_reader: false,
        overlap_gate: None,
    };
    let output = producer().expect("build matching representations");
    for view in [whole, projected] {
        let upstream = BTreeMap::from([("producer".to_owned(), view)]);
        aggregate.spec.cache_disposition = CachePolicy::Recompute;
        assert!(crate::scheduler::verify_attach_drift(&aggregate, &upstream, &output).is_ok());
        aggregate.spec.cache_disposition = CachePolicy::Persistent;
        let Err(error) = crate::scheduler::verify_attach_drift(&aggregate, &upstream, &output)
        else {
            panic!("a persistent output cannot inherit those store keys");
        };
        assert!(error.to_string().contains("inherits content store"));
    }
}
