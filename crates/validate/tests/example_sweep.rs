// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full slice-example validation sweep (Task 6).
//!
//! This integration test proves the closed-world fidelity of the SHACL→JSON
//! Schema projection over the WHOLE example corpus: for every `slices/*/*/
//! examples/*.ttl` data graph, the projected JSON-LD `@graph` instance form
//! validates against the JSON Schema the emitter derives from the SAME merged
//! shapes the live validator uses ([`purrdf::shapes::shape_union::load_shapes`]).
//!
//! # Soundness contract
//!
//! The JSON Schema is a CLOSED-WORLD projection of the SHACL shapes: it claims
//! to accept exactly the data the SHACL validator accepts (for the modeled
//! subset). Therefore:
//!
//! * If an example does NOT conform to its SHACL shapes, it is illustrative
//!   (not valid instance data) and OUT OF SCOPE for the schema sweep. Such
//!   examples are listed in [`NON_CONFORMANT`] with a one-line reason. The test
//!   asserts the excluded set is EXACTLY the set that fails native SHACL — so an
//!   exclusion can never silently mask a real schema bug.
//!
//!   SHACL conformance is decided over the example UNIONED WITH THE ONTOLOGY
//!   TBox — the same merged-module graph `make validate` validates, one example
//!   at a time. An example referencing a value individual whose `rdf:type` lives
//!   in its slice's `module.ttl` is therefore CONFORMANT here: that graph carries
//!   the module. Isolation is not a reason to be on the allowlist.
//!   See [`union_with_tbox`].
//! * If an example DOES conform to SHACL but the JSON Schema REJECTS it, that is
//!   a soundness bug in the emitter/projector, surfaced as a test failure with a
//!   readable per-example violation report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gmeow_validate::instance::{InstanceFormat, validate_instance};
use purrdf::shapes::engine::PreparedValidator;
use purrdf::shapes::report::Severity;
use purrdf::shapes::shapes::Shapes;
use purrdf::shapes::term::{NamedNode, Term};
use purrdf::shapes::{instance, json_schema, shape_union};

use purrdf::parse_dataset;
use purrdf::{RdfDataset, RdfQuad, RdfTerm, flat_dataset_from_quads, flat_rdf_quads_from_dataset};

