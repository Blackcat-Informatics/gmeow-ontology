// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage: native EL/DL reasoned closure + artifacts — the SOLE
//! reasoning pass.
//!
//! It reasons ONCE over the object-level EDB
//! ([`crate::stages::carrier::assemble_object_level_edb`]: ontology + imports +
//! statements + alignments + logic/relational-core/correspondence, WITHOUT the
//! meta/report graphs), canonicalizes it (RDFC-1.0) for transport-independent Skolem
//! witnesses, runs `gmeow_logic::reason::reason_all`, and serializes the three
//! committed artifacts via the `gmeow_logic::reason::artifacts` builders. The single
//! result also backs the bundle's `graph/reasoning` projection (dual carriage), so
//! the closure shipped in `gmeow.gts` and the committed files agree by construction —
//! there is no separate full-fold export leaf. Reasoning serializes under the pipeline
//! `ENGINE_LOCK` (this is the sole `Reason`-kind stage).

use std::collections::BTreeMap;
use std::sync::Arc;

use gmeow_logic::reason::artifacts::{
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
};
use gmeow_logic::reason::reason_all;
use gmeow_logic::result::ReasoningResult;
use gmeow_logic::result_rdf::{project_reasoning_result, GRAPH_REASONING};
use gmeow_rdf::{NativeRdfFormat, RdfDataset, RdfDatasetBuilder, RdfTerm};

use crate::bundle::{bundle_from_artifacts_over, PipelineHandle};
use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// COMMITTED logical path of the native told-vs-inferred closure (RDF 1.2). This is
/// the SOLE reasoning pass: it reasons once over the object-level EDB
/// ([`crate::stages::carrier::assemble_object_level_edb`]) and owns the committed
/// closure directly — there is no separate full-fold export leaf. The same result
/// also backs the `graph/reasoning` projection folded into the bundle (dual carriage),
/// so the closure shipped in `gmeow.gts` and the committed file agree by construction.
pub const CLOSURE_PATH: &str = "generated/logic/inferred-closure.rdf12.ttl";
/// COMMITTED logical path of the per-axiom proof-skeleton explanations (RDF 1.2).
pub const EXPLANATIONS_PATH: &str = "generated/logic/reasoning-explanations.rdf12.ttl";
/// COMMITTED logical path of the report-only native DL/EL crosscheck ledger.
pub const LEDGER_PATH: &str = "generated/logic/dl-el-crosscheck-report.ttl";

/// The reasoned artifacts a single `reason_all` produces: the three committed-style
/// Turtle blobs plus the typed [`ReasoningResult`] itself (the C7 typed handle's
/// payload and the source of the `graph/reasoning` projection).
pub struct ReasonArtifacts {
    /// The told-vs-inferred derived closure Turtle.
    pub closure: String,
    /// The per-axiom proof-skeleton explanations Turtle.
    pub explanations: String,
    /// The native DL·EL crosscheck ledger Turtle.
    pub ledger: String,
    /// The typed five-axis result (#1132 C7 handle payload).
    pub result: ReasoningResult,
}

/// Reason over a composed dataset (N-Quads bytes) and return the three artifacts plus
/// the typed [`ReasoningResult`]. Parses then delegates to [`reason_over_dataset`].
pub fn reason_artifacts(composed_nquads: &[u8]) -> Result<ReasonArtifacts, PipelineError> {
    let edb = gmeow_rdf::dataset_from_bytes(composed_nquads, NativeRdfFormat::NQuads)
        .map_err(|e| PipelineError::Parse(format!("reason input parse: {e}")))?;
    reason_over_dataset(edb.as_ref())
}

