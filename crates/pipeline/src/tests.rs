// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Unit tests. P1: DAG validation (cycle / completeness / sink / engine-lock),
//! registry binding agreement, the dogfooded-DAG Turtle round-trip. P2: the
//! self-verifying cache, provenance stamping, and scheduler determinism.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::cache::{stage_key, PipelineCache};
use crate::error::PipelineError;
use crate::loader::{bind, PipelineSpec, StageSpec};
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::provenance::register_stage_unit;
use crate::registry::StageRegistry;
use crate::scheduler::{run, RunContext};

fn spec(id: &str, kind: StageKind, consumes: &[&str]) -> StageSpec {
    StageSpec {
        id: id.to_string(),
        kind,
        impl_key: format!("impl:{id}"),
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
        engine_lock: kind.carries_engine_lock(),
        formats: Vec::new(),
    }
}

/// A diamond: source → (a, b) → sink.
fn diamond() -> PipelineSpec {
    PipelineSpec {
        id: "pipeline-build".to_string(),
        stages: vec![
            spec("source", StageKind::SourceLoad, &[]),
            spec("a", StageKind::Transform, &["source"]),
            spec("b", StageKind::Transform, &["source"]),
            spec("sink", StageKind::Sink, &["a", "b"]),
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
        Err(PipelineError::InvalidDag(msg)) => assert!(msg.contains("cycle"), "{msg}"),
        other => panic!("expected cycle rejection, got {other:?}"),
    }
}

#[test]
fn dangling_dependency_is_rejected() {
    let mut s = diamond();
    s.stages[1].consumes = vec!["ghost".to_string()];
    match s.validate() {
        Err(PipelineError::InvalidDag(msg)) => assert!(msg.contains("ghost"), "{msg}"),
        other => panic!("expected dangling-dependency rejection, got {other:?}"),
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
        Err(PipelineError::InvalidDag(msg)) => {
            assert!(msg.contains("ghost"), "{msg}");
        }
        other => panic!("expected InvalidDag for an unknown consumer key, got {other:?}"),
    }
}

#[test]
fn missing_sink_is_rejected() {
    let mut s = diamond();
    s.stages[3].kind = StageKind::ExportLeaf; // demote the only sink
    match s.validate() {
        Err(PipelineError::InvalidDag(msg)) => assert!(msg.contains("no Sink"), "{msg}"),
        other => panic!("expected missing-sink rejection, got {other:?}"),
    }
}

#[test]
fn multiple_sinks_are_rejected() {
    let mut s = diamond();
    s.stages[1].kind = StageKind::Sink; // now `a` and `sink` are both sinks
    match s.validate() {
        Err(PipelineError::InvalidDag(msg)) => assert!(msg.contains("Sink stages"), "{msg}"),
        other => panic!("expected multiple-sink rejection, got {other:?}"),
    }
}

#[test]
fn engine_lock_must_equal_kind_derivation() {
    let mut s = diamond();
    // Lie: a Transform claims it carries the engine lock.
    s.stages[1].engine_lock = true;
    match s.validate() {
        Err(PipelineError::EngineLockMismatch {
            stage,
            rdf,
            derived,
        }) => {
            assert_eq!(stage, "a");
            assert!(rdf);
            assert!(!derived);
        }
        other => panic!("expected engine-lock mismatch, got {other:?}"),
    }
}

#[test]
fn reason_stage_engine_lock_is_consistent() {
    // A Reason stage with engine_lock=true validates; with false it fails.
    let mut s = PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("source", StageKind::SourceLoad, &[]),
            spec("r", StageKind::Reason, &["source"]),
            spec("sink", StageKind::Sink, &["r"]),
        ],
    };
    assert!(s.validate().is_ok());
    s.stages[1].engine_lock = false;
    assert!(matches!(
        s.validate(),
        Err(PipelineError::EngineLockMismatch { .. })
    ));
}

// ── Binding agreement ────────────────────────────────────────────────────────

struct FakeStage {
    id: String,
    kind: StageKind,
    consumes: Vec<String>,
}

impl Stage for FakeStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> StageKind {
        self.kind
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        Ok(StageOutput {
            product: StageProduct::new(self.id.clone(), "deadbeef"),
        })
    }
}

