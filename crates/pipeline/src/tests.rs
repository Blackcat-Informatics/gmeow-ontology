// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests. P1: DAG validation (cycle / completeness / sink), registry
//! binding agreement (capabilities / consumes / resources), the dogfooded-DAG
//! Turtle round-trip. P2: the self-verifying cache, provenance stamping, and
//! scheduler determinism.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cache::{
    PipelineCache, RawInputDigest, ReceiptOutputSelection, StageInputDigest, StageKeyContext,
    stage_key,
};
use crate::loader::{PipelineSpec, StageSpec, bind};
use crate::node::{
    CachePolicy, ENGINE_RESOURCE, SINK_CAPABILITY, SOURCE_ORIGIN, Stage, StageInput, StageOutput,
    StageProduct, StageStability,
};
use crate::provenance::register_stage_unit;
use crate::registry::StageRegistry;
use crate::scheduler::{RunContext, dependency_closure, run, run_targets};
use gmeow_cli_core::Reporter;
use gmeow_errors::Report;

/// The resources / capabilities a stage with id `id` declares, mirroring the real
/// stage impls so bind's agreement holds in fixtures: `r` (the reasoner) requires the
/// exclusive engine resource, `source` holds [`SOURCE_ORIGIN`], `sink` holds
/// [`SINK_CAPABILITY`], everything else declares neither.
fn resources_for(id: &str) -> Vec<String> {
    if id == "r" {
        vec![ENGINE_RESOURCE.to_string()]
    } else {
        Vec::new()
    }
}

fn capabilities_for(id: &str) -> Vec<String> {
    match id {
        "source" => vec![SOURCE_ORIGIN.to_string()],
        "sink" => vec![SINK_CAPABILITY.to_string()],
        _ => Vec::new(),
    }
}

fn spec(id: &str, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        capabilities: capabilities_for(id),
        impl_key: format!("impl:{id}"),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: resources_for(id),
        stability: StageStability::StablePrefix,
        cache_disposition: CachePolicy::Persistent,
        dataflow_entities: Vec::new(),
        formats: Vec::new(),
        attaches_graphs: Vec::new(),
        attaches_blob_reps: Vec::new(),
    }
}

/// A diamond: source → (a, b) → sink.
fn diamond() -> PipelineSpec {
    PipelineSpec {
        id: "pipeline-build".to_string(),
        stages: vec![
            spec("source", &[]),
            spec("a", &["source"]),
            spec("b", &["source"]),
            spec("sink", &["a", "b"]),
        ],
    }
}

#[test]
fn valid_diamond_levels_producers_first() {
    let g = diamond().validate().expect("diamond is valid");
    assert_eq!(g.levels[0], vec!["source"]);
    assert_eq!(g.levels[1], vec!["a", "b"]); // sorted within level
    assert_eq!(g.levels[2], vec!["sink"]);
    assert_eq!(g.len(), 4);
}

#[test]
fn cycle_is_rejected() {
    let mut s = diamond();
    // Make `source` consume `sink`: source → a/b → sink → source.
    s.stages[0].consumes = vec!["sink".to_string()];
    match s.validate() {
        Ok(_) => panic!("expected cycle rejection"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(d.to_string().contains("cycle"), "{d}");
        }
    }
}

#[test]
fn dangling_dependency_is_rejected() {
    let mut s = diamond();
    s.stages[1].consumes = vec!["ghost".to_string()];
    match s.validate() {
        Ok(_) => panic!("expected dangling-dependency rejection"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(d.to_string().contains("ghost"), "{d}");
        }
    }
}

#[test]
fn unknown_consumes_edge_errors_instead_of_panicking() {
    // A `consumes` map whose CONSUMER KEY is not a declared node would once have
    // panicked at `index[stage]` in the public `StageGraph::build`. It must now
    // return an InvalidDag error.
    use crate::graph::StageGraph;
    use std::collections::{BTreeMap, BTreeSet};

    let nodes: BTreeSet<String> = ["source".to_string(), "sink".to_string()]
        .into_iter()
        .collect();
    let mut consumes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // `ghost` is not a node, yet it declares a dependency (a malformed adjacency).
    consumes.insert(
        "ghost".to_string(),
        ["source".to_string()].into_iter().collect(),
    );
    match StageGraph::build(&nodes, &consumes) {
        Ok(_) => panic!("expected InvalidDag for an unknown consumer key"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(d.to_string().contains("ghost"), "{d}");
        }
    }
}

#[test]
fn missing_sink_is_rejected() {
    let mut s = diamond();
    s.stages[3].capabilities.clear(); // strip SINK_CAPABILITY from the only sink
    match s.validate() {
        Ok(_) => panic!("expected missing-sink rejection"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(d.to_string().contains("no Sink"), "{d}");
        }
    }
}

#[test]
fn multiple_sinks_are_rejected() {
    let mut s = diamond();
    // Give `a` SINK_CAPABILITY too: now `a` and `sink` are both sinks.
    s.stages[1].capabilities = vec![SINK_CAPABILITY.to_string()];
    match s.validate() {
        Ok(_) => panic!("expected multiple-sink rejection"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(d.to_string().contains("Sink stages"), "{d}");
        }
    }
}

#[test]
fn typed_dataflow_from_non_consumed_producer_is_rejected() {
    // `a` consumes only `source`; a typed-dataflow narrowing from `b` (a real stage
    // it does NOT consume) would key `a`'s cache on a graph the scheduler never feeds
    // it — the validator HARD-fails (no-optionality).
    let mut s = diamond();
    let a = s.stages.iter_mut().find(|st| st.id == "a").unwrap();
    a.dataflow_entities = vec![("b".to_string(), vec!["https://example.org/g".to_string()])];
    match s.validate() {
        Ok(_) => panic!("expected typed-dataflow rejection"),
        Err(d) => {
            assert_eq!(d.code(), crate::error::InvalidDag::register(), "{d}");
            assert!(
                d.to_string().contains("does not gmeow:dataflowConsumes"),
                "{d}"
            );
        }
    }
}

// ── Binding agreement ────────────────────────────────────────────────────────

struct FakeStage {
    id: String,
    capabilities: Vec<String>,
    consumes: Vec<String>,
    resources: Vec<String>,
}

impl Stage for FakeStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        Ok(StageOutput::new(StageProduct::new(
            self.id.clone(),
            "deadbeef",
        )))
    }
}

fn fake(id: &str, consumes: &[&str]) -> Arc<dyn Stage> {
    Arc::new(FakeStage {
        id: id.to_string(),
        capabilities: capabilities_for(id),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: resources_for(id),
    })
}

fn registry_for(spec: &PipelineSpec, stages: Vec<Arc<dyn Stage>>) -> StageRegistry {
    let mut r = StageRegistry::new();
    for (s, st) in spec.stages.iter().zip(stages) {
        r.register(s.impl_key.clone(), st);
    }
    r
}

#[test]
fn bind_succeeds_when_rust_agrees() {
    let s = diamond();
    let g = s.validate().unwrap();
    let reg = registry_for(
        &s,
        vec![
            fake("source", &[]),
            fake("a", &["source"]),
            fake("b", &["source"]),
            fake("sink", &["a", "b"]),
        ],
    );
    let bound = bind(&s, &g, &reg).expect("binds");
    // Bound in topological order.
    assert_eq!(bound.first().unwrap().id(), "source");
    assert_eq!(bound.last().unwrap().id(), "sink");
}

