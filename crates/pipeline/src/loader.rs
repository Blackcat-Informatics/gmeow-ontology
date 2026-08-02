// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DAG loader: read the dogfooded build graph from `gmeow:` individuals,
//! validate it, and bind each stage to its Rust implementation.
//!
//! The build DAG is authored as data — `gmeow:Pipeline` / `gmeow:PipelineStage`
//! individuals in `slices/core/pipeline/` — and read back here, so the build is
//! a first-class ontological citizen (MAXIMAL DOGFOODING). [`PipelineSpec`] is
//! the parsed-but-unbound shape; [`PipelineSpec::validate`] proves the structure
//! (acyclic, complete, exactly one sink); [`bind`] resolves each `gmeow:stageImpl`
//! against the [`StageRegistry`] and proves the Rust impl agrees with the RDF
//! declaration (capabilities + consumes + resources + typed dataflow). Every check
//! HARD-fails before any stage runs.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{
    DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, TermRef, TermValue, parse_dataset,
};

use crate::graph::StageGraph;
use crate::node::{GMEOW, SINK_CAPABILITY, Stage};
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

/// A stage's typed dataflow: for each upstream producer it narrows to specific
/// named-graph entities, a `(producer-local-name, sorted entity IRIs)` pair; the
/// whole list sorted by producer. Empty = every consumed producer is a whole-product
/// dependency.
pub type DataFlowEntities = Vec<(String, Vec<String>)>;