/// Reason over an in-memory EDB and return the three artifacts plus the typed
/// [`ReasoningResult`]. Canonicalizes the EDB (RDFC-1.0) BEFORE reasoning so the
/// content-addressed Skolem witnesses are transport-independent (carrier vs a
/// re-imported `gmeow.gts` yield byte-identical artifacts), then mirrors
/// `reason_native_artifacts` in non-merge mode (the regenerate path).
pub fn reason_over_dataset(edb: &RdfDataset) -> Result<ReasonArtifacts, PipelineError> {
    let canon_quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(edb).map_err(|e| {
        PipelineError::Stage {
            stage: "stage-reason".to_string(),
            message: format!("flatten EDB for canonicalization: {e}"),
        }
    })?;
    let canon_quads =
        gmeow_rdf::canonicalize_quads(canon_quads).map_err(|e| PipelineError::Stage {
            stage: "stage-reason".to_string(),
            message: format!("RDFC-1.0 canonicalize EDB: {e}"),
        })?;
    let canon =
        gmeow_rdf::dataset_from_oxigraph_quads(&canon_quads).map_err(|e| PipelineError::Stage {
            stage: "stage-reason".to_string(),
            message: format!("re-fold canonical quads: {e}"),
        })?;
    let result = reason_all(canon.as_ref()).map_err(|e| PipelineError::Stage {
        stage: "stage-reason".to_string(),
        message: format!("native reasoning failed: {e}"),
    })?;
    // Non-merge (the regenerate path): the closure is told-vs-inferred only.
    let closure = build_inferred_closure_ttl(&result, None).map_err(|e| PipelineError::Stage {
        stage: "stage-reason".to_string(),
        message: format!("closure serialization failed: {e}"),
    })?;
    let explanations = build_explanations_ttl(&result).map_err(|e| PipelineError::Stage {
        stage: "stage-reason".to_string(),
        message: format!("explanations serialization failed: {e}"),
    })?;
    let ledger = build_dl_el_ledger_ttl(&result);
    Ok(ReasonArtifacts {
        closure,
        explanations,
        ledger,
        result,
    })
}

