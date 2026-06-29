// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The DAG loader: read the dogfooded build graph from `gmeow:` individuals,
//! validate it, and bind each stage to its Rust implementation (#861).
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

use oxigraph::model::Term;
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::graph::StageGraph;
use crate::node::{Stage, GMEOW, SINK_CAPABILITY};
use crate::registry::StageRegistry;
use crate::stages::source_load::turtle_bytes_into_store;

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

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
    /// HARD-fails on: a dangling `dataflowConsumes`, a cycle, no `Sink`, or more
    /// than one `Sink`. (Rust/RDF resource agreement is proven at `bind` time,
    /// once the executable twin is resolved.)
    pub fn validate(&self) -> Result<StageGraph, PipelineError> {
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
    /// `module.ttl` and any example DAG). Uses the same lenient oxigraph parser
    /// the slice crate uses, so `@x-gmeow-*` language tags are accepted.
    pub fn from_turtle(docs: &[&str]) -> Result<Self, PipelineError> {
        let store = Store::new()
            .map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
        for doc in docs {
            turtle_bytes_into_store(&store, doc.as_bytes(), "pipeline-spec")?;
        }
        Self::from_store(&store)
    }

    /// Extract the (single) `gmeow:Pipeline` and its stages from a parsed store.
    pub fn from_store(store: &Store) -> Result<Self, PipelineError> {
        let pipeline_iri = single_subject_of_type(store, &iri(GMEOW, "Pipeline"))?
            .ok_or_else(|| PipelineError::Parse("no `a gmeow:Pipeline` individual found".into()))?;

        // Collect the hasStage members.
        let has_stage = iri(GMEOW, "hasStage");
        let mut stage_iris: BTreeSet<String> = BTreeSet::new();
        for o in objects(store, &pipeline_iri, &has_stage)? {
            if let Term::NamedNode(nn) = o {
                stage_iris.insert(nn.as_str().to_string());
            }
        }

        let mut stages: Vec<StageSpec> = Vec::new();
        for stage_iri in &stage_iris {
            stages.push(parse_stage(store, stage_iri)?);
        }
        stages.sort_by(|a, b| a.id.cmp(&b.id));

        // Attach the reified gmeow:DataFlow typed-dataflow edges to their consumer
        // stages (the artifact-level dependency declarations).
        let mut by_consumer = parse_dataflow_edges(store)?;
        for s in &mut stages {
            if let Some(entities) = by_consumer.remove(&s.id) {
                s.dataflow_entities = entities;
            }
        }

        Ok(PipelineSpec {
            id: local_name(&pipeline_iri),
            stages,
        })
    }
}

