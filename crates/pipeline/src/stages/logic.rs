// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `logic` export leaf (#861 P6 parity): native EL/DL reasoning over the FULL
//! snapshot fold.
//!
//! The committed logic artifacts (`generated/logic/inferred-closure.rdf12.ttl`,
//! `reasoning-explanations.rdf12.ttl`, `dl-el-crosscheck-report.ttl`) are minted by
//! `native_reason_gen.py`, which calls
//! `gmeow_logic.reason_native_artifacts(gmeow.gts, merge=False)` — i.e. it reasons
//! over the COMMITTED `gmeow.gts` bundle (the full structured snapshot fold, ~66.8k
//! quads) through the native EL/DL reasoner and serializes the three artifacts via
//! the `gmeow_logic::reason::artifacts` builders.
//!
//! The earlier `stage-reason` reasons over the EARLY composed N-Quads subset (a
//! smaller graph), so its closure diverges from the committed one. This leaf closes
//! that gap: it reads THIS run's `stage-snapshot` fold bytes (the same single-pass
//! source every other export leaf consumes) and feeds them through the IDENTICAL
//! `import_gts_events` → `reason_all` → `build_*_ttl` path that
//! `reason_native_artifacts` uses — so the three artifacts reproduce the
//! committed bytes exactly.
//!
//! ## Engine lock
//!
//! This is an [`StageKind::ExportLeaf`], so the scheduler does NOT auto-acquire the
//! pipeline [`crate::ENGINE_LOCK`] for it (only [`StageKind::Reason`] carries it).
//! The native reasoner drives the global Nemo chase, which must never run
//! concurrently with another reasoner instance, so this leaf acquires `ENGINE_LOCK`
//! explicitly around the reasoning call — exactly as `stage-reason` is serialized.

use std::collections::BTreeMap;

use gmeow_logic::reason::artifacts::{
    build_dl_el_ledger_ttl, build_explanations_ttl, build_inferred_closure_ttl,
};
use gmeow_logic::reason::reason_all;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::scheduler::ENGINE_LOCK;

/// Committed logical path of the native told-vs-inferred closure (RDF 1.2).
pub const CLOSURE_PATH: &str = "generated/logic/inferred-closure.rdf12.ttl";
/// Committed logical path of the per-axiom proof-skeleton explanations (RDF 1.2).
pub const EXPLANATIONS_PATH: &str = "generated/logic/reasoning-explanations.rdf12.ttl";
/// Committed logical path of the report-only native DL/EL crosscheck ledger.
pub const LEDGER_PATH: &str = "generated/logic/dl-el-crosscheck-report.ttl";

/// Reason over the FULL snapshot fold and return the three artifacts
/// `(closure, explanations, ledger)`. Mirrors `reason_native_artifacts` in non-merge
/// mode (the `native_reason_gen` regenerate path): the closure is told-vs-inferred
/// only (`merge=None`).
///
/// Takes the shared `import_gts_events` dataset view (#1132 C5) — the parse-once view
/// the snapshot stage carries — so this leaf no longer re-parses the `gmeow.gts`
/// bytes itself. The reasoned closure is materialized into the result here and never
/// escapes as a mutable graph, so the immutable shared `dataset` stays untouched
/// across the ENGINE_LOCK boundary.
fn reason_fold_artifacts(
    dataset: &gmeow_rdf::RdfDataset,
) -> Result<(String, String, String), PipelineError> {
    // Canonicalize the fold (RDFC-1.0) before reasoning so the content-addressed
    // Skolem witnesses are independent of carrier-assembly vs gts-round-trip blank
    // labelling (#1132). The committed goldens are reasoned over the gts-canonical
    // fold (`native_reason_gen` / the `import_gts_events` test path); reasoning over
    // the in-memory carrier directly would mint different (but isomorphic) Skolem IRIs.
    // Canonicalizing here makes the reasoned artifacts transport-independent: the carrier
    // and a re-imported `gmeow.gts` yield byte-identical bytes. Idempotent on an
    // already-canonical fold (the test path), so both paths agree.
    let canon_quads = gmeow_rdf::oxigraph::flat_oxigraph_quads_from_dataset(dataset)
        .map_err(|e| stage_err(&format!("flatten fold for canonicalization: {e}")))?;
    let canon_quads = gmeow_rdf::canonicalize_quads(canon_quads)
        .map_err(|e| stage_err(&format!("RDFC-1.0 canonicalize fold: {e}")))?;
    let canon = gmeow_rdf::dataset_from_oxigraph_quads(&canon_quads)
        .map_err(|e| stage_err(&format!("re-fold canonical quads: {e}")))?;
    let result = reason_all(canon.as_ref())
        .map_err(|e| stage_err(&format!("native reasoning failed: {e}")))?;
    // Non-merge (the regenerate path): the closure is told-vs-inferred only.
    let closure = build_inferred_closure_ttl(&result, None)
        .map_err(|e| stage_err(&format!("closure serialization failed: {e}")))?;
    let explanations = build_explanations_ttl(&result)
        .map_err(|e| stage_err(&format!("explanations serialization failed: {e}")))?;
    let ledger = build_dl_el_ledger_ttl(&result);
    Ok((closure, explanations, ledger))
}

