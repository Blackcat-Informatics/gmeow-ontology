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
use crate::node::{CachePolicy, GMEOW, SINK_CAPABILITY, Stage, StageStability};
use crate::reads::{Read, ReadSet};
use crate::registry::StageRegistry;

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
    /// Exactly one `gmeow:stageStability` closed-vocabulary individual. The loader
    /// rejects missing, multiple, non-IRI, or unknown declarations and proves this
    /// value agrees with the executable stage twin before scheduling.
    pub stability: StageStability,
    /// Exactly one `gmeow:stageCacheDisposition` closed-vocabulary individual. This
    /// declaration decides whether the independently bounded stage contribution is
    /// admitted to the persistent cache or recomputed from live upstream products.
    pub cache_disposition: CachePolicy,
    /// Reified `gmeow:BuildDataFlow` typed dataflow: for each upstream producer this
    /// stage reads only SPECIFIC named-graph entities from, the `(producer-local-name,
    /// sorted entity IRIs)`, the list sorted by producer. Empty = every consumed
    /// producer is a whole-product dependency (the sound default). Validated against the
    /// Rust impl's `consumed_entities()` at bind time (Rust/RDF agreement).
    pub dataflow_entities: DataFlowEntities,
    /// The reads each reified `gmeow:BuildDataFlow` edge into this stage declares, by
    /// producer: which lanes of that producer's product this stage may read. The
    /// scheduler hands the stage a view of each product that records every access, and
    /// fails the stage when the recorded reads and these declarations differ. Retention
    /// of a product's lanes is derived from the declarations of the consumers still to run.
    pub reads: BTreeMap<String, ReadSet>,
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

        // ── Declared reads: a read set names what a stage reads of a producer it
        //    consumes. A read set on any other stage could never be served, because the
        //    scheduler hands a stage only the products of its dependencies. ──
        for s in &self.stages {
            for producer in s.reads.keys() {
                if !s.consumes.iter().any(|c| c == producer) {
                    return Err(gmeow_errors::Diag::of_kind(crate::error::InvalidDag {
                        message: format!(
                            "stage {} declares reads of {producer} but does not gmeow:dataflowConsumes it",
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
            builder.push_dataset(&parsed)?;
        }
        let ds = builder.freeze().map_err(|e| {
            gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!("dataset freeze failed: {e}"),
            })
        })?;
        Self::from_dataset(&ds)
    }

    /// Extract the (single) `gmeow:Pipeline` and its stages from a parsed dataset.
    /// Implementation keys, output formats and blob-representation labels require
    /// complete native-valid, untagged string/token literals. Each stage declares
    /// exactly one implementation key before any lexical projection.
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
            if let TermValue::Iri(nn) = o {
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
            if let Some(declared) = by_consumer.remove(&s.id) {
                s.dataflow_entities = declared.entities;
                s.reads = declared.reads;
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
            TermValue::Iri(nn) => Some(nn),
            _ => None,
        })
        .collect();
    capabilities.sort();
    capabilities.dedup();

    let impl_key = required_string_declaration(ds, stage_iri, "stageImpl", &id)?;

    // gmeow:requiresResource (zero or more resource IRIs, kept whole — resources
    // are not stages, so they are not reduced to local names).
    let mut resources: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "requiresResource"))
        .into_iter()
        .filter_map(|t| match t {
            TermValue::Iri(nn) => Some(nn),
            _ => None,
        })
        .collect();
    resources.sort();
    resources.dedup();

    let stability_iri = required_named_declaration(ds, stage_iri, "stageStability", &id)?;
    let stability = StageStability::from_iri(&stability_iri).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!("stage {id} has unknown gmeow:stageStability value {stability_iri}"),
        })
    })?;

    let cache_disposition_iri =
        required_named_declaration(ds, stage_iri, "stageCacheDisposition", &id)?;
    let cache_disposition = CachePolicy::from_iri(&cache_disposition_iri).ok_or_else(|| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!(
                "stage {id} has unknown gmeow:stageCacheDisposition value {cache_disposition_iri}"
            ),
        })
    })?;

    // gmeow:dataflowConsumes (zero or more stage IRIs → local names).
    let mut consumes: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "dataflowConsumes"))
        .into_iter()
        .filter_map(|t| match t {
            TermValue::Iri(nn) => Some(local_name(&nn)),
            _ => None,
        })
        .collect();
    consumes.sort();
    consumes.dedup();

    let formats = string_declarations(ds, stage_iri, "producesFormat", &id)?;

    // gmeow:attachesGraph (zero or more named-graph IRIs, kept whole — they are graphs,
    // not stages, so they are not reduced to local names).
    let mut attaches_graphs: Vec<String> = objects(ds, stage_iri, &iri(GMEOW, "attachesGraph"))
        .into_iter()
        .filter_map(|t| match t {
            TermValue::Iri(nn) => Some(nn),
            _ => None,
        })
        .collect();
    attaches_graphs.sort();
    attaches_graphs.dedup();

    let attaches_blob_reps = string_declarations(ds, stage_iri, "attachesBlobRep", &id)?;

    Ok(StageSpec {
        id,
        capabilities,
        impl_key,
        consumes,
        resources,
        stability,
        cache_disposition,
        // Filled in by from_store from the reified gmeow:BuildDataFlow edges.
        dataflow_entities: Vec::new(),
        reads: BTreeMap::new(),
        formats,
        attaches_graphs,
        attaches_blob_reps,
    })
}