fn fake(id: &str, kind: StageKind, consumes: &[&str]) -> Arc<dyn Stage> {
    Arc::new(FakeStage {
        id: id.to_string(),
        kind,
        consumes: consumes.iter().map(|s| s.to_string()).collect(),
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
            fake("source", StageKind::SourceLoad, &[]),
            fake("a", StageKind::Transform, &["source"]),
            fake("b", StageKind::Transform, &["source"]),
            fake("sink", StageKind::Sink, &["a", "b"]),
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
            fake("source", StageKind::SourceLoad, &[]),
            fake("a", StageKind::Transform, &[]),
            fake("b", StageKind::Transform, &["source"]),
            fake("sink", StageKind::Sink, &["a", "b"]),
        ],
    );
    match bind(&s, &g, &reg).map(|v| v.len()) {
        Err(PipelineError::ConsumesMismatch { stage, .. }) => assert_eq!(stage, "a"),
        other => panic!("expected consumes mismatch, got {other:?}"),
    }
}

#[test]
fn bind_rejects_unknown_impl() {
    let s = diamond();
    let g = s.validate().unwrap();
    let reg = StageRegistry::new(); // empty
    assert!(matches!(
        bind(&s, &g, &reg).map(|v| v.len()),
        Err(PipelineError::UnknownStageImpl { .. })
    ));
}

// ── Dogfooded-DAG Turtle round-trip ──────────────────────────────────────────

const DAG_TTL: &str = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .

gmeow:pipeline-test a gmeow:Pipeline ;
    gmeow:hasStage gmeow:stageSource , gmeow:stageReason , gmeow:stageSink .

gmeow:stageSource a gmeow:PipelineStage ;
    gmeow:stageKind gmeow:kindSourceLoad ;
    gmeow:stageImpl "source_load" ;
    gmeow:carriesEngineLock false .

gmeow:stageReason a gmeow:PipelineStage ;
    gmeow:stageKind gmeow:kindReason ;
    gmeow:stageImpl "reason" ;
    gmeow:dataflowConsumes gmeow:stageSource ;
    gmeow:carriesEngineLock true .

gmeow:stageSink a gmeow:PipelineStage ;
    gmeow:stageKind gmeow:kindSink ;
    gmeow:stageImpl "gts_sink" ;
    gmeow:dataflowConsumes gmeow:stageReason ;
    gmeow:producesFormat "gts" ;
    gmeow:carriesEngineLock false .
"#;

#[test]
fn turtle_dag_round_trips_and_validates() {
    let spec = PipelineSpec::from_turtle(&[DAG_TTL]).expect("parses");
    assert_eq!(spec.id, "pipeline-test");
    assert_eq!(spec.stages.len(), 3);

    let reason = spec.stage("stageReason").expect("reason stage");
    assert_eq!(reason.kind, StageKind::Reason);
    assert_eq!(reason.impl_key, "reason");
    assert!(reason.engine_lock);
    assert_eq!(reason.consumes, vec!["stageSource"]);

    let sink = spec.stage("stageSink").expect("sink stage");
    assert_eq!(sink.formats, vec!["gts"]);

    let g = spec.validate().expect("DAG is valid");
    assert_eq!(g.order(), vec!["stageSource", "stageReason", "stageSink"]);
}

// ── Cache key ─────────────────────────────────────────────────────────────────

#[test]
fn stage_key_is_deterministic_and_order_sensitive() {
    let k1 = stage_key("s", "v1", &["aa".into(), "bb".into()], None);
    let k2 = stage_key("s", "v1", &["aa".into(), "bb".into()], None);
    assert_eq!(k1, k2, "same inputs → same key");

    let k3 = stage_key("s", "v1", &["bb".into(), "aa".into()], None);
    assert_ne!(k1, k3, "upstream digest order matters (caller sorts)");

    let k4 = stage_key("s", "v2", &["aa".into(), "bb".into()], None);
    assert_ne!(k1, k4, "impl version bump changes the key");

    let k5 = stage_key("s", "v1", &["aa".into(), "bb".into()], Some("src"));
    assert_ne!(k1, k5, "source digest changes the key");
}

// ── P2: content-addressed self-verifying cache ───────────────────────────────

#[test]
fn cache_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = PipelineCache::open(dir.path().join("c")).unwrap();
    assert!(c.is_empty());
    let p = StageProduct::new("s", "abc123");
    c.put("key1", &p).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c.get("key1").unwrap(), Some(p));
    assert_eq!(c.get("absent").unwrap(), None);

    // Reopening the same dir recovers the index (persistence).
    let c2 = PipelineCache::open(dir.path().join("c")).unwrap();
    assert_eq!(c2.get("key1").unwrap().unwrap().digest, "abc123");
}