/// One `gmeow:PipelineStage` individual, parsed but not yet bound to its impl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageSpec {
    /// The stage id — the `gmeow:PipelineStage` individual's local name.
    pub id: String,
    /// `gmeow:hasCapability` — the capability IRIs this stage holds (e.g.
    /// [`crate::node::SINK_CAPABILITY`], [`crate::node::SOURCE_ORIGIN`]), sorted and
    /// deduplicated. Validated against the Rust impl's `capabilities()` at bind time
    /// (Rust/RDF agreement); the executor reads these in place of a kind enum.
    pub capabilities: Vec<String>,
    /// `gmeow:stageImpl` — the registry key binding this to a Rust [`Stage`].
    pub impl_key: String,
    /// `gmeow:dataflowConsumes` — upstream stage ids, sorted, deduplicated.
    pub consumes: Vec<String>,
    /// `gmeow:requiresResource` — the IRIs of shared resources the stage holds
    /// exclusively while running, sorted, deduplicated. Validated against the Rust
    /// impl's `resources()` at bind time (Rust/RDF agreement).
    pub resources: Vec<String>,
    /// Reified `gmeow:BuildDataFlow` typed dataflow: for each upstream producer this
    /// stage reads only SPECIFIC named-graph entities from, the `(producer-local-name,
    /// sorted entity IRIs)`, the list sorted by producer. Empty = every consumed
    /// producer is a whole-product dependency (the sound default). Validated against the
    /// Rust impl's `consumed_entities()` at bind time (Rust/RDF agreement).
    pub dataflow_entities: DataFlowEntities,
    /// `gmeow:producesFormat` — output format tags, sorted (export leaves).
    pub formats: Vec<String>,
    /// `gmeow:attachesGraph` — the named-graph IRIs (kept whole) this stage attaches
    /// to the carrier as its delta, sorted and deduplicated. Validated against the Rust
    /// impl's `attaches_graphs()` at bind time (Rust/RDF agreement).
    pub attaches_graphs: Vec<String>,
    /// `gmeow:attachesBlobRep` — the blob-representation lane labels this stage attaches
    /// (e.g. `"axioms-archive"`, `"diagnostics:nodes"`), sorted and deduplicated.
    /// Validated against the Rust impl's `attaches_blob_reps()` at bind time.
    pub attaches_blob_reps: Vec<String>,
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
    /// than one `Sink`, or a typed-dataflow narrowing whose producer the consumer
    /// does not actually `dataflowConsumes`. (Rust/RDF resource agreement is proven
    /// at `bind` time, once the executable twin is resolved.)
    pub fn validate(&self) -> Result<StageGraph, gmeow_errors::Diag> {
        // ── Typed-dataflow endpoints: every gmeow:BuildDataFlow producer a consumer
        //    narrows to must be a declared stage AND one the consumer actually
        //    gmeow:dataflowConsumes. Otherwise the cache would narrow on a graph the
        //    scheduler never feeds the stage (its upstream map is built from
        //    `consumes`), silently serving a stale product — a no-optionality hazard. ──
        let node_ids: BTreeSet<&str> = self.stages.iter().map(|s| s.id.as_str()).collect();
        for s in &self.stages {
            for (producer, _entities) in &s.dataflow_entities {
                if !node_ids.contains(producer.as_str()) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                        message: format!(
                            "stage {} declares typed dataflow from unknown producer {producer}",
                            s.id
                        ),
                    }));
                }
                if !s.consumes.iter().any(|c| c == producer) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                        message: format!(
                            "stage {} declares typed dataflow from {producer} but does not gmeow:dataflowConsumes it",
                            s.id
                        ),
                    }));
                }
            }
        }

        // ── Exactly one Sink (the gts narrow waist): the stage holding
        //    gmeow:sinkCapability. ──
        let sinks: Vec<&str> = self
            .stages
            .iter()
            .filter(|s| s.capabilities.iter().any(|c| c == SINK_CAPABILITY))
            .map(|s| s.id.as_str())
            .collect();
        match sinks.len() {
            1 => {}
            0 => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                    message:
                        "the pipeline has no Sink stage (the gts narrow waist requires exactly one)"
                            .to_string(),
                }));
            }
            n => {
                return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                    message: format!(
                        "the pipeline has {n} Sink stages ({}); exactly one is allowed (the gts narrow waist)",
                        sinks.join(", ")
                    ),
                }));
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
    pub fn from_turtle(docs: &[&str]) -> Result<Self, gmeow_errors::Diag> {
        let mut builder = RdfDatasetBuilder::new();
        for doc in docs {
            let parsed = parse_dataset(doc.as_bytes(), "text/turtle", None).map_err(|e| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("syntax error in pipeline-spec: {e}"),
                })
            })?;
            builder.push_dataset(&parsed);
        }
        let ds = builder.freeze().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("dataset freeze failed: {e}"),
            })
        })?;
        Self::from_dataset(&ds)
    }

    /// Extract the (single) `gmeow:Pipeline` and its stages from a parsed dataset.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, gmeow_errors::Diag> {
        let pipeline_iri =
            single_subject_of_type(ds, &iri(GMEOW, "Pipeline"))?.ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: "no `a gmeow:Pipeline` individual found".into(),
                })
            })?;

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

        // Attach the reified gmeow:BuildDataFlow typed-dataflow edges to their consumer
        // stages (the artifact-level dependency declarations).
        let mut by_consumer = parse_dataflow_edges(ds)?;
        for s in &mut stages {
            if let Some(entities) = by_consumer.remove(&s.id) {
                s.dataflow_entities = entities;
            }
        }
        // A gmeow:BuildDataFlow whose gmeow:buildFlowTo is not a hasStage member of
        // this pipeline would be silently dropped; hard-fail instead (no-optionality).
        if !by_consumer.is_empty() {
            let mut unknown: Vec<String> = by_consumer.into_keys().collect();
            unknown.sort();
            return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                message: format!(
                    "gmeow:BuildDataFlow targets unknown consumer stage(s): {}",
                    unknown.join(", ")
                ),
            }));
        }

        Ok(PipelineSpec {
            id: local_name(&pipeline_iri),
            stages,
        })
    }
}

/// Parse one `gmeow:PipelineStage` individual into a [`StageSpec`].
fn parse_stage(ds: &RdfDataset, stage_iri: &str) -> Result<StageSpec, gmeow_errors::Diag> {
    let id = local_name(stage_iri);

    // gmeow:hasCapability (zero or more capability IRIs, kept whole — capabilities are
    // not stages, so they are not reduced to local names).
    let mut capabilities: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "hasCapability"))
        .into_iter()
        .filter_map(|t| match t {
            ObjTerm::Named(nn) => Some(nn),
            _ => None,
        })
        .collect();
    capabilities.sort();
    capabilities.dedup();

    // gmeow:stageImpl (exactly one string).
    let impl_key = objects(ds, stage_iri, &iri(GMEOW, "stageImpl"))
        .into_iter()
        .find_map(literal_string)
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("stage {id} has no gmeow:stageImpl"),
            })
        })?;

    // gmeow:requiresResource (zero or more resource IRIs, kept whole — resources
    // are not stages, so they are not reduced to local names).
    let mut resources: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "requiresResource"))
        .into_iter()
        .filter_map(|t| match t {
            ObjTerm::Named(nn) => Some(nn),
            _ => None,
        })
        .collect();
    resources.sort();
    resources.dedup();

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

    // gmeow:attachesGraph (zero or more named-graph IRIs, kept whole — they are graphs,
    // not stages, so they are not reduced to local names).
    let mut attaches_graphs: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "attachesGraph"))
        .into_iter()
        .filter_map(|t| match t {
            ObjTerm::Named(nn) => Some(nn),
            _ => None,
        })
        .collect();
    attaches_graphs.sort();
    attaches_graphs.dedup();

    // gmeow:attachesBlobRep (zero or more representation-lane label strings).
    let mut attaches_blob_reps: Vec<String> =
        objects(ds, stage_iri, &iri(GMEOW, "attachesBlobRep"))
            .into_iter()
            .filter_map(literal_string)
            .collect();
    attaches_blob_reps.sort();
    attaches_blob_reps.dedup();

    Ok(StageSpec {
        id,
        capabilities,
        impl_key,
        consumes,
        resources,
        // Filled in by from_store from the reified gmeow:BuildDataFlow edges.
        dataflow_entities: Vec::new(),
        formats,
        attaches_graphs,
        attaches_blob_reps,
    })
}