/// Read one required closed-vocabulary stage declaration as an IRI.
///
/// Cardinality is checked over every RDF object before term discrimination: a literal
/// plus an IRI is still two values and therefore fails rather than silently selecting
/// the usable-looking one.
fn required_named_declaration(
    ds: &RdfDataset,
    stage_iri: &str,
    property: &str,
    stage_id: &str,
) -> Result<String, gmeow_errors::Diag> {
    let values = objects(ds, stage_iri, &iri(GMEOW, property));
    if values.len() != 1 {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::InvalidDeclaration {
                message: format!(
                    "stage {stage_id} must declare exactly one gmeow:{property}; found {}",
                    values.len()
                ),
            },
        ));
    }
    match values.into_iter().next().expect("one value was checked") {
        TermValue::Iri(value) => Ok(value),
        _ => Err(gmeow_errors::Diag::of_kind(
            crate::error::InvalidDeclaration {
                message: format!(
                    "stage {stage_id} gmeow:{property} must name a closed-vocabulary IRI"
                ),
            },
        )),
    }
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
) -> Result<BTreeMap<String, EdgeDeclarations>, gmeow_errors::Diag> {
    // consumer -> producer -> entity set
    let mut acc: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    // consumer -> producer -> declared read set
    let mut reads_acc: BTreeMap<String, BTreeMap<String, ReadSet>> = BTreeMap::new();
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
                TermValue::Iri(nn) => Some(local_name(&nn)),
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
                TermValue::Iri(nn) => Some(local_name(&nn)),
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
                TermValue::Iri(nn) => Some(nn),
                _ => None,
            })
            .collect();
        let reads = parse_edge_reads(ds, edge)?;
        if entities.is_empty() && reads.is_empty() {
            return Err(gmeow_errors::Diag::of_kind(crate::error::Parse {
                message: format!(
                    "gmeow:BuildDataFlow {edge} declares neither a read (gmeow:flowReadsLane, \
                     gmeow:flowReadsHandle, gmeow:flowReadsBlobRep, gmeow:flowReadsArtifact) nor a \
                     gmeow:flowEntity: an edge that carries nothing is not a dependency"
                ),
            }));
        }
        if !entities.is_empty() {
            acc.entry(consumer.clone())
                .or_default()
                .entry(producer.clone())
                .or_default()
                .extend(entities);
        }
        reads_acc
            .entry(consumer)
            .or_default()
            .entry(producer)
            .or_default()
            .extend(reads);
    }
    // Materialize to sorted Vecs (by producer; entities sorted).
    let mut out: BTreeMap<String, EdgeDeclarations> = BTreeMap::new();
    for (consumer, producers) in acc {
        let mut rows: Vec<(String, Vec<String>)> = producers
            .into_iter()
            .map(|(p, ents)| (p, ents.into_iter().collect()))
            .collect();
        rows.sort();
        out.entry(consumer).or_default().entities = rows;
    }
    for (consumer, producers) in reads_acc {
        out.entry(consumer).or_default().reads = producers;
    }
    Ok(out)
}

/// What the reified `gmeow:BuildDataFlow` edges into one consumer declare.
#[derive(Debug, Default)]
struct EdgeDeclarations {
    /// The named-graph narrowing of the cache key, per producer.
    entities: DataFlowEntities,
    /// The declared read set, per producer.
    reads: BTreeMap<String, ReadSet>,
}