fn stage_err(message: &str) -> PipelineError {
    PipelineError::Stage {
        stage: "stage-export-logic".to_string(),
        message: message.to_string(),
    }
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `logic` export-leaf stage: emits the three native EL/DL reasoning artifacts
/// from THIS run's full snapshot fold.
pub struct LogicStage {
    consumes: Vec<String>,
}

impl LogicStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for LogicStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for LogicStage {
    fn id(&self) -> &str {
        "stage-export-logic"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "logic.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        // THIS run's snapshot carrier dataset, read DIRECTLY off the product bundle —
        // never re-parsing the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        // Serialize the native reasoner under the pipeline ENGINE_LOCK: this leaf is
        // not a `Reason`-kind stage, so the scheduler will not take the lock for it,
        // but the global Nemo chase must never run concurrently with another
        // reasoner instance (e.g. a sibling-level leaf or the `stage-reason` stage).
        // `dataset` is an immutable `Arc<RdfDataset>`; the reasoned closure is
        // materialized inside `reason_fold_artifacts` and never crosses the lock as a
        // mutable graph, so the shared carrier is untouched.
        let (closure, explanations, ledger) = {
            let _guard = ENGINE_LOCK
                .lock()
                .map_err(|e| stage_err(&format!("ENGINE_LOCK poisoned: {e}")))?;
            reason_fold_artifacts(dataset.as_ref())?
        };
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
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Regenerate the committed logic goldens from the COMMITTED `gmeow.gts` fold.
    ///
    /// Reuses the exact authority path (`reason_fold_artifacts`) the assertion
    /// test below verifies, so the written bytes are precisely what that test
    /// expects. Reasoning over the *committed* snapshot (not a fresh rebuild)
    /// keeps the goldens CI-consistent and sidesteps local↔CI env-skew. Ignored
    /// by default — run explicitly after an ordering/serialization change:
    /// `cargo nextest run -p gmeow-pipeline regen_logic_goldens_from_committed_fold --run-ignored all`
    /// (or `cargo test -p gmeow-pipeline regen_logic_goldens_from_committed_fold -- --ignored`).
    #[test]
    #[ignore = "writes committed goldens; run explicitly to regenerate (#883)"]
    fn regen_logic_goldens_from_committed_fold() {
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).unwrap();
        let bundle = gmeow_rdf::import_gts_events(&gts).expect("import gmeow.gts");
        let (closure, explanations, ledger) =
            reason_fold_artifacts(bundle.dataset.as_ref()).expect("reason");
        std::fs::write(root.join(CLOSURE_PATH), closure).unwrap();
        std::fs::write(root.join(EXPLANATIONS_PATH), explanations).unwrap();
        std::fs::write(root.join(LEDGER_PATH), ledger).unwrap();
    }

    /// Reasoning the COMMITTED `gmeow.gts` fold through the leaf's artifact path
    /// reproduces the committed logic artifacts byte-for-byte — the exact same
    /// GTS-import → `reason_all` → `build_*_ttl` path `native_reason_gen.py`
    /// uses, so the bytes are identical (deterministic serializer over the same fold).
    #[test]
    fn logic_artifacts_reproduce_committed_over_full_fold() {
        let root = repo_root();
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).unwrap();
        let bundle = gmeow_rdf::import_gts_events(&gts).expect("import gmeow.gts");
        let (closure, explanations, ledger) =
            reason_fold_artifacts(bundle.dataset.as_ref()).expect("reason");
        for (path, produced) in [
            (CLOSURE_PATH, &closure),
            (EXPLANATIONS_PATH, &explanations),
            (LEDGER_PATH, &ledger),
        ] {
            let committed = std::fs::read_to_string(root.join(path))
                .unwrap_or_else(|_| panic!("committed missing: {path}"));
            assert_eq!(
                produced, &committed,
                "{path} drifted from committed over the full fold"
            );
        }
    }
}