#[test]
fn bind_rejects_consumes_disagreement() {
    let s = diamond();
    let g = s.validate().unwrap();
    // `a`'s Rust impl forgets it consumes `source`.
    let reg = registry_for(
        &s,
        vec![
            fake("source", &[]),
            fake("a", &[]),
            fake("b", &["source"]),
            fake("sink", &["a", "b"]),
        ],
    );
    match bind(&s, &g, &reg).map(|v| v.len()) {
        Ok(_) => panic!("expected consumes mismatch"),
        Err(d) => {
            let k = d
                .downcast_ref::<crate::error::ConsumesMismatch>()
                .unwrap_or_else(|| panic!("expected consumes mismatch, got {d}"));
            assert_eq!(k.stage, "a");
        }
    }
}

#[test]
fn bind_rejects_resource_disagreement() {
    // A Reason spec declares the engine resource, but its Rust impl forgets it —
    // bind HARD-fails, because a divergence would break the scheduler's
    // serialization (single source of truth).
    let s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("source", &[]),
            spec("r", &["source"]),
            spec("sink", &["r"]),
        ],
    };
    let g = s.validate().unwrap();
    let mut reg = StageRegistry::new();
    reg.register("impl:source".to_string(), fake("source", &[]));
    // The reason impl declares NO resources, disagreeing with the spec.
    reg.register(
        "impl:r".to_string(),
        Arc::new(FakeStage {
            id: "r".to_string(),
            capabilities: Vec::new(),
            consumes: vec!["source".to_string()],
            resources: Vec::new(),
        }) as Arc<dyn Stage>,
    );
    reg.register("impl:sink".to_string(), fake("sink", &["r"]));
    match bind(&s, &g, &reg).map(|v| v.len()) {
        Ok(_) => panic!("expected resource mismatch"),
        Err(d) => {
            let k = d
                .downcast_ref::<crate::error::ResourceMismatch>()
                .unwrap_or_else(|| panic!("expected resource mismatch, got {d}"));
            assert_eq!(k.stage, "r");
            assert_eq!(k.rdf, vec![ENGINE_RESOURCE.to_string()]);
            assert!(k.rust.is_empty());
        }
    }
}

#[test]
fn bind_rejects_unknown_impl() {
    let s = diamond();
    let g = s.validate().unwrap();
    let reg = StageRegistry::new(); // empty
    assert!(
        bind(&s, &g, &reg)
            .map(|v| v.len())
            .unwrap_err()
            .is::<crate::error::UnknownStageImpl>()
    );
}

// ── Dogfooded-DAG Turtle round-trip ──────────────────────────────────────────

const DAG_TTL: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:pipeline-test a gmeow:Pipeline ;
    gmeow:hasStage gmeow:stageSource , gmeow:stageReason , gmeow:stageSink .

gmeow:stageSource a gmeow:PipelineStage ;
    gmeow:hasCapability gmeow:sourceOrigin ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cachePersistentContribution ;
    gmeow:stageImpl "source_load" .

gmeow:stageReason a gmeow:PipelineStage ;
    gmeow:stageImpl "reason" ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cacheRecomputeAggregate ;
    gmeow:dataflowConsumes gmeow:stageSource ;
    gmeow:requiresResource gmeow:engineResource .

gmeow:stageSink a gmeow:PipelineStage ;
    gmeow:hasCapability gmeow:sinkCapability ;
    gmeow:stageImpl "gts_sink" ;
    gmeow:stageStability gmeow:stabilityStablePrefix ;
    gmeow:stageCacheDisposition gmeow:cachePersistentContribution ;
    gmeow:dataflowConsumes gmeow:stageReason ;
    gmeow:producesFormat "gts" .
"#;

#[test]
fn turtle_dag_round_trips_and_validates() {
    let spec = PipelineSpec::from_turtle(&[DAG_TTL]).expect("parses");
    assert_eq!(spec.id, "pipeline-test");
    assert_eq!(spec.stages.len(), 3);

    let reason = spec.stage("stageReason").expect("reason stage");
    assert!(reason.capabilities.is_empty());
    assert_eq!(reason.impl_key, "reason");
    assert_eq!(reason.resources, vec![ENGINE_RESOURCE.to_string()]);
    assert_eq!(reason.consumes, vec!["stageSource"]);

    let source = spec.stage("stageSource").expect("source stage");
    assert_eq!(source.capabilities, vec![SOURCE_ORIGIN.to_string()]);

    let sink = spec.stage("stageSink").expect("sink stage");
    assert_eq!(sink.capabilities, vec![SINK_CAPABILITY.to_string()]);
    assert_eq!(sink.formats, vec!["gts"]);

    let g = spec.validate().expect("DAG is valid");
    assert_eq!(g.order(), vec!["stageSource", "stageReason", "stageSink"]);
}

#[test]
fn stage_cache_declarations_fail_closed() {
    let missing = DAG_TTL.replacen(
        "    gmeow:stageCacheDisposition gmeow:cachePersistentContribution ;\n",
        "",
        1,
    );
    let err = PipelineSpec::from_turtle(&[&missing]).expect_err("missing disposition must fail");
    assert!(
        err.to_string()
            .contains("must declare exactly one gmeow:stageCacheDisposition; found 0")
    );

    let unknown = DAG_TTL.replacen("gmeow:cachePersistentContribution", "gmeow:cacheUnknown", 1);
    let err = PipelineSpec::from_turtle(&[&unknown]).expect_err("unknown disposition must fail");
    assert!(
        err.to_string()
            .contains("unknown gmeow:stageCacheDisposition value")
    );

    let multiple = DAG_TTL.replacen(
        "gmeow:stageCacheDisposition gmeow:cachePersistentContribution ;",
        "gmeow:stageCacheDisposition gmeow:cachePersistentContribution, gmeow:cacheRecomputeAggregate ;",
        1,
    );
    let err = PipelineSpec::from_turtle(&[&multiple]).expect_err("multiple dispositions must fail");
    assert!(
        err.to_string()
            .contains("must declare exactly one gmeow:stageCacheDisposition; found 2")
    );

    let missing_stability = DAG_TTL.replacen(
        "    gmeow:stageStability gmeow:stabilityStablePrefix ;\n",
        "",
        1,
    );
    let err =
        PipelineSpec::from_turtle(&[&missing_stability]).expect_err("missing stability must fail");
    assert!(
        err.to_string()
            .contains("must declare exactly one gmeow:stageStability; found 0")
    );
}

// ── Cache key ─────────────────────────────────────────────────────────────────