/// Parse the reads one `gmeow:BuildDataFlow` edge declares.
///
/// A value of the wrong term kind, or a lane individual outside the closed vocabulary,
/// is a hard failure: dropping it would silently narrow what the consumer may read.
fn parse_edge_reads(ds: &RdfDataset, edge: &str) -> Result<ReadSet, gmeow_errors::Diag> {
    let invalid = |property: &str, value: &TermValue| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!(
                "gmeow:BuildDataFlow {edge} has an invalid gmeow:{property} value {value:?}"
            ),
        })
    };
    let mut reads = ReadSet::new();
    for value in objects(ds, edge, &iri(GMEOW, "flowReadsLane")) {
        let lane = match &value {
            TermValue::Iri(lane) => Read::from_lane_iri(lane),
            _ => None,
        };
        reads.insert(lane.ok_or_else(|| invalid("flowReadsLane", &value))?);
    }
    for value in objects(ds, edge, &iri(GMEOW, "flowReadsHandle")) {
        match &value {
            TermValue::Iri(key) => reads.insert(Read::Handle(key.clone())),
            _ => return Err(invalid("flowReadsHandle", &value)),
        };
    }
    for rep in string_declarations(ds, edge, "flowReadsBlobRep", edge)? {
        reads.insert(Read::BlobRep(rep));
    }
    for path in string_declarations(ds, edge, "flowReadsArtifact", edge)? {
        reads.insert(Read::Artifact(path));
    }
    Ok(reads)
}

/// Resolve every stage's `gmeow:stageImpl` against the registry and bind each
/// implementation to the declarations of its stage. Returns the bound stages in the
/// validated topological order.
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

        // The body names its product by the stage id, so the two must be one name.
        if stage.id() != s.id {
            return Err(gmeow_errors::Diag::of_kind(
                crate::error::InvalidDeclaration {
                    message: format!(
                        "stage {} binds implementation {}, whose body names itself {}",
                        s.id,
                        s.impl_key,
                        stage.id()
                    ),
                },
            ));
        }
        bound.push(Arc::new(BoundStage::new(s.clone(), stage)));
    }
    Ok(bound)
}

/// A stage as the scheduler runs it: the declarations of the pipeline description and
/// the implementation its `gmeow:stageImpl` names.
///
/// Every declaration the scheduler reads comes from the description. The implementation
/// supplies only what a description cannot hold: its body, its implementation version,
/// the source files it reads from the working tree, and its output family. What the
/// description declares about reads also decides retention: which consumed products
/// keep a live carrier for this stage, which internal artifacts survive a release for
/// it, and whether it reads the source-span table.
struct BoundStage {
    spec: StageSpec,
    body: Arc<dyn crate::node::StageBody>,
    carrier_consumes: Vec<String>,
}

impl BoundStage {
    fn new(spec: StageSpec, body: Arc<dyn crate::node::StageBody>) -> Self {
        let carrier_consumes = spec
            .consumes
            .iter()
            .filter(|producer| {
                // A named-graph narrowing reads those graphs of the producer's dataset,
                // so it keeps the carrier just as a declared carrier lane does.
                spec.reads
                    .get(*producer)
                    .is_some_and(crate::reads::needs_carrier)
                    || spec
                        .dataflow_entities
                        .iter()
                        .any(|(narrowed, _)| narrowed == *producer)
            })
            .cloned()
            .collect();
        Self {
            spec,
            body,
            carrier_consumes,
        }
    }
}