/// Examples that do NOT conform to the merged SHACL shapes and are therefore out
/// of scope for the JSON-schema sweep (not valid instance data). The sweep asserts
/// this set is EXACTLY the SHACL-failing set, so this allowlist cannot hide a
/// JSON-schema soundness bug.
///
/// Conformance is decided over the TBox union ([`union_with_tbox`]), so "the value
/// individual is typed in `module.ttl`, not in the example" is NOT a reason to be
/// here — that graph carries the module. Each trailing comment is the ACTUAL
/// violation the engine reports over the union: constraint component, focus node
/// (local name), and path, up to three distinct ones.
///
/// Two causes are represented, and they are not the same kind of thing:
///
/// * A REAL GAP IN THE EXAMPLE — the scene never binds a property the ontology
///   requires (a P11 reference frame, a `lang:Form`'s sign system, a frame
///   profile's components, a bound variable on a binder expression, a span's
///   offsets). The example is illustrative prose-with-triples, not instance data.
/// * A DEFECT IN THE DERIVED SHAPE, for the `gmeow:spanStart`/`gmeow:spanEnd`
///   node-kind entries: `generated/shapes/validation-shapes.ttl` gives
///   `gmeow:Chunk-shape` both offsets `sh:nodeKind sh:BlankNodeOrIRI`, while
///   `slices/core/ai/module.ttl` declares them `owl:DatatypeProperty` with
///   `rdfs:range xsd:nonNegativeInteger` — so the OWL→SHACL projection rejects a
///   CORRECT integer offset. The example is right and the projection is wrong.
const NON_CONFORMANT: &[&str] = &[
    "slices/core/ai/examples/grounded-claim.ttl", // still non-conformant unioned with the TBox — 2 violation(s): NodeKindConstraintComponent on chunk-042 (gmeow:spanEnd); NodeKindConstraintComponent on chunk-042 (gmeow:spanStart)
    "slices/core/epistemics/examples/flagship-epistemic-ledger.ttl", // still non-conformant unioned with the TBox — 2 violation(s): NodeKindConstraintComponent on chunk1 (gmeow:spanEnd); NodeKindConstraintComponent on chunk1 (gmeow:spanStart)
    "slices/grounding/lang/examples/forms-and-sign-systems.ttl", // still non-conformant unioned with the TBox — 11 violation(s): SPARQLConstraintComponent on formulaCatsChaseMice (-); MinCountConstraintComponent on swDe (lang:lexemeOf)
    "slices/grounding/lang/examples/gmn-dialect.ttl", // still non-conformant unioned with the TBox — 12 violation(s): MinCountConstraintComponent on claimGateOpenMonday (gmeow:observationMethod); QualifiedMinCountConstraintComponent on claimGateOpenMonday (gmeow:observationMethod); SPARQLConstraintComponent on claimGateOpenMonday (-)
    "slices/grounding/math/examples/measure-and-dimension.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on expectedEnergy (math:argumentSlot); MinCountConstraintComponent on expectedEnergy (math:boundVariable)
    "slices/grounding/math/examples/gmn-dimension-roundtrip.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on netForce (math:argumentSlot); MinCountConstraintComponent on netForce (math:boundVariable)
    "slices/grounding/math/examples/numbers-sets-functions.ttl", // still non-conformant unioned with the TBox — 1 violation(s): SPARQLConstraintComponent on evenCondition (-)
    "slices/grounding/math/examples/homomorphic-encryption.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on ciphertextRing (math:satisfiesDistributivity)
    "slices/grounding/math/examples/analysis-and-geometry.ttl", // still non-conformant unioned with the TBox — 20 violation(s): MinCountConstraintComponent on lorentzFactorLimit (math:argumentSlot); MinCountConstraintComponent on lorentzFactorLimit (math:boundVariable)
    "slices/grounding/math/examples/linear-algebra-and-learning.ttl", // still non-conformant unioned with the TBox — 36 violation(s): MinCountConstraintComponent on complexSpace4096 (math:satisfiesAxiom); MinCountConstraintComponent on complexSpace4096 (math:structureOperation); MinCountConstraintComponent on complexSpace4096 (math:underlyingSet)
    "slices/grounding/math/examples/bridges.ttl", // still non-conformant unioned with the TBox — 13 violation(s): SPARQLConstraintComponent on fitLogicFormula (-); MinCountConstraintComponent on onnxBridgePlan (logic:planGoal); MinCountConstraintComponent on onnxBridgePlan (logic:planSuccessMode)
    "slices/grounding/math/examples/combinatorial-laplacian.ttl", // still non-conformant unioned with the TBox — 3 violation(s): ClassConstraintComponent on laplacian1 (math:combinatorialLaplacianComplex); QualifiedMinCountConstraintComponent on laplacian1 (math:combinatorialLaplacianComplex)
    "slices/extensions/semantic-topology/examples/compilation-worked.ttl", // still non-conformant unioned with the TBox — 17 violation(s): MinCountConstraintComponent on filtration (math:hasFiltrationStage); MinCountConstraintComponent on finding (gmeow:findingCode); MinCountConstraintComponent on finding (gmeow:findingMessage)
    "slices/grounding/math/examples/probability.ttl", // still non-conformant unioned with the TBox — 16 violation(s): MinCountConstraintComponent on utcFrame (gmeow:determinacyModel); MinCountConstraintComponent on utcFrame (gmeow:dimensionCount); MinCountConstraintComponent on utcFrame (gmeow:frameKind)
    "slices/grounding/math/examples/statistics-hypotheses-pvalues.ttl", // still non-conformant unioned with the TBox — 8 violation(s): MinCountConstraintComponent on testFrame (gmeow:determinacyModel); MinCountConstraintComponent on testFrame (gmeow:dimensionCount); MinCountConstraintComponent on testFrame (gmeow:frameKind)
    "slices/grounding/math/examples/pvalue-tri-slice.ttl", // still non-conformant unioned with the TBox — 12 violation(s): MinCountConstraintComponent on samplingFrame (gmeow:determinacyModel); MinCountConstraintComponent on samplingFrame (gmeow:dimensionCount); MinCountConstraintComponent on samplingFrame (gmeow:frameKind)
    "slices/core/names/examples/person-names.ttl", // still non-conformant unioned with the TBox — 1 violation(s): MinCountConstraintComponent on chosenForm (lang:lexemeOf)
    "slices/extensions/embedding-projection/examples/purremb-bookshelf.ttl", // still non-conformant unioned with the TBox — 4 violation(s): ClassConstraintComponent on embRowB (gmeow:embeddingOf); ClassConstraintComponent on retrievalEventOld (gmeow:againstIndex)
    "slices/extensions/graphrag/examples/lillith-pipeline.ttl", // still non-conformant unioned with the TBox — 2 violation(s): NodeKindConstraintComponent on chunk-7 (gmeow:spanEnd); NodeKindConstraintComponent on chunk-7 (gmeow:spanStart)
    "slices/extensions/images/examples/photo-metadata.ttl", // still non-conformant unioned with the TBox — 1 violation(s): MinCountConstraintComponent on photoExpression (gmeow:hasReferenceFrame)
    "slices/core/preference/examples/comparison-worked.ttl", // still non-conformant unioned with the TBox — 3 violation(s): MinCountConstraintComponent on prefConnection (math:connectionOn); QualifiedMinCountConstraintComponent on sheaf (math:hasStalk); QualifiedMinCountConstraintComponent on sheaf (math:restrictionMap)
    "slices/core/preference/examples/condorcet-cycle.ttl", // still non-conformant unioned with the TBox — 8 violation(s): QualifiedMinCountConstraintComponent on cellA (math:cellDimension); MinCountConstraintComponent on cycleHolonomy (math:holonomyLoop); QualifiedMinCountConstraintComponent on cycleHolonomy (math:holonomyOf)
    "slices/core/preference/examples/hard-fails-soft-high.ttl", // still non-conformant unioned with the TBox — 9 violation(s): MinCountConstraintComponent on helpfulness (gmeow:penaltyPole); MinCountConstraintComponent on helpfulness (gmeow:rewardPole); QualifiedMinCountConstraintComponent on helpfulness (gmeow:penaltyPole)
    "slices/core/preference/examples/model-delta-worked.ttl", // still non-conformant unioned with the TBox — 13 violation(s): DatatypeConstraintComponent on cl12 (math:spaceDimension); MinCountConstraintComponent on cl12 (math:pseudoscalarSquare); MinCountConstraintComponent on cl12 (math:scalarField)
    "slices/core/preference/examples/multi-evaluator-no-winner.ttl", // still non-conformant unioned with the TBox — 5 violation(s): MinCountConstraintComponent on cycleHolonomy (math:holonomyLoop); QualifiedMinCountConstraintComponent on cycleHolonomy (math:holonomyOf); MinCountConstraintComponent on prefConnection (math:connectionOn)
    "slices/core/preference/examples/pareto-incomparable.ttl", // still non-conformant unioned with the TBox — 16 violation(s): MinCountConstraintComponent on c1Latency (gmeow:penaltyPole); MinCountConstraintComponent on c1Latency (gmeow:rewardPole); QualifiedMinCountConstraintComponent on c1Latency (gmeow:penaltyPole)
    "slices/core/preference/examples/promotion-worked.ttl", // still non-conformant unioned with the TBox — 4 violation(s): MinCountConstraintComponent on evalSpan (gmeow:spanEnd); MinCountConstraintComponent on evalSpan (gmeow:spanOfChunk); MinCountConstraintComponent on evalSpan (gmeow:spanStart)
    "slices/profile/agent-runtime/examples/tool-usage.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on sortSchema (logic:capability); MinCountConstraintComponent on sortSchema (logic:precondition)
    "slices/core/creative-works/examples/wemi-novel.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on englishText (gmeow:hasReferenceFrame)
    "slices/core/documents/examples/web-presence.ttl", // still non-conformant unioned with the TBox — 1 violation(s): MinCountConstraintComponent on siteContent (gmeow:hasReferenceFrame)
    "slices/core/affect/examples/two-critics.ttl", // still non-conformant unioned with the TBox — 1 violation(s): MinCountConstraintComponent on novelExpression (gmeow:hasReferenceFrame)
    "slices/core/coreference/examples/authority-links.ttl", // still non-conformant unioned with the TBox — 2 violation(s): MinCountConstraintComponent on eveningStarForm (lang:lexemeOf)
    "slices/core/notation/examples/notation-systems.ttl", // still non-conformant unioned with the TBox — 23 violation(s): MinCountConstraintComponent on melodyContent (lang:inSignSystem); QualifiedMinCountConstraintComponent on melodyContent (lang:inSignSystem); MinCountConstraintComponent on staffProfile (skos:definition)
    "slices/core/notation/examples/pydantic-projection-profile.ttl", // still non-conformant unioned with the TBox — 23 violation(s): MinCountConstraintComponent on gmeowShapeSet (lang:inSignSystem); QualifiedMinCountConstraintComponent on gmeowShapeSet (lang:inSignSystem); MinCountConstraintComponent on pydanticProfile (skos:definition)
    "slices/grounding/math/examples/expression-rendering.ttl", // still non-conformant unioned with the TBox — 5 violation(s): MinCountConstraintComponent on latexNotation (lang:grammarFor); MinCountConstraintComponent on sumLatexForm (lang:inSignSystem); QualifiedMinCountConstraintComponent on sumLatexForm (lang:inSignSystem)
];