#[test]
fn stage_key_is_deterministic_and_structurally_sensitive() {
    let input = |producer: &str, entity: Option<&str>, digest: &str| StageInputDigest {
        producer: producer.to_string(),
        entity: entity.map(str::to_owned),
        digest: digest.to_string(),
    };
    let context = |version: &str, upstream: Vec<StageInputDigest>, raw_inputs| {
        StageKeyContext::new("s", version, upstream, raw_inputs)
    };
    let c1 = context(
        "v1",
        vec![input("left", None, "aa"), input("right", None, "bb")],
        Vec::new(),
    );
    let k1 = stage_key(&c1);
    let k2 = stage_key(&c1);
    assert_eq!(k1, k2, "same inputs → same key");

    let reordered = context(
        "v1",
        vec![input("right", None, "bb"), input("left", None, "aa")],
        Vec::new(),
    );
    assert_eq!(k1, stage_key(&reordered), "typed rows sort canonically");

    let swapped = context(
        "v1",
        vec![input("left", None, "bb"), input("right", None, "aa")],
        Vec::new(),
    );
    assert_ne!(
        k1,
        stage_key(&swapped),
        "swapping digests between producers changes the key"
    );
    let entity = context(
        "v1",
        vec![
            input("left", Some("http://example.org/entity"), "aa"),
            input("right", None, "bb"),
        ],
        Vec::new(),
    );
    assert_ne!(k1, stage_key(&entity), "entity marker changes the key");

    let k4 = stage_key(&context(
        "v2",
        vec![input("left", None, "aa"), input("right", None, "bb")],
        Vec::new(),
    ));
    assert_ne!(k1, k4, "impl version bump changes the key");

    let k5 = stage_key(&context(
        "v1",
        vec![input("left", None, "aa"), input("right", None, "bb")],
        vec![RawInputDigest {
            path: "source.ttl".to_string(),
            digest: "src".to_string(),
        }],
    ));
    assert_ne!(k1, k5, "source digest changes the key");
}

// ── P2: content-addressed self-verifying cache ───────────────────────────────

#[test]
fn cache_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let c = PipelineCache::open(dir.path().join("c")).unwrap();
    assert!(c.is_empty());
    let p = StageProduct::new("s", "abc123");
    let context = StageKeyContext::new("s", "v1", Vec::new(), Vec::new());
    c.put(
        &context,
        "stable",
        "persistent",
        &ReceiptOutputSelection::default(),
        &p,
    )
    .unwrap();
    assert_eq!(c.len(), 1);
    // `StageProduct`'s carrier (`Arc<PipelineBundle>`) has no value equality, so
    // compare by the persisted fields: id, digest, and the byte-artifact lane.
    let got = c
        .get(&context)
        .unwrap()
        .expect("cached product round-trips");
    assert_eq!(got.product.stage_id, p.stage_id);
    assert_eq!(got.product.digest, p.digest);
    assert_eq!(got.product.artifacts(), p.artifacts());
    let absent = StageKeyContext::new("absent", "v1", Vec::new(), Vec::new());
    assert!(c.get(&absent).unwrap().is_none());

    // Reopening the same dir recovers the immutable receipt (persistence).
    let c2 = PipelineCache::open(dir.path().join("c")).unwrap();
    assert_eq!(c2.get(&context).unwrap().unwrap().product.digest, "abc123");
}

#[test]
fn cache_hard_fails_on_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let cdir = dir.path().join("c");
    let c = PipelineCache::open(&cdir).unwrap();
    let context = StageKeyContext::new("s", "v1", Vec::new(), Vec::new());
    c.put(
        &context,
        "stable",
        "persistent",
        &ReceiptOutputSelection::default(),
        &StageProduct::new("s", "abc123"),
    )
    .unwrap();

    // Corrupt the blob: append a byte so its re-hash no longer matches the index.
    // The on-disk store lives under the version-segmented `v<CACHE_VERSION>` leaf.
    let blobs = cdir
        .join(format!("v{}", crate::cache::CACHE_VERSION))
        .join("blobs");
    for entry in std::fs::read_dir(&blobs).unwrap() {
        let path = entry.unwrap().path();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.push(b'!');
        std::fs::write(&path, bytes).unwrap();
    }
    // No silent repair — a corrupt entry is a hard failure.
    assert!(
        c.get(&context)
            .unwrap_err()
            .is::<crate::error::CacheMismatch>()
    );
}

// ── P2: provenance stamping ──────────────────────────────────────────────────

#[test]
fn provenance_stamps_capability_derived_origin() {
    use purrdf::provenance::{DatasetProvenance, OriginKind};
    let mut prov = DatasetProvenance::new();
    let source_caps = [SOURCE_ORIGIN.to_string()];
    let load = register_stage_unit(&mut prov, "stage-source-load", &source_caps);
    // A stage holding no SOURCE_ORIGIN capability (e.g. the reasoner) stamps Generated.
    let reason = register_stage_unit(&mut prov, "stage-reason", &[]);
    assert_eq!(prov.unit_kind(load), Some(&OriginKind::Source));
    assert_eq!(prov.unit_kind(reason), Some(&OriginKind::Generated));
    // Idempotent: re-registering the same id returns the same unit.
    let load2 = register_stage_unit(&mut prov, "stage-source-load", &source_caps);
    assert_eq!(load, load2);
}

// ── P2: scheduler — a stage that hashes its upstream (deterministic) ─────────

/// A synthetic stage whose product digest is a pure function of its id and its
/// (sorted) upstream digests, with a run counter to observe cache hits.
struct ComputeStage {
    id: String,
    capabilities: Vec<String>,
    consumes: Vec<String>,
    resources: Vec<String>,
    stability: StageStability,
    cache_policy: CachePolicy,
    runs: Arc<AtomicUsize>,
}

impl Stage for ComputeStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn resources(&self) -> &[String] {
        &self.resources
    }
    fn stability(&self) -> StageStability {
        self.stability
    }
    fn cache_policy(&self) -> CachePolicy {
        self.cache_policy
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let mut ups: Vec<String> = input.upstream.values().map(|p| p.digest.clone()).collect();
        ups.sort();
        let mut fields: Vec<&[u8]> = vec![self.id.as_bytes()];
        for u in &ups {
            fields.push(u.as_bytes());
        }
        let digest = crate::cache::content_digest(&fields);
        Ok(StageOutput::new(StageProduct::new(self.id.clone(), digest)))
    }
}

fn compute_registry(spec: &PipelineSpec, runs: &Arc<AtomicUsize>) -> StageRegistry {
    let mut r = StageRegistry::new();
    for s in &spec.stages {
        r.register(
            s.impl_key.clone(),
            Arc::new(ComputeStage {
                id: s.id.clone(),
                capabilities: s.capabilities.clone(),
                consumes: s.consumes.clone(),
                resources: s.resources.clone(),
                stability: s.stability,
                cache_policy: CachePolicy::Persistent,
                runs: Arc::clone(runs),
            }) as Arc<dyn Stage>,
        );
    }
    r
}

/// A diamond with a Reason node, exercising engine-resource serialization +
/// parallel levels.
fn reason_diamond() -> PipelineSpec {
    PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("source", &[]),
            spec("a", &["source"]),
            spec("r", &["source"]),
            spec("sink", &["a", "r"]),
        ],
    }
}

#[derive(Default)]
struct RecordingReporter {
    starts: Mutex<Vec<String>>,
    ends: Mutex<Vec<String>>,
}

impl Reporter for RecordingReporter {
    fn report(&self, _report: &Report) {}

    fn stage_start(&self, stage: &str) {
        self.starts.lock().unwrap().push(stage.to_string());
    }

