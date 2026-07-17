// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `goal-directed` stage: run the native proof-carrying full-FOL backward engine over
//! the AUTHORED `logic:ReasoningProgram` demonstrator corpus and fold its checked answers +
//! proof derivations into the bundle's `graph/goal-directed` named graph.
//!
//! This is the PRODUCTION consumer that makes the backward engine non-dark: without it the
//! engine (`gmeow_logic::physical::resolve_fol` + its Curry–Howard `check`) would only ever
//! run in tests. The stage reads the compiled [`LogicProgram`](gmeow_logic_compile::ir::LogicProgram)
//! off the `stage-compile-logic` product's typed `PipelineHandle::Logic` handle (its
//! `reasoning_programs` field is the authored corpus, folded there by the compile-logic stage
//! from `slices/grounding/logic/examples/reasoning-programs.ttl`), reads the reasoned
//! `rdfs:subClassOf` closure off the `stage-reason` product's typed `PipelineHandle::Reasoning`
//! handle (the order-sorted math-subsort demonstrator's `subsort_edges`), and calls
//! `gmeow_logic::goal_directed::evaluate_reasoning_programs` — which compiles each program,
//! validates every answer's proof, and returns RDF-serializable data — then routes the
//! projected N-Triples into [`GRAPH_GOAL_DIRECTED`]. `stage-snapshot`'s
//! [`crate::stages::carrier::assemble_carrier`] folds that named graph into `gmeow.gts` (the
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
/// `PipelineHandle::Logic` handle. HARD-fails (no-optionality) when the upstream product is
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
    let PipelineHandle::Logic(program) = &entry.payload else {
        return Err(stage_err(format!(
            "the handle at <{}> is not the Logic arm",
            crate::stages::compile_logic::GRAPH_LOGIC
        )));
    };
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
    let mut edges: Vec<(String, String)> = result
        .inferred()
        .iter()
        .filter(|axiom| bare_iri(&axiom.predicate) == RDFS_SUBCLASS_OF)
        .map(|axiom| {
            (
                bare_iri(&axiom.subject).to_owned(),
                bare_iri(&axiom.object).to_owned(),
            )
        })
        .collect();
    edges.sort();
    edges.dedup();
    Ok(edges)
}

