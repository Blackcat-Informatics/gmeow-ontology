// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DAG loader: read the dogfooded build graph from `gmeow:` individuals,
//! validate it, and bind each stage to its Rust implementation (#861).
//!
//! The build DAG is authored as data — `gmeow:Pipeline` / `gmeow:PipelineStage`
//! individuals in `slices/core/pipeline/` — and read back here, so the build is
//! a first-class ontological citizen (MAXIMAL DOGFOODING). [`PipelineSpec`] is
//! the parsed-but-unbound shape; [`PipelineSpec::validate`] proves the structure
//! (acyclic, complete, exactly one sink, engine-lock derived); [`bind`] resolves
//! each `gmeow:stageImpl` against the [`StageRegistry`] and proves the Rust impl
//! agrees with the RDF declaration (kind + consumes). Every check HARD-fails
//! before any stage runs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_rdf::{
    parse_dataset, DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, TermRef, TermValue,
};

use crate::error::PipelineError;
use crate::graph::StageGraph;
use crate::node::{Stage, StageKind, GMEOW};
use crate::registry::StageRegistry;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// An RDF object term surfaced from the native IR (the oxigraph-free replacement
/// for the `oxigraph::model::Term` discrimination the loader relied on).
enum ObjTerm {
    /// An IRI object.
    Named(String),
    /// A literal object, by its lexical value.
    Literal(String),
    /// Any other term kind (blank / triple) — the loader never reads inside one.
    Other,
}

/// One `gmeow:PipelineStage` individual, parsed but not yet bound to its impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSpec {
    /// The stage id — the `gmeow:PipelineStage` individual's local name.
    pub id: String,
    /// `gmeow:stageKind` resolved to a [`StageKind`].
    pub kind: StageKind,
    /// `gmeow:stageImpl` — the registry key binding this to a Rust [`Stage`].
    pub impl_key: String,
    /// `gmeow:dataflowConsumes` — upstream stage ids, sorted, deduplicated.
    pub consumes: Vec<String>,
    /// `gmeow:carriesEngineLock` — validated to equal `kind.carries_engine_lock()`.
    pub engine_lock: bool,
    /// `gmeow:producesFormat` — output format tags, sorted (export leaves).
    pub formats: Vec<String>,
}

/// A parsed `gmeow:Pipeline` individual: its id plus its stages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipelineSpec {
    /// The pipeline id — the `gmeow:Pipeline` individual's local name.
    pub id: String,
    /// The stages, sorted by id for determinism.
    pub stages: Vec<StageSpec>,
}

impl PipelineSpec {
    /// Look up a stage by id.
    pub fn stage(&self, id: &str) -> Option<&StageSpec> {
        self.stages.iter().find(|s| s.id == id)
    }

    /// Validate the DAG structure and return the levelled execution plan.
    ///
    /// HARD-fails on: a dangling `dataflowConsumes`, a cycle, no `Sink`, more
    /// than one `Sink`, or any stage whose `carriesEngineLock` disagrees with
    /// the kind-derived value (single source of truth, #861).
    pub fn validate(&self) -> Result<StageGraph, PipelineError> {
        // ── Engine-lock single source of truth. ──
        for s in &self.stages {
            let derived = s.kind.carries_engine_lock();
            if s.engine_lock != derived {
                return Err(PipelineError::EngineLockMismatch {
                    stage: s.id.clone(),
                    rdf: s.engine_lock,
                    derived,
                });
            }
        }

        // ── Exactly one Sink (the gts narrow waist). ──
        let sinks: Vec<&str> = self
            .stages
            .iter()
            .filter(|s| s.kind == StageKind::Sink)
            .map(|s| s.id.as_str())
            .collect();
        match sinks.len() {
            1 => {}
            0 => {
                return Err(PipelineError::InvalidDag(
                    "the pipeline has no Sink stage (the gts narrow waist requires exactly one)"
                        .to_string(),
                ))
            }
            n => {
                return Err(PipelineError::InvalidDag(format!(
                    "the pipeline has {n} Sink stages ({}); exactly one is allowed (the gts narrow waist)",
                    sinks.join(", ")
                )))
            }
        }

        // ── Acyclicity + completeness + topological levelling. ──
        let nodes: BTreeSet<String> = self.stages.iter().map(|s| s.id.clone()).collect();
        let consumes: BTreeMap<String, BTreeSet<String>> = self
            .stages
            .iter()
            .map(|s| (s.id.clone(), s.consumes.iter().cloned().collect()))
            .collect();
        StageGraph::build(&nodes, &consumes)
    }

