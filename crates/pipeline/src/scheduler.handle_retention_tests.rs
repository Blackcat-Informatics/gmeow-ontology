// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic_compile::ir::LogicProgram;
use purrdf::provenance::DatasetProvenance;

use super::{PipelineSpec, StageSpec, registry_for, spec};
use crate::bundle::{PipelineHandle, bundle_from_artifacts_over};
use crate::node::{CachePolicy, OutputFamily, Stage, StageInput, StageOutput, StageProduct};
use crate::reads::Read;
use crate::scheduler::{CarrierRetention, RunContext, run};

const EARLY: &str = "urn:handle:early";
const LATE: &str = "urn:handle:late";
const UNREAD: &str = "urn:handle:unread";

struct HandleStage {
    spec: StageSpec,
    keep_all: bool,
    whole_reader: bool,
}

impl Stage for HandleStage {
    fn id(&self) -> &str {
        &self.spec.id
    }

    fn impl_version(&self) -> &str {
        "handle-release-fixture-v1"
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
        if self.id() == "producer" {
            return handle_product();
        }

        let source = &input.upstream["producer"];
        if self.id() == "first" {
            assert!(source.handle(EARLY).is_some());
            assert_eq!(source.raw_bundle().handle(UNREAD).is_some(), self.keep_all);
        } else if self.whole_reader {
            let bundle = source.bundle();
            assert!(
                [EARLY, LATE, UNREAD]
                    .iter()
                    .all(|key| bundle.handle(key).is_some())
            );
        } else {
            assert!(source.handle(LATE).is_some());
            assert_eq!(source.raw_bundle().handle(EARLY).is_some(), self.keep_all);
            assert!(source.handle_commitments().contains_key(EARLY));
        }
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            BTreeMap::new(),
        )?))
    }
}

fn handle_product() -> gmeow_errors::Result<StageOutput> {
    let quads = [EARLY, LATE, UNREAD]
        .map(|graph| format!("<urn:s> <urn:p> <urn:o> <{graph}> .\n"))
        .concat();
    let dataset = purrdf::parse_dataset(quads.as_bytes(), "application/n-quads", None)
        .expect("synthetic typed backing graphs");
    let mut bundle = bundle_from_artifacts_over(dataset, BTreeMap::new(), DatasetProvenance::new());
    let mut control = purrdf::CanonicalizationControl::new(purrdf::CANON_CORPUS_POLICY, None);
    for graph in [EARLY, LATE, UNREAD] {
        let pin = bundle
            .graph_digest(graph, &mut control)
            .expect("graph digest");
        let program = LogicProgram::new(Vec::new(), Vec::new(), Vec::new(), Some(graph.to_owned()));
        bundle
            .pin_handle(
                graph.to_owned(),
                PipelineHandle::Logic(Arc::new(program)),
                pin,
                &mut control,
            )
            .expect("pin synthetic typed handle");
    }
    Ok(StageOutput::new(StageProduct::from_bundle(
        "producer",
        Arc::new(bundle),
    )?))
}

/// Fresh and hydrated handle products retain their complete commitments while
/// releasing keyed payloads before the next consumer reads a surviving sibling.
#[test]
fn keyed_handles_release_between_consumers_without_changing_cache_identity() {
    for (retained, whole_reader) in [(false, false), (true, false), (false, true)] {
        let mut producer = spec("producer", &[]);
        producer.attaches_graphs = [EARLY, LATE, UNREAD].map(str::to_owned).to_vec();
        let mut first = spec("first", &["producer"]);
        first.cache_disposition = CachePolicy::Recompute;
        first.reads.insert(
            "producer".to_owned(),
            [Read::Handle(EARLY.to_owned())].into(),
        );
        let mut sink = spec("sink", &["first", "producer"]);
        sink.cache_disposition = CachePolicy::Recompute;
        let sink_read = if whole_reader {
            Read::Carrier
        } else {
            Read::Handle(LATE.to_owned())
        };
        sink.reads.insert("producer".to_owned(), [sink_read].into());
        let pipeline = PipelineSpec {
            id: "keyed-retention".to_owned(),
            stages: vec![producer, first, sink],
        };
        let graph = pipeline.validate().expect("keyed retention graph");
        let registry = registry_for(
            &pipeline,
            pipeline
                .stages
                .iter()
                .map(|spec| {
                    Arc::new(HandleStage {
                        spec: spec.clone(),
                        keep_all: retained || whole_reader,
                        whole_reader,
                    }) as Arc<dyn Stage>
                })
                .collect(),
        );
        let bound = crate::loader::bind(&pipeline, &graph, &registry).expect("bound stages");
        let directory = tempfile::tempdir().expect("synthetic cache directory");
        let execute = || {
            let mut ctx = RunContext::open(directory.path(), 1).expect("open synthetic cache");
            ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
            if retained {
                ctx.retained_carriers.insert("producer".to_owned());
            }
            run(&graph, &bound, &mut ctx).expect("enforced keyed retention run")
        };
        let cold = execute();
        let warm = execute();
        assert_eq!(cold.combined_digest, warm.combined_digest);
        assert_eq!(cold.receipt_root, warm.receipt_root);
        assert!(
            warm.stage_timings
                .iter()
                .any(|stage| stage.stage_id == "producer" && stage.cached)
        );
        assert_eq!(warm.products["producer"].handle(EARLY).is_some(), retained);
    }
}

/// A reader's shared carrier survives a release attempt until that reader ends.
#[test]
fn an_in_flight_handle_reader_keeps_the_shared_carrier() {
    let mut product = handle_product().expect("synthetic handle carrier").product;
    let view = product.consumer_view(crate::reads::EdgeAccess::new(
        "in-flight",
        "producer",
        [Read::Handle(LATE.to_owned())].into(),
    ));

    product.release_unread_handles(|key| key == LATE);
    assert!(Arc::ptr_eq(product.raw_bundle(), view.raw_bundle()));
    assert!(view.handle(LATE).is_some());
    assert!(product.handle(EARLY).is_some());

    drop(view);
    product.release_unread_handles(|key| key == LATE);
    assert!(product.handle(EARLY).is_none());
    assert!(product.handle(LATE).is_some());
}