    fn stage_end(&self, stage: &str, _elapsed: Duration) {
        self.ends.lock().unwrap().push(stage.to_string());
    }

    fn summary(&self, _report: &Report) {}
}

#[test]
fn scheduler_streams_each_stage_to_an_explicit_progress_sink() {
    let spec = reason_diamond();
    let graph = spec.validate().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let registry = compute_registry(&spec, &runs);
    let bound = bind(&spec, &graph, &registry).unwrap();
    let reporter = Arc::new(RecordingReporter::default());
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(dir.path(), 4)
        .unwrap()
        .with_progress(reporter.clone());

    run(&graph, &bound, &mut ctx).unwrap();

    let mut starts = reporter.starts.lock().unwrap().clone();
    let mut ends = reporter.ends.lock().unwrap().clone();
    starts.sort();
    ends.sort();
    assert_eq!(starts, vec!["a", "r", "sink", "source"]);
    assert_eq!(ends, starts, "every successful stage emits an end event");
}

#[test]
fn target_run_executes_exactly_the_declared_dependency_closure() {
    use std::collections::BTreeSet;

    let spec = reason_diamond();
    let graph = spec.validate().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let registry = compute_registry(&spec, &runs);
    let bound = bind(&spec, &graph, &registry).unwrap();
    let targets = BTreeSet::from(["a".to_string()]);
    assert_eq!(
        dependency_closure(&bound, &targets).unwrap(),
        BTreeSet::from(["a".to_string(), "source".to_string()])
    );

    let dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(dir.path(), 4).unwrap();
    let result = run_targets(&graph, &bound, &mut ctx, &targets).unwrap();
    assert_eq!(
        result.products.keys().cloned().collect::<BTreeSet<_>>(),
        dependency_closure(&bound, &targets).unwrap()
    );
    assert_eq!(runs.load(Ordering::SeqCst), 2);

    let unknown = BTreeSet::from(["ghost".to_string()]);
    assert!(dependency_closure(&bound, &unknown).is_err());
    assert!(dependency_closure(&bound, &BTreeSet::new()).is_err());
}

#[test]
fn scheduler_runs_diamond_and_caches() {
    let s = reason_diamond();
    let g = s.validate().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let reg = compute_registry(&s, &runs);
    let bound = bind(&s, &g, &reg).unwrap();

    let dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(dir.path(), 4).unwrap();

    let first = run(&g, &bound, &mut ctx).unwrap();
    assert_eq!(first.products.len(), 4);
    assert_eq!(
        runs.load(Ordering::SeqCst),
        4,
        "cold cache runs every stage"
    );

    // Provenance stamped one unit per stage.
    // Second run with the warm cache recomputes nothing.
    let second = run(&g, &bound, &mut ctx).unwrap();
    assert_eq!(
        runs.load(Ordering::SeqCst),
        4,
        "warm cache short-circuits every stage"
    );
    assert_eq!(
        first.combined_digest, second.combined_digest,
        "warm-cache run is identical"
    );
    assert_eq!(
        first.stage_receipts, second.stage_receipts,
        "hit/miss observations never enter immutable receipts"
    );
    assert_eq!(
        first.receipt_root, second.receipt_root,
        "cold and warm topological receipt roots are identical"
    );
}

#[test]
fn recompute_policy_reruns_the_stage_but_keeps_downstream_cache_hits() {
    let mut source = spec("source", &[]);
    source.cache_disposition = CachePolicy::Recompute;
    let spec = PipelineSpec {
        id: "recompute-policy".to_owned(),
        stages: vec![source, spec("sink", &["source"])],
    };
    let graph = spec.validate().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let mut registry = StageRegistry::new();
    for stage in &spec.stages {
        registry.register(
            stage.impl_key.clone(),
            Arc::new(ComputeStage {
                id: stage.id.clone(),
                capabilities: stage.capabilities.clone(),
                consumes: stage.consumes.clone(),
                resources: stage.resources.clone(),
                stability: stage.stability,
                cache_policy: if stage.id == "source" {
                    CachePolicy::Recompute
                } else {
                    CachePolicy::Persistent
                },
                runs: Arc::clone(&runs),
            }) as Arc<dyn Stage>,
        );
    }
    let bound = bind(&spec, &graph, &registry).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(dir.path(), 2).unwrap();

    let cold = run(&graph, &bound, &mut ctx).unwrap();
    let warm = run(&graph, &bound, &mut ctx).unwrap();

    assert_eq!(runs.load(Ordering::SeqCst), 3, "source reruns; sink hits");
    assert_eq!(cold.combined_digest, warm.combined_digest);
    assert_eq!(
        warm.stage_timings
            .iter()
            .map(|timing| (timing.stage_id.as_str(), timing.cached))
            .collect::<Vec<_>>(),
        vec![("source", false), ("sink", true)]
    );
}

#[test]
fn unstable_stage_never_reads_or_writes_the_persistent_cache() {
    let mut source = spec("source", &[]);
    source.stability = StageStability::PerTurnVariance;
    let spec = PipelineSpec {
        id: "unstable-policy".to_owned(),
        stages: vec![source, spec("sink", &["source"])],
    };
    let graph = spec.validate().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let registry = compute_registry(&spec, &runs);
    let bound = bind(&spec, &graph, &registry).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(dir.path(), 2).unwrap();

    let cold = run(&graph, &bound, &mut ctx).unwrap();
    let warm = run(&graph, &bound, &mut ctx).unwrap();

    assert_eq!(runs.load(Ordering::SeqCst), 3, "source reruns; sink hits");
    assert_eq!(cold.combined_digest, warm.combined_digest);
    assert_eq!(
        warm.stage_timings
            .iter()
            .map(|timing| {
                (
                    timing.stage_id.as_str(),
                    timing.cached,
                    timing.cache_outcome.as_str(),
                )
            })
            .collect::<Vec<_>>(),
        vec![("source", false, "bypass:unstable"), ("sink", true, "hit")]
    );
}

/// A leaf that reads a raw source file and declares it via `input_files` — its
/// cache key must reflect the file's CONTENT so an edit busts the cache.
struct FileReadingStage {
    file: std::path::PathBuf,
    runs: Arc<AtomicUsize>,
}

impl Stage for FileReadingStage {
    fn id(&self) -> &str {
        "file-leaf"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn input_files(
        &self,
        _root: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        Ok(vec![self.file.clone()])
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let bytes = std::fs::read(&self.file)?;
        Ok(StageOutput::new(StageProduct::new(
            "file-leaf",
            crate::cache::content_digest(&[&bytes]),
        )))
    }
}

#[test]
fn input_files_content_busts_the_cache() {
    // A leaf declaring `input_files` is served from cache while the file is
    // unchanged, and RE-RUNS (different cache key) once the file's bytes change —
    // the cache-soundness guarantee for source-reading leaves.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("source.txt");
    std::fs::write(&file, b"v1").unwrap();

    let runs = Arc::new(AtomicUsize::new(0));
    let spec = PipelineSpec {
        id: "p".to_string(),
        stages: vec![spec("file-leaf", &[]), spec("sink", &[])],
    };
    let graph = spec.validate().unwrap();
    let mut reg = StageRegistry::new();
    reg.register(
        "impl:file-leaf".to_string(),
        Arc::new(FileReadingStage {
            file: file.clone(),
            runs: Arc::clone(&runs),
        }) as Arc<dyn Stage>,
    );
    reg.register("impl:sink".to_string(), fake("sink", &[]));
    let bound = bind(&spec, &graph, &reg).unwrap();

    let mut ctx = RunContext::open(dir.path().join("cache"), 2).unwrap();
    run(&graph, &bound, &mut ctx).unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 1, "cold cache runs the leaf");

