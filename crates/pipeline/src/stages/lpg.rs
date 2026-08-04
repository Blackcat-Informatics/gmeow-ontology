// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `lpg` export leaf (P4): RDF → Labeled Property Graph.
//!
//! Renders the generic CSV, Neo4j Admin Import CSV, openCypher, and GraphML 1.0
//! packages via purrdf's native LPG projections
//! (`purrdf::project_lpg_csv` / `project_neo4j_csv` / `project_lpg_cypher` /
//! `project_lpg_graphml`), retiring gmeow's hand-rolled node/edge model and
//! CSV/Cypher/GraphML writers (the former genuine port of
//! `src/gmeow_tools/lpg.py`). purrdf owns the encoding; gmeow owns only the
//! scoping decision below and the merge into `generated/lpg/`.
//!
//! # Scope: the `statements` named graph ONLY
//!
//! purrdf's `project_lpg` is a generic `DatasetView → LPG` mapper with no notion
//! of "business layer" vs. "ontology machinery" — it walks every quad the view
//! exposes. The full carrier snapshot (`crate::stages::carrier::snapshot_dataset`)
//! is the WHOLE composed `gmeow.gts` dataset: ~2.3M quads across 111 named
//! graphs (ontology axioms, reasoning closure, EDOAL correspondence tables,
//! diagnostics, provenance, math producers, …). Projecting that unscoped
//! balloons `generated/lpg/` from ~276 KB to multiple GIGABYTES and the stage
//! from milliseconds to many minutes — measured directly: the generic CSV
//! package alone was 1.73 GB (901 MB `nodes.csv` + 830 MB `edges.csv`) for
//! 262k nodes / 1.2M edges, taking ~76s for that ONE of four packages.
//!
//! The retired hand-rolled `build_lpg` scoped its scan to exactly one named
//! graph — [`STATEMENTS_GRAPH`] (`.../graph/statements`), the typed-resource
//! "business" graph — plus the RDF-1.2 reifiers/annotations attached to that
//! graph's triples (statement metadata folded in as edge/node properties).
//! [`render_from_dataset`] preserves that EXACT scope by first projecting the
//! carrier down to `STATEMENTS_GRAPH` via
//! [`purrdf::RdfDataset::project_named_graph_full`] — purrdf's own established
//! "graph-scoped fold with full RDF-star sidecar" primitive (used elsewhere in
//! this pipeline, e.g. the superset gate's per-graph fold) — and THEN hands
//! that small scoped dataset to purrdf's four LPG projections. Do not "fix"
//! this to project the whole carrier: that is the multi-GB regression above,
//! not a bug in the scoping.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use purrdf::{LpgConfig, LpgExecutionLimits, LpgScope, ProjectionLimits, RdfDataset};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};

/// Logical-path prefix of the generated LPG artifacts.
pub const LPG_DIR: &str = "generated/lpg";

/// The RDF predicate whose IRI-object statements become native LPG labels.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// The ONE named graph projected — see the module doc for why (never the whole
/// carrier).
const STATEMENTS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/statements";

fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-export-lpg".into(),
        message: message.into(),
    })
}

/// The gmeow-owned `LpgConfig`: the `rdf_type` label predicate plus GENEROUS
/// (tens-to-low-hundreds-of-MB) resource bounds. The scoped `STATEMENTS_GRAPH`
/// dataset is small (hundreds of nodes/edges, low-single-digit MB of packaged
/// bytes per format — measured), so these bounds are comfortable headroom for
/// corpus growth, never a real cap: under no-optionality a too-small limit is a
/// HARD FAIL, so this errs generous rather than tight.
fn lpg_config() -> Result<LpgConfig, gmeow_errors::Diag> {
    let limits = ProjectionLimits::new(
        8_192,       // max_artifacts: real need ~40 (one file per neo4j label/edge-type group)
        256_000_000, // max_artifact_bytes: 256 MB per artifact
        512_000_000, // max_total_bytes: 512 MB per package
        600_000_000, // max_archive_bytes (>= max_total_bytes; archives are never written here)
        16,          // max_term_depth: the hard safety ceiling
    )
    .map_err(|e| err(format!("build ProjectionLimits: {e}")))?;
    // The dataset handed to purrdf is ALREADY scoped to exactly `STATEMENTS_GRAPH`
    // (see `render_from_dataset`), so the config projects the complete passed view.
    // Generous uniform execution ceilings: the scoped statements graph is hundreds
    // of records/nodes/edges, so ~1000x headroom — never a real cap (no-optionality:
    // a too-small limit is a HARD FAIL, so this errs generous).
    let execution_limits = LpgExecutionLimits::new(
        100_000, // max_input_records
        100_000, // max_model_records
        100_000, // max_nodes
        100_000, // max_edges
    )
    .map_err(|e| err(format!("build LpgExecutionLimits: {e}")))?;
    LpgConfig::new(RDF_TYPE, LpgScope::all(), limits, execution_limits)
        .map_err(|e| err(format!("build LpgConfig: {e}")))
}