#[test]
fn cache_hard_fails_on_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let cdir = dir.path().join("c");
    let mut c = PipelineCache::open(&cdir).unwrap();
    c.put("key1", &StageProduct::new("s", "abc123")).unwrap();

    // Corrupt the blob: append a byte so its re-hash no longer matches the index.
    let blobs = cdir.join("blobs");
    for entry in std::fs::read_dir(&blobs).unwrap() {
        let path = entry.unwrap().path();
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.push(b'!');
        std::fs::write(&path, bytes).unwrap();
    }
    // No silent repair — a corrupt entry is a hard failure.
    assert!(matches!(
        c.get("key1"),
        Err(PipelineError::CacheMismatch { .. })
    ));
}

// ── P2: provenance stamping ──────────────────────────────────────────────────

#[test]
fn provenance_stamps_kind_derived_origin() {
    use gmeow_rdf::provenance::{DatasetProvenance, OriginKind};
    let mut prov = DatasetProvenance::new();
    let load = register_stage_unit(&mut prov, "stage-source-load", StageKind::SourceLoad);
    let reason = register_stage_unit(&mut prov, "stage-reason", StageKind::Reason);
    assert_eq!(prov.unit_kind(load), Some(&OriginKind::Source));
    assert_eq!(prov.unit_kind(reason), Some(&OriginKind::Generated));
    // Idempotent: re-registering the same id returns the same unit.
    let load2 = register_stage_unit(&mut prov, "stage-source-load", StageKind::SourceLoad);
    assert_eq!(load, load2);
}

// ── P2: scheduler — a stage that hashes its upstream (deterministic) ─────────

/// A synthetic stage whose product digest is a pure function of its id and its
/// (sorted) upstream digests, with a run counter to observe cache hits.
struct ComputeStage {
    id: String,
    kind: StageKind,
    consumes: Vec<String>,
    runs: Arc<AtomicUsize>,
}

impl Stage for ComputeStage {
    fn id(&self) -> &str {
        &self.id
    }
    fn kind(&self) -> StageKind {
        self.kind
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let mut ups: Vec<String> = input.upstream.values().map(|p| p.digest.clone()).collect();
        ups.sort();
        let mut fields: Vec<&[u8]> = vec![self.id.as_bytes()];
        for u in &ups {
            fields.push(u.as_bytes());
        }
        let digest = crate::cache::content_digest(&fields);
        Ok(StageOutput {
            product: StageProduct::new(self.id.clone(), digest),
        })
    }
}

fn compute_registry(spec: &PipelineSpec, runs: &Arc<AtomicUsize>) -> StageRegistry {
    let mut r = StageRegistry::new();
    for s in &spec.stages {
        r.register(
            s.impl_key.clone(),
            Arc::new(ComputeStage {
                id: s.id.clone(),
                kind: s.kind,
                consumes: s.consumes.clone(),
                runs: Arc::clone(runs),
            }) as Arc<dyn Stage>,
        );
    }
    r
}

/// A diamond with a Reason node, exercising ENGINE_LOCK + parallel levels.
fn reason_diamond() -> PipelineSpec {
    PipelineSpec {
        id: "p".to_string(),
        stages: vec![
            spec("source", StageKind::SourceLoad, &[]),
            spec("a", StageKind::Transform, &["source"]),
            spec("r", StageKind::Reason, &["source"]),
            spec("sink", StageKind::Sink, &["a", "r"]),
        ],
    }
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
}

/// A leaf that reads a raw source file and declares it via `input_files` — its
/// cache key must reflect the file's CONTENT so an edit busts the cache (#863).
struct FileReadingStage {
    file: std::path::PathBuf,
    runs: Arc<AtomicUsize>,
}

impl Stage for FileReadingStage {
    fn id(&self) -> &str {
        "file-leaf"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
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
    ) -> Result<Vec<std::path::PathBuf>, PipelineError> {
        Ok(vec![self.file.clone()])
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        let bytes = std::fs::read(&self.file)?;
        Ok(StageOutput {
            product: StageProduct::new("file-leaf", crate::cache::content_digest(&[&bytes])),
        })
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
        stages: vec![
            spec("file-leaf", StageKind::ExportLeaf, &[]),
            spec("sink", StageKind::Sink, &[]),
        ],
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
    reg.register("impl:sink".to_string(), fake("sink", StageKind::Sink, &[]));
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
