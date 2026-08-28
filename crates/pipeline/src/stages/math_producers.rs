// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math_producers` stage: fold the ten `math:` producers' output into the carrier
//! (Design A).
//!
//! Each flagship-acceptance scenario in
//! `slices/grounding/math/examples/flagship-acceptance.ttl` names a native producer entrypoint
//! (`gmeow:demonstratedByProducer`). Those entrypoints are five deterministic functions in
//! [`gmeow_math::producers`]; four build a byte-deterministic RDF graph fragment (Turtle) from
//! constants + formatted exact integers/rationals, and the fifth — the `rBridge` flagship's
//! [`gmeow_math::producers::r_lift`] — RUNS the shipped R front-end over a real committed
//! script. [`gmeow_math::producers::probability_model_seam`] folds the same way but
//! is NOT bound to a flagship scenario — it exists solely to carry the probability layer's
//! live `logic:probabilityModel` A-box crossing triple inside `gmeow.gts`.
//! [`gmeow_math::producers::pvalue_tri_slice`], likewise non-flagship, carries the signature
//! `lang:` → `logic:` → `math:` p-value round-trip inside `gmeow.gts`, and
//! [`gmeow_math::producers::clifford_twelve_thirteen`] calculates both
//! exact `Cl(12)` → `Cl(13)` positive extensions without changing the five-flagship contract.
//! [`gmeow_math::producers::r_lift`], [`onnx_lift`](gmeow_math::producers::onnx_lift), and
//! [`proof_lift`](gmeow_math::producers::proof_lift) are the EXECUTABLE ingestion lifts:
//! each runs the shipped `gmeow_math_lift` front-end (the same entrypoint the `gmeow` CLI
//! calls) over a real committed artifact embedded in the binary at compile time, so the
//! bundle carries the output of the actual R / ONNX / TSTP parsers rather than a hand-written
//! imitation of them. Only `r_lift` is flagship-bound; the manifest stays at exactly five.
//!
//! This stage RUNS all ten and parses each producer's `.turtle` into its own named carrier
//! graph ([`crate::stages::carrier::MATH_PRODUCER_GRAPHS`], in producer order). The snapshot
//! presenter reads those graphs back via `producer_graph` and folds them into `gmeow.gts`, so
//! the producer output ships in the bundle — the shippable deliverable, maximal dogfooding —
//! rather than living only behind a test-side equality gate.
//!
//! The emitted graph content comes ONLY from the producers: no hand-typed fallback, clock, or
//! randomness (the producers are pure), so those graphs are byte-deterministic. The stage reads
//! seven committed producer fixtures only as parity witnesses and hard-fails unless each is
//! graph-isomorphic to its named producer output; fixture bytes never enter the emitted graph.
//! The three lift sources themselves remain `include_str!` / `include_bytes!` compile-time
//! embeddings — those bytes ride the binary, never the machine that ran the build, and every IRI
//! they mint is a content digest of those bytes.
//! A producer/parse failure is a HARD FAIL — propagated, never swallowed (no-optionality).
//!
//! Every attached graph here is COMPUTED: this stage reads no corpus off disk, so its inputs
//! are the workspace sources its producers compile from and nothing else. The authored
//! `examples/*.ttl` ABox of every slice — the math slice's included — is loaded by
//! `stage-source-load` into [`gmeow_logic::reasoning_graphs::GRAPH_EXAMPLES`], where an
//! authored corpus belongs: with the loader that reads the slices, for every slice at once,
//! rather than as one grounding slice's special case bolted onto a producer stage.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::RdfDataset;

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{MATH_PRODUCER_GRAPHS, parse_into_graph};

/// The committed producer fixtures paired with the exact producer result index they
/// witness. This parity is enforced by the production stage itself; no test is
/// permitted to call a producer to regenerate a comparison graph.
const PRODUCER_PARITY_FIXTURES: [(usize, &str, &str); 7] = [
    (
        0,
        "E8 Weyl",
        "slices/grounding/math/tests/conformance-fixtures/e8-weyl-produced.ttl",
    ),
    (
        1,
        "additive homomorphic encryption",
        "slices/grounding/math/tests/conformance-fixtures/he-scheme-produced.ttl",
    ),
    (
        2,
        "proof ingest",
        "slices/grounding/math/tests/conformance-fixtures/verification-result-produced.ttl",
    ),
    (
        7,
        "R lift",
        "slices/grounding/math/tests/fixtures/lifted-r.ttl",
    ),
    (
        8,
        "ONNX lift",
        "slices/grounding/math/tests/fixtures/lifted-onnx.ttl",
    ),
    (
        9,
        "proof lift",
        "slices/grounding/math/tests/fixtures/lifted-proof.ttl",
    ),
    (
        3,
        "exact PCA residual",
        "slices/grounding/math/tests/conformance-fixtures/pca-residual-lifted.ttl",
    ),
];

/// Run the ten producers in the pinned [`MATH_PRODUCER_GRAPHS`] order and pair each with its
/// target graph IRI. The order is the SINGLE source of the producer→graph mapping shared with
/// the snapshot presenter (both index into `MATH_PRODUCER_GRAPHS`).
fn producer_turtles() -> [(&'static str, String); 10] {
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
            gmeow_math::producers::exact_pca_residual().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[4],
            gmeow_math::producers::probability_model_seam().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[5],
            gmeow_math::producers::pvalue_tri_slice().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[6],
            gmeow_math::producers::clifford_twelve_thirteen().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[7],
            gmeow_math::producers::r_lift().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[8],
            gmeow_math::producers::onnx_lift().turtle,
        ),
        (
            MATH_PRODUCER_GRAPHS[9],
            gmeow_math::producers::proof_lift().turtle,
        ),
    ]
}

