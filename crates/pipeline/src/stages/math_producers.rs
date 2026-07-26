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
//! The graph content comes ONLY from the producers: no hand-typed constant, no clock, no
//! randomness (the producers are pure), so the eight producer graphs are byte-deterministic.
//! A producer/parse failure is a HARD FAIL — propagated, never swallowed (no-optionality).
//!
//! **The math-examples ABox (G13).** This stage ALSO reads every
//! `slices/grounding/math/examples/*.ttl` file — the slice's authored positive-demonstrator
//! corpus — and unions them into [`gmeow_logic::reasoning_graphs::GRAPH_MATH_EXAMPLES`], a
//! ninth named graph this stage attaches. That graph is admitted to the object-level
//! reasoning EDB (see `crate::stages::carrier::assemble_object_level_edb`), so the corpus's
//! authored `math:structuralKey` / `math:NormalizationDeclaration` instances actually reach
//! the shipped bundle's reasoned closure. Before this, no slice's `examples/*.ttl` ABox
//! reached the object-level bundle at all: `docs_render::docs_source_files` reads the same
//! files, but only to harvest competency-question IRI references for documentation — it
//! never asserts their triples as object-level axioms — so the expression-identity gate
//! (`gmeow_logic::math_expression::check_math_expression_findings`) ran vacuously against
//! every shipped bundle. This is a genuine disk read (unlike the eight pure producers), so
//! `input_files` declares the corpus and a missing/unreadable file is a HARD FAIL.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder};

use crate::node::{Stage, StageInput, StageOutput, StageProduct};
use crate::stages::carrier::{MATH_PRODUCER_GRAPHS, parse_into_graph, rooted_in_graph};
use crate::stages::source_load::turtle_bytes_to_dataset;

fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-math-producers".to_string(),
        message: message.into(),
    })
}

/// Every `slices/grounding/math/examples/*.ttl` file, sorted. The math slice's
/// positive-demonstrator ABox corpus — declared as `input_files` so a change to any
/// example invalidates this stage's cache key.
fn math_example_files(root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
    let dir = root
        .join("slices")
        .join("grounding")
        .join("math")
        .join("examples");
    let entries = std::fs::read_dir(&dir).map_err(|e| {
        stage_err(format!(
            "read math examples directory {}: {e}",
            dir.display()
        ))
    })?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| stage_err(format!("read {}: {e}", dir.display())))?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "ttl") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Parse and union every math example file into one dataset rooted at
/// [`gmeow_logic::reasoning_graphs::GRAPH_MATH_EXAMPLES`]. Each file's blank nodes are
/// standardized apart (`RdfDatasetBuilder::push_dataset`) so a structurally-distinct blank
/// axiom in two example files can never collide.
fn math_examples_graph(files: &[PathBuf]) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut builder = RdfDatasetBuilder::new();
    for path in files {
        let bytes =
            std::fs::read(path).map_err(|e| stage_err(format!("read {}: {e}", path.display())))?;
        let parsed = turtle_bytes_to_dataset(&bytes, &path.display().to_string())?;
        builder.push_dataset(parsed.as_ref());
    }
    let unioned = builder
        .freeze()
        .map_err(|e| stage_err(format!("freeze math-examples union: {e}")))?;
    rooted_in_graph(
        unioned.as_ref(),
        gmeow_logic::reasoning_graphs::GRAPH_MATH_EXAMPLES,
    )
}

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

/// The `math_producers` pipeline stage — a leaf compute node. It consumes no upstream
/// STAGE product (the producers are self-contained native functions and the
/// math-examples corpus is read directly off disk, not off another stage's product) and
/// attaches the eight producer graphs plus `graph/math-examples` to its carrier dataset.
pub struct MathProducersStage {
    consumes: Vec<String>,
}

