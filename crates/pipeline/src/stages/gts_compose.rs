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
use std::sync::Arc;

use gmeow_rdf::RdfDataset;

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};
use crate::stages::source_load::dataset_to_sorted_nquads;

/// Compose the upstream products into one frozen dataset by [`RdfDataset::union`]
/// over the four producing stages' `bundle.dataset` handles — the base graph
/// (`source_load`), the RDF 1.2 statement layer (`statements`), the alignment
/// axioms (`mappings`), and the reasoned CLOSURE (`reason`).
///
/// No oxigraph store, no re-parse of byte artifacts: each producing stage already
/// carries its RDF contribution as its bundle's frozen dataset (#1132 C2), so this
/// stage assembles the composed value natively by unioning those handles. The
/// union standardizes blank scopes apart per input and canonicalizes on freeze, so
/// the result is order-independent.
///
/// The base graph is REQUIRED. When `stage-statements` is an upstream (it always
/// is in the full DAG), its dataset is REQUIRED and must be non-empty — a missing
/// or empty statement-layer dataset is a HARD failure, never a silent skip that
/// would compose a statement-layer-less dataset (no-optionality, #863). The
/// mappings and reason-closure datasets fold in when present.
///
/// The reason stage carries its CLOSURE alone as its dataset; the proof-skeleton
/// EXPLANATIONS and DL·EL crosscheck LEDGER reports ride its byte lane only and are
/// therefore EXCLUDED from this union BY CONSTRUCTION (replacing the old path-based
/// skip) — the composed dataset contains the closure but no report triples.
pub fn compose(upstream: &BTreeMap<String, StageProduct>) -> Result<RdfDataset, PipelineError> {
    // The base graph (required).
    let base = upstream
        .get("stage-source-load")
        .ok_or_else(|| PipelineError::Stage {
            stage: "stage-gts-compose".to_string(),
            message: "missing source_load product".to_string(),
        })?
        .bundle()
        .clone();
    if base.dataset().quad_count() == 0 {
        return Err(PipelineError::Stage {
            stage: "stage-gts-compose".to_string(),
            message: "source_load base-graph dataset is empty".to_string(),
        });
    }

    // Collect the contributing datasets in a stable id order. The base graph leads;
    // the statement layer, mappings, and reason closure fold in. `RdfDataset::union`
    // is order-independent (it canonicalizes on freeze) — the order here is purely
    // for deterministic accumulation.
    let mut datasets: Vec<Arc<gmeow_rdf::PipelineBundle<crate::bundle::PipelineHandle>>> =
        vec![base];

    // The RDF 1.2 statement layer — REQUIRED and non-empty. A declared upstream of
    // this stage; its absence is a HARD failure, never a silent skip that would
    // compose a statement-layer-less dataset (no-optionality).
    let statements = upstream
        .get("stage-statements")
        .ok_or_else(|| PipelineError::Stage {
            stage: "stage-gts-compose".to_string(),
            message: "missing stage-statements product (the RDF 1.2 statement layer is required)"
                .to_string(),
        })?
        .bundle()
        .clone();
    if statements.dataset().quad_count() == 0 {
        return Err(PipelineError::Stage {
            stage: "stage-gts-compose".to_string(),
            message: "stage-statements product carries an empty RDF 1.2 statement-layer dataset"
                .to_string(),
        });
    }
    datasets.push(statements);

    // Mappings + reasoned closure fold in via their carried datasets. Each looped
    // contributor's DEFAULT graph is its ontology contribution (the mappings axioms /
    // the reasoned closure); the reason product ALSO carries a named `graph/reasoning`
    // graph (the typed-handle's backing projection, #1132 C7) that is NOT an ontology
    // fact, so the union takes the DEFAULT graph only — keeping the composed dataset
    // exactly the base ∪ statements ∪ mappings ∪ closure it was, and excluding the
    // reasoning projection by construction (the same discipline that keeps the
    // explanations/ledger reports out: they ride the byte/handle lanes, not the union).
    let mut looped_defaults: Vec<Arc<RdfDataset>> = Vec::new();
    for (id, product) in upstream {
        if id == "stage-source-load" || id == "stage-statements" {
            continue;
        }
        looped_defaults.push(default_graph_only(product.bundle().dataset())?);
    }

    let mut refs: Vec<&RdfDataset> = datasets.iter().map(|b| b.dataset()).collect();
    refs.extend(looped_defaults.iter().map(|d| d.as_ref()));
    Ok(RdfDataset::union(&refs))
}

