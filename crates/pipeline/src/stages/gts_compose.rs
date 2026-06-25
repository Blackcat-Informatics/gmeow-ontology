// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_compose` stage (#861 P3): union the upstream stage products into one
//! in-memory composed dataset.
//!
//! Every downstream export leaf and the docs renderer query THIS dataset rather
//! than re-parsing `gmeow.gts` from disk. It is the union of the authored base
//! graph (`source_load`), the RDF 1.2 statement layer (`statements`), the
//! alignment artifacts (`mappings`), and the reasoned closure (`reason`) as those
//! stages land. The single serialization to `gmeow.gts` happens later in the
//! sole `gts_sink` (the narrow waist); this stage only assembles the value.

use std::collections::BTreeMap;

use oxigraph::store::Store;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::source_load::{rdf_bytes_into_store, store_to_nquads, BASE_GRAPH_PATH};
use crate::stages::statements::RDF12_PATH;

/// Logical path of the composed dataset (N-Quads, in-memory dataflow).
pub const COMPOSED_PATH: &str = "pipeline/composed.nq";

/// Parse an artifact (Turtle or N-Quads, by extension heuristic) into `store`,
/// tolerating RDF 1.2 triple terms (the statement layer).
fn ingest(store: &Store, logical_path: &str, bytes: &[u8]) -> Result<(), PipelineError> {
    let media_type = if logical_path.ends_with(".nq") {
        "application/n-quads"
    } else if logical_path.ends_with(".nt") {
        "application/n-triples"
    } else {
        "text/turtle"
    };
    rdf_bytes_into_store(
        store,
        bytes,
        media_type,
        &format!("composing {logical_path}"),
    )
}

/// Compose the upstream products into one store: the base graph plus the RDF 1.2
/// statement layer (and, as they land, mappings + reasoned closure). Returns the
/// composed N-Quads bytes.
pub fn compose(upstream: &BTreeMap<String, StageProduct>) -> Result<Vec<u8>, PipelineError> {
    let store =
        Store::new().map_err(|e| PipelineError::Parse(format!("store creation failed: {e}")))?;

    // The base graph (required).
    let base = upstream
        .get("stage-source-load")
        .and_then(|p| p.artifact(BASE_GRAPH_PATH))
        .ok_or_else(|| PipelineError::Stage {
            stage: "stage-gts-compose".to_string(),
            message: "missing source_load base graph".to_string(),
        })?;
    ingest(&store, BASE_GRAPH_PATH, base)?;

    // The RDF 1.2 statement layer. When `stage-statements` is an upstream (it
    // always is in the full DAG), its artifact is REQUIRED — a missing RDF12
    // artifact is a HARD failure, never a silent skip that would compose a
    // statement-layer-less dataset (no-optionality, #863).
    if let Some(statements) = upstream.get("stage-statements") {
        let rdf12 = statements
            .artifact(RDF12_PATH)
            .ok_or_else(|| PipelineError::Stage {
                stage: "stage-gts-compose".to_string(),
                message: format!("stage-statements product is missing its {RDF12_PATH} artifact"),
            })?;
        ingest(&store, RDF12_PATH, rdf12)?;
    }

    // Every other upstream RDF artifact (mappings, reason) folds in by the same
    // channel as those stages land — union all *.ttl / *.nq / *.nt artifacts. The
    // `stage-reason` product carries THREE artifacts (the inferred CLOSURE plus the
    // proof-skeleton EXPLANATIONS and the DL/EL crosscheck LEDGER report); only the
    // closure is dataset facts. Folding the explanation/ledger REPORT TTLs into the
    // composed dataset would pollute it with provenance-reifier / report triples
    // that are not part of the ontology — so reason contributes ONLY its closure
    // artifact (#863).
    for (id, product) in upstream {
        if id == "stage-source-load" || id == "stage-statements" {
            continue;
        }
        if id == "stage-reason" {
            let closure = product
                .artifact(crate::stages::reason::CLOSURE_PATH)
                .ok_or_else(|| PipelineError::Stage {
                    stage: "stage-gts-compose".to_string(),
                    message: format!(
                        "stage-reason product is missing its closure artifact {}",
                        crate::stages::reason::CLOSURE_PATH
                    ),
                })?;
            ingest(&store, crate::stages::reason::CLOSURE_PATH, closure)?;
            continue;
        }
        for (path, bytes) in &product.artifacts {
            if path.ends_with(".ttl") || path.ends_with(".nq") || path.ends_with(".nt") {
                ingest(&store, path, bytes)?;
            }
        }
    }

    store_to_nquads(&store)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `gts_compose` pipeline stage.
pub struct GtsComposeStage {
    consumes: Vec<String>,
}

impl GtsComposeStage {
    /// Construct the stage with its upstream dependency ids (from the DAG).
    pub fn new() -> Self {
        // The composed dataset = base graph ∪ statement layer ∪ mappings ∪
        // reasoned closure. The slice DAG's stage-gts-compose dataflowConsumes is
        // reconciled to this exact set when the full pipeline is wired (P6).
        Self {
            consumes: vec![
                "stage-mappings".to_string(),
                "stage-reason".to_string(),
                "stage-source-load".to_string(),
                "stage-statements".to_string(),
            ],
        }
    }
}

impl Default for GtsComposeStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for GtsComposeStage {
    fn id(&self) -> &str {
        "stage-gts-compose"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "gts_compose.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let composed = compose(input.upstream)?;
        let mut artifacts: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        artifacts.insert(COMPOSED_PATH.to_string(), composed);
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), artifacts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::source_load::load_authored_store;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn compose_unions_base_and_statement_layer() {
        let root = repo_root();
        // Build the two upstream products the way their stages would.
        let base_store = load_authored_store(&root).unwrap();
        let base_nq = store_to_nquads(&base_store).unwrap();
        let (_, rdf12) = crate::stages::statements::compile_statements(&root).unwrap();

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut sl: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        sl.insert(BASE_GRAPH_PATH.to_string(), base_nq.clone());
        upstream.insert(
            "stage-source-load".to_string(),
            StageProduct::from_artifacts("stage-source-load", sl),
        );
        let mut st: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        st.insert(RDF12_PATH.to_string(), rdf12.into_bytes());
        upstream.insert(
            "stage-statements".to_string(),
            StageProduct::from_artifacts("stage-statements", st),
        );

        let composed = compose(&upstream).expect("compose");
        // The composed dataset is at least the base graph (RDF 1.2 triple terms
        // from the statement layer fold in on top).
        let composed_lines = composed.iter().filter(|&&b| b == b'\n').count();
        let base_lines = base_nq.iter().filter(|&&b| b == b'\n').count();
        assert!(
            composed_lines >= base_lines,
            "composed ({composed_lines}) must include the base graph ({base_lines})"
        );
        // And it re-parses (the RDF 1.2 statement terms survived the union).
        let reparsed = crate::stages::source_load::parse_base_graph(&composed).expect("reparse");
        assert!(reparsed.len().unwrap() >= base_store.len().unwrap());
    }
}