impl MathProducersStage {
    /// Construct the stage. It consumes no upstream product: the producers compute from
    /// pinned in-code constants and the math-examples corpus is read directly from
    /// `slices/grounding/math/examples/` (declared via `input_files`, not a stage edge).
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
        // v5 (G13): additionally read every slices/grounding/math/examples/*.ttl file and
        // fold their union into graph/math-examples, a ninth attached graph admitted to
        // the object-level reasoning EDB — the math slice's positive-demonstrator ABox now
        // reaches the shipped bundle's reasoned closure instead of only the docs/CQ harvest.
        "math_producers.v5"
    }
    fn input_files(&self, root: &Path) -> Result<Vec<PathBuf>, gmeow_errors::Diag> {
        // The eight producers are self-contained native functions whose bytes ride the
        // workspace-source BUILD_FINGERPRINT (any code change to `crates/math` yields fresh
        // cache keys), so they declare nothing here. The math-examples corpus IS a genuine
        // disk read, so its files are the stage's complete input-file basis.
        math_example_files(root)
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, gmeow_errors::Diag> {
        // Parse each producer's deterministic Turtle into its own named carrier graph, union
        // in the math-examples corpus's own named graph, and fold everything into one frozen
        // dataset the snapshot presenter folds into the bundle (and, via
        // `assemble_object_level_edb`, into the reasoned closure). A producer/parse/read
        // failure is a HARD FAIL — propagated, never swallowed (no-optionality).
        let turtles = producer_turtles();
        let mut graphs: Vec<Arc<RdfDataset>> = Vec::with_capacity(turtles.len() + 1);
        for (graph_iri, turtle) in &turtles {
            graphs.push(parse_into_graph(
                turtle.as_bytes(),
                "text/turtle",
                graph_iri,
            )?);
        }
        let example_files = math_example_files(input.root)?;
        graphs.push(math_examples_graph(&example_files)?);
        let refs: Vec<&RdfDataset> = graphs.iter().map(|g| g.as_ref()).collect();
        let dataset = Arc::new(RdfDataset::union(&refs));
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

    /// Repo root (the workspace, two levels up from this crate's manifest) — the
    /// math-examples read needs the REAL `slices/grounding/math/examples/` tree.
    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    /// The stage attaches EXACTLY the eight producer graphs, each non-empty and carrying its
    /// producer's pinned content — the proof the producer output reaches the carrier (and thence
    /// `gmeow.gts`), not merely a test.
    #[test]
    fn run_attaches_the_eight_producer_graphs() {
        let stage = MathProducersStage::new();
        let upstream = BTreeMap::new();
        let root = repo_root();
        let out = stage
            .run(StageInput {
                root: &root,
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

    /// The math-examples corpus reaches its own named graph, non-empty, carrying a real
    /// authored witness: `ex:matrixProductAst`'s `math:structuralKey` from
    /// `reference-ast-act.ttl` — the G13 non-vacuous-gate fix's load-bearing proof that the
    /// slice's positive-demonstrator ABox actually reaches the carrier (and thence
    /// `gmeow.gts` and the object-level reasoning EDB), not merely the docs/CQ harvest.
    #[test]
    fn run_attaches_the_math_examples_graph_carrying_a_structural_key_witness() {
        let stage = MathProducersStage::new();
        let upstream = BTreeMap::new();
        let root = repo_root();
        let out = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("math_producers stage runs");
        let dataset = out.product.dataset();
        let projected =
            dataset.project_named_graph(gmeow_logic::reasoning_graphs::GRAPH_MATH_EXAMPLES);
        assert!(
            projected.quad_count() > 100,
            "graph/math-examples must carry the whole examples/*.ttl corpus, got {} quads",
            projected.quad_count()
        );
        let nquads = purrdf::canonical_flat_nquads(&projected).expect("canon math-examples");
        assert!(
            nquads.contains("matrixProductAst") && nquads.contains("structuralKey"),
            "graph/math-examples must carry reference-ast-act.ttl's math:structuralKey witness"
        );
    }

    /// Determinism: two runs attach byte-identical carrier datasets (the producers are pure —
    /// no clock, no RNG, no HashMap iteration order — and the examples corpus is a fixed
    /// on-disk tree).
    #[test]
    fn run_is_deterministic() {
        let stage = MathProducersStage::new();
        let upstream = BTreeMap::new();
        let root = repo_root();
        let a = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("run a");
        let b = stage
            .run(StageInput {
                root: &root,
                upstream: &upstream,
            })
            .expect("run b");
        assert_eq!(
            purrdf::canonical_flat_nquads(a.product.dataset()).expect("canon a"),
            purrdf::canonical_flat_nquads(b.product.dataset()).expect("canon b"),
            "the math-producers carrier dataset must be deterministic"
        );
    }

    /// `math_example_files` discovers the real corpus, sorted, `.ttl`-only.
    #[test]
    fn math_example_files_discovers_the_real_corpus_sorted() {
        let root = repo_root();
        let files = math_example_files(&root).expect("list math examples");
        assert!(
            files.len() > 20,
            "expected 20+ math example files, got {}",
            files.len()
        );
        assert!(
            files.windows(2).all(|w| w[0] < w[1]),
            "files must be sorted"
        );
        assert!(
            files
                .iter()
                .all(|p| p.extension().is_some_and(|e| e == "ttl"))
        );
        assert!(
            files
                .iter()
                .any(|p| p.file_name().unwrap() == "reference-ast-act.ttl")
        );
    }
}