/// Parse the closure Turtle into the default graph and FOLD the deterministic
/// `graph/reasoning` projection of `result` into the named graph [`GRAPH_REASONING`],
/// returning the dual-carriage dataset the reason stage's bundle backs. The closure
/// stays the default-graph contribution to the compose union; the reasoning
/// projection rides alongside as its own named graph (the typed handle's backing).
fn reason_dataset(
    closure_ttl: &str,
    result: &ReasoningResult,
) -> Result<Arc<RdfDataset>, PipelineError> {
    let closure_ds = gmeow_rdf::parse_dataset(closure_ttl.as_bytes(), "text/turtle", None)
        .map_err(|e| PipelineError::Parse(format!("reason closure parse: {e}")))?;
    let reasoning_nt = project_reasoning_result(result);
    let reasoning_ds =
        gmeow_rdf::parse_dataset(reasoning_nt.as_bytes(), "application/n-triples", None)
            .map_err(|e| PipelineError::Parse(format!("reason projection parse: {e}")))?;

    let mut builder = RdfDatasetBuilder::new();
    // The closure stays in the default graph (the compose-union contribution).
    builder.push_dataset(closure_ds.as_ref());
    // The graph/reasoning projection is routed into its own named graph.
    let graph = RdfTerm::Iri(GRAPH_REASONING.to_owned());
    for quad in reasoning_ds.owned_quads() {
        let mut routed = quad.clone();
        routed.graph_name = Some(graph.clone());
        builder.push_owned_quad(&routed);
    }
    builder
        .freeze()
        .map_err(|e| PipelineError::Parse(format!("freeze reason dual-carriage dataset: {e}")))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `reason` pipeline stage — the sole engine-lock-carrying stage.
pub struct ReasonStage {
    consumes: Vec<String>,
}

impl ReasonStage {
    /// Construct the stage. It reasons over the object-level EDB assembled from the
    /// compile-logic / mappings / source-load / statements producers (plus the on-disk
    /// authored / imports / alignments sources); the slice DAG's `stage-reason`
    /// `dataflowConsumes` mirrors this set.
    pub fn new() -> Self {
        Self {
            consumes: vec![
                "stage-compile-logic".to_string(),
                "stage-mappings".to_string(),
                "stage-source-load".to_string(),
                "stage-statements".to_string(),
            ],
        }
    }
}

impl Default for ReasonStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ReasonStage {
    fn id(&self) -> &str {
        "stage-reason"
    }
    fn kind(&self) -> StageKind {
        StageKind::Reason
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "reason.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // Reason ONCE over the object-level EDB (ontology + imports + statements +
        // alignments + logic/relational-core/correspondence), assembled in the SAME
        // graph layout the bundle carries but WITHOUT the meta/report graphs — they
        // assert no axioms, so excluding them is closure-isomorphic and makes the
        // Skolem witnesses a function of the ontology alone. This pass owns the
        // committed closure AND backs the bundle's `graph/reasoning`; there is no
        // second full-fold export leaf.
        let edb = crate::stages::carrier::assemble_object_level_edb(input.root, input.upstream)?;
        let reasoned = reason_over_dataset(edb.as_ref())?;
        // The CLOSURE is the reason stage's contribution to `gts_compose`'s union and
        // stays the dataset's DEFAULT graph. The EXPLANATIONS and LEDGER are diagnostic
        // REPORTS (proof skeletons / DL·EL crosscheck), NOT ontology facts; they stay
        // byte-lane only and are EXCLUDED from the compose union BY CONSTRUCTION. The
        // typed five-axis result rides BOTH as the `graph/reasoning` named graph (the
        // repo-free RDF projection) AND as the typed `PipelineHandle::Reasoning` handle
        // pinned to that graph (#1132 C7) — dual carriage.
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CLOSURE_PATH.to_string(), reasoned.closure.into_bytes());
        artifacts.insert(
            EXPLANATIONS_PATH.to_string(),
            reasoned.explanations.into_bytes(),
        );
        artifacts.insert(LEDGER_PATH.to_string(), reasoned.ledger.into_bytes());

        // Attach the typed Reasoning handle, pinned to the `graph/reasoning` named
        // graph's canonical digest. `pin_handle` HARD-fails on a digest mismatch, so a
        // handle that disagrees with its backing graph can never attach (fail-closed).
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            artifacts,
            gmeow_rdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result)),
                pinned,
            )
            .map_err(|e| PipelineError::Stage {
                stage: "stage-reason".to_string(),
                message: format!("pin Reasoning handle to <{GRAPH_REASONING}>: {e}"),
            })?;
        Ok(StageOutput {
            product: StageProduct::from_bundle(self.id(), Arc::new(bundle)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_produces_nonempty_artifacts_over_tiny_graph() {
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");

        // Wiring check: the native reasoner ran end-to-end and the three
        // builders produced their artifacts (each carries at least its generated
        // header), and the closure contains a concrete derived transitive
        // subclass axiom.
        for (name, ttl) in [
            ("closure", &reasoned.closure),
            ("explanations", &reasoned.explanations),
            ("ledger", &reasoned.ledger),
        ] {
            assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
        }
        assert!(reasoned.closure.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
    }

    #[test]
    fn reason_stage_pins_a_reasoning_handle_to_graph_reasoning() {
        // The dual-carriage dataset folds the graph/reasoning projection as a named
        // graph and the typed handle pins to it (the digest invariant must hold).
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
<http://example.org/B> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result).expect("dual dataset");
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            gmeow_rdf::provenance::DatasetProvenance::new(),
        );
        let pinned = bundle.graph_digest(GRAPH_REASONING);
        bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result.clone())),
                pinned,
            )
            .expect("pin Reasoning handle to its backing graph");
        let entry = bundle.handle(GRAPH_REASONING).expect("handle attached");
        let PipelineHandle::Reasoning(r) = &entry.payload else {
            panic!("the handle arm is Reasoning");
        };
        assert_eq!(
            r.as_ref(),
            &reasoned.result,
            "the typed result is carried verbatim"
        );
        // The graph/reasoning named graph is non-empty (the projection landed).
        assert_ne!(
            bundle.graph_digest(GRAPH_REASONING),
            bundle.graph_digest("https://blackcatinformatics.ca/gmeow/graph/absent"),
            "graph/reasoning carries the projection"
        );
    }

    #[test]
    fn pin_handle_hard_fails_on_a_digest_mismatch() {
        let nq = br#"
<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/B> <http://gmeow.example/w> .
"#;
        let reasoned = reason_artifacts(nq).expect("reason");
        let dataset = reason_dataset(&reasoned.closure, &reasoned.result).expect("dual dataset");
        let mut bundle = bundle_from_artifacts_over(
            dataset,
            BTreeMap::new(),
            gmeow_rdf::provenance::DatasetProvenance::new(),
        );
        // A WRONG pinned digest must be rejected (no silently-stale handle).
        let wrong = gmeow_rdf::ContentDigest::of(b"not the backing graph");
        let err = bundle
            .pin_handle(
                GRAPH_REASONING,
                PipelineHandle::Reasoning(Arc::new(reasoned.result)),
                wrong,
            )
            .expect_err("a mismatched pin must hard-fail");
        let _ = err;
    }
}
