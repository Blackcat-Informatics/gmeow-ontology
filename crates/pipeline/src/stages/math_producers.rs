// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math_producers` stage: fold the eight `math:` producers' output into the carrier
//! (Design A).
//!
//! Each flagship-acceptance scenario in
//! `slices/grounding/math/examples/flagship-acceptance.ttl` names a native producer entrypoint
//! (`gmeow:demonstratedByProducer`). Those entrypoints are five deterministic,
//! exact-arithmetic functions in [`gmeow_math::producers`]; each returns a byte-deterministic
//! RDF graph fragment (Turtle) built from constants + formatted exact integers/rationals. A
//! SIXTH producer, [`gmeow_math::producers::probability_model_seam`], folds the same way but
//! is NOT bound to a flagship scenario — it exists solely to carry the probability layer's
//! live `logic:probabilityModel` A-box crossing triple inside `gmeow.gts`. A SEVENTH producer,
//! [`gmeow_math::producers::pvalue_tri_slice`], likewise non-flagship, carries the signature
//! `lang:` → `logic:` → `math:` p-value round-trip inside `gmeow.gts`.
//! An EIGHTH producer, [`gmeow_math::producers::clifford_twelve_thirteen`], calculates both
//! exact `Cl(12)` → `Cl(13)` positive extensions without changing the five-flagship contract.
//!
//! This stage RUNS all eight and parses each producer's `.turtle` into its own named carrier
//! graph ([`crate::stages::carrier::MATH_PRODUCER_GRAPHS`], in producer order). The snapshot
//! presenter reads those graphs back via `producer_graph` and folds them into `gmeow.gts`, so
//! the producer output ships in the bundle — the shippable deliverable, maximal dogfooding —
//! rather than living only behind a test-side equality gate.
//!
//! The graph content comes ONLY from the producers: no hand-typed constant, no disk read, no
//! clock, no randomness (the producers are pure), so the attached dataset is byte-deterministic.
//! A producer/parse failure is a HARD FAIL — propagated, never swallowed (no-optionality).

use std::collections::BTreeMap;
use std::path::Path;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{MATH_PRODUCER_GRAPHS, parse_into_graph};