    /// Parse a `PipelineSpec` from one or more Turtle documents (the slice
    /// `module.ttl` and any example DAG). Uses the native lenient codecs, so
    /// `@x-gmeow-*` language tags are accepted. Each document is merged under a
    /// fresh blank scope (`push_dataset`); quads dedup at freeze.
    pub fn from_turtle(docs: &[&str]) -> Result<Self, PipelineError> {
        let mut builder = RdfDatasetBuilder::new();
        for doc in docs {
            let parsed = parse_dataset(doc.as_bytes(), "text/turtle", None)
                .map_err(|e| PipelineError::Parse(format!("syntax error in pipeline-spec: {e}")))?;
            builder.push_dataset(&parsed);
        }
        let ds = builder
            .freeze()
            .map_err(|e| PipelineError::Parse(format!("dataset freeze failed: {e}")))?;
        Self::from_dataset(&ds)
    }

    /// Extract the (single) `gmeow:Pipeline` and its stages from a parsed dataset.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, PipelineError> {
        let pipeline_iri = single_subject_of_type(ds, &iri(GMEOW, "Pipeline"))?
            .ok_or_else(|| PipelineError::Parse("no `a gmeow:Pipeline` individual found".into()))?;

        // Collect the hasStage members.
        let has_stage = iri(GMEOW, "hasStage");
        let mut stage_iris: BTreeSet<String> = BTreeSet::new();
        for o in objects(ds, &pipeline_iri, &has_stage) {
            if let ObjTerm::Named(nn) = o {
                stage_iris.insert(nn);
            }
        }

        let mut stages: Vec<StageSpec> = Vec::new();
        for stage_iri in &stage_iris {
            stages.push(parse_stage(ds, stage_iri)?);
        }
        stages.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(PipelineSpec {
            id: local_name(&pipeline_iri),
            stages,
        })
    }
}

/// Parse one `gmeow:PipelineStage` individual into a [`StageSpec`].
fn parse_stage(ds: &RdfDataset, stage_iri: &str) -> Result<StageSpec, PipelineError> {
    let id = local_name(stage_iri);

    // gmeow:stageKind (exactly one).
    let kind_iri = objects(ds, stage_iri, &iri(GMEOW, "stageKind"))
        .into_iter()
        .find_map(|t| match t {
            ObjTerm::Named(nn) => Some(nn),
            _ => None,
        })
        .ok_or_else(|| PipelineError::Parse(format!("stage {id} has no gmeow:stageKind")))?;
    let kind = StageKind::from_iri(&kind_iri).ok_or_else(|| {
        PipelineError::Parse(format!("stage {id} has unknown stageKind {kind_iri}"))
    })?;

    // gmeow:stageImpl (exactly one string).
    let impl_key = objects(ds, stage_iri, &iri(GMEOW, "stageImpl"))
        .into_iter()
        .find_map(literal_string)
        .ok_or_else(|| PipelineError::Parse(format!("stage {id} has no gmeow:stageImpl")))?;

    // gmeow:carriesEngineLock (exactly one boolean).
    let engine_lock = objects(ds, stage_iri, &iri(GMEOW, "carriesEngineLock"))
        .into_iter()
        .find_map(literal_bool)
        .ok_or_else(|| {
            PipelineError::Parse(format!("stage {id} has no gmeow:carriesEngineLock"))
        })?;

    // gmeow:dataflowConsumes (zero or more stage IRIs → local names).
    let mut consumes: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "dataflowConsumes"))
        .into_iter()
        .filter_map(|t| match t {
            ObjTerm::Named(nn) => Some(local_name(&nn)),
            _ => None,
        })
        .collect();
    consumes.sort();
    consumes.dedup();

    // gmeow:producesFormat (zero or more strings).
    let mut formats: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "producesFormat"))
        .into_iter()
        .filter_map(literal_string)
        .collect();
    formats.sort();
    formats.dedup();

    Ok(StageSpec {
        id,
        kind,
        impl_key,
        consumes,
        engine_lock,
        formats,
    })
}

