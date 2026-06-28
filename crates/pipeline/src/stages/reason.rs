// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage (#861 P3): native EL/DL reasoned closure + artifacts.
//!
//! This is pure WIRING of the existing Rust reasoner — no port. It unions the
//! upstream transforms (base graph + statement layer + mappings) into an oxigraph
//! dataset, runs `gmeow_logic::reason::reason_all`, and serializes the three
//! committed artifacts via the `gmeow_logic::reason::artifacts` builders — the
//! exact functions `reason_native_artifacts` calls. Reasoning serializes under
//! the pipeline `ENGINE_LOCK` (this is the sole `Reason`-kind stage).

use std::collections::BTreeMap;

use gmeow_logic::reason::artifacts::{
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
};
use gmeow_logic::reason::reason_all;
use gmeow_rdf::NativeRdfFormat;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// INTERNAL logical path of the reasoned closure this stage folds into the
/// composed dataset (`pipeline/`-prefixed, so it is in-memory dataflow only and is
/// NOT reconciled as a committed artifact — `run_full` skips the `pipeline/`
/// prefix). The COMMITTED `generated/logic/inferred-closure.rdf12.ttl` is owned by
/// the `stage-export-logic` leaf, which reasons over the FULL snapshot fold; this
/// stage reasons over the EARLY composed subset purely to seed the reasoned-closure
/// material `docs-render` consumes via `composed.nq`. Emitting it under a committed
/// path would collide with the leaf and drift the parity gate (small-subset closure
/// ≠ full-fold closure).
pub const CLOSURE_PATH: &str = "pipeline/reason-closure.rdf12.ttl";
/// INTERNAL logical path of the per-axiom proof-skeleton explanations (see
/// [`CLOSURE_PATH`]; the committed counterpart is owned by `stage-export-logic`).
pub const EXPLANATIONS_PATH: &str = "pipeline/reason-explanations.rdf12.ttl";
/// INTERNAL logical path of the native DL/EL crosscheck ledger (see
/// [`CLOSURE_PATH`]; the committed counterpart is owned by `stage-export-logic`).
pub const LEDGER_PATH: &str = "pipeline/reason-dl-el-crosscheck-report.ttl";

/// Reason over a composed dataset (N-Quads bytes) and return the three artifacts
/// `(closure, explanations, ledger)`. Mirrors `reason_native_artifacts` in
/// non-merge mode (the regenerate path).
pub fn reason_artifacts(composed_nquads: &[u8]) -> Result<(String, String, String), PipelineError> {
    let edb = gmeow_rdf::dataset_from_bytes(composed_nquads, NativeRdfFormat::NQuads)
        .map_err(|e| PipelineError::Parse(format!("reason input parse: {e}")))?;
    let result = reason_all(edb.as_ref()).map_err(|e| PipelineError::Stage {
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
    Ok((closure, explanations, ledger))
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `reason` pipeline stage — the sole engine-lock-carrying stage.
pub struct ReasonStage {
    consumes: Vec<String>,
}

impl ReasonStage {
    /// Construct the stage. It reasons over the union of the upstream transforms
    /// (base graph + statement layer + mappings); the slice DAG's stage-reason
    /// dataflowConsumes is reconciled to this set when the full pipeline is wired.
    pub fn new() -> Self {
        Self {
            consumes: vec![
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
        // Union the upstream transforms into the dataset the reasoner consumes.
        let composed = crate::stages::gts_compose::compose(input.upstream)?;
        let (closure, explanations, ledger) = reason_artifacts(&composed)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(CLOSURE_PATH.to_string(), closure.into_bytes());
        artifacts.insert(EXPLANATIONS_PATH.to_string(), explanations.into_bytes());
        artifacts.insert(LEDGER_PATH.to_string(), ledger.into_bytes());
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
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
        let (closure, explanations, ledger) = reason_artifacts(nq).expect("reason");

        // Wiring check: the native reasoner ran end-to-end and the three
        // builders produced their artifacts (each carries at least its generated
        // header), and the closure contains a concrete derived transitive
        // subclass axiom.
        for (name, ttl) in [
            ("closure", &closure),
            ("explanations", &explanations),
            ("ledger", &ledger),
        ] {
            assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
        }
        assert!(closure.contains("<http://example.org/A> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.org/C> ."));
    }
}