    // Same file → warm cache hit, no re-run.
    run(&graph, &bound, &mut ctx).unwrap();
    assert_eq!(
        runs.load(Ordering::SeqCst),
        1,
        "unchanged input file ⇒ cache hit"
    );

    // Edit the file → the input-files digest changes ⇒ the cache key changes ⇒ re-run.
    std::fs::write(&file, b"v2-changed").unwrap();
    run(&graph, &bound, &mut ctx).unwrap();
    assert_eq!(
        runs.load(Ordering::SeqCst),
        2,
        "a changed input file busts the cache"
    );
}

#[test]
fn scheduler_is_order_independent() {
    let s = reason_diamond();
    let g = s.validate().unwrap();

    // jobs=1 (sequential) vs jobs=8 (parallel), each with a FRESH cold cache so
    // every stage actually computes — the combined digest must be identical.
    let run_with = |jobs: usize| {
        let runs = Arc::new(AtomicUsize::new(0));
        let reg = compute_registry(&s, &runs);
        let bound = bind(&s, &g, &reg).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut ctx = RunContext::open(dir.path(), jobs).unwrap();
        run(&g, &bound, &mut ctx).unwrap().combined_digest
    };

    assert_eq!(
        run_with(1),
        run_with(8),
        "the combined digest is independent of completion order / parallelism"
    );
}

// ── P2: artifact-level (typed-dataflow) incremental rebuild ──────────────────

/// IRIs of the two named graphs the synthetic producer emits.
const G1: &str = "https://example.org/g1";
const G2: &str = "https://example.org/g2";

/// A producer whose dataset carries TWO named graphs: `g1`'s content tracks an input
/// file, `g2` is constant. Editing the file changes ONLY g1's canonical digest.
struct TwoGraphProducer {
    file: std::path::PathBuf,
    runs: Arc<AtomicUsize>,
    /// The two named graphs it attaches ([`G1`], [`G2`]) — declared so the scheduler's
    /// attach-drift check (delta == declaration) passes.
    attaches: Vec<String>,
}

impl Stage for TwoGraphProducer {
    fn id(&self) -> &str {
        "producer"
    }
    fn consumes(&self) -> &[String] {
        &[]
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.attaches
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn input_files(
        &self,
        _root: &std::path::Path,
    ) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        Ok(vec![self.file.clone()])
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let content = std::fs::read_to_string(&self.file)?;
        // g1's object tracks the file; g2 is constant. So an edit moves g1's digest
        // while g2's stays put — the artifact-level distinction the consumers exploit.
        let nq = format!(
            "<https://example.org/s> <https://example.org/p> \"{content}\" <{G1}> .\n\
             <https://example.org/s> <https://example.org/p> \"const\" <{G2}> .\n"
        );
        let dataset =
            purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("producer dataset: {e}"),
                })
            })?;
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            "producer",
            dataset,
            std::collections::BTreeMap::new(),
        )))
    }
}

/// A consumer that declares (via typed dataflow) it reads ONLY one named graph from
/// the producer. Its run counter shows whether it was recomputed.
struct EntityConsumer {
    id: String,
    entities: Vec<(String, Vec<String>)>,
    runs: Arc<AtomicUsize>,
}

impl Stage for EntityConsumer {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        std::slice::from_ref(&self.entities[0].0)
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        Ok(StageOutput::new(StageProduct::new(
            self.id.clone(),
            "deadbeef",
        )))
    }
}

#[test]
fn artifact_level_invalidation_reruns_only_the_changed_graphs_consumer() {
    // producer → {cg1 reads only g1, cg2 reads only g2} → sink.
    // Editing the file changes g1 (not g2): cg1 must re-run, cg2 must CACHE-HIT, even
    // though the producer itself re-ran and its WHOLE-product digest changed. This is
    // the artifact-level (not stage-level) invalidation the typed dataflow buys.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("g1-source.txt");
    std::fs::write(&file, b"v1").unwrap();

    let prod_runs = Arc::new(AtomicUsize::new(0));
    let cg1_runs = Arc::new(AtomicUsize::new(0));
    let cg2_runs = Arc::new(AtomicUsize::new(0));

    let mut s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("producer", &[]),
            spec("cg1", &["producer"]),
            spec("cg2", &["producer"]),
            spec("sink", &["cg1", "cg2"]),
        ],
    };
    // Declare the typed dataflow on the consumer specs (Rust/RDF agreement at bind).
    for st in &mut s.stages {
        if st.id == "cg1" {
            st.dataflow_entities = vec![("producer".to_string(), vec![G1.to_string()])];
        } else if st.id == "cg2" {
            st.dataflow_entities = vec![("producer".to_string(), vec![G2.to_string()])];
        } else if st.id == "producer" {
            // The producer attaches g1 + g2; declare them so the attach-drift check passes.
            st.attaches_graphs = vec![G1.to_string(), G2.to_string()];
        }
    }
    let g = s.validate().unwrap();

    let mut reg = StageRegistry::new();
    reg.register(
        "impl:producer".to_string(),
        Arc::new(TwoGraphProducer {
            file: file.clone(),
            runs: Arc::clone(&prod_runs),
            attaches: vec![G1.to_string(), G2.to_string()],
        }) as Arc<dyn Stage>,
    );
    reg.register(
        "impl:cg1".to_string(),
        Arc::new(EntityConsumer {
            id: "cg1".to_string(),
            entities: vec![("producer".to_string(), vec![G1.to_string()])],
            runs: Arc::clone(&cg1_runs),
        }) as Arc<dyn Stage>,
    );
    reg.register(
        "impl:cg2".to_string(),
        Arc::new(EntityConsumer {
            id: "cg2".to_string(),
            entities: vec![("producer".to_string(), vec![G2.to_string()])],
            runs: Arc::clone(&cg2_runs),
        }) as Arc<dyn Stage>,
    );
    reg.register("impl:sink".to_string(), fake("sink", &["cg1", "cg2"]));
    let bound = bind(&s, &g, &reg).expect("binds (resource + dataflow agreement hold)");

    let mut ctx = RunContext::open(dir.path().join("cache"), 4).unwrap();
    run(&g, &bound, &mut ctx).unwrap();
    assert_eq!(cg1_runs.load(Ordering::SeqCst), 1, "cold: cg1 runs");
    assert_eq!(cg2_runs.load(Ordering::SeqCst), 1, "cold: cg2 runs");

    // Warm, unchanged: nothing recomputes (determinism / soundness).
    run(&g, &bound, &mut ctx).unwrap();
    assert_eq!(cg1_runs.load(Ordering::SeqCst), 1, "warm: cg1 cache-hits");
    assert_eq!(cg2_runs.load(Ordering::SeqCst), 1, "warm: cg2 cache-hits");

    // Edit the file: g1 changes, g2 unchanged. The producer re-runs (its input-file
    // and whole digest change), cg1 re-runs (its consumed graph g1 changed), but cg2
    // CACHE-HITS — it depends only on g2, which is byte-identical.
    std::fs::write(&file, b"v2-different").unwrap();
    run(&g, &bound, &mut ctx).unwrap();
    assert!(
        prod_runs.load(Ordering::SeqCst) >= 2,
        "producer re-runs on the file edit"
    );
    assert_eq!(
        cg1_runs.load(Ordering::SeqCst),
        2,
        "artifact-level: cg1 re-runs because g1 (the graph it reads) changed"
    );
    assert_eq!(
        cg2_runs.load(Ordering::SeqCst),
        1,
        "artifact-level: cg2 CACHE-HITS — g2 is unchanged though the producer re-ran"
    );
}

