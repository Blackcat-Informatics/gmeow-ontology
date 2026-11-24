// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;
use std::num::NonZeroU64;
use std::path::Path;
use std::sync::Arc;

use super::{PipelineSpec, StageSpec, registry_for, spec};
use crate::memory_record::MemoryEvent;
use crate::node::{CachePolicy, OutputFamily, Stage, StageInput, StageOutput, StageProduct};
use crate::reads::Read;
use crate::scheduler::{CarrierRetention, ReadyOrder, RunContext, run};
use crate::stages::parse_sources::{GRAPH_SOURCE_CATALOG, STAGE_ID as PARSE_SOURCES};

struct FrontierStage(StageSpec, usize);

impl Stage for FrontierStage {
    fn id(&self) -> &str {
        &self.0.id
    }

    fn impl_version(&self) -> &str {
        "frontier-fixture-v1"
    }

    fn output_family(&self) -> OutputFamily {
        self.0.output_family
    }

    fn consumes(&self) -> &[String] {
        &self.0.consumes
    }

    fn capabilities(&self) -> &[String] {
        &self.0.capabilities
    }

    fn attaches_graphs(&self) -> &[String] {
        &self.0.attaches_graphs
    }

    fn cache_policy(&self) -> CachePolicy {
        self.0.cache_disposition
    }