/// Aggregate every purrdf LPG package's [`purrdf::LossLedger`] into ONE tracing
/// report (target `lpg_loss`) so no runtime RDF→LPG lowering loss is ever
/// silently dropped. The four `project_lpg_*` calls each independently
/// recompute `purrdf::project_lpg` over the SAME scoped dataset, so their
/// ledgers largely duplicate one another; grouping by `(code, note)` across all
/// four collapses that duplication into one entry per distinct loss reason.
fn report_lpg_losses(ledgers: &[&purrdf::LossLedger]) {
    let mut grouped: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
    for ledger in ledgers {
        for loss in ledger.entries() {
            let subject = loss
                .location
                .as_deref()
                .and_then(|location| location.subject.as_deref())
                .unwrap_or("<unlocated>");
            grouped
                .entry((loss.code.as_ref(), loss.note.as_ref()))
                .or_default()
                .push(subject);
        }
    }
    for ((construct, reason), mut subjects) in grouped {
        subjects.sort_unstable();
        subjects.dedup();
        let examples = subjects
            .iter()
            .take(5)
            .copied()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if subjects.len() > 5 {
            format!(" (+{} more)", subjects.len() - 5)
        } else {
            String::new()
        };
        tracing::info!(
            target: "lpg_loss",
            construct = construct,
            subjects = subjects.len(),
            reason = reason,
            examples = %format!("{examples}{suffix}"),
            "lossy drop projecting the scoped statements-graph RDF to the LPG surface",
        );
    }
}

/// Merge one purrdf `ProjectionPackage`'s members into `merged`, each key
/// prefixed with `LPG_DIR/`. A colliding relative path across the four
/// projections is a HARD FAIL (never a silent overwrite) — purrdf namespaces
/// neo4j/open-cypher/graphml into their own subdirs and the generic CSV
/// projection at the package root, so no collision is expected in practice;
/// this is the safety net, not the design.
fn merge_package(
    merged: &mut BTreeMap<String, Vec<u8>>,
    label: &str,
    package: &purrdf::ProjectionPackage,
) -> Result<(), gmeow_errors::Diag> {
    for (path, bytes) in package.artifacts() {
        let key = format!("{LPG_DIR}/{path}");
        match merged.entry(key) {
            Entry::Vacant(slot) => {
                slot.insert(bytes.to_vec());
            }
            Entry::Occupied(slot) => {
                return Err(err(format!(
                    "lpg artifact key collision at `{}`: emitted by more than one purrdf LPG \
                     projection (most recently `{label}`)",
                    slot.key()
                )));
            }
        }
    }
    Ok(())
}

/// Project the LPG (generic CSV + Neo4j Admin Import CSV + openCypher +
/// GraphML 1.0) from the carrier `dataset`, scoped to [`STATEMENTS_GRAPH`] (see
/// the module doc). The snapshot gatherer calls this to attach the opaque LPG
/// fanout as a blob (superset law); the export leaf calls it for the disk
/// fanout.
pub(crate) fn render_from_dataset(
    dataset: &RdfDataset,
) -> Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag> {
    // Scope to the business-layer statements graph ONLY — see the module doc
    // for why the whole carrier is never projected here.
    let scoped = dataset.project_named_graph_full(STATEMENTS_GRAPH);
    let config = lpg_config()?;

    let csv = purrdf::project_lpg_csv(&scoped, &config)
        .map_err(|e| err(format!("project_lpg_csv: {e}")))?;
    let neo4j = purrdf::project_neo4j_csv(&scoped, &config)
        .map_err(|e| err(format!("project_neo4j_csv: {e}")))?;
    let cypher = purrdf::project_lpg_cypher(&scoped, &config)
        .map_err(|e| err(format!("project_lpg_cypher: {e}")))?;
    let graphml = purrdf::project_lpg_graphml(&scoped, &config)
        .map_err(|e| err(format!("project_lpg_graphml: {e}")))?;

    report_lpg_losses(&[
        &csv.loss_ledger,
        &neo4j.loss_ledger,
        &cypher.loss_ledger,
        &graphml.loss_ledger,
    ]);

    let mut merged: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    merge_package(&mut merged, "csv", &csv.package)?;
    merge_package(&mut merged, "neo4j", &neo4j.package)?;
    merge_package(&mut merged, "cypher", &cypher.package)?;
    merge_package(&mut merged, "graphml", &graphml.package)?;
    Ok(merged)
}

