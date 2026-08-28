// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gts_compose` stage (P3): union the upstream stage products into one
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

use purrdf::RdfDataset;

use crate::node::{CachePolicy, Stage, StageInput, StageOutput, StageProduct};
use crate::stages::source_load::dataset_to_sorted_nquads;

/// Compose the upstream products into one frozen dataset by [`RdfDataset::union`]
/// over the four producing stages' `bundle.dataset` handles — the base graph
/// (`source_load`), the RDF 1.2 statement layer (`statements`), the alignment
/// axioms (`mappings`), and the reasoned CLOSURE (`reason`).
///
/// No oxigraph store, no re-parse of byte artifacts: each producing stage already
/// carries its RDF contribution as its bundle's frozen dataset (C2), so this
/// stage assembles the composed value natively by unioning those handles. The
/// union standardizes blank scopes apart per input and canonicalizes on freeze, so
/// the result is order-independent.
///
/// The base graph is REQUIRED. When `stage-statements` is an upstream (it always
/// is in the full DAG), its dataset is REQUIRED and must be non-empty — a missing
/// or empty statement-layer dataset is a HARD failure, never a silent skip that
/// would compose a statement-layer-less dataset (no-optionality). The
/// mappings and reason-closure datasets fold in when present.
///
/// The reason stage carries its CLOSURE alone as its dataset; the proof-skeleton
/// EXPLANATIONS and DL·EL crosscheck LEDGER reports ride its byte lane only and are
/// therefore EXCLUDED from this union BY CONSTRUCTION (replacing the old path-based
/// skip) — the composed dataset contains the closure but no report triples.
pub fn compose(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<RdfDataset, gmeow_errors::Diag> {
    // The base graph (required).
    let base = upstream
        .get("stage-source-load")
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-gts-compose".to_string(),
                message: "missing source_load product".to_string(),
            })
        })?
        .bundle()
        .clone();
    if base.dataset().quad_count() == 0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-gts-compose".to_string(),
            message: "source_load base-graph dataset is empty".to_string(),
        }));
    }

    // The RDF 1.2 statement layer — REQUIRED and non-empty. A declared upstream of
    // this stage; its absence is a HARD failure, never a silent skip that would
    // compose a statement-layer-less dataset (no-optionality).
    let statements = upstream
        .get("stage-statements")
        .ok_or_else(|| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-gts-compose".to_string(),
                message:
                    "missing stage-statements product (the RDF 1.2 statement layer is required)"
                        .to_string(),
            })
        })?
        .bundle()
        .clone();
    if statements.dataset().quad_count() == 0 {
        return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-gts-compose".to_string(),
            message: "stage-statements product carries an empty RDF 1.2 statement-layer dataset"
                .to_string(),
        }));
    }

    // Only the statement layer rides WHOLE (its RDF-1.2 reifier/annotation side-tables
    // are standpoint-scoped, not graph-scoped, so they must survive the fold). EVERY
    // other contributor — including the base graph — folds its DEFAULT graph only, so a
    // producer that ALSO carries named sidecar graphs contributes only its ontology
    // default graph to the composed EDB: `source_load` carries its self-description
    // named graphs (imports/metadata/verify/…), `reason` carries `graph/reasoning`, and
    // neither pollutes the composed base ∪ statements ∪ mappings ∪ closure. Taking the
    // default graph of `source_load` is byte-identical to taking its whole dataset while
    // it carries only a default graph, so this fold is unchanged for the current tree.
    let mut looped_defaults: Vec<Arc<RdfDataset>> = Vec::new();
    for (id, product) in upstream {
        if id == "stage-statements" {
            continue;
        }
        looped_defaults.push(default_graph_only(product.bundle().dataset())?);
    }

    let mut refs: Vec<&RdfDataset> = vec![statements.dataset()];
    refs.extend(looped_defaults.iter().map(|d| d.as_ref()));
    Ok(RdfDataset::union(&refs))
}

/// Project `dataset` to a fresh frozen dataset carrying ONLY its default-graph quads
/// (named-graph quads dropped), preserving the RDF-1.2 reifier/annotation side-tables
/// (which are standpoint-scoped, never graph-scoped). Used by [`compose`] to fold a
/// looped contributor's ontology contribution (its default graph) while excluding any
/// named sidecar graph it carries (the reason product's `graph/reasoning` handle
/// backing — C7).
fn default_graph_only(dataset: &RdfDataset) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut builder = purrdf::RdfDatasetBuilder::new();
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
    builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-gts-compose".to_string(),
            message: format!("default-graph projection freeze: {e}"),
        })
    })
}

/// [`compose`] projected to the deterministic sorted N-Quads byte form. The reason
/// stage uses this to seed the reasoner's input EDB (its `dataset_from_bytes`). This
/// is the SOLE consumer of the composed value's byte projection: the composed dataset
/// itself rides the stage product's `bundle.dataset` carrier (C2/C11), so there
/// is no `composed.nq` artifact byte lane — `snapshot` and every other consumer take the
/// carried dataset, not a re-parsed byte artifact.
pub fn compose_nquads(
    upstream: &BTreeMap<String, StageProduct>,
) -> Result<Vec<u8>, gmeow_errors::Diag> {
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
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn cache_policy(&self) -> CachePolicy {
        // This is a whole-dataset aggregate over already-live upstream products.
        // Hydrating its canonical cache blob must parse and reconstruct the same
        // carrier the native union creates, while also retaining a duplicate on disk.
        CachePolicy::Recompute
    }
    fn impl_version(&self) -> &str {
        "gts_compose.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Assemble the composed dataset natively by unioning the upstream stages'
        // carried datasets (C2). The stage's product carries the composed
        // DATASET as its bundle's frozen dataset — the SOLE carrier. The old
        // `pipeline/composed.nq` byte-transport artifact (C11) is retired: every
        // consumer reads the carried dataset, and `reason` re-projects the byte EDB it
        // needs through `compose_nquads` itself, so the artifact had no reader.
        let composed = compose(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            Arc::new(composed),
            BTreeMap::new(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product(stage: &str, turtle: &str) -> StageProduct {
        let dataset = purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None)
            .expect("parse synthetic dataset");
        StageProduct::from_artifacts_over(stage, dataset, BTreeMap::new())
    }

    #[test]
    fn compose_unions_synthetic_base_and_statement_layers() {
        let upstream = BTreeMap::from([
            (
                "stage-source-load".to_string(),
                product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
            ),
            (
                "stage-statements".to_string(),
                product("stage-statements", "<urn:statement> <urn:p> <urn:o> ."),
            ),
        ]);
        let composed = compose(&upstream).expect("compose synthetic layers");
        assert_eq!(composed.quad_count(), 2);
        let nquads =
            String::from_utf8(compose_nquads(&upstream).expect("project union")).expect("utf8");
        assert!(nquads.contains("<urn:base>"));
        assert!(nquads.contains("<urn:statement>"));
    }

    #[test]
    fn compose_fails_closed_on_missing_or_empty_required_layers() {
        let base_only = BTreeMap::from([(
            "stage-source-load".to_string(),
            product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
        )]);
        assert!(compose(&base_only).is_err());

        let empty_statements = BTreeMap::from([
            (
                "stage-source-load".to_string(),
                product("stage-source-load", "<urn:base> <urn:p> <urn:o> ."),
            ),
            (
                "stage-statements".to_string(),
                StageProduct::from_artifacts("stage-statements", BTreeMap::new()),
            ),
        ]);
        assert!(compose(&empty_statements).is_err());
    }
}