    fn run(&self, input: StageInput<'_>) -> gmeow_errors::Result<StageOutput> {
        assert_eq!(rayon::current_num_threads(), self.1);
        let product = match self.id() {
            "producer" => {
                let triple = format!("<urn:subject> <urn:predicate> \"{}\" .", "x".repeat(2048));
                let dataset = purrdf::parse_dataset(triple.as_bytes(), "application/n-quads", None)
                    .expect("synthetic dataset");
                StageProduct::from_artifacts_over(self.id(), dataset, BTreeMap::new())?
            }
            PARSE_SOURCES => {
                let triple = format!("<urn:source> <urn:predicate> \"{}\" .", "x".repeat(8192));
                crate::stages::parse_sources::synthetic_product(&triple)
            }
            "a-dataset-reader" | "z-reader" => {
                assert_eq!(input.upstream["producer"].dataset().quad_count(), 1);
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "z-catalog-reader" => {
                assert!(
                    input.upstream[PARSE_SOURCES]
                        .handle(GRAPH_SOURCE_CATALOG)
                        .is_some()
                );
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            "catalog-receipt-reader" => {
                let source = &input.upstream[PARSE_SOURCES];
                assert_eq!(source.dataset().quad_count(), 3);
                assert!(source.raw_bundle().handle(GRAPH_SOURCE_CATALOG).is_none());
                StageProduct::from_artifacts(self.id(), BTreeMap::new())?
            }
            _ => StageProduct::from_artifacts(self.id(), BTreeMap::new())?,
        };
        Ok(StageOutput::new(product))
    }
}

fn execute_frontier(
    pipeline: &PipelineSpec,
    root: &Path,
    order: ReadyOrder,
) -> (Vec<String>, String, String) {
    let report = run_frontier(pipeline, root, order, 1).expect("run selected frontier");
    let dispatch = report
        .memory
        .iter()
        .filter(|record| record.event == MemoryEvent::Dispatch)
        .map(|record| record.stage_id.clone())
        .collect();
    (dispatch, report.combined_digest, report.receipt_root)
}

fn run_frontier(
    pipeline: &PipelineSpec,
    root: &Path,
    order: ReadyOrder,
    jobs: usize,
) -> gmeow_errors::Result<crate::scheduler::RunResult> {
    let graph = pipeline.validate().expect("frontier graph");
    let registry = registry_for(
        pipeline,
        pipeline
            .stages
            .iter()
            .map(|stage| Arc::new(FrontierStage(stage.clone(), jobs)) as Arc<dyn Stage>)
            .collect(),
    );
    let bound = crate::loader::bind(pipeline, &graph, &registry).expect("bind frontier graph");
    let mut ctx = RunContext::open_ephemeral(root, jobs).expect("cold run");
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    ctx.ready_order = order;
    run(&graph, &bound, &mut ctx)
}

/// A calibration run isolates stages while each dispatched stage can use the
/// full Rayon pool, as checked by the stage body.
#[test]
fn calibration_serializes_dispatch_with_the_full_worker_pool() {
    let directory = tempfile::tempdir().expect("synthetic calibration root");
    let pipeline = PipelineSpec {
        id: "calibration-fixture".into(),
        stages: vec![
            spec("a-source", &[]),
            spec("b-source", &[]),
            spec("sink", &["a-source", "b-source"]),
        ],
    };
    let report = run_frontier(&pipeline, directory.path(), ReadyOrder::Calibrate, 3)
        .expect("run calibration frontier");
    let boundaries: Vec<_> = report
        .memory
        .iter()
        .filter(|record| matches!(record.event, MemoryEvent::Dispatch | MemoryEvent::Complete))
        .map(|record| (record.stage_id.as_str(), record.event))
        .collect();
    assert_eq!(
        boundaries,
        [
            ("a-source", MemoryEvent::Dispatch),
            ("a-source", MemoryEvent::Complete),
            ("b-source", MemoryEvent::Dispatch),
            ("b-source", MemoryEvent::Complete),
            ("sink", MemoryEvent::Dispatch),
            ("sink", MemoryEvent::Complete),
        ]
    );
}

/// Admission refuses an oversized stage and queues combined work that exceeds
/// the ceiling. Delaying dispatch preserves products and receipts.
#[test]
fn active_work_ceiling_queues_heavy_work_while_a_lighter_stage_runs() {
    let directory = tempfile::tempdir().expect("synthetic admission root");
    let pipeline = PipelineSpec {
        id: "admission-fixture".into(),
        stages: vec![
            spec("a-heavy", &[]),
            spec("b-heavy", &[]),
            spec("c-light", &[]),
            spec("sink", &["a-heavy", "b-heavy", "c-light"]),
        ],
    };
    let order = |ceiling| {
        ReadyOrder::from_estimate_json(
            br#"{
                "a-heavy":{"transient_bytes":6,"published_bytes":0},
                "b-heavy":{"transient_bytes":1,"published_bytes":6},
                "c-light":{"transient_bytes":3,"published_bytes":0},
                "sink":{"transient_bytes":0,"published_bytes":0}
            }"#,
            NonZeroU64::new(ceiling),
        )
        .expect("fixed admission estimates")
    };
    let refusal = run_frontier(&pipeline, directory.path(), order(5), 3)
        .expect_err("an oversized stage is refused");
    assert!(
        refusal.message().contains(
            "a-heavy requires 6 estimated active-work bytes, above the ceiling of 5 bytes"
        )
    );

    let baseline = run_frontier(&pipeline, directory.path(), ReadyOrder::StageId, 3)
        .expect("run unbounded frontier");
    let limited =
        run_frontier(&pipeline, directory.path(), order(9), 3).expect("run limited frontier");
    let boundaries: Vec<_> = limited
        .memory
        .iter()
        .filter(|record| matches!(record.event, MemoryEvent::Dispatch | MemoryEvent::Complete))
        .map(|record| (record.stage_id.as_str(), record.event))
        .collect();
    assert_eq!(
        boundaries[0..2],
        [
            ("c-light", MemoryEvent::Dispatch),
            ("a-heavy", MemoryEvent::Dispatch),
        ]
    );
    let completed = boundaries
        .iter()
        .position(|boundary| *boundary == ("a-heavy", MemoryEvent::Complete));
    let next = boundaries
        .iter()
        .position(|boundary| *boundary == ("b-heavy", MemoryEvent::Dispatch));
    assert!(matches!((completed, next), (Some(left), Some(right)) if left < right));
    assert_eq!(limited.combined_digest, baseline.combined_digest);
    assert_eq!(limited.receipt_root, baseline.receipt_root);
}

/// A repeated stage must fail before a smaller charge can overwrite its estimate.
#[test]
fn active_work_profile_refuses_repeated_stage_names() {
    let Err(refusal) = ReadyOrder::from_estimate_json(
        br#"{
            "a-heavy":{"transient_bytes":12,"published_bytes":0},
            "sink":{"transient_bytes":0,"published_bytes":0},
            "a-heavy":{"transient_bytes":1,"published_bytes":0}
        }"#,
        NonZeroU64::new(10),
    ) else {
        panic!("duplicate stage was admitted");
    };
    let message = refusal.to_string();
    assert!(message.contains("duplicate") && message.contains("a-heavy"));
}