/// Parse one `gmeow:PipelineStage` individual into a [`StageSpec`].
fn parse_stage(store: &Store, stage_iri: &str) -> Result<StageSpec, PipelineError> {
    let id = local_name(stage_iri);

    // gmeow:hasCapability (zero or more capability IRIs, kept whole — capabilities are
    // not stages, so they are not reduced to local names).
    let mut capabilities: Vec<String> = objects(store, stage_iri, &iri(GMEOW, "hasCapability"))?
        .into_iter()
        .filter_map(|t| match t {
            Term::NamedNode(nn) => Some(nn.as_str().to_string()),
            _ => None,
        })
        .collect();
    capabilities.sort();
    capabilities.dedup();

    // gmeow:stageImpl (exactly one string).
    let impl_key = objects(store, stage_iri, &iri(GMEOW, "stageImpl"))?
        .into_iter()
        .find_map(literal_string)
        .ok_or_else(|| PipelineError::Parse(format!("stage {id} has no gmeow:stageImpl")))?;

    // gmeow:requiresResource (zero or more resource IRIs, kept whole — resources
    // are not stages, so they are not reduced to local names).
    let mut resources: Vec<String> = objects(store, stage_iri, &iri(GMEOW, "requiresResource"))?
        .into_iter()
        .filter_map(|t| match t {
            Term::NamedNode(nn) => Some(nn.as_str().to_string()),
            _ => None,
        })
        .collect();
    resources.sort();
    resources.dedup();

    // gmeow:dataflowConsumes (zero or more stage IRIs → local names).
    let mut consumes: Vec<String> = objects(store, stage_iri, &iri(GMEOW, "dataflowConsumes"))?
        .into_iter()
        .filter_map(|t| match t {
            Term::NamedNode(nn) => Some(local_name(nn.as_str())),
            _ => None,
        })
        .collect();
    consumes.sort();
    consumes.dedup();

    // gmeow:producesFormat (zero or more strings).
    let mut formats: Vec<String> = objects(store, stage_iri, &iri(GMEOW, "producesFormat"))?
        .into_iter()
        .filter_map(literal_string)
        .collect();
    formats.sort();
    formats.dedup();

    Ok(StageSpec {
        id,
        capabilities,
        impl_key,
        consumes,
        resources,
        // Filled in by from_store from the reified gmeow:DataFlow edges.
        dataflow_entities: Vec::new(),
        formats,
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
    store: &Store,
) -> Result<BTreeMap<String, DataFlowEntities>, PipelineError> {
    // consumer -> producer -> entity set
    let mut acc: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    let dataflow_class = iri(GMEOW, "BuildDataFlow");
    let rdf_type = oxigraph::model::NamedNode::new(RDF_TYPE)
        .map_err(|e| PipelineError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = oxigraph::model::NamedNode::new(&dataflow_class)
        .map_err(|e| PipelineError::Parse(format!("invalid BuildDataFlow class IRI: {e}")))?;
    let mut edges: BTreeSet<String> = BTreeSet::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        None,
    ) {
        let quad = quad.map_err(|e| PipelineError::Parse(e.to_string()))?;
        if let oxigraph::model::NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            edges.insert(nn.as_str().to_string());
        }
    }
    for edge in &edges {
        let producer = objects(store, edge, &iri(GMEOW, "buildFlowFrom"))?
            .into_iter()
            .find_map(|t| match t {
                Term::NamedNode(nn) => Some(local_name(nn.as_str())),
                _ => None,
            })
            .ok_or_else(|| {
                PipelineError::Parse(format!(
                    "gmeow:BuildDataFlow {edge} has no gmeow:buildFlowFrom"
                ))
            })?;
        let consumer = objects(store, edge, &iri(GMEOW, "buildFlowTo"))?
            .into_iter()
            .find_map(|t| match t {
                Term::NamedNode(nn) => Some(local_name(nn.as_str())),
                _ => None,
            })
            .ok_or_else(|| {
                PipelineError::Parse(format!(
                    "gmeow:BuildDataFlow {edge} has no gmeow:buildFlowTo"
                ))
            })?;
        let entity_terms = objects(store, edge, &iri(GMEOW, "flowEntity"))?;
        let entities: BTreeSet<String> = entity_terms
            .into_iter()
            .filter_map(|t| match t {
                Term::NamedNode(nn) => Some(nn.as_str().to_string()),
                _ => None,
            })
            .collect();
        if entities.is_empty() {
            return Err(PipelineError::Parse(format!(
                "gmeow:BuildDataFlow {edge} declares no gmeow:flowEntity (a typed dataflow edge must name at least one entity graph)"
            )));
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

        // Capability agreement (both sides sorted+deduped): the executable twin must
        // declare exactly the capabilities the RDF does, or the executor would read a
        // sink/source role the authored model never declared (single source of truth).
        let mut rust_caps: Vec<String> = stage.capabilities().to_vec();
        rust_caps.sort();
        rust_caps.dedup();
        if rust_caps != s.capabilities {
            return Err(PipelineError::CapabilityMismatch {
                stage: s.id.clone(),
                rdf: s.capabilities.clone(),
                rust: rust_caps,
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

        // Resource agreement (both sides sorted+deduped): the executable twin must
        // declare exactly the shared resources the RDF does, or the scheduler's
        // serialization would diverge from the authored model.
        let mut rust_res: Vec<String> = stage.resources().to_vec();
        rust_res.sort();
        rust_res.dedup();
        if rust_res != s.resources {
            return Err(PipelineError::ResourceMismatch {
                stage: s.id.clone(),
                rdf: s.resources.clone(),
                rust: rust_res,
            });
        }

        // Typed-dataflow (artifact-level) agreement: the executable twin must declare
        // exactly the entity narrowing the RDF gmeow:DataFlow edges declare, or the
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
            return Err(PipelineError::DataFlowMismatch {
                stage: s.id.clone(),
                rdf: s.dataflow_entities.clone(),
                rust: rust_entities,
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

/// All objects of `(subject, predicate, _)`.
fn objects(store: &Store, subject: &str, predicate: &str) -> Result<Vec<Term>, PipelineError> {
    let s = oxigraph::model::NamedNode::new(subject)
        .map_err(|e| PipelineError::Parse(format!("invalid subject IRI {subject}: {e}")))?;
    let p = oxigraph::model::NamedNode::new(predicate)
        .map_err(|e| PipelineError::Parse(format!("invalid predicate IRI {predicate}: {e}")))?;
    let mut out = Vec::new();
    for quad in store.quads_for_pattern(Some(s.as_ref().into()), Some(p.as_ref()), None, None) {
        let quad = quad.map_err(|e| PipelineError::Parse(e.to_string()))?;
        out.push(quad.object);
    }
    Ok(out)
}

/// The single subject of `(_, rdf:type, class)`, or `None` if there are none.
/// HARD-fails if there is more than one.
fn single_subject_of_type(store: &Store, class_iri: &str) -> Result<Option<String>, PipelineError> {
    let rdf_type = oxigraph::model::NamedNode::new(RDF_TYPE)
        .map_err(|e| PipelineError::Parse(format!("invalid rdf:type IRI: {e}")))?;
    let class = oxigraph::model::NamedNode::new(class_iri)
        .map_err(|e| PipelineError::Parse(format!("invalid class IRI {class_iri}: {e}")))?;
    let mut subjects: BTreeSet<String> = BTreeSet::new();
    for quad in store.quads_for_pattern(
        None,
        Some(rdf_type.as_ref()),
        Some(class.as_ref().into()),
        None,
    ) {
        let quad = quad.map_err(|e| PipelineError::Parse(e.to_string()))?;
        if let oxigraph::model::NamedOrBlankNode::NamedNode(nn) = &quad.subject {
            subjects.insert(nn.as_str().to_string());
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

fn literal_string(term: Term) -> Option<String> {
    match term {
        Term::Literal(l) => Some(l.value().to_string()),
        _ => None,
    }
}
