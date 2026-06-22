// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! P1 unit tests: DAG validation (cycle / completeness / sink / engine-lock),
//! registry binding agreement, and the dogfooded-DAG Turtle round-trip.

use std::sync::Arc;

use crate::cache::stage_key;
use crate::error::PipelineError;
use crate::loader::{bind, PipelineSpec, StageSpec};
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::registry::StageRegistry;

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
