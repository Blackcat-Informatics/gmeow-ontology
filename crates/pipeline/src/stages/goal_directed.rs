// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `goal-directed` stage: run the native proof-carrying full-FOL backward engine over
//! the AUTHORED `logic:ReasoningProgram` demonstrator corpus and fold its checked answers +
//! proof derivations into the bundle's `graph/goal-directed` named graph.
//!
//! This is the PRODUCTION consumer that makes the backward engine non-dark: without it the
//! engine (`gmeow_logic::physical::resolve_fol` + its Curry–Howard `check`) would only ever
//! run in tests. The stage reads the compiled [`LogicProgram`](gmeow_logic_compile::ir::LogicProgram)
//! off the `stage-compile-logic` product's typed logic publication (its
//! `reasoning_programs` field is the authored corpus, folded there by the compile-logic stage
//! from `slices/grounding/logic/examples/reasoning-programs.ttl`), reads the reasoned
//! `rdfs:subClassOf` closure off the `stage-reason` product's typed `PipelineHandle::Reasoning`
//! handle (the order-sorted math-subsort demonstrator's `subsort_edges`), and calls
//! `gmeow_logic::goal_directed::evaluate_reasoning_programs` — which compiles each program,
//! validates every answer's proof, and returns RDF-serializable data — then routes the
//! projected N-Triples into [`GRAPH_GOAL_DIRECTED`]. `stage-snapshot`'s
//! `assemble_carrier` folds that named graph into `gmeow.gts` (the
//! shippable deliverable), so a repo-free consumer reads every proof-checked backward answer
//! straight out of the bundle.
//!
//! The demonstrator corpus is a SET of authored `logic:ReasoningProgram` cells, not a single
//! hardcoded program or a Rust constant: appending a demonstrator to the authored corpus
//! reaches the bundle through this same stage with no stage change.

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::goal_directed::{evaluate_reasoning_programs, project_goal_directed};
use gmeow_logic::result_rdf::GRAPH_REASONING;

use crate::bundle::PipelineHandle;
use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-goal-directed".to_string(),
        message: message.into(),
    })
}

/// The `rdfs:subClassOf` IRI — the predicate every subsort-lattice covering edge is
/// filtered on out of the reasoned closure's derived (non-EDB) axioms.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// Strip a native-engine term's optional surrounding `<>` bracket pair (mirrors the
/// small `bare_iri` helper duplicated at `gmeow_logic::reason::artifacts`/`verify`):
/// an [`gmeow_logic::reason::el::InferredAxiom`]'s subject/predicate/object fields are
/// NOT uniformly bracketed across every derivation arm, so callers reading them back
/// must strip defensively rather than assume one convention.
fn bare_iri(value: &str) -> &str {
    value
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(value)
}