/// Resolve every stage's `gmeow:stageImpl` against the registry and prove the
/// Rust impl agrees with the RDF declaration (kind + consumes). Returns the
/// bound stages in the validated topological order.
pub fn bind(
    spec: &PipelineSpec,
    graph: &StageGraph,
    registry: &StageRegistry,
) -> Result<Vec<Arc<dyn Stage>>, PipelineError> {
    let mut bound: Vec<Arc<dyn Stage>> = Vec::with_capacity(graph.len());
    for id in graph.order() {
        let s = spec.stage(&id).expect("graph id is a spec stage");
        let stage = registry
            .get(&s.impl_key)
            .ok_or_else(|| PipelineError::UnknownStageImpl {
                stage: s.id.clone(),
                impl_key: s.impl_key.clone(),
            })?;

        // Kind agreement.
        if stage.kind() != s.kind {
            return Err(PipelineError::KindMismatch {
                stage: s.id.clone(),
                rdf: s.kind.tag().to_string(),
                rust: stage.kind().tag().to_string(),
            });
        }

        // Consumes agreement (both sides sorted+deduped).
        let mut rust: Vec<String> = stage.consumes().to_vec();
        rust.sort();
        rust.dedup();
        if rust != s.consumes {
            return Err(PipelineError::ConsumesMismatch {
                stage: s.id.clone(),
                rdf: s.consumes.clone(),
                rust,
            });
        }

        bound.push(stage);
    }
    Ok(bound)
}

// ── RDF helpers (mirror gmeow-slice::catalog idioms) ─────────────────────────

fn iri(prefix: &str, local: &str) -> String {
    format!("{prefix}{local}")
}

fn local_name(iri: &str) -> String {
    iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string()
}

/// All objects of `(subject, predicate, _)`, in dataset order. An IRI subject or
/// predicate absent from the dataset's term table yields no matches (its id does
/// not exist), matching the old empty-pattern scan.
fn objects(ds: &RdfDataset, subject: &str, predicate: &str) -> Vec<ObjTerm> {
    let (Some(s), Some(p)) = (
        ds.term_id_by_value(&TermValue::iri(subject)),
        ds.term_id_by_value(&TermValue::iri(predicate)),
    ) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
        .map(|q| match ds.resolve(q.o) {
            TermRef::Iri(iri) => ObjTerm::Named(iri.to_owned()),
            TermRef::Literal { lexical, .. } => ObjTerm::Literal(lexical.to_owned()),
            _ => ObjTerm::Other,
        })
        .collect()
}

/// The single named subject of `(_, rdf:type, class)`, or `None` if there are none.
/// HARD-fails if there is more than one.
fn single_subject_of_type(
    ds: &RdfDataset,
    class_iri: &str,
) -> Result<Option<String>, PipelineError> {
    let (Some(p), Some(o)) = (
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(class_iri)),
    ) else {
        return Ok(None);
    };
    let mut subjects: BTreeSet<String> = BTreeSet::new();
    for q in ds.quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default) {
        if let TermRef::Iri(iri) = ds.resolve(q.s) {
            subjects.insert(iri.to_owned());
        }
    }
    match subjects.len() {
        0 => Ok(None),
        1 => Ok(subjects.into_iter().next()),
        n => Err(PipelineError::Parse(format!(
            "expected exactly one `a <{class_iri}>` individual, found {n}"
        ))),
    }
}

fn literal_string(term: ObjTerm) -> Option<String> {
    match term {
        ObjTerm::Literal(v) => Some(v),
        _ => None,
    }
}

fn literal_bool(term: ObjTerm) -> Option<bool> {
    match term {
        ObjTerm::Literal(v) => match v.as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        },
        _ => None,
    }
}