/// Parse the reified `gmeow:BuildDataFlow` typed-dataflow edges into a
/// `consumer-local-name → sorted [(producer-local-name, sorted entity IRIs)]` map.
///
/// Each `gmeow:BuildDataFlow` individual (a specialization of the canonical
/// `logic:DataFlowEdge`) carries `gmeow:buildFlowFrom` (the producer stage, a
/// `logic:flowFrom` leg), `gmeow:buildFlowTo` (the consumer stage, a `logic:flowTo`
/// leg), and one or more `gmeow:flowEntity` (the named-graph IRIs that flow on this
/// edge — kept whole, they are graphs not stages). Edges sharing a (consumer,
/// producer) pair are merged.
fn parse_dataflow_edges(
    ds: &RdfDataset,
) -> Result<BTreeMap<String, DataFlowEntities>, gmeow_errors::Diag> {
    // consumer -> producer -> entity set
    let mut acc: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    // Every named `?edge a gmeow:BuildDataFlow` subject (kept whole), sorted.
    let mut edges: BTreeSet<String> = BTreeSet::new();
    if let (Some(p), Some(o)) = (
        ds.term_id_by_value(&TermValue::iri(RDF_TYPE)),
        ds.term_id_by_value(&TermValue::iri(iri(GMEOW, "BuildDataFlow"))),
    ) {
        for q in ds.quads_for_pattern(None, Some(p), Some(o), GraphMatch::Default) {
            if let TermRef::Iri(edge) = ds.resolve(q.s) {
                edges.insert(edge.to_owned());
            }
        }
    }
    for edge in &edges {
        let producer = objects(ds, edge, &iri(GMEOW, "buildFlowFrom"))
            .into_iter()
            .find_map(|t| match t {
                ObjTerm::Named(nn) => Some(local_name(&nn)),
                _ => None,
            })
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("gmeow:BuildDataFlow {edge} has no gmeow:buildFlowFrom"),
                })
            })?;
        let consumer = objects(ds, edge, &iri(GMEOW, "buildFlowTo"))
            .into_iter()
            .find_map(|t| match t {
                ObjTerm::Named(nn) => Some(local_name(&nn)),
                _ => None,
            })
            .ok_or_else(|| {
                gmeow_errors::Diag::of_kind(crate::error::Parse {
                    message: format!("gmeow:BuildDataFlow {edge} has no gmeow:buildFlowTo"),
                })
            })?;
        let entities: BTreeSet<String> = objects(ds, edge, &iri(GMEOW, "flowEntity"))
            .into_iter()
            .filter_map(|t| match t {
                ObjTerm::Named(nn) => Some(nn),
                _ => None,
            })
            .collect();
        if entities.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!(
                    "gmeow:BuildDataFlow {edge} declares no gmeow:flowEntity (a typed dataflow edge must name at least one entity graph)"
                ),
            }));
        }
        acc.entry(consumer)
            .or_default()
            .entry(producer)
            .or_default()
            .extend(entities);
    }
    // Materialize to sorted Vecs (by producer; entities sorted).
    let out = acc
        .into_iter()
        .map(|(consumer, producers)| {
            let mut rows: Vec<(String, Vec<String>)> = producers
                .into_iter()
                .map(|(p, ents)| (p, ents.into_iter().collect()))
                .collect();
            rows.sort();
            (consumer, rows)
        })
        .collect();
    Ok(out)
}