// ── Stage impl ───────────────────────────────────────────────────────────────

/// The `lpg` export-leaf stage.
pub struct LpgStage {
    consumes: Vec<String>,
}

impl LpgStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for LpgStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for LpgStage {
    fn id(&self) -> &str {
        "stage-export-lpg"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "lpg.v1-purrdf"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Consume THIS run's snapshot carrier dataset DIRECTLY off the product bundle —
        // no re-parse of the gmeow.gts bytes (GTS is exit-only).
        let dataset = crate::stages::carrier::snapshot_dataset(input.upstream)?;
        Ok(StageOutput::new(StageProduct::from_artifacts(
            self.id(),
            render_from_dataset(dataset.as_ref())?,
        )))
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

    /// Load the committed `gmeow.gts` carrier dataset (the same bytes
    /// `stage-export-lpg` consumes off THIS run's snapshot in production).
    fn committed_dataset(root: &Path) -> std::sync::Arc<RdfDataset> {
        let gts = std::fs::read(root.join("generated/dist/gmeow.gts")).unwrap();
        purrdf::import_gts_events(&gts).unwrap().dataset
    }

    #[test]
    fn render_from_dataset_emits_expected_package_layout() {
        let root = repo_root();
        let dataset = committed_dataset(&root);
        let arts = render_from_dataset(dataset.as_ref()).expect("render_from_dataset");

        assert!(!arts.is_empty(), "expected a non-empty LPG artifact map");
        assert!(arts.contains_key(&format!("{LPG_DIR}/nodes.csv")));
        assert!(arts.contains_key(&format!("{LPG_DIR}/edges.csv")));
        assert!(
            arts.keys()
                .any(|k| k.starts_with(&format!("{LPG_DIR}/open-cypher/"))),
            "expected an open-cypher/ member"
        );
        assert!(
            arts.keys()
                .any(|k| k.starts_with(&format!("{LPG_DIR}/graphml/"))),
            "expected a graphml/ member"
        );
        assert!(
            arts.keys()
                .any(|k| k.starts_with(&format!("{LPG_DIR}/neo4j/"))),
            "expected a neo4j/ member"
        );
        for key in arts.keys() {
            assert!(
                key.starts_with(&format!("{LPG_DIR}/")),
                "every artifact key must live under {LPG_DIR}/, got {key}"
            );
        }
    }

    #[test]
    fn render_from_dataset_is_byte_deterministic() {
        let root = repo_root();
        let dataset = committed_dataset(&root);
        let first = render_from_dataset(dataset.as_ref()).expect("first render");
        let second = render_from_dataset(dataset.as_ref()).expect("second render");
        assert_eq!(
            first, second,
            "render_from_dataset must be byte-deterministic"
        );
    }

    /// ROUND-TRIP: the generic-CSV projection's `LpgGraph` lifts back
    /// (`purrdf::lift_lpg`) to a dataset that is exactly isomorphic to the
    /// SCOPED `STATEMENTS_GRAPH` dataset fed into the projection — every LPG
    /// label/property/edge carries its exact RDF-1.2 sideband quad, so nothing
    /// is lost lifting back. Compared via `purrdf::turtle_normalize::render`
    /// (the same canonical-Turtle round-trip idiom the superset gate uses for
    /// full-fidelity RDF-star folds), never a tautological self-comparison.
    #[test]
    fn lpg_lift_round_trips_the_scoped_statements_graph() {
        let root = repo_root();
        let dataset = committed_dataset(&root);
        let scoped = dataset.project_named_graph_full(STATEMENTS_GRAPH);
        let config = lpg_config().expect("lpg_config");

        let csv = purrdf::project_lpg_csv(&scoped, &config).expect("project_lpg_csv");
        assert!(!csv.graph.nodes.is_empty(), "expected a non-empty node set");
        assert!(!csv.graph.edges.is_empty(), "expected a non-empty edge set");

        let outcome = purrdf::lift_lpg(&csv.graph, &config).expect("lift_lpg");

        let prefixes = crate::stages::superset::rdf_prefixes();
        let expected = purrdf::turtle_normalize::render(&scoped, &prefixes);
        let actual = purrdf::turtle_normalize::render(&outcome.dataset, &prefixes);
        assert_eq!(
            actual, expected,
            "lift_lpg must reproduce the exact scoped statements-graph dataset"
        );
    }
}