/// The bundle-internal named graph the checked goal-directed answers + proof derivations are
/// folded into (dual carriage with no committed byte artifact in this task — the
/// `generated/goal-directed/` fanout goldens are produced by Task 9's regenerate). A sibling
/// of `graph/reasoning`: a queryable projection of a native engine's result that ships inside
/// `gmeow.gts`, excluded from the object-level EDB (it asserts derived answers, not axioms).
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Build a REAL `stage-compile-logic` upstream product carrying a typed `Logic` handle
    /// whose `reasoning_programs` are the AUTHORED demonstrator corpus
    /// (`REASONING_PROGRAMS_EXAMPLE_PATH`), parsed through the exact same production
    /// frontend entry point (`parse_logic_path`) the real stage uses — never a hand-built
    /// fake corpus. Its `graph/logic` backing graph carries only a placeholder triple: the
    /// handle pin only requires digest self-consistency (proven at construction), not full
    /// program re-derivability, and this stage never reads that graph directly (only the
    /// handle payload).
    fn compile_logic_upstream_with_reasoning_programs() -> StageProduct {
        let path = repo_root().join(crate::stages::compile_logic::REASONING_PROGRAMS_EXAMPLE_PATH);
        let (parsed, _diags) = gmeow_logic_compile::frontend::parse_logic_path(&path, None)
            .expect("parse the authored reasoning-programs cell");
        assert!(
            !parsed.reasoning_programs.is_empty(),
            "the authored cell must carry at least one logic:ReasoningProgram"
        );
        let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None)
            .with_reasoning_programs(parsed.reasoning_programs);
        let dataset = crate::stages::carrier::parse_into_graph(
            b"<https://example.test/s> <https://example.test/p> <https://example.test/o> .\n",
            "application/n-triples",
            crate::stages::compile_logic::GRAPH_LOGIC,
        )
        .expect("route the placeholder graph/logic triple");
        let mut bundle = crate::bundle::bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
        bundle
            .pin_handle(
                crate::stages::compile_logic::GRAPH_LOGIC,
                PipelineHandle::Logic(Arc::new(program)),
                pinned,
            )
            .expect("pin the Logic handle to graph/logic");
        StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle))
    }

    /// Build a REAL `stage-reason` upstream product carrying a typed `Reasoning` handle
    /// whose `rdfs:subClassOf` closure includes the math: subsort tower — reasoned through
    /// the exact same production `reason_artifacts` entry point the real stage uses, over a
    /// tiny synthetic EDB carrying only the tower's TOLD edges (not the whole ontology, so
    /// the test stays cheap). This proves `math:Integer ⊑ math:RealNumber` is genuinely
    /// DERIVED by the reasoner's transitive closure, never hardcoded.
    fn reason_upstream_with_math_tower() -> StageProduct {
        const NATURAL: &str = "https://blackcatinformatics.ca/math/NaturalNumber";
        const INTEGER: &str = "https://blackcatinformatics.ca/math/Integer";
        const RATIONAL: &str = "https://blackcatinformatics.ca/math/RationalNumber";
        const REAL: &str = "https://blackcatinformatics.ca/math/RealNumber";
        const COMPLEX: &str = "https://blackcatinformatics.ca/math/ComplexNumber";
        const WORLD: &str = "https://blackcatinformatics.ca/gmeow/graph/authored-default";
        let nq = format!(
            "<{NATURAL}> <{RDFS_SUBCLASS_OF}> <{INTEGER}> <{WORLD}> .\n\
             <{INTEGER}> <{RDFS_SUBCLASS_OF}> <{RATIONAL}> <{WORLD}> .\n\
             <{RATIONAL}> <{RDFS_SUBCLASS_OF}> <{REAL}> <{WORLD}> .\n\
             <{REAL}> <{RDFS_SUBCLASS_OF}> <{COMPLEX}> <{WORLD}> .\n"
        );
        let reasoned = crate::stages::reason::reason_artifacts(nq.as_bytes())
            .expect("reason the synthetic math tower EDB");
        // Confirm the transitive derivation actually happened (the whole point of routing
        // subsort_edges through the REASONED closure rather than a hardcoded tower):
        // Integer ⊑ RealNumber must be a DERIVED (non-EDB) axiom, entailed from the told
        // Integer⊑Rational and Rational⊑Real edges.
        assert!(
            reasoned.result.inferred().iter().any(|axiom| {
                !axiom.is_edb
                    && bare_iri(&axiom.predicate) == RDFS_SUBCLASS_OF
                    && bare_iri(&axiom.subject) == INTEGER
                    && bare_iri(&axiom.object) == REAL
            }),
            "math:Integer subClassOf math:RealNumber must be genuinely DERIVED by the reasoner"
        );
        let reasoning_nt = gmeow_logic::result_rdf::project_reasoning_result(&reasoned.result);
        let dataset = crate::stages::carrier::parse_into_graph(
            reasoning_nt.as_bytes(),
            "application/n-triples",
            GRAPH_REASONING,
        )
        .expect("route the graph/reasoning projection");
        let mut bundle = crate::bundle::bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result)),
                pinned,
            )
            .expect("pin the Reasoning handle to graph/reasoning");
        StageProduct::from_bundle("stage-reason", Arc::new(bundle))
    }

    fn real_upstream() -> BTreeMap<String, StageProduct> {
        let mut upstream = BTreeMap::new();
        upstream.insert(
            "stage-compile-logic".to_string(),
            compile_logic_upstream_with_reasoning_programs(),
        );
        upstream.insert(
            "stage-reason".to_string(),
            reason_upstream_with_math_tower(),
        );
        upstream
    }

    #[test]
    fn goal_directed_stage_attaches_a_nonempty_goal_directed_graph() {
        let stage = GoalDirectedStage::new();
        let upstream = real_upstream();
        let root = std::path::Path::new(".");
        let out = stage
            .run(StageInput {
                root,
                upstream: &upstream,
            })
            .expect("goal-directed run");
        let dataset = out.product.bundle().dataset();
        let graph = dataset.project_named_graph(GRAPH_GOAL_DIRECTED);
        let quads: Vec<_> = graph.owned_quads().collect();
        assert!(
            !quads.is_empty(),
            "the stage attaches a non-empty graph/goal-directed"
        );
        // The authored examples' namespace (slices/grounding/logic/examples/reasoning-programs.ttl):
        // every relation/constant/function symbol is a REAL IRI (never a bare local name), so
        // the rendered answer atoms carry it in full.
        const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";
        // The graph carries the Peano demonstrator's ground answer atom + a
        // proof-derivation IRI (the proof reached the bundle, not just the answer).
        let peano_atom = format!(
            "{EX}add({EX}s({EX}s({EX}zero)),{EX}s({EX}zero),{EX}s({EX}s({EX}s({EX}zero))))"
        );
        let has_atom = quads.iter().any(
            |q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == peano_atom),
        );
        assert!(has_atom, "the ground answer atom is in graph/goal-directed");
        let has_derivation = quads
            .iter()
            .any(|q| q.predicate == "https://blackcatinformatics.ca/gmeow/goalDirectedDerivation");
        assert!(
            has_derivation,
            "a proof-derivation IRI is in graph/goal-directed"
        );

        // The structured member/append demonstrator's cons-list answer atom rode through.
        let member_atom =
            format!("{EX}member({EX}a,{EX}cons({EX}a,{EX}cons({EX}b,{EX}cons({EX}c,{EX}nil))))");
        let has_structured = quads.iter().any(
            |q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == member_atom),
        );
        assert!(
            has_structured,
            "a structured cons-list membership answer is in graph/goal-directed"
        );

        // The three-valued SLG-WFS negation demonstrator: an `undefined` loop verdict AND both
        // founded verdicts reached the graph — SLG-WFS is observable (non-dark).
        let verdict_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedVerdict";
        let verdict_values: Vec<&str> = quads
            .iter()
            .filter(|q| q.predicate == verdict_pred)
            .filter_map(|q| match &q.object {
                purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            verdict_values.contains(&"undefined"),
            "an undefined WFS verdict is in graph/goal-directed: {verdict_values:?}"
        );
        assert!(
            verdict_values.contains(&"true") && verdict_values.contains(&"false"),
            "founded true/false WFS verdicts are in graph/goal-directed: {verdict_values:?}"
        );

        // The order-sorted (ℤ ⊑ ℝ) demonstrator's subsort-unified answer atom rode through.
        let subsort_atom = format!("{EX}p({EX}one)");
        let has_subsort = quads.iter().any(
            |q| matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == subsort_atom),
        );
        assert!(
            has_subsort,
            "the order-sorted subsort-unified answer p(one) is in graph/goal-directed"
        );

        // R6: the math-subsort-control program (`ex:mathSubsortControl`) projects a query
        // node with status "ok" and ZERO answer atoms — a positive presence-of-absence: the
        // sort lattice actively refused the Integer-sorted constant for the incomparable
        // Set-sorted variable, rather than the program being silently skipped/empty.
        let name_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedName";
        let status_pred = "https://blackcatinformatics.ca/gmeow/goalDirectedStatus";
        let has_answer_pred = "https://blackcatinformatics.ca/gmeow/hasGoalDirectedAnswer";
        let control_query = quads
            .iter()
            .find(|q| {
                q.predicate == name_pred
                    && matches!(&q.object, purrdf::RdfTerm::Literal(l) if l.lexical_form == "mathSubsortControl")
            })
            .map(|q| q.subject.clone())
            .expect("the mathSubsortControl query node is in graph/goal-directed");
        let control_status = quads
            .iter()
            .find(|q| q.subject == control_query && q.predicate == status_pred)
            .and_then(|q| match &q.object {
                purrdf::RdfTerm::Literal(l) => Some(l.lexical_form.as_str()),
                _ => None,
            });
        assert_eq!(
            control_status,
            Some("ok"),
            "the control program's status is ok (not partial/exhausted)"
        );
        assert!(
            !quads
                .iter()
                .any(|q| q.subject == control_query && q.predicate == has_answer_pred),
            "the control program has ZERO hasGoalDirectedAnswer edges (presence-of-absence)"
        );
    }

    #[test]
    fn run_hard_fails_when_stage_compile_logic_is_missing() {
        let stage = GoalDirectedStage::new();
        let mut upstream = BTreeMap::new();
        upstream.insert(
            "stage-reason".to_string(),
            reason_upstream_with_math_tower(),
        );
        let root = std::path::Path::new(".");
        let result = stage.run(StageInput {
            root,
            upstream: &upstream,
        });
        let Err(err) = result else {
            panic!("a missing stage-compile-logic product must HARD-fail, never fall back");
        };
        assert!(format!("{err:?}").contains("stage-compile-logic"));
    }

    #[test]
    fn run_hard_fails_when_stage_reason_is_missing() {
        let stage = GoalDirectedStage::new();
        let mut upstream = BTreeMap::new();
        upstream.insert(
            "stage-compile-logic".to_string(),
            compile_logic_upstream_with_reasoning_programs(),
        );
        let root = std::path::Path::new(".");
        let result = stage.run(StageInput {
            root,
            upstream: &upstream,
        });
        let Err(err) = result else {
            panic!("a missing stage-reason product must HARD-fail, never fall back");
        };
        assert!(format!("{err:?}").contains("stage-reason"));
    }

    #[test]
    fn run_hard_fails_when_the_logic_handle_carries_zero_reasoning_programs() {
        let stage = GoalDirectedStage::new();
        let program = gmeow_logic_compile::ir::LogicProgram::new(vec![], vec![], vec![], None);
        let dataset = crate::stages::carrier::parse_into_graph(
            b"<https://example.test/s> <https://example.test/p> <https://example.test/o> .\n",
            "application/n-triples",
            crate::stages::compile_logic::GRAPH_LOGIC,
        )
        .expect("route the placeholder graph/logic triple");
        let mut bundle = crate::bundle::bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            purrdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(crate::stages::compile_logic::GRAPH_LOGIC);
        bundle
            .pin_handle(
                crate::stages::compile_logic::GRAPH_LOGIC,
                PipelineHandle::Logic(Arc::new(program)),
                pinned,
            )
            .expect("pin the Logic handle to graph/logic");
        let mut upstream = BTreeMap::new();
        upstream.insert(
            "stage-compile-logic".to_string(),
            StageProduct::from_bundle("stage-compile-logic", Arc::new(bundle)),
        );
        upstream.insert(
            "stage-reason".to_string(),
            reason_upstream_with_math_tower(),
        );
        let root = std::path::Path::new(".");
        let result = stage.run(StageInput {
            root,
            upstream: &upstream,
        });
        let Err(err) = result else {
            panic!("zero authored reasoning programs must HARD-fail, never an empty result");
        };
        let msg = format!("{err:?}");
        assert!(msg.contains("reasoning_programs") || msg.contains("ReasoningProgram"));
    }
}