// ── Attach-drift verification (gmeow:attachesGraph run-time contract) ─────────

/// A synthetic stage that ATTACHES `attach_graph` (a real named graph in its product)
/// when `Some`, and DECLARES `declared` via `attaches_graphs()`. The scheduler compares
/// the actual attach delta against the declaration and HARD-fails on any divergence.
struct AttachTestStage {
    id: String,
    attach_graph: Option<String>,
    declared: Vec<String>,
    consumes: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl Stage for AttachTestStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.declared
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        match &self.attach_graph {
            Some(g) => {
                let nq = format!("<https://example.org/s> <https://example.org/p> \"x\" <{g}> .\n");
                let dataset = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)
                    .map_err(|e| {
                        gmeow_errors::Diag::of_kind(crate::error::Parse {
                            message: format!("attach-test dataset: {e}"),
                        })
                    })?;
                Ok(StageOutput::new(StageProduct::from_artifacts_over(
                    self.id.clone(),
                    dataset,
                    std::collections::BTreeMap::new(),
                )))
            }
            None => Ok(StageOutput::new(StageProduct::new(
                self.id.clone(),
                "empty",
            ))),
        }
    }
}

/// Build a two-stage DAG (the attach-test producer → a fake sink) and RUN it, returning
/// the scheduler result so a test can assert the attach-drift HARD-fail (or clean pass).
fn run_attach_case(
    attach_graph: Option<&str>,
    declared: &[&str],
) -> Result<(), gmeow_errors::Diag> {
    let dir = tempfile::tempdir().unwrap();
    let declared: Vec<String> = declared.iter().map(|s| s.to_string()).collect();
    let mut s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![spec("producer", &[]), spec("sink", &["producer"])],
    };
    // The RDF/spec attach declaration must match the Rust impl for bind to pass — this
    // isolates the scheduler's RUN-TIME drift check from the loader's LOAD-time agreement.
    for st in &mut s.stages {
        if st.id == "producer" {
            st.attaches_graphs = declared.clone();
        }
    }
    let g = s.validate().unwrap();
    let mut reg = StageRegistry::new();
    reg.register(
        "impl:producer".to_string(),
        Arc::new(AttachTestStage {
            id: "producer".to_string(),
            attach_graph: attach_graph.map(|s| s.to_string()),
            declared: declared.clone(),
            consumes: Vec::new(),
            entities: Vec::new(),
        }) as Arc<dyn Stage>,
    );
    reg.register("impl:sink".to_string(), fake("sink", &["producer"]));
    let bound = bind(&s, &g, &reg).expect("binds (attach declaration agrees Rust↔spec)");
    let mut ctx = RunContext::open(dir.path().join("cache"), 2).unwrap();
    run(&g, &bound, &mut ctx).map(|_| ())
}

#[test]
fn attach_drift_hard_fails_when_declared_graph_is_not_attached() {
    // Declares it attaches G but attaches NOTHING → declared-but-not-attached drift.
    const G: &str = "https://example.org/graph/declared-not-attached";
    let err = run_attach_case(None, &[G]).expect_err("declared-but-not-attached must HARD-fail");
    assert_eq!(err.code(), crate::error::AttachDrift::register());
    assert!(
        format!("{err}").contains(&format!("declared-but-not-attached [{G:?}]")),
        "the drift diagnostic must report the unfulfilled declaration: {err}"
    );
}

#[test]
fn attach_drift_hard_fails_when_attached_graph_is_not_declared() {
    // Attaches G but declares NOTHING → attached-but-undeclared drift (the inverse).
    const G: &str = "https://example.org/graph/attached-not-declared";
    let err = run_attach_case(Some(G), &[]).expect_err("attached-but-undeclared must HARD-fail");
    assert_eq!(err.code(), crate::error::AttachDrift::register());
    assert!(
        format!("{err}").contains(&format!("attached-but-undeclared [{G:?}]")),
        "the drift diagnostic must report the undeclared attachment: {err}"
    );
}

#[test]
fn attach_drift_clean_when_declaration_matches_the_attach_delta() {
    // Attaches exactly G and declares exactly G → no drift, the run completes.
    const G: &str = "https://example.org/graph/matched";
    run_attach_case(Some(G), &[G]).expect("a matching attach declaration must not drift");
}

#[test]
fn attach_drift_honors_typed_entity_inputs() {
    // The producer carries G1 + G2, but the consumer's typed edge reads ONLY G1.
    // When the consumer emits G2, G2 is therefore a real attachment: its mere presence
    // elsewhere in the producer carrier must not hide the consumer's attach delta.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("typed-attach-source.txt");
    std::fs::write(&file, b"v1").unwrap();

    let mut s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("producer", &[]),
            spec("consumer", &["producer"]),
            spec("sink", &["consumer"]),
        ],
    };
    for st in &mut s.stages {
        match st.id.as_str() {
            "producer" => st.attaches_graphs = vec![G1.to_string(), G2.to_string()],
            "consumer" => {
                st.dataflow_entities = vec![("producer".to_string(), vec![G1.to_string()])];
                st.attaches_graphs = vec![G2.to_string()];
            }
            _ => {}
        }
    }
    let graph = s.validate().unwrap();

    let mut registry = StageRegistry::new();
    registry.register(
        "impl:producer".to_string(),
        Arc::new(TwoGraphProducer {
            file,
            runs: Arc::new(AtomicUsize::new(0)),
            attaches: vec![G1.to_string(), G2.to_string()],
        }) as Arc<dyn Stage>,
    );
    registry.register(
        "impl:consumer".to_string(),
        Arc::new(AttachTestStage {
            id: "consumer".to_string(),
            attach_graph: Some(G2.to_string()),
            declared: vec![G2.to_string()],
            consumes: vec!["producer".to_string()],
            entities: vec![("producer".to_string(), vec![G1.to_string()])],
        }) as Arc<dyn Stage>,
    );
    registry.register("impl:sink".to_string(), fake("sink", &["consumer"]));
    let bound = bind(&s, &graph, &registry).expect("typed attach fixture binds");
    let mut ctx = RunContext::open(dir.path().join("cache"), 2).unwrap();

    run(&graph, &bound, &mut ctx)
        .expect("typed entity narrowing exposes the consumer's real G2 attachment");
}