/// Run the eight producers in the pinned [`MATH_PRODUCER_GRAPHS`] order and pair each with its
/// target graph IRI. The order is the SINGLE source of the producer→graph mapping shared with
/// the snapshot presenter (both index into `MATH_PRODUCER_GRAPHS`).
fn producer_turtles() -> [(&'static str, String); 8] {
    [
        (
            MATH_PRODUCER_GRAPHS[0],
            gmeow_math::producers::e8_weyl_order().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[1],
            gmeow_math::producers::additive_he_demo().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[2],
            gmeow_math::producers::proof_ingest().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[3],
            gmeow_math::producers::r_bridge_lift().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[4],
            gmeow_math::producers::exact_pca_residual().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[5],
            gmeow_math::producers::probability_model_seam().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[6],
            gmeow_math::producers::pvalue_tri_slice().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[7],
            gmeow_math::producers::clifford_twelve_thirteen().turtle,
        ),
    ]
}

/// The `math_producers` pipeline stage — a leaf compute node. It consumes no upstream product
/// (the producers are self-contained native functions) and attaches the eight producer graphs to
/// its carrier dataset.
pub struct MathProducersStage {
    consumes: Vec<String>,
}

impl MathProducersStage {
    /// Construct the stage. It reads nothing upstream — the producers compute from pinned
    /// in-code constants.
    pub fn new() -> Self {
        Self {
            consumes: Vec::new(),
        }
    }
}

impl Default for MathProducersStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for MathProducersStage {
    fn id(&self) -> &str {
        "stage-math-producers"
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    /// The named graphs this stage attaches to the carrier (its delta), from the
    /// single Rust-side attach table; mirrored by the slice module.ttl gmeow:attachesGraph
    /// declarations and verified against the run-time delta by the scheduler.
    fn attaches_graphs(&self) -> &[String] {
        crate::stages::attach::graphs(self.id())
    }
    /// The blob-representation lanes this stage attaches (its delta), from the single
    /// Rust-side attach table; mirrored by gmeow:attachesBlobRep and run-time-verified.
    fn attaches_blob_reps(&self) -> &[String] {
        crate::stages::attach::blob_reps(self.id())
    }
    fn impl_version(&self) -> &str {
        // v1: fold the five math flagship producers' graphs into the carrier.
        // v2: additionally fold the probability-model seam producer's graph (a sixth,
        // non-flagship producer carrying the live logic:probabilityModel crossing triple).
        // v3: additionally fold the p-value tri-slice producer's graph (a seventh,
        // non-flagship producer carrying the signature lang: -> logic: -> math: round-trip).
        // v4: additionally fold the exact Cl(12) -> Cl(13) producer's graph (an eighth,
        // non-flagship producer carrying both positive-extension calculations).
        "math_producers.v4"
    }
    fn input_files(&self, _root: &Path) -> Result<Vec<std::path::PathBuf>, gmeow_errors::Diag> {
        // No source files: the producers are self-contained native functions whose bytes ride
        // the workspace-source BUILD_FINGERPRINT (any code change to `crates/math` yields fresh
        // cache keys), so there is nothing to declare here.
        Ok(Vec::new())
    }
    fn run(&self, _input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Parse each producer's deterministic Turtle into its own named carrier graph and union
        // them into one frozen dataset the snapshot presenter folds into the bundle. The content
        // is the producers' output ALONE — a parse failure hard-fails (propagated).
        let turtles = producer_turtles();
        let mut graphs: Vec<std::sync::Arc<purrdf::RdfDataset>> = Vec::with_capacity(turtles.len());
        for (graph_iri, turtle) in &turtles {
            graphs.push(parse_into_graph(
                turtle.as_bytes(),
                "text/turtle",
                graph_iri,
            )?);
        }
        let refs: Vec<&purrdf::RdfDataset> = graphs.iter().map(|g| g.as_ref()).collect();
        let dataset = std::sync::Arc::new(purrdf::RdfDataset::union(&refs));
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            BTreeMap::new(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stage attaches EXACTLY the eight producer graphs, each non-empty and carrying its
    /// producer's pinned content — the proof the producer output reaches the carrier (and thence
    /// `gmeow.gts`), not merely a test.
    #[test]
    fn run_attaches_the_eight_producer_graphs() {
        let stage = MathProducersStage::new();
        let upstream = BTreeMap::new();
        let out = stage
            .run(StageInput {
                root: Path::new("."),
                upstream: &upstream,
            })
            .expect("math_producers stage runs");
        let dataset = out.product.dataset();
        for graph_iri in MATH_PRODUCER_GRAPHS {
            let projected = dataset.project_named_graph(graph_iri);
            assert!(
                projected.quad_count() > 0,
                "producer graph <{graph_iri}> must carry the producer's triples"
            );
        }
    }

    /// Determinism: two runs attach byte-identical carrier datasets (the producers are pure —
    /// no clock, no RNG, no HashMap iteration order).
    #[test]
    fn run_is_deterministic() {
        let stage = MathProducersStage::new();
        let upstream = BTreeMap::new();
        let a = stage
            .run(StageInput {
                root: Path::new("."),
                upstream: &upstream,
            })
            .expect("run a");
        let b = stage
            .run(StageInput {
                root: Path::new("."),
                upstream: &upstream,
            })
            .expect("run b");
        assert_eq!(
            purrdf::canonical_flat_nquads(a.product.dataset()).expect("canon a"),
            purrdf::canonical_flat_nquads(b.product.dataset()).expect("canon b"),
            "the math-producers carrier dataset must be deterministic"
        );
    }
}