mod conformance_support;

/// The repo root (two levels up from this crate's manifest dir).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Load one Turtle data-graph file into a frozen [`RdfDataset`] via the native
/// codec — the SAME lenient native path the shape union uses
/// ([`shape_union::load_shapes`]).
fn load_data_graph(path: &Path) -> Arc<RdfDataset> {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_dataset(&bytes, "text/turtle", None)
        .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Glob every `slices/*/*/examples/*.ttl`, sorted, as repo-relative paths.
fn example_files(repo: &Path) -> Vec<PathBuf> {
    let slices = repo.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    for group in read_dirs(&slices) {
        for slice in read_dirs(&group) {
            let examples = slice.join("examples");
            if !examples.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&examples)
                .unwrap_or_else(|e| panic!("read {}: {e}", examples.display()))
            {
                let path = entry.expect("dir entry").path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") && path.is_file() {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// Immediate subdirectories of `dir`, sorted (empty when `dir` is absent).
fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        // Fail fast on an unreadable entry rather than silently dropping a
        // slice/example directory (which would let the sweep pass without covering
        // the full corpus) — matching the file-level `example_files()` behavior.
        .map(|e| {
            e.unwrap_or_else(|err| panic!("read {} entry: {err}", dir.display()))
                .path()
        })
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Render a path relative to the repo root (forward slashes) for reports/allowlist.
fn rel(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Blank-label scope prefix applied to the example's own blank nodes when they are
/// merged into the ontology TBox, so an example label (`b0`, `genid1`, …) can never
/// collide with a TBox blank label and silently fuse two distinct nodes (which would
/// corrupt `rdf:List` / `owl:Restriction` walks). The prefix also lets the merge
/// hand the engine the EXACT blank focus nodes the example introduced.
const EXAMPLE_BLANK_SCOPE: &str = "gmeow-example-sweep-";

/// The example data graph UNIONED WITH THE ONTOLOGY TBox, plus the focus nodes the
/// example introduces.
///
/// `make validate` (`gmeow-dev validate` → `ValidationRun::run`) validates the
/// authored sources — `gmeow_pipeline::stages::source_load::authored_files`: the
/// root ontology, every `slices/**/module.ttl`, and `imports/` — as ONE merged
/// graph. `examples/` is NOT in that corpus, so the gate never reads an example at
/// all, and an example validated in closed-world isolation is validated against a
/// graph that exists nowhere in the system: every `rdf:type`, `rdfs:subClassOf`,
/// and shared value individual its shapes resolve through is missing by
/// construction. Neither reading is the example's own semantics.
///
/// So the sweep validates each example against the SAME merged-module TBox
/// `make validate` builds, ONE example at a time — the TBox side is
/// [`conformance_support::base_ontology_dataset`] (every `slices/**/module.ttl`,
/// blank-standardized per source, flattened to the default graph), the crate's
/// single merged-module union, reused rather than rebuilt. One example at a time
/// matters: merging the whole corpus would let one example silently supply a
/// triple another one is missing, and would expose every whole-graph `sh:sparql`
/// constraint to 199 unrelated scenes at once.
///
/// The returned focus set is every IRI the example mentions in SUBJECT or OBJECT
/// position (object position covers inverse-path shapes) plus every blank node the
/// example contributes. A focus node outside that set is a pure-TBox node whose
/// entire neighborhood is its TBox-only neighborhood — the example, being
/// `ex:`-scoped, adds no quad with it in subject or object position — so it
/// validates identically with or without the example and cannot change any verdict.
fn union_with_tbox(example: &RdfDataset, tbox: &[RdfQuad]) -> (Arc<RdfDataset>, Vec<Term>) {
    let mut iris: BTreeSet<String> = BTreeSet::new();
    let mut blanks: BTreeSet<String> = BTreeSet::new();
    // SHACL-project the example (flatten named graphs, expand RDF 1.2 reifiers and
    // annotations into quads) BEFORE the merge, so the merged quad list is already
    // the projection of the union: `tbox` is the pre-projected TBox, and projection
    // is quad-wise, so `project(TBox ∪ example) = project(TBox) ∪ project(example)`.
    let projected =
        purrdf::shapes::engine::project_dataset(example).expect("SHACL projection of an example");
    let mut example_quads = flat_rdf_quads_from_dataset(&projected);
    for quad in &mut example_quads {
        for slot in [&mut quad.subject, &mut quad.object] {
            match slot {
                RdfTerm::Iri(iri) => {
                    iris.insert(iri.clone());
                }
                RdfTerm::BlankNode(label) => {
                    let scoped = format!("{EXAMPLE_BLANK_SCOPE}{label}");
                    *label = scoped.clone();
                    blanks.insert(scoped);
                }
                _ => {}
            }
        }
    }

    let mut quads: Vec<RdfQuad> = Vec::with_capacity(tbox.len() + example_quads.len());
    quads.extend_from_slice(tbox);
    quads.extend(example_quads);
    let merged = flat_dataset_from_quads(&quads).expect("merged TBox+example dataset must freeze");

    let focus: Vec<Term> = iris
        .into_iter()
        .map(|iri| Term::NamedNode(NamedNode::new_unchecked(iri)))
        .chain(blanks.into_iter().map(Term::BlankNode))
        .collect();
    (merged, focus)
}

/// Whether `example` conforms to the merged `shapes` per the native SHACL engine,
/// validated UNIONED WITH THE ONTOLOGY TBox (see [`union_with_tbox`]) — the graph
/// `make validate` actually validates, not the example in closed-world isolation.
///
/// SHACL conformance is gated by `sh:Violation` results ONLY — `Info`/`Warning`
/// results (e.g. the advisory-tier `logic:severity "Info"` constraints) are
/// non-gating per spec, so the engine's own `conforms` flag (which flips false
/// on ANY result) is not the right signal here. Recompute it the same way
/// `gmeow_validate::advisory::split_advisory_results` does for its retained
/// set: conforms iff no result carries `Severity::Violation`.
///
/// Returns the violation messages (empty ⇒ conformant) so a drifted allowlist entry
/// can name the shape that actually fires.
fn shacl_violations(example: &RdfDataset, tbox: &[RdfQuad], shapes: &Arc<Shapes>) -> Vec<String> {
    let (merged, focus) = union_with_tbox(example, tbox);
    // Prepare once per example (class closure + SHACL-SPARQL targets), then evaluate
    // ONLY the example's own focus nodes: the prepared validator answers target
    // membership by index probe instead of materializing every shape's target set
    // over the ~120k-quad union.
    let validator = PreparedValidator::from_projected_dataset(merged, Arc::clone(shapes))
        .expect("prepare the merged TBox+example dataset for validation");
    let report = validator
        .validate_focus_nodes(&focus)
        .expect("validation over a frozen dataset is infallible");
    report
        .results
        .iter()
        .filter(|r| matches!(r.severity, Severity::Violation))
        .map(|r| {
            format!(
                "{} on {} (path {}, value {}): {}",
                r.source_constraint_component,
                r.focus_node,
                r.result_path
                    .as_ref()
                    .map_or_else(|| "-".to_owned(), ToString::to_string),
                r.value
                    .as_ref()
                    .map_or_else(|| "-".to_owned(), ToString::to_string),
                r.message.clone().unwrap_or_default()
            )
        })
        .collect()
}

fn gmeow_namespaces() -> json_schema::Namespaces {
    json_schema::Namespaces::new(
        "gmeow",
        &[
            (
                "gmeow".to_owned(),
                "https://blackcatinformatics.ca/gmeow/".to_owned(),
            ),
            (
                "logic".to_owned(),
                "https://blackcatinformatics.ca/logic/".to_owned(),
            ),
            (
                "lang".to_owned(),
                "https://blackcatinformatics.ca/lang/".to_owned(),
            ),
            (
                "math".to_owned(),
                "https://blackcatinformatics.ca/math/".to_owned(),
            ),
        ],
    )
    .expect("gmeow namespaces")
}

/// One example's sweep verdict.
struct Outcome {
    /// Repo-relative path (the allowlist key).
    relpath: String,
    /// `sh:Violation`-severity results over the TBox-unioned graph (empty ⇒ conformant).
    shacl_violations: Vec<String>,
    /// Whether the example is on [`NON_CONFORMANT`] (so the schema phase is skipped).
    excluded: bool,
    /// Closed-world JSON Schema violations (empty ⇒ the schema accepted the projection).
    schema_violations: Vec<String>,
}

/// Sweep ONE example: SHACL over the TBox union, then — when in scope — the
/// closed-world JSON Schema over the example's own instance projection.
fn sweep_one(
    repo: &Path,
    path: &Path,
    tbox: &[RdfQuad],
    shapes: &Arc<Shapes>,
    schema_bytes: &[u8],
    non_conformant: &BTreeSet<&str>,
) -> Outcome {
    let relpath = rel(repo, path);
    let store = load_data_graph(path);

    // (A) Does the example conform to its SHACL shapes when validated the way
    //     `make validate` validates it — unioned with the ontology TBox? If not,
    //     it is illustrative, not valid instance data → out of scope.
    let shacl_violations = shacl_violations(store.as_ref(), tbox, shapes);
    let excluded = non_conformant.contains(relpath.as_str());

    // (B) Project the EXAMPLE ALONE to JSON-LD and validate against the
    //     closed-world schema: the schema's claim is about the instance document
    //     an emitter ships, which carries the example's data and nothing else.
    let schema_violations = if excluded {
        Vec::new()
    } else {
        let instance_value = instance::project_graph(&store, &gmeow_namespaces());
        let instance_bytes = serde_json::to_vec(&instance_value).expect("serialize instance");
        validate_instance(&instance_bytes, InstanceFormat::Json, schema_bytes)
            .unwrap_or_else(|e| panic!("validate_instance hard error for {relpath}: {e}"))
    };

    Outcome {
        relpath,
        shacl_violations,
        excluded,
        schema_violations,
    }
}

/// Workers for the corpus sweep. Each per-example validation is single-threaded
/// (one merged TBox+example graph, prepared and evaluated by one worker), so the
/// corpus — not the individual example — is what scales across cores.
fn sweep_workers() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

#[test]
fn example_corpus_validates_against_closed_world_schema() {
    let repo = repo_root();

    // The merged shape union + the JSON Schema derived from those same shapes.
    let (_shapes_store, shapes) =
        shape_union::load_shapes(&repo).expect("load merged SHACL shapes");
    let compiled = json_schema::compile(&shapes, &gmeow_namespaces());
    let schema_bytes = compiled.schema_json.as_bytes();
    let shapes = Arc::new(shapes);

    // The ontology TBox every example is unioned with (the merged `module.ttl`
    // corpus), SHACL-projected once for the whole sweep.
    let tbox = flat_rdf_quads_from_dataset(
        &purrdf::shapes::engine::project_dataset(conformance_support::base_ontology_dataset())
            .expect("SHACL projection of the merged module TBox"),
    );

    let non_conformant: BTreeSet<&str> = NON_CONFORMANT.iter().copied().collect();

    let examples = example_files(&repo);
    assert!(
        !examples.is_empty(),
        "no example fixtures found under slices/*/*/examples/*.ttl"
    );

    // Per-example outcomes, computed by a fixed worker pool over a shared cursor
    // (each example is claimed by exactly one worker) and re-sorted by path, so the
    // reported order is independent of scheduling.
    let cursor = AtomicUsize::new(0);
    let mut outcomes: Vec<Outcome> = std::thread::scope(|scope| {
        let (cursor, examples, tbox, shapes, non_conformant, repo) = (
            &cursor,
            &examples,
            &tbox,
            &shapes,
            &non_conformant,
            repo.as_path(),
        );
        let handles: Vec<_> = (0..sweep_workers())
            .map(|_| {
                scope.spawn(move || {
                    let mut mine: Vec<Outcome> = Vec::new();
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(path) = examples.get(index) else {
                            return mine;
                        };
                        mine.push(sweep_one(
                            repo,
                            path,
                            tbox,
                            shapes,
                            schema_bytes,
                            non_conformant,
                        ));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("example sweep worker joins"))
            .collect()
    });
    outcomes.sort_by(|a, b| a.relpath.cmp(&b.relpath));

    let mut schema_failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut shacl_failing: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut excluded_count = 0usize;
    let mut passed_count = 0usize;

    for outcome in outcomes {
        if !outcome.shacl_violations.is_empty() {
            shacl_failing.insert(outcome.relpath.clone(), outcome.shacl_violations);
        }
        if outcome.excluded {
            excluded_count += 1;
        } else if outcome.schema_violations.is_empty() {
            passed_count += 1;
        } else {
            schema_failures.push((outcome.relpath, outcome.schema_violations));
        }
    }

    // Every swept example is EXACTLY one of passed / excluded / schema-failure —
    // a partition invariant (replaces a bare sweep-summary log line).
    assert_eq!(
        passed_count + excluded_count + schema_failures.len(),
        examples.len(),
        "sweep partition: {passed_count} passed + {excluded_count} excluded + {} schema-failures must total {} examples",
        schema_failures.len(),
        examples.len(),
    );

    // Both invariants are reported TOGETHER: a drifted allowlist and a rejected
    // projection are independent findings, and failing on the first would hide the
    // second behind a second run.
    let mut failures = String::new();

    // Invariant 1: the allowlist must be EXACTLY the SHACL-failing set, so an
    // exclusion can never silently mask a JSON-schema soundness bug.
    let allowlisted: BTreeSet<String> = non_conformant.iter().map(|s| (*s).to_owned()).collect();
    let failing: BTreeSet<String> = shacl_failing.keys().cloned().collect();
    if allowlisted != failing {
        let only_allowlist: Vec<&String> = allowlisted.difference(&failing).collect();
        let mut only_shacl = String::new();
        for path in failing.difference(&allowlisted) {
            only_shacl.push_str(&format!("\n{path}:\n"));
            for violation in &shacl_failing[path] {
                only_shacl.push_str(&format!("  - {violation}\n"));
            }
        }
        failures.push_str(&format!(
            "NON_CONFORMANT allowlist drifted from the SHACL-failing set.\n\
             listed but actually SHACL-CONFORMANT (remove from allowlist): {only_allowlist:#?}\n\
             SHACL-NON-CONFORMANT but not listed (fix the example, or list it with a \
             reason): {only_shacl}\n"
        ));
    }

    // Invariant 2: every in-scope (SHACL-conformant, non-excluded) example must
    // validate against the closed-world JSON Schema.
    if !schema_failures.is_empty() {
        failures.push_str(
            "closed-world JSON Schema REJECTED SHACL-conformant example data \
             (soundness bug in emitter/projector):\n",
        );
        for (path, violations) in &schema_failures {
            failures.push_str(&format!("\n{path}:\n"));
            for v in violations.iter().take(5) {
                failures.push_str(&format!("  - {v}\n"));
            }
            if violations.len() > 5 {
                failures.push_str(&format!("  … and {} more\n", violations.len() - 5));
            }
        }
    }

    assert!(failures.is_empty(), "{failures}");
}