/// A synthetic producer/consumer that attaches one content-addressed record under a
/// shared blob-representation label. Different payloads must be distinct attachments
/// even when an upstream product already carries that representation label.
struct AttachBlobStage {
    id: String,
    consumes: Vec<String>,
    representation: String,
    payload: Vec<u8>,
    declared: Vec<String>,
}

impl Stage for AttachBlobStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn attaches_blob_reps(&self) -> &[String] {
        &self.declared
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        let dataset = purrdf::parse_dataset(b"", "application/n-quads", None).map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("attach-blob empty dataset: {e}"),
            })
        })?;
        let bundle = crate::bundle::bundle_from_artifacts_over_with_rep_blob(
            dataset,
            std::collections::BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
            &self.representation,
            "application/octet-stream",
            self.payload.clone(),
        );
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id.clone(),
            Arc::new(bundle),
        )))
    }
}

#[test]
fn attach_drift_distinguishes_shared_blob_rep_by_content() {
    const REP: &str = "diagnostics:nodes";
    let dir = tempfile::tempdir().unwrap();
    let mut s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("producer", &[]),
            spec("consumer", &["producer"]),
            spec("sink", &["consumer"]),
        ],
    };
    for st in &mut s.stages {
        if st.id == "producer" || st.id == "consumer" {
            st.attaches_blob_reps = vec![REP.to_string()];
        }
    }
    let graph = s.validate().unwrap();

    let stage = |id: &str, consumes: &[&str], payload: &[u8]| {
        Arc::new(AttachBlobStage {
            id: id.to_string(),
            consumes: consumes
                .iter()
                .map(|producer| (*producer).to_string())
                .collect(),
            representation: REP.to_string(),
            payload: payload.to_vec(),
            declared: vec![REP.to_string()],
        }) as Arc<dyn Stage>
    };
    let mut registry = StageRegistry::new();
    registry.register(
        "impl:producer".to_string(),
        stage("producer", &[], b"compile"),
    );
    registry.register(
        "impl:consumer".to_string(),
        stage("consumer", &["producer"], b"reason"),
    );
    registry.register("impl:sink".to_string(), fake("sink", &["consumer"]));
    let bound = bind(&s, &graph, &registry).expect("shared blob-rep fixture binds");
    let mut ctx = RunContext::open(dir.path().join("cache"), 2).unwrap();

    run(&graph, &bound, &mut ctx)
        .expect("content-distinct records on one representation lane are both attachments");
}

// ── Bounded carrier retention (drop-after-last-consumer, generalized) ─────────

/// The shared registry the retention fixture uses to OBSERVE, live and mid-run,
/// whether a stage's carrier dataset is still resident: each stage publishes a
/// [`std::sync::Weak`] to the `Arc<RdfDataset>` it emitted, and every later stage
/// records which of those weaks still upgrade at the moment it runs.
#[derive(Default)]
struct CarrierWatch {
    /// stage id → weak handle on the dataset that stage's product carries.
    published: Mutex<std::collections::BTreeMap<String, std::sync::Weak<purrdf::RdfDataset>>>,
    /// stage id → the set of stage ids whose dataset was STILL LIVE when it ran.
    observed: Mutex<std::collections::BTreeMap<String, std::collections::BTreeSet<String>>>,
}

impl CarrierWatch {
    /// The ids whose published dataset is still resident right now.
    fn live(&self) -> std::collections::BTreeSet<String> {
        self.published
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, weak)| weak.upgrade().is_some())
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// A synthetic stage that emits a REAL carrier exercising all three lanes
/// [`crate::bundle::release_carrier`] treats differently: one named graph of its own,
/// one COMMITTED byte artifact, and one INTERNAL `pipeline/`-prefixed byte artifact.
struct CarrierStage {
    id: String,
    empty: Vec<String>,
    capabilities: Vec<String>,
    consumes: Vec<String>,
    attaches: Vec<String>,
    watch: Arc<CarrierWatch>,
}

/// The named graph `id`'s carrier stage attaches.
fn watch_graph(id: &str) -> String {
    format!("https://example.org/retention/graph/{id}")
}

/// The one quad `id`'s carrier stage emits, in its own named graph.
fn watch_nquads(id: &str) -> String {
    format!(
        "<https://example.org/retention/s/{id}> <https://example.org/retention/p> \
         <https://example.org/retention/o> <{}> .\n",
        watch_graph(id)
    )
}

impl Stage for CarrierStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn capabilities(&self) -> &[String] {
        &self.capabilities
    }
    fn resources(&self) -> &[String] {
        &self.empty
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.attaches
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn cache_policy(&self) -> CachePolicy {
        CachePolicy::Recompute
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Record what is STILL RESIDENT at this stage's execution instant — the live
        // half of the bounded-retention proof.
        self.watch
            .observed
            .lock()
            .unwrap()
            .insert(self.id.clone(), self.watch.live());
        // Read every declared upstream so the consumes edge is a real read, not a
        // decoration: a stage that never touches its upstream would make the drop
        // trivially safe and prove nothing.
        for dep in &self.consumes {
            let up = input
                .upstream
                .get(dep)
                .expect("declared upstream product is present");
            assert!(
                !up.carrier_released,
                "stage {} read upstream {dep} whose carrier was already released",
                self.id
            );
            assert_eq!(
                up.dataset().owned_quads().count(),
                1,
                "upstream {dep} still carries its quad when its consumer runs"
            );
        }
        let nq = watch_nquads(&self.id);
        let dataset = purrdf::parse_dataset(nq.as_bytes(), "application/n-quads", None)?;
        self.watch
            .published
            .lock()
            .unwrap()
            .insert(self.id.clone(), Arc::downgrade(&dataset));
        let artifacts = std::collections::BTreeMap::from([
            (
                format!("generated/retention/{}.txt", self.id),
                self.id.clone().into_bytes(),
            ),
            (format!("pipeline/{}.nq", self.id), nq.into_bytes()),
        ]);
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id.clone(),
            dataset,
            artifacts,
        )))
    }
}

/// A four-level CHAIN — `source → a → b → late` — so each stage's last consumer sits
/// in a different level and the retention schedule is a non-trivial function of the
/// levelling. `late` is the only stage nothing consumes.
fn retention_chain() -> PipelineSpec {
    let carrier_spec = |id: &str, consumes: &[&str]| StageSpec {
        id: id.to_string(),
        // `late` is the chain's terminal: the DAG validator requires exactly one Sink.
        capabilities: capabilities_for(if id == "late" { "sink" } else { id }),
        impl_key: format!("impl:{id}"),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        resources: Vec::new(),
        stability: StageStability::StablePrefix,
        // This fixture isolates carrier lifetime. Its Rust twin intentionally
        // recomputes, so the RDF half must make the same declaration and let the
        // binding parity gate remain load-bearing.
        cache_disposition: CachePolicy::Recompute,
        dataflow_entities: Vec::new(),
        formats: Vec::new(),
        attaches_graphs: vec![watch_graph(id)],
        attaches_blob_reps: Vec::new(),
    };
    PipelineSpec {
        id: "retention".to_string(),
        stages: vec![
            carrier_spec("source", &[]),
            carrier_spec("a", &["source"]),
            carrier_spec("b", &["a"]),
            carrier_spec("late", &["b"]),
        ],
    }
}