/// Resolve every stage's `gmeow:stageImpl` against the registry and prove the
/// Rust impl agrees with the RDF declaration (capabilities + consumes + resources +
/// typed dataflow). Returns the bound stages in the validated topological order.
pub fn bind(
    spec: &PipelineSpec,
    graph: &StageGraph,
    registry: &StageRegistry,
) -> Result<Vec<Arc<dyn Stage>>, gmeow_errors::Diag> {
    let mut bound: Vec<Arc<dyn Stage>> = Vec::with_capacity(graph.len());
    for id in graph.order() {
        let s = spec.stage(&id).expect("graph id is a spec stage");
        let stage = registry.get(&s.impl_key).ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::UnknownStageImpl {
                stage: s.id.clone(),
                impl_key: s.impl_key.clone(),
            })
        })?;

        // Capability agreement (both sides sorted+deduped): the executable twin must
        // declare exactly the capabilities the RDF does, or the executor would read a
        // sink/source role the authored model never declared (single source of truth).
        let mut rust_caps: Vec<String> = stage.capabilities().to_vec();
        rust_caps.sort();
        rust_caps.dedup();
        if rust_caps != s.capabilities {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::CapabilityMismatch {
                    stage: s.id.clone(),
                    rdf: s.capabilities.clone(),
                    rust: rust_caps,
                },
            ));
        }

        // Consumes agreement (both sides sorted+deduped).
        let mut rust: Vec<String> = stage.consumes().to_vec();
        rust.sort();
        rust.dedup();
        if rust != s.consumes {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::ConsumesMismatch {
                    stage: s.id.clone(),
                    rdf: s.consumes.clone(),
                    rust,
                },
            ));
        }

        // Resource agreement (both sides sorted+deduped): the executable twin must
        // declare exactly the shared resources the RDF does, or the scheduler's
        // serialization would diverge from the authored model.
        let mut rust_res: Vec<String> = stage.resources().to_vec();
        rust_res.sort();
        rust_res.dedup();
        if rust_res != s.resources {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::ResourceMismatch {
                    stage: s.id.clone(),
                    rdf: s.resources.clone(),
                    rust: rust_res,
                },
            ));
        }

        // Typed-dataflow (artifact-level) agreement: the executable twin must declare
        // exactly the entity narrowing the RDF gmeow:BuildDataFlow edges declare, or the
        // cache could narrow on a different entity set than the stage reads (stale
        // hazard). Normalize both sides (sorted by producer, entities sorted+deduped).
        let mut rust_entities: Vec<(String, Vec<String>)> = stage
            .consumed_entities()
            .iter()
            .map(|(producer, ents)| {
                let mut e = ents.clone();
                e.sort();
                e.dedup();
                (producer.clone(), e)
            })
            .collect();
        rust_entities.sort();
        if rust_entities != s.dataflow_entities {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::DataFlowMismatch {
                    stage: s.id.clone(),
                    rdf: s.dataflow_entities.clone(),
                    rust: rust_entities,
                },
            ));
        }

        // Attach-declaration agreement (both sides sorted+deduped): the executable twin
        // must declare exactly the named graphs / blob-rep lanes the RDF
        // gmeow:attachesGraph / gmeow:attachesBlobRep declare. The scheduler verifies the
        // ACTUAL run-time delta against this same declaration (error::AttachDrift), so a
        // Rust/RDF disagreement here would let the run enforce a declaration the authored
        // model never made (single source of truth).
        let mut rust_graphs: Vec<String> = stage.attaches_graphs().to_vec();
        rust_graphs.sort();
        rust_graphs.dedup();
        if rust_graphs != s.attaches_graphs {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::AttachDeclMismatch {
                    stage: s.id.clone(),
                    lane: "gmeow:attachesGraph".to_string(),
                    rdf: s.attaches_graphs.clone(),
                    rust: rust_graphs,
                },
            ));
        }
        let mut rust_blob_reps: Vec<String> = stage.attaches_blob_reps().to_vec();
        rust_blob_reps.sort();
        rust_blob_reps.dedup();
        if rust_blob_reps != s.attaches_blob_reps {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::AttachDeclMismatch {
                    stage: s.id.clone(),
                    lane: "gmeow:attachesBlobRep".to_string(),
                    rdf: s.attaches_blob_reps.clone(),
                    rust: rust_blob_reps,
                },
            ));
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
) -> Result<Option<String>, gmeow_errors::Diag> {
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
        n => Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
            message: format!("expected exactly one `a <{class_iri}>` individual, found {n}"),
        })),
    }
}

fn literal_string(term: ObjTerm) -> Option<String> {
    match term {
        ObjTerm::Literal(v) => Some(v),
        _ => None,
    }
}