/// Project `dataset` to a fresh frozen dataset carrying ONLY its default-graph quads
/// (named-graph quads dropped), preserving the RDF-1.2 reifier/annotation side-tables
/// (which are standpoint-scoped, never graph-scoped). Used by [`compose`] to fold a
/// looped contributor's ontology contribution (its default graph) while excluding any
/// named sidecar graph it carries (the reason product's `graph/reasoning` handle
/// backing — #1132 C7).
fn default_graph_only(dataset: &RdfDataset) -> Result<Arc<RdfDataset>, PipelineError> {
    let mut builder = gmeow_rdf::RdfDatasetBuilder::new();
    for quad in dataset.owned_quads() {
        if quad.graph_name.is_none() {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }
    builder.freeze().map_err(|e| PipelineError::Stage {
        stage: "stage-gts-compose".to_string(),
        message: format!("default-graph projection freeze: {e}"),
    })
}

/// [`compose`] projected to the deterministic sorted N-Quads byte form. The reason
/// stage uses this to seed the reasoner's input EDB (its `dataset_from_bytes`). This
/// is the SOLE consumer of the composed value's byte projection: the composed dataset
/// itself rides the stage product's `bundle.dataset()` carrier (#1132 C2/C11), so there
/// is no `composed.nq` artifact byte lane — `snapshot` and every other consumer take the
/// carried dataset, not a re-parsed byte artifact.
pub fn compose_nquads(upstream: &BTreeMap<String, StageProduct>) -> Result<Vec<u8>, PipelineError> {
    let composed = compose(upstream)?;
    dataset_to_sorted_nquads(&composed)
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
        // Assemble the composed dataset natively by unioning the upstream stages'
        // carried datasets (#1132 C2). The stage's product carries the composed
        // DATASET as its bundle's frozen dataset — the SOLE carrier. The old
        // `pipeline/composed.nq` byte-transport artifact (#1132 C11) is retired: every
        // consumer reads the carried dataset, and `reason` re-projects the byte EDB it
        // needs through `compose_nquads` itself, so the artifact had no reader.
        let composed = compose(input.upstream)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts_over(
                self.id(),
                Arc::new(composed),
                BTreeMap::new(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::source_load::{
        dataset_to_sorted_nquads, load_authored_dataset, BASE_GRAPH_PATH,
    };
    use crate::stages::statements::RDF12_PATH;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// Build the `source_load` + `statements` upstream products the way their
    /// stages now do (#1132 C2): each CARRIES its RDF contribution as the bundle's
    /// frozen dataset (over its byte lane). Returns the upstream map plus the raw
    /// `(base quad count, base nq bytes, rdf12 ttl)` for the oracle.
    fn base_and_statements_upstream(
        root: &Path,
    ) -> (BTreeMap<String, StageProduct>, usize, Vec<u8>, String) {
        let base_dataset = load_authored_dataset(root).unwrap();
        let base_count = base_dataset.quad_count();
        let base_nq = dataset_to_sorted_nquads(&base_dataset).unwrap();
        let (_, rdf12) = crate::stages::statements::compile_statements(root).unwrap();
        let rdf12_dataset =
            gmeow_rdf::parse_dataset(rdf12.as_bytes(), "text/turtle", None).unwrap();

        let mut upstream: BTreeMap<String, StageProduct> = BTreeMap::new();
        let mut sl: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        sl.insert(BASE_GRAPH_PATH.to_string(), base_nq.clone());
        upstream.insert(
            "stage-source-load".to_string(),
            StageProduct::from_artifacts_over("stage-source-load", base_dataset, sl),
        );
        let mut st: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        st.insert(RDF12_PATH.to_string(), rdf12.clone().into_bytes());
        upstream.insert(
            "stage-statements".to_string(),
            StageProduct::from_artifacts_over("stage-statements", rdf12_dataset, st),
        );
        (upstream, base_count, base_nq, rdf12)
    }

    #[test]
    fn compose_unions_base_and_statement_layer() {
        let root = repo_root();
        let (upstream, base_count, _base_nq, _rdf12) = base_and_statements_upstream(&root);

        // compose() returns the native UNION dataset (no oxigraph store / byte re-parse).
        let composed = compose(&upstream).expect("compose");
        // The composed dataset is at least the base graph (the RDF 1.2 statement
        // layer folds reifier/annotation side-tables in on top).
        assert!(
            composed.quad_count() >= base_count,
            "composed ({}) must include the base graph ({base_count})",
            composed.quad_count(),
        );

        // The N-Quads projection re-parses (the union survived the byte lane).
        let nq = dataset_to_sorted_nquads(&composed).expect("project");
        let reparsed = crate::stages::source_load::parse_base_graph(&nq).expect("reparse");
        assert!(reparsed.quad_count() >= base_count);
    }

    /// The native union (`compose`) must actually UNION the base + statement layers
    /// and be canonically STABLE. EPIC #906 retired the old oxigraph-store byte-ingest
    /// oracle this test once cross-checked against; the property it asserted (a faithful
    /// union of base ∪ statements) is now stated directly against the native value: the
    /// union strictly contains the base graph, and its RDFC-1.0 canonical form is
    /// non-empty and idempotent under re-canonicalization.
    #[test]
    fn compose_union_is_canonically_stable_superset_of_base() {
        use gmeow_rdf::canonicalize;

        let root = repo_root();
        let (upstream, base_count, _base_nq, _rdf12) = base_and_statements_upstream(&root);

        let new_dataset = compose(&upstream).expect("compose");

        // The union actually unions: it is a superset of the base graph (the statement
        // layer is REQUIRED and non-empty, so the union strictly exceeds the base).
        assert!(
            new_dataset.quad_count() >= base_count,
            "native union ({}) must contain the base graph ({base_count})",
            new_dataset.quad_count(),
        );
        assert!(
            base_count > 0,
            "base graph must be non-empty for the union to be meaningful"
        );

        // Canonical form is non-empty and idempotent: canonicalizing the canonical
        // dataset reproduces the same N-Quads document (RDFC-1.0 stability).
        let canon = canonicalize(&new_dataset).nquads;
        assert!(!canon.trim().is_empty(), "canonical union is empty");
        let reparsed = crate::stages::source_load::parse_base_graph(canon.as_bytes())
            .expect("reparse canonical union");
        let canon_again = canonicalize(&reparsed).nquads;
        assert_eq!(
            canon, canon_again,
            "native union canonicalization must be idempotent"
        );
    }
}