fn retention_registry(spec: &PipelineSpec, watch: &Arc<CarrierWatch>) -> StageRegistry {
    let mut r = StageRegistry::new();
    for s in &spec.stages {
        r.register(
            s.impl_key.clone(),
            Arc::new(CarrierStage {
                id: s.id.clone(),
                empty: Vec::new(),
                capabilities: s.capabilities.clone(),
                consumes: s.consumes.clone(),
                attaches: s.attaches_graphs.clone(),
                watch: Arc::clone(watch),
            }) as Arc<dyn Stage>,
        );
    }
    r
}

/// Run the retention chain under `retention`, returning the run result and the watch.
fn run_retention_chain(
    retention: crate::scheduler::CarrierRetention,
) -> (crate::scheduler::RunResult, Arc<CarrierWatch>) {
    let dir = tempfile::tempdir().unwrap();
    let spec = retention_chain();
    let graph = spec.validate().expect("retention chain validates");
    let watch = Arc::new(CarrierWatch::default());
    let bound = bind(&spec, &graph, &retention_registry(&spec, &watch)).expect("binds");
    let mut ctx = RunContext::open_uncached(dir.path(), 4);
    ctx.carrier_retention = retention;
    let result = run(&graph, &bound, &mut ctx).expect("retention chain runs");
    (result, watch)
}

#[test]
fn last_consumer_level_is_total_over_consumed_stages_and_absent_for_outputs() {
    let spec = retention_chain();
    let graph = spec.validate().unwrap();
    let watch = Arc::new(CarrierWatch::default());
    let bound = bind(&spec, &graph, &retention_registry(&spec, &watch)).unwrap();
    let by_id: std::collections::BTreeMap<&str, &Arc<dyn Stage>> =
        bound.iter().map(|s| (s.id(), s)).collect();

    let levels = crate::scheduler::last_consumer_levels(&graph, &by_id);
    assert_eq!(
        levels,
        std::collections::BTreeMap::from([
            ("source".to_string(), 1),
            ("a".to_string(), 2),
            ("b".to_string(), 3),
        ]),
        "each consumed stage's drop point is the level of its LAST consumer"
    );
    assert!(
        !levels.contains_key("late"),
        "a stage nothing consumes is a run OUTPUT and has no drop point"
    );
}

#[test]
fn carrier_retention_is_bounded_by_the_live_frontier() {
    let (result, watch) =
        run_retention_chain(crate::scheduler::CarrierRetention::DropAfterLastConsumer);

    // ── The LIVE bound: at every stage's execution instant, the only carriers still
    //    resident are those with a consumer that has not yet run. Recorded from inside
    //    the run, so it pins the drop's TIMING, not merely its end state. A regression
    //    to the one-off `stage-source-load` span special case reds here: `source` would
    //    still be live when `b` runs and `a` when `late` runs.
    let observed = watch.observed.lock().unwrap().clone();
    assert_eq!(
        observed.get("a").cloned().unwrap_or_default(),
        std::collections::BTreeSet::from(["source".to_string()]),
        "when `a` runs, only its own upstream `source` has been published and it is live"
    );
    assert_eq!(
        observed.get("b").cloned().unwrap_or_default(),
        std::collections::BTreeSet::from(["a".to_string()]),
        "when `b` runs, `source` (last consumer `a`, level 1) is ALREADY released"
    );
    assert_eq!(
        observed.get("late").cloned().unwrap_or_default(),
        std::collections::BTreeSet::from(["b".to_string()]),
        "when `late` runs, both `source` and `a` are released; only `b` is still live"
    );

    // ── The end state: exactly the consumed stages are released; the output is not.
    let released: std::collections::BTreeSet<&str> = result
        .products
        .values()
        .filter(|p| p.carrier_released)
        .map(|p| p.stage_id.as_str())
        .collect();
    assert_eq!(
        released,
        std::collections::BTreeSet::from(["source", "a", "b"]),
        "every stage with a consumer is released; a run output never is"
    );
    assert_eq!(
        watch.live(),
        std::collections::BTreeSet::from(["late".to_string()]),
        "only the run output's dataset survives the run"
    );

    // ── What a released product still owes the post-run reconcile, and what it does not.
    for id in ["source", "a", "b"] {
        let product = &result.products[id];
        assert_eq!(
            product.dataset().owned_quads().count(),
            0,
            "{id}: the frozen dataset is released"
        );
        assert!(
            product.artifact(&format!("pipeline/{id}.nq")).is_none(),
            "{id}: the INTERNAL dataflow artifact is released"
        );
        assert_eq!(
            product.artifact(&format!("generated/retention/{id}.txt")),
            Some(id.as_bytes()),
            "{id}: the COMMITTED artifact survives for the reconcile, byte-exact"
        );
    }
    let late = &result.products["late"];
    assert_eq!(
        late.dataset().owned_quads().count(),
        1,
        "the run output keeps its whole carrier"
    );
    assert!(
        late.artifact("pipeline/late.nq").is_some(),
        "the run output keeps even its internal artifacts"
    );
}

#[test]
fn releasing_carriers_changes_no_byte_the_run_produces() {
    let (dropped, _) =
        run_retention_chain(crate::scheduler::CarrierRetention::DropAfterLastConsumer);
    let (retained, retained_watch) =
        run_retention_chain(crate::scheduler::CarrierRetention::RetainAll);

    // The SAME fixture under RetainAll observes the opposite: every published carrier
    // is still live at every later stage's instant, and all four survive the run. This
    // is what makes the drop profile's live-frontier assertions load-bearing rather
    // than an artefact of the fixture (a weak that died for some other reason).
    let retained_observed = retained_watch.observed.lock().unwrap().clone();
    assert_eq!(
        retained_observed.get("late").cloned().unwrap_or_default(),
        std::collections::BTreeSet::from(["source".to_string(), "a".to_string(), "b".to_string()]),
        "under RetainAll every upstream carrier is still resident when `late` runs"
    );
    assert_eq!(
        retained_watch.live(),
        std::collections::BTreeSet::from([
            "source".to_string(),
            "a".to_string(),
            "b".to_string(),
            "late".to_string(),
        ]),
        "under RetainAll every carrier survives the whole run"
    );

    assert_eq!(
        dropped.combined_digest, retained.combined_digest,
        "the determinism witness is identical under both retention profiles"
    );
    assert!(
        retained.products.values().all(|p| !p.carrier_released),
        "RetainAll releases nothing"
    );
    for (id, retained_product) in &retained.products {
        let dropped_product = &dropped.products[id];
        assert_eq!(
            dropped_product.digest, retained_product.digest,
            "{id}: a released product keeps its digest verbatim — it is the identity \
             witness of the carrier that was released, never a fold over the residue"
        );
        let committed = |p: &StageProduct| -> std::collections::BTreeMap<String, Vec<u8>> {
            p.artifacts()
                .into_iter()
                .filter(|(path, _)| !path.starts_with(crate::bundle::INTERNAL_ARTIFACT_PREFIX))
                .collect()
        };
        assert_eq!(
            committed(dropped_product),
            committed(retained_product),
            "{id}: every committed artifact byte survives the release"
        );
    }
}
