// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `reason` stage (#861 P3): native EL/DL reasoned closure + artifacts.
//!
//! This is pure WIRING of the existing Rust reasoner — no port. It unions the
//! upstream transforms (base graph + statement layer + mappings) into an oxigraph
//! store, wraps it as a `gmeow_rdf::oxigraph::OxigraphStore` (which `impl
//! RdfStore`), runs `gmeow_logic::reason::reason_all`, and serializes the three
//! committed artifacts via the `gmeow_logic::reason::artifacts` builders — the
//! exact functions `reason_native_artifacts` calls. Reasoning serializes under
//! the pipeline `ENGINE_LOCK` (this is the sole `Reason`-kind stage).

use std::collections::BTreeMap;

use gmeow_logic::reason::artifacts::{
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
};
use gmeow_logic::reason::reason_all;
use gmeow_rdf::oxigraph::OxigraphStore;
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

/// Committed logical path of the native told-vs-inferred closure.
pub const CLOSURE_PATH: &str = "generated/logic/inferred-closure.rdf12.ttl";
/// Committed logical path of the per-axiom proof-skeleton explanations.
pub const EXPLANATIONS_PATH: &str = "generated/logic/reasoning-explanations.rdf12.ttl";
/// Committed logical path of the native DL/EL crosscheck ledger.
pub const LEDGER_PATH: &str = "generated/logic/dl-el-crosscheck-report.ttl";

/// Reason over a composed dataset (N-Quads bytes) and return the three artifacts
/// `(closure, explanations, ledger)`. Mirrors `reason_native_artifacts` in
/// non-merge mode (the regenerate path).
pub fn reason_artifacts(composed_nquads: &[u8]) -> Result<(String, String, String), PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;
    for quad in RdfParser::from_format(RdfFormat::NQuads)
        .lenient()
        .for_reader(composed_nquads)
    {
        let quad = quad.map_err(|e| PipelineError::Parse(format!("reason input parse: {e}")))?;
        store
            .insert(&quad)
            .map_err(|e| PipelineError::Parse(format!("store insert failed: {e}")))?;
    }
    let edb = OxigraphStore::new(&store);
    let result = reason_all(&edb).map_err(|e| PipelineError::Stage {
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
    use crate::stages::source_load::{load_authored_store, store_to_nquads};
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn reason_produces_nonempty_artifacts_over_the_authored_graph() {
        let root = repo_root();
        // Reason over the authored base graph (imports + slice TBox) — the
        // reasoner's EDB. (Statements/mappings add ABox metadata; the TBox closure
        // is exercised by the base alone.)
        let store = load_authored_store(&root).unwrap();
        let nq = store_to_nquads(&store).unwrap();
        let (closure, explanations, ledger) = reason_artifacts(&nq).expect("reason");

        // Wiring check: the native reasoner ran end-to-end and the three
        // builders produced their artifacts (each carries at least its generated
        // header). Closure-CONTENT fold-parity against the committed logic
        // artifacts is a P6 gate (and is subject to the committed-vs-local
        // reasoner env-skew), so it is not asserted here.
        for (name, ttl) in [
            ("closure", &closure),
            ("explanations", &explanations),
            ("ledger", &ledger),
        ] {
            assert!(!ttl.trim().is_empty(), "{name} artifact is empty");
        }
    }
}