/// Independent ready stages can run in either order. The default memory
/// preference frees the live dataset before publishing the unrelated product.
/// A calibrated scratch requirement can instead favor the unrelated stage.
#[test]
fn memory_ready_order_uses_last_reader_credit_and_fixed_transient_cost() {
    let directory = tempfile::tempdir().expect("synthetic frontier root");
    let mut reader = spec("z-reader", &["producer"]);
    reader
        .reads
        .insert("producer".into(), [Read::Dataset].into());
    let pipeline = PipelineSpec {
        id: "frontier-fixture".into(),
        stages: vec![
            spec("producer", &[]),
            spec("a-peer", &["producer"]),
            reader,
            spec("sink", &["a-peer", "z-reader"]),
        ],
    };
    let execute = |order| execute_frontier(&pipeline, directory.path(), order);
    let (stage_id_order, digest, receipts) = execute(ReadyOrder::StageId);
    let (memory_order, memory_digest, memory_receipts) = execute(ReadyOrder::Memory(None));
    assert_eq!(stage_id_order, ["producer", "a-peer", "z-reader", "sink"]);
    assert_eq!(memory_order, ["producer", "z-reader", "a-peer", "sink"]);
    assert_eq!(memory_digest, digest);
    assert_eq!(memory_receipts, receipts);

    let profile = ReadyOrder::from_estimate_json(
        br#"{
            "producer":{"transient_bytes":0,"published_bytes":0},
            "z-reader":{"transient_bytes":100,"published_bytes":0},
            "a-peer":{"transient_bytes":10,"published_bytes":0},
            "sink":{"transient_bytes":0,"published_bytes":0}
        }"#,
        None,
    )
    .expect("fixed transient estimates");
    let (profile_order, profile_digest, profile_receipts) = execute(profile);
    assert_eq!(profile_order, stage_id_order);
    assert_eq!(profile_digest, digest);
    assert_eq!(profile_receipts, receipts);
}

/// The last keyed reader frees the native aggregate before a competing dataset
/// release. A later dataset reader still needs the catalog's RDF receipt.
#[test]
fn memory_ready_order_credits_the_exclusive_source_catalog_payload() {
    let directory = tempfile::tempdir().expect("synthetic catalog frontier root");
    let mut source = spec(PARSE_SOURCES, &[]);
    source.attaches_graphs = vec![GRAPH_SOURCE_CATALOG.into()];
    source.cache_disposition = CachePolicy::Recompute;
    let mut dataset_reader = spec("a-dataset-reader", &["producer", PARSE_SOURCES]);
    dataset_reader
        .reads
        .insert("producer".into(), [Read::Dataset].into());
    let mut catalog_reader = spec("z-catalog-reader", &["producer", PARSE_SOURCES]);
    catalog_reader.reads.insert(
        PARSE_SOURCES.into(),
        [Read::Handle(GRAPH_SOURCE_CATALOG.into())].into(),
    );
    let mut receipt_reader = spec(
        "catalog-receipt-reader",
        &[PARSE_SOURCES, "z-catalog-reader"],
    );
    receipt_reader
        .reads
        .insert(PARSE_SOURCES.into(), [Read::Dataset].into());
    let pipeline = PipelineSpec {
        id: "catalog-frontier-fixture".into(),
        stages: vec![
            spec("producer", &[]),
            source,
            dataset_reader,
            catalog_reader,
            receipt_reader,
            spec("sink", &["a-dataset-reader", "catalog-receipt-reader"]),
        ],
    };

    let execute = |order| execute_frontier(&pipeline, directory.path(), order);
    let (_, digest, receipts) = execute(ReadyOrder::StageId);
    let (dispatch, memory_digest, memory_receipts) = execute(ReadyOrder::Memory(None));
    assert_eq!(
        dispatch,
        [
            "producer",
            PARSE_SOURCES,
            "z-catalog-reader",
            "a-dataset-reader",
            "catalog-receipt-reader",
            "sink"
        ]
    );
    assert_eq!(memory_digest, digest);
    assert_eq!(memory_receipts, receipts);
}