impl Stage for BoundStage {
    fn id(&self) -> &str {
        &self.spec.id
    }
    fn output_family(&self) -> crate::node::OutputFamily {
        self.body.output_family()
    }
    fn consumes(&self) -> &[String] {
        &self.spec.consumes
    }
    fn carrier_consumes(&self) -> &[String] {
        &self.carrier_consumes
    }
    fn internal_artifact_reads(&self) -> Vec<(String, String)> {
        self.spec
            .reads
            .iter()
            .flat_map(|(producer, reads)| {
                crate::reads::internal_artifacts(reads)
                    .map(|artifact| (producer.clone(), artifact.to_owned()))
            })
            .collect()
    }
    fn reads(&self) -> BTreeMap<String, ReadSet> {
        self.spec.reads.clone()
    }
    fn capabilities(&self) -> &[String] {
        &self.spec.capabilities
    }
    fn resources(&self) -> &[String] {
        &self.spec.resources
    }
    fn stability(&self) -> StageStability {
        self.spec.stability
    }
    fn cache_policy(&self) -> CachePolicy {
        self.spec.cache_disposition
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.spec.dataflow_entities
    }
    fn consumes_span_table(&self) -> bool {
        let span_table = Read::BlobRep(crate::stages::carrier::REP_SPAN_TABLE.to_owned());
        self.spec
            .reads
            .values()
            .any(|reads| reads.contains(&span_table))
    }
    fn attaches_graphs(&self) -> &[String] {
        &self.spec.attaches_graphs
    }
    fn attaches_blob_reps(&self) -> &[String] {
        &self.spec.attaches_blob_reps
    }
    fn impl_version(&self) -> &str {
        self.body.impl_version()
    }
    fn input_files(&self, root: &std::path::Path) -> gmeow_errors::Result<Vec<std::path::PathBuf>> {
        self.body.input_files(root)
    }
    fn run(
        &self,
        input: crate::node::StageInput<'_>,
    ) -> gmeow_errors::Result<crate::node::StageOutput> {
        self.body.run(input)
    }
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
fn objects(ds: &RdfDataset, subject: &str, predicate: &str) -> Vec<TermValue> {
    let (Some(s), Some(p)) = (
        ds.term_id_by_value(&TermValue::iri(subject)),
        ds.term_id_by_value(&TermValue::iri(predicate)),
    ) else {
        return Vec::new();
    };
    ds.quads_for_pattern(Some(s), Some(p), None, GraphMatch::Default)
        .map(|q| ds.term_value(q.o))
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

/// Admit one complete machine-string declaration without changing its lexical bytes.
///
/// # Errors
/// Rejects nonliterals, language/direction metadata, foreign datatypes and native
/// lexical failures with the owning stage and property in the diagnostic.
fn declaration_string(
    term: TermValue,
    stage_id: &str,
    property: &str,
) -> Result<String, gmeow_errors::Diag> {
    let invalid = |detail: &str| {
        gmeow_errors::Diag::of_kind(crate::error::InvalidDeclaration {
            message: format!("stage {stage_id} gmeow:{property} {detail}"),
        })
    };
    let TermValue::Literal {
        lexical_form,
        datatype,
        language,
        direction,
    } = term
    else {
        return Err(invalid(
            "must be an untagged xsd:string or xsd:token literal",
        ));
    };
    if language.is_some() || direction.is_some() {
        return Err(invalid("must not carry language or direction"));
    }
    match datatype.as_str() {
        purrdf::xsd::datatype::XSD_STRING => purrdf::xsd::simple::read_xml_string(&lexical_form),
        purrdf::xsd::datatype::XSD_TOKEN => purrdf::xsd::simple::read_token(&lexical_form),
        _ => {
            return Err(invalid(&format!(
                "must use xsd:string or xsd:token; found <{datatype}>"
            )));
        }
    }
    .map_err(|detail| {
        invalid(&format!(
            "has an invalid <{datatype}> lexical form: {detail}"
        ))
    })?;
    Ok(lexical_form)
}

/// Read exactly one native machine-string term before projecting its lexical value.
///
/// # Errors
/// Rejects a missing or multiple declaration and every failure of literal admission.
fn required_string_declaration(
    ds: &RdfDataset,
    stage_iri: &str,
    property: &str,
    stage_id: &str,
) -> Result<String, gmeow_errors::Diag> {
    let values = objects(ds, stage_iri, &iri(GMEOW, property));
    if values.len() != 1 {
        return Err(gmeow_errors::Diag::of_kind(
            crate::error::InvalidDeclaration {
                message: format!(
                    "stage {stage_id} must declare exactly one gmeow:{property}; found {}",
                    values.len()
                ),
            },
        ));
    }
    declaration_string(
        values.into_iter().next().expect("one value was checked"),
        stage_id,
        property,
    )
}

/// Admit every optional machine-string term, then retain the existing lexical set.
///
/// # Errors
/// Rejects any supplied value that fails literal admission; invalid rows never vanish.
fn string_declarations(
    ds: &RdfDataset,
    stage_iri: &str,
    property: &str,
    stage_id: &str,
) -> Result<Vec<String>, gmeow_errors::Diag> {
    let mut values = objects(ds, stage_iri, &iri(GMEOW, property))
        .into_iter()
        .map(|term| declaration_string(term, stage_id, property))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort();
    values.dedup();
    Ok(values)
}