/// Read the compiled [`LogicProgram`](gmeow_logic_compile::ir::LogicProgram)'s authored
/// `reasoning_programs` off the `stage-compile-logic` upstream product's typed
/// logic publication. HARD-fails (no-optionality) when the upstream product is
/// missing, the handle is absent or the wrong arm, or the compiled program carries zero
/// authored programs — never a silent empty-corpus fallback to the retired Rust constants.
fn read_reasoning_programs(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Arc<gmeow_logic_compile::ir::LogicProgram>, gmeow_errors::Diag> {
    let compile_logic = upstream.get("stage-compile-logic").ok_or_else(|| {
        stage_err(
            "missing stage-compile-logic product — the compiled authored reasoning programs \
             are required",
        )
    })?;
    let entry = compile_logic
        .bundle()
        .handle(crate::stages::compile_logic::GRAPH_LOGIC)
        .ok_or_else(|| {
            stage_err(format!(
                "stage-compile-logic product carries no typed handle at <{}>",
                crate::stages::compile_logic::GRAPH_LOGIC
            ))
        })?;
    let program = entry.payload.logic_program().ok_or_else(|| {
        stage_err(format!(
            "the handle at <{}> carries no compiled logic program",
            crate::stages::compile_logic::GRAPH_LOGIC
        ))
    })?;
    if program.reasoning_programs.is_empty() {
        return Err(stage_err(
            "the compiled LogicProgram carries zero authored logic:ReasoningProgram \
             individuals — the goal-directed demonstrator corpus is missing (never fall back \
             to a hardcoded Rust corpus)",
        ));
    }
    Ok(Arc::clone(program))
}

/// Read the reasoned `rdfs:subClassOf` closure's derived (non-EDB) axioms off the
/// `stage-reason` upstream product's typed `PipelineHandle::Reasoning` handle, filtered to
/// `rdfs:subClassOf` subject/object pairs — the order-sorted math-subsort demonstrator's
/// `subsort_edges` (e.g. the transitively-derived `math:Integer ⊑ math:RealNumber`, entailed
/// from the authored `math:Integer ⊑ math:RationalNumber ⊑ math:RealNumber` told edges, so it
/// is a DERIVED, not told, axiom). HARD-fails when the upstream product or handle is missing —
/// never a silent empty-edges fallback that would make the subsort demonstrator vacuous.
fn read_subsort_edges(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<(String, String)>, gmeow_errors::Diag> {
    let reason = upstream.get("stage-reason").ok_or_else(|| {
        stage_err(
            "missing stage-reason product — the reasoned rdfs:subClassOf closure is required \
             for the order-sorted math-subsort demonstrator",
        )
    })?;
    let entry = reason.bundle().handle(GRAPH_REASONING).ok_or_else(|| {
        stage_err(format!(
            "stage-reason product carries no typed handle at <{GRAPH_REASONING}>"
        ))
    })?;
    let PipelineHandle::Reasoning(result) = &entry.payload else {
        return Err(stage_err(format!(
            "the handle at <{GRAPH_REASONING}> is not the Reasoning arm"
        )));
    };
    let mut edges: Vec<(String, String)> =
        result
            .inferred()
            .iter()
            .filter(|axiom| bare_iri(&axiom.predicate) == RDFS_SUBCLASS_OF)
            .map(|axiom| {
                Ok((
                bare_iri(&axiom.subject).to_owned(),
                axiom.object.as_iri().ok_or_else(|| stage_err(format!(
                    "reasoned subclass edge from {:?} has a non-resource class object {:?}",
                    axiom.subject, axiom.object
                )))?.to_owned(),
            ))
            })
            .collect::<gmeow_errors::Result<_>>()?;
    edges.sort();
    edges.dedup();
    if edges.is_empty() {
        return Err(stage_err(
            "the reasoned rdfs:subClassOf closure carries ZERO derived edges — the \
             order-sorted math-subsort demonstrator cannot be driven (never a silent \
             empty-edges fallback that would make it vacuous)",
        ));
    }
    Ok(edges)
}

/// The bundle-internal named graph into which checked goal-directed answers and proof
/// derivations are folded. Registered fanout emits the `generated/goal-directed/`
/// goldens from the same carrier. As a sibling of `graph/reasoning`, it is a queryable
/// native-engine result inside `gmeow.gts` and is excluded from the object-level EDB
/// because it asserts derived answers rather than axioms.
pub const GRAPH_GOAL_DIRECTED: &str = "https://blackcatinformatics.ca/gmeow/graph/goal-directed";

/// The `goal-directed` pipeline stage.
pub struct GoalDirectedStage {
    consumes: Vec<String>,
    entities: Vec<(String, Vec<String>)>,
}

impl GoalDirectedStage {
    /// Construct the stage. It consumes `stage-compile-logic` (whose typed Logic handle
    /// carries the authored `logic:ReasoningProgram` demonstrator corpus) and `stage-reason`
    /// (whose typed Reasoning handle carries the reasoned `rdfs:subClassOf` closure the
    /// order-sorted math-subsort demonstrator's unification lattice is seeded from).
    ///
    /// Typed dataflow (artifact-level): from `stage-reason` it reads ONLY the
    /// `graph/reasoning` named graph (the Reasoning handle's backing graph) — never that
    /// product's committed closure/explanations/ledger byte lanes — so a change to those
    /// alone leaves the narrowed entity's digest unchanged and skips re-running this stage.
    /// `stage-compile-logic` is consumed whole (no narrowing): the Logic handle it reads is
    /// pinned to `graph/logic`, so any change there already reruns `stage-compile-logic`
    /// itself.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-reason".to_string(),
            ],
            entities: vec![(
                "stage-reason".to_string(),
                vec![GRAPH_REASONING.to_string()],
            )],
        }
    }
}

impl Default for GoalDirectedStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GoalDirectedStage {
    fn id(&self) -> &str {
        "stage-goal-directed"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn cache_policy(&self) -> CachePolicy {
        // The backward engine rebuilds this small proof-carrying result faster than a
        // structural cache hydrate would reparse + re-key its named graph, and Recompute
        // keeps the proof-check gate live on every run (an unchecked answer HARD-fails in
        // the façade). Mirrors stage-reason's Recompute rationale.
        CachePolicy::Recompute
    }
    fn consumed_entities(&self) -> &[(String, Vec<String>)] {
        &self.entities
    }
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v2: the stage now compiles the AUTHORED logic:ReasoningProgram corpus (read off
        // the stage-compile-logic Logic handle) against the reasoned rdfs:subClassOf closure
        // (read off the stage-reason Reasoning handle) via
        // gmeow_logic::goal_directed::evaluate_reasoning_programs, instead of calling
        // evaluate_shipped_demonstrators() over the hand-interned Rust constants.
        "goal-directed.v2"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Read the compiled authored demonstrator corpus and the reasoned subsort closure
        // off their respective typed upstream handles — both HARD-fail (no fallback to the
        // retired Rust constants) when the upstream product/handle is missing or the corpus
        // is empty.
        let program = read_reasoning_programs(input.upstream)?;
        let subsort_edges = read_subsort_edges(input.upstream)?;

        // Evaluate the authored reasoning-program corpus through the proof-carrying backward
        // engine. Every answer's proof is `check`-validated inside the façade, so a proof
        // that does not re-derive its answer atom HARD-fails here (fail-closed).
        let evals = evaluate_reasoning_programs(&program.reasoning_programs, &subsort_edges)?;
        let nt = project_goal_directed(&evals);
        // Route the projected N-Triples into the bundle-internal graph/goal-directed named
        // graph (the stage's sole attach delta).
        let dataset = crate::stages::carrier::parse_into_graph(
            nt.as_bytes(),
            "application/n-triples",
            GRAPH_GOAL_DIRECTED,
        )?;
        let bundle = crate::bundle::bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        Ok(StageOutput::new(StageProduct::from_bundle(
            self.id(),
            Arc::new(bundle),
        )))
    }
}

#[path = "goal_directed.tests.rs"]
#[cfg(test)]
mod tests;