fn canonical_turtle(bytes: &[u8], label: &str) -> Result<String, gmeow_errors::Diag> {
    let parsed = purrdf::parse_dataset(bytes, "text/turtle", None).map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-math-producers".to_owned(),
            message: format!("cannot parse {label} Turtle for producer parity: {error}"),
        })
    })?;
    let quads = purrdf::flat_rdf_quads_from_dataset(&parsed);
    let flat = purrdf::flat_dataset_from_quads(&quads).map_err(|error| {
        gmeow_errors::Diag::of_kind(crate::error::StageFailed {
            stage: "stage-math-producers".to_owned(),
            message: format!("cannot freeze {label} graph for producer parity: {error}"),
        })
    })?;
    Ok(purrdf::canonicalize(&flat).nquads)
}

fn enforce_producer_fixture_parity(
    root: &Path,
    produced: &[(&str, String)],
) -> Result<(), gmeow_errors::Diag> {
    for (producer_index, label, relative_path) in PRODUCER_PARITY_FIXTURES {
        let fixture_path = root.join(relative_path);
        let fixture = std::fs::read(&fixture_path).map_err(|error| {
            gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-math-producers".to_owned(),
                message: format!(
                    "cannot read {label} producer fixture {}: {error}",
                    fixture_path.display()
                ),
            })
        })?;
        let fixture_graph = canonical_turtle(&fixture, relative_path)?;
        let producer_graph = canonical_turtle(produced[producer_index].1.as_bytes(), label)?;
        if fixture_graph != producer_graph {
            return Err(gmeow_errors::Diag::of_kind(crate::error::StageFailed {
                stage: "stage-math-producers".to_owned(),
                message: format!(
                    "{label} producer output is not graph-isomorphic to producer fixture {relative_path}"
                ),
            }));
        }
    }
    Ok(())
}

/// The `math_producers` pipeline stage — a leaf compute node. It consumes no upstream STAGE
/// product (the producers are self-contained native functions) and attaches the ten producer
/// graphs to its carrier dataset.
pub struct MathProducersStage {
    consumes: Vec<String>,
}

impl MathProducersStage {
    /// Construct the stage. It consumes no upstream product: the producers compute from
    /// pinned in-code constants and compile-time-embedded artifact bytes.
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
        // v5: additionally fold the three EXECUTABLE lift producers' graphs (r_lift,
        // onnx_lift, proof_lift) — the shipped gmeow_math_lift front-ends run over real
        // committed artifacts embedded at compile time.
        // v6: RETIRE the r_bridge_lift graph (graph/math-producers/r-bridge). That producer
        // parsed nothing — it pushed a fixed Turtle string — and r_lift, which runs the real
        // R front-end over a committed script and emits a strictly richer math:RIngestRun,
        // fully subsumes it. Ten graphs now, and the rBridge flagship names r_lift.
        // v7: RELEASE the examples-corpus read. An authored `examples/*.ttl` ABox is source,
        // not a computed producer graph, and admitting only the math slice's made every other
        // slice's demonstrators unreachable from the bundle. `stage-source-load` now loads
        // EVERY slice's corpus into graph/examples; this stage is once more purely the ten
        // native producers, with no disk read contributing to emitted graph content.
        // v8: move the five flagship producer≡fixture parity laws out of a corpus-producing
        // test and into this explicit producer boundary. The fixtures are read only as
        // witnesses and never contribute output bytes.
        // v9: retain ONNX/proof lift drift coverage at the same explicit boundary after
        // deleting the last producer-running test. Seven fixture laws now share the one
        // already-required producer execution.
        "math_producers.v9-producers-with-fixture-parity"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The ten producers are self-contained native functions whose bytes ride the
        // workspace-source BUILD_FINGERPRINT (any code change to `crates/math` yields fresh
        // cache keys), including the three lifts' `include_str!` / `include_bytes!`
        // compile-time embeddings. This stage reads nothing off disk, so it declares no
        // input file for their computation. The seven committed producer fixtures are
        // explicit comparison witnesses, so they enter the stage action key directly.
        Ok(PRODUCER_PARITY_FIXTURES
            .iter()
            .map(|(_, _, relative_path)| root.join(relative_path))
            .collect())
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Parse each producer's deterministic Turtle into its own named carrier graph and
        // fold them into one frozen dataset the snapshot presenter folds into the bundle. A
        // producer/parse failure is a HARD FAIL — propagated, never swallowed
        // (no-optionality).
        let turtles = producer_turtles();
        enforce_producer_fixture_parity(input.root, &turtles)?;
        let mut graphs: Vec<Arc<RdfDataset>> = Vec::with_capacity(turtles.len());
        for (graph_iri, turtle) in &turtles {
            graphs.push(parse_into_graph(
                turtle.as_bytes(),
                "text/turtle",
                graph_iri,
            )?);
        }
        let refs: Vec<&RdfDataset> = graphs.iter().map(|g| g.as_ref()).collect();
        let dataset = Arc::new(RdfDataset::union(&refs));
        Ok(StageOutput::new(StageProduct::from_artifacts_over(
            self.id(),
            dataset,
            BTreeMap::new(),
        )))
    }
}
